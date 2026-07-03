//! ECB (European Central Bank) exchange rate provider for EUR-based currency pairs.
//!
//! Uses the ECB Statistical Data Warehouse (SDW) XML API to fetch official daily reference rates.
//! Rates are published around 16:00 CET daily and are accepted by German tax authorities.

use std::collections::HashMap;
use std::sync::OnceLock;

#[cfg(test)]
use indoc::indoc;
use log::warn;
use reqwest::Url;
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::core::GenericResult;
use crate::currency::Cash;
use crate::forex;
use crate::formats::xml;
use crate::formatting;
use crate::http;
use crate::quotes::{CurrencyRate, QuotesMap, QuotesProvider};
use crate::time::{self, Date};
use crate::types::Decimal;
use crate::util::{self, DecimalRestrictions};

// Referenced only by the non-test `RateSource::Ecb` constructor, so it reads as dead under `cfg(test)`.
#[cfg_attr(test, allow(dead_code))]
pub const BASE_URL: &str = "https://data-api.ecb.europa.eu";
pub const BASE_CURRENCY: &str = "EUR";

/// European Central Bank exchange rate provider.
///
/// Provides EUR-based exchange rates using the ECB Statistical Data Warehouse API.
/// The ECB publishes daily reference rates which are widely used in the EU
/// and accepted by German tax authorities.
pub struct Ecb {
    url: String,
    client: Client,
    rates: OnceLock<HashMap<String, Decimal>>,
}

impl Ecb {
    pub fn new(url: &str) -> Ecb {
        Ecb {
            url: url.to_owned(),
            client: Client::new(),
            rates: OnceLock::new(),
        }
    }

    /// Get current daily exchange rates from ECB.
    ///
    /// Returns a map of currency codes to their EUR exchange rates.
    /// For example, USD → 1.08 means 1 EUR = 1.08 USD.
    fn get_currency_rates(&self) -> GenericResult<HashMap<String, Decimal>> {
        // ECB SDMX-ML format for daily exchange rates.
        // D = Daily, . = all currencies, EUR = base, SP00 = spot, A = average.
        // lastNObservations=1 keeps the response to the latest value per currency instead of the
        // full history back to 1999.
        let result: GenericMessage =
            self.query("currency rates", "service/data/EXR/D..EUR.SP00.A?lastNObservations=1")?;

        let today = time::today();
        let mut rates = HashMap::new();
        let mut latest_date: Option<Date> = None;

        // Parse the SDMX-ML generic data format
        if let Some(data_set) = result.data_set {
            for series in data_set.series {
                // Extract currency code from series key
                let currency = series
                    .series_key
                    .values
                    .iter()
                    .find(|v| v.id == "CURRENCY")
                    .map(|v| v.value.clone());

                let Some(currency) = currency else {
                    continue;
                };

                // Get the most recent observation (last in the list)
                if let Some(obs) = series.obs.last() {
                    let parsed_date = time::parse_date(&obs.obs_dimension.value, "%Y-%m-%d")
                        .map_err(|e| format!("Invalid date in ECB response: {e}"))?;

                    let parsed_rate = util::parse_decimal(
                        &obs.obs_value.value,
                        DecimalRestrictions::StrictlyPositive,
                    )
                    .map_err(|_| {
                        format!("Invalid rate in ECB response: {}", obs.obs_value.value)
                    })?;

                    rates.insert(currency, parsed_rate);

                    // Track the latest date we've seen
                    if latest_date.map_or(true, |d| parsed_date > d) {
                        latest_date = Some(parsed_date);
                    }
                }
            }
        }

        // Warn if rates seem outdated (more than 5 calendar days old, i.e. beyond a normal weekend)
        if let Some(date) = latest_date {
            let days_old = (today - date).num_days();
            if days_old > 5 {
                warn!(
                    "Got outdated ({}) currency rates from ECB.",
                    formatting::format_date(date)
                );
            }
        }

        Ok(rates)
    }

    /// Get historical exchange rates for a specific currency over a date range.
    ///
    /// The ECB `EXR/D.<CCY>.EUR.SP00.A` series publishes the rate as units of `<CCY>` per 1 EUR
    /// (e.g. USD ≈ 1.08). The currency rate cache, however, stores the base-currency price of one
    /// unit of the foreign currency (EUR per `<CCY>`, matching the CBR convention consumed by
    /// `CurrencyConverter`). We therefore invert the published rate before returning it.
    pub fn get_historical_currency_rates(
        &self,
        currency: &str,
        start_date: Date,
        end_date: Date,
    ) -> GenericResult<Vec<CurrencyRate>> {
        // ECB SDMX-ML format with specific currency and date range
        let path = format!(
            "service/data/EXR/D.{currency}.EUR.SP00.A?startPeriod={}&endPeriod={}",
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        );

        let result: GenericMessage = self.query("historical currency rates", &path)?;
        let mut rates = Vec::new();

        if let Some(data_set) = result.data_set {
            for series in data_set.series {
                for obs in series.obs {
                    let parsed_date = time::parse_date(&obs.obs_dimension.value, "%Y-%m-%d")
                        .map_err(|e| format!("Invalid date in ECB response: {e}"))?;

                    let parsed_rate = util::parse_decimal(
                        &obs.obs_value.value,
                        DecimalRestrictions::StrictlyPositive,
                    )
                    .map_err(|_| {
                        format!("Invalid rate in ECB response: {}", obs.obs_value.value)
                    })?;

                    rates.push(CurrencyRate {
                        date: parsed_date,
                        price: dec!(1) / parsed_rate,
                    });
                }
            }
        }

        // Sort by date ascending
        rates.sort_by_key(|r| r.date);

        Ok(rates)
    }

    fn query<T: serde::de::DeserializeOwned>(&self, name: &str, path: &str) -> GenericResult<T> {
        let url = format!("{}/{}", self.url, path);
        let url = Url::parse(&url)?;

        let get = |url: &str| -> GenericResult<T> {
            let response = http::send_request(&self.client, url, None)?;
            let result: T = xml::deserialize(response)?;
            Ok(result)
        };

        Ok(get(url.as_str()).map_err(|e| format!("Failed to get {name} from {url}: {e}"))?)
    }
}

impl QuotesProvider for Ecb {
    fn name(&self) -> String {
        s!("ECB")
    }

    fn supports_forex(&self) -> bool {
        true
    }

    fn get_quotes(&self, symbols: &[&str]) -> GenericResult<QuotesMap> {
        // Cache only successful fetches: a transient HTTP failure must not be memoized for the
        // lifetime of the process.
        let rates = match self.rates.get() {
            Some(rates) => rates,
            None => {
                let fetched = self.get_currency_rates()?;
                let _ = self.rates.set(fetched);
                self.rates.get().unwrap()
            }
        };

        let mut quotes = QuotesMap::new();

        for &symbol in symbols {
            let (base, quote) = forex::parse_currency_pair(symbol)?;
            if let Some(quote_value) = get_quote(base, quote, rates) {
                quotes.insert(symbol.to_owned(), quote_value);
            }
        }

        Ok(quotes)
    }
}

/// Calculate a quote from ECB rates.
///
/// ECB rates are expressed as 1 EUR = X units of other currency.
/// For example: USD rate = 1.08 means 1 EUR = 1.08 USD.
///
/// To get quote for BASE/QUOTE pair:
/// - EUR/USD: return USD rate directly (1.08)
/// - USD/EUR: return 1/USD_rate (1/1.08 ≈ 0.926)
/// - USD/GBP: return GBP_rate/USD_rate
fn get_quote(base: &str, quote: &str, rates: &HashMap<String, Decimal>) -> Option<Cash> {
    let base_rate = match base {
        BASE_CURRENCY => dec!(1),
        _ => *rates.get(base)?,
    };

    let quote_rate = match quote {
        BASE_CURRENCY => dec!(1),
        _ => *rates.get(quote)?,
    };

    // ECB rates are "1 EUR = X currency", so:
    // - For EUR/X: we want X (quote_rate)
    // - For X/EUR: we want 1/X (1/base_rate)
    // - For X/Y: we want Y/X (quote_rate/base_rate)
    Some(Cash::new(quote, quote_rate / base_rate))
}

// SDMX-ML Generic Data Format structures
// Reference: https://sdmx.org/wp-content/uploads/SDMX_2-1-1_SECTION_3A_SDMX_ML.pdf
//
// The ECB API returns XML with namespaces (generic:, message:), but serde-xml-rs
// handles them by matching the local name without prefix.
// Attributes are prefixed with @ in serde-xml-rs.

#[derive(Debug, Deserialize)]
#[serde(rename = "GenericData")]
struct GenericMessage {
    #[serde(rename = "DataSet")]
    data_set: Option<DataSet>,
}

#[derive(Debug, Deserialize)]
struct DataSet {
    #[serde(rename = "Series", default)]
    series: Vec<Series>,
}

#[derive(Debug, Deserialize)]
struct Series {
    #[serde(rename = "SeriesKey")]
    series_key: SeriesKey,
    #[serde(rename = "Obs", default)]
    obs: Vec<Observation>,
}

#[derive(Debug, Deserialize)]
struct SeriesKey {
    #[serde(rename = "Value", default)]
    values: Vec<KeyValue>,
}

#[derive(Debug, Deserialize)]
struct KeyValue {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@value")]
    value: String,
}

#[derive(Debug, Deserialize)]
struct Observation {
    #[serde(rename = "ObsDimension")]
    obs_dimension: ObsDimension,
    #[serde(rename = "ObsValue")]
    obs_value: ObsValue,
}

#[derive(Debug, Deserialize)]
struct ObsDimension {
    #[serde(rename = "@value")]
    value: String,
}

#[derive(Debug, Deserialize)]
struct ObsValue {
    #[serde(rename = "@value")]
    value: String,
}

#[cfg(test)]
mod tests {
    use mockito::{Mock, Server, ServerGuard};

    use super::*;

    #[test]
    fn current_rates() {
        let (mut server, client) = create_server();

        let _rates_mock = mock_response(
            &mut server,
            "/service/data/EXR/D..EUR.SP00.A?lastNObservations=1",
            indoc!(
                r#"
                <?xml version="1.0" encoding="UTF-8"?>
                <GenericData>
                    <DataSet>
                        <Series>
                            <SeriesKey>
                                <Value id="CURRENCY" value="USD"/>
                            </SeriesKey>
                            <Obs>
                                <ObsDimension value="2024-06-27"/>
                                <ObsValue value="1.0700"/>
                            </Obs>
                        </Series>
                        <Series>
                            <SeriesKey>
                                <Value id="CURRENCY" value="GBP"/>
                            </SeriesKey>
                            <Obs>
                                <ObsDimension value="2024-06-27"/>
                                <ObsValue value="0.84550"/>
                            </Obs>
                        </Series>
                        <Series>
                            <SeriesKey>
                                <Value id="CURRENCY" value="CHF"/>
                            </SeriesKey>
                            <Obs>
                                <ObsDimension value="2024-06-27"/>
                                <ObsValue value="0.9577"/>
                            </Obs>
                        </Series>
                    </DataSet>
                </GenericData>
            "#
            ),
        );

        let mut quotes = client
            .get_quotes(&[
                "EUR/EUR", "EUR/USD", "USD/EUR", "EUR/GBP", "GBP/EUR", "USD/GBP", "GBP/USD",
                "XXX/YYY",
            ])
            .unwrap();
        // Use 4 decimal places to avoid rounding differences in cross-rate calculations
        quotes = quotes
            .into_iter()
            .map(|(symbol, quote)| (symbol, quote.round_to(4)))
            .collect();

        assert_eq!(
            quotes,
            hashmap! {
                s!("EUR/EUR") => Cash::new("EUR", dec!(1)),

                s!("EUR/USD") => Cash::new("USD", dec!(1.07)),
                s!("USD/EUR") => Cash::new("EUR", dec!(0.9346)),

                s!("EUR/GBP") => Cash::new("GBP", dec!(0.8455)),
                s!("GBP/EUR") => Cash::new("EUR", dec!(1.1827)),

                // Cross rates: USD/GBP = GBP_rate / USD_rate = 0.8455 / 1.07
                s!("USD/GBP") => Cash::new("GBP", dec!(0.7902)),
                s!("GBP/USD") => Cash::new("USD", dec!(1.2655)),
            }
        );
    }

    #[test]
    fn historical_rates() {
        let (mut server, client) = create_server();

        let _rates_mock = mock_response(
            &mut server,
            "/service/data/EXR/D.USD.EUR.SP00.A?startPeriod=2024-01-01&endPeriod=2024-01-05",
            indoc!(
                r#"
                <?xml version="1.0" encoding="UTF-8"?>
                <GenericData>
                    <DataSet>
                        <Series>
                            <SeriesKey>
                                <Value id="CURRENCY" value="USD"/>
                            </SeriesKey>
                            <Obs>
                                <ObsDimension value="2024-01-02"/>
                                <ObsValue value="1.0956"/>
                            </Obs>
                            <Obs>
                                <ObsDimension value="2024-01-03"/>
                                <ObsValue value="1.0912"/>
                            </Obs>
                            <Obs>
                                <ObsDimension value="2024-01-04"/>
                                <ObsValue value="1.0932"/>
                            </Obs>
                            <Obs>
                                <ObsDimension value="2024-01-05"/>
                                <ObsValue value="1.0942"/>
                            </Obs>
                        </Series>
                    </DataSet>
                </GenericData>
            "#
            ),
        );

        let start = time::parse_date("2024-01-01", "%Y-%m-%d").unwrap();
        let end = time::parse_date("2024-01-05", "%Y-%m-%d").unwrap();
        let rates = client
            .get_historical_currency_rates("USD", start, end)
            .unwrap();

        // The published series is USD-per-EUR; the cache stores EUR-per-USD, so the returned
        // prices are the inverse of the raw observations (1.0956 -> 1/1.0956).
        assert_eq!(rates.len(), 4);
        assert_eq!(
            rates[0].date,
            time::parse_date("2024-01-02", "%Y-%m-%d").unwrap()
        );
        assert_eq!(rates[0].price, dec!(1) / dec!(1.0956));
        assert_eq!(
            rates[3].date,
            time::parse_date("2024-01-05", "%Y-%m-%d").unwrap()
        );
        assert_eq!(rates[3].price, dec!(1) / dec!(1.0942));
    }

    fn create_server() -> (ServerGuard, Ecb) {
        let server = Server::new();
        let client = Ecb::new(&server.url());
        (server, client)
    }

    fn mock_response(server: &mut ServerGuard, path: &str, data: &str) -> Mock {
        server
            .mock("GET", path)
            .with_status(200)
            .with_header("content-type", "application/xml; charset=utf-8")
            .with_body(data)
            .create()
    }
}

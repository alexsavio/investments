//! ECB (European Central Bank) exchange rate provider for EUR-based currency pairs.
//!
//! Fetches official daily reference rates from the ECB Data Portal API in SDMX CSV format
//! (`format=csvdata`). Rates are published around 16:00 CET daily and are accepted by German tax
//! authorities.

use std::collections::HashMap;
use std::sync::OnceLock;

#[cfg(test)]
use indoc::indoc;
use log::warn;
use reqwest::Url;
use reqwest::blocking::{Client, Response};

use crate::core::GenericResult;
use crate::currency::Cash;
use crate::forex;
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
        // ECB SDMX CSV format for daily exchange rates.
        // D = Daily, . = all currencies, EUR = base, SP00 = spot, A = average.
        // lastNObservations=1 keeps the response to the latest value per currency instead of the
        // full history back to 1999.
        let observations = self.query(
            "currency rates", "service/data/EXR/D..EUR.SP00.A?lastNObservations=1&format=csvdata")?;

        let today = time::today();
        let mut rates = HashMap::new();
        let mut latest_date: Option<Date> = None;

        // The published series is `<CCY>` units per 1 EUR; `get_quote` consumes it in that
        // orientation, so store the raw observation value here (no inversion).
        for observation in observations {
            if latest_date.is_none_or(|date| observation.date > date) {
                latest_date = Some(observation.date);
            }
            rates.insert(observation.currency, observation.value);
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
        // ECB SDMX CSV format with specific currency and date range
        let path = format!(
            "service/data/EXR/D.{currency}.EUR.SP00.A?startPeriod={}&endPeriod={}&format=csvdata",
            start_date.format("%Y-%m-%d"),
            end_date.format("%Y-%m-%d")
        );

        let observations = self.query("historical currency rates", &path)?;

        // The published series is `<CCY>` units per 1 EUR; the cache stores the base-currency price
        // of one foreign unit (EUR per `<CCY>`), so invert before returning.
        let mut rates: Vec<CurrencyRate> = observations
            .into_iter()
            .map(|observation| CurrencyRate {
                date: observation.date,
                price: dec!(1) / observation.value,
            })
            .collect();

        rates.sort_by_key(|rate| rate.date);

        Ok(rates)
    }

    fn query(&self, name: &str, path: &str) -> GenericResult<Vec<EcbObservation>> {
        let url = format!("{}/{}", self.url, path);
        let url = Url::parse(&url)?;

        let get = |url: &str| -> GenericResult<Vec<EcbObservation>> {
            let response = http::send_request(&self.client, url, None)?;
            parse_observations(response)
        };

        Ok(get(url.as_str()).map_err(|e| format!("Failed to get {name} from {url}: {e}"))?)
    }
}

/// A single ECB observation: `value` is `<CCY>` units per 1 EUR, as published.
struct EcbObservation {
    currency: String,
    date: Date,
    value: Decimal,
}

fn parse_observations(response: Response) -> GenericResult<Vec<EcbObservation>> {
    let response = response.text()?;

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(response.as_bytes());

    let headers = reader.headers()?;
    let (Some(currency_index), Some(date_index), Some(value_index)) = (
        headers.iter().position(|name| name == "CURRENCY"),
        headers.iter().position(|name| name == "TIME_PERIOD"),
        headers.iter().position(|name| name == "OBS_VALUE"),
    ) else {
        return Err!("Got an unexpected ECB CSV header: {:?}", headers);
    };

    let mut observations = Vec::new();

    for record in reader.records() {
        let record = record?;

        let (Some(currency), Some(date), Some(value)) = (
            record.get(currency_index).map(str::to_owned),
            record.get(date_index).and_then(|date| time::parse_date(date, "%Y-%m-%d").ok()),
            record.get(value_index)
                .and_then(|value| util::parse_decimal(value, DecimalRestrictions::StrictlyPositive).ok()),
        ) else {
            return Err!("Got an unexpected ECB CSV record: {:?}", record);
        };

        observations.push(EcbObservation { currency, date, value });
    }

    Ok(observations)
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

#[cfg(test)]
mod tests {
    use mockito::{Mock, Server, ServerGuard};

    use super::*;

    #[test]
    fn current_rates() {
        let (mut server, client) = create_server();

        let _rates_mock = mock_response(
            &mut server,
            "/service/data/EXR/D..EUR.SP00.A?lastNObservations=1&format=csvdata",
            indoc!(
                r#"
                KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2024-06-27,1.0700
                EXR.D.GBP.EUR.SP00.A,D,GBP,EUR,SP00,A,2024-06-27,0.84550
                EXR.D.CHF.EUR.SP00.A,D,CHF,EUR,SP00,A,2024-06-27,0.9577
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
            "/service/data/EXR/D.USD.EUR.SP00.A?startPeriod=2024-01-01&endPeriod=2024-01-05&format=csvdata",
            indoc!(
                r#"
                KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2024-01-02,1.0956
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2024-01-03,1.0912
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2024-01-04,1.0932
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2024-01-05,1.0942
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

    #[test]
    fn historical_rates_real_response() {
        // Captured verbatim from https://data-api.ecb.europa.eu (format=csvdata). Guards against a
        // regression where the parser expects a shape the live API does not actually return.
        let (mut server, client) = create_server();

        let _rates_mock = mock_response(
            &mut server,
            "/service/data/EXR/D.USD.EUR.SP00.A?startPeriod=2025-11-17&endPeriod=2025-11-19&format=csvdata",
            indoc!(
                r#"
                KEY,FREQ,CURRENCY,CURRENCY_DENOM,EXR_TYPE,EXR_SUFFIX,TIME_PERIOD,OBS_VALUE
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2025-11-17,1.1593
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2025-11-18,1.159
                EXR.D.USD.EUR.SP00.A,D,USD,EUR,SP00,A,2025-11-19,1.1583
            "#
            ),
        );

        let start = time::parse_date("2025-11-17", "%Y-%m-%d").unwrap();
        let end = time::parse_date("2025-11-19", "%Y-%m-%d").unwrap();
        let rates = client
            .get_historical_currency_rates("USD", start, end)
            .unwrap();

        assert_eq!(rates.len(), 3);
        assert_eq!(
            rates[2].date,
            time::parse_date("2025-11-19", "%Y-%m-%d").unwrap()
        );
        assert_eq!(rates[2].price, dec!(1) / dec!(1.1583));
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
            .with_header("content-type", "text/csv; charset=utf-8")
            .with_body(data)
            .create()
    }
}

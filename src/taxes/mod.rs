mod calculator;
pub mod germany;
pub mod long_term_ownership;
mod net_calculator;
mod payment_day;
mod rates;
pub mod remapping;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::{Deserializer, Error};

use crate::brokers::Broker;
use crate::core::{EmptyResult, GenericResult};
use crate::currency;
use crate::instruments::EtfClassification;
use crate::localities::Jurisdiction;
use crate::types::Decimal;

pub use self::calculator::{Tax, TaxCalculator, TaxWithheld};
pub use self::long_term_ownership::{
    LtoDeductibleProfit, LtoDeductionCalculator, LtoDeduction,
    NetLtoDeduction, NetLtoDeductionCalculator};
pub use self::net_calculator::{NetTax, NetTaxCalculator};
pub use self::payment_day::{TaxPaymentDay, TaxPaymentDaySpec};
pub use self::rates::{TaxRate, FixedTaxRate, ProgressiveTaxRate};
pub use self::remapping::TaxRemapping;

/// Tax jurisdiction selected in the config. Typed so a typo (e.g. `germny`) is a hard
/// deserialization error rather than a silent fallback to Russia.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaxJurisdiction {
    #[serde(alias = "Russia")]
    Russia,
    #[serde(alias = "Germany")]
    Germany,
}

/// Year-boundary redemption prices (EUR per unit) for a fund, used to compute the Vorabpauschale
/// (§18 InvStG). Supplied manually because a foreign broker's statement carries no German
/// year-start / year-end NAVs. Values are EUR per unit; the tool multiplies by the year-end
/// position quantity.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundNav {
    /// Rücknahmepreis per unit at the start of the calendar year (2 January).
    pub jan1: Decimal,
    /// Rücknahmepreis per unit at the end of the calendar year (31 December).
    pub dec31: Decimal,
    /// Month of acquisition (1–12) when the units were bought during this calendar year, driving
    /// the Zwölftelung (§18 Abs. 2 InvStG). Omit for units held from the start of the year.
    #[serde(default)]
    pub acquired_month: Option<u32>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxConfig {
    #[serde(default)]
    pub income: BTreeMap<i32, Decimal>,
    /// Jurisdiction for tax calculations (default: Russia when omitted; set to `germany` for the
    /// German tax statement).
    #[serde(default)]
    pub jurisdiction: Option<TaxJurisdiction>,
    /// Kirchensteuer (church tax) rate for Germany (8% or 9%), default 0
    #[serde(default)]
    pub church_tax_rate: Option<Decimal>,
    /// Festgestellter Verlustvortrag for the stock pot (Verlustverrechnungstopf Aktien, §20(6)
    /// S.4 EStG) as of Dec 31 of the prior year, in EUR. Offsets only future share-sale gains.
    #[serde(default)]
    pub loss_carryforward_stock: Option<Decimal>,
    /// Festgestellter Verlustvortrag for the general pot as of Dec 31 of the prior year, in EUR.
    /// Offsets all other capital income (dividends, interest, fund/FX gains).
    #[serde(default)]
    pub loss_carryforward_other: Option<Decimal>,
    /// Sparer-Pauschbetrag (saver's allowance) in EUR. Defaults to €1,000 (2023+) / €801 (before);
    /// set to 0 if the allowance is already consumed via a Freistellungsauftrag at a German bank.
    #[serde(default)]
    pub sparer_pauschbetrag: Option<Decimal>,
    /// Deprecated per-year loss carryforward. Superseded by `loss_carryforward_stock` /
    /// `loss_carryforward_other`; kept only to emit a clear migration error if still present.
    #[serde(default)]
    pub loss_carryforward: BTreeMap<i32, Decimal>,
    /// ETF classification by ISIN for Teilfreistellung (partial exemption)
    /// Maps ISIN -> classification (equity, mixed, bond)
    #[serde(default)]
    pub etf_classification: BTreeMap<String, EtfClassification>,
    /// Basiszins per calendar year for the Vorabpauschale (§18 Abs. 4 InvStG), as a fraction
    /// (e.g. `2025: 0.0253`). Overrides the statutory BMF values the tool ships for 2023–2025.
    #[serde(default)]
    pub basiszins: BTreeMap<i32, Decimal>,
    /// Year-boundary NAVs for the Vorabpauschale (§18 InvStG), keyed by ISIN then calendar year.
    #[serde(default)]
    pub fund_nav: BTreeMap<String, BTreeMap<i32, FundNav>>,
    /// Accumulated gross Vorabpauschale already taxed in prior years per fund (ISIN -> EUR). Reduces
    /// the taxable gain when the units are sold (§19 Abs. 1 InvStG, deducted in full before
    /// Teilfreistellung). The tool emits the updated total each year to carry forward.
    #[serde(default)]
    pub vorabpauschale_carryforward: BTreeMap<String, Decimal>,
}

impl TaxConfig {
    /// Get the ETF classification for a given ISIN, defaulting to None (no exemption)
    pub fn get_etf_classification(&self, isin: &str) -> EtfClassification {
        self.etf_classification.get(isin).copied().unwrap_or_default()
    }

    /// German church tax (Kirchensteuer) as a fraction of the Abgeltungsteuer.
    ///
    /// Accepts the documented percent form (0, 8, 9) as well as the fraction form (0.08, 0.09);
    /// anything else is rejected. Used raw, the documented `church_tax_rate: 9` would be applied as
    /// a factor of 9 (a ~900% church tax).
    pub fn german_church_tax_fraction(&self) -> GenericResult<Decimal> {
        let raw = self.church_tax_rate.unwrap_or(Decimal::ZERO);
        Ok(match raw {
            r if r == dec!(0) => dec!(0),
            r if r == dec!(8) || r == dec!(0.08) => dec!(0.08),
            r if r == dec!(9) || r == dec!(0.09) => dec!(0.09),
            other => return Err!(
                "Invalid church_tax_rate {other}: use 0, 8, or 9 (percent), or 0.08 / 0.09 (fraction)"
            ),
        })
    }

    /// German loss carryforward as (stock pot, general pot) EUR amounts. Rejects the deprecated
    /// per-year `loss_carryforward` map with migration guidance.
    pub fn german_loss_carryforward(&self) -> GenericResult<(Decimal, Decimal)> {
        if !self.loss_carryforward.is_empty() {
            return Err!(
                "`taxes.loss_carryforward` (per-year map) is no longer supported: it summed every \
                 year, including future ones. Replace it with `loss_carryforward_stock` and \
                 `loss_carryforward_other` — single EUR amounts, the festgestellter Verlustvortrag \
                 of each pot as of Dec 31 of the prior year"
            );
        }
        Ok((
            self.loss_carryforward_stock.unwrap_or(Decimal::ZERO),
            self.loss_carryforward_other.unwrap_or(Decimal::ZERO),
        ))
    }

    /// German Sparer-Pauschbetrag for the year: the configured value, or the statutory default
    /// (€1,000 single since 2023, €801 before).
    pub fn german_sparer_pauschbetrag(&self, year: i32) -> Decimal {
        self.sparer_pauschbetrag.unwrap_or_else(|| {
            if year >= 2023 {
                dec!(1000)
            } else {
                dec!(801)
            }
        })
    }

    /// Basiszins (as a fraction) for the Vorabpauschale in `year`: the configured override, else the
    /// statutory BMF value. Errors for years the tool ships no default for.
    pub fn german_basiszins(&self, year: i32) -> GenericResult<Decimal> {
        if let Some(&rate) = self.basiszins.get(&year) {
            return Ok(rate);
        }
        Ok(match year {
            2023 => dec!(0.0255),
            2024 => dec!(0.0229),
            2025 => dec!(0.0253),
            _ => return Err!(
                "No Basiszins known for {year}. The BMF publishes it each January; \
                 set `taxes.basiszins.{year}` in the config"
            ),
        })
    }

    /// Year-boundary NAVs for a fund in `year`, if configured.
    pub fn german_fund_nav(&self, isin: &str, year: i32) -> Option<&FundNav> {
        self.fund_nav.get(isin).and_then(|by_year| by_year.get(&year))
    }

    /// Accumulated gross Vorabpauschale already taxed for a fund (EUR), reducing its taxable gain
    /// at sale (§19 InvStG).
    pub fn german_vorabpauschale_carryforward(&self, isin: &str) -> Decimal {
        self.vorabpauschale_carryforward.get(isin).copied().unwrap_or(Decimal::ZERO)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum IncomeType {
    Trading,
    Dividends,
    Interest,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TaxExemption {
    LongTermOwnership,
    TaxFree,
}

impl<'de> Deserialize<'de> for TaxExemption {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "long-term-ownership" => TaxExemption::LongTermOwnership,
            "tax-free" => TaxExemption::TaxFree,
            _ => return Err(D::Error::unknown_variant(&value, &["long-term-ownership", "tax-free"])),
        })
    }
}

pub fn validate_tax_exemptions(broker: Broker, exemptions: &[TaxExemption]) -> EmptyResult {
    if exemptions.is_empty() {
        return Ok(());
    }

    if exemptions.len() > 1 {
        return Err!("Only one tax exemption can be specified per portfolio");
    }

    if broker.jurisdiction() != Jurisdiction::Russia {
        return Err!("Tax exemptions are only supported for brokers with Russia jurisdiction");
    }

    Ok(())
}

// When we work with taxes in Russia, the following rounding rules are applied:
// 1. Result of all calculations must be with kopecks precision
// 2. If we have income in foreign currency then:
//    2.1. Round it to cents
//    2.2. Convert to rubles using precise currency rate (65.4244 for example)
//    2.3. Round to kopecks
// 3. Taxes are calculated with rouble precision. But we should use double rounding here:
//    calculate them with kopecks precision first and then round to roubles.
//
// Декларация program allows to enter income only with kopecks precision - not bigger.
// It calculates tax for $10.64 income with 65.4244 currency rate as following:
// 1. income = round(10.64 * 65.4244, 2) = 696.12 (696.115616 without rounding)
// 2. tax = round(round(696.12 * 0.13, 2), 0) = 91 (90.4956 without rounding)
pub fn round_tax(tax: Decimal, precision: u32) -> Decimal {
    currency::round_to(currency::round(tax), precision)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use super::*;

    #[rstest(tax, expected,
        case("13",      "13"),
        case("13.0000", "13"),
        case("13.1111", "13"),
        case("13.4949", "13"),
        case("13.4950", "14"),
        case("13.9999", "14"),
    )]
    fn tax_rounding(tax: &str, expected: &str) {
        let tax = tax.parse().unwrap();
        let result = round_tax(tax, Jurisdiction::Russia.traits().tax_precision);
        assert_eq!(result, expected.parse().unwrap());
    }

    #[rstest(raw, expected,
        case(None, "0"),
        case(Some("0"), "0"),
        case(Some("8"), "0.08"),
        case(Some("9"), "0.09"),
        case(Some("0.08"), "0.08"),
        case(Some("0.09"), "0.09"),
    )]
    fn german_church_tax_fraction_normalizes(raw: Option<&str>, expected: &str) {
        let config = TaxConfig {
            church_tax_rate: raw.map(|r| r.parse().unwrap()),
            ..Default::default()
        };
        assert_eq!(
            config.german_church_tax_fraction().unwrap(),
            expected.parse().unwrap()
        );
    }

    #[test]
    fn german_church_tax_fraction_rejects_invalid() {
        let config = TaxConfig {
            church_tax_rate: Some(dec!(5)),
            ..Default::default()
        };
        assert!(config.german_church_tax_fraction().is_err());
    }

    #[test]
    fn german_loss_carryforward_reads_split_pots() {
        let config = TaxConfig {
            loss_carryforward_stock: Some(dec!(500)),
            loss_carryforward_other: Some(dec!(300)),
            ..Default::default()
        };
        assert_eq!(
            config.german_loss_carryforward().unwrap(),
            (dec!(500), dec!(300))
        );
    }

    #[test]
    fn german_loss_carryforward_rejects_deprecated_map() {
        let mut config = TaxConfig::default();
        config.loss_carryforward.insert(2023, dec!(1500));
        assert!(config.german_loss_carryforward().is_err());
    }

    #[test]
    fn german_sparer_pauschbetrag_defaults_by_year() {
        let config = TaxConfig::default();
        assert_eq!(config.german_sparer_pauschbetrag(2024), dec!(1000));
        assert_eq!(config.german_sparer_pauschbetrag(2022), dec!(801));

        let overridden = TaxConfig {
            sparer_pauschbetrag: Some(dec!(0)),
            ..Default::default()
        };
        assert_eq!(overridden.german_sparer_pauschbetrag(2024), dec!(0));
    }

    #[test]
    fn german_basiszins_uses_defaults_and_overrides() {
        let config = TaxConfig::default();
        assert_eq!(config.german_basiszins(2023).unwrap(), dec!(0.0255));
        assert_eq!(config.german_basiszins(2024).unwrap(), dec!(0.0229));
        assert_eq!(config.german_basiszins(2025).unwrap(), dec!(0.0253));
        // A year with no shipped default errors rather than guessing.
        assert!(config.german_basiszins(2027).is_err());

        let mut overridden = TaxConfig::default();
        overridden.basiszins.insert(2027, dec!(0.0300));
        assert_eq!(overridden.german_basiszins(2027).unwrap(), dec!(0.0300));
        // A config override wins over the shipped default.
        overridden.basiszins.insert(2025, dec!(0.0260));
        assert_eq!(overridden.german_basiszins(2025).unwrap(), dec!(0.0260));
    }

    #[test]
    fn german_fund_nav_parses_and_looks_up() {
        let config: TaxConfig = serde_yaml::from_str(
            "fund_nav:\n  \
             IE00B4L5Y983:\n    \
             2024:\n      jan1: '80.50'\n      dec31: '92.30'\n      acquired_month: 4\n",
        )
        .unwrap();

        let nav = config.german_fund_nav("IE00B4L5Y983", 2024).unwrap();
        assert_eq!(nav.jan1, dec!(80.50));
        assert_eq!(nav.dec31, dec!(92.30));
        assert_eq!(nav.acquired_month, Some(4));

        assert!(config.german_fund_nav("IE00B4L5Y983", 2023).is_none());
        assert!(config.german_fund_nav("UNKNOWN", 2024).is_none());
    }

    #[test]
    fn german_vorabpauschale_carryforward_defaults_to_zero() {
        let mut config = TaxConfig::default();
        config
            .vorabpauschale_carryforward
            .insert("IE00B4L5Y983".to_string(), dec!(12.34));
        assert_eq!(
            config.german_vorabpauschale_carryforward("IE00B4L5Y983"),
            dec!(12.34)
        );
        assert_eq!(config.german_vorabpauschale_carryforward("UNKNOWN"), dec!(0));
    }

    #[test]
    fn jurisdiction_parses_known_values_and_rejects_typos() {
        let parse = |yaml: &str| serde_yaml::from_str::<TaxConfig>(yaml).map(|c| c.jurisdiction);

        assert_eq!(parse("{}").unwrap(), None);
        assert_eq!(
            parse("jurisdiction: germany").unwrap(),
            Some(TaxJurisdiction::Germany)
        );
        // The capitalized form the old string match accepted still parses.
        assert_eq!(
            parse("jurisdiction: Germany").unwrap(),
            Some(TaxJurisdiction::Germany)
        );
        assert_eq!(
            parse("jurisdiction: russia").unwrap(),
            Some(TaxJurisdiction::Russia)
        );
        // A typo is a hard error, not a silent fallback to Russia.
        assert!(parse("jurisdiction: germny").is_err());
    }
}
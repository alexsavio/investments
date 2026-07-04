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

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxConfig {
    #[serde(default)]
    pub income: BTreeMap<i32, Decimal>,
    /// Jurisdiction for tax calculations (default: Russia, optional: Germany)
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// Kirchensteuer (church tax) rate for Germany (8% or 9%), default 0
    #[serde(default)]
    pub church_tax_rate: Option<Decimal>,
    /// Loss carryforward from previous years (year -> amount in EUR)
    #[serde(default)]
    pub loss_carryforward: BTreeMap<i32, Decimal>,
    /// ETF classification by ISIN for Teilfreistellung (partial exemption)
    /// Maps ISIN -> classification (equity, mixed, bond)
    #[serde(default)]
    pub etf_classification: BTreeMap<String, EtfClassification>,
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
}
mod calculator;
pub mod germany;
pub mod long_term_ownership;
mod net_calculator;
mod payment_day;
mod rates;
pub mod remapping;
pub mod spain;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::{Deserializer, Error};

use crate::brokers::Broker;
use crate::core::{EmptyResult, GenericResult};
use crate::currency;
use crate::instruments::EtfClassification;
use crate::localities::Jurisdiction;
use crate::types::{Date, Decimal};

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
    #[serde(alias = "Spain")]
    Spain,
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

/// Spanish IRPF configuration. Required when `jurisdiction: spain`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanishTaxConfig {
    /// Which regime the filer is subject to. Deliberately has no default: the regimes differ in
    /// the savings scale, in whether acquisition costs are actualized, in whether the two
    /// savings-base groups may offset each other, and in whether custody fees are deductible, so a
    /// guessed default would silently file under the wrong tax code.
    pub regime: spain::SpanishTaxRegime,

    /// Pending negative savings-base balances (saldos negativos pendientes de compensación)
    /// brought in from prior returns, keyed by the year each arose in. Both groups carry forward
    /// for four years, so the origin year is part of the data, not bookkeeping trivia.
    #[serde(default)]
    pub loss_carryforward: SpanishLossCarryforward,

    /// Wash-sale losses already deferred by an earlier return or another tool, seeding the
    /// valores-homogéneos replay with blocked lots the statement itself cannot see.
    #[serde(default)]
    pub deferred_losses: Vec<DeferredLossConfig>,

    /// Actualization coefficients, keyed by disposal year then acquisition year. Overrides and
    /// extends the Decreto Foral tables the tool ships, so a newly published year can be used
    /// without waiting for a release.
    #[serde(default)]
    pub coefficients: BTreeMap<i32, BTreeMap<i32, Decimal>>,
}

impl SpanishTaxConfig {
    /// The savings-base scale in force for this filer's regime in `year`.
    ///
    /// Errors for a year the tool ships no scale for: a savings scale is set by statute each year,
    /// so extrapolating one would file real money under an invented rate.
    pub fn savings_scale(&self, year: i32) -> GenericResult<spain::scale::SavingsScale> {
        spain::scale::SavingsScale::for_year(self.regime, year)
    }

    /// Reject nonsensical `taxes.spain.coefficients` overrides before any lot is priced.
    ///
    /// Not a `serde` check: the shape is valid YAML either way, and the failure a bad coefficient
    /// produces is a plausible-looking number on a tax return rather than a parse error.
    pub fn validate_coefficients(&self) -> EmptyResult {
        spain::coefficients::validate_overrides(&self.coefficients)
    }

    /// Whether a disposal in `year` can be priced at all.
    ///
    /// Always true under Territorio Común, where the coefficient is 1 for every year.
    pub fn has_actualization_table(&self, year: i32) -> bool {
        match self.regime {
            spain::SpanishTaxRegime::Gipuzkoa => {
                spain::coefficients::has_table(year, &self.coefficients)
            }
            spain::SpanishTaxRegime::Comun => true,
        }
    }

    /// The coefficient a FIFO lot's acquisition cost is actualized by before the gain is computed.
    ///
    /// Always 1 under Territorio Común: Ley 26/2014 deleted LIRPF art. 35.2 with effect from 2015,
    /// and even before that actualization applied only to real estate, never to securities.
    pub fn actualization_coefficient(
        &self,
        disposal_year: i32,
        acquisition_date: Date,
    ) -> GenericResult<Decimal> {
        match self.regime {
            spain::SpanishTaxRegime::Gipuzkoa => spain::coefficients::gipuzkoa_coefficient(
                disposal_year,
                acquisition_date,
                &self.coefficients,
            ),
            spain::SpanishTaxRegime::Comun => Ok(Decimal::ONE),
        }
    }

    /// The two savings-base loss ledgers as of `filing_year`, validated against the 4-year
    /// carry-forward window.
    ///
    /// Validation lives here rather than in `serde` because the window is relative to the year
    /// being filed: the same config is valid for one return and expired for the next, so it cannot
    /// be checked at deserialization time.
    pub fn loss_ledgers(
        &self,
        filing_year: i32,
    ) -> GenericResult<(spain::carryforward::LossLedger, spain::carryforward::LossLedger)> {
        Ok((
            spain::carryforward::LossLedger::from_config(
                &self.loss_carryforward.rcm,
                filing_year,
                "rcm",
            )?,
            spain::carryforward::LossLedger::from_config(
                &self.loss_carryforward.gyp,
                filing_year,
                "gyp",
            )?,
        ))
    }
}

/// Pending negative balances per savings-base group, keyed by the year each arose in.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanishLossCarryforward {
    /// Rendimientos del capital mobiliario (dividends, interest).
    #[serde(default)]
    pub rcm: BTreeMap<i32, Decimal>,
    /// Ganancias y pérdidas patrimoniales from transfers.
    #[serde(default)]
    pub gyp: BTreeMap<i32, Decimal>,
}

/// A loss deferred under the valores-homogéneos rule, blocked against a repurchased holding.
///
/// Doubles as the tool's own carry-out format: the statement prints surviving blocked lots in
/// exactly this shape so next year's config is a copy-paste rather than a manual reconstruction.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeferredLossConfig {
    /// Ticker of the instrument whose loss is blocked.
    pub symbol: String,
    /// ISIN, preferred over the symbol for identity when present (tickers get reused and renamed).
    #[serde(default)]
    pub isin: Option<String>,
    /// Deferred loss magnitude in EUR, as a positive number.
    pub loss: Decimal,
    /// Shares still blocking the loss, i.e. those repurchased inside the window and not yet resold.
    pub blocked_quantity: Decimal,
    /// When the blocking shares were acquired.
    ///
    /// Not bookkeeping trivia: reintegration triggers when *those* shares are disposed of, and a
    /// FIFO lot identifies itself by its acquisition date. Without it the deferral could not be
    /// matched to the sale that releases it.
    #[serde(deserialize_with = "deserialize_spanish_date")]
    pub acquisition_date: Date,
    /// Date of the loss-making sale the deferral came from.
    #[serde(deserialize_with = "deserialize_spanish_date")]
    pub sale_date: Date,
}

/// Accepts ISO `YYYY-MM-DD` as well as the `YYYY.MM.DD` / `DD.MM.YYYY` forms the rest of the config
/// takes. ISO is what the tool prints in its own carry-out block, so its output round-trips back
/// into the config without the user having to reformat every date by hand.
fn deserialize_spanish_date<'de, D>(deserializer: D) -> Result<Date, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    crate::time::parse_date(&raw, "%Y-%m-%d")
        .or_else(|_| crate::time::parse_user_date(&raw))
        .map_err(D::Error::custom)
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
    /// Spanish IRPF settings. Nested per-jurisdiction block (the flat `german_*` fields above are
    /// the older pattern; nesting is the pattern for jurisdictions added since).
    #[serde(default)]
    pub spain: Option<SpanishTaxConfig>,
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
    /// The `taxes.spain` block, required by the Spanish filing path.
    ///
    /// Unlike [`spanish_regime`](Self::spanish_regime), which defaults so the analysis views keep
    /// working, this errors: a filed statement computed under a guessed regime would be wrong in
    /// the scale, the coefficients, the cross-group offset and fee deductibility all at once.
    pub fn spanish(&self) -> GenericResult<&SpanishTaxConfig> {
        match self.spain {
            Some(ref spain) => Ok(spain),
            None => Err!(
                "`taxes.jurisdiction` is `spain` but there is no `taxes.spain` block. Add one and \
                 set `regime` to `gipuzkoa` or `comun` — the two regimes differ in the savings \
                 scale, in whether acquisition costs are actualized, in whether the savings-base \
                 groups may offset each other, and in whether custody fees are deductible, so \
                 there is no safe default"
            ),
        }
    }

    /// Regime driving the `localities::spain` analysis approximation.
    ///
    /// Falls back to Gipuzkoa when the `taxes.spain` block is absent, because the analysis and
    /// rebalancing views must still produce a rate rather than fail. The filing path does not share
    /// this leniency: it reads the block directly and errors when it is missing, so a real
    /// statement is never generated under a guessed regime.
    pub fn spanish_regime(&self) -> spain::SpanishTaxRegime {
        self.spain
            .as_ref()
            .map(|spain| spain.regime)
            .unwrap_or(spain::SpanishTaxRegime::Gipuzkoa)
    }

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
    fn spanish_config_parses_the_full_block() {
        let config: TaxConfig = serde_yaml::from_str(
            "jurisdiction: spain\n\
             spain:\n  \
             regime: gipuzkoa\n  \
             loss_carryforward:\n    \
             gyp: {2024: '1200.50'}\n    \
             rcm: {2025: '80.00'}\n  \
             deferred_losses:\n    \
             - {symbol: VUSA, isin: IE00B3XXRP09, loss: '420.00', blocked_quantity: '15', \
             acquisition_date: '2025-12-20', sale_date: '2025-12-10'}\n  \
             coefficients:\n    \
             2027: {2020: '1.11'}\n",
        )
        .unwrap();

        let spain = config.spanish().unwrap();
        assert_eq!(spain.regime, spain::SpanishTaxRegime::Gipuzkoa);
        assert_eq!(spain.loss_carryforward.gyp[&2024], dec!(1200.50));
        assert_eq!(spain.loss_carryforward.rcm[&2025], dec!(80));
        assert_eq!(spain.coefficients[&2027][&2020], dec!(1.11));

        let deferred = &spain.deferred_losses[0];
        assert_eq!(deferred.symbol, "VUSA");
        assert_eq!(deferred.isin.as_deref(), Some("IE00B3XXRP09"));
        assert_eq!(deferred.loss, dec!(420));
        assert_eq!(deferred.blocked_quantity, dec!(15));
        // The acquisition date of the blocking shares is what reintegration matches on, so it is
        // required alongside the sale the deferral came from.
        assert_eq!(deferred.acquisition_date, date!(2025, 12, 20));
        assert_eq!(deferred.sale_date, date!(2025, 12, 10));
    }

    /// Only `regime` is required: a filer with no prior-year balances and no deferred losses should
    /// not have to write four empty sections to get a statement.
    #[test]
    fn spanish_config_defaults_everything_but_the_regime() {
        let config: TaxConfig = serde_yaml::from_str("spain:\n  regime: comun\n").unwrap();

        let spain = config.spanish().unwrap();
        assert_eq!(spain.regime, spain::SpanishTaxRegime::Comun);
        assert!(spain.loss_carryforward.gyp.is_empty());
        assert!(spain.loss_carryforward.rcm.is_empty());
        assert!(spain.deferred_losses.is_empty());
        assert!(spain.coefficients.is_empty());
    }

    /// The regime drives the scale, the coefficients, the cross-group offset and fee
    /// deductibility, so omitting it must fail rather than default to either regime.
    #[test]
    fn spanish_config_requires_the_regime() {
        assert!(serde_yaml::from_str::<TaxConfig>("spain: {}\n").is_err());
        // A typo is a hard error too, not a silent fallback.
        assert!(serde_yaml::from_str::<TaxConfig>("spain:\n  regime: guipuzcoa\n").is_err());
        // `deny_unknown_fields` catches a misspelled key rather than ignoring the setting.
        assert!(
            serde_yaml::from_str::<TaxConfig>("spain:\n  regime: comun\n  coeficients: {}\n")
                .is_err()
        );
    }

    /// The accented spelling is what a Spanish filer would naturally write.
    #[test]
    fn spanish_config_accepts_the_accented_regime_spelling() {
        let config: TaxConfig = serde_yaml::from_str("spain:\n  regime: común\n").unwrap();
        assert_eq!(
            config.spanish().unwrap().regime,
            spain::SpanishTaxRegime::Comun
        );
    }

    /// Filing without the block must name the block and the values it needs, not fail obscurely
    /// somewhere downstream in the processor.
    #[test]
    fn spanish_accessor_errors_without_the_block() {
        let error = TaxConfig::default().spanish().unwrap_err().to_string();
        assert!(error.contains("taxes.spain"), "{error}");
        assert!(error.contains("gipuzkoa"), "{error}");
        assert!(error.contains("comun"), "{error}");
    }

    /// Actualization is the single largest structural difference between the regimes on a capital
    /// gain, so the regime switch must reach it.
    #[test]
    fn spanish_actualization_is_gipuzkoa_only() {
        let config = |regime: &str| -> TaxConfig {
            serde_yaml::from_str(&format!("spain:\n  regime: {regime}\n")).unwrap()
        };

        assert_eq!(
            config("gipuzkoa")
                .spanish()
                .unwrap()
                .actualization_coefficient(2026, date!(2021, 3, 10))
                .unwrap(),
            dec!(1.212)
        );
        // Territorio Común never actualizes, so the year pair is irrelevant and it never errors —
        // not even for a disposal year no foral table is shipped for.
        for year in [2024, 2026, 2030] {
            assert_eq!(
                config("comun")
                    .spanish()
                    .unwrap()
                    .actualization_coefficient(year, date!(2021, 3, 10))
                    .unwrap(),
                dec!(1)
            );
        }
    }

    /// The scale follows the configured regime, so the same base is taxed differently under each.
    #[test]
    fn spanish_savings_scale_follows_the_regime_and_year() {
        let config = |regime: &str| -> TaxConfig {
            serde_yaml::from_str(&format!("spain:\n  regime: {regime}\n")).unwrap()
        };

        let gipuzkoa = config("gipuzkoa");
        let comun = config("comun");

        // 2026: Gipuzkoa 7,500×19% + 2,500×20% = 1,925; state 6,000×19% + 4,000×21% = 1,980.
        assert_eq!(
            gipuzkoa
                .spanish()
                .unwrap()
                .savings_scale(2026)
                .unwrap()
                .tax(dec!(10000)),
            dec!(1925)
        );
        assert_eq!(
            comun
                .spanish()
                .unwrap()
                .savings_scale(2026)
                .unwrap()
                .tax(dec!(10000)),
            dec!(1980)
        );
        // A year outside the shipped range errors rather than extrapolating.
        assert!(gipuzkoa.spanish().unwrap().savings_scale(2027).is_err());
    }

    /// The window is relative to the year being filed, so the same config is valid for one return
    /// and expired for the next. That is why it cannot be a `serde` validation.
    #[test]
    fn spanish_loss_ledgers_are_validated_against_the_filing_year() {
        let config: TaxConfig = serde_yaml::from_str(
            "spain:\n  \
             regime: gipuzkoa\n  \
             loss_carryforward:\n    \
             gyp: {2022: '300'}\n    \
             rcm: {2025: '80'}\n",
        )
        .unwrap();
        let spain = config.spanish().unwrap();

        let (rcm, gyp) = spain.loss_ledgers(2026).unwrap();
        assert_eq!(rcm.total(), dec!(80));
        assert_eq!(gyp.total(), dec!(300));
        // 2022 is in its fourth and final year against the 2026 return.
        assert_eq!(gyp.expiring_after(2026), dec!(300));

        // One year on, the same 2022 balance is out of the window and must be rejected.
        assert!(spain.loss_ledgers(2027).is_err());
    }

    /// The carry-out block the tool prints uses ISO dates, so its own output must parse back in.
    #[rstest(raw, case("2025-12-10"), case("2025.12.10"), case("10.12.2025"))]
    fn spanish_deferred_loss_accepts_iso_and_user_dates(raw: &str) {
        let config: TaxConfig = serde_yaml::from_str(&format!(
            "spain:\n  regime: gipuzkoa\n  deferred_losses:\n    \
             - {{symbol: VUSA, loss: '420.00', blocked_quantity: '15', \
             acquisition_date: '{raw}', sale_date: '{raw}'}}\n"
        ))
        .unwrap();

        let deferred = &config.spanish().unwrap().deferred_losses[0];
        assert_eq!(deferred.sale_date, date!(2025, 12, 10));
        assert_eq!(deferred.acquisition_date, date!(2025, 12, 10));
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

        assert_eq!(
            parse("jurisdiction: spain").unwrap(),
            Some(TaxJurisdiction::Spain)
        );
        assert_eq!(
            parse("jurisdiction: Spain").unwrap(),
            Some(TaxJurisdiction::Spain)
        );
    }
}

//! End-to-end tests for the German tax pipeline: Flex Query XML → BrokerStatement →
//! GermanTaxStatement → CSV. These exercise the real code paths (FIFO cost basis, Flex
//! Query ingestion, currency conversion) that the per-module unit tests never touch.
//!
//! Fixtures live under `testdata/germany/`. The currency converter is a fixed-rate EUR
//! backend so expected values are hand-computable and no network/database is required.

use std::path::PathBuf;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::config::{Config, ForeignCurrencyTaxation, PortfolioConfig};
use crate::core::{EmptyResult, GenericResult};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterBackend};
use crate::taxes::TaxConfig;
use crate::time::{self, Date};
use crate::types::Decimal;

use super::{GermanTaxStatement, format_eur, process_broker_statement};

/// Fixed EUR conversion backend: every non-EUR currency converts to EUR at a constant rate,
/// independent of date. Keeps expected tax figures hand-computable.
struct FixedEurBackend {
    today: Date,
    eur_per_usd: Decimal,
}

impl CurrencyConverterBackend for FixedEurBackend {
    fn today(&self) -> Date {
        self.today
    }

    fn batch(&self, _from: &str, _to: &str, _date: Date) -> EmptyResult {
        Ok(())
    }

    fn currency_rate(
        &self,
        from: &str,
        to: &str,
        _date: Date,
    ) -> GenericResult<(Option<Decimal>, Option<Decimal>)> {
        assert_eq!(to, "EUR", "the German pipeline only ever converts to EUR");
        match from {
            "EUR" => Ok((None, None)),
            "USD" => Ok((Some(self.eur_per_usd), None)),
            other => Err!("fixture converter has no rate for {other}"),
        }
    }
}

fn converter() -> CurrencyConverter {
    CurrencyConverter::new_with_backend(Box::new(FixedEurBackend {
        today: time::today(),
        eur_per_usd: dec!(0.9),
    }))
}

pub(super) fn read_fixture(name: &str) -> BrokerStatement {
    // The fixture path is repo-relative, so it is assigned after deserialization: the config
    // deserializer requires an absolute path, which a checked-out test tree cannot provide.
    let mut portfolio: PortfolioConfig =
        serde_yaml::from_str("name: test\nbroker: interactive-brokers\n").unwrap();
    portfolio.statements = Some(PathBuf::from(format!(
        "src/tax_statement/germany/testdata/{name}"
    )));

    BrokerStatement::load(&Config::mock(), &portfolio, ReadingStrictness::all()).unwrap()
}

/// Run the full German tax pipeline over a fixture with an explicit tax config, defaulting to the
/// §20 (interest-bearing) foreign-currency treatment.
pub(super) fn run_pipeline_with_config(
    fixture: &str,
    year: i32,
    tax_config: &TaxConfig,
) -> GermanTaxStatement {
    run_pipeline_full(
        fixture,
        year,
        tax_config,
        ForeignCurrencyTaxation::InterestBearing,
    )
}

/// Run the full German tax pipeline with an explicit tax config and foreign-currency treatment.
pub(super) fn run_pipeline_full(
    fixture: &str,
    year: i32,
    tax_config: &TaxConfig,
    fx_taxation: ForeignCurrencyTaxation,
) -> GermanTaxStatement {
    let statement = read_fixture(fixture);
    let converter = converter();

    let mut german = GermanTaxStatement::new(year, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
    process_broker_statement(
        &mut german,
        &statement,
        year,
        &converter,
        tax_config,
        fx_taxation,
        &Default::default(),
    )
    .unwrap();
    german.calculate_totals();
    german
}

/// Run the full German tax pipeline over a fixture and return the finalized statement.
pub(super) fn run_pipeline(fixture: &str, year: i32) -> GermanTaxStatement {
    run_pipeline_with_config(fixture, year, &TaxConfig::default())
}

/// The shared EUR formatter (used by both the CSV and the console) rounds half away from zero, the
/// German tax-form convention. This is exactly where the `{:.2}` formatter diverges: it rounds
/// half-to-even, so a `…2.325` midpoint would render as `12.32` on the console while the CSV shows
/// `12.33`. Routing both surfaces through this helper keeps them in agreement.
#[test]
fn format_eur_rounds_half_away_from_zero() {
    assert_eq!(format_eur(dec!(12.325)), "12.33");
    assert_eq!(format_eur(dec!(-12.325)), "-12.33");
    assert_eq!(format_eur(dec!(0)), "0.00");
    assert_eq!(format_eur(dec!(29.35)), "29.35");
}

/// Smoke test: the fixture parses through the real IB Flex reader and the two AAPL sells are
/// FIFO-matched. Guards the harness itself so the value assertions below fail for the right
/// reason (wrong number), not because the pipeline is broken.
#[test]
fn harness_reads_fifo_fixture() {
    let statement = read_fixture("fifo");
    assert_eq!(statement.stock_buys.len(), 2);
    assert_eq!(statement.stock_sells.len(), 2);
}

/// FIFO cost basis must consume lots. AAPL: buy 100@$10 (Jan), buy 100@$20 (Feb), sell 100
/// (Mar, $15), sell 100 (Apr, $25). At 0.9 EUR/USD each sale nets €450, total €900. The
/// broken `calculate_cost_basis` re-matches both sales against the €10 lot, reporting the
/// April cost as €900 instead of €1800 and inflating the total gain to €1800.
///
/// Enabled by T2 (per-lot FIFO via `StockSell::calculate`).
#[test]
fn fifo_cost_basis_consumes_lots() {
    let german = run_pipeline("fifo", 2024);
    assert_eq!(german.total_capital_gains, dec!(900));
    assert_eq!(german.total_capital_losses, dec!(0));
}

/// A real stock whose ticker ends in 'W' (GLW, Corning) must appear in the tax report. The old
/// symbol-pattern `is_derivative` check dropped any 'W'-ending or "WS"/"WT"/"NOTE"/"CERT" symbol,
/// silently omitting real tickers. Asset-category filtering at the parser keeps the STK and drops
/// the OPT row, so GLW's €450 gain (buy 100@$10, sell 100@$15, 0.9 EUR/USD) is reported.
///
/// Enabled by T8 (remove symbol-pattern derivative detection).
#[test]
fn real_ticker_ending_in_w_is_not_dropped() {
    let german = run_pipeline("derivative", 2024);
    assert_eq!(german.total_capital_gains, dec!(450));
    assert_eq!(german.capital_gains.len(), 1);
    assert_eq!(german.capital_gains[0].symbol, "GLW");
}

/// Vorabpauschale (§18 InvStG) is computed for a fund held at year end, using config NAVs.
///
/// EUNL (IE00B4L5Y983, equity 30% Teilfreistellung), 100 units, NAV €80 → €92, Basiszins 2024
/// 2.29%, no distributions: basisertrag = 8000 × 0.0229 × 0.7 = 128.24 (below the €1,200 value
/// increase); taxable after Teilfreistellung = 128.24 × 0.70 = 89.768. Deemed received 2 Jan 2025,
/// so it is not part of the 2024 taxable base.
///
/// Enabled by T11 (Vorabpauschale).
#[test]
fn vorabpauschale_computed_for_year_end_fund_holding() {
    let mut tax_config = TaxConfig::default();
    tax_config.etf_classification.insert(
        "IE00B4L5Y983".to_string(),
        crate::instruments::EtfClassification::Equity,
    );
    let mut by_year = std::collections::BTreeMap::new();
    by_year.insert(
        2024,
        crate::taxes::FundNav {
            jan1: dec!(80),
            dec31: dec!(92),
            acquired_month: None,
        },
    );
    tax_config
        .fund_nav
        .insert("IE00B4L5Y983".to_string(), by_year);

    let german = run_pipeline_with_config("vorabpauschale", 2024, &tax_config);

    assert_eq!(german.vorabpauschale.len(), 1);
    let vp = &german.vorabpauschale[0];
    assert_eq!(vp.isin, "IE00B4L5Y983");
    assert_eq!(vp.gross_vorabpauschale, dec!(128.24));
    assert_eq!(vp.taxable_amount, dec!(89.768));
    assert_eq!(vp.deemed_received, Date::from_ymd_opt(2025, 1, 2).unwrap());
    assert_eq!(german.total_vorabpauschale_gross, dec!(128.24));
    // Vorabpauschale is next-year income, so the 2024 taxable base stays zero.
    assert_eq!(german.total_taxable_income, dec!(0));
    assert!(german.vorabpauschale_missing_nav.is_empty());
}

/// §19 InvStG: accumulated Vorabpauschale reduces a fund's sale gain, in full and before
/// Teilfreistellung, on a full disposal. EUNL fully sold in 2024 for a €2,000 gross gain with a
/// €300 accumulated carryforward → taxable = (2000 − 300) × 0.70 = €1,190 (vs €1,400 without it).
///
/// Enabled by T11 (Vorabpauschale).
#[test]
fn vorabpauschale_carryforward_reduces_fund_sale_gain() {
    let mut tax_config = TaxConfig::default();
    tax_config.etf_classification.insert(
        "IE00B4L5Y983".to_string(),
        crate::instruments::EtfClassification::Equity,
    );
    tax_config
        .vorabpauschale_carryforward
        .insert("IE00B4L5Y983".to_string(), dec!(300));

    let german = run_pipeline_with_config("vorabpauschale_sale", 2024, &tax_config);

    assert_eq!(german.capital_gains.len(), 1);
    let sale = &german.capital_gains[0];
    assert!(!sale.is_stock);
    assert_eq!(sale.gross_gain_loss, dec!(2000));
    assert_eq!(sale.taxable_amount, dec!(1190));
    assert!(sale.notes.as_deref().unwrap().contains("§19"));
    // The Anlage KAP-INV Veräußerung line must carry the §19-reduced gross (2000 − 300 = 1700),
    // not the raw 2000 — otherwise the filed figure re-taxes the already-taxed Vorabpauschale.
    assert_eq!(sale.taxable_before_exemption, dec!(1700));
    assert_eq!(german.kap_inv_equity.sale_gains, dec!(1700));
    // Nothing held at year end, so there is no Vorabpauschale to compute.
    assert!(german.vorabpauschale.is_empty());
}

/// Short positions get no tax computation: they are reported separately for manual §20 review and
/// never enter the cost-basis reconciliation. The fixture holds a long EUNL position (reconciled
/// against its buy) and a short −50 TSLA; only the short lands in `short_positions`.
///
/// Enabled by T12b (short-position reporting).
#[test]
fn short_positions_reported_for_manual_review() {
    let german = run_pipeline("short_position", 2024);

    assert_eq!(
        german.short_positions,
        vec![("TSLA".to_string(), dec!(-50))]
    );
    // The long fund holding is not misfiled as a short.
    assert!(
        !german
            .short_positions
            .iter()
            .any(|(symbol, _)| symbol == "EUNL")
    );
    // No capital gain is fabricated from the open short.
    assert!(german.capital_gains.is_empty());
}

/// Vorabpauschale uses holdings as of 31 December of the tax year, not the statement's last-date
/// snapshot. The statement runs into mid-2025; EUNL is held at end of 2024 but sold in March 2025,
/// so it is absent from the end-of-statement open positions. The 2024 Vorabpauschale must still be
/// computed on the 100 units held on 31 Dec 2024 (gross 128.24, as in the single-year case).
///
/// Enabled by T12/HIGH-2 (tax-year-end holdings reconstruction).
#[test]
fn vorabpauschale_uses_tax_year_end_holdings_not_statement_end() {
    let mut tax_config = TaxConfig::default();
    tax_config.etf_classification.insert(
        "IE00B4L5Y983".to_string(),
        crate::instruments::EtfClassification::Equity,
    );
    let mut by_year = std::collections::BTreeMap::new();
    by_year.insert(
        2024,
        crate::taxes::FundNav {
            jan1: dec!(80),
            dec31: dec!(92),
            acquired_month: None,
        },
    );
    tax_config
        .fund_nav
        .insert("IE00B4L5Y983".to_string(), by_year);

    let german = run_pipeline_with_config("vorabpauschale_post_year_end_sale", 2024, &tax_config);

    assert_eq!(german.vorabpauschale.len(), 1);
    assert_eq!(german.vorabpauschale[0].quantity, dec!(100));
    assert_eq!(german.vorabpauschale[0].gross_vorabpauschale, dec!(128.24));
    // The 2025 sale is not a 2024 capital gain.
    assert!(german.capital_gains.is_empty());
}

/// When the config does not pin `acquired_month`, the Zwölftelung is derived from the trade history:
/// EUNL bought 15 Apr 2024 (3 full months before the acquisition month) → 9/12 of the full-year
/// figure. Full VP = 8000 × 0.0229 × 0.7 = 128.24; prorated = 128.24 × 9/12 = 96.18.
///
/// Enabled by T12/LOW-6 (derive acquisition month from trades).
#[test]
fn vorabpauschale_prorates_derived_midyear_acquisition() {
    let mut tax_config = TaxConfig::default();
    tax_config.etf_classification.insert(
        "IE00B4L5Y983".to_string(),
        crate::instruments::EtfClassification::Equity,
    );
    let mut by_year = std::collections::BTreeMap::new();
    by_year.insert(
        2024,
        crate::taxes::FundNav {
            jan1: dec!(80),
            dec31: dec!(92),
            acquired_month: None, // not pinned — derived from the 15 Apr 2024 buy
        },
    );
    tax_config
        .fund_nav
        .insert("IE00B4L5Y983".to_string(), by_year);

    let german = run_pipeline_with_config("vorabpauschale_midyear", 2024, &tax_config);

    assert_eq!(german.vorabpauschale.len(), 1);
    assert_eq!(german.vorabpauschale[0].gross_vorabpauschale, dec!(96.18));
}

/// A year-end fund holding whose NAVs are not configured is flagged, not silently omitted.
#[test]
fn vorabpauschale_missing_nav_is_flagged() {
    let mut tax_config = TaxConfig::default();
    tax_config.etf_classification.insert(
        "IE00B4L5Y983".to_string(),
        crate::instruments::EtfClassification::Equity,
    );
    // No fund_nav entry configured for the held fund.

    let german = run_pipeline_with_config("vorabpauschale", 2024, &tax_config);

    assert!(german.vorabpauschale.is_empty());
    assert_eq!(german.vorabpauschale_missing_nav, vec!["IE00B4L5Y983"]);
}

/// A year with no shipped default and no configured Basiszins warns and skips the Vorabpauschale
/// instead of aborting the whole report. NAV is configured for 2027, so this exercises the missing
/// Basiszins path specifically (not the missing-NAV one).
#[test]
fn vorabpauschale_missing_basiszins_skips_without_aborting() {
    let mut tax_config = TaxConfig::default();
    tax_config.etf_classification.insert(
        "IE00B4L5Y983".to_string(),
        crate::instruments::EtfClassification::Equity,
    );
    let mut by_year = std::collections::BTreeMap::new();
    by_year.insert(
        2027,
        crate::taxes::FundNav {
            jan1: dec!(80),
            dec31: dec!(92),
            acquired_month: None,
        },
    );
    tax_config
        .fund_nav
        .insert("IE00B4L5Y983".to_string(), by_year);
    // No taxes.basiszins entry for 2027, and the tool ships no default beyond 2025.

    // Must not panic: process_broker_statement returns Ok, so the rest of the report still generates.
    let german = run_pipeline_with_config("vorabpauschale", 2027, &tax_config);

    assert!(german.vorabpauschale.is_empty());
    // The NAV is present, so this is a missing-Basiszins skip, not a missing-NAV one.
    assert!(german.vorabpauschale_missing_nav.is_empty());
}

// The IB Flex parser's income-dedup and edge-row handling (T3) is verified next to the parser
// itself in `broker_statement::ib::flex_query` (the `FlexQueryResponse` type is private to that
// module), against the shared `income_edge` fixture in this module's `testdata/` directory.

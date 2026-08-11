//! End-to-end tests for the Spanish tax pipeline: Flex Query XML → BrokerStatement →
//! SpanishTaxStatement → CSV.
//!
//! Fixtures live under `testdata/`. The currency converter is a fixed-rate EUR backend so expected
//! values are hand-computable and no network/database is required. It duplicates the German
//! module's backend rather than sharing one: the top-level `testdata/` submodule is private and
//! empty, so these in-src fixtures are the only runnable path, and ~30 test-only lines are a
//! cheaper price than a shared test-utils module that both jurisdictions must then agree on.

use std::path::PathBuf;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::brokers::Broker;
use crate::config::Config;
use crate::core::{EmptyResult, GenericResult};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterBackend};
use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::{SpanishTaxConfig, TaxConfig, TaxRemapping};
use crate::time::{self, Date};
use crate::types::Decimal;

use super::SpanishTaxStatement;

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
        assert_eq!(to, "EUR", "the Spanish pipeline only ever converts to EUR");
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

fn read_fixture(name: &str) -> BrokerStatement {
    let broker = Broker::InteractiveBrokers
        .get_info(&Config::mock(), None)
        .unwrap();
    let path = PathBuf::from(format!("src/tax_statement/spain/testdata/{name}"));
    BrokerStatement::read(
        broker,
        &path,
        &Default::default(),
        &Default::default(),
        &Default::default(),
        TaxRemapping::new(),
        &[],
        &[],
        ReadingStrictness::all(),
    )
    .unwrap()
}

fn spain_config(regime: SpanishTaxRegime) -> TaxConfig {
    TaxConfig {
        spain: Some(SpanishTaxConfig {
            regime,
            loss_carryforward: Default::default(),
            deferred_losses: Vec::new(),
            coefficients: Default::default(),
        }),
        ..Default::default()
    }
}

/// Run the full Spanish pipeline over a fixture with an explicit tax config.
fn run_pipeline_with_config(
    fixture: &str,
    year: i32,
    tax_config: &TaxConfig,
) -> SpanishTaxStatement {
    let statement = read_fixture(fixture);
    let converter = converter();
    let (spanish, _has_income) =
        super::compute_tax_year(&statement, year, &converter, tax_config).unwrap();
    spanish
}

/// Run the full Spanish pipeline over a fixture under one regime.
fn run_pipeline(fixture: &str, year: i32, regime: SpanishTaxRegime) -> SpanishTaxStatement {
    run_pipeline_with_config(fixture, year, &spain_config(regime))
}

/// Smoke test: the fixture parses through the real IB Flex reader and the two AAPL sells are
/// FIFO-matched across the 2021 and 2024 buys. Guards the harness itself, so the value assertions
/// added by later phases fail for the right reason (wrong number), not because the pipeline is
/// broken.
#[test]
fn harness_reads_fifo_fixture() {
    let statement = read_fixture("fifo");
    assert_eq!(statement.stock_buys.len(), 2);
    assert_eq!(statement.stock_sells.len(), 2);
    // The fixture converter is what makes the expected EUR figures hand-computable.
    assert_eq!(
        converter()
            .convert_to(time::today(), crate::currency::Cash::new("USD", dec!(100)), "EUR")
            .unwrap(),
        dec!(90)
    );
}

/// FIFO must consume lots, and the actualization coefficient is a **per-lot** figure keyed to that
/// lot's acquisition year — not one coefficient for the whole position.
///
/// AAPL: buy 100 @ $100 on 2021-03-10, buy 100 @ $200 on 2024-06-10, sell 100 @ $250 on 2026-04-15,
/// sell 100 @ $300 on 2026-09-15, no commissions, 0.9 EUR/USD. The March sale takes the 2021 lot
/// (coefficient 1.212), the September sale the 2024 lot (1.050):
///
/// - sale 1: 22,500 − 9,000 × 1.212 = 22,500 − 10,908 = 11,592
/// - sale 2: 27,000 − 18,000 × 1.050 = 27,000 − 18,900 = 8,100
#[test]
fn gipuzkoa_actualizes_each_fifo_lot_by_its_acquisition_year() {
    let spain = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);

    assert_eq!(spain.capital_gains.len(), 2);

    let first = &spain.capital_gains[0];
    assert_eq!(first.symbol, "AAPL");
    assert_eq!(first.sale_date, Date::from_ymd_opt(2026, 4, 15).unwrap());
    assert_eq!(first.quantity, dec!(100));
    assert_eq!(first.proceeds_eur, dec!(22500));
    assert_eq!(first.cost_eur, dec!(9000));
    assert_eq!(first.actualized_cost_eur, dec!(10908));
    assert_eq!(first.fiscal_gain_loss, dec!(11592));
    assert_eq!(first.integrable_amount, dec!(11592));

    // The per-lot audit line must reproduce the coefficient arithmetic step by step.
    assert_eq!(first.lots.len(), 1);
    let lot = &first.lots[0];
    assert_eq!(lot.acquisition_date, Date::from_ymd_opt(2021, 3, 10).unwrap());
    assert_eq!(lot.quantity, dec!(100));
    assert_eq!(lot.cost_eur, dec!(9000));
    assert_eq!(lot.coefficient, dec!(1.212));
    assert_eq!(lot.actualized_cost_eur, dec!(10908));
    assert_eq!(lot.gain_eur, dec!(11592));

    let second = &spain.capital_gains[1];
    assert_eq!(second.sale_date, Date::from_ymd_opt(2026, 9, 15).unwrap());
    assert_eq!(second.cost_eur, dec!(18000));
    assert_eq!(second.lots[0].coefficient, dec!(1.050));
    assert_eq!(second.actualized_cost_eur, dec!(18900));
    assert_eq!(second.fiscal_gain_loss, dec!(8100));

    // Year totals: 11,592 + 8,100, taxed at 1,425 + 1,500 + 4,692 × 22%.
    assert_eq!(spain.gyp_net, dec!(19692));
    assert_eq!(spain.savings_base, dec!(19692));
    assert_eq!(spain.savings_quota, dec!(3957.24));
}

/// Territorio Común abolished actualization in 2015, so the same trades are taxed on their nominal
/// gain: 22,500 − 9,000 and 27,000 − 18,000.
#[test]
fn comun_does_not_actualize() {
    let spain = run_pipeline("fifo", 2026, SpanishTaxRegime::Comun);

    assert_eq!(spain.capital_gains[0].lots[0].coefficient, dec!(1));
    assert_eq!(spain.capital_gains[0].actualized_cost_eur, dec!(9000));
    assert_eq!(spain.capital_gains[0].fiscal_gain_loss, dec!(13500));
    assert_eq!(spain.capital_gains[1].fiscal_gain_loss, dec!(9000));
    assert_eq!(spain.gyp_net, dec!(22500));
}

/// The two regimes must differ on these trades by **exactly** the actualization effect and nothing
/// else, which is what proves the coefficient is the only thing the regime switch changed here.
#[test]
fn regimes_differ_exactly_by_the_coefficient_delta() {
    let gipuzkoa = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
    let comun = run_pipeline("fifo", 2026, SpanishTaxRegime::Comun);

    // 9,000 × 0.212 + 18,000 × 0.050 = 1,908 + 900.
    let expected_delta = dec!(9000) * dec!(0.212) + dec!(18000) * dec!(0.050);
    assert_eq!(expected_delta, dec!(2808));
    assert_eq!(comun.gyp_net - gipuzkoa.gyp_net, expected_delta);

    // Everything the coefficient does not touch is identical.
    for (a, b) in gipuzkoa.capital_gains.iter().zip(&comun.capital_gains) {
        assert_eq!(a.proceeds_eur, b.proceeds_eur);
        assert_eq!(a.cost_eur, b.cost_eur);
        assert_eq!(a.quantity, b.quantity);
        assert_eq!(a.sale_date, b.sale_date);
    }
}

/// A sale outside the requested tax year is not this year's income, even though the statement spans
/// it. The fixture's buys are 2021 and 2024; nothing is disposed of before 2026.
#[test]
fn only_the_requested_tax_year_is_reported() {
    for year in [2024, 2025] {
        let spain = run_pipeline("fifo", year, SpanishTaxRegime::Gipuzkoa);
        assert!(spain.capital_gains.is_empty(), "year {year}");
        assert_eq!(spain.gyp_net, dec!(0));
        assert_eq!(spain.savings_quota, dec!(0));
    }
}

/// A year the tool ships no scale for must fail loudly rather than compute a statement at an
/// invented rate.
#[test]
fn unsupported_tax_year_is_rejected() {
    let statement = read_fixture("fifo");
    let converter = converter();
    let error = super::compute_tax_year(
        &statement,
        2027,
        &converter,
        &spain_config(SpanishTaxRegime::Gipuzkoa),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("2027"), "{error}");
}

/// Filing without a `taxes.spain` block must name the block rather than defaulting to a regime.
#[test]
fn missing_spain_config_is_rejected() {
    let statement = read_fixture("fifo");
    let converter = converter();
    let error = super::compute_tax_year(&statement, 2026, &converter, &TaxConfig::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("taxes.spain"), "{error}");
}

/// RCM income: an AAPL dividend of $1,000 with $300 US withholding (30%), $100 of broker interest
/// and a $50 custody fee, all at 0.9 EUR/USD → gross €900, withheld €270, interest €90, fee €45.
///
/// The treaty caps the creditable portion at 15% of the gross (€135), so €135 of the €270 withheld
/// is creditable in Spain and the rest has to be reclaimed from the IRS.
#[test]
fn rcm_income_is_reported_with_a_treaty_capped_credit_candidate() {
    let spain = run_pipeline("income", 2026, SpanishTaxRegime::Gipuzkoa);

    assert_eq!(spain.dividends.len(), 1);
    let dividend = &spain.dividends[0];
    assert_eq!(dividend.symbol, "AAPL");
    assert_eq!(dividend.date, Date::from_ymd_opt(2026, 5, 20).unwrap());
    assert_eq!(dividend.gross_eur, dec!(900));
    assert_eq!(dividend.withheld_eur, dec!(270));
    // 900 × 15%, well below the 270 actually withheld.
    assert_eq!(dividend.treaty_capped_credit, dec!(135));

    assert_eq!(spain.interest.len(), 1);
    assert_eq!(spain.interest[0].gross_eur, dec!(90));

    assert_eq!(spain.total_dividend_income, dec!(900));
    assert_eq!(spain.total_interest_income, dec!(90));
    assert_eq!(spain.total_foreign_withholding, dec!(270));
}

/// Fee deductibility is the sharpest split between the regimes. Gipuzkoa has no equivalent of LIRPF
/// art. 26.1.a — NF 3/2014 art. 39 is a closed list — so the custody fee is reported but changes
/// nothing; under Territorio Común the same €45 reduces the RCM result.
#[test]
fn custody_fee_deductibility_follows_the_regime() {
    let gipuzkoa = run_pipeline("income", 2026, SpanishTaxRegime::Gipuzkoa);
    assert_eq!(gipuzkoa.fees.len(), 1);
    assert!(!gipuzkoa.fees[0].deductible);
    assert_eq!(gipuzkoa.fees[0].amount_eur, dec!(45));
    assert!(gipuzkoa.fees[0].notes.as_deref().unwrap().contains("art. 39"));
    assert_eq!(gipuzkoa.total_deductible_fees, dec!(0));
    assert_eq!(gipuzkoa.total_informational_fees, dec!(45));
    // 900 dividend + 90 interest, nothing deducted.
    assert_eq!(gipuzkoa.rcm_net, dec!(990));

    let comun = run_pipeline("income", 2026, SpanishTaxRegime::Comun);
    assert!(comun.fees[0].deductible);
    assert!(comun.fees[0].notes.is_none());
    assert_eq!(comun.total_deductible_fees, dec!(45));
    assert_eq!(comun.total_informational_fees, dec!(0));
    assert_eq!(comun.rcm_net, dec!(945));

    // The regimes differ by exactly the fee, and by nothing else.
    assert_eq!(gipuzkoa.rcm_net - comun.rcm_net, dec!(45));
    assert_eq!(gipuzkoa.total_dividend_income, comun.total_dividend_income);
    assert_eq!(gipuzkoa.total_interest_income, comun.total_interest_income);
}

/// The savings base is the sum of the two groups' positive balances: neither reduces the other.
#[test]
fn rcm_and_gyp_enter_the_base_as_separate_groups() {
    let spain = run_pipeline("income", 2026, SpanishTaxRegime::Gipuzkoa);

    assert_eq!(spain.rcm_net, dec!(990));
    assert_eq!(spain.gyp_net, dec!(0));
    assert_eq!(spain.savings_base, dec!(990));
    // 990 sits entirely in the first bracket: 990 × 19%.
    assert_eq!(spain.savings_quota, dec!(188.10));
}

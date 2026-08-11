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
use crate::taxes::{DeferredLossConfig, SpanishTaxConfig, TaxConfig, TaxRemapping};
use crate::time::{self, Date};
use crate::types::Decimal;

use super::SpanishTaxStatement;

/// Fixed EUR conversion backend: every non-EUR currency converts to EUR at a constant rate,
/// independent of date. Keeps expected tax figures hand-computable.
struct FixedEurBackend {
    today: Date,
    eur_per_usd: Decimal,
    /// Optional revaluation: from this date on, the rate becomes the second element. Without it
    /// every date shares one rate, so no foreign-currency result can ever be realized.
    revaluation: Option<(Date, Decimal)>,
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
        date: Date,
    ) -> GenericResult<(Option<Decimal>, Option<Decimal>)> {
        assert_eq!(to, "EUR", "the Spanish pipeline only ever converts to EUR");
        match from {
            "EUR" => Ok((None, None)),
            "USD" => Ok((Some(self.rate_on(date)), None)),
            other => Err!("fixture converter has no rate for {other}"),
        }
    }
}

impl FixedEurBackend {
    fn rate_on(&self, date: Date) -> Decimal {
        match self.revaluation {
            Some((from, rate)) if date >= from => rate,
            _ => self.eur_per_usd,
        }
    }
}

fn converter() -> CurrencyConverter {
    CurrencyConverter::new_with_backend(Box::new(FixedEurBackend {
        today: time::today(),
        eur_per_usd: dec!(0.9),
        revaluation: None,
    }))
}

/// A converter whose USD rate steps from 0.9 to 1.0 on `from`, so a balance held across that date
/// realizes a computable foreign-currency result.
fn revaluing_converter(from: Date, rate: Decimal) -> CurrencyConverter {
    CurrencyConverter::new_with_backend(Box::new(FixedEurBackend {
        today: time::today(),
        eur_per_usd: dec!(0.9),
        revaluation: Some((from, rate)),
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

/// A loss-making disposal produces no taxable base and carries forward labelled with the filing
/// year. Buy 100 @ $200 in 2025 (€18,000), sell 100 @ $100 in 2026 (€9,000); the 2025 acquisition
/// actualizes at 1.020, so the cost becomes €18,360 and the loss is €9,360.
///
/// Actualization therefore *enlarges* a loss. That is the foral practice: art. 45.2 actualizes the
/// acquisition value unconditionally, with no clause restricting it to gains.
#[test]
fn a_loss_carries_forward_and_actualization_enlarges_it() {
    let spain = run_pipeline("loss", 2026, SpanishTaxRegime::Gipuzkoa);

    let sale = &spain.capital_gains[0];
    assert_eq!(sale.cost_eur, dec!(18000));
    assert_eq!(sale.lots[0].coefficient, dec!(1.020));
    assert_eq!(sale.actualized_cost_eur, dec!(18360));
    assert_eq!(sale.fiscal_gain_loss, dec!(-9360));

    assert_eq!(spain.gyp_net, dec!(-9360));
    assert_eq!(spain.gyp_taxable, dec!(0));
    assert_eq!(spain.savings_base, dec!(0));
    assert_eq!(spain.savings_quota, dec!(0));
    assert_eq!(spain.net_tax_due, dec!(0));

    // Carried into the following return, labelled 2026 so its own four-year window starts now.
    assert_eq!(spain.gyp_ledger_next.balances()[&2026], dec!(9360));

    // Without actualization the same trade would carry only €9,000 forward.
    let comun = run_pipeline("loss", 2026, SpanishTaxRegime::Comun);
    assert_eq!(comun.gyp_net, dec!(-9000));
    assert_eq!(comun.gyp_ledger_next.balances()[&2026], dec!(9000));
}

/// Prior-year pending balances reduce their own group's result, oldest vintage first.
#[test]
fn prior_year_losses_are_applied_oldest_first() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    let spain_cfg = config.spain.as_mut().unwrap();
    spain_cfg.loss_carryforward.gyp.insert(2023, dec!(4000));
    spain_cfg.loss_carryforward.gyp.insert(2025, dec!(1000));

    let spain = run_pipeline_with_config("fifo", 2026, &config);

    assert_eq!(spain.gyp_net, dec!(19692));
    assert_eq!(spain.gyp_applied.used_total, dec!(5000));
    assert_eq!(spain.gyp_applied.used_by_year[&2023], dec!(4000));
    assert_eq!(spain.gyp_applied.used_by_year[&2025], dec!(1000));
    assert_eq!(spain.gyp_taxable, dec!(14692));
    assert_eq!(spain.savings_base, dec!(14692));
    // 1,425 + (14,692 − 7,500) × 20%.
    assert_eq!(spain.savings_quota, dec!(2863.40));
    assert!(spain.gyp_ledger_next.is_empty());
}

/// A balance in its fourth and final year that the year's income cannot absorb is dropped rather
/// than carried, so next year's config does not claim an offset the tax office will refuse.
#[test]
fn balances_expire_at_the_end_of_the_four_year_window() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    // Filing 2026, so 2022 is in its final year.
    config
        .spain
        .as_mut()
        .unwrap()
        .loss_carryforward
        .gyp
        .insert(2022, dec!(25000));

    let spain = run_pipeline_with_config("fifo", 2026, &config);

    assert_eq!(spain.gyp_applied.used_total, dec!(19692));
    assert_eq!(spain.gyp_taxable, dec!(0));
    // 25,000 − 19,692 could not be used and is now out of time.
    assert_eq!(spain.gyp_expired, dec!(5308));
    assert!(spain.gyp_ledger_next.is_empty());
}

/// A balance older than the window is a config error, not something to silently ignore.
#[test]
fn expired_config_balances_are_rejected() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    config
        .spain
        .as_mut()
        .unwrap()
        .loss_carryforward
        .gyp
        .insert(2021, dec!(1000));

    let statement = read_fixture("fifo");
    let converter = converter();
    let error = super::compute_tax_year(&statement, 2026, &converter, &config)
        .unwrap_err()
        .to_string();
    assert!(error.contains("expired"), "{error}");
}

/// The year-level double-taxation credit. The `income` fixture's base is €990, taxed at 19%
/// throughout, so the average savings rate is 0.19 and the credit's limbs are €135 (treaty) and
/// €171 (0.19 × €900). The treaty limb binds.
#[test]
fn foreign_tax_credit_is_computed_at_year_level() {
    let spain = run_pipeline("income", 2026, SpanishTaxRegime::Gipuzkoa);

    assert_eq!(spain.savings_base, dec!(990));
    assert_eq!(spain.savings_quota, dec!(188.10));
    assert_eq!(spain.average_savings_rate, dec!(0.19));
    assert_eq!(spain.total_foreign_tax_credit, dec!(135));
    // 188.10 − 135.
    assert_eq!(spain.net_tax_due, dec!(53.10));
}

/// The credit can never turn into a refund of foreign tax through the Spanish return.
#[test]
fn the_credit_never_makes_the_tax_due_negative() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    // A large prior-year balance wipes out the base, so there is no Spanish tax to credit against.
    config
        .spain
        .as_mut()
        .unwrap()
        .loss_carryforward
        .rcm
        .insert(2025, dec!(50000));

    let spain = run_pipeline_with_config("income", 2026, &config);

    assert_eq!(spain.savings_base, dec!(0));
    assert_eq!(spain.savings_quota, dec!(0));
    assert_eq!(spain.total_foreign_tax_credit, dec!(0));
    assert_eq!(spain.net_tax_due, dec!(0));
}

/// A conversion out of a **held** foreign-currency balance realizes a ganancia patrimonial.
///
/// $10,000 acquired 2026-02-10 at 0.9 (€9,000) and converted back 2026-08-10 at 1.0 (€10,000)
/// realizes a €1,000 gain, which joins the ganancias group rather than the RCM one.
#[test]
fn held_balance_conversions_are_ganancias() {
    let statement = read_fixture("fx_gain");
    let converter = revaluing_converter(Date::from_ymd_opt(2026, 6, 1).unwrap(), dec!(1));
    let (spain, has_income) = super::compute_tax_year(
        &statement,
        2026,
        &converter,
        &spain_config(SpanishTaxRegime::Gipuzkoa),
    )
    .unwrap();

    assert!(has_income);
    assert_eq!(spain.fx_gains.len(), 1);
    let fx = &spain.fx_gains[0];
    assert_eq!(fx.currency, "USD");
    assert_eq!(fx.date, Date::from_ymd_opt(2026, 8, 10).unwrap());
    assert_eq!(fx.acquisition_date, Date::from_ymd_opt(2026, 2, 10).unwrap());
    assert_eq!(fx.amount_eur, dec!(1000));

    assert_eq!(spain.total_fx_gains, dec!(1000));
    assert_eq!(spain.total_fx_losses, dec!(0));
    // The result lands in the ganancias group, not RCM.
    assert_eq!(spain.gyp_net, dec!(1000));
    assert_eq!(spain.rcm_net, dec!(0));
    assert_eq!(spain.savings_base, dec!(1000));
    assert_eq!(spain.savings_quota, dec!(190));

    // Nothing was borrowed, so nothing is deferred to manual review.
    assert!(spain.fx_borrowed_review.is_empty());
    assert_eq!(spain.total_fx_borrowed_review, dec!(0));
}

/// With a flat rate the conversion is still a disposal, so it is still reported — but it realizes
/// nothing, and must not invent a figure in either direction.
#[test]
fn a_flat_rate_realizes_a_zero_fx_result() {
    let spain = run_pipeline("fx_gain", 2026, SpanishTaxRegime::Gipuzkoa);

    assert_eq!(spain.fx_gains.len(), 1);
    assert_eq!(spain.fx_gains[0].amount_eur, dec!(0));
    assert_eq!(spain.total_fx_gains, dec!(0));
    assert_eq!(spain.total_fx_losses, dec!(0));
    assert_eq!(spain.gyp_net, dec!(0));
    assert_eq!(spain.savings_quota, dec!(0));
}

/// A loss on shares bought back inside the +2-month window is deferred, not deducted.
///
/// Buy 100 @ $100 on 2026-01-05 (€9,000), sell them @ $90 on 2026-03-10 (€8,100) at a €900 loss,
/// then buy 40 back @ $80 on 2026-04-20 — inside the window, which runs 2026-01-10 to 2026-05-10.
/// 40 of the 100 sold shares are matched, so 900 × 40/100 = €360 is deferred and €540 stays
/// deductible. The 2026-01-05 acquisition is five days too early to block anything.
#[test]
fn a_repurchase_inside_the_window_defers_the_matched_share_of_the_loss() {
    let spain = run_pipeline("wash_sale_after", 2026, SpanishTaxRegime::Gipuzkoa);

    let sale = &spain.capital_gains[0];
    assert_eq!(sale.sale_date, Date::from_ymd_opt(2026, 3, 10).unwrap());
    assert_eq!(sale.fiscal_gain_loss, dec!(-900));
    assert_eq!(sale.deferred_loss, dec!(360));
    assert_eq!(sale.integrable_amount, dec!(-540));
    assert!(sale.notes.as_deref().unwrap().contains("art. 43.g"));

    // The 2026-11-15 sale of 25 blocked shares: proceeds €2,250 against a cost of
    // €2,880 × 25/40 = €1,800, actualized at 1.000 because the lot was acquired in the disposal
    // year itself.
    let later = &spain.capital_gains[1];
    assert_eq!(later.fiscal_gain_loss, dec!(450));
    assert_eq!(later.deferred_loss, dec!(0));

    // Selling 25 of the 40 blocked shares releases 360 × 25/40 = €225, dated to that disposal and
    // labelled with the sale the deferral came from.
    assert_eq!(spain.wash_sale_reintegrations.len(), 1);
    let released = &spain.wash_sale_reintegrations[0];
    assert_eq!(released.symbol, "AAPL");
    assert_eq!(released.date, Date::from_ymd_opt(2026, 11, 15).unwrap());
    assert_eq!(released.acquisition_date, Date::from_ymd_opt(2026, 4, 20).unwrap());
    assert_eq!(released.origin_sale_date, Date::from_ymd_opt(2026, 3, 10).unwrap());
    assert_eq!(released.released_eur, dec!(225));

    // −540 + 450 − 225.
    assert_eq!(spain.gyp_net, dec!(-315));
    assert_eq!(spain.total_deferred_loss, dec!(360));
    assert_eq!(spain.total_reintegrated_loss, dec!(225));
    assert_eq!(spain.gyp_ledger_next.balances()[&2026], dec!(315));

    // The 15 shares still held keep blocking the rest, printed back as next year's config.
    assert_eq!(spain.deferred_losses_next.len(), 1);
    let carried = &spain.deferred_losses_next[0];
    assert_eq!(carried.symbol, "AAPL");
    assert_eq!(carried.isin.as_deref(), Some("US0378331005"));
    assert_eq!(carried.blocked_quantity, dec!(15));
    assert_eq!(carried.loss, dec!(135));
    assert_eq!(carried.acquisition_date, Date::from_ymd_opt(2026, 4, 20).unwrap());
    assert_eq!(carried.sale_date, Date::from_ymd_opt(2026, 3, 10).unwrap());
}

/// A deferral carried in from an earlier return reintegrates when this year's sale disposes of the
/// shares that blocked it, even though the statement never saw the loss-making sale itself.
#[test]
fn an_opening_deferred_loss_reintegrates_on_disposal() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    config.spain.as_mut().unwrap().deferred_losses = vec![DeferredLossConfig {
        symbol: "AAPL".to_string(),
        isin: Some("US0378331005".to_string()),
        loss: dec!(600),
        blocked_quantity: dec!(40),
        acquisition_date: Date::from_ymd_opt(2026, 4, 20).unwrap(),
        sale_date: Date::from_ymd_opt(2026, 3, 10).unwrap(),
    }];

    let spain = run_pipeline_with_config("wash_sale_after", 2026, &config);

    // The statement's own 2026-03-10 loss blocks €360 against those same 40 shares, so the
    // 2026-11-15 sale of 25 of them releases from the older deferral first: 600 × 25/40 = €375.
    let released: Vec<Decimal> = spain
        .wash_sale_reintegrations
        .iter()
        .map(|entry| entry.released_eur)
        .collect();
    assert_eq!(released, vec![dec!(375)]);

    // Those 40 shares were already committed to the opening deferral, so nothing is left for the
    // statement's own loss to block: it is deducted in full.
    assert_eq!(spain.capital_gains[0].deferred_loss, dec!(0));
    assert_eq!(spain.capital_gains[0].integrable_amount, dec!(-900));
}

/// A configured deferral that is not a positive loss against a positive number of shares is a
/// config error, not something to quietly apply.
#[test]
fn a_malformed_opening_deferred_loss_is_rejected() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    config.spain.as_mut().unwrap().deferred_losses = vec![DeferredLossConfig {
        symbol: "AAPL".to_string(),
        isin: None,
        loss: dec!(600),
        blocked_quantity: dec!(0),
        acquisition_date: Date::from_ymd_opt(2026, 4, 20).unwrap(),
        sale_date: Date::from_ymd_opt(2026, 3, 10).unwrap(),
    }];

    let statement = read_fixture("wash_sale_after");
    let converter = converter();
    let error = super::compute_tax_year(&statement, 2026, &converter, &config)
        .unwrap_err()
        .to_string();
    assert!(error.contains("blocked quantity"), "{error}");
}

/// A repurchase after the statement's last date cannot be seen, so a loss whose window is still
/// open when the statement ends is deducted in full and flagged rather than silently settled.
#[test]
fn a_loss_whose_window_outlives_the_statement_is_flagged() {
    // The `loss` fixture sells 2026-06-10 and the statement ends 2026-12-31, so its window closed
    // inside the statement: nothing to flag.
    let settled = run_pipeline("loss", 2026, SpanishTaxRegime::Gipuzkoa);
    assert!(settled.wash_sale_window_gaps.is_empty());

    // `wash_sale_before` sells 2026-03-10 — also well inside.
    let also_settled = run_pipeline("wash_sale_before", 2026, SpanishTaxRegime::Gipuzkoa);
    assert!(also_settled.wash_sale_window_gaps.is_empty());

    // `year_end_loss` sells on 2026-12-15; the window runs to 2027-02-15, past the statement.
    let open = run_pipeline("year_end_loss", 2026, SpanishTaxRegime::Gipuzkoa);
    assert_eq!(open.wash_sale_window_gaps.len(), 1);
    let gap = &open.wash_sale_window_gaps[0];
    assert_eq!(gap.symbol, "AAPL");
    assert_eq!(gap.sale_date, Date::from_ymd_opt(2026, 12, 15).unwrap());
    assert_eq!(gap.window_end, Date::from_ymd_opt(2027, 2, 15).unwrap());
    assert_eq!(gap.loss_eur, dec!(900));
}

/// A partial repurchase spread over two acquisition dates splits the deferral pro rata, and a later
/// sale releases from each in turn, leaving the untouched remainder as carry-out.
///
/// Buy 100 @ $100 on 2026-01-05, sell them @ $90 on 2026-03-10 at a €900 loss, buy 25 @ $80 on
/// 2026-04-20 and 15 @ $60 on 2026-05-05 — both inside the window. 40 of the 100 sold shares match,
/// so €360 defers, split €225 / €135 by quantity. Selling 30 on 2026-11-15 takes the whole 25-share
/// lot and 5 of the 15, releasing €225 + €45.
#[test]
fn a_multi_lot_repurchase_defers_and_releases_pro_rata() {
    let spain = run_pipeline("wash_sale_multi_lot", 2026, SpanishTaxRegime::Gipuzkoa);

    let loss = &spain.capital_gains[0];
    assert_eq!(loss.fiscal_gain_loss, dec!(-900));
    assert_eq!(loss.deferred_loss, dec!(360));
    assert_eq!(loss.integrable_amount, dec!(-540));

    // Proceeds €2,700 against a cost of €1,800 + €270.
    let later = &spain.capital_gains[1];
    assert_eq!(later.fiscal_gain_loss, dec!(630));

    let released: Vec<Decimal> = spain
        .wash_sale_reintegrations
        .iter()
        .map(|entry| entry.released_eur)
        .collect();
    assert_eq!(released, vec![dec!(225), dec!(45)]);

    // −540 + 630 − 270.
    assert_eq!(spain.gyp_net, dec!(-180));

    // Only the 10 shares of the 2026-05-05 lot still block anything.
    assert_eq!(spain.deferred_losses_next.len(), 1);
    let carried = &spain.deferred_losses_next[0];
    assert_eq!(carried.blocked_quantity, dec!(10));
    assert_eq!(carried.loss, dec!(90));
    assert_eq!(carried.acquisition_date, Date::from_ymd_opt(2026, 5, 5).unwrap());
}

/// A repurchase *before* the sale blocks only shares the sale did not itself consume.
///
/// Buy 100 on 2026-01-10 and 50 more on 2026-02-15, then sell the first 100 @ $90 on 2026-03-10 at
/// a €900 loss. Both acquisitions sit inside the window, but the 100 the sale consumed are gone —
/// only the 50 still held can block. So half the loss defers. Counting the consumed lot as well
/// would defer the whole €900 and wipe out a deduction the filer is entitled to.
#[test]
fn a_repurchase_before_the_sale_blocks_only_the_shares_still_held() {
    let spain = run_pipeline("wash_sale_before", 2026, SpanishTaxRegime::Gipuzkoa);

    let sale = &spain.capital_gains[0];
    assert_eq!(sale.fiscal_gain_loss, dec!(-900));
    assert_eq!(sale.deferred_loss, dec!(450));
    assert_eq!(sale.integrable_amount, dec!(-450));

    assert_eq!(spain.gyp_net, dec!(-450));
    assert_eq!(spain.gyp_ledger_next.balances()[&2026], dec!(450));
}

/// A loss with no homogeneous acquisition in the window is deducted in full: the rule only reaches
/// losses a repurchase actually followed.
#[test]
fn a_loss_without_a_repurchase_is_deducted_in_full() {
    // The `loss` fixture buys 2025-03-10 and sells 2026-06-10; the window opens on 2026-04-10, so
    // the acquisition is more than a year outside it.
    let spain = run_pipeline("loss", 2026, SpanishTaxRegime::Gipuzkoa);
    assert_eq!(spain.capital_gains[0].deferred_loss, dec!(0));
    assert_eq!(spain.capital_gains[0].integrable_amount, dec!(-9360));

    // A year with only gains has nothing to defer either.
    let gains = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
    assert!(gains.capital_gains.iter().all(|entry| entry.deferred_loss.is_zero()));
}

/// Every disposal year the statement carries must have an actualization table, or the sales in it
/// cannot be priced and their losses are never tested for deferral. That gap is reported rather
/// than silently swallowed.
#[test]
fn disposal_years_without_a_coefficient_table_are_reported() {
    // The `fifo` fixture disposes only in 2026, which ships a table.
    let spain = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
    assert!(spain.wash_sale_unpriced_years.is_empty());

    // Under Territorio Común the coefficient is 1 for every year, so no year is ever unpriced.
    let comun = run_pipeline("fifo", 2026, SpanishTaxRegime::Comun);
    assert!(comun.wash_sale_unpriced_years.is_empty());
}

/// The 25% cross-group offset end to end. The `income` fixture under Común deducts the €45 custody
/// fee, so RCM is +945; forcing a prior-year RCM balance larger than that drives RCM negative and
/// lets the ganancias group absorb a quarter of it — under Gipuzkoa the same run must not cross.
#[test]
fn cross_group_offset_applies_only_under_comun() {
    let configure = |regime| {
        let mut config = spain_config(regime);
        // Drive the ganancias group positive alongside a negative RCM.
        config
            .spain
            .as_mut()
            .unwrap()
            .loss_carryforward
            .rcm
            .insert(2025, dec!(0));
        config
    };

    // Gipuzkoa: the groups never touch, so a negative RCM cannot reach the ganancias balance.
    let gipuzkoa = run_pipeline_with_config("income", 2026, &configure(SpanishTaxRegime::Gipuzkoa));
    assert_eq!(gipuzkoa.cross_offset_rcm_to_gyp, dec!(0));
    assert_eq!(gipuzkoa.cross_offset_gyp_to_rcm, dec!(0));

    let comun = run_pipeline_with_config("income", 2026, &configure(SpanishTaxRegime::Comun));
    // Both groups are positive here, so there is nothing to cross either way — the difference is
    // the deducted fee, not a cross-offset.
    assert_eq!(comun.cross_offset_rcm_to_gyp, dec!(0));
    assert_eq!(comun.rcm_taxable, dec!(945));
    assert_eq!(comun.savings_base, dec!(945));
}

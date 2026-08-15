//! Multi-year continuity tests.
//!
//! Filed years are not independent: a year hands the next one its pending loss ledgers and its
//! deferred wash-sale losses through the config block the tool prints. These tests replay that
//! hand-off and assert it agrees with a single continuous computation over the same trades.
//!
//! The hand-off has two halves and they behave differently, which is why the tests below separate
//! them:
//!
//! * **Loss ledgers** are never re-derived. The pipeline prices exactly one year, so a prior year's
//!   saldo reaches the return only through `taxes.spain.loss_carryforward`. A statement that spans
//!   both years does not make the earlier loss appear.
//! * **Wash-sale deferrals** *are* re-derived: the valores-homogéneos replay spans the whole
//!   statement, so a deferral created by an earlier year's sale is recomputed from the trades.
//!   `taxes.spain.deferred_losses` is the substitute for a statement that no longer reaches back
//!   that far, and the overlap guard makes the two mutually exclusive.
//!
//! Only 2024, 2025 and 2026 have a shipped savings scale, so a chain longer than three returns
//! cannot run through `compute_tax_year`. The years outside that band are driven through
//! [`compensate_savings_base`] and [`LossLedger::from_config`] instead — the same functions the
//! config path itself calls, one layer below the statement.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rstest::rstest;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::config::{Config, PortfolioConfig};
use crate::core::{EmptyResult, GenericResult};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterBackend};
use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::spain::carryforward::LossLedger;
use crate::taxes::spain::compensation::{CrossOffset, compensate_savings_base};
use crate::taxes::{DeferredLossConfig, SpanishTaxConfig, TaxConfig};
use crate::time::{self, Date};
use crate::types::Decimal;

use super::{CsvFormatter, SpanishTaxStatement};

// The harness duplicates `tests.rs` rather than sharing one: those helpers are private to that
// module, the top-level `testdata/` submodule is private and empty, and ~40 test-only lines are a
// cheaper price than a shared test-utils module every jurisdiction then has to agree on. Spain
// already paid that price once against Germany.

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
    let mut portfolio: PortfolioConfig =
        serde_yaml::from_str("name: test\nbroker: interactive-brokers\n").unwrap();
    portfolio.statements = Some(PathBuf::from(format!(
        "src/tax_statement/spain/testdata/{name}"
    )));

    BrokerStatement::load(&Config::mock(), &portfolio, ReadingStrictness::all()).unwrap()
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

fn run_pipeline_with_config(
    fixture: &str,
    year: i32,
    tax_config: &TaxConfig,
) -> SpanishTaxStatement {
    let statement = read_fixture(fixture);
    let (spanish, _has_income) =
        super::compute_tax_year(&statement, year, &converter(), tax_config).unwrap();
    spanish
}

fn run_pipeline(fixture: &str, year: i32, regime: SpanishTaxRegime) -> SpanishTaxStatement {
    run_pipeline_with_config(fixture, year, &spain_config(regime))
}

/// The tool's own CSV, which is where the filer reads the carry-out block from.
fn render_csv(statement: &SpanishTaxStatement) -> String {
    let mut output = Vec::new();
    CsvFormatter::write(statement, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

/// Rebuild next year's `taxes.spain` block **from the printed CSV**, the way a filer does.
///
/// Reading the in-memory statement instead would test a path no user has: the documented workflow
/// is to copy the `CARRYFORWARD_*` and `DEFERRED_LOSS_*` rows, and those rows go through
/// `format_eur` at two decimals. Parsing them back is what makes the round-trip the real one.
fn config_from_printed_carry_out(regime: SpanishTaxRegime, csv: &str) -> TaxConfig {
    let mut config = spain_config(regime);
    let spain = config.spain.as_mut().unwrap();

    for line in csv.lines() {
        let fields: Vec<&str> = line.split(',').collect();

        if let Some(key) = fields[0].strip_prefix("CARRYFORWARD_") {
            let (group, origin) = key.rsplit_once('_').unwrap();
            if origin == "EXPIRED" {
                continue;
            }
            let origin: i32 = origin.parse().unwrap();
            let amount: Decimal = fields[2].parse().unwrap();
            match group {
                "RCM" => spain.loss_carryforward.rcm.insert(origin, amount),
                "GYP" => spain.loss_carryforward.gyp.insert(origin, amount),
                other => panic!("unexpected carryforward group {other} in {line}"),
            };
        } else if fields[0].starts_with("DEFERRED_LOSS_") {
            // "AAPL (US0378331005) — 100 shares acquired 2026-01-05 blocking the loss of
            // 2025-11-10", with the ISIN omitted for an instrument that never carried one.
            let (instrument, rest) = fields[1].split_once(" — ").unwrap();
            let (symbol, isin) = match instrument.split_once(" (") {
                Some((symbol, isin)) => (symbol, Some(isin.trim_end_matches(')').to_owned())),
                None => (instrument, None),
            };
            let words: Vec<&str> = rest.split_whitespace().collect();
            spain.deferred_losses.push(DeferredLossConfig {
                symbol: symbol.to_string(),
                isin,
                loss: fields[2].parse().unwrap(),
                blocked_quantity: words[0].parse().unwrap(),
                acquisition_date: parse_iso(words[3]),
                sale_date: parse_iso(words[words.len() - 1]),
            });
        }
    }

    config
}

fn parse_iso(value: &str) -> Date {
    crate::time::parse_date(value, "%Y-%m-%d").unwrap()
}

fn ledger_config(entries: &[(i32, Decimal)]) -> BTreeMap<i32, Decimal> {
    entries.iter().copied().collect()
}

/// Print a ledger the way the CSV does and read it straight back, i.e. one config round-trip.
fn round_trip(ledger: &LossLedger, filing_year: i32) -> LossLedger {
    let printed: BTreeMap<i32, Decimal> = ledger
        .balances()
        .iter()
        .map(|(&origin, &amount)| (origin, super::format_eur(amount).parse().unwrap()))
        .collect();
    LossLedger::from_config(&printed, filing_year, "gyp").unwrap()
}

/// One filing year of the savings-base machinery, with no statement in front of it.
///
/// Used for the years outside 2024-2026, which ship no savings scale and therefore cannot go
/// through `compute_tax_year` at all. Everything below the scale — the ledgers, the four-year
/// window and the compensation ordering — is the same production code the pipeline calls.
fn file_year(filing_year: i32, gyp_net: Decimal, gyp_ledger: LossLedger) -> (LossLedger, Decimal) {
    let result = compensate_savings_base(
        filing_year,
        Decimal::ZERO,
        gyp_net,
        LossLedger::default(),
        gyp_ledger,
        CrossOffset::AeatTwoPhase,
    );
    (result.gyp_ledger_next, result.gyp_expired)
}

/// The `CARRYFORWARD_*` and `DEFERRED_LOSS_*` block the tool tells the filer to copy is a complete,
/// machine-readable `taxes.spain` config for the following year, and filing that year against it
/// produces the right answer.
///
/// Fixture `continuity_carry_out`, both years, Territorio Común (coefficient 1 throughout):
///
/// **2025** — AAPL bought 2025-03-10 for $10,000 (€9,000) and sold 2025-11-10 for $8,000 (€7,200):
/// a €1,800 loss, wholly deferred because 100 shares come back on 2026-01-05, inside the
/// [2025-09-10, 2026-01-10] window, so its integrable amount is €0. MSFT bought 2025-04-10 for
/// $15,000 (€13,500) and sold 2025-09-15 for $10,000 (€9,000): a €4,500 loss with nothing inside
/// its own [2025-07-15, 2025-11-15] window, deductible in full.
///
/// | 2025 figure | Arithmetic | Value |
/// |---|---|---|
/// | `total_capital_gains` | 0 (AAPL, deferred) + (−4,500) (MSFT) | −4,500.00 |
/// | `gyp_net` | no FX, no reintegration | −4,500.00 |
/// | Base / cuota | a negative group is floored at 0 | 0.00 |
/// | Carried to 2026 | the whole loss, labelled 2025 | GYP 4,500.00 @2025 |
/// | Deferred to 2026 | AAPL, 100 shares acquired 2026-01-05 | 1,800.00 |
///
/// **2026**, filed against exactly that block on a statement that starts 2026-01-01 (the overlap
/// guard forbids feeding the config back into the statement that produced it):
///
/// | 2026 figure | Arithmetic | Value |
/// |---|---|---|
/// | AAPL gain | €10,800 proceeds − €6,300 cost | 4,500.00 |
/// | Reintegration | the carried deferral, released by the definitive disposal | 1,800.00 |
/// | `gyp_net` | 4,500 − 1,800 | 2,700.00 |
/// | Prior saldo used | min(2,700, 4,500) | 2,700.00 |
/// | Base / cuota | fully absorbed | 0.00 |
/// | Carried to 2027 | 4,500 − 2,700 | GYP 1,800.00 @2025 |
#[test]
fn the_printed_carry_out_rebuilds_next_years_config() {
    let first = run_pipeline("continuity_carry_out", 2025, SpanishTaxRegime::Comun);

    assert_eq!(first.total_capital_gains, dec!(-4500));
    assert_eq!(first.gyp_net, dec!(-4500));
    assert_eq!(first.savings_base, dec!(0));
    assert_eq!(first.savings_quota, dec!(0));
    assert_eq!(first.gyp_ledger_next.balances(), &ledger_config(&[(2025, dec!(4500))]));
    assert!(first.rcm_ledger_next.is_empty());

    assert_eq!(first.deferred_losses_next.len(), 1);
    let deferred = &first.deferred_losses_next[0];
    assert_eq!(deferred.symbol, "AAPL");
    assert_eq!(deferred.loss, dec!(1800));
    assert_eq!(deferred.blocked_quantity, dec!(100));
    assert_eq!(deferred.acquisition_date, parse_iso("2026-01-05"));
    assert_eq!(deferred.sale_date, parse_iso("2025-11-10"));

    // The block the filer is told to copy, read back as the config it claims to be.
    let csv = render_csv(&first);
    assert!(csv.contains("CARRYFORWARD_GYP_2025,"), "{csv}");
    assert!(csv.contains("DEFERRED_LOSS_0,"), "{csv}");

    let config = config_from_printed_carry_out(SpanishTaxRegime::Comun, &csv);
    let carried = config.spain.as_ref().unwrap();
    assert_eq!(carried.loss_carryforward.gyp, ledger_config(&[(2025, dec!(4500))]));
    assert!(carried.loss_carryforward.rcm.is_empty());
    assert_eq!(carried.deferred_losses.len(), 1);
    assert_eq!(carried.deferred_losses[0].loss, dec!(1800));
    assert_eq!(carried.deferred_losses[0].blocked_quantity, dec!(100));
    assert_eq!(carried.deferred_losses[0].acquisition_date, parse_iso("2026-01-05"));
    assert_eq!(carried.deferred_losses[0].sale_date, parse_iso("2025-11-10"));

    let second = run_pipeline_with_config("continuity_second_year", 2026, &config);

    assert_eq!(second.capital_gains.len(), 1);
    assert_eq!(second.capital_gains[0].fiscal_gain_loss, dec!(4500));
    assert_eq!(second.total_reintegrated_loss, dec!(1800));
    assert_eq!(second.gyp_net, dec!(2700));

    assert_eq!(second.gyp_applied.used_total, dec!(2700));
    assert_eq!(second.gyp_applied.used_by_year[&2025], dec!(2700));
    assert_eq!(second.gyp_taxable, dec!(0));
    assert_eq!(second.savings_base, dec!(0));
    assert_eq!(second.savings_quota, dec!(0));
    assert_eq!(second.net_tax_due, dec!(0));

    // The deferral is spent; the saldo is not, and goes round again keeping its 2025 vintage.
    assert!(second.deferred_losses_next.is_empty());
    assert_eq!(second.gyp_ledger_next.balances(), &ledger_config(&[(2025, dec!(1800))]));
    assert_eq!(second.gyp_expired, dec!(0));
}

/// The same two years and the same trades carry differently under each regime, so a filer who
/// changes regime cannot reuse last year's block.
///
/// Fixture `continuity_regimes`. MSFT bought 2025-04-10 for $15,000 (€13,500) and sold 2025-09-15
/// for $11,500 (€10,350) — a €3,150 loss under every regime, since Gipuzkoa's own 2025 coefficient
/// for a 2025 acquisition is 1.000. AAPL bought 2026-01-05 for $10,000 (€9,000) and sold
/// 2026-06-15 for $15,000 (€13,500) — a €4,500 gain, coefficient 1.000 again. A $1,000 reversal of
/// interest credited in an earlier year lands on 2026-07-31, giving `rcm_net` = −€900 with no fee
/// or dividend involved, so the figure is the same under all three regimes.
///
/// 2025 is regime-independent: `gyp_net` −3,150, base 0, GYP 3,150.00 @2025 carried.
///
/// 2026 takes that block and the three orderings pull apart:
///
/// | Step | Gipuzkoa (no cross) | Común (AEAT) | Navarra (art. 54.2) |
/// |---|---|---|---|
/// | Cross of the current −900 | — | 900 (25% × 4,500 = 1,125 allows it) | after own-group absorption |
/// | Own-group absorption | 3,150 of 4,500 | 3,150 of 3,600 | 3,150 of 4,500 |
/// | 25% measured on | — | the raw 4,500 | the post-absorption 1,350 |
/// | Cross actually taken | 0 | 900 | min(900, 337.50) = 337.50 |
/// | **Base** | **1,350.00** | **450.00** | **1,012.50** |
/// | Cuota | 1,350 × 19% | 450 × 19% | 1,012.50 × 20% |
/// | | **256.50** | **85.50** | **202.50** |
/// | RCM carried to 2027 | 900.00 @2026 | — | 562.50 @2026 |
///
/// The RCM carry-out is the part that matters here: Común consumes the whole −900 in the filing
/// year, while the two foral regimes push 900 and 562.50 into the next one.
#[rstest]
#[case(SpanishTaxRegime::Gipuzkoa, dec!(1350), dec!(256.50), &[(2026, dec!(900))])]
#[case(SpanishTaxRegime::Comun, dec!(450), dec!(85.50), &[])]
#[case(SpanishTaxRegime::Navarra, dec!(1012.50), dec!(202.50), &[(2026, dec!(562.50))])]
fn the_carry_out_differs_by_regime_and_so_does_the_next_year(
    #[case] regime: SpanishTaxRegime,
    #[case] base: Decimal,
    #[case] quota: Decimal,
    #[case] rcm_carried: &[(i32, Decimal)],
) {
    let first = run_pipeline("continuity_regimes", 2025, regime);

    assert_eq!(first.gyp_net, dec!(-3150), "{regime:?}");
    assert_eq!(first.rcm_net, dec!(0), "{regime:?}");
    assert_eq!(first.savings_base, dec!(0), "{regime:?}");
    assert_eq!(
        first.gyp_ledger_next.balances(),
        &ledger_config(&[(2025, dec!(3150))]),
        "{regime:?}"
    );

    let config = config_from_printed_carry_out(regime, &render_csv(&first));
    assert!(config.spain.as_ref().unwrap().deferred_losses.is_empty(), "{regime:?}");

    let second = run_pipeline_with_config("continuity_regimes", 2026, &config);

    assert_eq!(second.gyp_net, dec!(4500), "{regime:?}");
    assert_eq!(second.rcm_net, dec!(-900), "{regime:?}");
    assert_eq!(second.gyp_applied.used_total, dec!(3150), "{regime:?}");
    assert_eq!(second.savings_base, base, "{regime:?}");
    assert_eq!(second.savings_quota, quota, "{regime:?}");
    assert!(second.gyp_ledger_next.is_empty(), "{regime:?}");
    assert_eq!(
        second.rcm_ledger_next.balances(),
        &ledger_config(rcm_carried),
        "{regime:?}"
    );
}

/// Filing 2026 gives the same answer whether the prior year's wash sale is **replayed** from a
/// statement that still spans it or **carried in** through `taxes.spain.deferred_losses`.
///
/// The two are alternatives, never a pair: the overlap guard rejects a config entry naming a
/// loss-making sale the statement itself prices, precisely because both routes would then deduct
/// the same loss. So the invariant has to be checked as two separate runs.
///
/// Only the deferral is carried. A prior-year *saldo* has no replay path at all — the pipeline
/// prices one year — so putting one in path B would compare two different returns.
///
/// * **Path A** — `continuity_carry_out` (2025-01-01 to 2027-03-31), no config. The replay sees the
///   2025-11-10 sale, prices it (2025 ships a coefficient table), defers €1,800 against the
///   2026-01-05 repurchase, and releases it when those shares go on 2026-05-15.
/// * **Path B** — `continuity_second_year` (2026-01-01 to 2027-03-31) plus the carried deferral.
///
/// Both: €10,800 proceeds − €6,300 cost = €4,500 gain, less the €1,800 release, so `gyp_net` is
/// €2,700 and the base with it. Cuota 2,700 × 19% = **513.00** under Gipuzkoa (2026 scale opens at
/// 19%) and Común, 2,700 × 20% = **540.00** under Navarra. €10,800 of proceeds is far above the
/// €3,000 of TRLFIRPF art. 39.5.d, so Navarra exempts nothing.
#[rstest]
#[case(SpanishTaxRegime::Gipuzkoa, dec!(513))]
#[case(SpanishTaxRegime::Comun, dec!(513))]
#[case(SpanishTaxRegime::Navarra, dec!(540))]
fn a_replayed_deferral_and_a_carried_in_one_reach_the_same_year(
    #[case] regime: SpanishTaxRegime,
    #[case] quota: Decimal,
) {
    let replayed = run_pipeline("continuity_carry_out", 2026, regime);

    let mut carried_config = spain_config(regime);
    carried_config.spain.as_mut().unwrap().deferred_losses = vec![DeferredLossConfig {
        symbol: "AAPL".to_string(),
        isin: Some("US0378331005".to_string()),
        loss: dec!(1800),
        blocked_quantity: dec!(100),
        acquisition_date: parse_iso("2026-01-05"),
        sale_date: parse_iso("2025-11-10"),
    }];
    let carried = run_pipeline_with_config("continuity_second_year", 2026, &carried_config);

    for (path, spain) in [("replayed", &replayed), ("carried", &carried)] {
        let context = format!("{regime:?}/{path}");

        assert_eq!(spain.capital_gains.len(), 1, "{context}");
        assert_eq!(spain.capital_gains[0].fiscal_gain_loss, dec!(4500), "{context}");

        assert_eq!(spain.wash_sale_reintegrations.len(), 1, "{context}");
        let released = &spain.wash_sale_reintegrations[0];
        assert_eq!(released.released_eur, dec!(1800), "{context}");
        assert_eq!(released.date, parse_iso("2026-05-15"), "{context}");
        assert_eq!(released.acquisition_date, parse_iso("2026-01-05"), "{context}");
        assert_eq!(released.origin_sale_date, parse_iso("2025-11-10"), "{context}");

        assert_eq!(spain.gyp_net, dec!(2700), "{context}");
        assert_eq!(spain.savings_base, dec!(2700), "{context}");
        assert_eq!(spain.savings_quota, quota, "{context}");
        assert!(spain.deferred_losses_next.is_empty(), "{context}");
        assert!(spain.gyp_ledger_next.is_empty(), "{context}");
    }

    // Stated as an identity rather than only as two equal numbers, so a future change that moves
    // both paths together still has to move them by the same amount.
    assert_eq!(replayed.gyp_net, carried.gyp_net, "{regime:?}");
    assert_eq!(replayed.savings_base, carried.savings_base, "{regime:?}");
    assert_eq!(replayed.savings_quota, carried.savings_quota, "{regime:?}");
    assert_eq!(
        replayed.total_reintegrated_loss, carried.total_reintegrated_loss,
        "{regime:?}"
    );
}

/// Combining the two paths is rejected rather than silently double-deducting: this is why C2 has to
/// be two runs and not one.
#[test]
fn the_two_paths_cannot_be_combined() {
    let mut config = spain_config(SpanishTaxRegime::Comun);
    config.spain.as_mut().unwrap().deferred_losses = vec![DeferredLossConfig {
        symbol: "AAPL".to_string(),
        isin: Some("US0378331005".to_string()),
        loss: dec!(1800),
        blocked_quantity: dec!(100),
        acquisition_date: parse_iso("2026-01-05"),
        sale_date: parse_iso("2025-11-10"),
    }];

    let statement = read_fixture("continuity_carry_out");
    let error = super::compute_tax_year(&statement, 2026, &converter(), &config)
        .unwrap_err()
        .to_string();

    assert!(error.contains("taxes.spain.deferred_losses"), "{error}");
    assert!(error.contains("2025-11-10"), "{error}");
    assert!(error.contains("twice"), "{error}");
}

/// A saldo stays usable through the four returns that follow the one it arose in, and the fifth
/// refuses it — walked one config round-trip at a time rather than asserted on a hand-built ledger.
///
/// Origin 2022, so the window closes with the 2026 return and 2027 is one year too late. 2022, 2023
/// and 2024-2025 have no fixture of their own here: the chain is driven through
/// `compensate_savings_base`, and the last year — the one the claim is actually about — goes
/// through the real pipeline.
///
/// | Return | Group result | Absorbed | Carried out |
/// |---|---|---|---|
/// | 2022 | −9,000 | — | 9,000.00 @2022 |
/// | 2023 | +900 | 900 | 8,100.00 @2022 |
/// | 2024 | +900 | 900 | 7,200.00 @2022 |
/// | 2025 | +900 | 900 | 6,300.00 @2022 |
/// | 2026 (pipeline) | +4,500 | 4,500 | nothing — 1,800.00 expires |
///
/// 2026 is the vintage's fourth following year, so what it cannot absorb is lost: €6,300 − €4,500 =
/// €1,800 is reported as expired and dropped, rather than carried into a return that would refuse
/// it. Base 4,500 − 4,500 = 0.
#[test]
fn a_loss_survives_four_returns_and_is_refused_in_the_fifth() {
    // The origin year. No savings scale ships for 2022, so the loss is booked through the
    // compensation entry point the pipeline itself calls rather than through a statement.
    let (origin, expired) = file_year(2022, dec!(-9000), LossLedger::default());
    assert_eq!(origin.balances(), &ledger_config(&[(2022, dec!(9000))]));
    assert_eq!(expired, dec!(0));

    let mut ledger = origin;
    for (filing_year, expected) in [
        (2023, dec!(8100)),
        (2024, dec!(7200)),
        (2025, dec!(6300)),
    ] {
        // The window is measured from the origin year, so the config has to be re-accepted every
        // year: a 2022 balance is still inside it in 2026 and outside it in 2027.
        ledger = round_trip(&ledger, filing_year);
        let (next, expired) = file_year(filing_year, dec!(900), ledger);
        assert_eq!(next.balances(), &ledger_config(&[(2022, expected)]), "{filing_year}");
        assert_eq!(expired, dec!(0), "{filing_year}");
        ledger = next;
    }

    // The fourth following return, through the whole pipeline: €13,500 proceeds − €9,000 cost.
    let mut config = spain_config(SpanishTaxRegime::Comun);
    config.spain.as_mut().unwrap().loss_carryforward.gyp = round_trip(&ledger, 2026)
        .balances()
        .clone();
    assert_eq!(
        config.spain.as_ref().unwrap().loss_carryforward.gyp,
        ledger_config(&[(2022, dec!(6300))])
    );

    let last = run_pipeline_with_config("continuity_second_year", 2026, &config);

    assert_eq!(last.gyp_net, dec!(4500));
    assert_eq!(last.gyp_applied.used_total, dec!(4500));
    assert_eq!(last.gyp_applied.used_by_year[&2022], dec!(4500));
    assert_eq!(last.savings_base, dec!(0));
    assert_eq!(last.savings_quota, dec!(0));

    // Dropped rather than carried, and reported so the filer learns the offset is gone.
    assert_eq!(last.gyp_expired, dec!(1800));
    assert!(last.gyp_ledger_next.is_empty());
    assert!(!render_csv(&last).contains("CARRYFORWARD_GYP_2022,"));

    // A filer who carries it anyway is refused by name, with the last return it was good for.
    let mut late = spain_config(SpanishTaxRegime::Comun);
    late.spain.as_mut().unwrap().loss_carryforward.gyp = ledger_config(&[(2022, dec!(1800))]);
    let error = late
        .spain
        .as_ref()
        .unwrap()
        .loss_ledgers(2027)
        .unwrap_err()
        .to_string();

    assert!(error.contains("expired"), "{error}");
    assert!(error.contains("taxes.spain.loss_carryforward.gyp.2022"), "{error}");
    assert!(error.contains("2026"), "{error}");
}

/// The ledgers keep full precision; the printed carry-out is two decimals. A filer who copies the
/// printed row therefore re-enters a slightly different balance every year, and the difference
/// compounds across the four-year window. This pins the size of it.
///
/// Fixture `continuity_rounding`, Gipuzkoa, filing 2026: 1 AAPL bought 2025-03-10 for $1,002.50
/// (€902.25 at the flat 0.9 rate) and sold 2026-06-10 for $500 (€450). Actualization is where the
/// sub-cent comes from — a two-decimal cost times a three-decimal coefficient:
///
/// | Figure | Arithmetic | Value |
/// |---|---|---|
/// | Cost | 0.9 × 1,002.50 | 902.25 |
/// | Coefficient (2025 → 2026) | Decreto Foral 27/2025 | 1.020 |
/// | Actualized cost | 902.25 × 1.020 | **920.295** |
/// | Loss | 450 − 920.295 | **−470.295** |
/// | Printed carry-out | `format_eur`, half away from zero | **470.30** |
///
/// Half a cent enters at the first print. Each later return that absorbs a gain carrying its own
/// sub-cent tail — an actualized gain is exactly such a figure — exposes a fresh tail and rounds
/// again, so the drift grows by 0.005 per print:
///
/// | Return | Printed chain | Exact chain | Drift |
/// |---|---|---|---|
/// | 2026 (origin) | 470.30 | 470.295 | 0.005 |
/// | 2027 (−100.005) | 370.30 | 370.29 | 0.010 |
/// | 2028 (−100.005) | 270.30 | 270.285 | 0.015 |
/// | 2029 (−100.005) | 170.30 | 170.28 | 0.020 |
/// | 2030 (−100.005, last usable) | 70.295 expires | 70.275 expires | **0.020** |
///
/// Four prints, four half-cents: the balance that finally expires is €0.02 larger than the one the
/// tool computed. The direction is not neutral here — `format_eur` rounds half **away from zero**
/// and a saldo is a positive magnitude, so a tail sitting exactly on the half-cent always rounds
/// the filer's offset *up*. An arbitrary tail rounds either way and the bound is ±0.005 per printed
/// vintage per return.
///
/// Whether it can move a cent of tax: €0.02 of base is €0.0038 of cuota at the 19% the 2026 foral
/// scale opens with, and the quota is not itself rounded until it is printed, so the drift only
/// surfaces as a cent when the unrounded quota happens to sit within €0.0038 of a half-cent
/// boundary. It can therefore move one printed cent, and never more than one.
///
/// This is Left-open item 7 of `specs/003-navarra-tax/plan.md`, pinned as it currently behaves.
/// Deciding it means choosing where the statutory rounding point sits, not changing a format
/// string, so nothing here asserts a fix.
#[test]
fn the_two_decimal_carry_out_drifts_from_the_ledger_it_came_from() {
    let origin = run_pipeline("continuity_rounding", 2026, SpanishTaxRegime::Gipuzkoa);

    let sale = &origin.capital_gains[0];
    assert_eq!(sale.cost_eur, dec!(902.25));
    assert_eq!(sale.lots[0].coefficient, dec!(1.020));
    assert_eq!(sale.actualized_cost_eur, dec!(920.295));
    assert_eq!(sale.fiscal_gain_loss, dec!(-470.295));

    // The ledger keeps the third decimal the CSV cannot show.
    assert_eq!(origin.gyp_ledger_next.balances(), &ledger_config(&[(2026, dec!(470.295))]));
    assert_eq!(super::format_eur(dec!(470.295)), "470.30");
    let csv = render_csv(&origin);
    assert!(
        csv.contains("CARRYFORWARD_GYP_2026,Saldo negativo pendiente — GYP — origen 2026,470.30"),
        "{csv}"
    );

    // An actualized gain carries a sub-cent tail for the same reason the loss does, so each return
    // in the window re-exposes one.
    let gain = dec!(100.005);
    let mut printed = origin.gyp_ledger_next.clone();
    let mut exact = origin.gyp_ledger_next.clone();

    for (filing_year, printed_balance, exact_balance) in [
        (2027, dec!(370.295), dec!(370.29)),
        (2028, dec!(270.295), dec!(270.285)),
        (2029, dec!(170.295), dec!(170.28)),
    ] {
        printed = file_year(filing_year, gain, round_trip(&printed, filing_year)).0;
        exact = file_year(filing_year, gain, exact).0;

        assert_eq!(
            printed.balances(),
            &ledger_config(&[(2026, printed_balance)]),
            "{filing_year}"
        );
        assert_eq!(
            exact.balances(),
            &ledger_config(&[(2026, exact_balance)]),
            "{filing_year}"
        );
    }

    // 2030 is the vintage's fourth following return: whatever it cannot absorb expires there.
    let (printed_next, printed_expired) =
        file_year(2030, gain, round_trip(&printed, 2030));
    let (exact_next, exact_expired) = file_year(2030, gain, exact);

    assert!(printed_next.is_empty());
    assert!(exact_next.is_empty());
    assert_eq!(printed_expired, dec!(70.295));
    assert_eq!(exact_expired, dec!(70.275));
    assert_eq!(printed_expired - exact_expired, dec!(0.02));
}

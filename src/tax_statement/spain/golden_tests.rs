//! Golden regression tests for both emitted artefacts: the CSV statement and the printable report.
//!
//! Pins each of them, byte for byte, for a set of committed fixtures under every regime, so any
//! unintended change to a value, a label, a row order or a banner shows up as a diff in
//! `cargo test` rather than in a filer's return.
//!
//! Every golden under `testdata/golden/` is produced by this file from a committed synthetic
//! fixture under `testdata/`, through the same `compute_tax_year` → writer path the binary uses.
//! Nothing here reads a real broker statement, so the corpus is safe to commit.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests`; see
//! [`crate::tax_statement::golden`].

use std::path::PathBuf;

use rstest::rstest;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::config::{Config, PortfolioConfig};
use crate::core::{EmptyResult, GenericResult};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterBackend};
use crate::tax_statement::golden::GoldenCorpus;
use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::{SpanishTaxConfig, TaxConfig};
use crate::time::{self, Date, DateTime, Period};
use crate::types::Decimal;

use super::{CsvFormatter, HtmlReport, ReportMeta};

/// Fixed EUR conversion backend: every non-EUR currency converts to EUR at 0.9, independent of
/// date. Duplicated from the sibling `tests` module, which keeps it private; ~20 test-only lines
/// are cheaper than a shared test-utils module both modules would then have to agree on.
struct FixedEurBackend {
    today: Date,
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
            _ => dec!(0.9),
        }
    }
}

fn converter() -> CurrencyConverter {
    CurrencyConverter::new_with_backend(Box::new(FixedEurBackend {
        today: time::today(),
        revaluation: None,
    }))
}

/// A converter whose USD rate steps from 0.9 to 1.0 on 1 June 2026, so a balance held across that
/// date realizes a computable foreign-currency result.
fn revaluing_converter() -> CurrencyConverter {
    CurrencyConverter::new_with_backend(Box::new(FixedEurBackend {
        today: time::today(),
        revaluation: Some((Date::from_ymd_opt(2026, 6, 1).unwrap(), dec!(1))),
    }))
}

fn read_fixture(name: &str) -> BrokerStatement {
    // The fixture path is repo-relative, so it is assigned after deserialization: the config
    // deserializer requires an absolute path, which a checked-out test tree cannot provide.
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

/// Per-case config hook. Most cases file with empty ledgers; the compensation cases open one.
type Configure = fn(&mut SpanishTaxConfig);

fn no_opening_balances(_config: &mut SpanishTaxConfig) {}

/// A €2,000 RCM saldo from 2025 — enough for the two compensation orderings to separate.
fn rcm_saldo_2000(config: &mut SpanishTaxConfig) {
    config.loss_carryforward.rcm.insert(2025, dec!(2000));
}

/// An €8,000 RCM saldo from 2025 against a year with no RCM income of its own, so under Navarra a
/// *carried* saldo is what crosses and the open-reading warning fires.
fn rcm_saldo_8000(config: &mut SpanishTaxConfig) {
    config.loss_carryforward.rcm.insert(2025, dec!(8000));
}

fn corpus() -> GoldenCorpus {
    GoldenCorpus::new(
        "src/tax_statement/spain/testdata/golden",
        "UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests",
    )
}

/// The corpus. Each case is one committed golden; the regimes listed against a fixture are the ones
/// that make it say something different, not every regime the tool supports.
///
/// - `fifo` — per-lot actualization coefficients (Gipuzkoa 1.212 / 1.050) against Común's and
///   Navarra's unit coefficient, and the three regimes' Modelo 109 / 100 / F-93 box blocks.
/// - `income` — dividends, broker interest, foreign withholding and its credit, and the custody fee
///   under all three deduction rules (Gipuzkoa exempts the dividend and deducts nothing; Común
///   deducts in full; Navarra deducts up to the 3% ceiling).
/// - `wash_sale_multi_lot` — a deferral split over two repurchase lots, two reintegration rows, and
///   the carry-out block for the shares still blocked on 31 December.
/// - `cross_offset` — the compensation block with a prior-year saldo, under the two orderings that
///   cross groups; the Común and Navarra rows differ in both label and euro.
/// - `small_disposal` / `small_disposal_mixed` — the Navarra €3,000 exemption, including the case
///   where the year's loss must survive it untouched.
#[rstest]
#[case::fifo_gipuzkoa(
    "fifo_gipuzkoa_2026",
    "fifo",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances
)]
#[case::fifo_comun(
    "fifo_comun_2026",
    "fifo",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances
)]
#[case::fifo_navarra_carried_saldo(
    "fifo_navarra_2026_carried_saldo",
    "fifo",
    2026,
    SpanishTaxRegime::Navarra,
    rcm_saldo_8000
)]
#[case::income_gipuzkoa(
    "income_gipuzkoa_2026",
    "income",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances
)]
#[case::income_comun(
    "income_comun_2026",
    "income",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances
)]
#[case::income_navarra(
    "income_navarra_2026",
    "income",
    2026,
    SpanishTaxRegime::Navarra,
    no_opening_balances
)]
#[case::wash_sale_multi_lot_gipuzkoa(
    "wash_sale_multi_lot_gipuzkoa_2026",
    "wash_sale_multi_lot",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances
)]
#[case::cross_offset_comun(
    "cross_offset_comun_2026_saldo",
    "cross_offset",
    2026,
    SpanishTaxRegime::Comun,
    rcm_saldo_2000
)]
#[case::cross_offset_navarra(
    "cross_offset_navarra_2026_saldo",
    "cross_offset",
    2026,
    SpanishTaxRegime::Navarra,
    rcm_saldo_2000
)]
#[case::small_disposal_navarra(
    "small_disposal_navarra_2026",
    "small_disposal",
    2026,
    SpanishTaxRegime::Navarra,
    no_opening_balances
)]
#[case::small_disposal_comun(
    "small_disposal_comun_2026",
    "small_disposal",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances
)]
#[case::small_disposal_mixed_navarra(
    "small_disposal_mixed_navarra_2026",
    "small_disposal_mixed",
    2026,
    SpanishTaxRegime::Navarra,
    no_opening_balances
)]
fn emitted_statement_matches_its_golden(
    #[case] golden: &str,
    #[case] fixture: &str,
    #[case] year: i32,
    #[case] regime: SpanishTaxRegime,
    #[case] configure: Configure,
) {
    let mut config = spain_config(regime);
    configure(config.spain.as_mut().unwrap());

    let statement = read_fixture(fixture);
    let (spanish, has_income) =
        super::compute_tax_year(&statement, year, &converter(), &config).unwrap();
    assert!(
        has_income,
        "golden `{golden}` covers a year the pipeline reports as empty, so the file it pins would \
         never be written by the binary"
    );

    let mut emitted = Vec::new();
    CsvFormatter::write(&spanish, &mut emitted).unwrap();
    let emitted = String::from_utf8(emitted).expect("the statement must be valid UTF-8");

    corpus().assert(golden, "csv", &emitted);
}

/// A meta block with a constant `generated_at`, so the rendered report is reproducible. Everything
/// else in it is fixed too: the renderer is a pure function of the statement and this block.
fn fixed_meta(year: i32) -> ReportMeta {
    ReportMeta {
        year,
        broker_name: "Interactive Brokers LLC".to_owned(),
        portfolio_name: "ib".to_owned(),
        account_id: Some("U1234567".to_owned()),
        period: Period::new(
            Date::from_ymd_opt(year, 1, 1).unwrap(),
            Date::from_ymd_opt(year, 12, 31).unwrap(),
        )
        .unwrap(),
        generated_at: DateTime::new(
            Date::from_ymd_opt(2026, 1, 15).unwrap(),
            crate::time::Time::from_hms_opt(12, 0, 0).unwrap(),
        ),
    }
}

/// The rendered report, byte for byte, over four of the fixtures above: one per regime, plus the
/// deferral case whose section exists in no other.
///
/// - `fifo_gipuzkoa_2026` — the FIFO worksheets with their per-lot actualization coefficients.
/// - `income_comun_2026` — the cash bookings, the withholding table and the credit.
/// - `cross_offset_navarra_2026_saldo` — the compensation ledgers and the conditional F-93 boxes.
/// - `wash_sale_multi_lot_gipuzkoa_2026` — the valores-homogéneos section and the carry-out block.
/// - `fx_gain_gipuzkoa_2026` — the per-currency ledger, the widest table in the report and the one
///   no other case reaches.
#[rstest]
#[case::fifo_gipuzkoa(
    "fifo_gipuzkoa_2026",
    "fifo",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    converter
)]
#[case::income_comun(
    "income_comun_2026",
    "income",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances,
    converter
)]
#[case::cross_offset_navarra(
    "cross_offset_navarra_2026_saldo",
    "cross_offset",
    2026,
    SpanishTaxRegime::Navarra,
    rcm_saldo_2000,
    converter
)]
#[case::wash_sale_multi_lot_gipuzkoa(
    "wash_sale_multi_lot_gipuzkoa_2026",
    "wash_sale_multi_lot",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    converter
)]
#[case::fx_gain_gipuzkoa(
    "fx_gain_gipuzkoa_2026",
    "fx_gain",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    revaluing_converter
)]
fn rendered_report_matches_its_golden(
    #[case] golden: &str,
    #[case] fixture: &str,
    #[case] year: i32,
    #[case] regime: SpanishTaxRegime,
    #[case] configure: Configure,
    #[case] rates: fn() -> CurrencyConverter,
) {
    let mut config = spain_config(regime);
    configure(config.spain.as_mut().unwrap());

    let statement = read_fixture(fixture);
    let (spanish, _has_income) =
        super::compute_tax_year(&statement, year, &rates(), &config).unwrap();

    let mut emitted = Vec::new();
    HtmlReport::write(&spanish, &fixed_meta(year), &mut emitted).unwrap();
    let emitted = String::from_utf8(emitted).expect("the report must be valid UTF-8");

    corpus().assert(golden, "html", &emitted);
}

#[test]
fn every_golden_file_belongs_to_a_case() {
    let corpus = corpus();
    let claimed: Vec<PathBuf> = CORPUS
        .iter()
        .map(|name| corpus.path(name, "csv"))
        .chain(REPORT_CORPUS.iter().map(|name| corpus.path(name, "html")))
        .collect();
    corpus.assert_no_orphans(&claimed, &["csv", "html"]);
}

/// The file stems of the report cases above, in case order.
const REPORT_CORPUS: [&str; 5] = [
    "fifo_gipuzkoa_2026",
    "income_comun_2026",
    "cross_offset_navarra_2026_saldo",
    "wash_sale_multi_lot_gipuzkoa_2026",
    "fx_gain_gipuzkoa_2026",
];

/// The file stems of every case above, in case order. Kept beside the `#[case]` list rather than
/// derived from it: `rstest` does not expose its cases to another test.
const CORPUS: [&str; 12] = [
    "fifo_gipuzkoa_2026",
    "fifo_comun_2026",
    "fifo_navarra_2026_carried_saldo",
    "income_gipuzkoa_2026",
    "income_comun_2026",
    "income_navarra_2026",
    "wash_sale_multi_lot_gipuzkoa_2026",
    "cross_offset_comun_2026_saldo",
    "cross_offset_navarra_2026_saldo",
    "small_disposal_navarra_2026",
    "small_disposal_comun_2026",
    "small_disposal_mixed_navarra_2026",
];

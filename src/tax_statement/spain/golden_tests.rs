//! Golden-CSV regression tests.
//!
//! Pins the complete emitted statement, byte for byte, for a set of committed fixtures under every
//! regime, so any unintended change to a value, a label, a row order or a banner shows up as a diff
//! in `cargo test` rather than in a filer's return.
//!
//! Every golden under `testdata/golden/` is produced by this file from a committed synthetic
//! fixture under `testdata/`, through the same `compute_tax_year` → `CsvFormatter::write` path the
//! binary uses. Nothing here reads a real broker statement, so the corpus is safe to commit.
//!
//! # Regenerating
//!
//! When a change to the emitted statement is **intentional**, rewrite the corpus with:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests
//! ```
//!
//! then read the resulting `git diff` line by line: that diff is the whole point of the corpus, and
//! a golden nobody reviewed is worth no more than no golden at all. A plain `cargo test` never
//! writes to `testdata/golden/`.

use std::path::PathBuf;

use rstest::rstest;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::config::{Config, PortfolioConfig};
use crate::core::{EmptyResult, GenericResult};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterBackend};
use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::{SpanishTaxConfig, TaxConfig};
use crate::time::{self, Date};
use crate::types::Decimal;

use super::CsvFormatter;

/// Fixed EUR conversion backend: every non-EUR currency converts to EUR at 0.9, independent of
/// date. Duplicated from the sibling `tests` module, which keeps it private; ~20 test-only lines
/// are cheaper than a shared test-utils module both modules would then have to agree on.
struct FixedEurBackend {
    today: Date,
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
            "USD" => Ok((Some(dec!(0.9)), None)),
            other => Err!("fixture converter has no rate for {other}"),
        }
    }
}

fn converter() -> CurrencyConverter {
    CurrencyConverter::new_with_backend(Box::new(FixedEurBackend { today: time::today() }))
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

fn golden_dir() -> PathBuf {
    PathBuf::from("src/tax_statement/spain/testdata/golden")
}

fn golden_path(name: &str) -> PathBuf {
    golden_dir().join(format!("{name}.csv"))
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
    "fifo_gipuzkoa_2026", "fifo", 2026, SpanishTaxRegime::Gipuzkoa, no_opening_balances)]
#[case::fifo_comun(
    "fifo_comun_2026", "fifo", 2026, SpanishTaxRegime::Comun, no_opening_balances)]
#[case::fifo_navarra_carried_saldo(
    "fifo_navarra_2026_carried_saldo", "fifo", 2026, SpanishTaxRegime::Navarra, rcm_saldo_8000)]
#[case::income_gipuzkoa(
    "income_gipuzkoa_2026", "income", 2026, SpanishTaxRegime::Gipuzkoa, no_opening_balances)]
#[case::income_comun(
    "income_comun_2026", "income", 2026, SpanishTaxRegime::Comun, no_opening_balances)]
#[case::income_navarra(
    "income_navarra_2026", "income", 2026, SpanishTaxRegime::Navarra, no_opening_balances)]
#[case::wash_sale_multi_lot_gipuzkoa(
    "wash_sale_multi_lot_gipuzkoa_2026", "wash_sale_multi_lot", 2026,
    SpanishTaxRegime::Gipuzkoa, no_opening_balances)]
#[case::cross_offset_comun(
    "cross_offset_comun_2026_saldo", "cross_offset", 2026, SpanishTaxRegime::Comun, rcm_saldo_2000)]
#[case::cross_offset_navarra(
    "cross_offset_navarra_2026_saldo", "cross_offset", 2026,
    SpanishTaxRegime::Navarra, rcm_saldo_2000)]
#[case::small_disposal_navarra(
    "small_disposal_navarra_2026", "small_disposal", 2026,
    SpanishTaxRegime::Navarra, no_opening_balances)]
#[case::small_disposal_comun(
    "small_disposal_comun_2026", "small_disposal", 2026,
    SpanishTaxRegime::Comun, no_opening_balances)]
#[case::small_disposal_mixed_navarra(
    "small_disposal_mixed_navarra_2026", "small_disposal_mixed", 2026,
    SpanishTaxRegime::Navarra, no_opening_balances)]
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

    assert_golden(golden, &emitted);
}

/// A golden that no case claims is a golden nobody checks. Catches a renamed case leaving its file
/// behind, and a hand-added file that never had a test.
#[test]
fn every_golden_file_belongs_to_a_case() {
    let claimed: Vec<PathBuf> = CORPUS.iter().map(|name| golden_path(name)).collect();

    let mut orphans = Vec::new();
    for entry in std::fs::read_dir(golden_dir()).expect("the golden corpus directory must exist") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "csv") && !claimed.contains(&path)
        {
            orphans.push(path.display().to_string());
        }
    }
    orphans.sort();

    assert!(
        orphans.is_empty(),
        "golden files with no case in `emitted_statement_matches_its_golden`: {}. Delete them or \
         add the case they were written for.",
        orphans.join(", ")
    );
}

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

/// Compare against the committed golden, or rewrite it under `UPDATE_GOLDEN`.
fn assert_golden(name: &str, emitted: &str) {
    let path = golden_path(name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, emitted)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read the golden {}: {error}. If this case is new, create it with \
             `UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests`, then read the file before \
             committing it.",
            path.display()
        )
    });

    if expected == emitted {
        return;
    }

    panic!("{}", describe_difference(name, &expected, emitted));
}

/// The first differing line, in context. A whole-file dump of two 60-line statements tells a reader
/// nothing they can act on; the line number and its neighbours do.
fn describe_difference(name: &str, expected: &str, emitted: &str) -> String {
    const CONTEXT: usize = 3;

    let expected_lines: Vec<&str> = expected.lines().collect();
    let emitted_lines: Vec<&str> = emitted.lines().collect();

    let first_difference = expected_lines
        .iter()
        .zip(&emitted_lines)
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| expected_lines.len().min(emitted_lines.len()));

    let mut report = format!(
        "golden `{name}` no longer matches the emitted statement.\n  \
         file: {}\n  \
         first difference at line {} (golden has {} lines, the statement has {})\n\n",
        golden_path(name).display(),
        first_difference + 1,
        expected_lines.len(),
        emitted_lines.len(),
    );

    let start = first_difference.saturating_sub(CONTEXT);
    for (offset, line) in expected_lines[start..first_difference].iter().enumerate() {
        report += &format!("  {:>4} | {line}\n", start + offset + 1);
    }

    match expected_lines.get(first_difference) {
        Some(line) => report += &format!("- {:>4} | {line}\n", first_difference + 1),
        None => report += "-      | <the golden ends here>\n",
    }
    match emitted_lines.get(first_difference) {
        Some(line) => report += &format!("+ {:>4} | {line}\n", first_difference + 1),
        None => report += "+      | <the statement ends here>\n",
    }

    let tail_start = first_difference + 1;
    let tail_end = expected_lines.len().min(tail_start + CONTEXT);
    for (offset, line) in expected_lines[tail_start.min(tail_end)..tail_end].iter().enumerate() {
        report += &format!("  {:>4} | {line}\n", tail_start + offset + 1);
    }

    report += "\nIf the change is intended, regenerate with \
               `UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests` and review the diff.";
    report
}

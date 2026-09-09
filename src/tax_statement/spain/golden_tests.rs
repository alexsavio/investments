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
use crate::instruments::EtfClassification;
use crate::tax_statement::golden::GoldenCorpus;
use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::{DeferredLossConfig, SpanishTaxConfig, TaxConfig};
use crate::time::{self, Date, DateTime, Period};
use crate::types::Decimal;

use super::report::{BookingKind, BookingRow, LotSource, OpenLotRow};
use super::statement::{CashGrantEntry, SpanishTaxStatement};
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

/// Per-case classification hook, for the report corpus only. The asset class reads
/// `taxes.etf_classification`, an ISIN map that is not a Spanish key: no figure in the savings base
/// turns on it, but the report groups by it, and the €1,500 exemption's caveat names the payers it
/// marks as funds.
type Classify = fn(&mut TaxConfig);

fn everything_is_a_share(_config: &mut TaxConfig) {}

/// The fixture's *exempted* payer, classified as an equity fund; the other payer stays a share.
///
/// NF 3/2014 art. 9.24 does not reach distributions from instituciones de inversión colectiva, and
/// a broker statement cannot tell one from a company dividend — so the report names the payers it
/// has been told are funds. Only a payer the exemption actually reached is worth naming, which is
/// this one: the fixture's other dividend is already out on the anti-abuse clause.
fn the_exempted_payer_is_a_fund(config: &mut TaxConfig) {
    config
        .etf_classification
        .insert("US0378331005".to_owned(), EtfClassification::Equity);
}

/// The prior-year deferral the `unpriced_deferral` fixture needs to say anything: Gipuzkoa
/// publishes no 2023 actualization table, so the 2023 sale could not be priced and its deferral has
/// to be carried in from that year's own return.
fn carried_in_deferral(config: &mut SpanishTaxConfig) {
    config.deferred_losses = vec![DeferredLossConfig {
        symbol: "AAPL".to_owned(),
        isin: Some("US0378331005".to_owned()),
        loss: dec!(9000),
        blocked_quantity: dec!(100),
        acquisition_date: Date::from_ymd_opt(2023, 7, 10).unwrap(),
        sale_date: Date::from_ymd_opt(2023, 6, 10).unwrap(),
    }];
}

/// Per-case statement hook, applied to the computed statement just before it is rendered.
///
/// A few report paths exist for figures the Spanish pipeline carries but no committed fixture
/// produces, so the only way to pin how they render is to put the figure on the statement by hand.
/// Each hook below says which path it lights up and why no fixture reaches it. No hook computes a
/// tax figure of its own: it adds the rows a real run would have added, and re-runs
/// `calculate_totals` when it adds something a total depends on, so the report stays a pure
/// function of the statement.
type Mutate = fn(&mut SpanishTaxStatement);

fn as_computed(_statement: &mut SpanishTaxStatement) {}

/// A lot opened by a corporate action, which the open-positions table labels "Operación societaria".
///
/// Only a spinoff or a stock dividend opens such a lot. The one corporate action the fixtures carry
/// is a plain split, and a split restates the lots that already exist rather than opening one of
/// its own — so `LotSource::CorporateAction` reaches no golden through a fixture.
fn a_lot_opened_by_a_corporate_action(statement: &mut SpanishTaxStatement) {
    let vest = statement
        .report
        .open_lots
        .first()
        .expect("the grants fixture must leave the vested lot open at year end")
        .clone();
    statement.report.open_lots.push(OpenLotRow {
        source: LotSource::CorporateAction,
        open_date: Date::from_ymd_opt(2026, 6, 30).unwrap(),
        trade_id: None,
        quantity: dec!(2),
        // A corporate-action lot has no trade behind it, so it carries no currency and no price.
        currency: String::new(),
        price: None,
        cost_eur: Decimal::ZERO,
        ..vest
    });
}

/// A cash award and an open short position: two figures the return reports without taxing.
///
/// Neither is reachable from a committed fixture. Only the Sber reader builds a cash grant, so an
/// Interactive Brokers statement cannot produce one at all. A negative `OpenPosition` in a Flex
/// export *is* read as a short, but every fixture that already exists is shared with a golden that
/// would then move, so the position is put on the statement here instead.
fn reported_but_never_taxed(statement: &mut SpanishTaxStatement) {
    let date = Date::from_ymd_opt(2026, 4, 10).unwrap();
    let description = "PORTFOLIO TRANSFER BONUS";
    let amount_eur = dec!(120);

    // Both rows, as the processor writes them: the booking is what the movimientos table shows and
    // the entry is what the general-base table shows, and a statement carrying one without the
    // other is a state no run produces.
    statement.report.bookings.push(BookingRow {
        kind: BookingKind::CashGrant,
        date,
        symbol: String::new(),
        isin: String::new(),
        name: String::new(),
        category: None,
        description: description.to_owned(),
        currency: "EUR".to_owned(),
        amount: amount_eur,
        eur_per_unit: dec!(1),
        amount_eur,
    });
    statement.cash_grants.push(CashGrantEntry {
        date,
        description: description.to_owned(),
        amount_eur,
        notes: "Informational: a cash award belongs to the GENERAL base, not the savings base — \
                it is either employment income or a ganancia patrimonial no derivada de \
                transmisión. Declare it separately; this tool does not compute it"
            .to_owned(),
    });

    statement
        .short_positions
        .push(("TSLA".to_owned(), dec!(-25)));

    statement.calculate_totals();
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
/// - `income_navarra_2026` — the only case where the 3% fee ceiling actually bites, so the only one
///   that renders the capped-fee row and the "Importes informativos" block.
/// - `small_disposal_navarra_2026` — the €3,000 exemption row, which exists under one regime only.
/// - `dividend_exemption_gipuzkoa_2026_fund` — one payer classified as a fund and one left as a
///   share, so the only case that renders the "Fondos e IIC" grouping, keeps two classes apart, and
///   names a payer the €1,500 exemption does not reach.
/// - `wash_sale_split_gipuzkoa_2026` — a lot held across a 2-for-1 split, so the open-positions
///   section shows a restated share count.
///
/// The cases below it pin the paths the nine above never reach. Each renders a warning, a marker or
/// a table that only fires on a shape the main corpus has no example of, so a change to one of them
/// would otherwise land in a filer's report with the whole suite green:
///
/// - `pre_1995_lot_navarra_2026` — a lot bought before 31-12-1994, so the worksheet marks it
///   "anterior a 1995" and the abatement register has something to say.
/// - `grants_comun_2026` — the vest table, plus the two lot origins a purchase is not: a granted
///   lot from the fixture and a corporate-action lot added to the statement.
/// - `year_end_loss_gipuzkoa_2026` — a December loss whose two-month window outlives the statement,
///   so the report warns the deduction may be overstated.
/// - `wash_sale_venue_comun_2026` — a repurchase outside two months on a venue whose equivalence
///   decision has lapsed, the one review that names a market.
/// - `unpriced_deferral_gipuzkoa_2026` — a filing year that carries a deferral in from a year with
///   no actualization table, so the report says which years went untested.
/// - `fee_types_comun_2026` — the fee rows whose treatment is unsettled, listed as their own review.
/// - `income_comun_2026_reported_only` — a cash award and an open short position: reported in full,
///   taxed nowhere, and neither reachable from a committed fixture.
/// - `fx_gain_comun_2026` — the only case whose Modelo 100 block reaches the otros-elementos sums
///   (casillas 0386 and 0385) and the footer that tells the filer how to itemize them.
#[rstest]
#[case::fifo_gipuzkoa(
    "fifo_gipuzkoa_2026",
    "fifo",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::income_comun(
    "income_comun_2026",
    "income",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::cross_offset_navarra(
    "cross_offset_navarra_2026_saldo",
    "cross_offset",
    2026,
    SpanishTaxRegime::Navarra,
    rcm_saldo_2000,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::wash_sale_multi_lot_gipuzkoa(
    "wash_sale_multi_lot_gipuzkoa_2026",
    "wash_sale_multi_lot",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::fx_gain_gipuzkoa(
    "fx_gain_gipuzkoa_2026",
    "fx_gain",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    revaluing_converter,
    everything_is_a_share,
    as_computed
)]
#[case::income_navarra(
    "income_navarra_2026",
    "income",
    2026,
    SpanishTaxRegime::Navarra,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::small_disposal_navarra(
    "small_disposal_navarra_2026",
    "small_disposal",
    2026,
    SpanishTaxRegime::Navarra,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::dividend_exemption_gipuzkoa_fund(
    "dividend_exemption_gipuzkoa_2026_fund",
    "dividend_exemption",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    converter,
    the_exempted_payer_is_a_fund,
    as_computed
)]
#[case::wash_sale_split_gipuzkoa(
    "wash_sale_split_gipuzkoa_2026",
    "wash_sale_split",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::pre_1995_lot_navarra(
    "pre_1995_lot_navarra_2026",
    "pre_1995_lot",
    2026,
    SpanishTaxRegime::Navarra,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::grants_comun(
    "grants_comun_2026",
    "grants",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances,
    converter,
    everything_is_a_share,
    a_lot_opened_by_a_corporate_action
)]
#[case::year_end_loss_gipuzkoa(
    "year_end_loss_gipuzkoa_2026",
    "year_end_loss",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::wash_sale_venue_comun(
    "wash_sale_venue_comun_2026",
    "wash_sale_venue",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::unpriced_deferral_gipuzkoa(
    "unpriced_deferral_gipuzkoa_2026",
    "unpriced_deferral",
    2026,
    SpanishTaxRegime::Gipuzkoa,
    carried_in_deferral,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::fee_types_comun(
    "fee_types_comun_2026",
    "fee_types",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances,
    converter,
    everything_is_a_share,
    as_computed
)]
#[case::fx_gain_comun(
    "fx_gain_comun_2026",
    "fx_gain",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances,
    revaluing_converter,
    everything_is_a_share,
    as_computed
)]
#[case::income_comun_reported_only(
    "income_comun_2026_reported_only",
    "income",
    2026,
    SpanishTaxRegime::Comun,
    no_opening_balances,
    converter,
    everything_is_a_share,
    reported_but_never_taxed
)]
fn rendered_report_matches_its_golden(
    #[case] golden: &str,
    #[case] fixture: &str,
    #[case] year: i32,
    #[case] regime: SpanishTaxRegime,
    #[case] configure: Configure,
    #[case] rates: fn() -> CurrencyConverter,
    #[case] classify: Classify,
    #[case] mutate: Mutate,
) {
    let mut config = spain_config(regime);
    configure(config.spain.as_mut().unwrap());
    classify(&mut config);

    let statement = read_fixture(fixture);
    let (mut spanish, _has_income) =
        super::compute_tax_year(&statement, year, &rates(), &config).unwrap();
    mutate(&mut spanish);

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
const REPORT_CORPUS: [&str; 17] = [
    "fifo_gipuzkoa_2026",
    "income_comun_2026",
    "cross_offset_navarra_2026_saldo",
    "wash_sale_multi_lot_gipuzkoa_2026",
    "fx_gain_gipuzkoa_2026",
    "income_navarra_2026",
    "small_disposal_navarra_2026",
    "dividend_exemption_gipuzkoa_2026_fund",
    "wash_sale_split_gipuzkoa_2026",
    "pre_1995_lot_navarra_2026",
    "grants_comun_2026",
    "year_end_loss_gipuzkoa_2026",
    "wash_sale_venue_comun_2026",
    "unpriced_deferral_gipuzkoa_2026",
    "fee_types_comun_2026",
    "fx_gain_comun_2026",
    "income_comun_2026_reported_only",
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

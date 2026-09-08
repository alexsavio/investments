//! Structural tests of the HTML report over the German fixtures. Numbers are asserted through the
//! statement itself (the report only re-punctuates `format_eur` output), so these tests pin the
//! document shape, escaping and the reconciliation of the worksheet rows with the tax entries.

use std::path::PathBuf;

use rstest::rstest;

use crate::instruments::EtfClassification;
use crate::tax_statement::golden::GoldenCorpus;
use crate::tax_statement::html::format::punctuate;
use crate::taxes::TaxConfig;
use crate::time::{self, Date, DateTime, Period};

use super::super::report_details::{BookingKind, FxTreatment, TradeSide};
use super::super::tests::{run_pipeline, run_pipeline_with_config};
use super::super::{GermanTaxStatement, format_eur};
use super::format::eur;
use super::{HtmlReport, ReportMeta};

fn meta(year: i32) -> ReportMeta {
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
        generated_at: time::now(),
    }
}

fn render(statement: &GermanTaxStatement) -> String {
    let mut buffer = Vec::new();
    HtmlReport::write(statement, &meta(statement.year), &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

fn offset(html: &str, needle: &str) -> usize {
    html.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in the report"))
}

#[test]
fn document_skeleton_and_section_order() {
    let statement = run_pipeline("fifo", 2024);
    let html = render(&statement);

    assert!(html.starts_with("<!DOCTYPE html>\n<html lang=\"de\">"));
    assert!(html.contains("<meta charset=\"utf-8\">"));
    assert!(html.contains("<title>Informativer Steuerbericht 2024</title>"));
    assert!(html.contains("@page { size: A4 landscape;"));
    assert!(html.contains("Konto-ID: U1234567"));
    assert!(html.contains("Haftungsausschluss"));

    let ids = [
        "id=\"inhalt\"",
        "id=\"steuerformulare\"",
        "id=\"steuerberechnung\"",
        "id=\"aktivitaet\"",
        "id=\"wertpapiere\"",
        "id=\"wertpapiergeschaefte\"",
        "id=\"wertpapiertransaktionen\"",
        "id=\"wertpapieruebersicht\"",
        "id=\"hinweise\"",
    ];
    let offsets: Vec<usize> = ids.iter().map(|id| offset(&html, id)).collect();
    assert!(
        offsets.windows(2).all(|pair| pair[0] < pair[1]),
        "sections out of order: {offsets:?}"
    );

    // The table of contents links every rendered section and nothing else.
    assert!(html.contains("href=\"#wertpapiergeschaefte\""));
    assert!(!html.contains("href=\"#quellensteuer\""));
    assert!(!html.contains("id=\"quellensteuer\""));
}

#[test]
fn amounts_are_the_statement_figures_in_german_notation() {
    let statement = run_pipeline("fifo", 2024);
    let html = render(&statement);

    // €900 total gain, KAP line 19, rendered as "900,00".
    assert_eq!(format_eur(statement.kap_zeile_19), "900.00");
    assert!(html.contains(">900,00<"));
    assert_eq!(
        eur(statement.kap_zeile_19),
        punctuate(&format_eur(statement.kap_zeile_19))
    );

    // Instrument name and ISIN from the Flex export reach the report.
    assert!(html.contains("APPLE INC"));
    assert!(html.contains("US0378331005"));
    assert!(html.contains("Vereinigte Staaten (US)"));
}

#[test]
fn markup_in_names_is_escaped() {
    let mut statement = run_pipeline("fifo", 2024);
    for security in &mut statement.report.securities {
        security.name = "A<B&C \"quoted\"".to_owned();
    }
    for trade in &mut statement.report.trades {
        trade.name = "A<B&C \"quoted\"".to_owned();
    }
    let html = render(&statement);

    assert!(html.contains("A&lt;B&amp;C &quot;quoted&quot;"));
    assert!(!html.contains("A<B&C"));
}

/// The Altbestand flag is derived in the renderer from the lot's own acquisition date, not carried
/// on the row, and no fixture holds a pre-2009 lot — so this is the only thing pinning the cutoff.
#[test]
fn lots_acquired_before_2009_are_marked_altbestand() {
    let mut statement = run_pipeline("fifo", 2024);
    assert!(!render(&statement).contains("Altbestand (vor 2009)"));

    let lot = &mut statement.report.sales[0].lots[0];
    lot.open_date = Date::from_ymd_opt(2008, 12, 31).unwrap();
    assert!(render(&statement).contains("Altbestand (vor 2009)"));

    // The cutoff is exclusive: 1 January 2009 is already new stock.
    let lot = &mut statement.report.sales[0].lots[0];
    lot.open_date = Date::from_ymd_opt(2009, 1, 1).unwrap();
    assert!(!render(&statement).contains("Altbestand (vor 2009)"));
}

#[test]
fn empty_statement_renders_title_and_form_table_only() {
    let mut statement = GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
    statement.calculate_totals();
    let html = render(&statement);

    assert!(html.contains("id=\"steuerformulare\""));
    assert!(html.contains("id=\"steuerberechnung\""));
    assert!(html.contains("id=\"hinweise\""));
    for id in [
        "aktivitaet",
        "wertpapiere",
        "buchungen",
        "quellensteuer",
        "wertpapiergeschaefte",
        "wertpapiertransaktionen",
        "fremdwaehrung",
        "offene-positionen",
        "vorabpauschale",
        "wertpapieruebersicht",
    ] {
        assert!(
            !html.contains(&format!("id=\"{id}\"")),
            "{id} should be omitted"
        );
    }
}

#[test]
fn sale_worksheets_reconcile_with_capital_gain_entries() {
    let statement = run_pipeline("fifo", 2024);
    let report = &statement.report;

    assert_eq!(report.sales.len(), statement.capital_gains.len());
    assert_eq!(report.sales.len(), 2);
    for (sale, entry) in report.sales.iter().zip(&statement.capital_gains) {
        assert_eq!(sale.symbol, entry.symbol);
        assert_eq!(sale.proceeds_eur, entry.proceeds_eur);
        assert_eq!(sale.cost_basis_eur, entry.cost_basis_eur);
        assert_eq!(sale.gain_loss_eur, entry.gross_gain_loss);
        let lot_cost: crate::types::Decimal = sale.lots.iter().map(|lot| lot.cost_eur).sum();
        assert_eq!(lot_cost, entry.cost_basis_eur);
        assert!(sale.lots.iter().all(|lot| lot.holding_days > 0));
    }

    // Two buys and two sells; buys are cash outflows.
    assert_eq!(report.trades.len(), 4);
    let buys: Vec<_> = report
        .trades
        .iter()
        .filter(|trade| trade.side == TradeSide::Buy)
        .collect();
    assert_eq!(buys.len(), 2);
    assert!(buys.iter().all(|trade| trade.amount_eur < dec!(0)));
    let sells: Vec<_> = report
        .trades
        .iter()
        .filter(|trade| trade.side == TradeSide::Sell)
        .collect();
    assert_eq!(sells.len(), 2);
    assert_eq!(sells[0].amount_eur, statement.capital_gains[0].proceeds_eur);

    // Everything was sold, so nothing is open and the security list has the one stock.
    assert!(report.open_lots.is_empty());
    assert_eq!(report.securities.len(), 1);
    assert_eq!(report.securities[0].symbol, "AAPL");
    assert_eq!(report.securities[0].country_code, "US");
    assert_eq!(report.securities[0].name, "APPLE INC");
}

#[test]
fn withholding_rows_carry_country_and_credit() {
    let statement = run_pipeline("dividend_withholding", 2024);
    let report = &statement.report;

    assert_eq!(report.withholding.len(), 1);
    let row = &report.withholding[0];
    assert_eq!(row.symbol, "MSFT");
    assert_eq!(row.country_code, "US");
    assert_eq!(row.name, "MICROSOFT CORP");
    let entry = &statement.dividends[0];
    assert_eq!(row.gross_eur, entry.gross_amount_eur);
    assert_eq!(row.withheld_eur, entry.foreign_withholding_tax);
    assert_eq!(row.creditable_eur, entry.foreign_tax_credit);
    assert_eq!(row.withholding_rate, dec!(0.15));

    // Dividend, its withholding, the interest and the fee are all cash bookings.
    let kinds: Vec<BookingKind> = report.bookings.iter().map(|row| row.kind).collect();
    assert!(kinds.contains(&BookingKind::Dividend));
    assert!(kinds.contains(&BookingKind::WithholdingTax));
    assert!(kinds.contains(&BookingKind::Interest));
    assert!(kinds.contains(&BookingKind::Fee));
    let withholding = report
        .bookings
        .iter()
        .find(|row| row.kind == BookingKind::WithholdingTax)
        .unwrap();
    assert_eq!(withholding.amount_eur, -entry.foreign_withholding_tax);

    // IBKR trade ids from the Flex export are carried onto the raw trades and the open lots.
    assert_eq!(report.trades.len(), 1);
    assert_eq!(report.trades[0].trade_id.as_deref(), Some("4242"));
    assert_eq!(report.open_lots.len(), 1);
    assert_eq!(report.open_lots[0].trade_id.as_deref(), Some("4242"));
    assert_eq!(report.open_lots[0].quantity, dec!(10));
    // 10 × $100 + $1 commission at 0.9 EUR/USD.
    assert_eq!(report.open_lots[0].cost_eur, dec!(900.9));
    assert_eq!(report.trades[0].amount_eur, dec!(-900.9));

    let html = render(&statement);
    assert!(html.contains("id=\"quellensteuer\""));
    assert!(html.contains("id=\"buchungen\""));
    assert!(html.contains("id=\"offene-positionen\""));
    assert!(html.contains("Vereinigte Staaten (US)"));
    assert!(html.contains(">4242<"));
}

#[test]
fn open_lots_reflect_unsold_quantity() {
    let mut tax_config = crate::taxes::TaxConfig::default();
    tax_config.etf_classification.insert(
        "IE00B4L5Y983".to_string(),
        crate::instruments::EtfClassification::Equity,
    );
    let statement = run_pipeline_with_config("vorabpauschale", 2024, &tax_config);
    let report = &statement.report;

    let lots: Vec<_> = report
        .open_lots
        .iter()
        .filter(|lot| lot.symbol == "EUNL")
        .collect();
    assert!(!lots.is_empty());
    let quantity: crate::types::Decimal = lots.iter().map(|lot| lot.quantity).sum();
    assert_eq!(quantity, dec!(100));
    assert!(lots.iter().all(|lot| lot.cost_eur > dec!(0)));
    assert!(report.open_lots_as_of.is_some());

    let html = render(&statement);
    assert!(html.contains("id=\"offene-positionen\""));
    assert!(html.contains("Aktienfonds"));
}

/// A real Flex export with a Statement-of-Funds currency ledger flows through the whole pipeline
/// into the FX report rows and the rendered section: a EUR→USD exchange at 0.88 (paired EUR leg)
/// followed by a USD purchase valued at the ECB rate (0.90) is a §20 gain of 1000 × 0.02 = 20 EUR.
#[test]
fn fx_ledger_flows_from_statement_of_funds_into_the_report() {
    let statement = run_pipeline("fx_ledger", 2024);
    let rows = &statement.report.fx_rows;
    assert_eq!(rows.len(), 2, "{rows:#?}");

    let acquisition = &rows[0];
    assert_eq!(acquisition.currency, "USD");
    assert_eq!(acquisition.activity_code, "FOREX");
    assert_eq!(acquisition.transaction_id, "7001");
    assert_eq!(acquisition.units, dec!(1000));
    assert_eq!(acquisition.eur_per_unit, dec!(0.88));
    assert_eq!(acquisition.balance_after, dec!(1000));
    assert_eq!(acquisition.treatment, None);
    assert_eq!(acquisition.gain_loss_eur, None);

    let disposal = &rows[1];
    assert_eq!(disposal.activity_code, "BUY");
    assert_eq!(disposal.transaction_id, "7002");
    assert_eq!(disposal.units, dec!(-1000));
    assert_eq!(disposal.eur_per_unit, dec!(0.9));
    assert_eq!(disposal.open_eur_per_unit, Some(dec!(0.88)));
    assert_eq!(disposal.open_value_eur, Some(dec!(880)));
    assert_eq!(disposal.treatment, Some(FxTreatment::Section20Taxable));
    assert_eq!(disposal.gain_loss_eur, Some(dec!(20)));
    assert_eq!(disposal.balance_after, dec!(0));
    assert_eq!(disposal.holding_days, Some(14));

    // The row carries the very figure the tax entry uses.
    assert_eq!(statement.fx_gains.len(), 1);
    assert_eq!(
        Some(statement.fx_gains[0].gross_amount_eur),
        disposal.gain_loss_eur
    );
    assert_eq!(statement.total_fx_gains, dec!(20));

    // The Trades section is authoritative for the trade itself; its transactionID matches the ledger.
    assert_eq!(statement.report.trades.len(), 1);
    assert_eq!(statement.report.trades[0].trade_id.as_deref(), Some("7002"));

    let html = render(&statement);
    assert!(html.contains("id=\"fremdwaehrung\""));
    assert!(html.contains("USD – Fremdwährungskonto"));
    assert!(html.contains("§20 steuerpflichtig"));
    assert!(html.contains(">20,00<"));
}

fn corpus() -> GoldenCorpus {
    GoldenCorpus::new(
        "src/tax_statement/germany/testdata/golden",
        "UPDATE_GOLDEN=1 cargo test --lib germany::html::tests",
    )
}

/// A meta block with a constant `generated_at`, so the rendered document is reproducible.
fn fixed_meta(year: i32) -> ReportMeta {
    ReportMeta {
        generated_at: DateTime::new(
            Date::from_ymd_opt(2026, 1, 15).unwrap(),
            crate::time::Time::from_hms_opt(12, 0, 0).unwrap(),
        ),
        ..meta(year)
    }
}

/// Per-case tax configuration. Most fixtures need none; the fund fixture needs its classification.
type Configure = fn() -> TaxConfig;

fn no_classification() -> TaxConfig {
    TaxConfig::default()
}

/// The fixture's ETF is an equity fund, which is what puts a Teilfreistellung rate on the report.
fn equity_fund() -> TaxConfig {
    let mut config = TaxConfig::default();
    config
        .etf_classification
        .insert("IE00B4L5Y983".to_owned(), EtfClassification::Equity);
    config
}

/// The rendered report, byte for byte, over the committed fixtures. The renderer is a pure function
/// of the statement and the meta block, so the only non-reproducible input is the generation
/// timestamp, which [`fixed_meta`] freezes; every other figure comes out of the pipeline the binary
/// runs. Regenerate with `UPDATE_GOLDEN=1 cargo test --lib germany::html::tests`.
///
/// One committed golden per case:
///
/// - `fifo` — two FIFO sale worksheets with their lots, the raw trades and the security overview.
/// - `dividend_withholding` — the cash bookings, the withholding section and an open lot.
/// - `fx_ledger` — the foreign-currency ledger with a §20 taxable disposal.
/// - `vorabpauschale` — the fund sections: Vorabpauschale, Teilfreistellung and the open lots.
/// - `derivative` — instruments the tool declines to tax, and the notes that say so.
/// - `short_position` — a position with no automatic treatment, surfaced for manual review.
#[rstest]
#[case::fifo("fifo_2024", "fifo", 2024, no_classification)]
#[case::dividend_withholding(
    "dividend_withholding_2024",
    "dividend_withholding",
    2024,
    no_classification
)]
#[case::fx_ledger("fx_ledger_2024", "fx_ledger", 2024, no_classification)]
#[case::vorabpauschale("vorabpauschale_2024", "vorabpauschale", 2024, equity_fund)]
#[case::derivative("derivative_2024", "derivative", 2024, no_classification)]
#[case::short_position("short_position_2024", "short_position", 2024, no_classification)]
fn rendered_report_matches_its_golden(
    #[case] golden: &str,
    #[case] fixture: &str,
    #[case] year: i32,
    #[case] configure: Configure,
) {
    let statement = run_pipeline_with_config(fixture, year, &configure());

    let mut emitted = Vec::new();
    HtmlReport::write(&statement, &fixed_meta(year), &mut emitted).unwrap();
    let emitted = String::from_utf8(emitted).expect("the report must be valid UTF-8");

    corpus().assert(golden, "html", &emitted);
}

#[test]
fn every_golden_file_belongs_to_a_case() {
    let corpus = corpus();
    let claimed: Vec<PathBuf> = CORPUS
        .iter()
        .map(|name| corpus.path(name, "html"))
        .collect();
    corpus.assert_no_orphans(&claimed, &["html"]);
}

/// The file stems of every case above, in case order. Kept beside the `#[case]` list rather than
/// derived from it: `rstest` does not expose its cases to another test.
const CORPUS: [&str; 6] = [
    "fifo_2024",
    "dividend_withholding_2024",
    "fx_ledger_2024",
    "vorabpauschale_2024",
    "derivative_2024",
    "short_position_2024",
];

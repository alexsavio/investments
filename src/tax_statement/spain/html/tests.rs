//! Structural tests of the HTML report over the Spanish fixtures. Numbers are asserted through the
//! statement itself (the report only re-punctuates `format_eur` output), so these tests pin the
//! document shape, the Spanish notation, the escaping, the per-regime differences and the
//! reconciliation of the worksheet rows with the tax entries.

use rstest::rstest;

use crate::tax_statement::html::format::punctuate;
use crate::taxes::spain::SpanishTaxRegime;
use crate::time::{self, Date, Period};
use crate::types::Decimal;

use super::super::forms;
use super::super::statement::SpanishTaxStatement;
use super::super::tests::{
    read_fixture, revaluing_converter, run_pipeline, run_pipeline_with_config, spain_config,
};
use super::super::{compute_tax_year, format_eur};
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

fn render(statement: &SpanishTaxStatement) -> String {
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
    let statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
    let html = render(&statement);

    assert!(html.starts_with("<!DOCTYPE html>\n<html lang=\"es\">"));
    assert!(html.contains("<meta charset=\"utf-8\">"));
    assert!(html.contains("<title>Informe fiscal informativo 2026</title>"));
    assert!(html.contains("@page { size: A4 landscape;"));
    assert!(html.contains("Cuenta: U1234567"));
    assert!(html.contains("Gipuzkoa (Norma Foral 3/2014)"));
    assert!(html.contains("limitación de responsabilidad"));

    let ids = [
        "id=\"indice\"",
        "id=\"formularios\"",
        "id=\"calculo\"",
        "id=\"actividad\"",
        "id=\"valores\"",
        "id=\"transmisiones\"",
        "id=\"operaciones\"",
        "id=\"relacion-valores\"",
        "id=\"avisos\"",
    ];
    let offsets: Vec<usize> = ids.iter().map(|id| offset(&html, id)).collect();
    assert!(
        offsets.windows(2).all(|pair| pair[0] < pair[1]),
        "sections out of order: {offsets:?}"
    );

    // The table of contents links every rendered section and nothing else.
    assert!(html.contains("href=\"#transmisiones\""));
    assert!(!html.contains("href=\"#retenciones\""));
    assert!(!html.contains("id=\"retenciones\""));
}

/// The reading order is the same under every regime; only what each section says changes.
#[rstest]
#[case(SpanishTaxRegime::Gipuzkoa)]
#[case(SpanishTaxRegime::Comun)]
#[case(SpanishTaxRegime::Navarra)]
fn section_order_holds_under_every_regime(#[case] regime: SpanishTaxRegime) {
    let statement = run_pipeline("fifo", 2026, regime);
    let html = render(&statement);

    let offsets: Vec<usize> = [
        "id=\"formularios\"",
        "id=\"calculo\"",
        "id=\"actividad\"",
        "id=\"valores\"",
        "id=\"transmisiones\"",
        "id=\"operaciones\"",
        "id=\"relacion-valores\"",
        "id=\"avisos\"",
    ]
    .iter()
    .map(|id| offset(&html, id))
    .collect();
    assert!(
        offsets.windows(2).all(|pair| pair[0] < pair[1]),
        "{regime:?}: sections out of order: {offsets:?}"
    );
    assert!(html.contains(statement.regime.description()));
}

#[test]
fn amounts_are_the_statement_figures_in_spanish_notation() {
    let statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Comun);
    let html = render(&statement);

    assert_eq!(
        eur(statement.savings_base),
        punctuate(&format_eur(statement.savings_base))
    );
    assert!(html.contains(&format!(">{}<", eur(statement.savings_base))));
    assert!(html.contains(&format!(">{} €<", eur(statement.net_tax_due))));

    // Dates are dd/mm/yyyy, and the ISO form is confined to the configuration block.
    assert!(html.contains("Periodo: 01/01/2026 – 31/12/2026"));
    assert!(!html.contains(">2026-01-01<"));

    // Instrument name and ISIN from the Flex export reach the report.
    assert!(html.contains("APPLE INC"));
    assert!(html.contains("US0378331005"));
    assert!(html.contains("Estados Unidos (US)"));
}

/// Every broker-supplied string is poisoned, not just the two the eye lands on: the negative
/// assertion is only worth what the poison covers, and half of these reach `p`/`note`/`ul`, which
/// take trusted HTML and escape nothing themselves.
#[test]
fn broker_supplied_text_is_escaped_wherever_it_reaches_the_page() {
    const POISON: &str = "A<B&C \"quoted\"";

    let mut statement = run_pipeline("income", 2026, SpanishTaxRegime::Gipuzkoa);
    for security in &mut statement.report.securities {
        security.symbol = POISON.to_owned();
        security.isin = POISON.to_owned();
        security.name = POISON.to_owned();
        security.currency = POISON.to_owned();
    }
    for trade in &mut statement.report.trades {
        trade.name = POISON.to_owned();
        trade.symbol = POISON.to_owned();
        trade.isin = POISON.to_owned();
        trade.currency = POISON.to_owned();
        trade.trade_id = Some(POISON.to_owned());
    }
    for booking in &mut statement.report.bookings {
        booking.name = POISON.to_owned();
        booking.isin = POISON.to_owned();
        booking.description = POISON.to_owned();
        booking.currency = POISON.to_owned();
    }
    for row in &mut statement.report.withholding {
        row.name = POISON.to_owned();
        row.isin = POISON.to_owned();
        row.currency = POISON.to_owned();
    }
    for row in &mut statement.report.fx_rows {
        row.currency = POISON.to_owned();
        row.transaction_id = POISON.to_owned();
        row.activity_code = POISON.to_owned();
    }
    for lot in &mut statement.report.open_lots {
        lot.name = POISON.to_owned();
        lot.isin = POISON.to_owned();
        lot.currency = POISON.to_owned();
    }
    for entry in &mut statement.dividends {
        entry.symbol = POISON.to_owned();
        entry.isin = POISON.to_owned();
        entry.description = POISON.to_owned();
        entry.notes = Some(POISON.to_owned());
    }
    for entry in &mut statement.interest {
        entry.description = POISON.to_owned();
        entry.notes = Some(POISON.to_owned());
    }
    for entry in &mut statement.fees {
        entry.description = POISON.to_owned();
        entry.notes = Some(POISON.to_owned());
        entry.review = Some(POISON.to_owned());
    }
    for entry in &mut statement.stock_grants {
        entry.symbol = POISON.to_owned();
        entry.description = POISON.to_owned();
    }
    for entry in &mut statement.corporate_actions {
        entry.symbol = POISON.to_owned();
        entry.description = POISON.to_owned();
        entry.notes = POISON.to_owned();
    }
    statement
        .short_positions
        .push((POISON.to_owned(), dec!(-3)));

    let mut meta = meta(statement.year);
    meta.broker_name = POISON.to_owned();
    meta.portfolio_name = POISON.to_owned();
    meta.account_id = Some(POISON.to_owned());
    let mut buffer = Vec::new();
    HtmlReport::write(&statement, &meta, &mut buffer).unwrap();
    let html = String::from_utf8(buffer).unwrap();

    assert!(html.contains("A&lt;B&amp;C &quot;quoted&quot;"));
    assert!(!html.contains("A<B&C"), "unescaped poison reached the page");
    assert!(!html.contains("\"quoted\""));
}

/// A year whose only content is a pending balance still has a report: the four sections that can
/// speak without a single trade.
#[test]
fn a_carryforward_only_year_renders_the_sections_it_has() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    let spain = config.spain.as_mut().unwrap();
    spain.loss_carryforward.gyp.insert(2021, dec!(4000));
    spain.loss_carryforward.gyp.insert(2023, dec!(1000));

    // The fixture disposes only in 2026, so 2025 has nothing of its own.
    let statement = run_pipeline_with_config("fifo", 2025, &config);
    let html = render(&statement);

    for id in ["formularios", "calculo", "compensacion", "avisos"] {
        assert!(html.contains(&format!("id=\"{id}\"")), "{id} should render");
    }
    for id in [
        "actividad",
        "valores",
        "movimientos",
        "retenciones",
        "transmisiones",
        "valores-homogeneos",
        "operaciones",
        "divisas",
        "posiciones-abiertas",
        "relacion-valores",
    ] {
        assert!(
            !html.contains(&format!("id=\"{id}\"")),
            "{id} should be omitted"
        );
    }

    // The expiring vintage and the one with years left are both named, with the year each runs out.
    assert!(html.contains(">2021<"));
    assert!(html.contains(">2025<"));
    assert!(html.contains(">2023<"));
    assert!(html.contains(">2027<"));
}

#[test]
fn sale_worksheets_reconcile_with_the_capital_gain_entries() {
    let statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
    let report = &statement.report;

    assert_eq!(report.sales.len(), statement.capital_gains.len());
    assert_eq!(report.sales.len(), 2);
    for (sale, entry) in report.sales.iter().zip(&statement.capital_gains) {
        assert_eq!(sale.symbol, entry.symbol);
        assert_eq!(sale.sale_date, entry.sale_date);
        assert_eq!(sale.settle_date, entry.settle_date);
        assert_eq!(sale.quantity, entry.quantity);
        assert_eq!(sale.proceeds_eur, entry.proceeds_eur);
        assert_eq!(sale.cost_basis_eur, entry.cost_eur);
        assert_eq!(sale.gain_loss_eur, entry.fiscal_gain_loss);

        assert_eq!(sale.lots.len(), entry.lots.len());
        let cost: Decimal = sale.lots.iter().map(|lot| lot.cost_eur).sum();
        assert_eq!(cost, entry.cost_eur);
        for (lot, detail) in sale.lots.iter().zip(&entry.lots) {
            assert_eq!(lot.open_date, detail.acquisition_date);
            assert_eq!(lot.quantity, detail.quantity);
            assert_eq!(lot.cost_eur, detail.cost_eur);
            assert_eq!(lot.proceeds_eur, detail.proceeds_eur);
            assert_eq!(lot.gain_loss_eur, detail.gain_eur);
            assert!(lot.holding_days > 0);
        }
    }

    // Two sells in the filing year; the buys are older, so only the sells reach the trade rows.
    assert_eq!(report.trades.len(), 2);
    assert_eq!(
        report.trades[0].amount_eur,
        statement.capital_gains[0].proceeds_eur
    );
    assert_eq!(report.securities.len(), 1);
    assert_eq!(report.securities[0].symbol, "AAPL");
    assert_eq!(report.securities[0].country_code, "US");
}

#[test]
fn withholding_rows_carry_the_entry_figures() {
    let statement = run_pipeline("income", 2026, SpanishTaxRegime::Comun);
    let report = &statement.report;

    // A withholding row exists only for a dividend that had tax withheld, so the population is
    // the filtered one; zipping the whole dividend list would pass on this fixture and mis-pair on
    // a statement that mixes withheld and un-withheld payments.
    let withheld: Vec<_> = statement
        .dividends
        .iter()
        .filter(|entry| !entry.withheld_eur.is_zero())
        .collect();
    assert!(!withheld.is_empty());
    assert_eq!(report.withholding.len(), withheld.len());
    for (row, entry) in report.withholding.iter().zip(&withheld) {
        assert_eq!(row.symbol, entry.symbol);
        assert_eq!(row.gross_eur, entry.gross_eur);
        assert_eq!(row.withheld_eur, entry.withheld_eur);
        assert_eq!(row.creditable_eur, entry.treaty_capped_credit);
    }

    // The dividend, its withholding, the interest and the fee are all cash bookings, and the
    // withholding booking is the mirror of the entry's own figure.
    use super::super::report::BookingKind;
    let kinds: Vec<BookingKind> = report.bookings.iter().map(|row| row.kind).collect();
    for kind in [
        BookingKind::Dividend,
        BookingKind::WithholdingTax,
        BookingKind::Interest,
        BookingKind::Fee,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} missing from {kinds:?}");
    }
    let withheld = report
        .bookings
        .iter()
        .find(|row| row.kind == BookingKind::WithholdingTax)
        .unwrap();
    assert_eq!(withheld.amount_eur, -statement.dividends[0].withheld_eur);

    let html = render(&statement);
    assert!(html.contains("id=\"retenciones\""));
    assert!(html.contains("id=\"movimientos\""));
    assert!(html.contains("Retenciones en origen"));
    assert!(html.contains("Estados Unidos (US)") || html.contains("Desconocido"));
}

/// The ledger rows are the whole year's movements, labelled by the balance each realization came
/// off; their results are exactly the figures the entries carry.
#[test]
fn fx_rows_follow_the_ledger_with_their_treatment() {
    use super::super::report::FxTreatment;

    let broker_statement = read_fixture("fx_gain");
    let converter = revaluing_converter(Date::from_ymd_opt(2026, 6, 1).unwrap(), dec!(1));
    let (statement, _) = compute_tax_year(
        &broker_statement,
        2026,
        &converter,
        &spain_config(SpanishTaxRegime::Gipuzkoa),
    )
    .unwrap();

    let realized: Vec<&_> = statement
        .report
        .fx_rows
        .iter()
        .filter(|row| row.treatment.is_some())
        .collect();
    assert_eq!(realized.len(), statement.fx_gains.len());
    for (row, entry) in realized.iter().zip(&statement.fx_gains) {
        assert_eq!(row.treatment, Some(FxTreatment::HeldBalance));
        assert_eq!(row.gain_loss_eur, Some(entry.amount_eur));
        assert_eq!(row.date, entry.date);
        assert_eq!(row.open_date, Some(entry.acquisition_date));
    }
    // An acquisition realizes nothing and must carry no treatment beside a blank result.
    assert!(
        statement
            .report
            .fx_rows
            .iter()
            .any(|row| row.treatment.is_none() && row.gain_loss_eur.is_none())
    );

    let html = render(&statement);
    assert!(html.contains("id=\"divisas\""));
    assert!(html.contains("USD — cuenta en divisa"));
    assert!(html.contains("Ganancia/pérdida patrimonial (saldo propio)"));

    let broker_statement = read_fixture("fx_borrowed");
    let (borrowed, _) = compute_tax_year(
        &broker_statement,
        2026,
        &converter,
        &spain_config(SpanishTaxRegime::Gipuzkoa),
    )
    .unwrap();
    let realized: Vec<&_> = borrowed
        .report
        .fx_rows
        .iter()
        .filter(|row| row.treatment == Some(FxTreatment::BorrowedBalance))
        .collect();
    assert_eq!(realized.len(), borrowed.fx_borrowed_review.len());
    assert_eq!(
        realized[0].gain_loss_eur,
        Some(borrowed.fx_borrowed_review[0].amount_eur)
    );

    let html = render(&borrowed);
    assert!(html.contains("Revisión manual (saldo prestado)"));
}

/// Only Gipuzkoa actualizes an acquisition cost, so only its worksheet earns the two extra columns.
#[test]
fn the_coefficient_columns_are_gipuzkoa_only() {
    let gipuzkoa = render(&run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa));
    assert!(gipuzkoa.contains(">Coeficiente<"));
    assert!(gipuzkoa.contains(">Coste actualizado<"));
    // NF 3/2014 art. 45.2 gives the 2021 lot 1.050 and the older one 1.212 for a 2026 disposal.
    assert!(
        gipuzkoa.contains(">1,050<") || gipuzkoa.contains(">1,212<"),
        "no coefficient cell"
    );

    for regime in [SpanishTaxRegime::Comun, SpanishTaxRegime::Navarra] {
        let html = render(&run_pipeline("fifo", 2026, regime));
        assert!(!html.contains(">Coeficiente<"), "{regime:?}");
        assert!(!html.contains(">Coste actualizado<"), "{regime:?}");
    }
}

/// The F-93 blocks that only fire in some years — the exención, the negative-saldo boxes and the
/// carry-forward ones — reach the report through the same mapping the CSV prints.
#[test]
fn the_navarra_conditional_boxes_reach_the_report() {
    let mut config = spain_config(SpanishTaxRegime::Navarra);
    config
        .spain
        .as_mut()
        .unwrap()
        .loss_carryforward
        .rcm
        .insert(2025, dec!(2000));

    let statement = run_pipeline_with_config("cross_offset", 2026, &config);
    let html = render(&statement);

    assert!(html.contains("Modelo F-93"));
    let mapping = forms::form_mapping(&statement);

    // The year's transmissions are negative, so H3 carries the saldo and the rest goes forward.
    for casilla in ["8816", "818"] {
        assert!(
            mapping.boxes.iter().any(|form| form.casilla == casilla),
            "casilla {casilla} should fire on this year"
        );
    }
    // A box the year has nothing for is not printed as a zero: an empty negative-saldo box invites
    // a filer to fill in a block that does not exist.
    assert!(!mapping.boxes.iter().any(|form| form.casilla == "8850"));
    assert!(!html.contains(">8850<"));

    // The footer note that tells the filer H3 replaces H1 travels with the mapping too.
    assert!(html.contains("apartado H3 applies rather than"));

    for form_box in &mapping.boxes {
        assert!(
            html.contains(&form_box.casilla),
            "casilla {} missing from the report",
            form_box.casilla
        );
    }
}

/// Every casilla the CSV emits is also in the report: the two read one mapping, and a filer who
/// works from the printed page must not be sent to fewer boxes than the file names.
#[rstest]
#[case(SpanishTaxRegime::Gipuzkoa)]
#[case(SpanishTaxRegime::Comun)]
#[case(SpanishTaxRegime::Navarra)]
fn every_form_casilla_appears_in_the_report(#[case] regime: SpanishTaxRegime) {
    let statement = run_pipeline("income", 2026, regime);
    let html = render(&statement);
    let mapping = forms::form_mapping(&statement);

    assert!(!mapping.boxes.is_empty());
    for form_box in &mapping.boxes {
        assert!(
            html.contains(&form_box.casilla),
            "{regime:?}: casilla {} missing",
            form_box.casilla
        );
        assert!(
            html.contains(&crate::tax_statement::html::builder::escape(
                &form_box.concept
            )),
            "{regime:?}: concept of casilla {} missing",
            form_box.casilla
        );
    }

    // Foreign withholding is never a Spanish retención, and the trap is named on the page.
    assert!(html.contains("no son una retención española"));
    assert!(html.contains(&mapping.withholding_casilla));
}

/// A blocked loss is shown in its own section, with the carry-out the next return starts from.
#[test]
fn the_valores_homogeneos_section_shows_the_deferral_and_its_carry_out() {
    let statement = run_pipeline("wash_sale_multi_lot", 2026, SpanishTaxRegime::Gipuzkoa);
    assert!(statement.total_deferred_loss > dec!(0));

    let html = render(&statement);
    assert!(html.contains("id=\"valores-homogeneos\""));
    assert!(html.contains("NF 3/2014 art. 43.g"));
    assert!(html.contains(&eur(statement.total_deferred_loss)));

    if !statement.deferred_losses_next.is_empty() {
        assert!(html.contains("id=\"compensacion\""));
        assert!(html.contains("deferred_losses:"));
        // Only the configuration block uses ISO dates.
        assert!(html.contains(&format!(
            "sale_date: {}",
            statement.deferred_losses_next[0]
                .sale_date
                .format("%Y-%m-%d")
        )));
    }
}

/// The bracket table re-slices the base to show where the cuota comes from, so its own column has
/// to add up to the cuota the scale computed. A slice that drifted would print a table whose total
/// contradicts the figure two sections earlier.
#[rstest]
#[case(SpanishTaxRegime::Gipuzkoa)]
#[case(SpanishTaxRegime::Comun)]
#[case(SpanishTaxRegime::Navarra)]
fn the_bracket_slices_add_up_to_the_cuota(#[case] regime: SpanishTaxRegime) {
    let statement = run_pipeline("fifo", 2026, regime);
    let base = statement.savings_base;
    assert!(base > dec!(0), "{regime:?}: the case must reach a bracket");

    let brackets = statement.scale.brackets();
    let mut slices = Decimal::ZERO;
    let mut quota = Decimal::ZERO;
    for (index, &(floor, rate)) in brackets.iter().enumerate() {
        if base <= floor {
            break;
        }
        let upper = match brackets.get(index + 1) {
            Some(&(next_floor, _)) => std::cmp::min(base, next_floor),
            None => base,
        };
        slices += upper - floor;
        quota += (upper - floor) * rate;
    }

    assert_eq!(slices, base, "{regime:?}");
    assert_eq!(quota, statement.savings_quota, "{regime:?}");

    let html = render(&statement);
    assert!(html.contains(">Tramo desde<"));
    assert!(html.contains(&format!(">{}<", eur(statement.savings_quota))));
}

/// A year with no base at all reaches no bracket, so the table is the total row alone — and the
/// section still renders, because the return exists either way.
#[test]
fn a_zero_base_reaches_no_bracket() {
    let statement = run_pipeline("fee_only", 2026, SpanishTaxRegime::Gipuzkoa);
    assert_eq!(statement.savings_base, dec!(0));
    assert_eq!(statement.savings_quota, dec!(0));

    let html = render(&statement);
    assert!(html.contains("id=\"calculo\""));
    assert!(html.contains(">Tramo desde<"));
    // The one bracket row the scale would show starts at zero; with no base there is none.
    assert!(!html.contains(">19 %<") && !html.contains(">20 %<"));
}

/// Each cash-booking group totals the figure the tax entries carry, so the section a filer ties
/// their bank statement to cannot drift from the one the return is built from.
#[test]
fn the_booking_totals_reconcile_with_the_entry_totals() {
    use super::super::report::BookingKind;

    let statement = run_pipeline("income", 2026, SpanishTaxRegime::Comun);
    let total_of = |kind: BookingKind| -> Decimal {
        statement
            .report
            .bookings
            .iter()
            .filter(|row| row.kind == kind)
            .map(|row| row.amount_eur)
            .sum()
    };

    assert_eq!(
        total_of(BookingKind::Dividend),
        statement.total_dividend_income
    );
    assert_eq!(
        total_of(BookingKind::WithholdingTax),
        -statement.total_foreign_withholding
    );
    assert_eq!(
        total_of(BookingKind::Interest),
        statement.total_interest_income - statement.total_paid_interest
    );
    assert_eq!(
        total_of(BookingKind::Fee),
        -(statement.total_deductible_fees
            + statement.total_capped_fees
            + statement.total_informational_fees)
    );
}

/// The FIFO section joins two vectors by index, which is the one place a wrong number could be
/// attributed to the wrong disposal. A broken pairing must cost the section, not print another
/// sale's actualization coefficient against a lot.
#[test]
fn a_broken_worksheet_pairing_refuses_the_section() {
    let statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
    assert!(render(&statement).contains(">Coeficiente<"));

    // One fewer entry than worksheets.
    let mut short = statement.clone();
    short.capital_gains.pop();
    let html = render(&short);
    assert!(html.contains("id=\"transmisiones\""));
    assert!(html.contains("No se ha podido emparejar"));
    assert!(!html.contains(">Coeficiente<"));

    // Same count, but a worksheet against the wrong security.
    let mut swapped = statement.clone();
    swapped.capital_gains[0].symbol = "OTHER".to_owned();
    assert!(render(&swapped).contains("No se ha podido emparejar"));

    // Same count and security, but a lot count that would silently truncate the zip.
    let mut trimmed = statement.clone();
    trimmed.capital_gains[0].lots.pop();
    assert!(render(&trimmed).contains("No se ha podido emparejar"));
}

/// The activity total sums only what reaches a savings-base group, so it is exactly the two group
/// results before compensation. A row put in the wrong group, or an informational one leaking into
/// the sum, would break this identity.
#[rstest]
#[case("income", SpanishTaxRegime::Gipuzkoa)]
#[case("income", SpanishTaxRegime::Comun)]
#[case("income", SpanishTaxRegime::Navarra)]
#[case("fifo", SpanishTaxRegime::Gipuzkoa)]
#[case("wash_sale_multi_lot", SpanishTaxRegime::Gipuzkoa)]
fn the_activity_total_is_the_two_group_results(
    #[case] fixture: &str,
    #[case] regime: SpanishTaxRegime,
) {
    let statement = run_pipeline(fixture, 2026, regime);
    let html = render(&statement);

    let total = statement.rcm_net + statement.gyp_net;
    assert!(
        html.contains("TOTAL — resultado de los dos grupos antes de compensar"),
        "{fixture}/{regime:?}"
    );
    assert!(
        html.contains(&format!(">{}<", eur(total))),
        "{fixture}/{regime:?}: expected the total {} on the page",
        eur(total)
    );
}

/// Ejercicios up to 2024 carry the Anexo 3 breakdown, whose casillas live in a different numbering
/// space from the Hoja: casilla 28 means one thing on each. Every golden files 2026, so the report
/// columns for that branch are pinned only here.
#[test]
fn a_2024_gipuzkoa_report_names_the_anexo_3_sheet() {
    use crate::taxes::spain::carryforward::LossLedger;
    use crate::taxes::spain::compensation::CrossOffset;
    use crate::taxes::spain::scale::SavingsScale;

    let regime = SpanishTaxRegime::Gipuzkoa;
    let mut statement = SpanishTaxStatement::new(
        2024,
        regime,
        SavingsScale::for_year(regime, 2024).unwrap(),
        LossLedger::default(),
        LossLedger::default(),
        CrossOffset::None,
        dec!(0.15),
        dec!(1500),
        None,
        false,
    );
    statement.calculate_totals();

    let html = render(&statement);
    assert!(html.contains(">06+16 (anexo 3)<"));
    assert!(html.contains(">17 (anexo 3)<"));
    assert!(html.contains(">28 (hoja)<"));
    // NF 1/2025 renumbering has not happened for this ejercicio.
    assert!(html.contains(">60 (hoja)<"));
    assert!(!html.contains(">70 (hoja)<"));
}

/// The activity table keeps gains and losses in separate columns, so every row has to be built
/// entry by entry. A row that added a pre-netted total would land wholly in one column and hide
/// the other — which is what the two columns exist to show.
#[test]
fn the_activity_row_of_a_mixed_currency_year_splits_its_gains_from_its_losses() {
    use super::super::statement::{FxGainEntry, InterestEntry};

    let mut statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Comun);
    let fx = |amount: Decimal| FxGainEntry {
        date: Date::from_ymd_opt(2026, 3, 1).unwrap(),
        currency: "USD".to_owned(),
        acquisition_date: Date::from_ymd_opt(2026, 1, 1).unwrap(),
        amount_eur: amount,
        activity_code: "FOREX".to_owned(),
    };
    statement.fx_gains.push(fx(dec!(500)));
    statement.fx_gains.push(fx(dec!(-300)));
    statement.interest.push(InterestEntry {
        date: Date::from_ymd_opt(2026, 6, 30).unwrap(),
        description: "Broker interest received".to_owned(),
        gross_eur: dec!(90),
        taxable: true,
        notes: None,
    });
    statement.interest.push(InterestEntry {
        date: Date::from_ymd_opt(2026, 9, 30).unwrap(),
        description: "Broker interest received (reversal)".to_owned(),
        gross_eur: dec!(-40),
        taxable: true,
        notes: None,
    });
    statement.calculate_totals();

    // The nets the totals carry would each fit in one column; the entries do not.
    assert_eq!(statement.total_fx_result, dec!(200));
    assert_eq!(statement.total_fx_gains, dec!(500));
    assert_eq!(statement.total_fx_losses, dec!(300));
    assert_eq!(statement.total_interest_income, dec!(50));

    let html = render(&statement);
    assert!(html.contains(">500,00<"), "the currency gain is missing");
    assert!(html.contains(">-300,00<"), "the currency loss is missing");
    assert!(html.contains(">90,00<"), "the interest received is missing");
    assert!(
        html.contains(">-40,00<"),
        "the interest reversal is missing"
    );

    // Splitting a row must not move the total.
    assert!(html.contains(&format!(">{}<", eur(statement.rcm_net + statement.gyp_net))));
}

/// A Navarra year that misses the €3,000 relief has to say what it was measured on. Without the
/// two figures the article turns on, a nil exemption is a silence the filer cannot check.
#[test]
fn a_navarra_year_over_the_limit_says_what_the_relief_was_measured_on() {
    let statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Navarra);
    assert_eq!(statement.small_disposals_exemption, dec!(0));
    assert!(!statement.small_disposals_unmeasurable);
    assert!(statement.small_disposals_proceeds > dec!(3000));

    let html = render(&statement);
    assert!(html.contains("no relevó nada este ejercicio"));
    assert!(html.contains(&eur(statement.small_disposals_proceeds)));
    assert!(html.contains(&eur(statement.small_disposals_gains)));

    // The other two regimes never measure it, so they never mention it.
    for regime in [SpanishTaxRegime::Gipuzkoa, SpanishTaxRegime::Comun] {
        let other = render(&run_pipeline("fifo", 2026, regime));
        assert!(
            !other.contains("no relevó nada este ejercicio"),
            "{regime:?}"
        );
    }
}

/// The FIFO subtotal has to reconcile the way the rows above it do. Under Gipuzkoa the result is
/// measured from the *actualized* cost, so a blank total in that column makes the bold row read as
/// `ingreso − coste`, which is a different number.
#[test]
fn the_fifo_subtotal_totals_the_column_the_result_is_measured_from() {
    let statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
    let proceeds: Decimal = statement
        .capital_gains
        .iter()
        .map(|entry| entry.proceeds_eur)
        .sum();
    let actualized: Decimal = statement
        .capital_gains
        .iter()
        .map(|entry| entry.actualized_cost_eur)
        .sum();
    let result: Decimal = statement
        .capital_gains
        .iter()
        .map(|entry| entry.fiscal_gain_loss)
        .sum();
    assert_eq!(proceeds - actualized, result);
    // The unactualized cost would give a different figure, which is the whole point.
    let cost: Decimal = statement
        .capital_gains
        .iter()
        .map(|entry| entry.cost_eur)
        .sum();
    assert_ne!(proceeds - cost, result);

    let html = render(&statement);
    assert!(
        html.contains(&format!(">{}<", eur(actualized))),
        "the actualized-cost total {} is missing from the subtotal row",
        eur(actualized)
    );
}

/// An expired balance is not pending. It gets its own column on the vintage that ran out of years,
/// so every row closes: saldo inicial − aplicado − caducado = pendiente.
#[test]
fn an_expired_vintage_is_not_shown_as_pending() {
    let mut config = spain_config(SpanishTaxRegime::Gipuzkoa);
    let spain = config.spain.as_mut().unwrap();
    spain.loss_carryforward.gyp.insert(2021, dec!(4000));
    spain.loss_carryforward.gyp.insert(2023, dec!(1000));

    let statement = run_pipeline_with_config("fifo", 2025, &config);
    assert_eq!(statement.gyp_expired, dec!(4000));

    let html = render(&statement);
    assert!(html.contains(">Caducado<"));
    // The old shape put the lost amount in a row of its own labelled only "caducado".
    assert!(!html.contains(">caducado<"));

    // Both vintages close, and only the one with years left is pending.
    for (origin, opening, lost, pending) in [
        (2021, dec!(4000), dec!(4000), dec!(0)),
        (2023, dec!(1000), dec!(0), dec!(1000)),
    ] {
        assert_eq!(opening - dec!(0) - lost, pending, "vintage {origin}");
    }
    assert_eq!(statement.gyp_ledger_next.balances().get(&2021), None);
    assert_eq!(statement.gyp_ledger_next.balances()[&2023], dec!(1000));
}

/// A figure printed in two sections must carry the same sign in both. The informational amounts are
/// positive magnitudes on the statement, and both sections subtract them.
#[test]
fn the_informational_amounts_carry_one_sign() {
    // Gipuzkoa deducts no fee at all (NF 3/2014 art. 39 is a closed list), so the whole charge is
    // informational and reaches both sections.
    let statement = run_pipeline("income", 2026, SpanishTaxRegime::Gipuzkoa);
    assert!(statement.total_informational_fees > dec!(0));

    let html = render(&statement);
    let informational = eur(-statement.total_informational_fees);
    assert!(
        html.matches(&format!(">{informational}<")).count() >= 2,
        "the informational fees should read {informational} in both sections"
    );
    assert!(!html.contains(&format!(">{}<", eur(statement.total_informational_fees))));
}

/// The ceiling splits the year's qualifying fees in two, and the activity table promises every
/// operation. Without a row for the disallowed half it adds up to less than the cash bookings by
/// exactly that amount.
#[test]
fn the_capped_half_of_a_fee_reaches_the_activity_table() {
    let statement = run_pipeline("income", 2026, SpanishTaxRegime::Navarra);
    assert_eq!(statement.total_deductible_fees, dec!(27));
    assert_eq!(statement.total_capped_fees, dec!(18));

    let html = render(&statement);
    assert!(html.contains("Comisiones excluidas por el límite del 3%"));
    assert!(html.contains(&format!(">{}<", eur(-statement.total_capped_fees))));

    // The three fee rows now account for every euro the bookings section totals.
    use super::super::report::BookingKind;
    let booked: Decimal = statement
        .report
        .bookings
        .iter()
        .filter(|row| row.kind == BookingKind::Fee)
        .map(|row| row.amount_eur)
        .sum();
    assert_eq!(
        booked,
        -(statement.total_deductible_fees
            + statement.total_capped_fees
            + statement.total_informational_fees)
    );

    // A year the ceiling had nothing to bite on says nothing at all: no fees, so nothing capped.
    let slack = run_pipeline("fifo", 2026, SpanishTaxRegime::Navarra);
    assert_eq!(slack.total_capped_fees, dec!(0));
    assert!(
        slack.custody_fee_cap.is_some(),
        "the regime still sets a ceiling"
    );
    let slack = render(&slack);
    assert!(!slack.contains("El límite excluyó"));
    assert!(!slack.contains("Comisiones excluidas por el límite"));
}

/// The Modelo 100 footer is the most actionable line in the section — it tells the filer to split a
/// currency result out of a casilla by hand. It must not be styled as an aside.
#[test]
fn a_form_footer_that_warns_is_styled_as_a_warning() {
    use super::super::statement::FxGainEntry;

    let mut statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Comun);
    statement.fx_gains.push(FxGainEntry {
        date: Date::from_ymd_opt(2026, 3, 1).unwrap(),
        currency: "USD".to_owned(),
        acquisition_date: Date::from_ymd_opt(2026, 1, 1).unwrap(),
        amount_eur: dec!(400),
        activity_code: "FOREX".to_owned(),
    });
    statement.calculate_totals();

    let html = render(&statement);
    let footer = html
        .find("acciones-cotizadas block")
        .expect("the footer warning should render");
    let note_open = html[..footer]
        .rfind("<div class=\"note ")
        .expect("the footer should be a note");
    assert!(
        html[note_open..].starts_with("<div class=\"note warn\">"),
        "{}",
        &html[note_open..note_open + 40]
    );
}

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

#[test]
fn markup_in_names_is_escaped() {
    let mut statement = run_pipeline("fifo", 2026, SpanishTaxRegime::Gipuzkoa);
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

    assert_eq!(report.withholding.len(), statement.dividends.len());
    for (row, entry) in report.withholding.iter().zip(&statement.dividends) {
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

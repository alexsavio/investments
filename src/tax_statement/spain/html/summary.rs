//! Summary sections: the form boxes, the tax computation, the activity and per-security overviews,
//! the valores-homogéneos block, the compensation ledgers and the notes.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::spain::carryforward::CARRYFORWARD_YEARS;
use crate::taxes::spain::compensation::CrossOffset;
use crate::types::Decimal;

use super::super::format_eur;
use super::super::forms;
use super::super::report::AssetClass;
use super::super::statement::SpanishTaxStatement;
use super::ReportMeta;
use super::builder::{
    Align, Cell, Row, b, col, escape, h3, h4, note, p, section_end, section_start, table, ul,
};
use super::format::{self, Group, asset_class_label, eur, group_label, pct, qty};

fn line(label: String, value: Decimal) -> Row {
    Row::data(vec![Cell::raw(label), Cell::num(eur(value))])
}

const AMOUNT_COLUMNS: [super::builder::Column; 2] = [
    col("Concepto", Align::Left),
    col("Importe (EUR)", Align::Right),
];

/// A preformatted block of trusted text, styled inline: the shared stylesheet is the German
/// report's too, and this is the only page that needs a `<pre>`.
fn pre(out: &mut String, text: &str) {
    let _ = writeln!(
        out,
        "<pre style=\"background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 8px; \
         padding: 8pt 10pt; font-size: 8pt; line-height: 1.4; white-space: pre-wrap; \
         overflow-wrap: anywhere;\">{}</pre>",
        escape(text)
    );
}

/// Section: Resumen para los formularios.
pub(super) fn tax_forms(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let mapping = forms::form_mapping(statement);

    section_start(out, "formularios", "Resumen para los formularios");
    p(
        out,
        &format!(
            "Esta tabla indica en qué {} del {} va cada importe del ejercicio. Los valores son el \
             resultado de la liquidación de sus operaciones; compruebe cada casilla contra el \
             impreso de su propia campaña antes de trasladarla.",
            b("casilla"),
            b(forms::form_name(statement.regime))
        ),
    );

    // The source note travels verbatim with the mapping, so the report, the CSV and the console
    // cannot end up citing different specimens of the same form.
    let caveats = mapping.header_lines.join(" ");
    let class = if caveats.contains("WARNING") {
        "warn"
    } else {
        "info"
    };
    note(
        out,
        class,
        &format!("{} — {}", b(mapping.title), escape(&caveats)),
    );

    let columns = [
        col("Formulario", Align::Left),
        col("Casilla", Align::Left),
        col("Concepto", Align::Left),
        col("Importe (EUR)", Align::Right),
    ];
    let rows: Vec<Row> = mapping
        .boxes
        .iter()
        .map(|form_box| {
            let casilla = if form_box.sheet.is_empty() {
                form_box.casilla.clone()
            } else {
                format!("{} ({})", form_box.casilla, form_box.sheet)
            };
            Row::data(vec![
                Cell::text(form_box.form),
                Cell::text(casilla),
                Cell::text(&form_box.concept),
                Cell::num(eur(form_box.value)),
            ])
        })
        .collect();
    table(out, &columns, &rows);

    if !mapping.footer_lines.is_empty() {
        note(out, "info", &escape(&mapping.footer_lines.join(" ")));
    }

    if !statement.total_foreign_withholding.is_zero() {
        note(
            out,
            "warn",
            &format!(
                "Los {} EUR retenidos en el extranjero {} y no van en la casilla {}, que es para el \
                 impuesto retenido en España. Se recuperan únicamente a través de la deducción por \
                 doble imposición internacional; consignarlos en ambos sitios los reclama dos veces.",
                b(&eur(statement.total_foreign_withholding)),
                b("no son una retención española"),
                escape(&mapping.withholding_casilla)
            ),
        );
    }

    section_end(out);
    true
}

/// Section: Cálculo del impuesto.
pub(super) fn tax_computation(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    section_start(out, "calculo", "Cálculo del impuesto");
    p(
        out,
        &format!(
            "Liquidación de la {} del ejercicio: los dos grupos se integran por separado, se \
             compensan entre sí y con los saldos negativos de ejercicios anteriores en el orden que \
             marca el régimen, y la escala se aplica {} sobre la base resultante. En las tablas de \
             esta sección {}, de modo que cada columna suma hasta su total; el CSV publica las \
             mismas magnitudes en positivo. {}",
            b("base del ahorro"),
            b("una sola vez"),
            b("lo que resta lleva signo negativo"),
            b("Sólo es vinculante la liquidación de la Administración.")
        ),
    );

    h3(out, "Rendimientos del capital mobiliario");
    let mut rows = vec![line(
        "Dividendos y distribuciones (íntegros)".to_owned(),
        statement.total_dividend_income,
    )];
    if statement.dividend_exemption_limit() > Decimal::ZERO {
        rows.push(line(
            format!(
                "Dividendos exentos — límite de {} EUR anuales (NF 3/2014 art. 9.24)",
                eur(statement.dividend_exemption_limit())
            ),
            -statement.total_dividend_exemption,
        ));
    }
    rows.push(line(
        "Intereses cobrados".to_owned(),
        statement.total_interest_income,
    ));
    rows.push(line(
        "Gastos deducibles de administración y depósito".to_owned(),
        -statement.total_deductible_fees,
    ));
    rows.push(Row::total(vec![
        Cell::text("Rendimiento neto del capital mobiliario"),
        Cell::num(eur(statement.rcm_net)),
    ]));
    table(out, &AMOUNT_COLUMNS, &rows);

    // The ceiling and what it disallowed are figures *about* the deducted fees, not further
    // amounts to subtract: the deduction row above is already net of them. They go beside the
    // table rather than in its column, which has to add up to the total under it.
    if let Some(cap) = statement.custody_fee_cap {
        note(
            out,
            "info",
            &format!(
                "TRLFIRPF art. 32.1.a limita los gastos de administración y depósito al {} de los \
                 ingresos íntegros no exentos procedentes de los valores, que aquí son {} EUR. El \
                 límite excluyó {} EUR, ya descontados de la fila anterior.",
                b(&pct(statement
                    .custody_fee_cap_fraction()
                    .unwrap_or_default())),
                b(&eur(cap)),
                b(&eur(statement.total_capped_fees))
            ),
        );
    }

    h3(out, "Ganancias y pérdidas patrimoniales");
    let mut rows = vec![
        line(
            "Transmisiones de valores (importe integrable)".to_owned(),
            statement.total_capital_gains,
        ),
        line(
            forms::fx_summary_label(statement.regime).to_owned(),
            statement.total_fx_result,
        ),
        line(
            "Pérdidas diferidas reintegradas al transmitirse los valores que las bloqueaban"
                .to_owned(),
            -statement.total_reintegrated_loss,
        ),
    ];
    if statement.small_disposals_exemption > Decimal::ZERO {
        rows.push(line(
            "Incrementos exentos por transmisiones onerosas hasta 3.000 € (TRLFIRPF art. 39.5.d)"
                .to_owned(),
            -statement.small_disposals_exemption,
        ));
    } else if statement.small_disposals_unmeasurable {
        rows.push(Row::data(vec![
            Cell::text(
                "Exención de transmisiones hasta 3.000 € (art. 39.5.d): no aplicada — el importe \
                 global no es medible (véase Avisos)",
            ),
            Cell::num("—"),
        ]));
    }
    rows.push(Row::total(vec![
        Cell::text("Saldo de ganancias y pérdidas patrimoniales"),
        Cell::num(eur(statement.gyp_net)),
    ]));
    table(out, &AMOUNT_COLUMNS, &rows);

    // A deferred loss is not a further subtraction: the transmissions row above is already the
    // integrable result, net of it. It sits beside the table for the same reason as the fee
    // ceiling.
    if statement.total_deferred_loss > Decimal::ZERO {
        note(
            out,
            "info",
            &format!(
                "Además, {} EUR de pérdidas quedaron diferidas por valores homogéneos y no se \
                 integran este ejercicio; ya están descontadas de la fila de transmisiones. Se \
                 detallan en «Valores homogéneos».",
                b(&eur(statement.total_deferred_loss))
            ),
        );
    }

    h3(out, "Compensación");
    let labels = forms::compensation_labels(statement.regime);
    let mut rows = vec![
        line(
            labels.own_rcm.to_owned(),
            -statement.rcm_own_group_losses_applied(),
        ),
        line(
            labels.own_gyp.to_owned(),
            -statement.gyp_own_group_losses_applied(),
        ),
    ];
    // A regime that never lets a saldo leave its own group has no cross to report, and a row that
    // is structurally zero reads as a cross that happened to be nil this year.
    if statement.cross_offset() != CrossOffset::None {
        rows.push(line(
            labels.cross_rcm.to_owned(),
            statement.cross_offset_rcm_to_gyp,
        ));
        rows.push(line(
            labels.cross_gyp.to_owned(),
            statement.cross_offset_gyp_to_rcm,
        ));
        rows.push(line(
            labels.prior_rcm.to_owned(),
            statement.prior_cross_offset_rcm_to_gyp,
        ));
        rows.push(line(
            labels.prior_gyp.to_owned(),
            statement.prior_cross_offset_gyp_to_rcm,
        ));
    }
    rows.push(Row::subtotal(vec![
        Cell::text("RCM integrado en la base del ahorro"),
        Cell::num(eur(statement.rcm_taxable)),
    ]));
    rows.push(Row::subtotal(vec![
        Cell::text("Ganancias integradas en la base del ahorro"),
        Cell::num(eur(statement.gyp_taxable)),
    ]));
    rows.push(Row::total(vec![
        Cell::text("Base liquidable del ahorro"),
        Cell::num(eur(statement.savings_base)),
    ]));
    table(out, &AMOUNT_COLUMNS, &rows);

    h3(out, "Escala del ahorro");
    let columns = [
        col("Tramo desde", Align::Right),
        col("Tipo", Align::Right),
        col("Base en el tramo", Align::Right),
        col("Cuota", Align::Right),
    ];
    let brackets = statement.scale.brackets();
    let base = statement.savings_base;
    let mut rows = Vec::new();
    for (index, &(floor, rate)) in brackets.iter().enumerate() {
        if base <= floor {
            break;
        }
        // The bracket runs to the next floor, or to the base for the last one it reaches: the same
        // slicing `SavingsScale::tax` does, so the column adds up to the cuota it computed.
        let upper = match brackets.get(index + 1) {
            Some(&(next_floor, _)) => std::cmp::min(base, next_floor),
            None => base,
        };
        let slice = upper - floor;
        rows.push(Row::data(vec![
            Cell::num(eur(floor)),
            Cell::num(pct(rate)),
            Cell::num(eur(slice)),
            Cell::num(eur(slice * rate)),
        ]));
    }
    rows.push(Row::total(vec![
        Cell::text("Cuota íntegra del ahorro"),
        Cell::empty(),
        Cell::num(eur(base)),
        Cell::num(eur(statement.savings_quota)),
    ]));
    table(out, &columns, &rows);
    p(
        out,
        &format!(
            "Tipo medio de gravamen del ahorro: {}. Es el que limita la deducción por doble \
             imposición, y se toma redondeado a cuatro decimales tal y como lo fija la escala.",
            b(&pct(statement.average_savings_rate))
        ),
    );

    h3(out, "Deducción por doble imposición internacional");
    let rows = vec![
        line(
            "Retenciones practicadas en el extranjero".to_owned(),
            statement.total_foreign_withholding,
        ),
        line(
            "Renta bruta obtenida en el extranjero".to_owned(),
            statement.foreign_gross_income,
        ),
        line(
            "Renta neta extranjera integrada en la base (base del límite del tipo medio)"
                .to_owned(),
            statement.foreign_taxable_income,
        ),
        line(
            "Deducción por doble imposición internacional".to_owned(),
            statement.total_foreign_tax_credit,
        ),
        Row::total(vec![
            Cell::text("Cuota líquida del ahorro"),
            Cell::num(eur(statement.net_tax_due)),
        ]),
    ];
    table(out, &AMOUNT_COLUMNS, &rows);

    let informational = [
        (
            "Intereses pagados sobre saldo prestado (no deducibles)",
            statement.total_paid_interest,
        ),
        (
            "Comisiones informativas (no deducibles)",
            statement.total_informational_fees,
        ),
        (
            "Resultados de divisa sobre saldo prestado (excluidos — revisión manual)",
            statement.total_fx_borrowed_review,
        ),
    ];
    let rows: Vec<Row> = informational
        .iter()
        .filter(|(_, value)| !value.is_zero())
        .map(|(label, value)| line((*label).to_owned(), *value))
        .collect();
    if !rows.is_empty() {
        h3(out, "Importes informativos (fuera de la base del ahorro)");
        table(out, &AMOUNT_COLUMNS, &rows);
    }

    section_end(out);
    true
}

/// One line of the activity overview.
struct ActivityRow {
    category: &'static str,
    activity: String,
    gain: Decimal,
    loss: Decimal,
    deferred: Decimal,
    group: Group,
}

impl ActivityRow {
    fn new(category: &'static str, activity: impl Into<String>, group: Group) -> ActivityRow {
        ActivityRow {
            category,
            activity: activity.into(),
            gain: Decimal::ZERO,
            loss: Decimal::ZERO,
            deferred: Decimal::ZERO,
            group,
        }
    }

    fn add(&mut self, amount: Decimal) {
        if amount >= Decimal::ZERO {
            self.gain += amount;
        } else {
            self.loss += amount;
        }
    }

    fn net(&self) -> Decimal {
        self.gain + self.loss
    }

    fn is_empty(&self) -> bool {
        self.gain.is_zero() && self.loss.is_zero() && self.deferred.is_zero()
    }
}

/// Asset class per symbol, from the security overview; anything it does not name is a share, which
/// is what the processor assumes too.
fn classes(statement: &SpanishTaxStatement) -> BTreeMap<&str, AssetClass> {
    statement
        .report
        .securities
        .iter()
        .map(|security| (security.symbol.as_str(), security.category))
        .collect()
}

fn class_of(classes: &BTreeMap<&str, AssetClass>, symbol: &str) -> AssetClass {
    classes.get(symbol).copied().unwrap_or(AssetClass::Stock)
}

/// Section: Resumen por actividad y categoría.
pub(super) fn by_activity(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let classes = classes(statement);

    // (asset class, activity order) → row; BTreeMap keeps a stable, readable order.
    let mut by_class: BTreeMap<(AssetClass, u8), ActivityRow> = BTreeMap::new();

    for entry in &statement.dividends {
        let class = class_of(&classes, &entry.symbol);
        by_class
            .entry((class, 0))
            .or_insert_with(|| {
                ActivityRow::new(
                    asset_class_label(class),
                    "Dividendos y distribuciones",
                    Group::Rcm,
                )
            })
            .add(entry.gross_eur);
        if !entry.withheld_eur.is_zero() {
            by_class
                .entry((class, 1))
                .or_insert_with(|| {
                    ActivityRow::new(
                        asset_class_label(class),
                        "Retenciones en origen",
                        Group::Informational,
                    )
                })
                .add(-entry.withheld_eur);
        }
    }
    for entry in &statement.capital_gains {
        let class = class_of(&classes, &entry.symbol);
        let row = by_class.entry((class, 2)).or_insert_with(|| {
            ActivityRow::new(
                asset_class_label(class),
                "Transmisiones de valores",
                Group::Gyp,
            )
        });
        row.add(entry.integrable_amount);
        row.deferred += entry.deferred_loss;
    }
    for entry in &statement.wash_sale_reintegrations {
        let class = class_of(&classes, &entry.symbol);
        by_class
            .entry((class, 3))
            .or_insert_with(|| {
                ActivityRow::new(
                    asset_class_label(class),
                    "Pérdidas diferidas reintegradas",
                    Group::Gyp,
                )
            })
            .add(-entry.released_eur);
    }

    let mut extra: Vec<ActivityRow> = Vec::new();

    let mut exemption = ActivityRow::new(
        "Efectivo",
        "Dividendos exentos (NF 3/2014 art. 9.24)",
        Group::Rcm,
    );
    exemption.add(-statement.total_dividend_exemption);
    extra.push(exemption);

    let mut interest = ActivityRow::new("Efectivo", "Intereses cobrados", Group::Rcm);
    interest.add(statement.total_interest_income);
    extra.push(interest);

    let mut paid = ActivityRow::new(
        "Efectivo",
        "Intereses pagados sobre saldo prestado",
        Group::Informational,
    );
    paid.add(-statement.total_paid_interest);
    extra.push(paid);

    let mut deductible = ActivityRow::new(
        "Efectivo",
        "Comisiones de administración y depósito (deducibles)",
        Group::Rcm,
    );
    deductible.add(-statement.total_deductible_fees);
    extra.push(deductible);

    let mut informational_fees =
        ActivityRow::new("Efectivo", "Comisiones informativas", Group::Informational);
    informational_fees.add(-statement.total_informational_fees);
    extra.push(informational_fees);

    let mut fx = ActivityRow::new("Divisa", "Conversiones sobre saldo propio", Group::Gyp);
    fx.add(statement.total_fx_result);
    extra.push(fx);

    let mut borrowed = ActivityRow::new(
        "Divisa",
        "Conversiones sobre saldo prestado (revisión manual)",
        Group::Informational,
    );
    borrowed.add(statement.total_fx_borrowed_review);
    extra.push(borrowed);

    if statement.small_disposals_exemption > Decimal::ZERO {
        let mut exempt = ActivityRow::new(
            "Valores",
            "Incrementos exentos hasta 3.000 € (TRLFIRPF art. 39.5.d)",
            Group::Gyp,
        );
        exempt.add(-statement.small_disposals_exemption);
        extra.push(exempt);
    }

    let mut grants = ActivityRow::new(
        "Acciones",
        "Adjudicaciones (vesting) — base general",
        Group::Informational,
    );
    for entry in &statement.stock_grants {
        grants.add(entry.value_eur.unwrap_or_default());
    }
    extra.push(grants);

    let all: Vec<&ActivityRow> = by_class
        .values()
        .chain(extra.iter())
        .filter(|row| !row.is_empty())
        .collect();
    if all.is_empty() {
        return false;
    }

    section_start(out, "actividad", "Resumen por actividad y categoría");
    p(
        out,
        &format!(
            "Resumen de todas las operaciones del ejercicio por {} y {}, con el grupo de la base \
             del ahorro al que van. Los importes son los que se integran, ya netos de la parte \
             diferida por valores homogéneos, que se muestra en su propia columna. El signo es el \
             de la cuenta: lo que resta va en negativo. El total suma {}, no las informativas, y \
             es el resultado de los dos grupos {}: la base liquidable está en «Cálculo del \
             impuesto».",
            b("categoría de activo"),
            b("tipo de actividad"),
            b("las filas que llegan a un grupo"),
            b("antes de compensar")
        ),
    );

    let columns = [
        col("Categoría", Align::Left),
        col("Actividad", Align::Left),
        col("Ganancias EUR", Align::Right),
        col("Pérdidas EUR", Align::Right),
        col("Neto EUR", Align::Right),
        col("Diferido EUR", Align::Right),
        col("Grupo", Align::Left),
    ];
    let mut table_rows = Vec::with_capacity(all.len() + 1);
    let (mut total_gain, mut total_loss, mut total_deferred) =
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
    for row in &all {
        // Only what reaches a savings-base group is totalled. Summing the informational rows too
        // would print a bold figure that is no tax quantity at all — a reader would take it for
        // the base. What is left is `rcm_net + gyp_net`, which the section below then compensates.
        if row.group != Group::Informational {
            total_gain += row.gain;
            total_loss += row.loss;
        }
        total_deferred += row.deferred;
        table_rows.push(Row::data(vec![
            Cell::text(row.category),
            Cell::text(&row.activity),
            Cell::num(eur(row.gain)),
            Cell::num(eur(row.loss)),
            Cell::num(eur(row.net())),
            Cell::num(eur(row.deferred)),
            Cell::text(group_label(row.group)),
        ]));
    }
    table_rows.push(Row::total(vec![
        Cell::text("TOTAL — resultado de los dos grupos antes de compensar"),
        Cell::empty(),
        Cell::num(eur(total_gain)),
        Cell::num(eur(total_loss)),
        Cell::num(eur(total_gain + total_loss)),
        Cell::num(eur(total_deferred)),
        Cell::empty(),
    ]));
    table(out, &columns, &table_rows);

    section_end(out);
    true
}

#[derive(Default)]
struct SecurityResult {
    dividends: Decimal,
    withheld: Decimal,
    gain: Decimal,
    loss: Decimal,
    deferred: Decimal,
}

impl SecurityResult {
    fn add_result(&mut self, amount: Decimal) {
        if amount >= Decimal::ZERO {
            self.gain += amount;
        } else {
            self.loss += amount;
        }
    }
}

/// Section: Ganancias y pérdidas por valor.
pub(super) fn by_security(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    if statement.capital_gains.is_empty()
        && statement.dividends.is_empty()
        && statement.wash_sale_reintegrations.is_empty()
    {
        return false;
    }
    let names = statement.report.security_names();
    let classes = classes(statement);

    type Groups = BTreeMap<AssetClass, BTreeMap<(String, String), SecurityResult>>;

    /// The row of one security, keyed by the class it is grouped under and the name it is shown by.
    fn slot<'a>(
        groups: &'a mut Groups,
        classes: &BTreeMap<&str, AssetClass>,
        names: &HashMap<&str, &str>,
        symbol: &str,
        isin: &str,
    ) -> &'a mut SecurityResult {
        let class = class_of(classes, symbol);
        let name = names.get(symbol).copied().unwrap_or(symbol);
        groups
            .entry(class)
            .or_default()
            .entry((name.to_owned(), isin.to_owned()))
            .or_default()
    }

    let mut groups = Groups::new();

    for entry in &statement.dividends {
        let result = slot(&mut groups, &classes, &names, &entry.symbol, &entry.isin);
        result.dividends += entry.gross_eur;
        result.withheld += entry.withheld_eur;
    }

    for entry in &statement.capital_gains {
        let result = slot(&mut groups, &classes, &names, &entry.symbol, &entry.isin);
        result.add_result(entry.integrable_amount);
        result.deferred += entry.deferred_loss;
    }

    for entry in &statement.wash_sale_reintegrations {
        let result = slot(&mut groups, &classes, &names, &entry.symbol, &entry.isin);
        result.add_result(-entry.released_eur);
    }

    section_start(out, "valores", "Ganancias y pérdidas por valor");
    p(
        out,
        &format!(
            "Desglose por valor de todo lo que el ejercicio integró: {} y {}. Sirve para contrastar \
             el informe contra sus propios extractos posición a posición.",
            b("dividendos y sus retenciones"),
            b("resultados de las transmisiones")
        ),
    );

    let columns = [
        col("Valor", Align::Left),
        col("ISIN", Align::Left),
        col("Dividendos brutos", Align::Right),
        col("Retenido", Align::Right),
        col("Ganancias EUR", Align::Right),
        col("Pérdidas EUR", Align::Right),
        col("Diferido EUR", Align::Right),
        col("Neto integrable", Align::Right),
    ];

    for (class, securities) in &groups {
        h3(out, asset_class_label(*class));
        let mut rows = Vec::with_capacity(securities.len() + 1);
        let mut totals = SecurityResult::default();
        for ((name, isin), result) in securities {
            totals.dividends += result.dividends;
            totals.withheld += result.withheld;
            totals.gain += result.gain;
            totals.loss += result.loss;
            totals.deferred += result.deferred;
            rows.push(Row::data(vec![
                Cell::text(name),
                Cell::text(isin),
                Cell::num(eur(result.dividends)),
                Cell::num(eur(-result.withheld)),
                Cell::num(eur(result.gain)),
                Cell::num(eur(result.loss)),
                Cell::num(eur(result.deferred)),
                Cell::num(eur(result.gain + result.loss)),
            ]));
        }
        rows.push(Row::total(vec![
            Cell::text("Total"),
            Cell::empty(),
            Cell::num(eur(totals.dividends)),
            Cell::num(eur(-totals.withheld)),
            Cell::num(eur(totals.gain)),
            Cell::num(eur(totals.loss)),
            Cell::num(eur(totals.deferred)),
            Cell::num(eur(totals.gain + totals.loss)),
        ]));
        table(out, &columns, &rows);
    }

    section_end(out);
    true
}

/// Section: Valores homogéneos.
pub(super) fn wash_sales(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let deferrals: Vec<_> = statement
        .capital_gains
        .iter()
        .filter(|entry| entry.deferred_loss > Decimal::ZERO)
        .collect();

    if deferrals.is_empty()
        && statement.wash_sale_reintegrations.is_empty()
        && statement.deferred_losses_next.is_empty()
        && statement.wash_sale_window_gaps.is_empty()
        && statement.wash_sale_venue_reviews.is_empty()
        && statement.wash_sale_boundary_reviews.is_empty()
        && statement.wash_sale_unpriced_years.is_empty()
    {
        return false;
    }

    section_start(out, "valores-homogeneos", "Valores homogéneos");
    p(
        out,
        &format!(
            "Una pérdida por transmisión {} cuando se adquieren valores homogéneos dentro de los \
             dos meses anteriores o posteriores a la venta ({}). La pérdida no se pierde: se \
             integra a medida que se transmiten los valores que la bloquean.",
            b("no se integra"),
            b(article(statement))
        ),
    );

    if !deferrals.is_empty() {
        h3(out, "Pérdidas diferidas por transmisiones del ejercicio");
        let columns = [
            col("Fecha", Align::Left),
            col("Valor", Align::Left),
            col("ISIN", Align::Left),
            col("Resultado fiscal", Align::Right),
            col("Diferido", Align::Right),
            col("Integrable", Align::Right),
        ];
        let mut rows = Vec::with_capacity(deferrals.len() + 1);
        let mut deferred = Decimal::ZERO;
        for entry in &deferrals {
            deferred += entry.deferred_loss;
            rows.push(Row::data(vec![
                Cell::text(format::date(entry.sale_date)),
                Cell::text(&entry.symbol),
                Cell::text(&entry.isin),
                Cell::num(eur(entry.fiscal_gain_loss)),
                Cell::num(eur(entry.deferred_loss)),
                Cell::num(eur(entry.integrable_amount)),
            ]));
        }
        rows.push(Row::total(vec![
            Cell::text("Total diferido"),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::num(eur(deferred)),
            Cell::empty(),
        ]));
        table(out, &columns, &rows);
    }

    if !statement.wash_sale_reintegrations.is_empty() {
        h3(out, "Pérdidas reintegradas en el ejercicio");
        let columns = [
            col("Fecha de la transmisión que libera", Align::Left),
            col("Valor", Align::Left),
            col("ISIN", Align::Left),
            col("Adquisición bloqueante", Align::Left),
            col("Venta de origen", Align::Left),
            col("Importe reintegrado", Align::Right),
        ];
        let mut rows = Vec::with_capacity(statement.wash_sale_reintegrations.len() + 1);
        for entry in &statement.wash_sale_reintegrations {
            rows.push(Row::data(vec![
                Cell::text(format::date(entry.date)),
                Cell::text(&entry.symbol),
                Cell::text(&entry.isin),
                Cell::text(format::date(entry.acquisition_date)),
                Cell::text(format::date(entry.origin_sale_date)),
                Cell::num(eur(-entry.released_eur)),
            ]));
        }
        rows.push(Row::total(vec![
            Cell::text("Total reintegrado"),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::num(eur(-statement.total_reintegrated_loss)),
        ]));
        table(out, &columns, &rows);
    }

    if !statement.deferred_losses_next.is_empty() {
        h3(
            out,
            &format!("Pérdidas aún bloqueadas a 31/12/{}", statement.year),
        );
        p(
            out,
            "Estas pérdidas siguen bloqueadas por valores homogéneos que no se han transmitido. Se \
             arrastran a la configuración del ejercicio siguiente, en la sección «Compensación y \
             saldos pendientes».",
        );
        let columns = [
            col("Valor", Align::Left),
            col("ISIN", Align::Left),
            col("Cantidad bloqueante", Align::Right),
            col("Fecha de adquisición", Align::Left),
            col("Venta de origen", Align::Left),
            col("Pérdida", Align::Right),
        ];
        let rows: Vec<Row> = statement
            .deferred_losses_next
            .iter()
            .map(|deferred| {
                Row::data(vec![
                    Cell::text(&deferred.symbol),
                    Cell::text(deferred.isin.as_deref().unwrap_or("")),
                    Cell::num(qty(deferred.blocked_quantity)),
                    Cell::text(format::date(deferred.acquisition_date)),
                    Cell::text(format::date(deferred.sale_date)),
                    Cell::num(eur(deferred.loss)),
                ])
            })
            .collect();
        table(out, &columns, &rows);
    }

    for gap in &statement.wash_sale_window_gaps {
        note(
            out,
            "warn",
            &format!(
                "La ventana de valores homogéneos de la pérdida de {} de {} sigue abierta hasta el \
                 {}, más allá del último día del extracto. Una recompra en ese plazo diferiría {} \
                 EUR, que aquí se deducen íntegros: la pérdida puede estar {}. Vuelva a ejecutar la \
                 liquidación cuando el extracto cubra la ventana.",
                escape(&gap.symbol),
                format::date(gap.sale_date),
                format::date(gap.window_end),
                b(&eur(gap.loss_eur)),
                b("sobrevalorada")
            ),
        );
    }

    let literal: Vec<String> = statement
        .wash_sale_boundary_reviews
        .iter()
        .map(|review| review.message())
        .collect();
    if !literal.is_empty() {
        h4(out, "Avisos del cálculo (texto literal)");
        ul(
            out,
            &literal.iter().map(|text| escape(text)).collect::<Vec<_>>(),
        );
    }

    if !statement.wash_sale_venue_reviews.is_empty() {
        let items: Vec<String> = statement
            .wash_sale_venue_reviews
            .iter()
            .map(|review| {
                format!(
                    "{} vendido el {} — cotizado en {}: {} EUR",
                    escape(&review.symbol),
                    format::date(review.sale_date),
                    escape(review.venue.as_deref().unwrap_or("un mercado sin nombrar")),
                    b(&eur(review.loss_eur))
                )
            })
            .collect();
        note(
            out,
            "warn",
            "La deducción de las pérdidas siguientes depende de qué ventana toma el mercado de \
             cotización. Se adquirieron valores homogéneos dentro del año pero fuera de los dos \
             meses, de modo que el plazo de un año (NF 3/2014 art. 43.h / LIRPF art. 33.5.g / \
             TRLFIRPF art. 39.6.g) diferiría el importe indicado. Las consultas DGT V0778-25 y \
             V0951-25 resuelven el plazo de dos meses sólo para mercados amparados por una decisión \
             de equivalencia MiFID II en vigor. La pérdida se deduce íntegra.",
        );
        ul(out, &items);
    }

    if !statement.wash_sale_unpriced_years.is_empty() {
        let years = statement
            .wash_sale_unpriced_years
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        note(
            out,
            "warn",
            &format!(
                "Las ventas de {} no se han contrastado con la regla de valores homogéneos: no se \
                 publica tabla de actualización para esos ejercicios, así que su resultado no pudo \
                 valorarse. Configure <code>taxes.spain.coefficients.&lt;año&gt;</code>, o arrastre \
                 el diferimiento desde la declaración de aquel ejercicio en \
                 <code>taxes.spain.deferred_losses</code>.",
                escape(&years)
            ),
        );
    }

    section_end(out);
    true
}

/// The article that defers a loss under the filer's own statute.
fn article(statement: &SpanishTaxStatement) -> &'static str {
    match statement.regime {
        SpanishTaxRegime::Gipuzkoa => "NF 3/2014 art. 43.g",
        SpanishTaxRegime::Comun => "LIRPF art. 33.5.f",
        SpanishTaxRegime::Navarra => "TRLFIRPF art. 39.6.f",
    }
}

/// Section: Compensación y saldos pendientes.
pub(super) fn compensation(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let groups = [
        (
            "RCM",
            &statement.rcm_ledger_prior,
            &statement.rcm_applied,
            &statement.rcm_ledger_next,
            statement.rcm_expired,
        ),
        (
            "Ganancias",
            &statement.gyp_ledger_prior,
            &statement.gyp_applied,
            &statement.gyp_ledger_next,
            statement.gyp_expired,
        ),
    ];

    let has_ledger = groups.iter().any(|(_, prior, _, next, expired)| {
        !prior.is_empty() || !next.is_empty() || !expired.is_zero()
    });
    if !has_ledger && statement.deferred_losses_next.is_empty() {
        return false;
    }

    section_start(out, "compensacion", "Compensación y saldos pendientes");
    p(
        out,
        &format!(
            "Un saldo negativo de la base del ahorro sólo puede compensarse en los {} \
             siguientes al ejercicio en que se generó, así que el año de origen forma parte del \
             dato: «Aplicable hasta» es el último ejercicio en cuya declaración cabe usarlo. Lo que \
             queda pendiente se arrastra a la configuración del ejercicio siguiente.",
            b(&format!("{CARRYFORWARD_YEARS} años"))
        ),
    );

    if has_ledger {
        let columns = [
            col("Grupo", Align::Left),
            col("Año de origen", Align::Left),
            col("Saldo inicial", Align::Right),
            col("Aplicado", Align::Right),
            col("Pendiente", Align::Right),
            col("Aplicable hasta", Align::Left),
        ];
        let mut rows = Vec::new();
        for (group, prior, applied, next, expired) in groups {
            let years: BTreeSet<i32> = prior
                .balances()
                .keys()
                .chain(next.balances().keys())
                .chain(applied.used_by_year.keys())
                .copied()
                .collect();
            for origin in years {
                let opening = prior.balances().get(&origin).copied().unwrap_or_default();
                let used = applied
                    .used_by_year
                    .get(&origin)
                    .copied()
                    .unwrap_or_default();
                let pending = next.balances().get(&origin).copied().unwrap_or_default();
                rows.push(Row::data(vec![
                    Cell::text(group),
                    Cell::text(origin.to_string()),
                    Cell::num(eur(opening)),
                    Cell::num(eur(used)),
                    Cell::num(eur(pending)),
                    Cell::text((origin + CARRYFORWARD_YEARS).to_string()),
                ]));
            }
            if expired > Decimal::ZERO {
                rows.push(Row::data(vec![
                    Cell::text(group),
                    Cell::text("caducado"),
                    Cell::empty(),
                    Cell::empty(),
                    Cell::num(eur(expired)),
                    Cell::text("—"),
                ]));
            }
        }
        table(out, &columns, &rows);
    }

    let yaml = next_year_config(statement);
    if !yaml.is_empty() {
        h3(
            out,
            &format!("Configuración para el ejercicio {}", statement.year + 1),
        );
        p(
            out,
            "Copie este bloque en su fichero de configuración. Las fechas van en formato ISO \
             porque es el que la configuración lee.",
        );
        pre(out, &yaml);
    }

    section_end(out);
    true
}

/// The `taxes.spain` block the next return starts from — the same figures the console prints, with
/// the enclosing keys so the block can be pasted as it stands.
fn next_year_config(statement: &SpanishTaxStatement) -> String {
    let ledgers = [
        ("rcm", &statement.rcm_ledger_next),
        ("gyp", &statement.gyp_ledger_next),
    ];
    let has_ledgers = ledgers.iter().any(|(_, ledger)| !ledger.is_empty());
    if !has_ledgers && statement.deferred_losses_next.is_empty() {
        return String::new();
    }

    let mut yaml = String::from("taxes:\n  spain:\n");
    if has_ledgers {
        yaml.push_str("    loss_carryforward:\n");
        for (group, ledger) in ledgers {
            if ledger.is_empty() {
                continue;
            }
            let entries = ledger
                .balances()
                .iter()
                .map(|(origin, amount)| format!("{origin}: '{}'", format_eur(*amount)))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(yaml, "      {group}: {{{entries}}}");
        }
    }
    if !statement.deferred_losses_next.is_empty() {
        yaml.push_str("    deferred_losses:\n");
        for deferred in &statement.deferred_losses_next {
            let isin = deferred
                .isin
                .as_deref()
                .map(|isin| format!(", isin: {isin}"))
                .unwrap_or_default();
            let _ = writeln!(
                yaml,
                "      - {{symbol: {}{isin}, loss: '{}', blocked_quantity: {}, \
                 acquisition_date: {}, sale_date: {}}}",
                deferred.symbol,
                format_eur(deferred.loss),
                deferred.blocked_quantity.normalize(),
                format::iso_date(deferred.acquisition_date),
                format::iso_date(deferred.sale_date)
            );
        }
    }
    yaml
}

/// Section: Avisos y observaciones.
pub(super) fn notes(out: &mut String, statement: &SpanishTaxStatement, meta: &ReportMeta) -> bool {
    section_start(out, "avisos", "Avisos y observaciones");

    let year_start = crate::time::Date::from_ymd_opt(statement.year, 1, 1)
        .expect("1 January is always a valid date");
    let year_end = crate::time::Date::from_ymd_opt(statement.year, 12, 31)
        .expect("31 December is always a valid date");
    if meta.period.first_date() > year_start || meta.period.last_date() < year_end {
        note(
            out,
            "warn",
            &format!(
                "⚠️ El extracto no cubre el ejercicio completo ({} – {}). Faltan los rendimientos y \
                 las operaciones fuera de ese periodo.",
                format::date(meta.period.first_date()),
                format::date(meta.period.last_date())
            ),
        );
    }

    if !statement.short_positions.is_empty() {
        let listed: Vec<String> = statement
            .short_positions
            .iter()
            .map(|(symbol, quantity)| format!("{}: {}", escape(symbol), qty(*quantity)))
            .collect();
        note(
            out,
            "warn",
            &format!(
                "⚠️ Posiciones cortas abiertas al cierre del extracto. No se les da tratamiento \
                 automático y necesitan revisión manual: {}.",
                listed.join(", ")
            ),
        );
    }

    if !statement.total_fx_borrowed_review.is_zero() {
        note(
            out,
            "warn",
            &format!(
                "⚠️ {} EUR de resultados de divisa se realizaron sobre un saldo prestado (margen) y \
                 quedan fuera de la base del ahorro a la espera de revisión manual: devolver un \
                 préstamo en divisa no es claramente la transmisión de un elemento patrimonial, y \
                 ni NF 3/2014, ni la LIRPF, ni el TRLFIRPF lo resuelven.",
                b(&eur(statement.total_fx_borrowed_review))
            ),
        );
    }

    if statement.total_dividend_exemption > Decimal::ZERO {
        let classes = classes(statement);
        let funds: Vec<String> = statement
            .dividends
            .iter()
            .filter(|entry| entry.exemption_eligible)
            .filter(|entry| class_of(&classes, &entry.symbol) == AssetClass::Fund)
            .map(|entry| escape(&entry.symbol))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let payers = if funds.is_empty() {
            "Ningún pagador está configurado como fondo en <code>taxes.etf_classification</code>, \
             pero un extracto no distingue por sí solo un fondo de una sociedad."
                .to_owned()
        } else {
            format!(
                "Configurados como fondo en <code>taxes.etf_classification</code>: {}.",
                funds.join(", ")
            )
        };
        note(
            out,
            "warn",
            &format!(
                "⚠️ Se han exonerado {} EUR de dividendos (NF 3/2014 art. 9.24). La exención {} las \
                 distribuciones de instituciones de inversión colectiva (fondos, ETF, SICAV). {} \
                 Revise cada pagador y reduzca la exención a mano si alguno lo es.",
                b(&eur(statement.total_dividend_exemption)),
                b("no alcanza"),
                payers
            ),
        );
    }

    // The calculation warnings are one text for the CSV, the console and this page. Translating
    // them here would let the report and the log say different things about the same figure.
    let literal: Vec<String> = [
        statement.abatement_message(),
        statement.small_disposals_message(),
        statement.custody_fee_cap_message(),
        statement.carried_cross_offset_message(),
        statement.margin_interest_message(),
    ]
    .into_iter()
    .flatten()
    .map(|message| escape(&message))
    .collect();
    if !literal.is_empty() {
        h3(out, "Avisos del cálculo (texto literal)");
        p(
            out,
            "<span class=\"fine\">Reproducidos tal cual los emiten la consola y el CSV, para que \
             las tres superficies no puedan decir cosas distintas del mismo importe.</span>",
        );
        ul(out, &literal);
    }

    let reviews: Vec<String> = statement
        .fees
        .iter()
        .filter_map(|fee| fee.review.as_ref())
        .map(|review| escape(review))
        .collect();
    if !reviews.is_empty() {
        h3(out, "Comisiones sin doctrina asentada (texto literal)");
        ul(out, &reviews);
    }

    let mut position_notes: Vec<String> = Vec::new();
    for entry in &statement.capital_gains {
        if let Some(text) = &entry.notes {
            position_notes.push(format!(
                "{} – {}: {}",
                format::date(entry.sale_date),
                escape(&entry.symbol),
                escape(text)
            ));
        }
    }
    for entry in &statement.dividends {
        if let Some(text) = &entry.notes {
            position_notes.push(format!(
                "{} – {}: {}",
                format::date(entry.date),
                escape(&entry.symbol),
                escape(text)
            ));
        }
    }
    for entry in &statement.interest {
        if let Some(text) = &entry.notes {
            position_notes.push(format!(
                "{} – {}: {}",
                format::date(entry.date),
                escape(&entry.description),
                escape(text)
            ));
        }
    }
    for entry in &statement.fees {
        // A fee whose note is its own review sentence is already listed above.
        if let (Some(text), None) = (&entry.notes, &entry.review) {
            position_notes.push(format!(
                "{} – {}: {}",
                format::date(entry.date),
                escape(&entry.description),
                escape(text)
            ));
        }
    }
    // The notes come from four loops, so identical texts are never adjacent and `dedup` alone
    // would keep both. Order stays chronological within each source, which is how they read.
    let mut seen = std::collections::HashSet::new();
    position_notes.retain(|note| seen.insert(note.clone()));
    if !position_notes.is_empty() {
        h3(out, "Notas por posición");
        ul(out, &position_notes);
    }

    if !statement.stock_grants.is_empty() {
        h3(out, "Adjudicaciones de acciones (base general)");
        let columns = [
            col("Fecha", Align::Left),
            col("Valor", Align::Left),
            col("Descripción", Align::Left),
            col("Cantidad", Align::Right),
            col("Valor EUR", Align::Right),
        ];
        let rows: Vec<Row> = statement
            .stock_grants
            .iter()
            .map(|entry| {
                Row::data(vec![
                    Cell::text(format::date(entry.date)),
                    Cell::text(&entry.symbol),
                    Cell::text(&entry.description),
                    Cell::num(qty(entry.quantity)),
                    // Never computed: a statement without a vest-date FMV has no value to show.
                    Cell::num(entry.value_eur.map(eur).unwrap_or_else(|| "—".to_owned())),
                ])
            })
            .collect();
        table(out, &columns, &rows);
        p(
            out,
            "Un vesting es rendimiento del trabajo y pertenece a la <b>base general</b>, que esta \
             herramienta no calcula. Declárelo por separado. El valor de la fecha de consolidación \
             es el que fija el coste de adquisición de las acciones para cuando se vendan.",
        );
    }

    if !statement.corporate_actions.is_empty() {
        h3(out, "Operaciones societarias");
        let columns = [
            col("Fecha", Align::Left),
            col("Valor", Align::Left),
            col("Descripción", Align::Left),
            col("Tratamiento", Align::Left),
        ];
        let rows: Vec<Row> = statement
            .corporate_actions
            .iter()
            .map(|entry| {
                Row::data(vec![
                    Cell::text(format::date(entry.date)),
                    Cell::text(&entry.symbol),
                    Cell::text(&entry.description),
                    Cell::text(&entry.notes),
                ])
            })
            .collect();
        table(out, &columns, &rows);
    }

    h3(out, "Método y límites");
    ul(
        out,
        &[
            "Las transmisiones se valoran por <b>FIFO</b>: los valores vendidos se imputan a las \
             adquisiciones más antiguas, con las comisiones de compra en el coste y las de venta \
             restando del importe percibido."
                .to_owned(),
            format!(
                "Los importes en divisa se convierten con el <b>tipo de cambio de referencia del \
                 BCE</b> de la fecha de cada apunte. La liquidación en divisa se trata como una \
                 transmisión: {}.",
                b(
                    "el resultado del saldo propio va a ganancias y pérdidas patrimoniales, y el \
                   del saldo prestado queda para revisión manual"
                )
            ),
            format!(
                "La deducción por doble imposición se calcula con un tipo de convenio del {}, que \
                 es el de dividendos de cartera en la mayoría de los convenios de España. No lo es \
                 en todos: el <b>Reino Unido</b> limita el dividendo de cartera al 10% (el 15% sólo \
                 alcanza a los PID de REIT), y un <b>REIT estadounidense</b> pagado a quien posee \
                 más del 10% no goza de beneficio de convenio. Ajústela a mano si su caso es uno de \
                 esos.",
                b(&pct(statement.treaty_rate()))
            ),
            "La <b>categoría de activo</b> (acciones frente a fondos e IIC) sale de \
             <code>taxes.etf_classification</code> por ISIN; lo no clasificado se trata como \
             acciones. Ninguna cifra de la base del ahorro depende de ella."
                .to_owned(),
            "Los <b>derivados, opciones y operaciones a plazo</b> no se liquidan; las líneas \
             correspondientes del extracto se omiten."
                .to_owned(),
            "El cálculo no tiene en cuenta las circunstancias personales del contribuyente ni las \
             rentas ajenas a este bróker."
                .to_owned(),
            "Las decisiones interpretativas abiertas están recogidas en el registro de \
             <code>docs/spain-taxes.md</code>, cada una con la dirección en que puede fallar."
                .to_owned(),
        ],
    );

    section_end(out);
    true
}

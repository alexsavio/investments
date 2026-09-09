//! Detail sections: cash bookings, foreign withholding, the FIFO worksheets, the raw trades, the
//! foreign-currency ledger, the open lots and the security overview.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::taxes::spain::SpanishTaxRegime;
use crate::time::Date;
use crate::types::Decimal;

use super::super::forms;
use super::super::report::{
    AssetClass, BookingKind, BookingRow, FxRow, FxTreatment, OpenLotRow, TradeRow, TradeSide,
    WithholdingRow,
};
use super::super::statement::SpanishTaxStatement;
use super::ReportMeta;
use super::builder::{
    Align, Cell, Row, RowKind, b, col, h3, h4, note, p, section_end, section_start, table, ul,
};
use super::format::{
    self, activity_label, amount, asset_class_label, booking_kind_label, country_name, dec2, eur,
    lot_source_label, qty, rate, side_label, treatment_label,
};

/// A subtotal/total row: `label` in the first column, numbers at the given column indexes.
fn sum_row(kind: RowKind, label: &str, width: usize, values: &[(usize, String)]) -> Row {
    let mut cells: Vec<Cell> = (0..width).map(|_| Cell::empty()).collect();
    cells[0] = Cell::text(label);
    for (index, value) in values {
        cells[*index] = Cell::num(value);
    }
    Row { kind, cells }
}

fn security_heading(out: &mut String, name: &str, isin: &str, symbol: &str) {
    h4(out, name);
    let _ = writeln!(
        out,
        "<p class=\"security-id\">ISIN: {} | Símbolo: {}</p>",
        b(if isin.is_empty() { "–" } else { isin }),
        b(symbol)
    );
}

fn id_cell(id: Option<&String>) -> Cell {
    Cell::text(id.map(String::as_str).unwrap_or(""))
}

/// Section: Movimientos de efectivo.
pub(super) fn bookings(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.bookings.is_empty() {
        return false;
    }

    section_start(out, "movimientos", "Movimientos de efectivo");
    p(
        out,
        &format!(
            "Todos los {} del ejercicio con efecto fiscal: dividendos y distribuciones, \
             retenciones practicadas en origen, intereses y comisiones. Se muestran en la divisa \
             original y en euros, al tipo de cambio de referencia del BCE de la fecha del apunte. \
             El signo es el de la cuenta: los cargos son negativos.",
            b("apuntes de efectivo")
        ),
    );

    let mut groups: BTreeMap<BookingKind, Vec<&BookingRow>> = BTreeMap::new();
    for row in &report.bookings {
        groups.entry(row.kind).or_default().push(row);
    }

    let columns = [
        col("Divisa", Align::Left),
        col("Fecha", Align::Left),
        col("Categoría", Align::Left),
        col("ISIN", Align::Left),
        col("Valor", Align::Left),
        col("Concepto", Align::Left),
        col("Importe", Align::Right),
        col("Tipo BCE", Align::Right),
        col("Importe EUR", Align::Right),
    ];

    for (kind, mut rows_of_kind) in groups {
        rows_of_kind.sort_by_key(|row| row.date);
        h3(out, booking_kind_label(kind));

        let mut rows = Vec::with_capacity(rows_of_kind.len() + 1);
        let mut total = Decimal::ZERO;
        for row in rows_of_kind {
            total += row.amount_eur;
            rows.push(Row::data(vec![
                Cell::text(&row.currency),
                Cell::text(format::date(row.date)),
                Cell::text(row.category.map(asset_class_label).unwrap_or("Efectivo")),
                Cell::text(&row.isin),
                Cell::text(&row.name),
                Cell::text(&row.description),
                Cell::num(dec2(row.amount)),
                Cell::num(rate(row.eur_per_unit)),
                Cell::num(eur(row.amount_eur)),
            ]));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Total",
            columns.len(),
            &[(8, eur(total))],
        ));
        table(out, &columns, &rows);
    }

    section_end(out);
    true
}

/// Section: Retenciones en origen.
pub(super) fn withholding(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.withholding.is_empty() {
        return false;
    }

    section_start(out, "retenciones", "Retenciones en origen");
    p(
        out,
        &format!(
            "Detalle de las {} sobre dividendos y rendimientos asimilados. Es la base de la {}, que \
             evita — hasta donde puede — que la misma renta tribute dos veces.",
            b("retenciones practicadas en el extranjero"),
            b("deducción por doble imposición internacional")
        ),
    );
    h4(out, "Cómo se calcula");
    ul(
        out,
        &[
            format!(
                "La deducción tiene dos límites: lo que el convenio permite retener al Estado de la \
                 fuente ({} sobre el bruto de cada pago), y el tipo medio de gravamen del ahorro \
                 aplicado a la renta neta extranjera integrada en la base. Se toma el menor.",
                b(&format::pct(statement.treaty_rate()))
            ),
            "Lo retenido por encima del límite del convenio <b>no se deduce aquí</b>: se reclama al \
             Estado de la fuente mediante la solicitud de devolución que corresponda."
                .to_owned(),
            "Una renta exenta no soporta impuesto español, así que tampoco genera deducción: la \
             parte exenta se descuenta del bruto sobre el que se mide."
                .to_owned(),
        ],
    );

    let mut groups: BTreeMap<&str, Vec<&WithholdingRow>> = BTreeMap::new();
    for row in &report.withholding {
        groups
            .entry(row.country_code.as_str())
            .or_default()
            .push(row);
    }

    // Each original-currency amount carries its own code instead of one "Divisa" column labelling
    // the row: a broker may withhold in a currency other than the one it paid the dividend in, and
    // a single column would then name the wrong currency for one of the two.
    let columns = [
        col("Fecha", Align::Left),
        col("Valor", Align::Left),
        col("ISIN", Align::Left),
        col("Bruto", Align::Right),
        col("Bruto EUR", Align::Right),
        col("Retenido", Align::Right),
        col("Retenido EUR", Align::Right),
        col("Tipo", Align::Right),
        col("Crédito con límite del convenio", Align::Right),
    ];

    for (code, mut items) in groups {
        items.sort_by_key(|row| row.date);
        h3(
            out,
            &format!(
                "{} ({})",
                country_name(code),
                if code.is_empty() { "–" } else { code }
            ),
        );

        let mut rows = Vec::with_capacity(items.len() + 1);
        let (mut gross_eur, mut withheld_eur, mut creditable) =
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        for row in items {
            gross_eur += row.gross_eur;
            withheld_eur += row.withheld_eur;
            creditable += row.creditable_eur;
            rows.push(Row::data(vec![
                Cell::text(format::date(row.date)),
                Cell::text(&row.name),
                Cell::text(&row.isin),
                Cell::num(amount(row.gross, &row.currency)),
                Cell::num(eur(row.gross_eur)),
                Cell::num(amount(-row.withheld, &row.withheld_currency)),
                Cell::num(eur(-row.withheld_eur)),
                Cell::num(format::pct_observed(row.withholding_rate)),
                Cell::num(eur(row.creditable_eur)),
            ]));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Total",
            columns.len(),
            &[
                (4, eur(gross_eur)),
                (6, eur(-withheld_eur)),
                (8, eur(creditable)),
            ],
        ));
        table(out, &columns, &rows);
    }

    note(
        out,
        "info",
        &format!(
            "La deducción del ejercicio es de {} EUR: el límite del convenio, fila a fila, acotado \
             además por el tipo medio de gravamen del ahorro sobre la renta neta extranjera \
             integrada en la base. La columna de esta tabla muestra sólo el primero de los dos \
             límites.",
            b(&eur(statement.total_foreign_tax_credit))
        ),
    );

    section_end(out);
    true
}

/// Cut-off of the abatement regimes: TRLFIRPF DT 7.ª reaches elements acquired *before*
/// 31 December 1994, so an acquisition on that day itself is outside it.
fn abatement_cutoff() -> Date {
    Date::from_ymd_opt(1994, 12, 31).expect("31 December 1994 is always a valid date")
}

/// Section: Ganancias y pérdidas patrimoniales (FIFO).
pub(super) fn sales(out: &mut String, statement: &SpanishTaxStatement, _meta: &ReportMeta) -> bool {
    let report = &statement.report;
    if report.sales.is_empty() {
        return false;
    }

    // The worksheet of `capital_gains[i]` is `report.sales[i]`, and their lots line up index for
    // index: the processor pushes both in one iteration. The figures this section reads from the
    // entry — the actualized cost, the deferred part, each lot's coefficient — would otherwise be
    // printed against a different disposal, which is worse than not printing them. Refuse loudly
    // rather than guess.
    let paired = report.sales.len() == statement.capital_gains.len()
        && report
            .sales
            .iter()
            .zip(&statement.capital_gains)
            .all(|(sale, entry)| {
                sale.symbol == entry.symbol
                    && sale.sale_date == entry.sale_date
                    && sale.lots.len() == entry.lots.len()
            });
    if !paired {
        section_start(
            out,
            "transmisiones",
            "Ganancias y pérdidas patrimoniales (FIFO)",
        );
        note(
            out,
            "warn",
            "No se ha podido emparejar cada hoja de cálculo FIFO con su ganancia o pérdida \
             patrimonial, así que esta sección se omite para no atribuir a una transmisión los \
             importes de otra. Los resultados por transmisión están en «Cálculo del impuesto» y en \
             el CSV. Comunique este fallo.",
        );
        section_end(out);
        return true;
    }

    // Only Gipuzkoa actualizes an acquisition cost; elsewhere the coefficient is a constant 1 and
    // two columns of it would be noise.
    let actualizes = statement.regime == SpanishTaxRegime::Gipuzkoa;

    section_start(
        out,
        "transmisiones",
        "Ganancias y pérdidas patrimoniales (FIFO)",
    );
    p(
        out,
        &format!(
            "Resultado de cada transmisión del ejercicio, con los lotes que la {} consumió debajo. \
             El importe percibido va neto de la comisión de venta y el coste incluye la de compra; \
             ambos se convierten a euros al tipo de cambio del BCE de la fecha valor.",
            b("regla FIFO")
        ),
    );
    h4(out, "Cómo leer cada bloque");
    let mut bullets = vec![
        "La fila <b>Venta</b> es la transmisión: cantidad, precio, importe percibido, coste de \
         adquisición y resultado fiscal."
            .to_owned(),
        "Las filas de <b>lote</b> son las adquisiciones que la venta consumió, en orden de \
         antigüedad, con el importe percibido repartido a prorrata de la cantidad."
            .to_owned(),
        "<b>Diferido</b> es la parte de una pérdida que la regla de valores homogéneos bloquea; \
         <b>Integrable</b> es lo que realmente entra en la base de este ejercicio."
            .to_owned(),
    ];
    if actualizes {
        bullets.push(
            "El <b>coeficiente</b> actualiza el coste de cada lote según su año de adquisición (NF \
             3/2014 art. 45.2). Lo elige el año de la <i>venta</i>: una transmisión de 2025 se \
             actualiza con la tabla de 2025 aunque la declaración sea de otro ejercicio."
                .to_owned(),
        );
    }
    ul(out, &bullets);

    let mut columns = vec![
        col("Fecha", Align::Left),
        col("ID", Align::Left),
        col("Tipo", Align::Left),
        col("Cantidad", Align::Right),
        col("Precio", Align::Right),
        col("Bruto", Align::Right),
        col("Comisión", Align::Right),
        col("Tipo BCE", Align::Right),
        col("Ingreso EUR", Align::Right),
        col("Coste EUR", Align::Right),
    ];
    if actualizes {
        columns.push(col("Coeficiente", Align::Right));
        columns.push(col("Coste actualizado", Align::Right));
    }
    columns.push(col("Resultado", Align::Right));
    columns.push(col("Diferido", Align::Right));
    columns.push(col("Integrable", Align::Right));
    columns.push(col("Días", Align::Right));

    let width = columns.len();
    // Column indexes of the totalled money columns, which move with the optional pair.
    let (proceeds_at, cost_at) = (8, 9);
    let actualized_at = 11;
    let (result_at, deferred_at, integrable_at) = if actualizes {
        (12, 13, 14)
    } else {
        (10, 11, 12)
    };

    let cutoff = abatement_cutoff();
    let mut has_old_lot = false;

    let mut groups: BTreeMap<AssetClass, BTreeMap<(&str, &str, &str), Vec<usize>>> =
        BTreeMap::new();
    for (index, sale) in report.sales.iter().enumerate() {
        groups
            .entry(sale.category)
            .or_default()
            .entry((sale.name.as_str(), sale.symbol.as_str(), sale.isin.as_str()))
            .or_default()
            .push(index);
    }

    for (class, securities) in &groups {
        h3(out, asset_class_label(*class));
        for ((name, symbol, isin), indexes) in securities {
            security_heading(out, name, isin, symbol);

            let mut rows = Vec::new();
            let (mut proceeds, mut cost, mut actualized, mut result, mut deferred, mut integrable) = (
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::ZERO,
                Decimal::ZERO,
            );

            for &index in indexes {
                let sale = &report.sales[index];
                let entry = &statement.capital_gains[index];
                proceeds += sale.proceeds_eur;
                cost += sale.cost_basis_eur;
                actualized += entry.actualized_cost_eur;
                result += entry.fiscal_gain_loss;
                deferred += entry.deferred_loss;
                integrable += entry.integrable_amount;

                let mut cells = vec![
                    Cell::text(format::date(sale.sale_date)),
                    id_cell(sale.trade_id.as_ref()),
                    Cell::text("Venta"),
                    Cell::num(qty(-sale.quantity)),
                    Cell::num(dec2(sale.price)),
                    Cell::num(dec2(sale.gross)),
                    Cell::num(dec2(-sale.commission)),
                    Cell::num(rate(sale.eur_per_unit)),
                    Cell::num(eur(sale.proceeds_eur)),
                    Cell::num(eur(sale.cost_basis_eur)),
                ];
                if actualizes {
                    // The coefficient is a per-lot figure; only the actualized total belongs here.
                    cells.push(Cell::empty());
                    cells.push(Cell::num(eur(entry.actualized_cost_eur)));
                }
                cells.push(Cell::num(eur(entry.fiscal_gain_loss)));
                cells.push(Cell::num(eur(entry.deferred_loss)));
                cells.push(Cell::num(eur(entry.integrable_amount)));
                cells.push(Cell::empty());
                rows.push(Row::data(cells));

                for (lot, detail) in sale.lots.iter().zip(&entry.lots) {
                    let old = lot.open_date < cutoff;
                    has_old_lot |= old;
                    let source = if old {
                        format!("{} · anterior a 1995", lot_source_label(lot.source))
                    } else {
                        lot_source_label(lot.source).to_owned()
                    };

                    let mut cells = vec![
                        Cell::text(format::date(lot.open_date)),
                        id_cell(lot.open_trade_id.as_ref()),
                        Cell::text(source),
                        Cell::num(qty(lot.quantity)),
                        Cell::num(lot.price.map(dec2).unwrap_or_default()),
                        Cell::empty(),
                        Cell::empty(),
                        Cell::empty(),
                        Cell::num(eur(lot.proceeds_eur)),
                        Cell::num(eur(lot.cost_eur)),
                    ];
                    if actualizes {
                        cells.push(Cell::num(format::coefficient(detail.coefficient)));
                        cells.push(Cell::num(eur(detail.actualized_cost_eur)));
                    }
                    cells.push(Cell::num(eur(lot.gain_loss_eur)));
                    cells.push(Cell::empty());
                    cells.push(Cell::empty());
                    cells.push(Cell::num(format::days(lot.holding_days)));
                    rows.push(Row::lot(cells));
                }
            }

            // The actualized cost is the column the result is measured from, so leaving it blank
            // would make the bold row read as `ingreso − coste`, which is a different number: on
            // the Gipuzkoa fixture 49.500 − 27.000 = 22.500 against a result of 19.692.
            let mut totals = vec![
                (proceeds_at, eur(proceeds)),
                (cost_at, eur(cost)),
                (result_at, eur(result)),
                (deferred_at, eur(deferred)),
                (integrable_at, eur(integrable)),
            ];
            if actualizes {
                totals.push((actualized_at, eur(actualized)));
            }
            rows.push(sum_row(
                RowKind::Subtotal,
                "Total del valor",
                width,
                &totals,
            ));
            table(out, &columns, &rows);
        }
    }

    note(
        out,
        "info",
        &format!(
            "Totales del ejercicio: resultado integrable de las transmisiones {} EUR y pérdidas \
             diferidas por valores homogéneos {} EUR.",
            b(&eur(statement.total_capital_gains)),
            b(&eur(statement.total_deferred_loss))
        ),
    );

    if has_old_lot {
        // Only Navarra keeps an abatement regime the tool declines to compute, so only its report
        // carries the literal warning this would otherwise send every filer to look for.
        let pointer = match statement.regime {
            SpanishTaxRegime::Navarra => {
                " El aviso literal del cálculo, con la norma aplicable, está en «Avisos y observaciones»."
            }
            _ => "",
        };
        note(
            out,
            "warn",
            &format!(
                "Alguno de los lotes consumidos se adquirió antes del 31 de diciembre de 1994. Los \
                 regímenes de abatimiento que alcanzan a esas adquisiciones {} aquí, de modo que el \
                 resultado mostrado puede estar sobrevalorado.{pointer}",
                b("no se calculan")
            ),
        );
    }

    section_end(out);
    true
}

/// Section: Operaciones con valores.
pub(super) fn trades(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.trades.is_empty() {
        return false;
    }

    section_start(out, "operaciones", "Operaciones con valores");
    p(
        out,
        &format!(
            "Todas las {} del ejercicio, tal y como llegan del extracto. Es la documentación en \
             bruto: el resultado fiscal de cada venta está en «Ganancias y pérdidas patrimoniales \
             (FIFO)». El signo es el de la cuenta: las compras son negativas y las ventas \
             positivas.",
            b("compras y ventas")
        ),
    );

    let mut groups: BTreeMap<(&str, &str, &str), Vec<&TradeRow>> = BTreeMap::new();
    for trade in &report.trades {
        groups
            .entry((
                trade.name.as_str(),
                trade.symbol.as_str(),
                trade.isin.as_str(),
            ))
            .or_default()
            .push(trade);
    }

    let columns = [
        col("Divisa", Align::Left),
        col("Tipo BCE", Align::Right),
        col("Fecha", Align::Left),
        col("Fecha valor", Align::Left),
        col("ID", Align::Left),
        col("Operación", Align::Left),
        col("Cantidad", Align::Right),
        col("Precio", Align::Right),
        col("Bruto", Align::Right),
        col("Comisión", Align::Right),
        col("Neto", Align::Right),
        col("Importe EUR", Align::Right),
    ];

    for ((name, symbol, isin), trades) in &groups {
        security_heading(out, name, isin, symbol);
        let mut rows = Vec::with_capacity(trades.len() + 1);
        let (mut quantity, mut gross, mut commission, mut net, mut amount_eur) = (
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        );
        for trade in trades {
            let signed_quantity = match trade.side {
                TradeSide::Buy => trade.quantity,
                TradeSide::Sell => -trade.quantity,
            };
            quantity += signed_quantity;
            gross += trade.gross;
            commission += trade.commission;
            net += trade.net;
            amount_eur += trade.amount_eur;
            rows.push(Row::data(vec![
                Cell::text(&trade.currency),
                Cell::num(rate(trade.eur_per_unit)),
                Cell::text(format::date(trade.date)),
                Cell::text(format::date(trade.settle_date)),
                id_cell(trade.trade_id.as_ref()),
                Cell::text(side_label(trade.side)),
                Cell::num(qty(signed_quantity)),
                Cell::num(dec2(trade.price)),
                Cell::num(dec2(trade.gross)),
                Cell::num(dec2(trade.commission)),
                Cell::num(dec2(trade.net)),
                Cell::num(eur(trade.amount_eur)),
            ]));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Total",
            columns.len(),
            &[
                (6, qty(quantity)),
                (8, dec2(gross)),
                (9, dec2(commission)),
                (10, dec2(net)),
                (11, eur(amount_eur)),
            ],
        ));
        table(out, &columns, &rows);
    }

    section_end(out);
    true
}

fn treatment_order(treatment: FxTreatment) -> u8 {
    match treatment {
        FxTreatment::HeldBalance => 0,
        FxTreatment::BorrowedBalance => 1,
    }
}

/// Section: Diferencias de cambio.
pub(super) fn fx(out: &mut String, statement: &SpanishTaxStatement, _meta: &ReportMeta) -> bool {
    let report = &statement.report;
    if report.fx_rows.is_empty() {
        return false;
    }

    section_start(out, "divisas", "Diferencias de cambio");
    p(
        out,
        &format!(
            "Gastar divisa — para comprar valores, pagar una comisión o volver a euros — es la {} \
             de un elemento patrimonial, así que realiza el resultado acumulado desde que esa \
             divisa entró en la cuenta. Cada divisa lleva su propia cola FIFO.",
            b("transmisión")
        ),
    );
    ul(
        out,
        &[
            format!(
                "El resultado sobre un {} entra en ganancias y pérdidas patrimoniales, junto con \
                 las transmisiones de valores.",
                b("saldo propio")
            ),
            format!(
                "El resultado sobre un {} queda fuera de la base y se marca para revisión manual: \
                 devolver un préstamo en divisa no es claramente la transmisión de un elemento \
                 patrimonial, y ninguna de las tres normas lo resuelve.",
                b("saldo prestado (margen)")
            ),
            "La cola de cada divisa <b>empieza a cero</b> al inicio del extracto: no se arrastra \
             un saldo de apertura. Un extracto que empieza con divisa ya en la cuenta mostrará por \
             tanto un saldo prestado que en realidad no lo es."
                .to_owned(),
        ],
    );

    let mut groups: BTreeMap<&str, Vec<&FxRow>> = BTreeMap::new();
    for row in &report.fx_rows {
        groups.entry(row.currency.as_str()).or_default().push(row);
    }

    let columns = [
        col("Fecha", Align::Left),
        col("ID", Align::Left),
        col("Actividad", Align::Left),
        col("Unidades", Align::Right),
        col("Tipo BCE", Align::Right),
        col("Importe EUR", Align::Right),
        col("Adquisición", Align::Left),
        col("Tipo adq.", Align::Right),
        col("Valor adq. EUR", Align::Right),
        col("Resultado EUR", Align::Right),
        col("Días", Align::Right),
        col("Saldo", Align::Right),
        col("Tratamiento", Align::Left),
    ];

    for (currency, items) in &groups {
        h3(out, &format!("{currency} — cuenta en divisa"));
        let mut rows = Vec::with_capacity(items.len() + 3);
        let (mut units, mut amount_eur, mut result) = (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        let mut by_treatment: BTreeMap<u8, (FxTreatment, Decimal)> = BTreeMap::new();
        for row in items {
            units += row.units;
            amount_eur += row.amount_eur;
            if let (Some(gain), Some(treatment)) = (row.gain_loss_eur, row.treatment) {
                result += gain;
                by_treatment
                    .entry(treatment_order(treatment))
                    .or_insert((treatment, Decimal::ZERO))
                    .1 += gain;
            }
            rows.push(Row::data(vec![
                Cell::text(format::date(row.date)),
                Cell::text(&row.transaction_id),
                Cell::text(activity_label(&row.activity_code)),
                Cell::num(dec2(row.units)),
                Cell::num(rate(row.eur_per_unit)),
                Cell::num(eur(row.amount_eur)),
                Cell::text(row.open_date.map(format::date).unwrap_or_default()),
                Cell::num(row.open_eur_per_unit.map(rate).unwrap_or_default()),
                Cell::num(row.open_value_eur.map(eur).unwrap_or_default()),
                Cell::num(row.gain_loss_eur.map(eur).unwrap_or_default()),
                Cell::num(row.holding_days.map(format::days).unwrap_or_default()),
                Cell::num(dec2(row.balance_after)),
                Cell::text(row.treatment.map(treatment_label).unwrap_or("")),
            ]));
        }
        for (treatment, total) in by_treatment.values() {
            rows.push(sum_row(
                RowKind::Subtotal,
                &format!("Total — {}", treatment_label(*treatment)),
                columns.len(),
                &[(9, eur(*total))],
            ));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Total",
            columns.len(),
            &[(3, dec2(units)), (5, eur(amount_eur)), (9, eur(result))],
        ));
        table(out, &columns, &rows);
    }

    note(
        out,
        "info",
        &format!(
            "Resultado neto sobre saldo propio: {} EUR, que entra en ganancias y pérdidas \
             patrimoniales. Sobre saldo prestado: {} EUR, excluidos a la espera de revisión manual.",
            b(&eur(statement.total_fx_result)),
            b(&eur(statement.total_fx_borrowed_review))
        ),
    );
    if statement.regime == SpanishTaxRegime::Navarra {
        note(out, "warn", forms::fx_summary_label(statement.regime));
    }

    section_end(out);
    true
}

/// Section: Posiciones abiertas a 31/12.
pub(super) fn open_lots(
    out: &mut String,
    statement: &SpanishTaxStatement,
    meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.open_lots.is_empty() {
        return false;
    }
    let as_of = report
        .open_lots_as_of
        .map(format::date)
        .unwrap_or_else(|| format!("31/12/{}", statement.year));

    section_start(
        out,
        "posiciones-abiertas",
        &format!("Posiciones abiertas a {as_of}"),
    );
    p(
        out,
        &format!(
            "Lotes de compra que seguían sin transmitirse a {as_of}, cada uno con su {}. Son los \
             que la regla FIFO consumirá en las transmisiones de los ejercicios siguientes.",
            b("coste de adquisición en euros")
        ),
    );

    let year_end =
        Date::from_ymd_opt(statement.year, 12, 31).expect("31 December is always a valid date");
    if meta.period.last_date() > year_end {
        note(
            out,
            "warn",
            &format!(
                "El extracto llega hasta el {} y va por tanto más allá del ejercicio. Las ventas \
                 posteriores a {as_of} ya están descontadas de estos saldos.",
                format::date(meta.period.last_date())
            ),
        );
    }

    let mut groups: BTreeMap<AssetClass, BTreeMap<(&str, &str, &str), Vec<&OpenLotRow>>> =
        BTreeMap::new();
    for lot in &report.open_lots {
        groups
            .entry(lot.category)
            .or_default()
            .entry((lot.name.as_str(), lot.symbol.as_str(), lot.isin.as_str()))
            .or_default()
            .push(lot);
    }

    let columns = [
        col("Valor", Align::Left),
        col("ISIN", Align::Left),
        col("Fecha de adquisición", Align::Left),
        col("ID", Align::Left),
        col("Origen", Align::Left),
        col("Cantidad", Align::Right),
        col("Precio", Align::Right),
        col("Coste EUR", Align::Right),
    ];

    for (class, securities) in &groups {
        h3(out, asset_class_label(*class));
        let mut rows = Vec::new();
        let mut class_cost = Decimal::ZERO;
        for ((name, symbol, isin), lots) in securities {
            let (mut quantity, mut cost) = (Decimal::ZERO, Decimal::ZERO);
            for lot in lots {
                quantity += lot.quantity;
                cost += lot.cost_eur;
                let price = lot
                    .price
                    .map(|price| format!("{} {}", dec2(price), lot.currency))
                    .unwrap_or_default();
                rows.push(Row::data(vec![
                    Cell::text(name),
                    Cell::text(isin),
                    Cell::text(format::date(lot.open_date)),
                    id_cell(lot.trade_id.as_ref()),
                    Cell::text(lot_source_label(lot.source)),
                    Cell::num(qty(lot.quantity)),
                    Cell::num(price),
                    Cell::num(eur(lot.cost_eur)),
                ]));
            }
            class_cost += cost;
            // Named by symbol as well: two tickers can share an ISIN-less name, and two blocks
            // labelled "Total — {name}" would be indistinguishable.
            rows.push(sum_row(
                RowKind::Subtotal,
                &format!("Total — {name} ({symbol})"),
                columns.len(),
                &[(5, qty(quantity)), (7, eur(cost))],
            ));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Total",
            columns.len(),
            &[(7, eur(class_cost))],
        ));
        table(out, &columns, &rows);
    }

    section_end(out);
    true
}

/// Section: Relación de valores.
pub(super) fn securities(
    out: &mut String,
    statement: &SpanishTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.securities.is_empty() {
        return false;
    }

    section_start(out, "relacion-valores", "Relación de valores");
    p(
        out,
        &format!(
            "Todos los valores negociados, mantenidos o que pagaron rendimientos en el ejercicio, \
             con sus datos identificativos y la {} que usa este informe.",
            b("categoría")
        ),
    );
    h4(out, "⚠️ Sobre la categoría y el país");
    ul(
        out,
        &[
            "La <b>categoría</b> (acciones frente a fondos e IIC) sale de \
             <code>taxes.etf_classification</code> por ISIN; lo no clasificado se trata como \
             acciones. Ninguna cifra de la base del ahorro depende de ella: sirve para agrupar el \
             informe y para señalar los pagadores a los que la exención de dividendos no alcanza."
                .to_owned(),
            "El <b>país</b> se deduce del prefijo del ISIN y no siempre coincide con la residencia \
             del emisor (los ETF domiciliados en Irlanda son el caso habitual)."
                .to_owned(),
            "Los <b>productos complejos</b> (productos estructurados, royalty trusts, limited \
             partnerships o MLP) pueden necesitar una revisión manual de su calificación fiscal."
                .to_owned(),
        ],
    );

    let columns = [
        col("Símbolo", Align::Left),
        col("ISIN", Align::Left),
        col("Nombre", Align::Left),
        col("País", Align::Left),
        col("Divisa", Align::Left),
        col("Categoría", Align::Left),
    ];
    let rows: Vec<Row> = report
        .securities
        .iter()
        .map(|security| {
            Row::data(vec![
                Cell::text(&security.symbol),
                Cell::text(&security.isin),
                Cell::text(&security.name),
                Cell::text(if security.country_code.is_empty() {
                    String::new()
                } else {
                    format!(
                        "{} ({})",
                        country_name(&security.country_code),
                        security.country_code
                    )
                }),
                Cell::text(&security.currency),
                Cell::text(asset_class_label(security.category)),
            ])
        })
        .collect();
    table(out, &columns, &rows);

    section_end(out);
    true
}

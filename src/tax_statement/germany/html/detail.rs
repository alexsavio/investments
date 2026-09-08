//! Detail sections: cash bookings, withholding tax, FIFO sale worksheets, raw trades, the
//! foreign-currency ledger, open lots and the security overview.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::taxes::germany::TeilfreistellungRate;
use crate::time::Date;
use crate::types::Decimal;

use super::super::report_details::{
    AssetCategory, BookingKind, BookingRow, FxRow, FxTreatment, LotSource, OpenLotRow, SaleLotRow,
    SaleWorksheet, TradeRow, TradeSide, WithholdingRow,
};
use super::super::statement::GermanTaxStatement;
use super::ReportMeta;
use super::builder::{
    Align, Cell, Row, RowKind, b, col, h3, h4, note, p, section_end, section_start, table, ul,
};
use super::format::{
    self, activity_label, booking_kind_label, category_label, country_name, dec2, eur,
    lot_source_label, qty, rate, side_label, teilfreistellung_label, treatment_label,
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
        "<p class=\"security-id\">ISIN: {} | Symbol: {}</p>",
        b(if isin.is_empty() { "–" } else { isin }),
        b(symbol)
    );
}

fn id_cell(id: Option<&String>) -> Cell {
    Cell::text(id.map(String::as_str).unwrap_or(""))
}

/// Section: Barwirksame Buchungen.
pub(super) fn bookings(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.bookings.is_empty() {
        return false;
    }

    section_start(out, "buchungen", "Barwirksame Buchungen");
    p(
        out,
        &format!(
            "Die folgenden Tabellen enthalten alle {} des Steuerjahres, die für die steuerliche \
             Betrachtung relevant sind: Dividenden und Ausschüttungen, einbehaltene \
             Quellensteuer, Guthabenzinsen, Gebühren sowie sonstige Zuflüsse. Beträge in \
             Originalwährung und – zum EZB-Referenzkurs des Buchungstags – in EUR.",
            b("barwirksamen Buchungen")
        ),
    );

    let mut groups: BTreeMap<BookingKind, Vec<&BookingRow>> = BTreeMap::new();
    for row in &report.bookings {
        groups.entry(row.kind).or_default().push(row);
    }

    let columns = [
        col("Währung", Align::Left),
        col("Datum", Align::Left),
        col("Asset-Kategorie", Align::Left),
        col("ISIN", Align::Left),
        col("Name", Align::Left),
        col("Beschreibung", Align::Left),
        col("Betrag", Align::Right),
        col("EZB-Kurs", Align::Right),
        col("Betrag EUR", Align::Right),
    ];

    for (kind, mut bookings) in groups {
        bookings.sort_by_key(|row| row.date);
        h3(out, booking_kind_label(kind));

        let mut rows = Vec::with_capacity(bookings.len() + 1);
        let mut total = dec!(0);
        for row in bookings {
            total += row.amount_eur;
            rows.push(Row::data(vec![
                Cell::text(&row.currency),
                Cell::text(format::date(row.date)),
                Cell::text(row.category.map(category_label).unwrap_or("Cash")),
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
            "Gesamt",
            columns.len(),
            &[(8, eur(total))],
        ));
        table(out, &columns, &rows);
    }

    section_end(out);
    true
}

/// Section: Quellensteuer-Übersicht.
pub(super) fn withholding(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.withholding.is_empty() {
        return false;
    }

    section_start(out, "quellensteuer", "Quellensteuer-Übersicht");
    p(
        out,
        &format!(
            "Diese Tabelle zeigt eine detaillierte Aufstellung aller im Steuerjahr einbehaltenen \
             {} auf Dividenden und vergleichbare Erträge. Sie dient dazu, die {} zu ermitteln und \
             die korrekte Eintragung in der Steuererklärung vorzubereiten.",
            b("ausländischen Quellensteuern"),
            b("anrechenbaren Beträge")
        ),
    );
    h4(out, "Hinweise");
    ul(
        out,
        &[
            "Die aufgeführten Werte sind für die Anrechnung in der <b>Anlage KAP</b> (Zeile 41) \
             relevant und helfen, eine doppelte Besteuerung – soweit möglich – zu vermeiden."
                .to_owned(),
            "Angerechnet wird höchstens der typische DBA-Satz von <b>15 %</b> des Bruttoertrags \
             und höchstens 25 % des steuerpflichtigen Betrags (§32d Abs. 5 EStG). Darüber hinaus \
             einbehaltene Steuer (z. B. Schweiz 35 %, Frankreich) ist nur per Erstattungsantrag im \
             Quellenstaat zurückzuholen."
                .to_owned(),
            "Quellensteuer auf <b>Fondsausschüttungen</b> ist nach InvStG nicht anrechenbar; sie \
             wird informativ mit 0,00 EUR anrechenbar ausgewiesen."
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

    let columns = [
        col("Währung", Align::Left),
        col("Datum", Align::Left),
        col("Asset-Kategorie", Align::Left),
        col("ISIN", Align::Left),
        col("Name", Align::Left),
        col("Brutto", Align::Right),
        col("Brutto EUR", Align::Right),
        col("QSt", Align::Right),
        col("QSt EUR", Align::Right),
        col("QSt-Satz", Align::Right),
        col("Anrechnungsgrenze", Align::Right),
        col("Anrechenbar EUR", Align::Right),
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
        let (mut gross_eur, mut withheld_eur, mut creditable) = (dec!(0), dec!(0), dec!(0));
        for row in items {
            gross_eur += row.gross_eur;
            withheld_eur += row.withheld_eur;
            creditable += row.creditable_eur;
            let cap = match row.category {
                AssetCategory::Stock => "15 %".to_owned(),
                _ => "0 % (InvStG)".to_owned(),
            };
            rows.push(Row::data(vec![
                Cell::text(&row.currency),
                Cell::text(format::date(row.date)),
                Cell::text(category_label(row.category)),
                Cell::text(&row.isin),
                Cell::text(&row.name),
                Cell::num(dec2(row.gross)),
                Cell::num(eur(row.gross_eur)),
                Cell::num(dec2(-row.withheld)),
                Cell::num(eur(-row.withheld_eur)),
                Cell::num(format::pct_observed(row.withholding_rate)),
                Cell::num(cap),
                Cell::num(eur(row.creditable_eur)),
            ]));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Gesamt",
            columns.len(),
            &[
                (6, eur(gross_eur)),
                (8, eur(-withheld_eur)),
                (11, eur(creditable)),
            ],
        ));
        table(out, &columns, &rows);
    }

    note(
        out,
        "info",
        &format!(
            "Summe anrechenbar → Anlage KAP Zeile 41: {} EUR.",
            b(&eur(statement.kap_zeile_41))
        ),
    );

    section_end(out);
    true
}

fn lot_notes(lot: &SaleLotRow) -> String {
    let altbestand_cutoff =
        Date::from_ymd_opt(2009, 1, 1).expect("1 January 2009 is always a valid date");

    let mut parts: Vec<&str> = Vec::new();
    if lot.open_date < altbestand_cutoff {
        parts.push("Altbestand (vor 2009): Ergebnis steuerfrei");
    }
    match lot.source {
        LotSource::Trade => {}
        LotSource::Grant => parts.push("Zuteilung: Anschaffungskosten = Marktwert bei Vesting"),
        LotSource::CorporateAction => parts.push("Kapitalmaßnahme"),
    }
    parts.join("; ")
}

/// Section: Gewinne und Verluste aus Wertpapiergeschäften (FIFO worksheets).
pub(super) fn sales(out: &mut String, statement: &GermanTaxStatement, _meta: &ReportMeta) -> bool {
    let report = &statement.report;
    if report.sales.is_empty() {
        return false;
    }

    section_start(
        out,
        "wertpapiergeschaefte",
        "Gewinne und Verluste aus Wertpapiergeschäften",
    );
    p(
        out,
        &format!(
            "Diese Tabelle zeigt die nach dem FIFO-Prinzip („First In – First Out“) ermittelten \
             {} Gewinne und Verluste des Steuerjahres über alle steuerlich relevanten \
             Wertpapierverkäufe. Beträge werden in EUR zum EZB-Referenzkurs des jeweiligen \
             Valutatags umgerechnet; Kommissionen mindern Anschaffungskosten bzw. Erlöse. \
             Quellensteuern werden separat ausgewiesen und nicht den Anschaffungskosten \
             zugerechnet.",
            b("realisierten")
        ),
    );
    h4(out, "Berechnungslogik in Kürze");
    ul(
        out,
        &[
            "<b>Anschaffung:</b> Kauf/Zuteilung mit Anschaffungskosten in EUR (Kaufpreis + \
             Kommissionen) wird in den FIFO-Stack aufgenommen."
                .to_owned(),
            "<b>Veräußerung:</b> Verkauf → Veräußerungswert in EUR (Verkaufspreis − Kommissionen)."
                .to_owned(),
            "<b>FIFO-Zuordnung:</b> Veräußerte Stücke werden den ältesten offenen Anschaffungen \
             zugeordnet (ggf. auf mehrere Tranchen aufgeteilt; der Erlös wird anteilig verteilt)."
                .to_owned(),
            "<b>Ergebnis:</b> Gewinn/Verlust = Veräußerungswert − Anschaffungskosten. Nicht \
             veräußerte Restbestände verbleiben im FIFO-Stack (siehe Offene Positionen)."
                .to_owned(),
        ],
    );
    h4(out, "Besonderheiten");
    ul(
        out,
        &[
            "<b>Zuteilungen (RSU):</b> Anschaffungskosten = Marktwert bei Vesting (bereits als \
             Arbeitslohn versteuert)."
                .to_owned(),
            "<b>Altbestand:</b> Vor 2009 angeschaffte Tranchen sind steuerfrei; ihr Ergebnis wird \
             ausgewiesen, aber nicht in die Formularwerte übernommen."
                .to_owned(),
            "<b>Verlustverrechnung</b> gemäß deutscher Regelung (Aktien-Topf vs. allgemeiner Topf) \
             erfolgt außerhalb dieser Tabelle."
                .to_owned(),
        ],
    );

    let mut groups: BTreeMap<AssetCategory, BTreeMap<(&str, &str, &str), Vec<&SaleWorksheet>>> =
        BTreeMap::new();
    for sale in &report.sales {
        groups
            .entry(sale.category)
            .or_default()
            .entry((sale.name.as_str(), sale.symbol.as_str(), sale.isin.as_str()))
            .or_default()
            .push(sale);
    }

    let columns = [
        col("Datum", Align::Left),
        col("ID", Align::Left),
        col("Aktivität", Align::Left),
        col("O/C", Align::Left),
        col("Transaktionstyp", Align::Left),
        col("Menge", Align::Right),
        col("Betrag EUR", Align::Right),
        col("Eröffnungsdatum", Align::Left),
        col("Eröffnungs-ID", Align::Left),
        col("Eröffnungswert (EUR)", Align::Right),
        col("G/V EUR", Align::Right),
        col("Haltedauer (d)", Align::Right),
        col("Hinweise", Align::Left),
    ];

    for (category, securities) in &groups {
        h3(out, category_label(*category));
        for ((name, symbol, isin), sales) in securities {
            security_heading(out, name, isin, symbol);
            let mut sales: Vec<&SaleWorksheet> = sales.clone();
            sales.sort_by_key(|sale| sale.sale_date);

            let mut rows = Vec::new();
            let (mut quantity, mut proceeds, mut cost, mut gain) =
                (dec!(0), dec!(0), dec!(0), dec!(0));
            for sale in sales {
                quantity += sale.quantity;
                proceeds += sale.proceeds_eur;
                cost += sale.cost_basis_eur;
                gain += sale.gain_loss_eur;

                let sale_notes = sale.notes.clone().unwrap_or_default();
                if let [lot] = sale.lots.as_slice() {
                    let mut notes = lot_notes(lot);
                    if !sale_notes.is_empty() {
                        if !notes.is_empty() {
                            notes.push_str("; ");
                        }
                        notes.push_str(&sale_notes);
                    }
                    rows.push(Row::data(vec![
                        Cell::text(format::date(sale.sale_date)),
                        id_cell(sale.trade_id.as_ref()),
                        Cell::text("Veräußerung"),
                        Cell::text("Close"),
                        Cell::text("Verkauf"),
                        Cell::num(qty(-sale.quantity)),
                        Cell::num(eur(sale.proceeds_eur)),
                        Cell::text(format::date(lot.open_date)),
                        id_cell(lot.open_trade_id.as_ref()),
                        Cell::num(eur(lot.cost_eur)),
                        Cell::num(eur(sale.gain_loss_eur)),
                        Cell::num(format::days(lot.holding_days)),
                        Cell::text(notes),
                    ]));
                } else {
                    rows.push(Row::data(vec![
                        Cell::text(format::date(sale.sale_date)),
                        id_cell(sale.trade_id.as_ref()),
                        Cell::text("Veräußerung"),
                        Cell::text("Close"),
                        Cell::text("Verkauf"),
                        Cell::num(qty(-sale.quantity)),
                        Cell::num(eur(sale.proceeds_eur)),
                        Cell::text(format!("{} Tranchen", sale.lots.len())),
                        Cell::empty(),
                        Cell::num(eur(sale.cost_basis_eur)),
                        Cell::num(eur(sale.gain_loss_eur)),
                        Cell::empty(),
                        Cell::text(sale_notes),
                    ]));
                    for lot in &sale.lots {
                        rows.push(Row::lot(vec![
                            Cell::empty(),
                            Cell::empty(),
                            Cell::text("davon Tranche"),
                            Cell::empty(),
                            Cell::text(lot_source_label(lot.source)),
                            Cell::num(qty(-lot.quantity)),
                            Cell::num(eur(lot.proceeds_eur)),
                            Cell::text(format::date(lot.open_date)),
                            id_cell(lot.open_trade_id.as_ref()),
                            Cell::num(eur(lot.cost_eur)),
                            Cell::num(eur(lot.gain_loss_eur)),
                            Cell::num(format::days(lot.holding_days)),
                            Cell::text(lot_notes(lot)),
                        ]));
                    }
                }
            }
            rows.push(sum_row(
                RowKind::Subtotal,
                "Gesamt – Veräußerungen",
                columns.len(),
                &[
                    (5, qty(-quantity)),
                    (6, eur(proceeds)),
                    (9, eur(cost)),
                    (10, eur(gain)),
                ],
            ));
            table(out, &columns, &rows);
        }
    }

    p(
        out,
        "<span class=\"fine\">Bei Verkäufen aus mehreren Tranchen wird der Erlös anteilig nach \
         Stückzahl verteilt; maßgeblich ist das Gesamtergebnis des Verkaufs.</span>",
    );

    section_end(out);
    true
}

/// Section: Wertpapiertransaktionen (raw trades).
pub(super) fn trades(out: &mut String, statement: &GermanTaxStatement, _meta: &ReportMeta) -> bool {
    let report = &statement.report;
    if report.trades.is_empty() {
        return false;
    }

    section_start(out, "wertpapiertransaktionen", "Wertpapiertransaktionen");
    p(
        out,
        &format!(
            "Diese Tabelle zeigt alle {} aus Ihrem Konto im chronologischen Detail. Sie dient \
             als vollständige Dokumentation aller Kauf- und Verkaufsorders im Steuerjahr.",
            b("einzelnen Wertpapiertransaktionen")
        ),
    );
    h4(out, "Was enthält diese Übersicht?");
    ul(
        out,
        &[
            "<b>Handelsinformationen:</b> Datum, Valuta, Symbol, ISIN, Wertpapiername".to_owned(),
            "<b>Order-Details:</b> Kauf/Verkauf, Anzahl der Anteile, Preis pro Anteil".to_owned(),
            "<b>Kosten &amp; Gebühren:</b> Kommissionen".to_owned(),
            "<b>Währungen:</b> Originalwährung und umgerechnete Beträge in EUR (EZB-Kurs des \
             Valutatags)"
                .to_owned(),
            "<b>Transaktions-IDs:</b> Eindeutige Identifikationsnummern des Brokers, soweit im \
             Kontoauszug enthalten"
                .to_owned(),
        ],
    );
    p(
        out,
        &format!(
            "{} Diese Tabelle zeigt die Rohdaten aller Trades. Die steuerlich relevanten \
             Berechnungen (FIFO-Gewinne und -Verluste) finden Sie in der Tabelle „Gewinne und \
             Verluste aus Wertpapiergeschäften“. Vorzeichen aus Sicht des Kontos: Käufe negativ, \
             Verkäufe positiv.",
            b("Hinweis:")
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
        col("Währung", Align::Left),
        col("EZB-Kurs", Align::Right),
        col("Datum", Align::Left),
        col("Valuta", Align::Left),
        col("ID", Align::Left),
        col("Art", Align::Left),
        col("Menge", Align::Right),
        col("Kurs", Align::Right),
        col("Erlös", Align::Right),
        col("Gebühren", Align::Right),
        col("Netto", Align::Right),
        col("Betrag EUR", Align::Right),
    ];

    for ((name, symbol, isin), trades) in &groups {
        security_heading(out, name, isin, symbol);
        let mut rows = Vec::with_capacity(trades.len() + 1);
        let (mut quantity, mut gross, mut commission, mut net, mut amount_eur) =
            (dec!(0), dec!(0), dec!(0), dec!(0), dec!(0));
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
            "Gesamt",
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
        FxTreatment::Section20Taxable => 0,
        FxTreatment::LoanRepaymentNonTaxable => 1,
        FxTreatment::Section23ShortTerm => 2,
        FxTreatment::Section23LongTermTaxFree => 3,
        FxTreatment::Section23BorrowedReview => 4,
    }
}

/// Section: Fremdwährungsgewinne/-verluste.
pub(super) fn fx(out: &mut String, statement: &GermanTaxStatement, _meta: &ReportMeta) -> bool {
    let report = &statement.report;
    if report.fx_rows.is_empty() {
        return false;
    }

    section_start(out, "fremdwaehrung", "Fremdwährungsgewinne/-verluste");
    p(
        out,
        &format!(
            "Diese Tabelle zeigt realisierte Gewinne oder Verluste aus der Verwendung von \
             Fremdwährungen (z. B. USD, GBP, CAD), die zuvor auf einem Fremdwährungskonto beim \
             Broker gehalten wurden. Steuerlich wird zwischen {} und {} Fremdwährungskonten \
             unterschieden:",
            b("verzinslichen"),
            b("unverzinslichen")
        ),
    );
    ul(
        out,
        &[
            "<b>Verzinsliche Fremdwährungskonten (§20 EStG):</b> Guthaben, auf die der Broker \
             grundsätzlich Habenzinsen vorsieht, gelten steuerlich als Kapitalanlagen. Gewinne \
             und Verluste unterliegen daher der Abgeltungsteuer. Verluste können innerhalb des \
             allgemeinen Verlusttopfs mit anderen positiven Einkünften aus Kapitalvermögen (z. B. \
             Zinsen, Dividenden, Fondsgewinne) verrechnet werden."
                .to_owned(),
            "<b>Unverzinsliche Fremdwährungskonten (§23 EStG):</b> Guthaben, für die der Broker \
             generell keine Verzinsung vorsieht, gelten als private Veräußerungsgeschäfte. \
             Gewinne sind hier nur steuerpflichtig, wenn zwischen Anschaffung und Verwendung \
             weniger als ein Jahr liegt. Verluste sind steuerlich nicht anrechenbar."
                .to_owned(),
        ],
    );
    h4(out, "Wie werden Fremdwährungsgewinne berechnet?");
    p(
        out,
        &format!(
            "Jede Ausgabe von Fremdwährung (z. B. für einen Wertpapierkauf, eine Gebühr oder den \
             Rücktausch in Euro) gilt steuerlich als {}. Ein Gewinn entsteht, wenn der Euro-Wert \
             zum Zeitpunkt der Verwendung höher ist als beim ursprünglichen Zufluss. Auch wenn \
             kein Euro tatsächlich bewegt wird, z. B. bei einem Kauf in USD, entsteht ein \
             sogenannter {}. Die Zuordnung erfolgt nach FIFO je Währung; ein negativer Bestand \
             ist ein Fremdwährungskredit, dessen Tilgung nicht steuerbar ist (BMF 19.05.2022 Rz. \
             131).",
            b("Veräußerung"),
            b("verdeckter Fremdwährungsgewinn")
        ),
    );
    h4(out, "Umrechnungskurse bei Fremdwährungsgeschäften");
    ul(
        out,
        &[
            "<b>Tatsächliche Währungstransaktionen:</b> Bei echten Währungsumtäuschen (z. B. \
             USD → EUR) wird der <i>tatsächlich erzielte Umrechnungskurs</i> aus der \
             Broker-Transaktion verwendet."
                .to_owned(),
            "<b>Verdeckte Fremdwährungsgewinne:</b> Bei Transaktionen ohne direkten \
             Währungsumtausch (z. B. Wertpapierkauf in USD ohne EUR-Umtausch) wird der \
             <i>EZB-Referenzkurs</i> des jeweiligen Transaktionsdatums herangezogen."
                .to_owned(),
        ],
    );

    let mut groups: BTreeMap<&str, Vec<&FxRow>> = BTreeMap::new();
    for row in &report.fx_rows {
        groups.entry(row.currency.as_str()).or_default().push(row);
    }

    let columns = [
        col("Datum", Align::Left),
        col("ID", Align::Left),
        col("Aktivität", Align::Left),
        col("Betrag (FX)", Align::Right),
        col("EZB-Kurs", Align::Right),
        col("Betrag EUR", Align::Right),
        col("Eröffnungsdatum", Align::Left),
        col("Eröffnungswert (EUR)", Align::Right),
        col("FX G/V EUR", Align::Right),
        col("Bestand FX", Align::Right),
        col("Haltedauer (d)", Align::Right),
        col("Behandlung", Align::Left),
    ];

    for (currency, items) in &groups {
        h3(out, &format!("{currency} – Fremdwährungskonto"));
        let mut rows = Vec::with_capacity(items.len() + 3);
        let (mut units, mut amount_eur, mut gain) = (dec!(0), dec!(0), dec!(0));
        let mut by_treatment: BTreeMap<u8, (FxTreatment, Decimal)> = BTreeMap::new();
        for row in items {
            units += row.units;
            amount_eur += row.amount_eur;
            if let (Some(result), Some(treatment)) = (row.gain_loss_eur, row.treatment) {
                gain += result;
                by_treatment
                    .entry(treatment_order(treatment))
                    .or_insert((treatment, dec!(0)))
                    .1 += result;
            }
            rows.push(Row::data(vec![
                Cell::text(format::date(row.date)),
                Cell::text(&row.transaction_id),
                Cell::text(activity_label(&row.activity_code)),
                Cell::num(dec2(row.units)),
                Cell::num(rate(row.eur_per_unit)),
                Cell::num(eur(row.amount_eur)),
                Cell::text(row.open_date.map(format::date).unwrap_or_default()),
                Cell::num(row.open_value_eur.map(eur).unwrap_or_default()),
                Cell::num(row.gain_loss_eur.map(eur).unwrap_or_default()),
                Cell::num(dec2(row.balance_after)),
                Cell::num(row.holding_days.map(format::days).unwrap_or_default()),
                Cell::text(row.treatment.map(treatment_label).unwrap_or("")),
            ]));
        }
        for (treatment, result) in by_treatment.values() {
            rows.push(sum_row(
                RowKind::Subtotal,
                &format!("Gesamt – {}", treatment_label(*treatment)),
                columns.len(),
                &[(8, eur(*result))],
            ));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Gesamt",
            columns.len(),
            &[(3, dec2(units)), (5, eur(amount_eur)), (8, eur(gain))],
        ));
        table(out, &columns, &rows);
    }

    let mut summary = format!(
        "Steuerformular-Werte (alle Währungen): §20 Fremdwährungsgewinne {} EUR und -verluste \
         {} EUR (→ KAP Zeile 19 / 22)",
        b(&eur(statement.total_fx_gains)),
        b(&eur(statement.total_fx_losses))
    );
    if statement.non_taxable_margin_fx != dec!(0) {
        let _ = write!(
            summary,
            "; nicht steuerbar (Tilgung Fremdwährungskredit) {} EUR",
            b(&eur(statement.non_taxable_margin_fx))
        );
    }
    let section23 = &statement.section23;
    if !section23.is_empty() {
        let _ = write!(
            summary,
            "; §23 kurzfristig netto {} EUR, langfristig steuerfrei {} EUR, Fremdwährungskredit \
             zu prüfen {} EUR",
            b(&eur(section23.short_term_net())),
            b(&eur(section23.long_term_tax_free)),
            b(&eur(section23.borrowed_review))
        );
    }
    summary.push('.');
    note(out, "info", &summary);

    section_end(out);
    true
}

/// Section: Offene Positionen zum Jahresende.
pub(super) fn open_lots(
    out: &mut String,
    statement: &GermanTaxStatement,
    meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.open_lots.is_empty() {
        return false;
    }
    let as_of = report
        .open_lots_as_of
        .map(format::date)
        .unwrap_or_else(|| format!("31.12.{}", statement.year));

    section_start(
        out,
        "offene-positionen",
        &format!("Offene Positionen zum Jahresende – verbleibende Bestände zum {as_of}"),
    );
    p(
        out,
        &format!(
            "Diese Tabelle zeigt alle Wertpapierpositionen, die am Ende des Steuerjahres noch im \
             Depot gehalten wurden. Sie dokumentiert, welche Bestände in das Folgejahr übertragen \
             werden – inklusive ihrer ursprünglichen steuerlichen Anschaffungskosten. Jede Zeile \
             entspricht einer noch offenen Kauf-Tranche im Wertpapier-FIFO-Stack ({}).",
            b("§20 Abs. 4 Satz 7 EStG")
        ),
    );
    h4(out, "Spalten");
    ul(
        out,
        &[
            "<b>Anschaffungsdatum:</b> Ursprüngliches Anschaffungsdatum der Tranche.".to_owned(),
            "<b>ID:</b> Transaktionsnummer der ursprünglichen Anschaffung, soweit bekannt."
                .to_owned(),
            "<b>Transaktionstyp:</b> Kauf, Zuteilung (RSU) oder Kapitalmaßnahme.".to_owned(),
            "<b>Menge:</b> Anzahl der Stücke, die zum Stichtag noch im Bestand sind.".to_owned(),
            "<b>Betrag EUR:</b> Steuerlich relevante Anschaffungskosten dieser Tranche in Euro \
             (inkl. Kaufgebühren, EZB-Referenzkurs des Anschaffungstags)."
                .to_owned(),
        ],
    );
    h4(out, "Hinweis zur Vorabpauschale");
    p(
        out,
        &format!(
            "Für Investmentfonds (ETFs) nach dem InvStG bilden diese Jahresendbestände die \
             Grundlage für die Berechnung der Vorabpauschale gemäß {} im Folgejahr.",
            b("§18 InvStG")
        ),
    );

    let year_end = crate::time::Date::from_ymd_opt(statement.year, 12, 31).unwrap();
    if meta.period.last_date() > year_end {
        note(
            out,
            "warn",
            &format!(
                "Der Kontoauszug reicht bis {} und damit über das Steuerjahr hinaus. Verkäufe nach \
                 dem {as_of} sind von den Beständen bereits abgezogen.",
                format::date(meta.period.last_date())
            ),
        );
    }

    let mut groups: BTreeMap<AssetCategory, BTreeMap<(&str, &str, &str), Vec<&OpenLotRow>>> =
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
        col("Name", Align::Left),
        col("ISIN", Align::Left),
        col("Anschaffungsdatum", Align::Left),
        col("ID", Align::Left),
        col("Transaktionstyp", Align::Left),
        col("Menge", Align::Right),
        col("Kurs", Align::Right),
        col("Betrag EUR", Align::Right),
    ];

    for (category, securities) in &groups {
        h3(out, category_label(*category));
        let mut rows = Vec::new();
        let mut category_cost = dec!(0);
        for ((name, _symbol, isin), lots) in securities {
            let (mut quantity, mut cost) = (dec!(0), dec!(0));
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
            category_cost += cost;
            rows.push(sum_row(
                RowKind::Subtotal,
                &format!("Gesamt – {name}"),
                columns.len(),
                &[(5, qty(quantity)), (7, eur(cost))],
            ));
        }
        rows.push(sum_row(
            RowKind::Total,
            "Gesamt",
            columns.len(),
            &[(7, eur(category_cost))],
        ));
        table(out, &columns, &rows);
    }

    section_end(out);
    true
}

/// Section: Wertpapierübersicht.
pub(super) fn securities(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    let report = &statement.report;
    if report.securities.is_empty() {
        return false;
    }

    section_start(out, "wertpapieruebersicht", "Wertpapierübersicht");
    p(
        out,
        &format!(
            "Diese Tabelle zeigt eine {}, die im Steuerjahr gehandelt oder gehalten wurden. Für \
             jedes Wertpapier werden die wichtigsten Identifikationsmerkmale und die steuerliche \
             Klassifizierung dargestellt.",
            b("vollständige Übersicht aller Wertpapiere")
        ),
    );
    h4(out, "⚠️ Hinweise zur Kategorisierung");
    ul(
        out,
        &[
            "Die <b>Asset-Kategorisierung</b> (Aktie vs. Fondsart) folgt der Konfiguration je \
             ISIN (<code>taxes.etf_classification</code>); nicht klassifizierte Wertpapiere gelten \
             als Aktien."
                .to_owned(),
            "Bei <b>komplexen Finanzprodukten</b> (z. B. strukturierte Produkte, Royalty Trusts, \
             Limited Partnerships oder MLP) kann eine manuelle Überprüfung der steuerlichen \
             Einordnung erforderlich sein."
                .to_owned(),
            "Das <b>Land</b> wird aus dem ISIN-Präfix abgeleitet und entspricht nicht immer dem \
             Sitz des Emittenten (z. B. irische ETF-Vehikel)."
                .to_owned(),
        ],
    );

    let columns = [
        col("Symbol", Align::Left),
        col("ISIN", Align::Left),
        col("Name", Align::Left),
        col("Land", Align::Left),
        col("Währung", Align::Left),
        col("Asset-Kategorie", Align::Left),
        col("Teilfreistellung (InvStG)", Align::Left),
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
                Cell::text(category_label(security.category)),
                Cell::text(teilfreistellung_label(TeilfreistellungRate::from(
                    security.category,
                ))),
            ])
        })
        .collect();
    table(out, &columns, &rows);

    section_end(out);
    true
}

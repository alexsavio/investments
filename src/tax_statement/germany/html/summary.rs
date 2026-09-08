//! Summary sections: tax-form lines, the tax computation, the activity/asset-class overview, the
//! per-security results, the Vorabpauschale and the notes.

use std::collections::BTreeMap;

use crate::taxes::germany::TeilfreistellungRate;
use crate::types::Decimal;

use super::super::kap_inv_zeilen;
use super::super::report_details::AssetCategory;
use super::super::statement::{GermanTaxStatement, KapInvGroup, Section23};
use super::builder::{
    Align, Cell, Row, b, col, escape, h3, note, p, section_end, section_start, table, ul,
};
use super::format::{self, category_label, eur, pct, qty};
use super::{ReportMeta, security_names};

fn line(label: String, value: Decimal) -> Row {
    Row::data(vec![Cell::raw(label), Cell::num(eur(value))])
}

fn fund_group_label(rate: TeilfreistellungRate) -> &'static str {
    match rate {
        TeilfreistellungRate::Equity => "Aktienfonds",
        TeilfreistellungRate::Mixed => "Mischfonds",
        TeilfreistellungRate::Bond => "sonstige Investmentfonds",
        TeilfreistellungRate::None => "Aktien",
    }
}

fn kap_inv_groups(statement: &GermanTaxStatement) -> [(TeilfreistellungRate, KapInvGroup); 3] {
    [
        (TeilfreistellungRate::Equity, statement.kap_inv_equity),
        (TeilfreistellungRate::Mixed, statement.kap_inv_mixed),
        (TeilfreistellungRate::Bond, statement.kap_inv_other),
    ]
}

/// Section: Übersicht für die Steuerformulare.
pub(super) fn tax_forms(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    section_start(out, "steuerformulare", "Übersicht für die Steuerformulare");
    p(
        out,
        &format!(
            "Diese Tabelle zeigt eine {}, aufgeschlüsselt nach den {} (Anlage KAP, Anlage KAP-INV \
             und Anlage SO). Die Werte sind das Ergebnis der Auswertung Ihrer Transaktionsdaten. \
             Sie berücksichtigen u. a.:",
            b("Jahresübersicht aller steuerlich relevanten Beträge"),
            b("Zeilen der deutschen Steuerformulare")
        ),
    );
    ul(
        out,
        &[
            format!(
                "{} wie Dividenden, Zinsen und Gewinne aus Aktienveräußerungen,",
                b("Kapitaleinkünfte")
            ),
            format!(
                "{}, getrennt nach Aktienverkäufen und sonstigen Kapitalanlagen (§20 Abs. 6 EStG),",
                b("Verluste")
            ),
            format!(
                "{} (§32d Abs. 5 EStG, pauschal bis 15 % des Bruttoertrags),",
                b("Anrechenbare ausländische Quellensteuer")
            ),
            format!(
                "{} (Ausschüttungen, Veräußerungsgewinne, Vorabpauschale) nach InvStG,",
                b("Investmentfonds-Erträge")
            ),
            format!(
                "{}, z. B. Fremdwährungsgewinne eines unverzinslichen Kontos.",
                b("Sonstige Einkünfte nach §23 EStG")
            ),
        ],
    );

    let columns = [
        col("Steuerformular", Align::Left),
        col("Betrag (EUR)", Align::Right),
    ];
    let mut rows = Vec::new();

    rows.push(line(
        "Anlage KAP Zeile 19 – Ausländische Kapitalerträge (ohne Beträge aus den Zeilen 26a und 52)"
            .to_owned(),
        statement.kap_zeile_19,
    ));
    if statement.kap_zeile_20 != dec!(0) {
        rows.push(line(
            "Anlage KAP Zeile 20 – In Zeile 19 enthaltene Gewinne aus Aktienveräußerungen i. S. d. \
             §20 Abs. 2 Satz 1 Nr. 1 EStG"
                .to_owned(),
            statement.kap_zeile_20,
        ));
    }
    if statement.kap_zeile_22 != dec!(0) {
        rows.push(line(
            "Anlage KAP Zeile 22 – In den Zeilen 18 und 19 enthaltene Verluste ohne Verluste aus \
             der Veräußerung von Aktien"
                .to_owned(),
            statement.kap_zeile_22,
        ));
    }
    if statement.kap_zeile_23 != dec!(0) {
        rows.push(line(
            "Anlage KAP Zeile 23 – In den Zeilen 18 und 19 enthaltene Verluste aus der Veräußerung \
             von Aktien i. S. d. §20 Abs. 2 Satz 1 Nr. 1 EStG"
                .to_owned(),
            statement.kap_zeile_23,
        ));
    }
    if statement.kap_zeile_41 != dec!(0) {
        rows.push(line(
            "Anlage KAP Zeile 41 – Anrechenbare noch nicht angerechnete ausländische Steuern"
                .to_owned(),
            statement.kap_zeile_41,
        ));
    }

    let mut has_kap_inv = false;
    for (rate, group) in kap_inv_groups(statement) {
        let Some((zeile_dist, _zeile_vap, zeile_sale)) = kap_inv_zeilen(rate) else {
            continue;
        };
        let label = fund_group_label(rate);
        if group.distributions != dec!(0) {
            has_kap_inv = true;
            rows.push(line(
                format!("Anlage KAP-INV Zeile {zeile_dist} – Ausschüttungen {label} (brutto)"),
                group.distributions,
            ));
        }
        let net_sale = group.sale_gains - group.sale_losses;
        if group.sale_gains != dec!(0) || group.sale_losses != dec!(0) {
            has_kap_inv = true;
            rows.push(line(
                format!(
                    "Anlage KAP-INV Zeile {zeile_sale} – Gewinne/Verluste aus der Veräußerung von \
                     {label} (brutto, netto aus Gewinnen {} und Verlusten {})",
                    eur(group.sale_gains),
                    eur(group.sale_losses)
                ),
                net_sale,
            ));
        }
    }

    let section23 = &statement.section23;
    if !section23.is_empty() {
        rows.push(line(
            "Anlage SO Zeilen 41–47 – Private Veräußerungsgeschäfte (§23 EStG, Fremdwährung ≤ 1 \
             Jahr): Gewinne"
                .to_owned(),
            section23.short_term_gains,
        ));
        rows.push(line(
            "Anlage SO Zeilen 41–47 – Private Veräußerungsgeschäfte (§23 EStG, Fremdwährung ≤ 1 \
             Jahr): Verluste"
                .to_owned(),
            -section23.short_term_losses,
        ));
        rows.push(line(
            format!(
                "Anlage SO – Netto (Freigrenze {}: {} für alle privaten Veräußerungsgeschäfte des \
                 Jahres)",
                statement.year,
                eur(Section23::freigrenze(statement.year))
            ),
            section23.short_term_net(),
        ));
        if section23.long_term_tax_free != dec!(0) {
            rows.push(line(
                "ℹ Steuerfrei – Fremdwährungsgewinne mit mehr als einem Jahr Haltedauer (§23 Abs. 1 \
                 Nr. 2 EStG)"
                    .to_owned(),
                section23.long_term_tax_free,
            ));
        }
        if section23.borrowed_review != dec!(0) {
            rows.push(line(
                "⚠️ Prüfen – Realisierung aus Fremdwährungskredit (§23; die §20-Ausnahme gilt hier \
                 nicht)"
                    .to_owned(),
                section23.borrowed_review,
            ));
        }
    }

    if statement.total_stock_grant_income != dec!(0) {
        rows.push(line(
            "Anlage N – Arbeitslohn: geldwerter Vorteil aus Aktienzuteilungen (Marktwert bei \
             Vesting)"
                .to_owned(),
            statement.total_stock_grant_income,
        ));
    }
    if statement.total_cash_grant_income != dec!(0) {
        rows.push(line(
            "Anlage SO – Sonstige Einkünfte (§22 Nr. 3 EStG): Bargeldzuwendungen (Freigrenze 256 \
             EUR)"
                .to_owned(),
            statement.total_cash_grant_income,
        ));
    }
    if statement.non_taxable_margin_fx != dec!(0) {
        rows.push(line(
            "ℹ Nicht steuerbar – Tilgung eines Fremdwährungskredits (Margin Loan)".to_owned(),
            statement.non_taxable_margin_fx,
        ));
    }
    if statement.total_vorabpauschale_gross != dec!(0) {
        let deemed = statement
            .vorabpauschale
            .first()
            .map(|entry| format::date(entry.deemed_received))
            .unwrap_or_default();
        rows.push(line(
            format!(
                "ℹ Vorabpauschale {} (§18 InvStG, brutto) – Zufluss am {deemed}, in der \
                 Steuererklärung {} anzugeben",
                statement.year,
                statement.year + 1
            ),
            statement.total_vorabpauschale_gross,
        ));
    }

    table(out, &columns, &rows);

    if has_kap_inv {
        note(
            out,
            "info",
            "Anlage KAP-INV verlangt Bruttowerte; das Finanzamt wendet die Teilfreistellung \
             (§20 InvStG) selbst an. Die Steuerberechnung in diesem Bericht berücksichtigt sie \
             bereits.",
        );
    }

    section_end(out);
    true
}

/// Section: Steuerberechnung (tool extra).
pub(super) fn tax_computation(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    section_start(
        out,
        "steuerberechnung",
        "Steuerberechnung (Abgeltungsteuer)",
    );
    p(
        out,
        &format!(
            "Schätzung der Abgeltungsteuer nach §32d EStG auf die in diesem Bericht ermittelten \
             Kapitalerträge: Verlustverrechnung nach Töpfen (§20 Abs. 6 EStG), \
             Sparer-Pauschbetrag, Anrechnung ausländischer Quellensteuer (§32d Abs. 5 EStG). \
             {}",
            b("Maßgeblich ist allein die Festsetzung durch das Finanzamt.")
        ),
    );

    let columns = [
        col("Position", Align::Left),
        col("Betrag (EUR)", Align::Right),
    ];
    let rates = &statement.tax_rates;
    let mut rows = vec![
        line(
            "Veräußerungsgewinne".to_owned(),
            statement.total_capital_gains,
        ),
        line(
            "Veräußerungsverluste".to_owned(),
            -statement.total_capital_losses,
        ),
        line(
            "Dividenden und Ausschüttungen".to_owned(),
            statement.total_dividend_income,
        ),
        line("Zinsen".to_owned(), statement.total_interest_income),
        line(
            "Fremdwährungsgewinne (§20 EStG)".to_owned(),
            statement.total_fx_gains,
        ),
        line(
            "Fremdwährungsverluste (§20 EStG)".to_owned(),
            -statement.total_fx_losses,
        ),
    ];
    if statement.loss_carryforward_stock_prior != dec!(0) {
        rows.push(line(
            "Verlustvortrag Aktien aus Vorjahren".to_owned(),
            -statement.loss_carryforward_stock_prior,
        ));
    }
    if statement.loss_carryforward_other_prior != dec!(0) {
        rows.push(line(
            "Verlustvortrag sonstige Kapitalerträge aus Vorjahren".to_owned(),
            -statement.loss_carryforward_other_prior,
        ));
    }
    rows.push(line(
        "Sparer-Pauschbetrag (§20 Abs. 9 EStG)".to_owned(),
        statement.sparer_pauschbetrag,
    ));
    rows.push(line(
        "davon in Anspruch genommen".to_owned(),
        -statement.sparer_pauschbetrag_used,
    ));
    rows.push(Row::subtotal(vec![
        Cell::text(
            "Steuerpflichtige Kapitalerträge (nach Verlustverrechnung und Sparer-Pauschbetrag)",
        ),
        Cell::num(eur(statement.total_taxable_income)),
    ]));
    rows.push(line(
        format!("Abgeltungsteuer ({})", pct(rates.abgeltungssteuer)),
        statement.total_abgeltungssteuer,
    ));
    rows.push(line(
        format!(
            "Solidaritätszuschlag ({} der Abgeltungsteuer)",
            pct(rates.solidaritaetszuschlag)
        ),
        statement.total_solidaritaetszuschlag,
    ));
    if rates.kirchensteuer != dec!(0) {
        rows.push(line(
            format!(
                "Kirchensteuer ({} der Abgeltungsteuer)",
                pct(rates.kirchensteuer)
            ),
            statement.total_kirchensteuer,
        ));
    }
    rows.push(Row::subtotal(vec![
        Cell::text("Deutsche Steuer gesamt"),
        Cell::num(eur(statement.total_german_tax)),
    ]));
    rows.push(line(
        "Gezahlte ausländische Quellensteuer".to_owned(),
        statement.total_foreign_tax,
    ));
    rows.push(line(
        "davon anrechenbar (§32d Abs. 5 EStG; bereits in der Steuer berücksichtigt)".to_owned(),
        statement.total_foreign_tax_credit,
    ));
    rows.push(Row::total(vec![
        Cell::text("Voraussichtliche Steuerschuld (nach Anrechnung)"),
        Cell::num(eur(statement.net_tax_due)),
    ]));
    if statement.loss_carryforward_stock_next != dec!(0) {
        rows.push(line(
            "Verlustvortrag Aktien in das Folgejahr".to_owned(),
            statement.loss_carryforward_stock_next,
        ));
    }
    if statement.loss_carryforward_other_next != dec!(0) {
        rows.push(line(
            "Verlustvortrag sonstige Kapitalerträge in das Folgejahr".to_owned(),
            statement.loss_carryforward_other_next,
        ));
    }
    if statement.total_fees != dec!(0) {
        rows.push(line(
            "Gebühren (informativ; kein Werbungskostenabzug nach §20 Abs. 9 EStG)".to_owned(),
            statement.total_fees,
        ));
    }
    table(out, &columns, &rows);

    section_end(out);
    true
}

struct ActivityRow {
    category: String,
    activity: String,
    gain: Decimal,
    loss: Decimal,
    mapping: String,
}

impl ActivityRow {
    fn new(category: &str, activity: &str, mapping: String) -> ActivityRow {
        ActivityRow {
            category: category.to_owned(),
            activity: activity.to_owned(),
            gain: dec!(0),
            loss: dec!(0),
            mapping,
        }
    }

    fn add(&mut self, amount: Decimal) {
        if amount >= dec!(0) {
            self.gain += amount;
        } else {
            self.loss += amount;
        }
    }

    fn net(&self) -> Decimal {
        self.gain + self.loss
    }

    fn is_empty(&self) -> bool {
        self.gain == dec!(0) && self.loss == dec!(0)
    }
}

fn sale_mapping(category: AssetCategory, rate: TeilfreistellungRate, net: Decimal) -> String {
    match category {
        AssetCategory::Stock => {
            if net < dec!(0) {
                format!(
                    "Verlust {} EUR → KAP Zeile 23 | Netto → KAP Zeile 19",
                    eur(net)
                )
            } else {
                format!(
                    "Gewinn {} EUR → KAP Zeile 20 | Netto → KAP Zeile 19",
                    eur(net)
                )
            }
        }
        _ => match kap_inv_zeilen(rate) {
            Some((_, _, zeile_sale)) => format!("Netto → KAP-INV Zeile {zeile_sale} (brutto)"),
            None => String::new(),
        },
    }
}

fn dividend_mapping(category: AssetCategory, rate: TeilfreistellungRate) -> String {
    match category {
        AssetCategory::Stock => "→ KAP Zeile 19".to_owned(),
        _ => match kap_inv_zeilen(rate) {
            Some((zeile_dist, _, _)) => format!("→ KAP-INV Zeile {zeile_dist} (brutto)"),
            None => String::new(),
        },
    }
}

/// Section: Übersicht nach Aktivität und Assetkategorie.
pub(super) fn by_activity(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    // (category, activity order) → row; BTreeMap keeps a stable, readable order.
    let mut rows: BTreeMap<(AssetCategory, u8), ActivityRow> = BTreeMap::new();
    let mut rate_of: BTreeMap<AssetCategory, TeilfreistellungRate> = BTreeMap::new();

    for entry in &statement.capital_gains {
        let category = AssetCategory::from(entry.teilfreistellung_rate);
        rate_of.insert(category, entry.teilfreistellung_rate);
        rows.entry((category, 2))
            .or_insert_with(|| {
                ActivityRow::new(category_label(category), "Veräußerungen", String::new())
            })
            .add(entry.gross_gain_loss);
    }
    for entry in &statement.dividends {
        let category = AssetCategory::from(entry.teilfreistellung_rate);
        rate_of.insert(category, entry.teilfreistellung_rate);
        rows.entry((category, 0))
            .or_insert_with(|| {
                ActivityRow::new(
                    category_label(category),
                    "Dividenden / Ausschüttungen",
                    dividend_mapping(category, entry.teilfreistellung_rate),
                )
            })
            .add(entry.gross_amount_eur);
        if entry.foreign_withholding_tax != dec!(0) {
            rows.entry((category, 1))
                .or_insert_with(|| {
                    ActivityRow::new(
                        category_label(category),
                        "Quellensteuern",
                        match category {
                            AssetCategory::Stock => {
                                "anrechenbar (max. 15 %) → KAP Zeile 41".to_owned()
                            }
                            _ => "nicht anrechenbar (InvStG)".to_owned(),
                        },
                    )
                })
                .add(-entry.foreign_withholding_tax);
        }
    }
    for ((category, activity), row) in rows.iter_mut() {
        if *activity == 2 {
            row.mapping = sale_mapping(*category, rate_of[category], row.net());
        }
    }

    let mut extra: Vec<ActivityRow> = Vec::new();

    let mut interest = ActivityRow::new("Cash", "Guthabenzinsen", "→ KAP Zeile 19".to_owned());
    for entry in &statement.interest {
        interest.add(entry.gross_amount_eur);
    }
    extra.push(interest);

    let mut fx = ActivityRow::new(
        "Devisen",
        "Währungsgewinne/-verluste (verzinsl. Konto, §20 EStG)",
        "Netto → KAP Zeile 19 | Verluste → KAP Zeile 22".to_owned(),
    );
    for entry in &statement.fx_gains {
        fx.add(entry.gross_amount_eur);
    }
    extra.push(fx);

    let mut margin = ActivityRow::new(
        "Devisen",
        "FX G/V aus Verbindlichkeiten (Tilgung Fremdwährungskredit)",
        "ℹ Nicht steuerbar".to_owned(),
    );
    margin.add(statement.non_taxable_margin_fx);
    extra.push(margin);

    let section23 = &statement.section23;
    let mut short_term = ActivityRow::new(
        "Devisen",
        "Private Veräußerungsgeschäfte (§23 EStG, ≤ 1 Jahr)",
        "→ Anlage SO Zeilen 41–47".to_owned(),
    );
    short_term.add(section23.short_term_gains);
    short_term.add(-section23.short_term_losses);
    extra.push(short_term);
    let mut long_term = ActivityRow::new(
        "Devisen",
        "Private Veräußerungsgeschäfte (§23 EStG, > 1 Jahr)",
        "steuerfrei".to_owned(),
    );
    long_term.add(section23.long_term_tax_free);
    extra.push(long_term);
    let mut borrowed = ActivityRow::new(
        "Devisen",
        "Fremdwährungskredit (§23 EStG)",
        "⚠️ manuell prüfen".to_owned(),
    );
    borrowed.add(section23.borrowed_review);
    extra.push(borrowed);

    let mut fees = ActivityRow::new(
        "Cash",
        "Gebühren",
        "informativ – kein Werbungskostenabzug (§20 Abs. 9 EStG)".to_owned(),
    );
    for entry in &statement.fees {
        fees.add(-entry.amount_eur);
    }
    extra.push(fees);

    let mut grants = ActivityRow::new(
        "Aktien",
        "Aktienzuteilungen (Vesting)",
        "→ Anlage N (Arbeitslohn)".to_owned(),
    );
    grants.add(statement.total_stock_grant_income);
    extra.push(grants);

    let mut cash_grants = ActivityRow::new(
        "Cash",
        "Bargeldzuwendungen",
        "→ Anlage SO (§22 Nr. 3 EStG)".to_owned(),
    );
    cash_grants.add(statement.total_cash_grant_income);
    extra.push(cash_grants);

    let all: Vec<&ActivityRow> = rows
        .values()
        .chain(extra.iter())
        .filter(|row| !row.is_empty())
        .collect();
    if all.is_empty() {
        return false;
    }

    section_start(
        out,
        "aktivitaet",
        "Übersicht nach Aktivität und Assetkategorie – inkl. steuerlicher Zuordnung",
    );
    p(
        out,
        &format!(
            "Diese Tabelle zeigt eine {} des Steuerjahres. Für jede Aktivität und Assetklasse \
             wird der jeweilige {} dargestellt – inklusive der {} (z. B. KAP-19, KAP-23, \
             SO-Zeilen). Beträge sind Bruttowerte vor Teilfreistellung und Verlustverrechnung; \
             die Formularwerte finden Sie in der Übersicht für die Steuerformulare.",
            b("Übersicht aller relevanten Transaktionen"),
            b("Gewinn, Verlust und Gesamtsaldo in EUR"),
            b("zugehörigen steuerlichen Einordnung")
        ),
    );

    let columns = [
        col("Asset-Kategorie", Align::Left),
        col("Aktivität", Align::Left),
        col("Gewinn EUR", Align::Right),
        col("Verlust EUR", Align::Right),
        col("G/V EUR", Align::Right),
        col("Steuerliche Zuordnung", Align::Left),
    ];
    let mut table_rows = Vec::with_capacity(all.len() + 1);
    let (mut total_gain, mut total_loss) = (dec!(0), dec!(0));
    for row in &all {
        total_gain += row.gain;
        total_loss += row.loss;
        table_rows.push(Row::data(vec![
            Cell::text(&row.category),
            Cell::text(&row.activity),
            Cell::num(eur(row.gain)),
            Cell::num(eur(row.loss)),
            Cell::num(eur(row.net())),
            Cell::text(&row.mapping),
        ]));
    }
    table_rows.push(Row::total(vec![
        Cell::text("GESAMT"),
        Cell::empty(),
        Cell::num(eur(total_gain)),
        Cell::num(eur(total_loss)),
        Cell::num(eur(total_gain + total_loss)),
        Cell::empty(),
    ]));
    table(out, &columns, &table_rows);

    section_end(out);
    true
}

#[derive(Default)]
struct SecurityResult {
    dividends: Option<(Decimal, Decimal)>,
    sales: Option<(Decimal, Decimal)>,
}

fn add_signed(slot: &mut Option<(Decimal, Decimal)>, amount: Decimal) {
    let (gain, loss) = slot.get_or_insert((dec!(0), dec!(0)));
    if amount >= dec!(0) {
        *gain += amount;
    } else {
        *loss += amount;
    }
}

/// Section: Gewinne und Verluste nach Wertpapier.
pub(super) fn by_security(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    if statement.capital_gains.is_empty() && statement.dividends.is_empty() {
        return false;
    }
    let names = security_names(statement);

    // category → (name, symbol, isin) → results
    let mut groups: BTreeMap<AssetCategory, BTreeMap<(String, String, String), SecurityResult>> =
        BTreeMap::new();
    for entry in &statement.capital_gains {
        let name = names
            .get(entry.symbol.as_str())
            .copied()
            .unwrap_or(entry.symbol.as_str());
        let result = groups
            .entry(AssetCategory::from(entry.teilfreistellung_rate))
            .or_default()
            .entry((name.to_owned(), entry.symbol.clone(), entry.isin.clone()))
            .or_default();
        add_signed(&mut result.sales, entry.gross_gain_loss);
    }
    for entry in &statement.dividends {
        let name = names
            .get(entry.symbol.as_str())
            .copied()
            .unwrap_or(entry.symbol.as_str());
        let result = groups
            .entry(AssetCategory::from(entry.teilfreistellung_rate))
            .or_default()
            .entry((name.to_owned(), entry.symbol.clone(), entry.isin.clone()))
            .or_default();
        add_signed(&mut result.dividends, entry.gross_amount_eur);
    }

    section_start(out, "wertpapiere", "Gewinne und Verluste nach Wertpapier");
    p(
        out,
        &format!(
            "Diese Tabelle zeigt eine {} des Steuerjahres – {}. Für jedes Wertpapier wird der {} \
             dargestellt, zusätzlich aufgeschlüsselt nach {} (Veräußerungen, Dividenden). Beträge \
             sind Bruttowerte vor Teilfreistellung.",
            b("detaillierte Aufschlüsselung aller realisierten Gewinne und Verluste"),
            b("gruppiert nach einzelnen Wertpapieren"),
            b("Gewinn, Verlust und Gesamtsaldo in EUR"),
            b("Aktivitätsart")
        ),
    );
    p(
        out,
        &format!(
            "Diese Übersicht eignet sich besonders zur {} der Steuerwerte und zur detaillierten \
             Dokumentation für das Finanzamt.",
            b("Plausibilitätsprüfung")
        ),
    );

    let columns = [
        col("Name", Align::Left),
        col("ISIN", Align::Left),
        col("Aktivität", Align::Left),
        col("Gewinn EUR", Align::Right),
        col("Verlust EUR", Align::Right),
        col("G/V EUR", Align::Right),
    ];

    for (category, securities) in &groups {
        h3(out, category_label(*category));
        let mut rows = Vec::new();
        let (mut div_gain, mut div_loss) = (dec!(0), dec!(0));
        let (mut sale_gain, mut sale_loss) = (dec!(0), dec!(0));
        for ((name, _symbol, isin), result) in securities {
            if let Some((gain, loss)) = result.dividends {
                div_gain += gain;
                div_loss += loss;
                rows.push(Row::data(vec![
                    Cell::text(name),
                    Cell::text(isin),
                    Cell::text("Dividenden"),
                    Cell::num(eur(gain)),
                    Cell::num(eur(loss)),
                    Cell::num(eur(gain + loss)),
                ]));
            }
            if let Some((gain, loss)) = result.sales {
                sale_gain += gain;
                sale_loss += loss;
                rows.push(Row::data(vec![
                    Cell::text(name),
                    Cell::text(isin),
                    Cell::text("Veräußerungen"),
                    Cell::num(eur(gain)),
                    Cell::num(eur(loss)),
                    Cell::num(eur(gain + loss)),
                ]));
            }
        }
        if div_gain != dec!(0) || div_loss != dec!(0) {
            rows.push(Row::subtotal(vec![
                Cell::text("Gesamt – Dividenden"),
                Cell::empty(),
                Cell::empty(),
                Cell::num(eur(div_gain)),
                Cell::num(eur(div_loss)),
                Cell::num(eur(div_gain + div_loss)),
            ]));
        }
        if sale_gain != dec!(0) || sale_loss != dec!(0) {
            rows.push(Row::subtotal(vec![
                Cell::text("Gesamt – Veräußerungen"),
                Cell::empty(),
                Cell::empty(),
                Cell::num(eur(sale_gain)),
                Cell::num(eur(sale_loss)),
                Cell::num(eur(sale_gain + sale_loss)),
            ]));
        }
        rows.push(Row::total(vec![
            Cell::text("Gesamt"),
            Cell::empty(),
            Cell::empty(),
            Cell::num(eur(div_gain + sale_gain)),
            Cell::num(eur(div_loss + sale_loss)),
            Cell::num(eur(div_gain + sale_gain + div_loss + sale_loss)),
        ]));
        table(out, &columns, &rows);
    }

    section_end(out);
    true
}

/// Section: Vorabpauschale (tool extra).
pub(super) fn vorabpauschale(
    out: &mut String,
    statement: &GermanTaxStatement,
    _meta: &ReportMeta,
) -> bool {
    if statement.vorabpauschale.is_empty() && statement.vorabpauschale_missing_nav.is_empty() {
        return false;
    }
    let next_year = statement.year + 1;

    section_start(out, "vorabpauschale", "Vorabpauschale (§18 InvStG)");
    p(
        out,
        &format!(
            "Für Investmentfonds, die am 31.12.{} gehalten wurden, gilt eine fiktive \
             Mindestbesteuerung: die {}. Sie ergibt sich aus dem Wert zu Jahresbeginn, dem \
             Basiszins und den Ausschüttungen des Jahres, ist auf den Wertzuwachs begrenzt und \
             gilt am ersten Werktag des Folgejahres als zugeflossen. Sie ist daher {} und \
             {} in diesem Bericht enthalten. Bei einem späteren Verkauf mindert die kumulierte \
             Vorabpauschale den Veräußerungsgewinn (§19 InvStG).",
            statement.year,
            b("Vorabpauschale"),
            b(&format!("Einkunft des Jahres {next_year}")),
            b("nicht in den Steuerformular-Werten")
        ),
    );

    if !statement.vorabpauschale.is_empty() {
        let columns = [
            col("Fonds", Align::Left),
            col("ISIN", Align::Left),
            col("Menge", Align::Right),
            col("Wert 01.01.", Align::Right),
            col("Wert 31.12.", Align::Right),
            col("Ausschüttungen", Align::Right),
            col("Basiszins", Align::Right),
            col("Teilfreistellung", Align::Right),
            col("Vorabpauschale brutto", Align::Right),
            col("Steuerpflichtig", Align::Right),
            col("Steuer (geschätzt)", Align::Right),
            col("Kumuliert (§19)", Align::Right),
        ];
        let mut rows = Vec::new();
        let (mut gross, mut taxable, mut tax) = (dec!(0), dec!(0), dec!(0));
        for entry in &statement.vorabpauschale {
            gross += entry.gross_vorabpauschale;
            taxable += entry.taxable_amount;
            tax += entry.total_tax;
            rows.push(Row::data(vec![
                Cell::text(&entry.symbol),
                Cell::text(&entry.isin),
                Cell::num(qty(entry.quantity)),
                Cell::num(eur(entry.nav_jan1)),
                Cell::num(eur(entry.nav_dec31)),
                Cell::num(eur(entry.distributions)),
                Cell::num(pct(entry.basiszins)),
                Cell::num(pct(entry.teilfreistellung_rate.rate())),
                Cell::num(eur(entry.gross_vorabpauschale)),
                Cell::num(eur(entry.taxable_amount)),
                Cell::num(eur(entry.total_tax)),
                Cell::num(eur(entry.accumulated_after)),
            ]));
        }
        rows.push(Row::total(vec![
            Cell::text("Gesamt"),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::num(eur(gross)),
            Cell::num(eur(taxable)),
            Cell::num(eur(tax)),
            Cell::empty(),
        ]));
        table(out, &columns, &rows);

        let deemed = statement
            .vorabpauschale
            .first()
            .map(|entry| format::date(entry.deemed_received))
            .unwrap_or_default();
        note(
            out,
            "info",
            &format!(
                "Zufluss am {deemed}: in der Steuererklärung {next_year} anzugeben (Anlage \
                 KAP-INV, Vorabpauschale-Zeilen je Fondsart). Den kumulierten Betrag je Fonds in \
                 <code>taxes.vorabpauschale_carryforward</code> der Konfiguration fortschreiben."
            ),
        );
    }

    if !statement.vorabpauschale_missing_nav.is_empty() {
        note(
            out,
            "warn",
            &format!(
                "Für folgende zum Jahresende gehaltene Fonds konnte keine Vorabpauschale berechnet \
                 werden, weil die Rücknahmepreise zum 01.01. und 31.12. nicht konfiguriert sind \
                 (<code>taxes.fund_nav</code>): {}",
                escape(&statement.vorabpauschale_missing_nav.join(", "))
            ),
        );
    }

    section_end(out);
    true
}

/// Section: Hinweise und Warnungen (tool extra).
pub(super) fn notes(out: &mut String, statement: &GermanTaxStatement, meta: &ReportMeta) -> bool {
    section_start(out, "hinweise", "Hinweise und Warnungen");

    let mut warnings: Vec<String> = Vec::new();

    let year_start = crate::time::Date::from_ymd_opt(statement.year, 1, 1).unwrap();
    let year_end = crate::time::Date::from_ymd_opt(statement.year, 12, 31).unwrap();
    if meta.period.first_date() > year_start || meta.period.last_date() < year_end {
        warnings.push(format!(
            "Der Kontoauszug deckt nicht das gesamte Steuerjahr ab ({} – {}). Erträge und \
             Transaktionen außerhalb dieses Zeitraums fehlen.",
            format::date(meta.period.first_date()),
            format::date(meta.period.last_date())
        ));
    }
    if !statement.short_positions.is_empty() {
        let listed: Vec<String> = statement
            .short_positions
            .iter()
            .map(|(symbol, quantity)| format!("{}: {}", escape(symbol), qty(*quantity)))
            .collect();
        warnings.push(format!(
            "Short-Positionen zum Jahresende werden nicht steuerlich berechnet und erfordern eine \
             manuelle Prüfung nach §20 EStG (Termin-/Stillhaltergeschäfte): {}.",
            listed.join(", ")
        ));
    }
    if statement.section23.borrowed_review != dec!(0) {
        warnings.push(format!(
            "Realisierung aus Fremdwährungskredit unter §23 EStG in Höhe von {} EUR: die \
             §20-Ausnahme (BMF 19.05.2022 Rz. 131) gilt hier nicht; ggf. als privates \
             Veräußerungsgeschäft anzusetzen.",
            eur(statement.section23.borrowed_review)
        ));
    }
    if !statement.vorabpauschale_missing_nav.is_empty() {
        warnings.push(format!(
            "Vorabpauschale nicht berechnet (fehlende Rücknahmepreise): {}.",
            escape(&statement.vorabpauschale_missing_nav.join(", "))
        ));
    }
    if !warnings.is_empty() {
        h3(out, "Warnungen");
        for warning in &warnings {
            note(out, "warn", &format!("⚠️ {warning}"));
        }
    }

    // Per-position notes the processor attached (Altbestand, §19 reductions, FX losses, …).
    let mut position_notes: Vec<String> = Vec::new();
    for entry in &statement.capital_gains {
        if let Some(text) = &entry.notes {
            position_notes.push(format!(
                "{} – {}: {}",
                format::date(entry.transaction_date),
                escape(&entry.symbol),
                escape(text)
            ));
        }
    }
    for entry in &statement.dividends {
        if let Some(text) = &entry.notes {
            position_notes.push(format!(
                "{} – {}: {}",
                format::date(entry.payment_date),
                escape(&entry.symbol),
                escape(text)
            ));
        }
    }
    position_notes.dedup();
    if !position_notes.is_empty() {
        h3(out, "Hinweise zu einzelnen Positionen");
        ul(out, &position_notes);
    }

    if !statement.corporate_actions.is_empty() {
        h3(out, "Kapitalmaßnahmen");
        let columns = [
            col("Datum", Align::Left),
            col("Wertpapier", Align::Left),
            col("Art", Align::Left),
            col("Beschreibung", Align::Left),
            col("Steuerliche Auswirkung (EUR)", Align::Right),
            col("Hinweis", Align::Left),
        ];
        let rows: Vec<Row> = statement
            .corporate_actions
            .iter()
            .map(|entry| {
                Row::data(vec![
                    Cell::text(format::date(entry.date)),
                    Cell::text(&entry.symbol),
                    Cell::text(entry.action_type.to_string()),
                    Cell::text(&entry.description),
                    Cell::num(entry.tax_impact_eur.map(eur).unwrap_or_default()),
                    Cell::text(entry.notes.as_deref().unwrap_or("")),
                ])
            })
            .collect();
        table(out, &columns, &rows);
    }

    if !statement.stock_grants.is_empty() {
        h3(out, "Aktienzuteilungen (Anlage N)");
        let columns = [
            col("Vesting", Align::Left),
            col("Wertpapier", Align::Left),
            col("ISIN", Align::Left),
            col("Menge", Align::Right),
            col("Marktwert je Stück (EUR)", Align::Right),
            col("Arbeitslohn (EUR)", Align::Right),
        ];
        let mut rows: Vec<Row> = statement
            .stock_grants
            .iter()
            .map(|entry| {
                Row::data(vec![
                    Cell::text(format::date(entry.vest_date)),
                    Cell::text(&entry.symbol),
                    Cell::text(&entry.isin),
                    Cell::num(qty(entry.quantity)),
                    Cell::num(eur(entry.fmv_per_share_eur)),
                    Cell::num(eur(entry.total_fmv_eur)),
                ])
            })
            .collect();
        rows.push(Row::total(vec![
            Cell::text("Gesamt"),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::empty(),
            Cell::num(eur(statement.total_stock_grant_income)),
        ]));
        table(out, &columns, &rows);
        p(
            out,
            "Der Marktwert bei Vesting ist bereits als Arbeitslohn versteuert und bildet die \
             Anschaffungskosten der Aktien für spätere Veräußerungen.",
        );
    }

    if !statement.cash_grants.is_empty() {
        h3(out, "Bargeldzuwendungen (§22 Nr. 3 EStG)");
        let columns = [
            col("Datum", Align::Left),
            col("Beschreibung", Align::Left),
            col("Betrag (EUR)", Align::Right),
        ];
        let mut rows: Vec<Row> = statement
            .cash_grants
            .iter()
            .map(|entry| {
                Row::data(vec![
                    Cell::text(format::date(entry.date)),
                    Cell::text(&entry.description),
                    Cell::num(eur(entry.amount_eur)),
                ])
            })
            .collect();
        rows.push(Row::total(vec![
            Cell::text("Gesamt"),
            Cell::empty(),
            Cell::num(eur(statement.total_cash_grant_income)),
        ]));
        table(out, &columns, &rows);
        p(
            out,
            "Sonstige Einkünfte sind steuerpflichtig, wenn sie zusammen mit anderen Einkünften nach \
             §22 Nr. 3 EStG die Freigrenze von 256 EUR im Jahr erreichen.",
        );
    }

    h3(out, "Methodik und Grenzen");
    ul(
        out,
        &[
            "Veräußerungsgewinne werden je Verkauf nach dem FIFO-Prinzip (§20 Abs. 4 Satz 7 \
             EStG) aus den tatsächlichen Anschaffungskosten inklusive Kaufgebühren ermittelt; \
             Verkaufsgebühren mindern den Erlös."
                .to_owned(),
            "Fremdwährungsbeträge werden mit dem EZB-Referenzkurs des jeweiligen Buchungstags \
             umgerechnet (Erlöse und Anschaffungskosten am Valutatag, Gebühren am Handelstag); \
             echte Devisengeschäfte mit dem Ausführungskurs."
                .to_owned(),
            "Ausländische Quellensteuer wird pauschal bis zur Höhe von 15 % des Bruttoertrags \
             (typischer DBA-Satz) und höchstens 25 % des steuerpflichtigen Betrags als anrechenbar \
             ausgewiesen. Eine länderspezifische DBA-Tabelle ist nicht hinterlegt; bei abweichenden \
             Sätzen (z. B. Schweiz, Frankreich) bitte manuell prüfen. Fondsausschüttungen sind \
             nach InvStG nicht anrechenbar."
                .to_owned(),
            "Derivate, Optionen und Termingeschäfte werden nicht ausgewertet; entsprechende \
             Zeilen des Kontoauszugs wurden übersprungen."
                .to_owned(),
            "Die Klassifizierung von Fonds (Teilfreistellung) folgt der Konfiguration je ISIN; \
             nicht klassifizierte Wertpapiere gelten als Aktien."
                .to_owned(),
            "Die Steuerberechnung ist eine Schätzung ohne Berücksichtigung persönlicher \
             Verhältnisse (Günstigerprüfung, Freistellungsaufträge bei anderen Instituten, \
             sonstige Kapitalerträge)."
                .to_owned(),
        ],
    );

    section_end(out);
    true
}

//! Printable A4-landscape HTML report of the German tax statement ("Informativer Steuerbericht").
//!
//! The document shell, the neutral building blocks and the number formatting live in
//! [`crate::tax_statement::html`]; this module supplies the German language, labels, sections and
//! prose. Every EUR figure is the statement's own value routed through `format_eur` and
//! re-punctuated for German readers; the renderer computes no tax figure itself.

mod detail;
mod format;
mod summary;

#[cfg(test)]
mod tests;

use std::fmt::Write as _;
use std::io::Write;

use crate::core::EmptyResult;
use crate::tax_statement::html::{Document, Section, kpi};

use super::statement::GermanTaxStatement;

pub use crate::tax_statement::html::ReportMeta;
pub(super) use crate::tax_statement::html::builder;

use self::builder::{b, escape, p};

pub struct HtmlReport;

impl HtmlReport {
    pub fn write<W: Write>(
        statement: &GermanTaxStatement,
        meta: &ReportMeta,
        writer: &mut W,
    ) -> EmptyResult {
        writer.write_all(render(statement, meta).as_bytes())?;
        Ok(())
    }
}

/// Report sections in reading order.
const SECTIONS: &[Section<GermanTaxStatement>] = &[
    Section {
        id: "steuerformulare",
        title: "Übersicht für die Steuerformulare",
        render: summary::tax_forms,
    },
    Section {
        id: "steuerberechnung",
        title: "Steuerberechnung (Abgeltungsteuer)",
        render: summary::tax_computation,
    },
    Section {
        id: "aktivitaet",
        title: "Übersicht nach Aktivität und Assetkategorie",
        render: summary::by_activity,
    },
    Section {
        id: "wertpapiere",
        title: "Gewinne und Verluste nach Wertpapier",
        render: summary::by_security,
    },
    Section {
        id: "buchungen",
        title: "Barwirksame Buchungen",
        render: detail::bookings,
    },
    Section {
        id: "quellensteuer",
        title: "Quellensteuer-Übersicht",
        render: detail::withholding,
    },
    Section {
        id: "wertpapiergeschaefte",
        title: "Gewinne und Verluste aus Wertpapiergeschäften",
        render: detail::sales,
    },
    Section {
        id: "wertpapiertransaktionen",
        title: "Wertpapiertransaktionen",
        render: detail::trades,
    },
    Section {
        id: "fremdwaehrung",
        title: "Fremdwährungsgewinne/-verluste",
        render: detail::fx,
    },
    Section {
        id: "offene-positionen",
        title: "Offene Positionen zum Jahresende",
        render: detail::open_lots,
    },
    Section {
        id: "vorabpauschale",
        title: "Vorabpauschale (§18 InvStG)",
        render: summary::vorabpauschale,
    },
    Section {
        id: "wertpapieruebersicht",
        title: "Wertpapierübersicht",
        render: detail::securities,
    },
    Section {
        id: "hinweise",
        title: "Hinweise und Warnungen",
        render: summary::notes,
    },
];

fn render(statement: &GermanTaxStatement, meta: &ReportMeta) -> String {
    let document = Document {
        lang: "de",
        title: format!("Informativer Steuerbericht {}", meta.year),
        toc_id: "inhalt",
        toc_title: "Inhalt",
    };
    crate::tax_statement::html::render(&document, SECTIONS, statement, meta, |out| {
        title_block(out, statement, meta)
    })
}

fn title_block(out: &mut String, statement: &GermanTaxStatement, meta: &ReportMeta) {
    out.push_str("<header class=\"hero\">\n");
    let _ = writeln!(
        out,
        "<p class=\"eyebrow\">Steuerjahr {} · Einkünfte aus Kapitalvermögen</p>",
        statement.year
    );
    let _ = writeln!(
        out,
        "<h1>Informativer Steuerbericht für {}</h1>",
        statement.year
    );

    out.push_str("<ul class=\"chips\">\n");
    let _ = writeln!(out, "<li class=\"chip\">{}</li>", escape(&meta.broker_name));
    let _ = writeln!(
        out,
        "<li class=\"chip\">Depot: {}</li>",
        escape(&meta.portfolio_name)
    );
    if let Some(account_id) = &meta.account_id {
        let _ = writeln!(
            out,
            "<li class=\"chip\">Konto-ID: {}</li>",
            escape(account_id)
        );
    }
    let _ = writeln!(
        out,
        "<li class=\"chip\">Zeitraum: {} – {}</li>",
        format::date(meta.period.first_date()),
        format::date(meta.period.last_date())
    );
    out.push_str("</ul>\n");

    out.push_str("<div class=\"kpis\">\n");
    kpi(
        out,
        "Anlage KAP Zeile 19",
        statement.kap_zeile_19,
        "Ausländische Kapitalerträge",
    );
    kpi(
        out,
        "Steuerpflichtig",
        statement.total_taxable_income,
        "nach Verlustverrechnung und Sparer-Pauschbetrag",
    );
    kpi(
        out,
        "Voraussichtliche Steuer",
        statement.net_tax_due,
        "Abgeltungsteuer inkl. Zuschläge, nach Anrechnung",
    );
    kpi(
        out,
        "Anrechenbare Quellensteuer",
        statement.kap_zeile_41,
        "Anlage KAP Zeile 41",
    );
    out.push_str("</div>\n");

    out.push_str("<div class=\"disclaimer\">\n<h3>⚠️ Hinweis &amp; Haftungsausschluss</h3>\n");
    p(
        out,
        &format!(
            "Dieser Bericht wurde nach {} aus den Kontoauszügen Ihres Brokers erstellt. Bitte \
             beachten Sie, dass es sich um eine {} handelt und Sie die Angaben vor Verwendung in \
             Ihrer Steuererklärung prüfen sollten.",
            b("bestem Wissen und mit großer Sorgfalt"),
            b("automatisierte Auswertung")
        ),
    );
    p(
        out,
        &format!(
            "Der Bericht stellt {} dar, sondern unterstützt Privatanleger bei der eigenständigen \
             Erstellung ihrer Steuererklärung – ähnlich den Jahresübersichten, die von Banken oder \
             Brokern bereitgestellt werden. Verbindlich sind ausschließlich Ihre eigene \
             Steuererklärung sowie die Beurteilung durch Ihr Finanzamt. Es wird empfohlen, beim \
             Finanzamt zusätzlich die {} als Nachweis einzureichen.",
            b("keine steuerliche Beratung"),
            b("offiziellen Kontoauszüge")
        ),
    );
    p(
        out,
        &format!(
            "Für die Vollständigkeit und Richtigkeit der dargestellten Informationen kann {} \
             übernommen werden.",
            b("keine Haftung")
        ),
    );
    out.push_str("</div>\n");

    p(
        out,
        &format!(
            "Dieser Bericht wurde auf Basis der von {} exportierten Kontoauszüge erstellt. Er \
             dient als technische Aufbereitung und Zusammenfassung sämtlicher im Steuerjahr \
             angefallenen Transaktionen, Erträge und Verluste. Die Daten wurden automatisiert \
             verarbeitet, nach den Vorgaben des {} analysiert und den entsprechenden Zeilen der \
             Steuerformulare (insbesondere {}, {} und {}) zugeordnet. Veräußerungsgewinne werden \
             nach dem FIFO-Prinzip ermittelt.",
            b(&meta.broker_name),
            b("deutschen Steuerrechts"),
            b("Anlage KAP"),
            b("Anlage KAP-INV"),
            b("Anlage SO")
        ),
    );
    p(
        out,
        &format!(
            "Alle Beträge werden mit dem {} des Buchungstags in EUR umgerechnet (bei echten \
             Devisengeschäften mit dem tatsächlichen Ausführungskurs).",
            b("EZB-Referenzkurs")
        ),
    );
    let _ = writeln!(
        out,
        "<p class=\"meta\">Erstellt am {} mit <code>investments</code> (Steuerjahr {}).</p>",
        format::datetime(meta.generated_at),
        statement.year
    );
    out.push_str("</header>\n");
}

//! Printable A4-landscape HTML report of the Spanish tax statement ("Informe fiscal informativo").
//!
//! The document shell, the neutral building blocks and the number formatting live in
//! [`crate::tax_statement::html`]; this module supplies the Spanish language, labels, sections and
//! prose. Every EUR figure is the statement's own value routed through `format_eur` and
//! re-punctuated for Spanish readers; the renderer computes no tax figure itself.
//!
//! The calculation warnings the tool raises are shared with the CSV and the console, which are in
//! English, and are shown verbatim rather than paraphrased: a translated warning that drifted from
//! the one the log printed would be worse than an untranslated one.

mod detail;
mod format;
mod summary;

#[cfg(test)]
mod tests;

use std::fmt::Write as _;
use std::io::Write;

use crate::core::EmptyResult;
use crate::tax_statement::html::{Document, Section, kpi};

use super::forms;
use super::statement::SpanishTaxStatement;

pub use crate::tax_statement::html::ReportMeta;
pub(super) use crate::tax_statement::html::builder;

use self::builder::{b, escape, p};

pub struct HtmlReport;

impl HtmlReport {
    pub fn write<W: Write>(
        statement: &SpanishTaxStatement,
        meta: &ReportMeta,
        writer: &mut W,
    ) -> EmptyResult {
        writer.write_all(render(statement, meta).as_bytes())?;
        Ok(())
    }
}

/// Report sections in reading order: what the return needs first, then the workings behind it.
const SECTIONS: &[Section<SpanishTaxStatement>] = &[
    Section {
        id: "formularios",
        title: "Resumen para los formularios",
        render: summary::tax_forms,
    },
    Section {
        id: "calculo",
        title: "Cálculo del impuesto",
        render: summary::tax_computation,
    },
    Section {
        id: "actividad",
        title: "Resumen por actividad y categoría",
        render: summary::by_activity,
    },
    Section {
        id: "valores",
        title: "Ganancias y pérdidas por valor",
        render: summary::by_security,
    },
    Section {
        id: "movimientos",
        title: "Movimientos de efectivo",
        render: detail::bookings,
    },
    Section {
        id: "retenciones",
        title: "Retenciones en origen",
        render: detail::withholding,
    },
    Section {
        id: "transmisiones",
        title: "Ganancias y pérdidas patrimoniales (FIFO)",
        render: detail::sales,
    },
    Section {
        id: "valores-homogeneos",
        title: "Valores homogéneos",
        render: summary::wash_sales,
    },
    Section {
        id: "operaciones",
        title: "Operaciones con valores",
        render: detail::trades,
    },
    Section {
        id: "divisas",
        title: "Diferencias de cambio",
        render: detail::fx,
    },
    Section {
        // The heading the section prints carries the year and, when the statement ends earlier,
        // the statement's own last date; the index cannot, so it names the concept alone.
        id: "posiciones-abiertas",
        title: "Posiciones abiertas al cierre del ejercicio",
        render: detail::open_lots,
    },
    Section {
        id: "compensacion",
        title: "Compensación y saldos pendientes",
        render: summary::compensation,
    },
    Section {
        id: "relacion-valores",
        title: "Relación de valores",
        render: detail::securities,
    },
    Section {
        id: "avisos",
        title: "Avisos y observaciones",
        render: summary::notes,
    },
];

fn render(statement: &SpanishTaxStatement, meta: &ReportMeta) -> String {
    let document = Document {
        lang: "es",
        title: format!("Informe fiscal informativo {}", meta.year),
        toc_id: "indice",
        toc_title: "Índice",
    };
    crate::tax_statement::html::render(&document, SECTIONS, statement, meta, |out| {
        title_block(out, statement, meta)
    })
}

fn title_block(out: &mut String, statement: &SpanishTaxStatement, meta: &ReportMeta) {
    out.push_str("<header class=\"hero\">\n");
    let _ = writeln!(
        out,
        "<p class=\"eyebrow\">Ejercicio {} · Base del ahorro · {}</p>",
        statement.year,
        escape(statement.regime.description())
    );
    let _ = writeln!(
        out,
        "<h1>Informe fiscal informativo {}</h1>",
        statement.year
    );

    out.push_str("<ul class=\"chips\">\n");
    let _ = writeln!(out, "<li class=\"chip\">{}</li>", escape(&meta.broker_name));
    let _ = writeln!(
        out,
        "<li class=\"chip\">Cartera: {}</li>",
        escape(&meta.portfolio_name)
    );
    if let Some(account_id) = &meta.account_id {
        let _ = writeln!(
            out,
            "<li class=\"chip\">Cuenta: {}</li>",
            escape(account_id)
        );
    }
    let _ = writeln!(
        out,
        "<li class=\"chip\">Periodo: {} – {}</li>",
        format::date(meta.period.first_date()),
        format::date(meta.period.last_date())
    );
    let _ = writeln!(
        out,
        "<li class=\"chip\">Régimen: {}</li>",
        escape(statement.regime.description())
    );
    out.push_str("</ul>\n");

    out.push_str("<div class=\"kpis\">\n");
    kpi(
        out,
        "Base liquidable del ahorro",
        statement.savings_base,
        "tras la compensación entre grupos",
    );
    kpi(
        out,
        "Cuota íntegra del ahorro",
        statement.savings_quota,
        "escala del ahorro del ejercicio",
    );
    kpi(
        out,
        "Deducción por doble imposición",
        statement.total_foreign_tax_credit,
        "retenciones en origen imputables",
    );
    kpi(
        out,
        "Cuota líquida del ahorro",
        statement.net_tax_due,
        "cuota íntegra menos deducción",
    );
    out.push_str("</div>\n");

    out.push_str("<div class=\"disclaimer\">\n<h3>⚠️ Aviso y limitación de responsabilidad</h3>\n");
    p(
        out,
        &format!(
            "Este informe se ha elaborado {} a partir de los extractos de su bróker. Se trata de \
             una {} y conviene revisar cada importe antes de trasladarlo a la declaración.",
            b("con el mayor cuidado"),
            b("liquidación automatizada")
        ),
    );
    p(
        out,
        &format!(
            "El informe {}: sirve de apoyo al contribuyente que presenta su propia declaración, \
             como los resúmenes anuales que entregan bancos y brókers. Sólo son vinculantes su \
             propia declaración y la liquidación de la Administración. Se recomienda conservar los \
             {} como justificante.",
            b("no constituye asesoramiento fiscal"),
            b("extractos oficiales")
        ),
    );
    p(
        out,
        &format!(
            "No se asume {} por la integridad ni por la exactitud de la información presentada.",
            b("ninguna responsabilidad")
        ),
    );
    out.push_str("</div>\n");

    p(
        out,
        &format!(
            "Este informe parte de los extractos exportados por {} y resume las operaciones, los \
             rendimientos y las pérdidas del ejercicio, asignándolos a las casillas del {}. Las \
             transmisiones se valoran por el método {}, y los rendimientos y ganancias se integran \
             en la {}.",
            b(&meta.broker_name),
            b(forms::form_name(statement.regime)),
            b("FIFO"),
            b("base del ahorro")
        ),
    );
    p(
        out,
        &format!(
            "Los importes en divisa se convierten a euros con el {} de la fecha de cada apunte \
             (los ingresos de una venta y el coste de cada lote, con el de su fecha valor).",
            b("tipo de cambio de referencia del BCE")
        ),
    );
    let _ = writeln!(
        out,
        "<p class=\"meta\">Generado el {} con <code>investments</code> (ejercicio {}).</p>",
        format::datetime(meta.generated_at),
        statement.year
    );
    out.push_str("</header>\n");
}

//! Printable A4-landscape HTML report infrastructure shared by the jurisdictions.
//!
//! A page is self-contained (inline CSS, no external assets) and laid out for Chrome's print
//! engine, so it converts to PDF with any Chromium-based tool (`chromium --headless
//! --print-to-pdf`, Playwright, Gotenberg). This module owns the document shell — head, title
//! block slot, table of contents, sections — and the neutral building blocks under
//! [`builder`], [`format`] and [`style`]. Language, dates, labels and prose belong to each
//! jurisdiction's own `html` module.

pub(crate) mod builder;
pub(crate) mod format;
pub(crate) mod style;

use std::fmt::Write as _;

use crate::time::{DateTime, Period};
use crate::types::Decimal;

use self::builder::escape;

/// Header facts about the report that the statement itself does not carry.
pub struct ReportMeta {
    pub year: i32,
    /// Broker display name (e.g. "Interactive Brokers LLC").
    pub broker_name: String,
    pub portfolio_name: String,
    pub account_id: Option<String>,
    pub period: Period,
    pub generated_at: DateTime,
}

/// Renders one section into the buffer, returning `false` when it has nothing to show; the section
/// is then omitted from both the body and the table of contents.
pub(crate) type SectionRenderer<S> = fn(&mut String, &S, &ReportMeta) -> bool;

pub(crate) struct Section<S> {
    pub id: &'static str,
    pub title: &'static str,
    pub render: SectionRenderer<S>,
}

/// The language-dependent parts of the document shell.
pub(crate) struct Document {
    pub lang: &'static str,
    pub title: String,
    pub toc_id: &'static str,
    pub toc_title: &'static str,
}

/// Assemble the page: head, the jurisdiction's title block, the table of contents and the sections
/// that had something to say, in the order given.
pub(crate) fn render<S>(
    doc: &Document,
    sections: &[Section<S>],
    statement: &S,
    meta: &ReportMeta,
    title_block: impl FnOnce(&mut String),
) -> String {
    let mut body = String::new();
    let mut toc = Vec::new();
    for section in sections {
        let mut buffer = String::new();
        if (section.render)(&mut buffer, statement, meta) {
            toc.push((section.id, section.title));
            body.push_str(&buffer);
        }
    }

    let mut out = String::with_capacity(body.len() + 8192);
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>{css}</style>\n</head>\n<body>\n",
        lang = escape(doc.lang),
        title = escape(&doc.title),
        css = style::CSS,
    );

    title_block(&mut out);

    let _ = write!(
        out,
        "<nav id=\"{}\">\n<h2 class=\"first\">{}</h2>\n<ol class=\"toc\">\n",
        escape(doc.toc_id),
        escape(doc.toc_title),
    );
    for (id, title) in &toc {
        let _ = writeln!(out, "<li><a href=\"#{id}\">{}</a></li>", escape(title));
    }
    out.push_str("</ol>\n</nav>\n");

    out.push_str(&body);
    out.push_str("</body>\n</html>\n");
    out
}

/// One key-figure card on the title page.
pub(crate) fn kpi(out: &mut String, label: &str, value: Decimal, hint: &str) {
    let text = format::eur(value);
    let class = if text.starts_with('-') { " neg" } else { "" };
    let _ = writeln!(
        out,
        "<div class=\"kpi\"><div class=\"label\">{}</div><div class=\"value{class}\">{text} €</div>\
         <div class=\"hint\">{}</div></div>",
        escape(label),
        escape(hint)
    );
}

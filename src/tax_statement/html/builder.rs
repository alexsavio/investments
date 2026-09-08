//! Minimal HTML building blocks for the report: escaping, sections, paragraphs and tables.
//!
//! Every dynamic string passes through [`escape`]; functions taking `html` arguments expect the
//! caller to have escaped the dynamic parts already.

use std::fmt::Write as _;

/// Escape text for an HTML text node or attribute value.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Right,
}

/// A table cell holding already-escaped HTML.
pub(crate) struct Cell {
    html: String,
    align: Align,
    negative: bool,
}

impl Cell {
    pub fn text(text: impl AsRef<str>) -> Cell {
        Cell {
            html: escape(text.as_ref()),
            align: Align::Left,
            negative: false,
        }
    }

    pub fn raw(html: impl Into<String>) -> Cell {
        Cell {
            html: html.into(),
            align: Align::Left,
            negative: false,
        }
    }

    /// A right-aligned number, highlighted when negative.
    pub fn num(text: impl AsRef<str>) -> Cell {
        let text = text.as_ref();
        Cell {
            html: escape(text),
            align: Align::Right,
            negative: text.starts_with('-'),
        }
    }

    pub fn empty() -> Cell {
        Cell::raw("")
    }
}

pub(crate) struct Column {
    pub title: &'static str,
    pub align: Align,
}

pub(crate) const fn col(title: &'static str, align: Align) -> Column {
    Column { title, align }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowKind {
    Data,
    /// A FIFO lot line under its sale.
    Lot,
    Subtotal,
    Total,
}

pub(crate) struct Row {
    pub kind: RowKind,
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn data(cells: Vec<Cell>) -> Row {
        Row {
            kind: RowKind::Data,
            cells,
        }
    }

    pub fn lot(cells: Vec<Cell>) -> Row {
        Row {
            kind: RowKind::Lot,
            cells,
        }
    }

    pub fn subtotal(cells: Vec<Cell>) -> Row {
        Row {
            kind: RowKind::Subtotal,
            cells,
        }
    }

    pub fn total(cells: Vec<Cell>) -> Row {
        Row {
            kind: RowKind::Total,
            cells,
        }
    }
}

fn align_class(align: Align) -> &'static str {
    match align {
        Align::Left => "",
        Align::Right => " num",
    }
}

/// Render a table with a repeating header (`<thead>`), padding short rows with empty cells.
pub(crate) fn table(out: &mut String, columns: &[Column], rows: &[Row]) {
    out.push_str("<table>\n<thead><tr>");
    for column in columns {
        let _ = write!(
            out,
            "<th class=\"col{}\">{}</th>",
            align_class(column.align),
            escape(column.title)
        );
    }
    out.push_str("</tr></thead>\n<tbody>\n");

    for row in rows {
        let class = match row.kind {
            RowKind::Data => "data",
            RowKind::Lot => "lot",
            RowKind::Subtotal => "subtotal",
            RowKind::Total => "total",
        };
        let _ = write!(out, "<tr class=\"{class}\">");
        for index in 0..columns.len() {
            match row.cells.get(index) {
                Some(cell) => {
                    let _ = write!(
                        out,
                        "<td class=\"cell{}{}\">{}</td>",
                        align_class(cell.align),
                        if cell.negative { " neg" } else { "" },
                        cell.html
                    );
                }
                None => out.push_str("<td class=\"cell\"></td>"),
            }
        }
        out.push_str("</tr>\n");
    }

    out.push_str("</tbody>\n</table>\n");
}

pub(crate) fn section_start(out: &mut String, id: &str, title: &str) {
    let _ = write!(
        out,
        "<section id=\"{}\">\n<h2>{}</h2>\n",
        escape(id),
        escape(title)
    );
}

pub(crate) fn section_end(out: &mut String) {
    out.push_str("</section>\n");
}

pub(crate) fn h3(out: &mut String, text: &str) {
    let _ = writeln!(out, "<h3>{}</h3>", escape(text));
}

pub(crate) fn h4(out: &mut String, text: &str) {
    let _ = writeln!(out, "<h4>{}</h4>", escape(text));
}

/// A paragraph of trusted HTML (dynamic parts must already be escaped).
pub(crate) fn p(out: &mut String, html: &str) {
    let _ = writeln!(out, "<p>{html}</p>");
}

/// A bullet list of trusted HTML items.
pub(crate) fn ul(out: &mut String, items: &[String]) {
    out.push_str("<ul>\n");
    for item in items {
        let _ = writeln!(out, "<li>{item}</li>");
    }
    out.push_str("</ul>\n");
}

/// A highlighted note box of trusted HTML; `class` selects the colour (`info`, `warn`).
pub(crate) fn note(out: &mut String, class: &str, html: &str) {
    let _ = writeln!(out, "<div class=\"note {class}\">{html}</div>");
}

/// Bold label for inline use.
pub(crate) fn b(text: &str) -> String {
    format!("<b>{}</b>", escape(text))
}

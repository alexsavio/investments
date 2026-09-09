//! German date and label formatting for the report.
//!
//! The numbers come from [`crate::tax_statement::html::format`] and are re-exported here, so a
//! German section imports one module for everything it prints.

use crate::formatting::format_date;
use crate::taxes::germany::TeilfreistellungRate;
use crate::time::{Date, DateTime};

use super::super::report_details::{AssetCategory, BookingKind, FxTreatment, LotSource, TradeSide};

pub(super) use crate::tax_statement::html::format::{
    amount, days, dec2, eur, pct, pct_observed, qty, rate,
};

pub(super) fn date(value: Date) -> String {
    format_date(value)
}

pub(super) fn datetime(value: DateTime) -> String {
    value.format("%d.%m.%Y %H:%M").to_string()
}

/// Country name for the common ISIN prefixes; the code itself otherwise.
pub(super) fn country_name(code: &str) -> String {
    let name = match code {
        "AT" => "Österreich",
        "AU" => "Australien",
        "BE" => "Belgien",
        "BM" => "Bermuda",
        "CA" => "Kanada",
        "CH" => "Schweiz",
        "CN" => "China",
        "DE" => "Deutschland",
        "DK" => "Dänemark",
        "ES" => "Spanien",
        "FI" => "Finnland",
        "FR" => "Frankreich",
        "GB" => "Vereinigtes Königreich",
        "HK" => "Hongkong",
        "IE" => "Irland",
        "IL" => "Israel",
        "IT" => "Italien",
        "JE" => "Jersey",
        "JP" => "Japan",
        "KR" => "Südkorea",
        "KY" => "Kaimaninseln",
        "LU" => "Luxemburg",
        "NL" => "Niederlande",
        "NO" => "Norwegen",
        "PT" => "Portugal",
        "SE" => "Schweden",
        "SG" => "Singapur",
        "TW" => "Taiwan",
        "US" => "Vereinigte Staaten",
        "" => "Unbekannt",
        other => return other.to_owned(),
    };
    name.to_owned()
}

pub(super) fn category_label(category: AssetCategory) -> &'static str {
    match category {
        AssetCategory::Stock => "Aktien",
        AssetCategory::EquityFund => "Aktienfonds",
        AssetCategory::MixedFund => "Mischfonds",
        AssetCategory::OtherFund => "Sonstige Fonds",
    }
}

pub(super) fn teilfreistellung_label(rate: TeilfreistellungRate) -> String {
    match rate {
        TeilfreistellungRate::None => "–".to_owned(),
        TeilfreistellungRate::Bond => "0 % (sonstiger Fonds)".to_owned(),
        other => format!("{} ({})", pct(other.rate()), category_label(other.into())),
    }
}

pub(super) fn side_label(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Buy => "Kauf",
        TradeSide::Sell => "Verkauf",
    }
}

pub(super) fn lot_source_label(source: LotSource) -> &'static str {
    match source {
        LotSource::Trade => "Kauf",
        LotSource::Grant => "Zuteilung",
        LotSource::CorporateAction => "Kapitalmaßnahme",
    }
}

pub(super) fn booking_kind_label(kind: BookingKind) -> &'static str {
    match kind {
        BookingKind::Dividend => "Dividenden und Ausschüttungen",
        BookingKind::WithholdingTax => "Quellensteuer",
        BookingKind::Interest => "Guthabenzinsen",
        BookingKind::Fee => "Gebühren",
        BookingKind::CashGrant => "Sonstige Zuflüsse (Bargeldzuwendungen)",
    }
}

pub(super) fn treatment_label(treatment: FxTreatment) -> &'static str {
    match treatment {
        FxTreatment::Section20Taxable => "§20 steuerpflichtig",
        FxTreatment::LoanRepaymentNonTaxable => "Nicht steuerbar (Kredittilgung)",
        FxTreatment::Section23ShortTerm => "§23 steuerpflichtig (≤ 1 Jahr)",
        FxTreatment::Section23LongTermTaxFree => "§23 steuerfrei (> 1 Jahr)",
        FxTreatment::Section23BorrowedReview => "§23 prüfen (Kredittilgung)",
    }
}

/// Human label for an IBKR statement-of-funds activity code; the code itself when unknown.
pub(super) fn activity_label(code: &str) -> String {
    let label = match code {
        "BUY" => "Kauf",
        "SELL" => "Verkauf",
        "DIV" => "Dividende",
        "PIL" => "Ersatzdividende",
        "WHTAX" | "FRTAX" => "Quellensteuer",
        "FOREX" => "Devisen",
        "CINT" => "Habenzinsen",
        "DINT" | "BINT" => "Sollzinsen",
        "OFEE" | "FEE" => "Gebühr",
        "DEP" => "Einzahlung",
        "WITH" => "Auszahlung",
        "ADJ" => "Korrektur",
        "OPENING" => "Anfangsbestand",
        other => return other.to_owned(),
    };
    format!("{label} ({code})")
}

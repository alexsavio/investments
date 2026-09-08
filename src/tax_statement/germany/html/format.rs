//! German number, date and label formatting for the report.
//!
//! EUR amounts go through the statement's own [`format_eur`] (round half away from zero, two
//! places) and are only re-punctuated here, so the report never disagrees with the CSV or the
//! console by a rounding cent.

use rust_decimal::RoundingStrategy;

use crate::formatting::format_date;
use crate::taxes::germany::TeilfreistellungRate;
use crate::time::{Date, DateTime};
use crate::types::Decimal;

use super::super::format_eur;
use super::super::report_details::{AssetCategory, BookingKind, FxTreatment, LotSource, TradeSide};

/// Re-punctuate a plain `-1234.56` style number as German `-1.234,56`. A value that rounds to
/// zero loses its sign (`-0.00` → `0,00`).
pub(super) fn germanize(plain: &str) -> String {
    let (negative, rest) = match plain.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, plain),
    };
    let (integer, fraction) = match rest.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (rest, None),
    };

    let is_zero = integer.chars().all(|c| c == '0')
        && fraction.is_none_or(|fraction| fraction.chars().all(|c| c == '0'));

    let mut out = String::with_capacity(plain.len() + 4);
    if negative && !is_zero {
        out.push('-');
    }
    let digits = integer.len();
    for (index, ch) in integer.chars().enumerate() {
        if index > 0 && (digits - index) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if let Some(fraction) = fraction {
        out.push(',');
        out.push_str(fraction);
    }
    out
}

/// EUR amount, two places, German punctuation.
pub(super) fn eur(value: Decimal) -> String {
    germanize(&format_eur(value))
}

/// Any two-place amount (prices, foreign-currency cash), same rounding as EUR.
pub(super) fn dec2(value: Decimal) -> String {
    eur(value)
}

/// Exchange rate with four places.
pub(super) fn rate(value: Decimal) -> String {
    let mut value = value.round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(4);
    germanize(&value.to_string())
}

/// Quantity with up to four places and no trailing zeros.
pub(super) fn qty(value: Decimal) -> String {
    let value = value
        .round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero)
        .normalize();
    germanize(&value.to_string())
}

/// A fraction (`0.15`) as a percentage (`15 %`), up to two decimals.
pub(super) fn pct(fraction: Decimal) -> String {
    pct_with_places(fraction, 2)
}

/// A percentage rounded to one decimal, for observed rates (`0.14994` → `15 %`).
pub(super) fn pct_observed(fraction: Decimal) -> String {
    pct_with_places(fraction, 1)
}

fn pct_with_places(fraction: Decimal, places: u32) -> String {
    let value = (fraction * dec!(100))
        .round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero)
        .normalize();
    format!("{} %", germanize(&value.to_string()))
}

pub(super) fn date(value: Date) -> String {
    format_date(value)
}

pub(super) fn datetime(value: DateTime) -> String {
    value.format("%d.%m.%Y %H:%M").to_string()
}

pub(super) fn days(value: i64) -> String {
    value.to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn germanize_groups_thousands_and_swaps_separators() {
        assert_eq!(eur(dec!(1234567.891)), "1.234.567,89");
        assert_eq!(eur(dec!(-1234.5)), "-1.234,50");
        assert_eq!(eur(dec!(0.5)), "0,50");
        assert_eq!(eur(dec!(999)), "999,00");
        assert_eq!(eur(dec!(1000)), "1.000,00");
    }

    #[test]
    fn germanize_rounds_half_away_from_zero_and_drops_negative_zero() {
        assert_eq!(eur(dec!(-0.125)), "-0,13");
        assert_eq!(eur(dec!(-0.001)), "0,00");
        assert_eq!(eur(dec!(0)), "0,00");
    }

    #[test]
    fn quantities_rates_and_percentages() {
        assert_eq!(qty(dec!(35.8539)), "35,8539");
        assert_eq!(qty(dec!(17.0000)), "17");
        assert_eq!(qty(dec!(1234)), "1.234");
        assert_eq!(rate(dec!(0.85871234)), "0,8587");
        assert_eq!(rate(dec!(1)), "1,0000");
        assert_eq!(pct(dec!(0.15)), "15 %");
        assert_eq!(pct(dec!(0.055)), "5,5 %");
        assert_eq!(pct_observed(dec!(0.14994)), "15 %");
        assert_eq!(pct_observed(dec!(0.2635)), "26,4 %");
    }
}

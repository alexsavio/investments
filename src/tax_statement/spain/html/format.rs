//! Spanish dates and labels for the report.
//!
//! The numbers come from [`crate::tax_statement::html::format`] and are re-exported here, so a
//! Spanish section imports one module for everything it prints. Spanish and German notation
//! coincide (`1.234,56`); only the dates, the labels and the prose differ.

use rust_decimal::RoundingStrategy;

use crate::time::{Date, DateTime};
use crate::types::Decimal;

use super::super::report::{AssetClass, BookingKind, FxTreatment, LotSource, TradeSide};

pub(super) use crate::tax_statement::html::format::{
    amount, days, dec2, eur, pct, pct_observed, punctuate, qty, rate,
};

pub(super) fn date(value: Date) -> String {
    value.format("%d/%m/%Y").to_string()
}

/// ISO form, for the one block that has to round-trip back into the configuration file.
pub(super) fn iso_date(value: Date) -> String {
    value.format("%Y-%m-%d").to_string()
}

pub(super) fn datetime(value: DateTime) -> String {
    value.format("%d/%m/%Y %H:%M").to_string()
}

/// Actualization coefficient, at the three decimals the Decreto Foral publishes, so a reader can
/// match the cell against the table.
///
/// `rescale` alone rounds half-to-even, so an exact half in the fourth decimal would come out of
/// this cell rounded differently from every amount beside it. Shipped and validated coefficients
/// carry at most three decimals, so this only bites on a hand-written override.
pub(super) fn coefficient(value: Decimal) -> String {
    let mut value = value.round_dp_with_strategy(3, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(3);
    punctuate(&value.to_string())
}

/// Which of the two savings-base groups an activity belongs to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Group {
    Rcm,
    Gyp,
    /// Reported only: it reaches neither group.
    Informational,
}

pub(super) fn group_label(group: Group) -> &'static str {
    match group {
        Group::Rcm => "Rendimientos del capital mobiliario",
        Group::Gyp => "Ganancias y pérdidas patrimoniales",
        Group::Informational => "Informativo (fuera de la base del ahorro)",
    }
}

/// Country name for the common ISIN prefixes; the code itself otherwise.
pub(super) fn country_name(code: &str) -> String {
    let name = match code {
        "AT" => "Austria",
        "AU" => "Australia",
        "BE" => "Bélgica",
        "BM" => "Bermudas",
        "CA" => "Canadá",
        "CH" => "Suiza",
        "CN" => "China",
        "DE" => "Alemania",
        "DK" => "Dinamarca",
        "ES" => "España",
        "FI" => "Finlandia",
        "FR" => "Francia",
        "GB" => "Reino Unido",
        "HK" => "Hong Kong",
        "IE" => "Irlanda",
        "IL" => "Israel",
        "IT" => "Italia",
        "JE" => "Jersey",
        "JP" => "Japón",
        "KR" => "Corea del Sur",
        "KY" => "Islas Caimán",
        "LU" => "Luxemburgo",
        "NL" => "Países Bajos",
        "NO" => "Noruega",
        "PT" => "Portugal",
        "SE" => "Suecia",
        "SG" => "Singapur",
        "TW" => "Taiwán",
        "US" => "Estados Unidos",
        "" => "Desconocido",
        other => return other.to_owned(),
    };
    name.to_owned()
}

pub(super) fn asset_class_label(class: AssetClass) -> &'static str {
    match class {
        AssetClass::Stock => "Acciones",
        AssetClass::Fund => "Fondos e IIC",
    }
}

pub(super) fn side_label(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Buy => "Compra",
        TradeSide::Sell => "Venta",
    }
}

pub(super) fn lot_source_label(source: LotSource) -> &'static str {
    match source {
        LotSource::Trade => "Compra",
        LotSource::Grant => "Adjudicación (vesting)",
        LotSource::CorporateAction => "Operación societaria",
    }
}

pub(super) fn booking_kind_label(kind: BookingKind) -> &'static str {
    match kind {
        BookingKind::Dividend => "Dividendos y distribuciones",
        BookingKind::WithholdingTax => "Retenciones en origen",
        BookingKind::Interest => "Intereses",
        BookingKind::Fee => "Comisiones y gastos",
        BookingKind::CashGrant => "Otros ingresos",
    }
}

pub(super) fn treatment_label(treatment: FxTreatment) -> &'static str {
    match treatment {
        FxTreatment::HeldBalance => "Ganancia/pérdida patrimonial (saldo propio)",
        FxTreatment::BorrowedBalance => "Revisión manual (saldo prestado)",
    }
}

/// Human label for an IBKR statement-of-funds activity code; the code itself when unknown.
pub(super) fn activity_label(code: &str) -> String {
    let label = match code {
        "BUY" => "Compra",
        "SELL" => "Venta",
        "DIV" => "Dividendo",
        "PIL" => "Dividendo sustitutivo",
        "WHTAX" | "FRTAX" => "Retención en origen",
        "FOREX" => "Cambio de divisa",
        "CINT" => "Intereses cobrados",
        "DINT" | "BINT" => "Intereses pagados",
        "OFEE" | "FEE" => "Comisión",
        "DEP" => "Ingreso",
        "WITH" => "Retirada",
        "ADJ" => "Ajuste",
        "OPENING" => "Saldo inicial",
        other => return other.to_owned(),
    };
    format!("{label} ({code})")
}

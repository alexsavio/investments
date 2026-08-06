//! German tax statement generation module.
//!
//! Generates comprehensive CSV tax reports for German tax residents with foreign brokerage accounts.

mod csv_formatter;
mod fx_fifo;
mod processor;
mod statement;

#[cfg(test)]
mod tests;

use rust_decimal::RoundingStrategy;

use crate::types::Decimal;

pub use self::csv_formatter::CsvFormatter;
pub use self::processor::{compute_tax_year, process_broker_statement, produces_capital_gain};
pub use self::statement::{
    CapitalGainEntry, DividendEntry, FxGainEntry, GermanTaxStatement, InterestEntry, Section23,
};

/// Format a EUR amount to two places, half away from zero (German tax-form practice for per-line
/// amounts). The single source of truth for both the CSV and the console so the two surfaces never
/// disagree by a rounding cent. Each figure is rounded once from full precision; a one-cent gap
/// between rounded components and a rounded total is acceptable, unlike the silent truncation of the
/// `{:.2}` formatter, which rounds half-to-even and diverges from the form convention.
pub(crate) fn format_eur(value: Decimal) -> String {
    let mut value = round_eur(value);
    value.rescale(2);
    value.to_string()
}

/// Round a EUR amount to cents, half away from zero (German tax-form practice for per-line
/// amounts). Split out of `format_eur` so figures that stay `Decimal` — the sell simulation's
/// marginal tax, which is rendered by the generic table formatter — round the same way as the ones
/// the CSV formats itself.
pub(crate) fn round_eur(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

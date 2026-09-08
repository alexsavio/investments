//! Spanish tax statement generation module.
//!
//! Computes Spanish IRPF savings-income tax (base del ahorro) on foreign broker statements, for
//! both the Gipuzkoa foral regime (Norma Foral 3/2014) and Territorio Común (Ley 35/2006).

mod csv_formatter;
mod forms;
mod processor;
mod report;
mod statement;
mod wash_sale;

#[cfg(test)]
mod continuity_tests;
#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod tests;

pub use self::csv_formatter::CsvFormatter;
pub use self::processor::{compute_tax_year, produces_capital_gain};
pub use self::statement::{CapitalGainEntry, SpanishLotDetail, SpanishTaxStatement};

// The one-formatter invariant spans every EUR-filing jurisdiction: the CSV, the console summary and
// the sell simulation's table all round through the same pair.
pub(crate) use crate::tax_statement::eur::{format_eur, round_eur};

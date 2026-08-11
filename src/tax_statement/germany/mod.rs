//! German tax statement generation module.
//!
//! Generates comprehensive CSV tax reports for German tax residents with foreign brokerage accounts.

mod csv_formatter;
mod processor;
mod statement;

#[cfg(test)]
mod tests;

pub use self::csv_formatter::CsvFormatter;
pub use self::processor::{compute_tax_year, process_broker_statement, produces_capital_gain};
pub use self::statement::{
    CapitalGainEntry, DividendEntry, FxGainEntry, GermanTaxStatement, InterestEntry, Section23,
};

// The one-formatter invariant now spans every EUR-filing jurisdiction, not just this one. Re-exported
// so existing `germany::format_eur` call sites keep resolving.
pub(crate) use crate::tax_statement::eur::{format_eur, round_eur};

//! German tax statement generation module.
//!
//! Generates comprehensive CSV tax reports for German tax residents with foreign brokerage accounts.

mod csv_formatter;
mod processor;
mod statement;

#[cfg(test)]
mod tests;

pub use self::csv_formatter::GermanCsvFormatter;
pub use self::processor::process_broker_statement;
pub use self::statement::{
    CapitalGainEntry, DividendEntry, FxGainEntry, GermanTaxStatement, InterestEntry,
};

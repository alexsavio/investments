//! German tax calculation module.
//!
//! Implements German tax rules including:
//! - Abgeltungssteuer (25% flat capital gains tax since 2009)
//! - Solidaritätszuschlag (5.5% solidarity surcharge)
//! - Kirchensteuer (church tax, 8% or 9% depending on region)
//! - Teilfreistellung (partial exemptions for ETFs)
//! - FIFO cost basis tracking
//! - Foreign tax credit handling under double taxation treaties

mod capital_gains;
mod dividends;
mod rates;

pub use self::capital_gains::{CapitalGainResult, FifoLot, FifoQueue, calculate_capital_gain};
pub use self::dividends::{TeilfreistellungRate, calculate_dividend_tax};
pub use self::rates::{GermanTaxRates, calculate_german_taxes};

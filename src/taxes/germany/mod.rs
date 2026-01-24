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

// Re-export types that are used by other modules
pub use self::dividends::TeilfreistellungRate;
pub use self::rates::GermanTaxRates;

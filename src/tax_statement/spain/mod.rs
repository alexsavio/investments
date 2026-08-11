//! Spanish tax statement generation module.
//!
//! Computes Spanish IRPF savings-income tax (base del ahorro) on foreign broker statements, for
//! both the Gipuzkoa foral regime (Norma Foral 3/2014) and Territorio Común (Ley 35/2006).

mod processor;
mod statement;

#[cfg(test)]
mod tests;

pub use self::processor::{compute_tax_year, produces_capital_gain};
pub use self::statement::{CapitalGainEntry, SpanishLotDetail, SpanishTaxStatement};

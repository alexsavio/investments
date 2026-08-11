//! Spanish IRPF savings-income primitives.
//!
//! Pure law: scales, actualization coefficients, loss ledgers and compensation, with no
//! `BrokerStatement` dependency. The statement pipeline that feeds them lives in
//! `crate::tax_statement::spain`.

pub mod carryforward;
pub mod coefficients;
pub mod scale;

use serde::Deserialize;

/// Which Spanish IRPF regime the filer is subject to.
///
/// Not a cosmetic switch: the two regimes differ in the savings scale, in whether acquisition
/// costs are actualized, in whether the two savings-base groups may offset each other, and in
/// whether custody fees are deductible. Every one of those changes the tax due.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanishTaxRegime {
    /// Gipuzkoa foral regime — Norma Foral 3/2014, as amended by Norma Foral 1/2025.
    Gipuzkoa,
    /// Territorio Común — Ley 35/2006 (LIRPF).
    #[serde(alias = "común")]
    Comun,
}

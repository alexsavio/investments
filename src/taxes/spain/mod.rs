//! Spanish IRPF savings-income primitives.
//!
//! Pure law: scales, actualization coefficients, loss ledgers and compensation, with no
//! `BrokerStatement` dependency. The statement pipeline that feeds them lives in
//! `crate::tax_statement::spain`.

pub mod carryforward;
pub mod coefficients;
pub mod compensation;
pub mod credit;
pub mod exemption;
pub mod scale;

use serde::Deserialize;

/// Which Spanish IRPF regime the filer is subject to.
///
/// Not a cosmetic switch: the regimes differ in the savings scale, in whether acquisition costs are
/// actualized, in how and whether the two savings-base groups may offset each other, in whether
/// custody fees are deductible and by how much, and in which gains are exempt. Every one of those
/// changes the tax due.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanishTaxRegime {
    /// Gipuzkoa foral regime — Norma Foral 3/2014, as amended by Norma Foral 1/2025.
    Gipuzkoa,
    /// Territorio Común — Ley 35/2006 (LIRPF).
    #[serde(alias = "común")]
    Comun,
    /// Comunidad Foral de Navarra — Decreto Foral Legislativo 4/2008 (TRLFIRPF).
    Navarra,
}

impl SpanishTaxRegime {
    /// How the regime is named to the filer, with the statute it comes from.
    ///
    /// One definition, so the console banner and the CSV's `SUMMARY_REGIME` row cannot drift apart.
    pub fn description(self) -> &'static str {
        match self {
            SpanishTaxRegime::Gipuzkoa => "Gipuzkoa (Norma Foral 3/2014)",
            SpanishTaxRegime::Comun => "Territorio Común (Ley 35/2006)",
            SpanishTaxRegime::Navarra => "Navarra (Decreto Foral Legislativo 4/2008)",
        }
    }
}

//! EUR amount formatting shared by the EUR-filing jurisdictions.
//!
//! One formatter for every EUR surface — CSV, console, and the figures that stay `Decimal` for a
//! generic table — so no two of them can disagree by a rounding cent. Each figure is rounded once
//! from full precision; a one-cent gap between rounded components and a rounded total is accepted,
//! unlike the silent truncation of the `{:.2}` formatter, which rounds half-to-even and diverges
//! from the form convention.

use rust_decimal::RoundingStrategy;

use crate::types::Decimal;

/// Format a EUR amount to two places, half away from zero.
///
/// Half away from zero is German tax-form practice for per-line amounts.
// TODO(verify): Spanish forms (Modelo 109 / Modelo 100) are assumed to use the same two-decimal,
// half-away-from-zero convention for per-line amounts. Neither the foral nor the state instructions
// reachable during S1 state the rounding mode explicitly.
pub(crate) fn format_eur(value: Decimal) -> String {
    let mut value = round_eur(value);
    value.rescale(2);
    value.to_string()
}

/// Round a EUR amount to cents, half away from zero.
///
/// Split out of [`format_eur`] so figures that stay `Decimal` — a sell simulation's marginal tax,
/// rendered by the generic table formatter — round the same way as the ones the CSV formats itself.
pub(crate) fn round_eur(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

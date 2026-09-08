//! Number formatting shared by the jurisdiction reports.
//!
//! EUR amounts go through the statement's own [`format_eur`] (round half away from zero, two
//! places) and are only re-punctuated here, so a report never disagrees with the CSV or the
//! console by a rounding cent. German and Spanish notation coincide (`1.234,56`), so one
//! punctuation pass serves both; dates, labels and prose stay in each jurisdiction's own module.

use rust_decimal::RoundingStrategy;

use crate::tax_statement::eur::format_eur;
use crate::types::Decimal;

/// Re-punctuate a plain `-1234.56` style number as `-1.234,56`. A value that rounds to zero loses
/// its sign (`-0.00` → `0,00`).
pub(crate) fn punctuate(plain: &str) -> String {
    let (negative, rest) = match plain.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, plain),
    };
    let (integer, fraction) = match rest.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (rest, None),
    };

    let is_zero = integer.chars().all(|c| c == '0')
        && fraction.is_none_or(|fraction| fraction.chars().all(|c| c == '0'));

    let mut out = String::with_capacity(plain.len() + 4);
    if negative && !is_zero {
        out.push('-');
    }
    let digits = integer.len();
    for (index, ch) in integer.chars().enumerate() {
        if index > 0 && (digits - index) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if let Some(fraction) = fraction {
        out.push(',');
        out.push_str(fraction);
    }
    out
}

/// EUR amount, two places, punctuated.
pub(crate) fn eur(value: Decimal) -> String {
    punctuate(&format_eur(value))
}

/// Any two-place amount (prices, foreign-currency cash), same rounding as EUR.
pub(crate) fn dec2(value: Decimal) -> String {
    eur(value)
}

/// Exchange rate with four places.
pub(crate) fn rate(value: Decimal) -> String {
    let mut value = value.round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(4);
    punctuate(&value.to_string())
}

/// Quantity with up to four places and no trailing zeros.
pub(crate) fn qty(value: Decimal) -> String {
    let value = value
        .round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero)
        .normalize();
    punctuate(&value.to_string())
}

/// A fraction (`0.15`) as a percentage (`15 %`), up to two decimals.
pub(crate) fn pct(fraction: Decimal) -> String {
    pct_with_places(fraction, 2)
}

/// A percentage rounded to one decimal, for observed rates (`0.14994` → `15 %`).
pub(crate) fn pct_observed(fraction: Decimal) -> String {
    pct_with_places(fraction, 1)
}

fn pct_with_places(fraction: Decimal, places: u32) -> String {
    let value = (fraction * dec!(100))
        .round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero)
        .normalize();
    format!("{} %", punctuate(&value.to_string()))
}

pub(crate) fn days(value: i64) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuate_groups_thousands_and_swaps_separators() {
        assert_eq!(eur(dec!(1234567.891)), "1.234.567,89");
        assert_eq!(eur(dec!(-1234.5)), "-1.234,50");
        assert_eq!(eur(dec!(0.5)), "0,50");
        assert_eq!(eur(dec!(999)), "999,00");
        assert_eq!(eur(dec!(1000)), "1.000,00");
    }

    #[test]
    fn punctuate_rounds_half_away_from_zero_and_drops_negative_zero() {
        assert_eq!(eur(dec!(-0.125)), "-0,13");
        assert_eq!(eur(dec!(-0.001)), "0,00");
        assert_eq!(eur(dec!(0)), "0,00");
    }

    #[test]
    fn quantities_rates_and_percentages() {
        assert_eq!(qty(dec!(35.8539)), "35,8539");
        assert_eq!(qty(dec!(17.0000)), "17");
        assert_eq!(qty(dec!(1234)), "1.234");
        assert_eq!(rate(dec!(0.85871234)), "0,8587");
        assert_eq!(rate(dec!(1)), "1,0000");
        assert_eq!(pct(dec!(0.15)), "15 %");
        assert_eq!(pct(dec!(0.055)), "5,5 %");
        assert_eq!(pct_observed(dec!(0.14994)), "15 %");
        assert_eq!(pct_observed(dec!(0.2635)), "26,4 %");
    }
}

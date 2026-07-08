//! Vorabpauschale (§18 InvStG) — advance lump-sum taxation of investment funds.
//!
//! A fund held at the end of a calendar year is deemed to distribute an "advance lump sum" that is
//! taxable income of the *following* year. See
//! <https://www.gesetze-im-internet.de/invstg_2018/__18.html>.

use chrono::Datelike;

use crate::types::{Date, Decimal};

/// A fund's Vorabpauschale for a calendar year (§18 InvStG), in the position's currency.
///
/// All monetary inputs share one currency (EUR in this tool):
/// - `nav_jan1` / `nav_dec31`: total position value at the year boundaries (per-unit redemption
///   price × units held at year end).
/// - `distributions`: total distributions received during the year.
/// - `basiszins`: BMF Basiszins for the year, as a fraction.
/// - `full_months_before_acquisition`: 0 for units held from the start of the year, otherwise the
///   number of full months before the month of acquisition (Zwölftelung, §18 Abs. 2).
///
/// ```text
/// basisertrag    = nav_jan1 × basiszins × 0.7
/// value_increase = max(0, nav_dec31 − nav_jan1)
/// vorabpauschale = max(0, min(basisertrag − distributions, value_increase))
///                  × (12 − full_months_before_acquisition) / 12
/// ```
///
/// The `min` against `value_increase` is equivalent to the statutory cap on the Basisertrag
/// (`min(basisertrag, value_increase + distributions)`, then minus distributions): for a positive
/// value increase the two are identical, and for a non-positive one both floor the result at 0.
#[must_use]
pub fn vorabpauschale(
    nav_jan1: Decimal,
    nav_dec31: Decimal,
    distributions: Decimal,
    basiszins: Decimal,
    full_months_before_acquisition: u32,
) -> Decimal {
    let basisertrag = nav_jan1 * basiszins * dec!(0.7);
    let value_increase = (nav_dec31 - nav_jan1).max(Decimal::ZERO);
    let uncapped = (basisertrag - distributions)
        .min(value_increase)
        .max(Decimal::ZERO);

    // §18 Abs. 2: the Vorabpauschale (not the Basisertrag) is reduced by 1/12 per full month before
    // the month of acquisition.
    let months_held = 12u32.saturating_sub(full_months_before_acquisition.min(12));
    uncapped * Decimal::from(months_held) / dec!(12)
}

/// Full months of the calendar year that precede the month of acquisition, for the Zwölftelung.
/// `acquired_month` is 1–12; `None` (or an out-of-range value) means held from the year's start → 0.
#[must_use]
pub fn full_months_before_acquisition(acquired_month: Option<u32>) -> u32 {
    match acquired_month {
        Some(month) if (1..=12).contains(&month) => month - 1,
        _ => 0,
    }
}

/// First business day of `year`, on which the prior year's Vorabpauschale is deemed received
/// (§18 Abs. 3 InvStG). Approximated as the first weekday on or after 2 January (1 January is always
/// a public holiday); the tool carries no full German holiday calendar, so a first-weekday
/// approximation is used.
#[must_use]
pub fn first_business_day_of_year(year: i32) -> Date {
    let mut date = Date::from_ymd_opt(year, 1, 2).expect("2 January is always a valid date");
    while matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
        date = date
            .succ_opt()
            .expect("early-January dates always have a successor");
    }
    date
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worked_example_basisertrag_below_value_increase() {
        // NAV 20,000 → 21,000, Basiszins 3.2%, no distributions:
        // basisertrag = 20000 × 0.032 × 0.7 = 448 < value increase 1,000 → 448.
        assert_eq!(
            vorabpauschale(dec!(20000), dec!(21000), dec!(0), dec!(0.032), 0),
            dec!(448)
        );
    }

    #[test]
    fn value_increase_caps_the_basisertrag() {
        // basisertrag 448 but the fund only rose by 200 → capped at 200.
        assert_eq!(
            vorabpauschale(dec!(20000), dec!(20200), dec!(0), dec!(0.032), 0),
            dec!(200)
        );
    }

    #[test]
    fn distributions_reduce_the_vorabpauschale() {
        // basisertrag 448 − distributions 100 = 348 (below the 1,000 value increase).
        assert_eq!(
            vorabpauschale(dec!(20000), dec!(21000), dec!(100), dec!(0.032), 0),
            dec!(348)
        );
    }

    #[test]
    fn distributions_above_basisertrag_yield_zero() {
        assert_eq!(
            vorabpauschale(dec!(20000), dec!(21000), dec!(500), dec!(0.032), 0),
            dec!(0)
        );
    }

    #[test]
    fn negative_value_increase_yields_zero() {
        assert_eq!(
            vorabpauschale(dec!(20000), dec!(19000), dec!(0), dec!(0.032), 0),
            dec!(0)
        );
    }

    #[test]
    fn zwoelftelung_prorates_acquisition_year() {
        // Acquired in April (month 4) → 3 full months before → 9/12 of the 448 full-year figure.
        assert_eq!(
            vorabpauschale(dec!(20000), dec!(21000), dec!(0), dec!(0.032), 3),
            dec!(336)
        );
    }

    #[test]
    fn min_form_matches_statutory_cap_with_distributions() {
        // Statute: capped_basisertrag = min(70, value_increase 50 + distributions 30 = 80) = 70;
        // VP = max(0, 70 − 30) = 40. The min-against-value_increase form gives the same 40.
        assert_eq!(
            vorabpauschale(dec!(1000), dec!(1050), dec!(30), dec!(0.10), 0),
            dec!(40)
        );
    }

    #[test]
    fn full_months_before_acquisition_maps_month_to_count() {
        assert_eq!(full_months_before_acquisition(None), 0);
        assert_eq!(full_months_before_acquisition(Some(1)), 0);
        assert_eq!(full_months_before_acquisition(Some(4)), 3);
        assert_eq!(full_months_before_acquisition(Some(12)), 11);
        // Out-of-range values are treated as held-from-start rather than panicking.
        assert_eq!(full_months_before_acquisition(Some(0)), 0);
        assert_eq!(full_months_before_acquisition(Some(13)), 0);
    }

    #[test]
    fn first_business_day_skips_the_new_year_weekend() {
        // 2 Jan 2025 is a Thursday.
        assert_eq!(
            first_business_day_of_year(2025),
            Date::from_ymd_opt(2025, 1, 2).unwrap()
        );
        // 1 Jan 2022 = Sat, 2 Jan = Sun → first business day is Mon 3 Jan.
        assert_eq!(
            first_business_day_of_year(2022),
            Date::from_ymd_opt(2022, 1, 3).unwrap()
        );
    }
}

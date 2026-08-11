//! Gipuzkoa actualization coefficients (coeficientes de actualización).
//!
//! NF 3/2014 art. 45.2 actualizes the acquisition value of a transferred asset by a coefficient
//! that tracks consumer prices, so only the real gain is taxed. The table is approved every
//! December by Decreto Foral and is keyed by the year of acquisition, for transfers made in the
//! following year. Territorio Común has no equivalent: Ley 26/2014 deleted LIRPF art. 35.2 with
//! effect from 2015, and even before that it applied only to real estate, never to securities.

use std::collections::BTreeMap;

use chrono::Datelike;

use crate::core::{EmptyResult, GenericResult};
use crate::time::Date;
use crate::types::Decimal;

/// Coefficients for transfers made in 2026. Decreto Foral 27/2025, de 23 de diciembre,
/// disposición adicional primera (BOG nº 249, 30-12-2025).
static COEFFICIENTS_2026: &[(i32, &str)] = &[
    (1994, "2.030"), // "1994 y anteriores"
    (1995, "2.156"),
    (1996, "2.076"),
    (1997, "2.030"),
    (1998, "1.985"),
    (1999, "1.931"),
    (2000, "1.866"),
    (2001, "1.796"),
    (2002, "1.732"),
    (2003, "1.685"),
    (2004, "1.635"),
    (2005, "1.583"),
    (2006, "1.531"),
    (2007, "1.489"),
    (2008, "1.430"),
    (2009, "1.426"),
    (2010, "1.402"),
    (2011, "1.360"),
    (2012, "1.330"),
    (2013, "1.309"),
    (2014, "1.307"),
    (2015, "1.307"),
    (2016, "1.307"),
    (2017, "1.281"),
    (2018, "1.261"),
    (2019, "1.249"),
    (2020, "1.249"),
    (2021, "1.212"),
    (2022, "1.121"),
    (2023, "1.082"),
    (2024, "1.050"),
    (2025, "1.020"),
    (2026, "1.000"),
];

/// Coefficients for transfers made in 2025. Decreto Foral 61/2024, de 27 de diciembre
/// (BOG nº 250, 30-12-2024).
static COEFFICIENTS_2025: &[(i32, &str)] = &[
    (1994, "1.969"), // "1994 y anteriores"
    (1995, "2.091"),
    (1996, "2.014"),
    (1997, "1.969"),
    (1998, "1.925"),
    (1999, "1.873"),
    (2000, "1.809"),
    (2001, "1.742"),
    (2002, "1.680"),
    (2003, "1.634"),
    (2004, "1.586"),
    (2005, "1.536"),
    (2006, "1.485"),
    (2007, "1.444"),
    (2008, "1.387"),
    (2009, "1.383"),
    (2010, "1.360"),
    (2011, "1.319"),
    (2012, "1.290"),
    (2013, "1.270"),
    (2014, "1.268"),
    (2015, "1.268"),
    (2016, "1.267"),
    (2017, "1.243"),
    (2018, "1.223"),
    (2019, "1.212"),
    (2020, "1.212"),
    (2021, "1.175"),
    (2022, "1.088"),
    (2023, "1.050"),
    (2024, "1.018"),
    (2025, "1.000"),
];

/// Coefficients for transfers made in 2024. Decreto Foral 58/2023, de 28 de diciembre
/// (BOG nº 250, 29-12-2023).
static COEFFICIENTS_2024: &[(i32, &str)] = &[
    (1994, "1.945"), // "1994 y anteriores"
    (1995, "2.066"),
    (1996, "1.989"),
    (1997, "1.945"),
    (1998, "1.902"),
    (1999, "1.850"),
    (2000, "1.787"),
    (2001, "1.721"),
    (2002, "1.660"),
    (2003, "1.614"),
    (2004, "1.567"),
    (2005, "1.517"),
    (2006, "1.467"),
    (2007, "1.426"),
    (2008, "1.370"),
    (2009, "1.366"),
    (2010, "1.344"),
    (2011, "1.303"),
    (2012, "1.274"),
    (2013, "1.254"),
    (2014, "1.252"),
    (2015, "1.252"),
    (2016, "1.252"),
    (2017, "1.228"),
    (2018, "1.208"),
    (2019, "1.197"),
    (2020, "1.197"),
    (2021, "1.161"),
    (2022, "1.074"),
    (2023, "1.036"),
    (2024, "1.000"),
];

fn shipped_table(disposal_year: i32) -> Option<&'static [(i32, &'static str)]> {
    match disposal_year {
        2024 => Some(COEFFICIENTS_2024),
        2025 => Some(COEFFICIENTS_2025),
        2026 => Some(COEFFICIENTS_2026),
        _ => None,
    }
}

/// The coefficient a FIFO lot's acquisition cost is multiplied by, for a Gipuzkoa disposal.
///
/// `overrides` is `taxes.spain.coefficients`, keyed by disposal year then acquisition year. An
/// override wins over the shipped table for that exact acquisition year, and a disposal year the
/// tool ships no table for can be supplied entirely from config — so a newly published Decreto
/// Foral is usable without waiting for a release.
///
/// Takes the acquisition **date**, not just its year, because of the statutory seam at the bottom
/// of the table: an asset acquired on exactly 31 December 1994 takes the 1995 coefficient, not the
/// "1994 y anteriores" one.
/// Largest coefficient the config will accept.
///
/// The oldest shipped row is 2.156, and the tables track consumer prices over three decades, so
/// anything past 10 is a decimal-point slip or a percentage typed as a multiplier — not a
/// coefficient. Better to refuse than to actualize a cost basis by 105× and report the loss.
const MAX_COEFFICIENT: Decimal = dec!(10);

/// Reject nonsensical coefficient overrides before any lot is priced.
///
/// A coefficient multiplies the acquisition cost, so a zero or negative one does not shrink a gain,
/// it invents one out of the wrong sign — and the failure would surface as a plausible number on a
/// tax return rather than as an error. Mirrors `LossLedger::from_config`: name the config path and
/// refuse, never silently clamp.
pub fn validate_overrides(overrides: &BTreeMap<i32, BTreeMap<i32, Decimal>>) -> EmptyResult {
    for (&disposal_year, table) in overrides {
        for (&acquisition_year, &coefficient) in table {
            let path = format!("taxes.spain.coefficients.{disposal_year}.{acquisition_year}");

            if coefficient <= Decimal::ZERO {
                return Err!(
                    "{path} is {coefficient}: an actualization coefficient multiplies the \
                     acquisition cost, so it must be positive"
                );
            }

            if coefficient > MAX_COEFFICIENT {
                return Err!(
                    "{path} is {coefficient}, which is not a plausible actualization coefficient \
                     (the oldest shipped row is 2.156). Check the Decreto Foral table"
                );
            }
        }
    }

    Ok(())
}

pub fn gipuzkoa_coefficient(
    disposal_year: i32,
    acquisition_date: Date,
    overrides: &BTreeMap<i32, BTreeMap<i32, Decimal>>,
) -> GenericResult<Decimal> {
    // "No obstante, cuando el elemento patrimonial hubiese sido adquirido el 31 de diciembre de
    // 1994 será de aplicación el coeficiente de actualización correspondiente a 1995." Present in
    // all three Decretos Forales.
    let acquisition_year = if acquisition_date == date!(1994, 12, 31) {
        1995
    } else {
        acquisition_date.year()
    };

    if acquisition_year > disposal_year {
        return Err!(
            "Cannot actualize a {disposal_year} disposal against a {acquisition_year} acquisition: \
             an asset cannot be acquired after it is transferred"
        );
    }

    let overridden = overrides.get(&disposal_year);

    if let Some(&coefficient) = overridden.and_then(|table| table.get(&acquisition_year)) {
        return Ok(coefficient);
    }

    let Some(table) = shipped_table(disposal_year) else {
        return Err!(
            "No Gipuzkoa actualization coefficient is known for a {disposal_year} disposal of an \
             asset acquired in {acquisition_year}. The Diputación Foral approves the table each \
             December by Decreto Foral; set `taxes.spain.coefficients.{disposal_year}` in the \
             config to supply it"
        );
    };

    // Rows run from "1994 y anteriores" up to the disposal year, so the last row at or below the
    // acquisition year is the match, and anything older than the first row clamps onto it.
    let row = table
        .iter()
        .rev()
        .find(|(year, _)| *year <= acquisition_year)
        .unwrap_or(&table[0]);

    row.1
        .parse()
        .map_err(|_| format!("Malformed shipped coefficient {:?} for {}", row.1, row.0).into())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn no_overrides() -> BTreeMap<i32, BTreeMap<i32, Decimal>> {
        BTreeMap::new()
    }

    fn coefficient(disposal_year: i32, acquisition_year: i32) -> Decimal {
        gipuzkoa_coefficient(
            disposal_year,
            date!(acquisition_year, 6, 15),
            &no_overrides(),
        )
        .unwrap()
    }

    /// A coefficient multiplies the acquisition cost, so a non-positive one flips the sign of the
    /// cost basis and reports a gain that never happened. An absurdly large one is a decimal-point
    /// slip. Both are refused with the config path named, never clamped.
    #[rstest]
    #[case("0", "must be positive")]
    #[case("-1.05", "must be positive")]
    #[case("10.001", "not a plausible")]
    #[case("105", "not a plausible")]
    fn absurd_overrides_are_rejected(#[case] value: &str, #[case] expected: &str) {
        let overrides = BTreeMap::from([(
            2027,
            BTreeMap::from([(2020, value.parse::<Decimal>().unwrap())]),
        )]);

        let error = validate_overrides(&overrides).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("taxes.spain.coefficients.2027.2020"), "{error}");
    }

    /// The guard must not reject a coefficient the Diputación Foral could actually publish: the
    /// oldest shipped row is already 2.156, and deflation could in principle put one below 1.
    #[rstest]
    #[case("0.98")]
    #[case("1")]
    #[case("2.156")]
    #[case("10")]
    fn plausible_overrides_are_accepted(#[case] value: &str) {
        let overrides = BTreeMap::from([(
            2027,
            BTreeMap::from([(2020, value.parse::<Decimal>().unwrap())]),
        )]);
        validate_overrides(&overrides).unwrap();
    }

    /// Spot values from each Decreto Foral, including the two the `fifo` fixture is pinned to
    /// (2021 and 2024 acquisitions disposed in 2026).
    #[rstest]
    #[case(2026, 2021, "1.212")]
    #[case(2026, 2024, "1.050")]
    #[case(2026, 2025, "1.020")]
    #[case(2026, 2000, "1.866")]
    #[case(2025, 2021, "1.175")]
    #[case(2025, 2016, "1.267")]
    #[case(2024, 2021, "1.161")]
    #[case(2024, 2008, "1.370")]
    fn published_coefficients(
        #[case] disposal_year: i32,
        #[case] acquisition_year: i32,
        #[case] expected: &str,
    ) {
        assert_eq!(
            coefficient(disposal_year, acquisition_year),
            expected.parse::<Decimal>().unwrap()
        );
    }

    /// An asset sold in the year it was bought is not actualized at all.
    #[rstest]
    #[case(2024)]
    #[case(2025)]
    #[case(2026)]
    fn same_year_disposal_is_not_actualized(#[case] year: i32) {
        assert_eq!(coefficient(year, year), dec!(1.000));
    }

    /// The table bottoms out at "1994 y anteriores", so anything older takes that row rather than
    /// falling through to an error or, worse, to a coefficient of 1.
    #[rstest]
    #[case(1994)]
    #[case(1980)]
    #[case(1900)]
    fn acquisitions_older_than_the_table_clamp_to_its_first_row(#[case] acquisition_year: i32) {
        assert_eq!(coefficient(2026, acquisition_year), dec!(2.030));
    }

    /// The statutory seam: 31 December 1994 takes the 1995 coefficient, every other 1994 date takes
    /// the "1994 y anteriores" one. This is why the function takes a date rather than a year.
    #[test]
    fn new_years_eve_1994_takes_the_1995_coefficient() {
        let at = |date| gipuzkoa_coefficient(2026, date, &no_overrides()).unwrap();

        assert_eq!(at(date!(1994, 12, 31)), dec!(2.156));
        assert_eq!(at(date!(1994, 12, 30)), dec!(2.030));
        assert_eq!(at(date!(1994, 1, 1)), dec!(2.030));
        // And it really is the 1995 row, not a coincidence.
        assert_eq!(at(date!(1995, 7, 1)), dec!(2.156));
    }

    /// The table is NOT monotonic in the acquisition year: 1995 sits above "1994 y anteriores" in
    /// every published year. A plausible-looking "older means larger" assumption would be wrong.
    #[rstest]
    #[case(2024)]
    #[case(2025)]
    #[case(2026)]
    fn the_1995_row_exceeds_the_catch_all_row(#[case] disposal_year: i32) {
        assert!(coefficient(disposal_year, 1995) > coefficient(disposal_year, 1994));
    }

    /// Every shipped table must be well formed: contiguous ascending acquisition years from 1994 to
    /// the disposal year, all values parseable and ≥ 1, ending at exactly 1.000. Catches a
    /// transcription slip that a handful of spot values would miss.
    #[rstest]
    #[case(2024)]
    #[case(2025)]
    #[case(2026)]
    fn shipped_tables_are_well_formed(#[case] disposal_year: i32) {
        let table = shipped_table(disposal_year).unwrap();

        assert_eq!(table[0].0, 1994, "the first row is 1994 y anteriores");
        assert_eq!(table[table.len() - 1].0, disposal_year);
        assert_eq!(table.len(), (disposal_year - 1994 + 1) as usize);

        for (index, &(year, raw)) in table.iter().enumerate() {
            assert_eq!(year, 1994 + index as i32, "years must be contiguous");
            let value: Decimal = raw.parse().unwrap_or_else(|_| panic!("unparseable {raw:?}"));
            assert!(value >= dec!(1), "{year}: {value} actualizes downwards");
        }

        assert_eq!(
            table[table.len() - 1].1.parse::<Decimal>().unwrap(),
            dec!(1.000)
        );
    }

    /// Inflation accumulates, so for any given acquisition year a later disposal must actualize by
    /// at least as much. Cross-checks the three tables against each other.
    #[rstest]
    #[case(1994)]
    #[case(2000)]
    #[case(2010)]
    #[case(2021)]
    #[case(2023)]
    fn later_disposals_actualize_at_least_as_much(#[case] acquisition_year: i32) {
        assert!(coefficient(2025, acquisition_year) > coefficient(2024, acquisition_year));
        assert!(coefficient(2026, acquisition_year) > coefficient(2025, acquisition_year));
    }

    /// A config override replaces the shipped value for that exact acquisition year and leaves the
    /// rest of the table alone.
    #[test]
    fn overrides_win_over_the_shipped_table() {
        let mut overrides = BTreeMap::new();
        overrides.insert(2026, BTreeMap::from([(2021, dec!(1.5))]));

        assert_eq!(
            gipuzkoa_coefficient(2026, date!(2021, 6, 15), &overrides).unwrap(),
            dec!(1.5)
        );
        // An acquisition year the override does not mention still comes from the shipped table.
        assert_eq!(
            gipuzkoa_coefficient(2026, date!(2024, 6, 15), &overrides).unwrap(),
            dec!(1.050)
        );
        // And a different disposal year is untouched.
        assert_eq!(
            gipuzkoa_coefficient(2025, date!(2021, 6, 15), &overrides).unwrap(),
            dec!(1.175)
        );
    }

    /// A newly published Decreto Foral can be supplied entirely from config, without a release.
    #[test]
    fn overrides_extend_to_unshipped_disposal_years() {
        let mut overrides = BTreeMap::new();
        overrides.insert(2027, BTreeMap::from([(2020, dec!(1.11))]));

        assert_eq!(
            gipuzkoa_coefficient(2027, date!(2020, 6, 15), &overrides).unwrap(),
            dec!(1.11)
        );
        // An acquisition year the config does not cover still errors rather than guessing.
        assert!(gipuzkoa_coefficient(2027, date!(2019, 6, 15), &overrides).is_err());
    }

    /// A disposal year with no shipped table and no config must name the config key to set, not
    /// silently fall back to 1 (which would tax the inflation component as if it were real gain).
    #[test]
    fn unshipped_disposal_year_errors_naming_the_config_key() {
        let error = gipuzkoa_coefficient(2027, date!(2021, 6, 15), &no_overrides())
            .unwrap_err()
            .to_string();

        assert!(error.contains("taxes.spain.coefficients.2027"), "{error}");
        assert!(error.contains("Decreto Foral"), "{error}");
    }

    /// An acquisition after its own disposal is a data bug upstream, not something to actualize.
    #[test]
    fn acquisition_after_disposal_is_rejected() {
        assert!(gipuzkoa_coefficient(2024, date!(2025, 6, 15), &no_overrides()).is_err());
    }
}

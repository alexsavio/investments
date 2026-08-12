//! Savings-base tax scales (escala de la base liquidable del ahorro).

use rust_decimal::RoundingStrategy;

use crate::core::GenericResult;
use crate::types::Decimal;

use super::SpanishTaxRegime;

/// Progressive scale applied to the savings base (base liquidable del ahorro).
///
/// Brackets are `(floor, marginal rate)` in ascending order, the first with floor 0. This is
/// deliberately *not* [`crate::taxes::ProgressiveTaxRate`]: that type accumulates `tax_base`
/// across calls (correct for its own use, wrong here), rounds every slice, and exposes no average
/// rate — and the average savings rate is exactly what the double-taxation credit is capped at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavingsScale {
    brackets: Vec<(Decimal, Decimal)>,
}

impl SavingsScale {
    /// First tax year this tool ships a scale for.
    pub const FIRST_YEAR: i32 = 2024;
    /// Last tax year this tool ships a scale for.
    pub const LAST_YEAR: i32 = 2026;

    /// The scale in force for `regime` in `year`.
    ///
    /// Errors rather than extrapolating: a savings scale is set by statute each year, so guessing
    /// one would file real money under an invented rate.
    pub fn for_year(regime: SpanishTaxRegime, year: i32) -> GenericResult<SavingsScale> {
        let brackets = match regime {
            SpanishTaxRegime::Gipuzkoa => match year {
                // NF 3/2014 art. 76.1 as it stood before the NF 1/2025 reform. Published as bands
                // of the base, with no cumulative cuota-íntegra column.
                2024 | 2025 => vec![
                    (dec!(0), dec!(0.20)),
                    (dec!(2_500), dec!(0.21)),
                    (dec!(10_000), dec!(0.22)),
                    (dec!(15_000), dec!(0.23)),
                    (dec!(30_000), dec!(0.25)),
                ],
                // NF 1/2025 art. 13.Tres -> NF 3/2014 art. 76.1, effective 2026-01-01 (disposición
                // final segunda). Nine brackets; the law publishes its own cuota-íntegra column,
                // which `tax()` reproduces exactly (see the unit tests).
                2026 => vec![
                    (dec!(0), dec!(0.19)),
                    (dec!(7_500), dec!(0.20)),
                    (dec!(15_000), dec!(0.22)),
                    (dec!(30_000), dec!(0.24)),
                    (dec!(50_000), dec!(0.255)),
                    (dec!(90_000), dec!(0.26)),
                    (dec!(120_000), dec!(0.265)),
                    (dec!(240_000), dec!(0.27)),
                    (dec!(300_000), dec!(0.28)),
                ],
                _ => return Err!("{}", Self::unsupported_year(regime, year)),
            },
            SpanishTaxRegime::Comun => match year {
                // LIRPF arts. 66.1.1º + 76 (aggregate = art. 66.2). The >300,000 bracket was
                // created at 28% by Ley 31/2022 (PGE 2023).
                2024 => vec![
                    (dec!(0), dec!(0.19)),
                    (dec!(6_000), dec!(0.21)),
                    (dec!(50_000), dec!(0.23)),
                    (dec!(200_000), dec!(0.27)),
                    (dec!(300_000), dec!(0.28)),
                ],
                // Ley 7/2024 df 7ª raised the top bracket to 30% with effect from 2025-01-01.
                // Unchanged for 2026: arts. 66/76 were not amended since, and the 2025 budget was
                // prorogued so there is no PGE 2026 to change them.
                2025 | 2026 => vec![
                    (dec!(0), dec!(0.19)),
                    (dec!(6_000), dec!(0.21)),
                    (dec!(50_000), dec!(0.23)),
                    (dec!(200_000), dec!(0.27)),
                    (dec!(300_000), dec!(0.30)),
                ],
                _ => return Err!("{}", Self::unsupported_year(regime, year)),
            },
            // TRLFIRPF art. 60, in the wording LF 36/2022 gave it with effect from 2023-01-01.
            // One table for every shipped year: LF 17/2025, which carries the 2026 changes, does
            // not touch art. 60. The law publishes its own cumulative cuota-íntegra column, which
            // `tax()` reproduces exactly (see the unit tests).
            SpanishTaxRegime::Navarra => match year {
                2024..=2026 => vec![
                    (dec!(0), dec!(0.20)),
                    (dec!(6_000), dec!(0.22)),
                    (dec!(10_000), dec!(0.24)),
                    (dec!(15_000), dec!(0.26)),
                    (dec!(200_000), dec!(0.27)),
                    (dec!(300_000), dec!(0.28)),
                ],
                _ => return Err!("{}", Self::unsupported_year(regime, year)),
            },
        };

        Ok(SavingsScale { brackets })
    }

    fn unsupported_year(regime: SpanishTaxRegime, year: i32) -> String {
        format!(
            "No Spanish savings-base scale is shipped for {regime:?} tax year {year}: \
             supported years are {}-{}",
            Self::FIRST_YEAR,
            Self::LAST_YEAR
        )
    }

    /// The `(floor, marginal rate)` brackets, ascending.
    pub fn brackets(&self) -> &[(Decimal, Decimal)] {
        &self.brackets
    }

    /// Cuota íntegra del ahorro for a savings base.
    ///
    /// Full precision throughout: no slice is rounded, and the total is not rounded either.
    /// Rounding belongs at the reporting boundary, once per figure. Rounding here would bias
    /// [`average_rate`](Self::average_rate), which caps the double-taxation credit.
    ///
    /// A non-positive base is untaxed. Compensation already floors the base at zero, so this is a
    /// belt-and-braces guard: a negative "tax" would be netted against real liability elsewhere.
    pub fn tax(&self, base: Decimal) -> Decimal {
        if base <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        let mut tax = Decimal::ZERO;

        for (index, &(floor, rate)) in self.brackets.iter().enumerate() {
            if base <= floor {
                break;
            }

            // The bracket runs to the next floor, or to infinity for the last one.
            let upper = match self.brackets.get(index + 1) {
                Some(&(next_floor, _)) => std::cmp::min(base, next_floor),
                None => base,
            };

            tax += (upper - floor) * rate;
        }

        tax
    }

    /// Average effective savings rate, `tax / base`, zero for a non-positive base.
    ///
    /// This is the "tipo medio de gravamen del ahorro" the double-taxation credit is capped at
    /// (NF 3/2014 art. 91.b, art. 76.2 / LIRPF art. 80.1.b, art. 80.2).
    ///
    /// Both statutes require it "expresado con dos decimales" — two decimals **as a percentage**,
    /// so four as a fraction. That is operative, not presentation: the AEAT Manual Práctico de
    /// Renta cap. 18 works its own example from the rounded rate (16,60% × 6.000 € = 996 €), so the
    /// rounding happens here, before the cap is multiplied out.
    ///
    /// Half away from zero, matching every EUR amount the tool prints. The rate is a quotient of two
    /// non-negative figures, so the negative-exact-half divergence noted on `tax_statement::eur`
    /// cannot arise here; the choice only matters for an exact half in the fifth decimal, where it
    /// rounds the rate up and so the credit cap up by a fraction of a cent.
    pub fn average_rate(&self, base: Decimal) -> Decimal {
        if base <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        (self.tax(base) / base)
            .round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// The 2026 Gipuzkoa scale must reproduce the bracket table of NF 3/2014 art. 76.1 exactly.
    #[test]
    fn gipuzkoa_2026_brackets_match_the_law() {
        let scale = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2026).unwrap();
        assert_eq!(
            scale.brackets(),
            [
                (dec!(0), dec!(0.19)),
                (dec!(7500), dec!(0.20)),
                (dec!(15000), dec!(0.22)),
                (dec!(30000), dec!(0.24)),
                (dec!(50000), dec!(0.255)),
                (dec!(90000), dec!(0.26)),
                (dec!(120000), dec!(0.265)),
                (dec!(240000), dec!(0.27)),
                (dec!(300000), dec!(0.28)),
            ]
        );
    }

    /// 2024 and 2025 share the pre-reform foral scale; 2026 is the reformed one. Guards against
    /// the reform leaking backwards into the years it does not govern.
    #[test]
    fn gipuzkoa_2024_and_2025_share_the_pre_reform_scale() {
        let y2024 = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2024).unwrap();
        let y2025 = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2025).unwrap();
        let y2026 = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2026).unwrap();

        assert_eq!(y2024, y2025);
        assert_ne!(y2025, y2026);
        assert_eq!(y2024.brackets().len(), 5);
        assert_eq!(y2024.brackets()[0], (dec!(0), dec!(0.20)));
        assert_eq!(y2024.brackets()[4], (dec!(30000), dec!(0.25)));
    }

    /// The state scale is year-sensitive in exactly one place: the top bracket rose from 28% to
    /// 30% for 2025 (Ley 7/2024 df 7ª). Everything below 300,000 is identical across 2024-2026.
    #[test]
    fn comun_top_bracket_rose_from_28_to_30_percent_in_2025() {
        let y2024 = SavingsScale::for_year(SpanishTaxRegime::Comun, 2024).unwrap();
        let y2025 = SavingsScale::for_year(SpanishTaxRegime::Comun, 2025).unwrap();
        let y2026 = SavingsScale::for_year(SpanishTaxRegime::Comun, 2026).unwrap();

        assert_eq!(y2025, y2026);
        assert_ne!(y2024, y2025);
        assert_eq!(y2024.brackets()[..4], y2025.brackets()[..4]);
        assert_eq!(y2024.brackets()[4], (dec!(300000), dec!(0.28)));
        assert_eq!(y2025.brackets()[4], (dec!(300000), dec!(0.30)));
    }

    /// Every shipped year resolves for both regimes, and the brackets are well-formed: ascending
    /// floors starting at zero, with strictly positive rates.
    #[rstest]
    fn shipped_years_are_well_formed(
        #[values(SpanishTaxRegime::Gipuzkoa, SpanishTaxRegime::Comun)] regime: SpanishTaxRegime,
        #[values(2024, 2025, 2026)] year: i32,
    ) {
        let scale = SavingsScale::for_year(regime, year).unwrap();
        let brackets = scale.brackets();

        assert!(!brackets.is_empty());
        assert_eq!(brackets[0].0, dec!(0), "the first bracket must start at zero");
        for pair in brackets.windows(2) {
            assert!(pair[0].0 < pair[1].0, "bracket floors must ascend");
        }
        assert!(brackets.iter().all(|&(_, rate)| rate > dec!(0)));
    }

    /// The law publishes its own cumulative cuota íntegra at every bracket threshold, so these are
    /// not hand-derived expectations but the statute's own numbers. If `tax()` reproduces all nine,
    /// the bracket table and the slice arithmetic are both right.
    #[rstest]
    #[case("7500", "1425")]
    #[case("15000", "2925")]
    #[case("30000", "6225")]
    #[case("50000", "11025")]
    #[case("90000", "21225")]
    #[case("120000", "29025")]
    #[case("240000", "60825")]
    #[case("300000", "77025")]
    // Above the last threshold there is no published cuota; 77,025 + 100,000 × 28%.
    #[case("400000", "105025")]
    // Mid-bracket: 7,500 × 19% + 2,500 × 20%.
    #[case("10000", "1925")]
    fn gipuzkoa_2026_reproduces_the_published_cuota_integra(
        #[case] base: &str,
        #[case] expected: &str,
    ) {
        let scale = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2026).unwrap();
        assert_eq!(
            scale.tax(base.parse().unwrap()),
            expected.parse::<Decimal>().unwrap()
        );
    }

    #[rstest]
    #[case("2500", "500")]
    #[case("10000", "2075")]
    #[case("15000", "3175")]
    #[case("30000", "6625")]
    #[case("50000", "11625")]
    fn gipuzkoa_pre_reform_cuota_integra(#[case] base: &str, #[case] expected: &str) {
        for year in [2024, 2025] {
            let scale = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, year).unwrap();
            assert_eq!(
                scale.tax(base.parse().unwrap()),
                expected.parse::<Decimal>().unwrap(),
                "year {year}"
            );
        }
    }

    #[rstest]
    #[case("6000", "1140")]
    #[case("50000", "10380")]
    #[case("200000", "44880")]
    #[case("300000", "71880")]
    #[case("400000", "101880")]
    fn comun_2025_and_2026_cuota_integra(#[case] base: &str, #[case] expected: &str) {
        for year in [2025, 2026] {
            let scale = SavingsScale::for_year(SpanishTaxRegime::Comun, year).unwrap();
            assert_eq!(
                scale.tax(base.parse().unwrap()),
                expected.parse::<Decimal>().unwrap(),
                "year {year}"
            );
        }
    }

    /// The 28%-vs-30% top bracket is the only place the state scale moves inside the shipped range,
    /// so it gets its own vector: below 300,000 the two years agree to the cent.
    #[test]
    fn comun_2024_differs_from_2025_only_above_the_top_threshold() {
        let y2024 = SavingsScale::for_year(SpanishTaxRegime::Comun, 2024).unwrap();
        let y2025 = SavingsScale::for_year(SpanishTaxRegime::Comun, 2025).unwrap();

        assert_eq!(y2024.tax(dec!(300000)), dec!(71880));
        assert_eq!(y2024.tax(dec!(400000)), dec!(99880));
        assert_eq!(y2025.tax(dec!(400000)), dec!(101880));
        // The 2,000 gap is exactly 100,000 × (30% − 28%).
        assert_eq!(y2025.tax(dec!(400000)) - y2024.tax(dec!(400000)), dec!(2000));
    }

    /// A savings base cannot be negative for tax purposes: compensation floors it at zero before
    /// the scale ever sees it, and a negative base must never produce a negative "tax" that would
    /// be netted off against real liability elsewhere.
    #[rstest]
    #[case("0")]
    #[case("-100")]
    #[case("-1000000")]
    fn non_positive_base_is_untaxed(#[case] base: &str) {
        for regime in [SpanishTaxRegime::Gipuzkoa, SpanishTaxRegime::Comun] {
            let scale = SavingsScale::for_year(regime, 2026).unwrap();
            assert_eq!(scale.tax(base.parse().unwrap()), dec!(0));
            assert_eq!(scale.average_rate(base.parse().unwrap()), dec!(0));
        }
    }

    /// The scale must not round per slice. `ProgressiveTaxRate` does (correctly, for its own use),
    /// which is why this type exists separately: the double-taxation credit is capped by the
    /// average rate, and rounding each slice would bias that cap.
    #[test]
    fn slices_are_not_rounded() {
        let scale = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2026).unwrap();
        // 7,500 × 19% + 0.555 × 20% = 1425 + 0.111.
        assert_eq!(scale.tax(dec!(7500.555)), dec!(1425.111));
        // A sub-cent base still produces a sub-cent tax rather than collapsing to zero.
        assert_eq!(scale.tax(dec!(0.001)), dec!(0.00019));
    }

    /// The average rate is what caps the double-taxation credit (NF art. 91.b / LIRPF art. 80.1.b),
    /// so it is the marginal rate only in the first bracket and strictly below it thereafter.
    #[test]
    fn average_rate_is_tax_over_base() {
        let scale = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2026).unwrap();

        // 1,925 / 10,000 = 19.25%, already exact at two decimals.
        assert_eq!(scale.average_rate(dec!(10000)), dec!(0.1925));
        // Inside the first bracket the average rate is the marginal rate.
        assert_eq!(scale.average_rate(dec!(5000)), dec!(0.19));
        // At the top the average stays well under the 28% marginal rate.
        assert!(scale.average_rate(dec!(400000)) < dec!(0.28));
    }

    /// NF 3/2014 art. 76.2 and LIRPF art. 80.2 both require the tipo medio to be "expresado con dos
    /// decimales" — two decimals **as a percentage**, i.e. four as a fraction. It is an operative
    /// rule, not presentation: the credit cap is computed from the rounded rate, so the tool has to
    /// round before multiplying or it credits cents the return does not allow.
    #[rstest]
    // 3,957.24 / 19,692 = 20.095673…% → 20.10%.
    #[case(dec!(19692), dec!(0.2010))]
    // 105,025 / 400,000 = 26.25625% → 26.26%.
    #[case(dec!(400000), dec!(0.2626))]
    // Already exact at two decimals: rounding must not move it.
    #[case(dec!(10000), dec!(0.1925))]
    #[case(dec!(5000), dec!(0.19))]
    fn average_rate_is_expressed_with_two_decimals(#[case] base: Decimal, #[case] expected: Decimal) {
        let scale = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2026).unwrap();
        assert_eq!(scale.average_rate(base), expected);
    }

    /// Guards the fixture the statement tests are pinned to: the Gipuzkoa 2026 savings base of
    /// €19,692 produced by the `fifo` fixture.
    #[test]
    fn gipuzkoa_2026_fixture_base() {
        let scale = SavingsScale::for_year(SpanishTaxRegime::Gipuzkoa, 2026).unwrap();
        // 1,425 + 1,500 + 4,692 × 22%.
        assert_eq!(scale.tax(dec!(19692)), dec!(3957.24));
    }

    /// An unshipped year errors and the message names the supported range, so the user is told how
    /// to fix it rather than being handed a silently extrapolated rate.
    #[rstest]
    #[case(2023)]
    #[case(2027)]
    fn unsupported_year_errors_naming_the_range(#[case] year: i32) {
        for regime in [SpanishTaxRegime::Gipuzkoa, SpanishTaxRegime::Comun] {
            let error = SavingsScale::for_year(regime, year).unwrap_err().to_string();
            assert!(error.contains(&year.to_string()), "{error}");
            assert!(error.contains("2024-2026"), "{error}");
        }
    }
}

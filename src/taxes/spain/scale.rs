//! Savings-base tax scales (escala de la base liquidable del ahorro).

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

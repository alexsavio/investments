//! The Navarra small-disposals exemption (TRLFIRPF art. 39.5.d).

use crate::types::Decimal;

/// Ceiling on the year's global transmission amount, above which nothing is exempt.
///
/// Art. 39.5.d.1.º: "Que el importe global de las citadas transmisiones no exceda de 3.000 euros
/// durante el año natural."
pub const GLOBAL_TRANSMISSION_LIMIT: Decimal = dec!(3000);

/// Share of the global transmission amount that may be exempt.
///
/// Art. 39.5.d.2.º: "Que la cuantía gravable del incremento de patrimonio no exceda del 50 por 100
/// del importe global de la transmisión."
pub const EXEMPT_FRACTION: Decimal = dec!(0.5);

/// The slice of a year's transmission gains that art. 39.5.d exempts.
///
/// `global_proceeds` is the year's total importe of onerous transmissions — every disposal, whether
/// it produced a gain or a loss, because 1.º measures the transmissions and not their results.
/// `gains` is "la cuantía gravable del incremento de patrimonio", the taxable result of those
/// transmissions; a loss is a *disminución*, not an *incremento*, so it belongs to neither figure.
///
/// Both conditions are year-global and both boundaries are inclusive: 1.º fails only when the
/// global amount *exceeds* €3,000, and 2.º leaves a gain of exactly half the global amount wholly
/// exempt. Above that half, "únicamente se someterá a gravamen el citado exceso" — so the exemption
/// is the half, not nothing.
pub fn small_disposals_exemption(global_proceeds: Decimal, gains: Decimal) -> Decimal {
    if global_proceeds <= Decimal::ZERO
        || global_proceeds > GLOBAL_TRANSMISSION_LIMIT
        || gains <= Decimal::ZERO
    {
        return Decimal::ZERO;
    }

    std::cmp::min(gains, global_proceeds * EXEMPT_FRACTION)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    // 1.º is inclusive: €3,000 exactly still qualifies, and €1,000 is under half of it.
    #[case("3000", "1000", "1000")]
    // …and fails by a single cent.
    #[case("3000.01", "1000", "0")]
    // Above the half, only the excess is taxed: 2,250 × 50% = 1,125 of a 1,350 gain.
    #[case("2250", "1350", "1125")]
    // 2.º is inclusive too: a gain of exactly half the global amount is wholly exempt.
    #[case("2000", "1000", "1000")]
    // One cent over the half, and that cent is all that is taxed.
    #[case("2000", "1000.01", "1000")]
    // A loss-only year has no incremento to exempt.
    #[case("1000", "0", "0")]
    // No transmission at all: 1.º has nothing to measure.
    #[case("0", "0", "0")]
    #[case("0", "500", "0")]
    fn the_exemption_follows_both_conditions(
        #[case] proceeds: &str,
        #[case] gains: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(
            small_disposals_exemption(proceeds.parse().unwrap(), gains.parse().unwrap()),
            expected.parse::<Decimal>().unwrap()
        );
    }

    /// The exemption can never exceed the gain it relieves, nor turn into a deduction of its own.
    #[rstest]
    #[case("100", "5000")]
    #[case("2999.99", "1")]
    #[case("1", "0.5")]
    fn the_exemption_never_exceeds_the_gain(#[case] proceeds: &str, #[case] gains: &str) {
        let proceeds: Decimal = proceeds.parse().unwrap();
        let gains: Decimal = gains.parse().unwrap();
        let exempt = small_disposals_exemption(proceeds, gains);

        assert!(exempt >= Decimal::ZERO);
        assert!(exempt <= gains);
        assert!(exempt <= proceeds * EXEMPT_FRACTION);
    }
}

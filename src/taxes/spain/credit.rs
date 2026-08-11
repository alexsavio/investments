//! Deducción por doble imposición internacional (NF 3/2014 art. 91 / LIRPF art. 80).

use crate::types::Decimal;

/// Creditable foreign tax on foreign-source savings income.
///
/// Both statutes take the **lesser** of two limbs:
///
/// - what was actually paid abroad, which a double-taxation treaty caps at the agreed source rate,
///   so anything withheld above it is reclaimed from the source state rather than credited here;
/// - the average savings rate applied to the foreign-taxed income, so a credit can never exceed the
///   Spanish tax on that income.
///
/// A year-level figure, not a per-row one: the second limb depends on the average savings rate,
/// which only exists once the whole year's base is known.
pub fn double_taxation_credit(
    withheld_eur: Decimal,
    gross_eur: Decimal,
    treaty_rate: Decimal,
    average_savings_rate: Decimal,
) -> Decimal {
    let treaty_capped = std::cmp::min(withheld_eur, gross_eur * treaty_rate);
    let rate_capped = gross_eur * average_savings_rate;

    std::cmp::max(
        Decimal::ZERO,
        std::cmp::min(treaty_capped, rate_capped),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `income` fixture: €900 gross US dividend, €270 withheld (30%), 15% treaty rate, and a
    /// 19% average savings rate. The treaty limb binds at €135; the €135 excess withheld is not
    /// creditable in Spain.
    #[test]
    fn treaty_limb_binds_when_withholding_exceeds_the_treaty_rate() {
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(900), dec!(0.15), dec!(0.19)),
            dec!(135)
        );
    }

    /// When the source state withholds less than the treaty allows, only what was actually paid is
    /// creditable — the treaty is a ceiling, not an entitlement.
    #[test]
    fn only_tax_actually_paid_is_creditable() {
        assert_eq!(
            double_taxation_credit(dec!(90), dec!(900), dec!(0.15), dec!(0.19)),
            dec!(90)
        );
    }

    /// The average-rate limb binds when Spanish tax on the foreign income is lower than the treaty
    /// cap, so the credit can never exceed the Spanish tax on that income.
    #[test]
    fn average_rate_limb_binds_when_spanish_tax_is_lower() {
        // 900 × 10% = 90, below the 135 treaty cap.
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(900), dec!(0.15), dec!(0.10)),
            dec!(90)
        );
    }

    /// A year whose savings base is entirely absorbed by losses carries a zero average rate, so
    /// there is no Spanish tax for the foreign tax to be credited against.
    #[test]
    fn no_credit_without_spanish_tax() {
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(900), dec!(0.15), dec!(0)),
            dec!(0)
        );
    }

    /// Nothing withheld, nothing to credit — and never a negative credit, which would add tax.
    #[test]
    fn credit_is_never_negative() {
        assert_eq!(
            double_taxation_credit(dec!(0), dec!(900), dec!(0.15), dec!(0.19)),
            dec!(0)
        );
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(0), dec!(0.15), dec!(0.19)),
            dec!(0)
        );
    }
}

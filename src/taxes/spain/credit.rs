//! Deducción por doble imposición internacional (NF 3/2014 art. 91 / LIRPF art. 80).

use crate::types::Decimal;

/// Creditable foreign tax on foreign-source savings income.
///
/// Both statutes take the **lesser** of two limbs:
///
/// - what was actually paid abroad, which a double-taxation treaty caps at the agreed source rate,
///   so anything withheld above it is reclaimed from the source state rather than credited here.
///   Measured against `gross_eur`, the income the source state actually taxed;
/// - the average savings rate applied to `taxable_eur`, so a credit can never exceed the Spanish
///   tax on that income.
///
/// The two bases differ. TEAC resolución RG 00/08643/2023 (20-10-2025, unificación de criterio, and
/// therefore binding on the administration per LGT art. 239.8) settles that the second limb takes
/// **rentas netas** — the foreign income "una vez deducidos los gastos y compensadas las rentas",
/// i.e. as it actually reaches the base liquidable. Deductible expenses attributable to it come off,
/// and whatever compensation removed from the group it sits in is gone from it too.
///
/// A year-level figure, not a per-row one: the second limb depends on the average savings rate,
/// which only exists once the whole year's base is known.
pub fn double_taxation_credit(
    withheld_eur: Decimal,
    gross_eur: Decimal,
    taxable_eur: Decimal,
    treaty_rate: Decimal,
    average_savings_rate: Decimal,
) -> Decimal {
    let treaty_capped = std::cmp::min(withheld_eur, gross_eur * treaty_rate);
    let rate_capped = taxable_eur * average_savings_rate;

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
            double_taxation_credit(dec!(270), dec!(900), dec!(900), dec!(0.15), dec!(0.19)),
            dec!(135)
        );
    }

    /// When the source state withholds less than the treaty allows, only what was actually paid is
    /// creditable — the treaty is a ceiling, not an entitlement.
    #[test]
    fn only_tax_actually_paid_is_creditable() {
        assert_eq!(
            double_taxation_credit(dec!(90), dec!(900), dec!(900), dec!(0.15), dec!(0.19)),
            dec!(90)
        );
    }

    /// The average-rate limb binds when Spanish tax on the foreign income is lower than the treaty
    /// cap, so the credit can never exceed the Spanish tax on that income.
    #[test]
    fn average_rate_limb_binds_when_spanish_tax_is_lower() {
        // 900 × 10% = 90, below the 135 treaty cap.
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(900), dec!(900), dec!(0.15), dec!(0.10)),
            dec!(90)
        );
    }

    /// The two limbs measure different bases. The treaty limb is a ceiling on what the *source*
    /// state may levy, so it is measured on the gross the source state taxed; the rate limb is a
    /// ceiling on the *Spanish* tax on that income, so it is measured on the net that actually
    /// reached the base liquidable (TEAC RG 00/08643/2023, unificación de criterio).
    ///
    /// €900 gross with €41 of attributable expenses and a group half wiped by compensation leaves
    /// €430 taxed here: 430 × 19% = €81.70 against a €135 treaty cap.
    #[test]
    fn the_rate_limb_measures_the_net_income_actually_taxed_here() {
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(900), dec!(430), dec!(0.15), dec!(0.19)),
            dec!(81.70)
        );
    }

    /// AEAT Manual Práctico de Renta cap. 18: a tipo medio of 16,60% on €6,000 of foreign income
    /// gives a limb of €996. Pinned as a golden vector because it is the one place the manual shows
    /// the multiplication with the two-decimal rate.
    #[test]
    fn the_aeat_manual_rate_limb_vector() {
        assert_eq!(
            double_taxation_credit(dec!(5000), dec!(20000), dec!(6000), dec!(0.15), dec!(0.1660)),
            dec!(996)
        );
    }

    /// A year whose savings base is entirely absorbed by losses carries a zero average rate, so
    /// there is no Spanish tax for the foreign tax to be credited against.
    #[test]
    fn no_credit_without_spanish_tax() {
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(900), dec!(900), dec!(0.15), dec!(0)),
            dec!(0)
        );
    }

    /// Compensation that wipes the group the foreign income sits in leaves nothing for the credit
    /// to attach to, even when the year still pays tax on its other group.
    #[test]
    fn no_credit_when_compensation_wiped_the_income() {
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(900), dec!(0), dec!(0.15), dec!(0.19)),
            dec!(0)
        );
    }

    /// Nothing withheld, nothing to credit — and never a negative credit, which would add tax.
    #[test]
    fn credit_is_never_negative() {
        assert_eq!(
            double_taxation_credit(dec!(0), dec!(900), dec!(900), dec!(0.15), dec!(0.19)),
            dec!(0)
        );
        assert_eq!(
            double_taxation_credit(dec!(270), dec!(0), dec!(0), dec!(0.15), dec!(0.19)),
            dec!(0)
        );
    }
}

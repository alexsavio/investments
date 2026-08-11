//! Integration and compensation of the savings base (NF 3/2014 / LIRPF art. 49).

use log::warn;

use crate::types::Decimal;

use super::carryforward::{CARRYFORWARD_YEARS, LedgerApplication, LossLedger};

/// What compensation did to a year's two savings-base groups.
#[derive(Clone, Debug)]
pub struct CompensationResult {
    /// Positive balance of each group after compensation; these sum to the savings base.
    pub rcm_taxable: Decimal,
    pub gyp_taxable: Decimal,
    pub savings_base: Decimal,

    /// Prior-year balances consumed this year, per group, broken down by origin year.
    pub rcm_applied: LedgerApplication,
    pub gyp_applied: LedgerApplication,

    /// Fase 1ª: current-year negative of one group set against the other's positive (Común only).
    pub cross_offset_rcm_to_gyp: Decimal,
    pub cross_offset_gyp_to_rcm: Decimal,

    /// Fase 2ª-2º: prior-year balance of one group its own group could not absorb, set against the
    /// other's remaining positive (Común only).
    pub prior_cross_offset_rcm_to_gyp: Decimal,
    pub prior_cross_offset_gyp_to_rcm: Decimal,

    /// Balances carried into the following return.
    pub rcm_ledger_next: LossLedger,
    pub gyp_ledger_next: LossLedger,

    /// Balances that reached the end of their four-year window unused and are now lost.
    pub rcm_expired: Decimal,
    pub gyp_expired: Decimal,
}

/// Compensate one year's savings-base groups against each other and against prior-year balances.
///
/// `cross_offset_fraction` is 0 under Gipuzkoa, where the groups are integrated "exclusivamente
/// entre sí" and never touch (Gipuzkoa Manual de Renta cap. 9), and 0.25 under Territorio Común.
///
/// The Común order follows the AEAT Manual Práctico de Renta cap. 12 (LIRPF art. 49):
///
/// 1. **Fase 1ª** — the year's own results meet each other: a current-year negative reduces the
///    other group's current-year positive.
/// 2. **Fase 2ª-1º** — prior-year balances reduce what is left of *their own* group. Art. 49.2
///    requires absorbing the maximum each year, which `LossLedger::apply` does oldest vintage first.
/// 3. **Fase 2ª-2º** — a prior-year balance its own group could not absorb crosses into the other
///    group's remainder.
///
/// The 25% limit is one allowance per group, measured on that group's **original** current-year
/// positive and consumed across steps 1 and 3 together. That is what makes the manual's own example
/// come out at a base of 200 rather than 0 or 300.
pub fn compensate_savings_base(
    filing_year: i32,
    rcm_net: Decimal,
    gyp_net: Decimal,
    mut rcm_ledger: LossLedger,
    mut gyp_ledger: LossLedger,
    cross_offset_fraction: Decimal,
) -> CompensationResult {
    let zero = Decimal::ZERO;

    let rcm_positive = std::cmp::max(zero, rcm_net);
    let gyp_positive = std::cmp::max(zero, gyp_net);
    let mut rcm_negative = std::cmp::max(zero, -rcm_net);
    let mut gyp_negative = std::cmp::max(zero, -gyp_net);

    // One allowance per group, on the original positive.
    let mut rcm_allowance = rcm_positive * cross_offset_fraction;
    let mut gyp_allowance = gyp_positive * cross_offset_fraction;

    let mut rcm_taxable = rcm_positive;
    let mut gyp_taxable = gyp_positive;

    // Fase 1ª. Only one direction can apply in a given year: a group cannot be both negative and
    // positive, so at most one of these two has anything to work with.
    let mut cross_offset_rcm_to_gyp = zero;
    let mut cross_offset_gyp_to_rcm = zero;

    if cross_offset_fraction > zero {
        cross_offset_rcm_to_gyp = rcm_negative.min(gyp_taxable).min(gyp_allowance);
        gyp_taxable -= cross_offset_rcm_to_gyp;
        gyp_allowance -= cross_offset_rcm_to_gyp;
        rcm_negative -= cross_offset_rcm_to_gyp;

        cross_offset_gyp_to_rcm = gyp_negative.min(rcm_taxable).min(rcm_allowance);
        rcm_taxable -= cross_offset_gyp_to_rcm;
        rcm_allowance -= cross_offset_gyp_to_rcm;
        gyp_negative -= cross_offset_gyp_to_rcm;
    }

    // Fase 2ª-1º.
    let mut rcm_applied = rcm_ledger.apply(rcm_taxable, None);
    let mut gyp_applied = gyp_ledger.apply(gyp_taxable, None);
    rcm_taxable -= rcm_applied.used_total;
    gyp_taxable -= gyp_applied.used_total;

    // Fase 2ª-2º, within what is left of the same allowance.
    let mut prior_cross_offset_rcm_to_gyp = zero;
    let mut prior_cross_offset_gyp_to_rcm = zero;

    if cross_offset_fraction > zero {
        let crossed = rcm_ledger.apply(gyp_taxable, Some(gyp_allowance));
        prior_cross_offset_rcm_to_gyp = crossed.used_total;
        gyp_taxable -= crossed.used_total;
        rcm_applied.merge(crossed);

        let crossed = gyp_ledger.apply(rcm_taxable, Some(rcm_allowance));
        prior_cross_offset_gyp_to_rcm = crossed.used_total;
        rcm_taxable -= crossed.used_total;
        gyp_applied.merge(crossed);
    }

    // Whatever negative remains is this year's pending balance, labelled with this year so
    // its own four-year window starts now.
    rcm_ledger.add(filing_year, rcm_negative);
    gyp_ledger.add(filing_year, gyp_negative);

    // A balance whose fourth year this was can never be used again. Dropping it silently would
    // leave the user carrying a figure the tax office will not accept.
    let rcm_expired = rcm_ledger.expiring_after(filing_year);
    let gyp_expired = gyp_ledger.expiring_after(filing_year);
    let expired_origin = filing_year - CARRYFORWARD_YEARS;

    for (group, expired) in [("RCM", rcm_expired), ("ganancias", gyp_expired)] {
        if expired > zero {
            warn!(
                "€{expired} of pending {group} losses from {expired_origin} expired unused: a \
                 negative savings-base balance may only be offset in the {CARRYFORWARD_YEARS} \
                 following years, and {filing_year} was the last."
            );
        }
    }

    rcm_ledger.drop_expired(filing_year);
    gyp_ledger.drop_expired(filing_year);

    CompensationResult {
        rcm_taxable,
        gyp_taxable,
        savings_base: rcm_taxable + gyp_taxable,
        rcm_applied,
        gyp_applied,
        cross_offset_rcm_to_gyp,
        cross_offset_gyp_to_rcm,
        prior_cross_offset_rcm_to_gyp,
        prior_cross_offset_gyp_to_rcm,
        rcm_ledger_next: rcm_ledger,
        gyp_ledger_next: gyp_ledger,
        rcm_expired,
        gyp_expired,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rstest::rstest;

    use super::*;

    const GIPUZKOA: Decimal = Decimal::ZERO;

    fn comun() -> Decimal {
        dec!(0.25)
    }

    fn ledger(entries: &[(i32, &str)], filing_year: i32) -> LossLedger {
        let map: BTreeMap<i32, Decimal> = entries
            .iter()
            .map(|&(year, amount)| (year, amount.parse().unwrap()))
            .collect();
        LossLedger::from_config(&map, filing_year, "test").unwrap()
    }

    /// Two positive groups with no history: nothing to compensate, the base is their sum.
    #[test]
    fn positive_groups_sum_into_the_base() {
        let result = compensate_savings_base(
            2026,
            dec!(990),
            dec!(19692),
            LossLedger::default(),
            LossLedger::default(),
            GIPUZKOA,
        );

        assert_eq!(result.rcm_taxable, dec!(990));
        assert_eq!(result.gyp_taxable, dec!(19692));
        assert_eq!(result.savings_base, dec!(20682));
        assert!(result.rcm_ledger_next.is_empty());
        assert!(result.gyp_ledger_next.is_empty());
    }

    /// A prior-year balance reduces its own group's positive result, oldest vintage first, and what
    /// it cannot absorb stays pending.
    #[test]
    fn prior_year_balances_reduce_their_own_group() {
        let result = compensate_savings_base(
            2026,
            dec!(0),
            dec!(19692),
            LossLedger::default(),
            ledger(&[(2023, "5000"), (2025, "1000")], 2026),
            GIPUZKOA,
        );

        assert_eq!(result.gyp_applied.used_total, dec!(6000));
        // Oldest first.
        assert_eq!(result.gyp_applied.used_by_year[&2023], dec!(5000));
        assert_eq!(result.gyp_applied.used_by_year[&2025], dec!(1000));
        assert_eq!(result.gyp_taxable, dec!(13692));
        assert!(result.gyp_ledger_next.is_empty());
    }

    /// A current-year loss becomes a pending balance labelled with this year, so its own four-year
    /// window starts now rather than inheriting an older one.
    #[test]
    fn a_current_year_loss_carries_forward_labelled_with_this_year() {
        let result = compensate_savings_base(
            2026,
            dec!(0),
            dec!(-9360),
            LossLedger::default(),
            LossLedger::default(),
            GIPUZKOA,
        );

        assert_eq!(result.gyp_taxable, dec!(0));
        assert_eq!(result.savings_base, dec!(0));
        assert_eq!(result.gyp_ledger_next.balances()[&2026], dec!(9360));
    }

    /// Under Gipuzkoa the groups never touch: a negative RCM leaves a positive ganancias balance
    /// fully taxable and carries forward on its own.
    #[test]
    fn gipuzkoa_never_crosses_the_groups() {
        let result = compensate_savings_base(
            2026,
            dec!(-2000),
            dec!(6000),
            LossLedger::default(),
            LossLedger::default(),
            GIPUZKOA,
        );

        assert_eq!(result.gyp_taxable, dec!(6000));
        assert_eq!(result.savings_base, dec!(6000));
        assert_eq!(result.cross_offset_rcm_to_gyp, dec!(0));
        assert_eq!(result.rcm_ledger_next.balances()[&2026], dec!(2000));
    }

    /// The AEAT Manual Práctico de Renta, cap. 12, worked example — the acceptance test for the
    /// two-phase order.
    ///
    /// Current ganancias +4,000, current RCM −800, prior-year ganancias balance 2,800, prior-year
    /// RCM balance 500. The manual's answer is a savings base of **200**:
    ///
    /// - Fase 1ª: the current RCM −800 meets the current +4,000 first, inside the 25% allowance of
    ///   1,000 measured on that original positive → ganancias 3,200, allowance left 200.
    /// - Fase 2ª-1º: the prior-year ganancias balance 2,800 attacks the remainder → 400 left.
    /// - Fase 2ª-2º: the prior-year RCM balance crosses into that 400, but only for what is left of
    ///   the same 25% allowance → 200.
    #[test]
    fn comun_follows_the_manual_two_phase_order() {
        let result = compensate_savings_base(
            2026,
            dec!(-800),
            dec!(4000),
            ledger(&[(2024, "500")], 2026),
            ledger(&[(2024, "2800")], 2026),
            comun(),
        );

        assert_eq!(result.cross_offset_rcm_to_gyp, dec!(800));
        assert_eq!(result.gyp_applied.used_total, dec!(2800));
        assert_eq!(result.prior_cross_offset_rcm_to_gyp, dec!(200));
        assert_eq!(result.rcm_applied.used_total, dec!(200));

        assert_eq!(result.gyp_taxable, dec!(200));
        assert_eq!(result.rcm_taxable, dec!(0));
        assert_eq!(result.savings_base, dec!(200));

        // 300 of the prior RCM balance could not be used and keeps its 2024 vintage.
        assert_eq!(result.rcm_ledger_next.balances()[&2024], dec!(300));
        assert!(result.gyp_ledger_next.is_empty());
    }

    /// The 25% allowance is measured on the **original** current-year positive, not on what prior
    /// years left of it, and it is a single allowance shared by both cross-offset steps.
    ///
    /// Current RCM −2,000 against current ganancias +6,000 with a prior-year ganancias balance of
    /// 4,000. The allowance is 25% × 6,000 = 1,500, all of it consumed in Fase 1ª. The prior-year
    /// balance then attacks 4,500 and leaves 500, and nothing crosses in Fase 2ª-2º because the
    /// allowance is spent. Measuring the cap after the prior-year balance instead would cross only
    /// 500 and leave a base of 1,500.
    #[test]
    fn the_cross_offset_cap_is_measured_on_the_original_positive() {
        let result = compensate_savings_base(
            2026,
            dec!(-2000),
            dec!(6000),
            LossLedger::default(),
            ledger(&[(2024, "4000")], 2026),
            comun(),
        );

        assert_eq!(result.cross_offset_rcm_to_gyp, dec!(1500));
        assert_eq!(result.gyp_applied.used_total, dec!(4000));
        assert_eq!(result.prior_cross_offset_rcm_to_gyp, dec!(0));
        assert_eq!(result.savings_base, dec!(500));
        assert_eq!(result.rcm_ledger_next.balances()[&2026], dec!(500));
    }

    /// Only one direction of the current-year cross-offset can apply in a year: a group is either
    /// positive or negative, never both, so Fase 1ª always has at most one negative to place.
    #[rstest]
    #[case(dec!(-2000), dec!(6000))]
    #[case(dec!(6000), dec!(-2000))]
    fn only_one_current_year_direction_can_apply(#[case] rcm: Decimal, #[case] gyp: Decimal) {
        let result = compensate_savings_base(
            2026,
            rcm,
            gyp,
            LossLedger::default(),
            LossLedger::default(),
            comun(),
        );

        assert!(
            result.cross_offset_rcm_to_gyp.is_zero() || result.cross_offset_gyp_to_rcm.is_zero()
        );
        assert_eq!(
            result.cross_offset_rcm_to_gyp + result.cross_offset_gyp_to_rcm,
            dec!(1500)
        );
    }

    /// Gipuzkoa integrates the groups "exclusivamente entre sí" (Manual de Renta cap. 9), so the
    /// two-phase order changes nothing there: neither a current-year negative nor an unabsorbed
    /// prior-year balance ever reaches the other group, in either phase.
    #[test]
    fn gipuzkoa_is_unchanged_by_the_two_phase_order() {
        let result = compensate_savings_base(
            2026,
            dec!(-800),
            dec!(4000),
            ledger(&[(2024, "500")], 2026),
            ledger(&[(2024, "2800")], 2026),
            GIPUZKOA,
        );

        // Same inputs as the AEAT example; under Gipuzkoa the answer is 1,200, not 200.
        assert_eq!(result.cross_offset_rcm_to_gyp, dec!(0));
        assert_eq!(result.prior_cross_offset_rcm_to_gyp, dec!(0));
        assert_eq!(result.prior_cross_offset_gyp_to_rcm, dec!(0));
        assert_eq!(result.gyp_applied.used_total, dec!(2800));
        assert_eq!(result.rcm_applied.used_total, dec!(0));
        assert_eq!(result.gyp_taxable, dec!(1200));
        assert_eq!(result.savings_base, dec!(1200));
        // The whole prior RCM balance survives, plus this year's own negative.
        assert_eq!(result.rcm_ledger_next.balances()[&2024], dec!(500));
        assert_eq!(result.rcm_ledger_next.balances()[&2026], dec!(800));
    }

    /// Territorio Común: RCM −2,000 against ganancias +6,000 crosses at 25% of 6,000 = 1,500, so
    /// 4,500 stays taxable and 500 carries forward.
    #[test]
    fn comun_crosses_the_groups_up_to_a_quarter() {
        let result = compensate_savings_base(
            2026,
            dec!(-2000),
            dec!(6000),
            LossLedger::default(),
            LossLedger::default(),
            comun(),
        );

        assert_eq!(result.cross_offset_rcm_to_gyp, dec!(1500));
        assert_eq!(result.gyp_taxable, dec!(4500));
        assert_eq!(result.savings_base, dec!(4500));
        assert_eq!(result.rcm_ledger_next.balances()[&2026], dec!(500));
    }

    /// The cross-offset works in both directions.
    #[test]
    fn comun_crosses_from_ganancias_to_rcm_too() {
        let result = compensate_savings_base(
            2026,
            dec!(6000),
            dec!(-2000),
            LossLedger::default(),
            LossLedger::default(),
            comun(),
        );

        assert_eq!(result.cross_offset_gyp_to_rcm, dec!(1500));
        assert_eq!(result.rcm_taxable, dec!(4500));
        assert_eq!(result.gyp_ledger_next.balances()[&2026], dec!(500));
    }

    /// The cap binds on the loss, not on the quarter, when the loss is the smaller of the two.
    #[test]
    fn comun_cross_offset_is_limited_by_the_loss_itself() {
        let result = compensate_savings_base(
            2026,
            dec!(-100),
            dec!(6000),
            LossLedger::default(),
            LossLedger::default(),
            comun(),
        );

        assert_eq!(result.cross_offset_rcm_to_gyp, dec!(100));
        assert_eq!(result.gyp_taxable, dec!(5900));
        assert!(result.rcm_ledger_next.is_empty());
    }

    /// A balance in its fourth and final year that this year's income could not absorb is dropped,
    /// not carried, and reported so the loss of the offset is visible.
    #[test]
    fn balances_reaching_the_end_of_the_window_expire() {
        let result = compensate_savings_base(
            2026,
            dec!(0),
            dec!(1000),
            LossLedger::default(),
            ledger(&[(2022, "5000")], 2026),
            GIPUZKOA,
        );

        // 1,000 of the 5,000 was absorbed; the remaining 4,000 has run out of years.
        assert_eq!(result.gyp_applied.used_total, dec!(1000));
        assert_eq!(result.gyp_taxable, dec!(0));
        assert_eq!(result.gyp_expired, dec!(4000));
        assert!(result.gyp_ledger_next.is_empty());
    }

    /// A vintage with a year still left is carried, not expired — the boundary must be exact.
    #[test]
    fn the_penultimate_vintage_survives() {
        let result = compensate_savings_base(
            2026,
            dec!(0),
            dec!(0),
            LossLedger::default(),
            ledger(&[(2023, "5000")], 2026),
            GIPUZKOA,
        );

        assert_eq!(result.gyp_expired, dec!(0));
        assert_eq!(result.gyp_ledger_next.balances()[&2023], dec!(5000));
    }
}

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

    /// Current-year negative of one group set against the other's positive (Común only).
    pub cross_offset_rcm_to_gyp: Decimal,
    pub cross_offset_gyp_to_rcm: Decimal,

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
/// entre sí" and never touch, and 0.25 under Territorio Común (LIRPF art. 49.1).
// TODO(verify): the order of operations within a year. This applies prior-year balances to each
// group first, then sets a current-year negative against the other group's *remaining* positive,
// capping at 25% of that remainder. Art. 49.1 says the cap is "el 25 por ciento de dicho saldo
// positivo" without settling whether "dicho saldo" is measured before or after prior-year balances
// are absorbed. Measuring it after is the conservative reading — it yields a smaller cross-offset.
pub fn compensate_savings_base(
    filing_year: i32,
    rcm_net: Decimal,
    gyp_net: Decimal,
    mut rcm_ledger: LossLedger,
    mut gyp_ledger: LossLedger,
    cross_offset_fraction: Decimal,
) -> CompensationResult {
    let zero = Decimal::ZERO;

    // Step 1: each group's prior-year balances reduce its own positive result. Art. 49.2 requires
    // absorbing the maximum each year, which `LossLedger::apply` does, oldest vintage first.
    let rcm_positive = std::cmp::max(zero, rcm_net);
    let gyp_positive = std::cmp::max(zero, gyp_net);

    let rcm_applied = rcm_ledger.apply(rcm_positive, None);
    let gyp_applied = gyp_ledger.apply(gyp_positive, None);

    let mut rcm_taxable = rcm_positive - rcm_applied.used_total;
    let mut gyp_taxable = gyp_positive - gyp_applied.used_total;

    // Step 2: Territorio Común only. A group's own negative result may reduce the other group's
    // positive, but only up to a fraction of it. Gipuzkoa skips this entirely.
    let mut rcm_negative = std::cmp::max(zero, -rcm_net);
    let mut gyp_negative = std::cmp::max(zero, -gyp_net);

    let mut cross_offset_rcm_to_gyp = zero;
    let mut cross_offset_gyp_to_rcm = zero;

    if cross_offset_fraction > zero {
        // Only one direction can apply in a given year: a group cannot be both negative and
        // positive, so at most one of these two has anything to work with.
        cross_offset_rcm_to_gyp = std::cmp::min(rcm_negative, gyp_taxable * cross_offset_fraction);
        gyp_taxable -= cross_offset_rcm_to_gyp;
        rcm_negative -= cross_offset_rcm_to_gyp;

        cross_offset_gyp_to_rcm = std::cmp::min(gyp_negative, rcm_taxable * cross_offset_fraction);
        rcm_taxable -= cross_offset_gyp_to_rcm;
        gyp_negative -= cross_offset_gyp_to_rcm;
    }

    // Step 3: whatever negative remains is this year's pending balance, labelled with this year so
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
        rcm_ledger_next: rcm_ledger,
        gyp_ledger_next: gyp_ledger,
        rcm_expired,
        gyp_expired,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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

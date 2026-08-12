//! Origin-year-labeled loss ledgers for the savings base.
//!
//! A negative balance in either savings-base group may be offset against future positive balances
//! of the same group, but only in the four years following the one it arose in (NF 3/2014 /
//! LIRPF art. 49.1). Tracking a single running total is therefore not enough: the ledger has to
//! know *which year* each pending amount came from, or it cannot tell an amount that is about to
//! expire from one with three years left.

use std::collections::BTreeMap;

use crate::core::GenericResult;
use crate::types::Decimal;

/// Number of following years in which a negative savings-base balance may still be offset.
///
/// LIRPF art. 49.1: "su importe se compensará en los cuatro años siguientes". Art. 49.2 adds that
/// each year must absorb the maximum it can, and forbids stretching the window by rolling an old
/// balance into a later year's losses.
pub const CARRYFORWARD_YEARS: i32 = 4;

/// Pending negative balances of one savings-base group, labeled by the year each arose in.
///
/// Amounts are stored as **positive magnitudes** — the sign lives in the type, not the number, so
/// there is no way to accidentally add a pending loss to a gain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LossLedger {
    balances: BTreeMap<i32, Decimal>,
}

/// What one [`LossLedger::apply`] call consumed, broken down by the origin year of each amount.
///
/// The per-year detail is what the statement needs to print next year's config: an aggregate would
/// lose track of which vintages survived and when they expire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LedgerApplication {
    pub used_total: Decimal,
    pub used_by_year: BTreeMap<i32, Decimal>,
}

impl LedgerApplication {
    /// Fold a second application of the **same** ledger into this one.
    ///
    /// Territorio Común consumes a group's ledger twice in a year — once against its own group, then
    /// against the other's remainder — and the statement reports one figure per group, so the two
    /// passes have to add up rather than overwrite each other.
    pub fn merge(&mut self, other: LedgerApplication) {
        self.used_total += other.used_total;
        for (origin, amount) in other.used_by_year {
            *self.used_by_year.entry(origin).or_insert(Decimal::ZERO) += amount;
        }
    }
}

impl LossLedger {
    /// Build a ledger from the user's `taxes.spain.loss_carryforward.<group>` map.
    ///
    /// Rejects rather than silently dropping: an origin year outside the window is either a typo
    /// or a real balance the user believes is still usable, and quietly ignoring it would
    /// understate the offset without telling anyone. `group` names the config path in errors.
    pub fn from_config(
        balances: &BTreeMap<i32, Decimal>,
        filing_year: i32,
        group: &str,
    ) -> GenericResult<LossLedger> {
        let oldest_usable = filing_year - CARRYFORWARD_YEARS;
        let mut ledger = LossLedger::default();

        for (&origin, &amount) in balances {
            let path = format!("taxes.spain.loss_carryforward.{group}.{origin}");

            if amount < Decimal::ZERO {
                return Err!(
                    "{path} is negative ({amount}): record a pending negative balance as a \
                     positive magnitude"
                );
            }

            if origin >= filing_year {
                return Err!(
                    "{path} is not a prior year: a {origin} loss cannot be carried into the \
                     {filing_year} return. This year's own losses are computed from the statement"
                );
            }

            if origin < oldest_usable {
                return Err!(
                    "{path} has expired: a negative savings-base balance may be offset only in \
                     the {CARRYFORWARD_YEARS} following years, so a {origin} loss could last be \
                     used in the {} return. Remove it from the config",
                    origin + CARRYFORWARD_YEARS
                );
            }

            if amount > Decimal::ZERO {
                ledger.balances.insert(origin, amount);
            }
        }

        Ok(ledger)
    }

    /// Record a new pending loss arising in `origin_year`.
    pub fn add(&mut self, origin_year: i32, loss: Decimal) {
        if loss <= Decimal::ZERO {
            return;
        }
        *self.balances.entry(origin_year).or_insert(Decimal::ZERO) += loss;
    }

    /// Consume up to `amount` of pending losses, oldest vintage first.
    ///
    /// Oldest-first is not a preference but a requirement: art. 49.2 obliges each year to absorb
    /// the maximum it can, and consuming a younger balance while an older one expires would throw
    /// away an offset the taxpayer was entitled to.
    ///
    /// `cap` carries the Común 25% cross-group limit; `None` means the only limit is `amount`.
    pub fn apply(&mut self, amount: Decimal, cap: Option<Decimal>) -> LedgerApplication {
        let mut budget = std::cmp::max(Decimal::ZERO, amount);
        if let Some(cap) = cap {
            budget = std::cmp::min(budget, std::cmp::max(Decimal::ZERO, cap));
        }

        let mut application = LedgerApplication::default();
        if budget.is_zero() {
            return application;
        }

        // `BTreeMap` iterates its keys in ascending order, so this walks the vintages oldest first.
        for origin in self.balances.keys().copied().collect::<Vec<i32>>() {
            if budget.is_zero() {
                break;
            }

            let balance = self.balances[&origin];
            let used = std::cmp::min(balance, budget);

            budget -= used;
            application.used_total += used;
            application.used_by_year.insert(origin, used);

            if used == balance {
                self.balances.remove(&origin);
            } else {
                self.balances.insert(origin, balance - used);
            }
        }

        application
    }

    /// Total pending magnitude across every vintage.
    pub fn total(&self) -> Decimal {
        self.balances.values().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.balances.is_empty()
    }

    /// The pending balances by origin year, for reporting next year's config.
    pub fn balances(&self) -> &BTreeMap<i32, Decimal> {
        &self.balances
    }

    /// Magnitude that can no longer be offset after `filing_year`, i.e. the vintage whose fourth
    /// and final year this was. Reported so the user learns the offset is being lost.
    pub fn expiring_after(&self, filing_year: i32) -> Decimal {
        self.balances
            .get(&(filing_year - CARRYFORWARD_YEARS))
            .copied()
            .unwrap_or(Decimal::ZERO)
    }

    /// Drop every vintage that can no longer be offset after `filing_year`, so the ledger handed to
    /// the following return contains only balances that return will actually accept.
    pub fn drop_expired(&mut self, filing_year: i32) {
        self.balances
            .retain(|&origin, _| origin > filing_year - CARRYFORWARD_YEARS);
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn config(entries: &[(i32, &str)]) -> BTreeMap<i32, Decimal> {
        entries
            .iter()
            .map(|&(year, amount)| (year, amount.parse().unwrap()))
            .collect()
    }

    /// Filing 2026, the window is origins 2022-2025: a 2022 loss is in its fourth and final year.
    #[rstest]
    #[case(2022)]
    #[case(2023)]
    #[case(2024)]
    #[case(2025)]
    fn origins_inside_the_window_are_accepted(#[case] origin: i32) {
        let ledger = LossLedger::from_config(&config(&[(origin, "100")]), 2026, "gyp").unwrap();
        assert_eq!(ledger.total(), dec!(100));
    }

    /// The boundary is the whole point of labeling vintages: 2022 is usable in the 2026 return,
    /// 2021 is one year too old and must be rejected rather than silently ignored.
    #[test]
    fn expired_origin_is_rejected_with_the_year_it_lapsed() {
        let error = LossLedger::from_config(&config(&[(2021, "1200.50")]), 2026, "gyp")
            .unwrap_err()
            .to_string();

        assert!(error.contains("expired"), "{error}");
        assert!(error.contains("2021"), "{error}");
        // Names the last return the loss could have been used in, so the user can check the claim.
        assert!(error.contains("2025"), "{error}");
        assert!(error.contains("taxes.spain.loss_carryforward.gyp.2021"), "{error}");
    }

    /// The filing year's own losses come from the statement, so configuring them would double-count.
    #[rstest]
    #[case(2026)]
    #[case(2027)]
    fn origin_at_or_after_the_filing_year_is_rejected(#[case] origin: i32) {
        let error = LossLedger::from_config(&config(&[(origin, "100")]), 2026, "rcm")
            .unwrap_err()
            .to_string();

        assert!(error.contains("not a prior year"), "{error}");
        assert!(error.contains("taxes.spain.loss_carryforward.rcm"), "{error}");
    }

    /// Pending balances are magnitudes. A negative entry means the user misread the convention, and
    /// taking it at face value would *increase* the taxable base instead of reducing it.
    #[test]
    fn negative_balance_is_rejected() {
        let error = LossLedger::from_config(&config(&[(2024, "-100")]), 2026, "gyp")
            .unwrap_err()
            .to_string();
        assert!(error.contains("positive magnitude"), "{error}");
    }

    /// Losses are consumed oldest first (art. 49.2), so a vintage about to expire is used before a
    /// younger one that still has years left.
    #[test]
    fn apply_consumes_oldest_vintage_first() {
        let mut ledger =
            LossLedger::from_config(&config(&[(2022, "300"), (2024, "500")]), 2026, "gyp").unwrap();

        let applied = ledger.apply(dec!(400), None);

        assert_eq!(applied.used_total, dec!(400));
        assert_eq!(applied.used_by_year, config(&[(2022, "300"), (2024, "100")]));
        // The exhausted vintage is gone; the younger one keeps its remainder.
        assert_eq!(ledger.balances(), &config(&[(2024, "400")]));
        assert_eq!(ledger.total(), dec!(400));
    }

    /// Applying more than the ledger holds consumes it entirely and reports only what existed —
    /// never inventing an offset to fill the request.
    #[test]
    fn apply_is_limited_by_the_pending_total() {
        let mut ledger = LossLedger::from_config(&config(&[(2025, "150")]), 2026, "gyp").unwrap();

        let applied = ledger.apply(dec!(1000), None);

        assert_eq!(applied.used_total, dec!(150));
        assert!(ledger.is_empty());
        assert_eq!(ledger.total(), dec!(0));
    }

    /// The cap carries the Común 25% cross-group limit, so it binds even when both the requested
    /// amount and the pending total are larger.
    #[test]
    fn apply_respects_the_cap() {
        let mut ledger = LossLedger::from_config(&config(&[(2024, "2000")]), 2026, "rcm").unwrap();

        let applied = ledger.apply(dec!(6000), Some(dec!(1500)));

        assert_eq!(applied.used_total, dec!(1500));
        assert_eq!(ledger.total(), dec!(500));
    }

    /// A non-positive target consumes nothing: there is no positive balance to offset against.
    #[rstest]
    #[case("0")]
    #[case("-250")]
    fn apply_to_a_non_positive_amount_consumes_nothing(#[case] amount: &str) {
        let mut ledger = LossLedger::from_config(&config(&[(2024, "500")]), 2026, "gyp").unwrap();

        let applied = ledger.apply(amount.parse().unwrap(), None);

        assert_eq!(applied, LedgerApplication::default());
        assert_eq!(ledger.total(), dec!(500));
    }

    /// `add` accumulates within a vintage rather than replacing it, so two losses arising in the
    /// same year both survive into the ledger.
    #[test]
    fn add_accumulates_within_a_vintage() {
        let mut ledger = LossLedger::default();
        ledger.add(2026, dec!(100));
        ledger.add(2026, dec!(50));
        // A non-positive "loss" is not a loss and must not create a phantom vintage.
        ledger.add(2026, dec!(0));
        ledger.add(2025, dec!(-10));

        assert_eq!(ledger.balances(), &config(&[(2026, "150")]));
    }

    /// Filing 2026, a 2022 vintage is in its final year: whatever is left of it after this year's
    /// offsetting can never be used again, and the user needs to be told.
    #[test]
    fn expiring_after_reports_the_final_year_vintage() {
        let ledger = LossLedger::from_config(
            &config(&[(2022, "300"), (2023, "400")]),
            2026,
            "gyp",
        )
        .unwrap();

        assert_eq!(ledger.expiring_after(2026), dec!(300));
        // In the following return the same balance is already out of the window.
        assert_eq!(ledger.expiring_after(2027), dec!(400));
    }

    /// A zero entry is accepted but carries no balance, so it does not show up as a phantom vintage
    /// in next year's config output.
    #[test]
    fn zero_entries_are_dropped() {
        let ledger = LossLedger::from_config(&config(&[(2024, "0")]), 2026, "gyp").unwrap();
        assert!(ledger.is_empty());
    }
}

//! Property-based tests for the conservation laws the reviewers proved by hand.
//!
//! Every euro the wash-sale engine defers must later be released or carried; every blocked share
//! must answer to a matched share; a compensated savings base must equal the sum of its two
//! non-negative group results. These are invariants over arbitrary inputs, not single cases.
//!
//! The unit tests next door pin *worked numbers* — Appendix A's vectors, the AEAT manual's own
//! example, the statutes' cuota columns. This module pins the *laws those numbers obey*, so a
//! refactor that keeps every fixture green while inventing or destroying a euro somewhere off the
//! fixture path still fails. Each test states its law in the doc comment; a failure names the law,
//! and proptest shrinks the counterexample to the smallest input that breaks it.
//!
//! Magnitudes are bounded on purpose. A 10^20 input tests `Decimal`'s overflow behaviour, not
//! Spanish tax law, and it hides real counterexamples behind noise. Every generated euro figure is
//! a whole number of cents inside a range a broker statement can actually produce, and every rate
//! is the four-decimal figure art. 67.2 / 59.2 express a tipo medio in.

use std::collections::BTreeMap;

use chrono::Duration;
use proptest::prelude::*;

use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::spain::carryforward::{CARRYFORWARD_YEARS, LossLedger};
use crate::taxes::spain::coefficients::{gipuzkoa_coefficient, validate_overrides};
use crate::taxes::spain::compensation::{CrossOffset, compensate_savings_base};
use crate::taxes::spain::credit::{double_taxation_credit, treaty_capped_credit};
use crate::taxes::spain::exemption::{GLOBAL_TRANSMISSION_LIMIT, small_disposals_exemption};
use crate::taxes::spain::scale::SavingsScale;
use crate::types::{Date, Decimal};

use super::wash_sale::{Acquisition, Disposal, WashSaleEngine};

/// Tolerance for a conservation law, in euros.
///
/// The pro-rata arithmetic divides a deferred amount by a share count, so a law stated as an exact
/// equality would be testing `Decimal`'s 28-digit rounding rather than the law. Residues from that
/// rounding sit around 10^-25 on these magnitudes; a micro-cent is nine orders of magnitude above
/// them and eight below the cent any figure is ever reported in, so it separates rounding dust from
/// a euro that actually went missing.
const EPSILON: Decimal = dec!(0.000001);

/// The three regimes, so a law is asserted for all of them or for none.
const REGIMES: [SpanishTaxRegime; 3] = [
    SpanishTaxRegime::Gipuzkoa,
    SpanishTaxRegime::Comun,
    SpanishTaxRegime::Navarra,
];

/// The compensation ordering each regime runs, in the same order as [`REGIMES`].
const CROSS_OFFSETS: [CrossOffset; 3] = [
    CrossOffset::None,
    CrossOffset::AeatTwoPhase,
    CrossOffset::NavarraOrdered,
];

const KEY: &str = "US0378331005";

// ---------------------------------------------------------------------------------------------
// 1. The wash-sale engine (`wash_sale.rs`)
// ---------------------------------------------------------------------------------------------

/// One generated trade: days after the start date, buy or sell, share count, fiscal result in cents.
type RawTrade = (u32, bool, u32, i64);

/// Sequences short enough that the blocked-lot bookkeeping stays inspectable, long enough that a
/// deferral is created, released, re-attached and released again inside one case.
fn trades() -> impl Strategy<Value = Vec<RawTrade>> {
    prop::collection::vec(
        (0u32..400, any::<bool>(), 1u32..30, -100_000i64..=100_000i64),
        1..7,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// **No euro is invented and none is lost, per originating sale.**
    ///
    /// Over any sequence of buys and sells, for every sale that deferred a loss:
    /// `deferred there == released from there + still blocked against there`.
    ///
    /// Stating it per origin rather than in aggregate is what makes it test
    /// [`BlockedLot::origin_sale_date`](super::wash_sale::BlockedLot::origin_sale_date) surviving a
    /// chain. A deferral that is released and re-attached — the V3282-18 case, where selling the
    /// blocking shares and buying homogeneous ones back inside the window moves the deferral onto
    /// the new shares instead of ending it — must arrive at the new lot still labelled with the sale
    /// it came from. An aggregate total would balance even if every origin were rewritten to the
    /// releasing sale's date; per origin, that mislabelling breaks the equality on both dates.
    ///
    /// Asserted after **every** disposal, not once at the end, so the shrunk counterexample points
    /// at the disposal that broke the law rather than at the whole sequence.
    #[test]
    fn every_deferred_euro_is_released_or_still_blocked(raw in trades()) {
        let mut trades = raw;
        // `sort_by_key` is stable, so trades on the same day keep their generated order.
        trades.sort_by_key(|&(day, ..)| day);

        let start = date!(2025, 1, 1);

        // Acquisitions are registered before the first disposal is processed: a repurchase *after*
        // a loss-making sale still blocks it, so the window search has to see the future.
        let mut acquired: BTreeMap<Date, Decimal> = BTreeMap::new();
        for &(day, is_buy, quantity, _) in &trades {
            if is_buy {
                *acquired
                    .entry(start + Duration::days(day.into()))
                    .or_insert(Decimal::ZERO) += Decimal::from(quantity);
            }
        }

        let mut engine = WashSaleEngine::new(
            acquired.iter().map(|(&date, &quantity)| Acquisition {
                key: KEY.to_owned(),
                date,
                quantity,
            }),
            [],
        );

        // Shares still held per acquisition date, and the FIFO consumption the processor would hand
        // the engine. Same bucket-per-date model the engine uses, so the two stay comparable.
        let mut held: BTreeMap<Date, Decimal> = BTreeMap::new();
        let mut consumed: BTreeMap<Date, Decimal> = BTreeMap::new();

        // Euros deferred by, and released back to, each originating sale.
        let mut deferred: BTreeMap<Date, Decimal> = BTreeMap::new();
        let mut released: BTreeMap<Date, Decimal> = BTreeMap::new();

        for &(day, is_buy, quantity, result_cents) in &trades {
            let date = start + Duration::days(day.into());
            let quantity = Decimal::from(quantity);

            if is_buy {
                *held.entry(date).or_insert(Decimal::ZERO) += quantity;
                continue;
            }

            let mut remaining = quantity;
            let mut lots: Vec<(Date, Decimal)> = Vec::new();
            for (&lot_date, available) in held.range_mut(..=date) {
                if remaining <= Decimal::ZERO {
                    break;
                }
                let taken = std::cmp::min(*available, remaining);
                if taken <= Decimal::ZERO {
                    continue;
                }
                *available -= taken;
                remaining -= taken;
                *consumed.entry(lot_date).or_insert(Decimal::ZERO) += taken;
                lots.push((lot_date, taken));
            }

            // A sell of shares nobody held is not a disposal; the generator is free to produce one.
            let sold = quantity - remaining;
            if sold <= Decimal::ZERO {
                continue;
            }

            let outcome = engine.process(&Disposal {
                key: KEY.to_owned(),
                date,
                quantity: sold,
                fiscal_result: Some(Decimal::new(result_cents, 2)),
                consumed: lots,
            });

            prop_assert!(outcome.deferred_loss >= Decimal::ZERO);
            prop_assert!(outcome.wider_window_loss >= Decimal::ZERO);
            if outcome.deferred_loss > Decimal::ZERO {
                *deferred.entry(date).or_insert(Decimal::ZERO) += outcome.deferred_loss;
            }

            for reintegration in &outcome.reintegrations {
                prop_assert!(reintegration.amount > Decimal::ZERO);
                // A released euro answers to a sale that actually deferred one.
                prop_assert!(deferred.contains_key(&reintegration.origin_sale_date));
                *released
                    .entry(reintegration.origin_sale_date)
                    .or_insert(Decimal::ZERO) += reintegration.amount;
            }

            let mut blocked_loss: BTreeMap<Date, Decimal> = BTreeMap::new();
            let mut blocked_shares: BTreeMap<Date, Decimal> = BTreeMap::new();
            for (_, lot) in engine.blocked_lots() {
                // A lot that kept shares but over-drew its euros would go negative here, which is
                // the observable form of "a release never exceeds what that lot was deferring".
                prop_assert!(lot.deferred_loss >= Decimal::ZERO);
                // A lot with no shares left blocks nothing and is dropped, so whatever it still
                // carried would vanish with it.
                prop_assert!(lot.blocked_quantity > Decimal::ZERO);
                prop_assert!(acquired.contains_key(&lot.buy_date));
                prop_assert!(deferred.contains_key(&lot.origin_sale_date));
                prop_assert!(lot.origin_sale_date <= date);

                *blocked_loss
                    .entry(lot.origin_sale_date)
                    .or_insert(Decimal::ZERO) += lot.deferred_loss;
                *blocked_shares
                    .entry(lot.buy_date)
                    .or_insert(Decimal::ZERO) += lot.blocked_quantity;
            }

            for (origin, &total) in &deferred {
                let out = released.get(origin).copied().unwrap_or(Decimal::ZERO);
                let still = blocked_loss.get(origin).copied().unwrap_or(Decimal::ZERO);
                prop_assert!(out <= total + EPSILON, "released {out} of {total} deferred on {origin}");
                prop_assert!(
                    (total - out - still).abs() <= EPSILON,
                    "{origin}: deferred {total}, released {out}, blocked {still}",
                );
            }

            // One blocked share per matched share: an acquisition date can never block more shares
            // than it acquired and has not since disposed of.
            for (buy_date, &shares) in &blocked_shares {
                let available = acquired[buy_date]
                    - consumed.get(buy_date).copied().unwrap_or(Decimal::ZERO);
                prop_assert!(
                    shares <= available + EPSILON,
                    "{buy_date} blocks {shares} of {available} available",
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The loss ledger (`carryforward.rs`)
// ---------------------------------------------------------------------------------------------

/// Build the ledger the config path would build, with one vintage per year of the four-year window.
fn window_ledger(filing_year: i32, balances: &[i64]) -> (BTreeMap<i32, Decimal>, LossLedger) {
    let config: BTreeMap<i32, Decimal> = balances
        .iter()
        .enumerate()
        .map(|(index, &cents)| {
            (
                filing_year - CARRYFORWARD_YEARS + index as i32,
                Decimal::new(cents, 2),
            )
        })
        .collect();

    // Origins run from `filing_year - 4` to `filing_year - 1`, exactly the window `from_config`
    // accepts, so every generated state is one a real config can reach.
    let ledger = LossLedger::from_config(&config, filing_year, "gyp").unwrap();
    (config, ledger)
}

proptest! {
    /// **`LossLedger::apply` conserves, never invents, and never skips a vintage.**
    ///
    /// `used + remaining == the total that was pending`; `used` is never negative, never exceeds the
    /// amount asked for, and never exceeds the cap; and the per-vintage breakdown adds up to it.
    ///
    /// Oldest-first is asserted as the law art. 49.2 makes it: once a vintage is left partly
    /// unconsumed, no younger vintage may be touched. Consuming a younger balance while an older one
    /// expires throws away an offset the taxpayer was entitled to, and it does so without changing
    /// any total — which is exactly why an aggregate assertion cannot see it.
    #[test]
    fn applying_a_loss_ledger_conserves_it_and_consumes_the_oldest_vintage_first(
        filing_year in 2024i32..=2030,
        balances in prop::collection::vec(0i64..=500_000i64, CARRYFORWARD_YEARS as usize),
        amount in -50_000i64..=1_500_000i64,
        cap in prop::option::of(-50_000i64..=1_500_000i64),
    ) {
        let (config, mut ledger) = window_ledger(filing_year, &balances);
        let pending = ledger.total();
        let amount = Decimal::new(amount, 2);
        let cap = cap.map(|cents| Decimal::new(cents, 2));

        let applied = ledger.apply(amount, cap);

        prop_assert!(applied.used_total >= Decimal::ZERO);
        prop_assert_eq!(applied.used_total + ledger.total(), pending);
        prop_assert!(applied.used_total <= std::cmp::max(Decimal::ZERO, amount));
        if let Some(cap) = cap {
            prop_assert!(applied.used_total <= std::cmp::max(Decimal::ZERO, cap));
        }
        prop_assert_eq!(applied.used_by_year.values().sum::<Decimal>(), applied.used_total);

        let mut starved = false;
        for (origin, &balance) in &config {
            let used = applied.used_by_year.get(origin).copied().unwrap_or(Decimal::ZERO);
            prop_assert!(used <= balance);

            // Whatever the vintage did not give up is still pending under its own year.
            let left = ledger.balances().get(origin).copied().unwrap_or(Decimal::ZERO);
            prop_assert_eq!(used + left, balance);

            if starved {
                prop_assert_eq!(used, Decimal::ZERO, "{} was consumed past an older vintage", origin);
            }
            if used < balance {
                starved = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Compensation (`compensation.rs`), all three regimes
// ---------------------------------------------------------------------------------------------

/// The figure each regime measures its 25% cross-offset allowance on, for one group.
///
/// Común takes a quarter of the group's **original** current-year positive (AEAT Manual cap. 12);
/// Navarra takes a quarter of what that group arrived at **after** absorbing its own carryforwards
/// (TRLFIRPF art. 54.2, "el saldo positivo resultante de la letra b)"); Gipuzkoa allows none at all.
fn cross_allowance(mode: CrossOffset, net: Decimal, ledger: &LossLedger) -> Decimal {
    let positive = std::cmp::max(Decimal::ZERO, net);

    match mode {
        CrossOffset::None => Decimal::ZERO,
        CrossOffset::AeatTwoPhase => positive * dec!(0.25),
        CrossOffset::NavarraOrdered => {
            let mut own = ledger.clone();
            let absorbed = own.apply(positive, None).used_total;
            (positive - absorbed) * dec!(0.25)
        },
    }
}

proptest! {
    /// **The savings base is the sum of two non-negative group results, and nothing crosses beyond
    /// the quarter its statute allows.**
    ///
    /// Asserted for all three orderings on the same inputs, because the regimes are meant to differ
    /// in their *answer* and agree on their *laws*: a base that is not `rcm + gyp`, a negative
    /// component, a Gipuzkoa cross, or a cross above 25% is a bug in whichever arm produced it.
    ///
    /// Both cross directions firing in the same year is impossible for the current-year cross — a
    /// group is either positive or negative, never both — and the reviewers proved it by hand for
    /// the Navarra arm. Here it is a law over arbitrary inputs.
    #[test]
    fn compensation_splits_the_savings_base_without_crossing_past_the_statutory_quarter(
        filing_year in 2024i32..=2030,
        rcm_net in -500_000i64..=500_000i64,
        gyp_net in -500_000i64..=500_000i64,
        rcm_balances in prop::collection::vec(0i64..=300_000i64, CARRYFORWARD_YEARS as usize),
        gyp_balances in prop::collection::vec(0i64..=300_000i64, CARRYFORWARD_YEARS as usize),
    ) {
        let rcm_net = Decimal::new(rcm_net, 2);
        let gyp_net = Decimal::new(gyp_net, 2);
        let (_, rcm_ledger) = window_ledger(filing_year, &rcm_balances);
        let (_, gyp_ledger) = window_ledger(filing_year, &gyp_balances);

        for mode in CROSS_OFFSETS {
            let result = compensate_savings_base(
                filing_year,
                rcm_net,
                gyp_net,
                rcm_ledger.clone(),
                gyp_ledger.clone(),
                mode,
            );

            prop_assert!(result.rcm_taxable >= Decimal::ZERO, "{mode:?}");
            prop_assert!(result.gyp_taxable >= Decimal::ZERO, "{mode:?}");
            prop_assert_eq!(
                result.savings_base,
                result.rcm_taxable + result.gyp_taxable,
                "{:?}", mode,
            );

            let crosses = [
                result.cross_offset_rcm_to_gyp,
                result.cross_offset_gyp_to_rcm,
                result.prior_cross_offset_rcm_to_gyp,
                result.prior_cross_offset_gyp_to_rcm,
            ];
            for cross in crosses {
                prop_assert!(cross >= Decimal::ZERO, "{mode:?}");
            }

            if mode == CrossOffset::None {
                // The groups integrate "exclusivamente entre sí" and never touch.
                for cross in crosses {
                    prop_assert_eq!(cross, Decimal::ZERO, "{:?} crossed", mode);
                }
            }

            // One allowance per receiving group, shared by the current-year and the carried cross.
            let into_gyp = result.cross_offset_rcm_to_gyp + result.prior_cross_offset_rcm_to_gyp;
            let into_rcm = result.cross_offset_gyp_to_rcm + result.prior_cross_offset_gyp_to_rcm;
            prop_assert!(
                into_gyp <= cross_allowance(mode, gyp_net, &gyp_ledger) + EPSILON,
                "{mode:?}: {into_gyp} crossed into ganancias",
            );
            prop_assert!(
                into_rcm <= cross_allowance(mode, rcm_net, &rcm_ledger) + EPSILON,
                "{mode:?}: {into_rcm} crossed into RCM",
            );

            // At most one direction can fire in a year: a group cannot be positive and negative.
            prop_assert!(
                result.cross_offset_rcm_to_gyp == Decimal::ZERO
                    || result.cross_offset_gyp_to_rcm == Decimal::ZERO,
                "{mode:?} crossed both ways",
            );
        }
    }
}

proptest! {
    /// **A ledger handed to the following return is the prior one, less what this year used, plus
    /// what this year could not absorb, less what expired.**
    ///
    /// `next + expired == prior − used + (this year's negative − what crossed out of it)`.
    ///
    /// This is the multi-year continuity law in its arithmetic form: the config the tool prints for
    /// next year is the only channel between two filings, so a euro dropped here is a euro the
    /// filer can never offset again, and one duplicated here is an offset claimed twice. Held for
    /// all three orderings.
    #[test]
    fn the_next_year_ledgers_account_for_every_pending_euro(
        filing_year in 2024i32..=2030,
        rcm_net in -500_000i64..=500_000i64,
        gyp_net in -500_000i64..=500_000i64,
        rcm_balances in prop::collection::vec(0i64..=300_000i64, CARRYFORWARD_YEARS as usize),
        gyp_balances in prop::collection::vec(0i64..=300_000i64, CARRYFORWARD_YEARS as usize),
    ) {
        let rcm_net = Decimal::new(rcm_net, 2);
        let gyp_net = Decimal::new(gyp_net, 2);
        let (_, rcm_ledger) = window_ledger(filing_year, &rcm_balances);
        let (_, gyp_ledger) = window_ledger(filing_year, &gyp_balances);
        let rcm_pending = rcm_ledger.total();
        let gyp_pending = gyp_ledger.total();

        for mode in CROSS_OFFSETS {
            let result = compensate_savings_base(
                filing_year,
                rcm_net,
                gyp_net,
                rcm_ledger.clone(),
                gyp_ledger.clone(),
                mode,
            );

            let groups = [
                (
                    "RCM",
                    rcm_pending,
                    rcm_net,
                    result.rcm_applied.used_total,
                    result.cross_offset_rcm_to_gyp,
                    &result.rcm_ledger_next,
                    result.rcm_expired,
                ),
                (
                    "ganancias",
                    gyp_pending,
                    gyp_net,
                    result.gyp_applied.used_total,
                    result.cross_offset_gyp_to_rcm,
                    &result.gyp_ledger_next,
                    result.gyp_expired,
                ),
            ];

            for (group, pending, net, used, crossed_out, ledger, expired) in groups {
                let next = ledger.total();
                prop_assert!(used >= Decimal::ZERO, "{mode:?} {group}");
                prop_assert!(used <= pending, "{mode:?} {group}: used {used} of {pending}");
                prop_assert!(expired >= Decimal::ZERO, "{mode:?} {group}");
                prop_assert!(next >= Decimal::ZERO, "{mode:?} {group}");

                let carried = std::cmp::max(Decimal::ZERO, -net) - crossed_out;
                prop_assert!(carried >= Decimal::ZERO, "{mode:?} {group}: crossed past its negative");
                prop_assert_eq!(next + expired, pending - used + carried, "{:?} {}", mode, group);

                // Nothing older than the window, and nothing newer than this year, survives into
                // next year's config: `from_config` would reject either on the following return.
                for (&origin, &balance) in ledger.balances() {
                    prop_assert!(origin > filing_year - CARRYFORWARD_YEARS, "{mode:?} {group}");
                    prop_assert!(origin <= filing_year, "{mode:?} {group}");
                    prop_assert!(balance > Decimal::ZERO, "{mode:?} {group}");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 4. The savings scale (`scale.rs`), every regime and every shipped year
// ---------------------------------------------------------------------------------------------

proptest! {
    /// **The cuota is zero at zero, never falls as the base rises, and never exceeds the top
    /// marginal rate applied to the whole base.**
    ///
    /// The average rate — the figure the double-taxation credit is capped at — stays inside
    /// `[0, top marginal]` with it. A rate above the top marginal, or a cuota that falls as the base
    /// rises, would let a bracket table typo pass every golden vector that happens to sit elsewhere.
    ///
    /// Asserted across all three regimes and all shipped years in one case, so a new year added to
    /// `for_year` is covered the day it lands rather than the day someone remembers to widen a test.
    #[test]
    fn the_savings_scale_is_monotone_and_bounded_by_its_top_rate(
        lower in 0i64..=40_000_000i64,
        step in 0i64..=40_000_000i64,
    ) {
        let lower = Decimal::new(lower, 2);
        let higher = lower + Decimal::new(step, 2);

        for regime in REGIMES {
            for year in SavingsScale::FIRST_YEAR..=SavingsScale::LAST_YEAR {
                let scale = SavingsScale::for_year(regime, year).unwrap();
                let brackets = scale.brackets();
                let top = brackets.last().unwrap().1;

                prop_assert_eq!(scale.tax(Decimal::ZERO), Decimal::ZERO, "{:?} {}", regime, year);
                prop_assert_eq!(scale.average_rate(Decimal::ZERO), Decimal::ZERO, "{:?} {}", regime, year);
                // A non-positive base is untaxed rather than credited.
                prop_assert_eq!(scale.tax(-higher - dec!(1)), Decimal::ZERO, "{:?} {}", regime, year);
                prop_assert_eq!(scale.average_rate(-higher - dec!(1)), Decimal::ZERO, "{:?} {}", regime, year);

                let low = scale.tax(lower);
                let high = scale.tax(higher);
                prop_assert!(low <= high, "{regime:?} {year}: tax fell from {lower} to {higher}");
                prop_assert!(high >= Decimal::ZERO, "{regime:?} {year}");
                prop_assert!(high <= higher * top, "{regime:?} {year}: {high} on {higher}");

                let rate = scale.average_rate(higher);
                prop_assert!(rate >= Decimal::ZERO, "{regime:?} {year}");
                prop_assert!(rate <= top, "{regime:?} {year}: average rate {rate} above {top}");

                // The scale is progressive: floors and marginal rates both ascend. Anything else
                // would make "the top marginal rate" the wrong bound above.
                for pair in brackets.windows(2) {
                    prop_assert!(pair[0].0 < pair[1].0, "{regime:?} {year}");
                    prop_assert!(pair[0].1 <= pair[1].1, "{regime:?} {year}");
                }
            }
        }
    }
}

proptest! {
    /// **No bracket edge carries a jump.**
    ///
    /// Crossing a floor costs exactly the marginal rate of the bracket being entered, times the
    /// distance travelled: `tax(floor + d) − tax(floor) == d × rate`, and symmetrically below the
    /// floor at the previous bracket's rate. A published scale that tabulates a cumulative cuota
    /// column — Gipuzkoa 2026 and Navarra both do — is exactly where an off-by-one in the bracket
    /// arithmetic produces a step at the edge, and a golden vector *at* the edge cannot see it.
    #[test]
    fn crossing_a_bracket_edge_costs_exactly_its_marginal_rate(step in 1i64..=100_000i64) {
        let step = Decimal::new(step, 2);

        for regime in REGIMES {
            for year in SavingsScale::FIRST_YEAR..=SavingsScale::LAST_YEAR {
                let scale = SavingsScale::for_year(regime, year).unwrap();
                let brackets = scale.brackets().to_vec();

                for (index, &(floor, rate)) in brackets.iter().enumerate() {
                    // Stay inside the bracket being entered, so the step has one rate to it.
                    let above = match brackets.get(index + 1) {
                        Some(&(next_floor, _)) => std::cmp::min(step, next_floor - floor),
                        None => step,
                    };
                    prop_assert_eq!(
                        scale.tax(floor + above) - scale.tax(floor),
                        above * rate,
                        "{:?} {} at {}", regime, year, floor,
                    );

                    // …and the same from below, at the rate of the bracket being left.
                    if index > 0 {
                        let (previous_floor, previous_rate) = brackets[index - 1];
                        let below = std::cmp::min(step, floor - previous_floor);
                        prop_assert_eq!(
                            scale.tax(floor) - scale.tax(floor - below),
                            below * previous_rate,
                            "{:?} {} below {}", regime, year, floor,
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 5. The small-disposals exemption (`exemption.rs`)
// ---------------------------------------------------------------------------------------------

proptest! {
    /// **The exemption never exceeds the gain it relieves, never exceeds half the global amount, is
    /// zero above €3,000, and only ever grows with the gain.**
    ///
    /// Art. 39.5.d is the one rule in this round whose failure direction is *less* tax, so its
    /// ceilings are the guard rails: an exemption above `I` would turn relief into a deduction of
    /// its own, and one above `50% × G` would exempt an excess the article says "únicamente se
    /// someterá a gravamen". Monotonicity in `I` rules out a bigger gain being relieved less, which
    /// is the shape a mis-signed clamp takes.
    #[test]
    fn the_exemption_stays_inside_both_of_its_conditions(
        proceeds in -100_000i64..=1_000_000i64,
        gains in -100_000i64..=1_000_000i64,
        extra in 0i64..=500_000i64,
    ) {
        let global = Decimal::new(proceeds, 2);
        let gains = Decimal::new(gains, 2);
        let exempt = small_disposals_exemption(global, gains);

        prop_assert!(exempt >= Decimal::ZERO);
        prop_assert!(exempt <= std::cmp::max(
            Decimal::ZERO,
            std::cmp::min(gains, global * dec!(0.5)),
        ));

        // 1.º is a ceiling on the transmissions, not on their results.
        if global > GLOBAL_TRANSMISSION_LIMIT {
            prop_assert_eq!(exempt, Decimal::ZERO);
        }

        let bigger = small_disposals_exemption(global, gains + Decimal::new(extra, 2));
        prop_assert!(bigger >= exempt, "a larger incremento was relieved less");
    }
}

// ---------------------------------------------------------------------------------------------
// 6. The double-taxation credit (`credit.rs`)
// ---------------------------------------------------------------------------------------------

proptest! {
    /// **A credit is never negative and never exceeds any of its three ceilings.**
    ///
    /// What was withheld, what the treaty lets the source state levy, and the Spanish tax on the
    /// income itself. A credit above any of them subtracts tax that was never paid — the one
    /// direction in this whole module that costs the filer a penalty rather than money — and a
    /// negative credit would *add* tax silently.
    ///
    /// The composition is what is tested, not each limb alone: the treaty cap is a per-payment
    /// figure feeding a year-level rate cap, and the pipeline has to stay bounded by all three
    /// however the two are combined, including on the sign-flipped inputs a broken upstream sum
    /// could produce.
    #[test]
    fn the_credit_never_exceeds_what_was_paid_nor_the_spanish_tax_on_the_income(
        withheld in -50_000i64..=500_000i64,
        gross in -50_000i64..=2_000_000i64,
        treaty in 0i64..=10_000i64,
        taxable in -50_000i64..=2_000_000i64,
        average in 0i64..=5_000i64,
    ) {
        let withheld = Decimal::new(withheld, 2);
        let gross = Decimal::new(gross, 2);
        let treaty = Decimal::new(treaty, 4);
        let taxable = Decimal::new(taxable, 2);
        let average = Decimal::new(average, 4);

        let capped = treaty_capped_credit(withheld, gross, treaty);
        prop_assert!(capped >= Decimal::ZERO);
        prop_assert!(capped <= std::cmp::max(
            Decimal::ZERO,
            std::cmp::min(withheld, gross * treaty),
        ));

        let credit = double_taxation_credit(capped, taxable, average);
        prop_assert!(credit >= Decimal::ZERO);
        prop_assert!(credit <= capped);
        prop_assert!(credit <= std::cmp::max(Decimal::ZERO, withheld));
        prop_assert!(credit <= std::cmp::max(Decimal::ZERO, gross * treaty));
        prop_assert!(credit <= std::cmp::max(Decimal::ZERO, taxable * average));
    }
}

// ---------------------------------------------------------------------------------------------
// 7. Actualization coefficients (`coefficients.rs`)
// ---------------------------------------------------------------------------------------------

proptest! {
    /// **Every coefficient the shipped tables can produce is positive.**
    ///
    /// A coefficient multiplies the acquisition cost, so a zero or negative one does not shrink a
    /// gain — it invents one with the wrong sign, and the result reads as a plausible number on a
    /// tax return. Every acquisition year from three decades before the disposal up to the disposal
    /// year itself has to land on a real row, including the years below the table's "1994 y
    /// anteriores" floor, which clamp onto it.
    ///
    /// Deliberately **not** asserted: monotonicity across acquisition years. The table steps back up
    /// at the 31-12-1994 seam by design, so a monotone assertion would be asserting a bug.
    #[test]
    fn every_shipped_actualization_coefficient_is_positive(
        disposal_year in 2024i32..=2026,
        age in 0i32..=40,
        month in 1u32..=12,
        day in 1u32..=28,
    ) {
        let acquisition_year = disposal_year - age;
        let acquired = Date::from_ymd_opt(acquisition_year, month, day).unwrap();

        let coefficient = gipuzkoa_coefficient(disposal_year, acquired, &BTreeMap::new()).unwrap();
        prop_assert!(coefficient > Decimal::ZERO, "{acquisition_year} -> {coefficient}");
    }
}

proptest! {
    /// **An override is accepted exactly when it is a plausible coefficient, and never clamped.**
    ///
    /// `validate_overrides` is the only gate between a mistyped Decreto Foral row and a cost basis
    /// actualized by the wrong order of magnitude, so the acceptance boundary is the property:
    /// accepted iff `0 < c ≤ 10`. An accepted override is then the coefficient the lot is priced at,
    /// which keeps the positivity law above true for config-supplied years too.
    #[test]
    fn an_override_is_accepted_exactly_when_it_is_a_plausible_coefficient(
        disposal_year in 2024i32..=2030,
        acquisition_year in 1990i32..=2024,
        coefficient in -20_000i64..=200_000i64,
    ) {
        let coefficient = Decimal::new(coefficient, 4);
        let mut table = BTreeMap::new();
        table.insert(acquisition_year, coefficient);
        let mut overrides = BTreeMap::new();
        overrides.insert(disposal_year, table);

        let accepted = validate_overrides(&overrides).is_ok();
        prop_assert_eq!(accepted, coefficient > Decimal::ZERO && coefficient <= dec!(10));

        if accepted && acquisition_year <= disposal_year {
            let acquired = Date::from_ymd_opt(acquisition_year, 6, 15).unwrap();
            let priced = gipuzkoa_coefficient(disposal_year, acquired, &overrides).unwrap();
            prop_assert_eq!(priced, coefficient);
            prop_assert!(priced > Decimal::ZERO);
        }
    }
}

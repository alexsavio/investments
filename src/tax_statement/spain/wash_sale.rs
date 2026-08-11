//! The valores-homogéneos rule (NF 3/2014 art. 43.g / LIRPF art. 33.5.f).
//!
//! A loss on listed securities is **not deductible** when homogeneous securities were acquired
//! within two months before or after the sale. The loss is deferred rather than destroyed: it
//! becomes integrable again as the securities that blocked it leave the estate.
//!
//! This module holds the primitives the rule is built from — the window, instrument identity, and
//! the matching between a sale's FIFO lots and the acquisitions that block it.
//!
//! Out of scope, both deliberately: art. 43.h (unlisted securities, one-year window) and art. 43.i
//! (fungible crypto). Both need a different window and neither is reachable from an IB statement
//! the tool supports.

use std::collections::BTreeMap;

use chrono::Months;

use crate::broker_statement::{StockBuy, StockSource};
use crate::instruments::InstrumentInfo;
use crate::types::{Date, Decimal};

/// Months either side of a sale in which a homogeneous acquisition blocks the loss.
///
/// NF 3/2014 art. 43.g and LIRPF art. 33.5.f both say "dos meses anteriores o posteriores a dichas
/// transmisiones" for securities admitted to trading on a regulated market.
pub const WINDOW_MONTHS: u32 = 2;

/// The `[sale − 2 months, sale + 2 months]` window, both ends inclusive.
///
/// Calendar months, not 60 days: chrono clamps to the end of the target month, so a 31 March sale
/// looks back to 31 January and forward to 31 May, and a 30 April sale looks back to 29 February in
/// a leap year and 28 February otherwise.
// TODO(verify): whether the endpoints themselves are inside the window. Neither text says whether
// "dos meses anteriores" includes the day exactly two months back. Inclusive is the conservative
// reading — it defers more, which understates the deductible loss rather than overstating it.
pub fn window(sale_date: Date) -> (Date, Date) {
    // The fallbacks are unreachable for any date a broker statement can carry; they only exist so
    // the window degrades to "unbounded" instead of panicking at chrono's representable limits.
    let start = sale_date
        .checked_sub_months(Months::new(WINDOW_MONTHS))
        .unwrap_or(Date::MIN);
    let end = sale_date
        .checked_add_months(Months::new(WINDOW_MONTHS))
        .unwrap_or(Date::MAX);
    (start, end)
}

/// Identity for "valores homogéneos": the ISIN when the statement carries one, the ticker otherwise.
///
/// Same-ISIN is the right test. RIRPF art. 8 and DF 33/2014 art. 47 define valores homogéneos as
/// securities of the same issuer forming part of a single operation of issue **and** carrying the
/// same rights — which is what an ISIN identifies. DGT V0796-26 confirms the consequences: different
/// share classes of the same issuer are **not** homogeneous, and neither are two ETFs of different
/// issuers tracking the same index. The ISIN is preferred over the ticker because tickers get reused
/// and renamed while two lines of the same issue under different tickers are still the same
/// securities.
pub fn instrument_key(instrument_info: &InstrumentInfo, symbol: &str) -> String {
    instrument_info
        .get(symbol)
        .and_then(|info| info.isin.iter().next())
        .map(|isin| isin.to_string())
        .unwrap_or_else(|| symbol.to_owned())
}

/// Whether a buy counts as an acquisition of homogeneous securities.
///
/// A corporate-action buy is excluded: the tool emulates a stock split as a sell of the old line
/// and a buy of the new one, but no securities were acquired — they are the same shares re-expressed
/// — so treating it as a repurchase would defer losses nobody could have avoided. A vest is a real
/// acquisition and counts.
pub fn is_acquisition(buy: &StockBuy) -> bool {
    match buy.type_ {
        StockSource::Trade { .. } | StockSource::Grant => true,
        StockSource::CorporateAction => false,
    }
}

/// Shares whose acquisition blocked part of a loss, and the loss they still block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockedLot {
    /// Acquisition date of the shares doing the blocking.
    pub buy_date: Date,
    /// Shares still blocking, i.e. acquired inside the window and not yet disposed of.
    pub blocked_quantity: Decimal,
    /// Loss still deferred by those shares, as a positive magnitude.
    pub deferred_loss: Decimal,
    /// The loss-making sale this deferral came from.
    pub origin_sale_date: Date,
}

/// An acquisition the window search can match against.
///
/// Every quantity in this module is in one unit — post-split shares as of the statement's last date
/// — so acquisitions, disposals, consumed lots and blocked lots stay comparable across a split.
pub struct Acquisition {
    pub key: String,
    pub date: Date,
    pub quantity: Decimal,
}

/// A disposal, as the replay sees it.
pub struct Disposal {
    pub key: String,
    pub date: Date,
    /// Shares disposed of, in post-split units.
    pub quantity: Decimal,
    /// Fiscal result after actualization. `None` when the disposal year has no coefficient table,
    /// in which case the sale can still release blocked lots but cannot create new ones.
    pub fiscal_result: Option<Decimal>,
    /// The FIFO lots this sale consumed: acquisition date and quantity.
    pub consumed: Vec<(Date, Decimal)>,
}

/// One deferred amount becoming integrable again because the shares blocking it were disposed of.
#[derive(Clone, Debug)]
pub struct Reintegration {
    /// Acquisition date of the shares that were blocking the loss.
    pub buy_date: Date,
    /// The loss-making sale the released amount was deferred from.
    pub origin_sale_date: Date,
    /// Loss released, as a positive magnitude.
    pub amount: Decimal,
}

/// What one disposal did to the valores-homogéneos state.
#[derive(Clone, Debug, Default)]
pub struct DisposalOutcome {
    /// Loss this sale could not deduct, as a positive magnitude.
    pub deferred_loss: Decimal,
    /// Earlier deferrals this sale released.
    pub reintegrations: Vec<Reintegration>,
}

/// Shares acquired on one date, and how much of them the replay has seen disposed of.
///
/// Same-date acquisitions share a bucket: a FIFO lot identifies itself by its conclusion date, so
/// two buys on the same day are indistinguishable to the consumption bookkeeping — and they are
/// always both inside or both outside any window, so nothing is lost by merging them.
#[derive(Default)]
struct AcquisitionBucket {
    quantity: Decimal,
    consumed: Decimal,
}

#[derive(Default)]
struct InstrumentState {
    acquisitions: BTreeMap<Date, AcquisitionBucket>,
    blocked: Vec<BlockedLot>,
}

/// Replays a statement's disposals in date order, deferring losses and releasing earlier deferrals.
///
/// The replay spans **all** of the statement's history rather than the filing year alone, because a
/// deferral created in one year is released in another, and a repurchase two months after a
/// December sale falls in the following year.
#[derive(Default)]
pub struct WashSaleEngine {
    instruments: BTreeMap<String, InstrumentState>,
}

impl WashSaleEngine {
    /// Build the engine over every acquisition the statement carries, plus any blocked lots the
    /// filer brought in from an earlier return.
    ///
    /// Acquisitions are registered up front rather than as the replay reaches them: a repurchase
    /// *after* a loss-making sale still blocks it, so the window search has to see the future.
    pub fn new(
        acquisitions: impl IntoIterator<Item = Acquisition>,
        opening: impl IntoIterator<Item = (String, BlockedLot)>,
    ) -> WashSaleEngine {
        let mut engine = WashSaleEngine::default();

        for acquisition in acquisitions {
            let bucket = engine
                .instruments
                .entry(acquisition.key)
                .or_default()
                .acquisitions
                .entry(acquisition.date)
                .or_default();
            bucket.quantity += acquisition.quantity;
        }

        for (key, lot) in opening {
            engine.instruments.entry(key).or_default().blocked.push(lot);
        }

        engine
    }

    /// Replay one disposal: release first, then defer.
    ///
    /// The order matters. A sale of blocked shares makes their deferred loss integrable *before*
    /// the same sale's own result is tested, and the freed shares must not then count as available
    /// to block that very sale.
    ///
    /// A release only counts to the extent the disposal was **definitive**. Both statutes make the
    /// deferred loss integrable "a medida que se transmitan los activos", and DGT V3282-18 reads
    /// that as requiring a real exit: selling the blocking shares and buying homogeneous ones back
    /// inside the window does not end the deferral, it moves it onto the new shares. The released
    /// amount is therefore split by the same matched fraction the rule uses everywhere else —
    /// the matched part is re-attached, the rest becomes integrable.
    pub fn process(&mut self, disposal: &Disposal) -> DisposalOutcome {
        let state = self.instruments.entry(disposal.key.clone()).or_default();
        let mut outcome = DisposalOutcome::default();

        let mut released = Vec::new();
        for &(lot_date, quantity) in &disposal.consumed {
            state
                .acquisitions
                .entry(lot_date)
                .or_default()
                .consumed += quantity;
            release(state, lot_date, quantity, &mut released);
        }
        state.blocked.retain(|lot| lot.blocked_quantity > Decimal::ZERO);

        let own_loss = match disposal.fiscal_result {
            Some(result) if result < Decimal::ZERO => -result,
            _ => Decimal::ZERO,
        };

        if own_loss.is_zero() && released.is_empty() {
            return outcome;
        }

        let (matched, matched_total) = match_window(state, disposal);
        let blocked_fraction = if disposal.quantity > Decimal::ZERO {
            matched_total / disposal.quantity
        } else {
            Decimal::ZERO
        };

        // Each deferred amount the matched shares end up carrying, with the sale it came from.
        let mut placements: Vec<(Date, Decimal)> = Vec::new();

        outcome.deferred_loss = own_loss * blocked_fraction;
        if outcome.deferred_loss > Decimal::ZERO {
            placements.push((disposal.date, outcome.deferred_loss));
        }

        for reintegration in released {
            let re_attached = reintegration.amount * blocked_fraction;
            let integrable = reintegration.amount - re_attached;

            if integrable > Decimal::ZERO {
                outcome.reintegrations.push(Reintegration {
                    amount: integrable,
                    ..reintegration
                });
            }
            if re_attached > Decimal::ZERO {
                placements.push((reintegration.origin_sale_date, re_attached));
            }
        }

        block(state, &matched, matched_total, &placements);

        outcome
    }

    /// Blocked lots still standing, keyed by instrument, for the carry-out config.
    pub fn blocked_lots(&self) -> impl Iterator<Item = (&str, &BlockedLot)> {
        self.instruments.iter().flat_map(|(key, state)| {
            state.blocked.iter().map(move |lot| (key.as_str(), lot))
        })
    }
}

/// Release the deferrals blocked by shares acquired on `lot_date` that this sale just consumed.
///
/// Oldest deferral first, and pro rata to the share of the blocked lot consumed.
///
/// Both statutes word reintegration as the disposal of "los activos que permanezcan en el patrimonio
/// de la persona contribuyente" (NF 3/2014 art. 43 closing ¶ / LIRPF art. 33.5 closing ¶), but the
/// DGT reads that as keyed to the **recompra pool** — the securities whose acquisition blocked the
/// loss — and applies FIFO within it: V0913-08 sets the pool, V3282-18 adds that the releasing
/// transfer must itself be definitive. Attaching the deferral to the blocking lots and releasing it
/// as those lots are consumed is exactly that reading, and it settles which shares of a
/// partly-blocked acquisition date a sale consumes first: the blocked ones.
fn release(
    state: &mut InstrumentState,
    lot_date: Date,
    mut quantity: Decimal,
    reintegrations: &mut Vec<Reintegration>,
) {
    for lot in &mut state.blocked {
        if quantity <= Decimal::ZERO {
            break;
        }
        if lot.buy_date != lot_date || lot.blocked_quantity <= Decimal::ZERO {
            continue;
        }

        let taken = std::cmp::min(quantity, lot.blocked_quantity);
        let released = lot.deferred_loss * taken / lot.blocked_quantity;

        lot.deferred_loss -= released;
        lot.blocked_quantity -= taken;
        quantity -= taken;

        if released > Decimal::ZERO {
            reintegrations.push(Reintegration {
                buy_date: lot.buy_date,
                origin_sale_date: lot.origin_sale_date,
                amount: released,
            });
        }
    }
}

/// Homogeneous acquisitions inside the disposal's window that can still block, oldest first.
///
/// Each acquired share blocks at most one sold share, so the match is capped at the disposal's own
/// quantity. An acquisition already consumed — by this sale or an earlier one — or already blocking
/// cannot block again: those shares are gone or spoken for.
fn match_window(state: &InstrumentState, disposal: &Disposal) -> (Vec<(Date, Decimal)>, Decimal) {
    let (start, end) = window(disposal.date);

    let mut matched: Vec<(Date, Decimal)> = Vec::new();
    let mut matched_total = Decimal::ZERO;

    for (&date, bucket) in state.acquisitions.range(start..=end) {
        let remaining = disposal.quantity - matched_total;
        if remaining <= Decimal::ZERO {
            break;
        }

        let blocking: Decimal = state
            .blocked
            .iter()
            .filter(|lot| lot.buy_date == date)
            .map(|lot| lot.blocked_quantity)
            .sum();

        let available = bucket.quantity - bucket.consumed - blocking;
        if available <= Decimal::ZERO {
            continue;
        }

        let taken = std::cmp::min(available, remaining);
        matched.push((date, taken));
        matched_total += taken;
    }

    (matched, matched_total)
}

/// Attach the deferred amounts to the matched shares.
///
/// The matched shares are shared out between the placements in proportion to their amounts, so the
/// blocked quantity across every lot this disposal creates stays exactly `matched_total` — one
/// blocked share per matched share, however many separate deferrals ride on them.
fn block(
    state: &mut InstrumentState,
    matched: &[(Date, Decimal)],
    matched_total: Decimal,
    placements: &[(Date, Decimal)],
) {
    let total: Decimal = placements.iter().map(|&(_, amount)| amount).sum();
    if matched_total <= Decimal::ZERO || total <= Decimal::ZERO {
        return;
    }

    for &(date, quantity) in matched {
        for &(origin_sale_date, amount) in placements {
            state.blocked.push(BlockedLot {
                buy_date: date,
                blocked_quantity: quantity * amount / total,
                deferred_loss: amount * quantity / matched_total,
                origin_sale_date,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// Calendar months, with chrono's end-of-month clamping — the seam where a naive 60-day window
    /// would diverge from the statute.
    #[rstest]
    // A year-end sale reaches into the previous and the following year.
    #[case(date!(2026, 12, 31), date!(2026, 10, 31), date!(2027, 2, 28))]
    // Two months back from 30 April is the last day of February, which depends on the leap year.
    #[case(date!(2024, 4, 30), date!(2024, 2, 29), date!(2024, 6, 30))]
    #[case(date!(2023, 4, 30), date!(2023, 2, 28), date!(2023, 6, 30))]
    // 31 January and 31 May both exist, so nothing is clamped here.
    #[case(date!(2026, 3, 31), date!(2026, 1, 31), date!(2026, 5, 31))]
    // A 31 December sale in a leap-year run still lands on 28 February: 2025 is not a leap year.
    #[case(date!(2024, 12, 31), date!(2024, 10, 31), date!(2025, 2, 28))]
    // An ordinary mid-month sale crosses the year boundary backwards without clamping.
    #[case(date!(2026, 1, 15), date!(2025, 11, 15), date!(2026, 3, 15))]
    // 29 February itself: two months either side of a leap day.
    #[case(date!(2024, 2, 29), date!(2023, 12, 29), date!(2024, 4, 29))]
    fn window_spans_two_calendar_months(
        #[case] sale: Date,
        #[case] expected_start: Date,
        #[case] expected_end: Date,
    ) {
        assert_eq!(window(sale), (expected_start, expected_end));
    }

    /// Identity prefers the ISIN, because tickers get reused and renamed while an ISIN does not.
    #[test]
    fn identity_prefers_the_isin_over_the_ticker() {
        let mut instruments = InstrumentInfo::new();
        instruments
            .add("AAPL")
            .unwrap()
            .add_isin("US0378331005".parse().unwrap());

        assert_eq!(instrument_key(&instruments, "AAPL"), "US0378331005");
        // An instrument the statement carries no ISIN for still needs an identity.
        assert_eq!(instrument_key(&instruments, "MSFT"), "MSFT");
        assert_eq!(instrument_key(&InstrumentInfo::new(), "AAPL"), "AAPL");
    }

    const KEY: &str = "US0378331005";

    fn acquisition(date: Date, quantity: Decimal) -> Acquisition {
        Acquisition { key: KEY.to_owned(), date, quantity }
    }

    fn disposal(
        date: Date, quantity: Decimal, result: Decimal, consumed: &[(Date, Decimal)],
    ) -> Disposal {
        Disposal {
            key: KEY.to_owned(),
            date,
            quantity,
            fiscal_result: Some(result),
            consumed: consumed.to_vec(),
        }
    }

    /// A repurchase *after* the sale blocks it: the replay has to see acquisitions the sale itself
    /// could not, which is why they are all registered before the first disposal is processed.
    ///
    /// Sell 100 at a €900 loss, buy 40 back inside the window → 40 of the 100 sold shares are
    /// blocked, so 900 × 40/100 = €360 is deferred and €540 stays deductible.
    #[test]
    fn a_repurchase_after_the_sale_defers_the_matched_fraction() {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(date!(2026, 4, 20), dec!(40)),
            ],
            [],
        );

        let outcome = engine.process(&disposal(
            date!(2026, 3, 10), dec!(100), dec!(-900), &[(date!(2026, 1, 5), dec!(100))]));

        assert_eq!(outcome.deferred_loss, dec!(360));
        assert!(outcome.reintegrations.is_empty());

        let blocked: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(blocked, vec![&BlockedLot {
            buy_date: date!(2026, 4, 20),
            blocked_quantity: dec!(40),
            deferred_loss: dec!(360),
            origin_sale_date: date!(2026, 3, 10),
        }]);
    }

    /// A repurchase *before* the sale blocks only the shares the sale did not itself consume.
    ///
    /// Buy 100 then 50, sell the first 100 at a €900 loss. Both acquisitions are in the window, but
    /// the 100 the sale consumed are gone — only the 50 still held can block. So 50 of the 100 sold
    /// shares are matched and half the loss is deferred. Counting the consumed lot too would defer
    /// the whole €900.
    #[test]
    fn a_repurchase_before_the_sale_cannot_block_with_shares_the_sale_consumed() {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 10), dec!(100)),
                acquisition(date!(2026, 2, 15), dec!(50)),
            ],
            [],
        );

        let outcome = engine.process(&disposal(
            date!(2026, 3, 10), dec!(100), dec!(-900), &[(date!(2026, 1, 10), dec!(100))]));

        assert_eq!(outcome.deferred_loss, dec!(450));
        let blocked: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(blocked[0].buy_date, date!(2026, 2, 15));
        assert_eq!(blocked[0].blocked_quantity, dec!(50));
    }

    /// The window boundary, from the engine's own matching rather than the arithmetic helper: a
    /// sale on 2026-03-10 reaches back to 2026-01-10 and forward to 2026-05-10, both inclusive, and
    /// an acquisition one day outside either end blocks nothing.
    #[rstest]
    #[case(date!(2026, 1, 9), dec!(0))]
    #[case(date!(2026, 1, 10), dec!(900))]
    #[case(date!(2026, 5, 10), dec!(900))]
    #[case(date!(2026, 5, 11), dec!(0))]
    fn the_window_boundary_is_inclusive(#[case] acquired: Date, #[case] expected: Decimal) {
        let mut engine = WashSaleEngine::new([acquisition(acquired, dec!(100))], []);
        let outcome = engine.process(&disposal(date!(2026, 3, 10), dec!(100), dec!(-900), &[]));
        assert_eq!(outcome.deferred_loss, expected);
    }

    /// Selling the blocked shares releases their deferral pro rata, and the rest stays blocked.
    #[test]
    fn disposing_of_blocked_shares_releases_the_deferral_pro_rata() {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(date!(2026, 4, 20), dec!(40)),
            ],
            [],
        );
        engine.process(&disposal(
            date!(2026, 3, 10), dec!(100), dec!(-900), &[(date!(2026, 1, 5), dec!(100))]));

        // 25 of the 40 blocked shares go; 360 × 25/40 becomes integrable again.
        let outcome = engine.process(&disposal(
            date!(2026, 11, 15), dec!(25), dec!(450), &[(date!(2026, 4, 20), dec!(25))]));

        assert_eq!(outcome.reintegrations.len(), 1);
        assert_eq!(outcome.reintegrations[0].amount, dec!(225));
        // Labelled with the sale the deferral came from, not the sale that released it.
        assert_eq!(outcome.reintegrations[0].origin_sale_date, date!(2026, 3, 10));

        let blocked: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(blocked[0].blocked_quantity, dec!(15));
        assert_eq!(blocked[0].deferred_loss, dec!(135));
    }

    /// A partial repurchase spread over two acquisition dates splits the deferral pro rata, and a
    /// later sale releases from each in FIFO order.
    #[test]
    fn a_multi_lot_repurchase_splits_and_releases_pro_rata() {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(date!(2026, 4, 20), dec!(25)),
                acquisition(date!(2026, 5, 5), dec!(15)),
            ],
            [],
        );

        let deferral = engine.process(&disposal(
            date!(2026, 3, 10), dec!(100), dec!(-900), &[(date!(2026, 1, 5), dec!(100))]));
        assert_eq!(deferral.deferred_loss, dec!(360));

        let blocked: Vec<Decimal> =
            engine.blocked_lots().map(|(_, lot)| lot.deferred_loss).collect();
        assert_eq!(blocked, vec![dec!(225), dec!(135)]);

        // Sell 30: the whole 25-share lot and 5 of the 15-share one.
        let release = engine.process(&disposal(
            date!(2026, 11, 15), dec!(30), dec!(630),
            &[(date!(2026, 4, 20), dec!(25)), (date!(2026, 5, 5), dec!(5))]));

        let released: Vec<Decimal> =
            release.reintegrations.iter().map(|entry| entry.amount).collect();
        assert_eq!(released, vec![dec!(225), dec!(45)]);

        let surviving: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(surviving.len(), 1);
        assert_eq!(surviving[0].blocked_quantity, dec!(10));
        assert_eq!(surviving[0].deferred_loss, dec!(90));
    }

    /// Each acquired share blocks at most one sold share: buying back more than was sold cannot
    /// defer more than the whole loss.
    #[test]
    fn a_larger_repurchase_cannot_defer_more_than_the_loss() {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(date!(2026, 4, 20), dec!(500)),
            ],
            [],
        );

        let outcome = engine.process(&disposal(
            date!(2026, 3, 10), dec!(100), dec!(-900), &[(date!(2026, 1, 5), dec!(100))]));

        assert_eq!(outcome.deferred_loss, dec!(900));
        let blocked: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(blocked[0].blocked_quantity, dec!(100));
    }

    /// Two loss sales chasing the same acquisitions. The second cannot re-use shares the first
    /// blocked, and the shares it consumes release the first deferral on the way through — release
    /// before deferral, so the freed shares are gone rather than available to block the sale that
    /// freed them.
    ///
    /// The second sale is itself only 40% definitive — 60 of its 100 shares were bought back on
    /// 2026-04-20 — so 60% of the released deferral is re-attached to those shares rather than
    /// becoming integrable (DGT V3282-18).
    #[test]
    fn shares_already_blocking_cannot_block_again() {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(date!(2026, 2, 1), dec!(100)),
                acquisition(date!(2026, 4, 20), dec!(60)),
            ],
            [],
        );

        let first = engine.process(&disposal(
            date!(2026, 3, 10), dec!(100), dec!(-900), &[(date!(2026, 1, 5), dec!(100))]));
        // The 2026-02-01 and 2026-04-20 acquisitions are both in the window: 100 + 60 available,
        // capped at the 100 shares sold.
        assert_eq!(first.deferred_loss, dec!(900));

        let second = engine.process(&disposal(
            date!(2026, 3, 20), dec!(100), dec!(-500), &[(date!(2026, 2, 1), dec!(100))]));
        // Selling the blocking shares releases the whole first deferral, but only 40 of the 100
        // shares left the estate for good: 900 × 40% = 360 becomes integrable.
        assert_eq!(second.reintegrations.len(), 1);
        assert_eq!(second.reintegrations[0].amount, dec!(360));
        assert_eq!(second.reintegrations[0].origin_sale_date, date!(2026, 3, 10));
        // Only the 60 shares bought on 2026-04-20 are free to block this loss, so 500 × 60/100
        // defers. The shares it just freed are gone, not available to itself.
        assert_eq!(second.deferred_loss, dec!(300));

        // The 60 shares now carry both deferrals: this sale's 300 and the re-attached 540.
        let blocked: Decimal =
            engine.blocked_lots().map(|(_, lot)| lot.deferred_loss).sum();
        assert_eq!(blocked, dec!(840));
        let blocked_quantity: Decimal =
            engine.blocked_lots().map(|(_, lot)| lot.blocked_quantity).sum();
        assert_eq!(blocked_quantity, dec!(60), "one blocked share per matched share");
    }

    /// A disposal with a homogeneous repurchase inside its own window is not a "transmisión
    /// definitiva", so it does not make an earlier deferral integrable — it moves it onto the
    /// shares that were bought back (DGT V3282-18). A later clean sale then releases it.
    #[test]
    fn a_release_needs_a_definitive_disposal() {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(date!(2026, 4, 20), dec!(40)),
                acquisition(date!(2026, 9, 15), dec!(40)),
            ],
            [],
        );

        engine.process(&disposal(
            date!(2026, 3, 10), dec!(100), dec!(-900), &[(date!(2026, 1, 5), dec!(100))]));

        // Sells every blocking share, but buys 40 back inside the window: nothing is integrable.
        let chained = engine.process(&disposal(
            date!(2026, 8, 10), dec!(40), dec!(720), &[(date!(2026, 4, 20), dec!(40))]));
        assert!(chained.reintegrations.is_empty());

        let blocked: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].buy_date, date!(2026, 9, 15));
        assert_eq!(blocked[0].deferred_loss, dec!(360));
        // Still labelled with the sale it originally came from, not the one that moved it.
        assert_eq!(blocked[0].origin_sale_date, date!(2026, 3, 10));

        // Nothing bought back this time, so the transfer is definitive.
        let definitive = engine.process(&disposal(
            date!(2026, 12, 20), dec!(40), dec!(720), &[(date!(2026, 9, 15), dec!(40))]));
        assert_eq!(definitive.reintegrations.len(), 1);
        assert_eq!(definitive.reintegrations[0].amount, dec!(360));
        assert_eq!(definitive.reintegrations[0].origin_sale_date, date!(2026, 3, 10));
        assert_eq!(engine.blocked_lots().count(), 0);
    }

    /// A disposal the tool cannot price still releases earlier deferrals — the release does not
    /// depend on the sale's own result — but it can never create a new one.
    #[test]
    fn an_unpriced_disposal_releases_but_never_defers() {
        let mut engine = WashSaleEngine::new(
            [acquisition(date!(2026, 4, 20), dec!(40))],
            [(
                KEY.to_owned(),
                BlockedLot {
                    buy_date: date!(2026, 4, 20),
                    blocked_quantity: dec!(40),
                    deferred_loss: dec!(360),
                    origin_sale_date: date!(2026, 3, 10),
                },
            )],
        );

        let outcome = engine.process(&Disposal {
            key: KEY.to_owned(),
            date: date!(2026, 11, 15),
            quantity: dec!(40),
            fiscal_result: None,
            consumed: vec![(date!(2026, 4, 20), dec!(40))],
        });

        assert_eq!(outcome.reintegrations[0].amount, dec!(360));
        assert_eq!(outcome.deferred_loss, dec!(0));
        assert_eq!(engine.blocked_lots().count(), 0);
    }

    /// Blocked lots brought in from an earlier return are part of the state from the start, so a
    /// statement that does not contain the original loss still reintegrates it correctly.
    #[test]
    fn opening_blocked_lots_are_seeded_from_the_prior_return() {
        let mut engine = WashSaleEngine::new(
            [acquisition(date!(2025, 12, 20), dec!(15))],
            [(
                KEY.to_owned(),
                BlockedLot {
                    buy_date: date!(2025, 12, 20),
                    blocked_quantity: dec!(15),
                    deferred_loss: dec!(420),
                    origin_sale_date: date!(2025, 12, 10),
                },
            )],
        );

        let outcome = engine.process(&disposal(
            date!(2026, 5, 10), dec!(15), dec!(100), &[(date!(2025, 12, 20), dec!(15))]));

        assert_eq!(outcome.reintegrations[0].amount, dec!(420));
        assert_eq!(engine.blocked_lots().count(), 0);
    }
}

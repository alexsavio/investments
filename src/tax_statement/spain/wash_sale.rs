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
//!
//! **Which window a third-country listing takes is settled for equivalence-decision venues.** The
//! two-month limb is written for securities "admitidos a negociación en alguno de los mercados
//! regulados de valores definidos en la Directiva 2014/65/UE", and the DGT reads that as reaching a
//! third-country market covered by an in-force Commission equivalence decision under MiFID II art.
//! 25(4)(a): CV V0778-25 (05-05-2025) answers it for NYSE, Nasdaq and CME by name, and V0951-25
//! (30-05-2025) generalizes to "los mercados de valores de Estados Unidos", both conditioned on the
//! decision standing — "mientras dicha decisión de equivalencia no haya sido objeto de derogación".
//! US venues are equivalent per Commission Implementing Decision (EU) 2017/2320, Australia per
//! 2017/2318 and Hong Kong per 2017/2319. Switzerland's decisions lapsed on 30-06-2019 and were
//! never renewed, so a SIX-only line falls to the one-year limb, as does any venue with no decision
//! at all. Following published DGT criteria also shields the filer from penalties (LGT art.
//! 179.2.d).
//!
//! Gipuzkoa: NF 3/2014 art. 43.g clones the state wording and interprets the same EU concept, but no
//! foral pronouncement exists — DGT criteria are persuasive there, not formally binding.
//!
//! [`venue_takes_the_two_month_window`] classifies the statement's listing venues against that
//! table. The engine's window does **not** change with it: two months applies to every instrument,
//! and a loss that only the one-year limb would defer is reported for review instead
//! ([`DisposalOutcome::wider_window_loss`]).

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, Months};

use crate::broker_statement::{StockBuy, StockSource};
use crate::instruments::InstrumentInfo;
use crate::types::{Date, Decimal};

/// Months either side of a sale in which a homogeneous acquisition blocks the loss.
///
/// NF 3/2014 art. 43.g and LIRPF art. 33.5.f both say "dos meses anteriores o posteriores a dichas
/// transmisiones" for securities admitted to trading on a regulated market.
pub const WINDOW_MONTHS: u32 = 2;

/// Months either side under the limb for securities **not** admitted to a regulated market.
///
/// NF 3/2014 art. 43.h / LIRPF art. 33.5.g. The engine never defers on this window; it only measures
/// what would be deferred if a venue turned out not to be covered by the two-month limb.
pub const WIDER_WINDOW_MONTHS: u32 = 12;

/// Venues whose listings take the **two-month** limb, as the broker names them.
///
/// EEA venues are regulated markets under Directive 2014/65 itself. The third-country venues below
/// are covered by an in-force Commission equivalence decision under MiFID II art. 25(4)(a) — (EU)
/// 2017/2320 for the United States, 2017/2318 for Australia, 2017/2319 for Hong Kong — which DGT CV
/// V0778-25 and V0951-25 read into art. 33.5.f. Anything not listed here is treated as unsettled,
/// **not** as excluded: OTC/pink venues are genuinely not regulated markets, Switzerland's decisions
/// lapsed on 30-06-2019, and the United Kingdom, Canada and Japan have no decision in force.
const TWO_MONTH_WINDOW_VENUES: &[&str] = &[
    // United States, Annex of (EU) 2017/2320.
    "AMEX", "ARCA", "BATS", "BYX", "BZX", "CBOE", "EDGA", "EDGX", "IEX", "NASDAQ", "NYSE",
    "NYSENAT", "PSE", "PHLX", // EEA regulated markets IB reports for stocks.
    "AEB", "BM", "BVL", "BVME", "CPH", "ENEXT.BE", "FWB", "GETTEX", "HEX", "IBIS", "IBIS2", "ICEX",
    "MIL", "OSE", "SBF", "SFB", "SWB", "TGATE", "VSE", "WSE",
    // Australia, (EU) 2017/2318; Hong Kong, (EU) 2017/2319.
    "ASX", "CHIXAU", "SEHK",
];

/// Whether a listing venue is settled as taking the two-month window.
///
/// `None` when the statement names no venue for the instrument: unknown is not the same as excluded,
/// and both cases are reported rather than acted on.
pub fn venue_takes_the_two_month_window<'a>(
    venues: impl IntoIterator<Item = &'a str>,
) -> Option<bool> {
    let mut known = false;

    for venue in venues {
        known = true;
        if !TWO_MONTH_WINDOW_VENUES
            .iter()
            .any(|equivalent| equivalent.eq_ignore_ascii_case(venue))
        {
            return Some(false);
        }
    }

    known.then_some(true)
}

/// The `[sale − 2 months, sale + 2 months]` window, both ends inclusive.
///
/// Calendar months, not 60 days: chrono clamps to the end of the target month, so a 31 March sale
/// looks back to 31 January and forward to 31 May, and a 30 April sale looks back to 29 February in
/// a leap year and 28 February otherwise.
///
/// Both readings the arithmetic has to settle are the settled ones. A period fixed in months runs
/// "de fecha a fecha" (Código Civil art. 5.1, supletory in tax matters through LGT art. 7.2), and
/// the Tribunal Supremo computes that terminal ordinal as the **last day of the period** rather than
/// the first day past it: STS 552/2022 (10-05-2022, RC 1874/2021), STS 02-07-2020 (RC 3780/2019) and
/// STS 02-04-2008 (rec. 323/2004), where publication on 13-02 makes a one-month period expire on
/// 13-03 and a filing on 15-03 late; STS 287/2009 applies the same de-fecha-a-fecha count in natural
/// days to substantive periods. When the terminal ordinal does not exist the period ends on the last
/// day of that month (CC art. 5.1; Ley 39/2015 art. 30.4). A practitioner worked example runs the
/// same way: a 10-02-2024 sale gives a window of 10-12-2023 to 10-04-2024.
///
/// No authority computes art. 33.5.f boundaries with concrete dates, so where an outcome actually
/// turns on one of those two edges the disposal reports it ([`DisposalOutcome::boundary_reviews`]).
pub fn window(sale_date: Date) -> (Date, Date) {
    months_window(sale_date, WINDOW_MONTHS)
}

/// The one-year window of the unlisted limb, on the same arithmetic as [`window`].
pub fn wider_window(sale_date: Date) -> (Date, Date) {
    months_window(sale_date, WIDER_WINDOW_MONTHS)
}

fn months_window(sale_date: Date, months: u32) -> (Date, Date) {
    // The fallbacks are unreachable for any date a broker statement can carry; they only exist so
    // the window degrades to "unbounded" instead of panicking at chrono's representable limits.
    let start = sale_date
        .checked_sub_months(Months::new(months))
        .unwrap_or(Date::MIN);
    let end = sale_date
        .checked_add_months(Months::new(months))
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
    /// Loss the **one-year** limb would defer on top of `deferred_loss`, as a positive magnitude.
    ///
    /// Non-zero only when homogeneous securities were acquired inside the year but outside the two
    /// months. It never changes what this sale defers; it is what the caller reports when the
    /// instrument's listing venue is not one the two-month limb is settled for.
    pub wider_window_loss: Decimal,
    /// Window edges this disposal's outcome actually hangs on.
    pub boundary_reviews: Vec<BoundaryReview>,
}

/// How a window edge could be read differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryKind {
    /// A repurchase on the window's own terminal day. Deferred here; deductible if the statute's
    /// "dos meses anteriores o posteriores" excluded its terminal ordinal.
    Endpoint,
    /// A repurchase one day outside the terminal day. Deductible here; deferred if the period ran
    /// one day further than de fecha a fecha puts it.
    NearMiss,
    /// The terminal day is a month-end clamp: the sale's day-of-month does not exist in the target
    /// month, so the edge is a construct rather than the statute's own ordinal.
    Clamp,
}

impl BoundaryKind {
    /// One phrase, used verbatim by every surface that reports the review.
    pub fn description(self) -> &'static str {
        match self {
            BoundaryKind::Endpoint => {
                "homogeneous securities were acquired on the window's own terminal day, which is \
                 inside the window only because the terminal ordinal counts as the last day of the \
                 period"
            }
            BoundaryKind::NearMiss => {
                "homogeneous securities were acquired one day outside the window, so they block \
                 nothing only because the period ends where de fecha a fecha puts it"
            }
            BoundaryKind::Clamp => {
                "the window's terminal day is a month-end clamp — the sale's day of the month does \
                 not exist in that month — so the edge is a construct, not the statute's own ordinal"
            }
        }
    }
}

/// A window edge a disposal's deferral actually turns on.
#[derive(Clone, Debug)]
pub struct BoundaryReview {
    pub kind: BoundaryKind,
    /// The window edge as the tool computes it.
    pub boundary_date: Date,
    /// The edge the alternative reading would use.
    pub alternative_date: Date,
    /// Loss that moves between deferred and deductible under that reading, as a positive magnitude.
    pub amount: Decimal,
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
    /// amount is therefore split by the matched fraction of the **blocked** shares the disposal
    /// consumed — the matched part is re-attached, the rest becomes integrable.
    pub fn process(&mut self, disposal: &Disposal) -> DisposalOutcome {
        let state = self.instruments.entry(disposal.key.clone()).or_default();
        let mut outcome = DisposalOutcome::default();

        let mut released = Vec::new();
        let mut blocked_consumed = Decimal::ZERO;
        for &(lot_date, quantity) in &disposal.consumed {
            state.acquisitions.entry(lot_date).or_default().consumed += quantity;
            blocked_consumed += release(state, lot_date, quantity, &mut released);
        }
        state
            .blocked
            .retain(|lot| lot.blocked_quantity > Decimal::ZERO);

        let own_loss = match disposal.fiscal_result {
            Some(result) if result < Decimal::ZERO => -result,
            _ => Decimal::ZERO,
        };

        if own_loss.is_zero() && released.is_empty() {
            return outcome;
        }

        let (matched, matched_total) = match_window(state, disposal, window(disposal.date));
        let blocked_fraction = if disposal.quantity > Decimal::ZERO {
            matched_total / disposal.quantity
        } else {
            Decimal::ZERO
        };

        // What the unlisted limb would add, measured but never applied. The engine defers on two
        // months for every instrument; whether a given listing venue is entitled to that limb is a
        // question about the venue, not about this sale, so the caller decides what to do with it.
        if own_loss > Decimal::ZERO && disposal.quantity > Decimal::ZERO {
            let (_, wider_total) = match_window(state, disposal, wider_window(disposal.date));
            outcome.wider_window_loss =
                own_loss * (wider_total - matched_total) / disposal.quantity;
            outcome.boundary_reviews = review_boundaries(state, disposal, own_loss, matched_total);
        }

        // Definitiveness is measured on the **blocked** shares, not on the whole disposal: the
        // window match is attributed to them first. Selling 40 blocked plus 60 unblocked shares and
        // buying 40 back replaces every share that was blocking, so nothing left the estate for
        // good — even though only 40% of the sale was matched.
        //
        // V3282-18 gives no allocation rule when a disposal mixes blocked and unblocked shares.
        // Blocked-first is the conservative reading: it re-attaches more and integrates less, which
        // postpones a deduction rather than granting one early.
        let release_fraction = if blocked_consumed > Decimal::ZERO {
            std::cmp::min(matched_total, blocked_consumed) / blocked_consumed
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
            let re_attached = reintegration.amount * release_fraction;
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
        self.instruments
            .iter()
            .flat_map(|(key, state)| state.blocked.iter().map(move |lot| (key.as_str(), lot)))
    }
}

/// Release the deferrals blocked by shares acquired on `lot_date` that this sale just consumed,
/// returning how many blocked shares that took.
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
) -> Decimal {
    let mut consumed = Decimal::ZERO;

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
        consumed += taken;

        if released > Decimal::ZERO {
            reintegrations.push(Reintegration {
                buy_date: lot.buy_date,
                origin_sale_date: lot.origin_sale_date,
                amount: released,
            });
        }
    }

    consumed
}

/// Homogeneous acquisitions inside the disposal's window that can still block, oldest first.
///
/// Each acquired share blocks at most one sold share, so the match is capped at the disposal's own
/// quantity. An acquisition already consumed — by this sale or an earlier one — or already blocking
/// cannot block again: those shares are gone or spoken for.
fn match_window(
    state: &InstrumentState,
    disposal: &Disposal,
    (start, end): (Date, Date),
) -> (Vec<(Date, Decimal)>, Decimal) {
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

/// Name the window edges this disposal's deferral actually turns on.
///
/// Each edge is re-matched against the readings that could replace it: one day inward (the terminal
/// ordinal excluded), one day outward, and — when the edge was clamped to a short month's last day —
/// the date the sale's own day-of-month rolls over to. An edge is reported only when moving it
/// changes how many shares block, and the euros at stake are the loss that moves with them.
///
/// At most one inward and one outward review per edge: two readings that move the same edge the same
/// way are one question, not two.
fn review_boundaries(
    state: &InstrumentState,
    disposal: &Disposal,
    own_loss: Decimal,
    matched_total: Decimal,
) -> Vec<BoundaryReview> {
    let (start, end) = window(disposal.date);
    let per_share = own_loss / disposal.quantity;
    let mut reviews = Vec::new();

    for (edge, anterior) in [(start, true), (end, false)] {
        // chrono only moves the day of the month when the sale's own ordinal is missing there.
        let clamped = edge.day() != disposal.date.day();
        let rolled = clamped.then(|| {
            edge + Duration::days(i64::from(disposal.date.day().saturating_sub(edge.day())))
        });

        let candidates = [
            Some(edge - Duration::days(1)),
            Some(edge + Duration::days(1)),
            rolled,
        ];

        let mut inward = Decimal::ZERO;
        let mut inward_date = edge;
        let mut outward = Decimal::ZERO;
        let mut outward_date = edge;

        for candidate in candidates.into_iter().flatten() {
            let bounds = if anterior {
                (candidate, end)
            } else {
                (start, candidate)
            };
            let (_, total) = match_window(state, disposal, bounds);

            let narrows = if anterior {
                candidate > edge
            } else {
                candidate < edge
            };
            let delta = if narrows {
                matched_total - total
            } else {
                total - matched_total
            };
            if delta <= Decimal::ZERO {
                continue;
            }

            let (best, best_date) = if narrows {
                (&mut inward, &mut inward_date)
            } else {
                (&mut outward, &mut outward_date)
            };
            if delta > *best {
                *best = delta;
                *best_date = candidate;
            }
        }

        for (quantity, alternative_date, unclamped_kind) in [
            (inward, inward_date, BoundaryKind::Endpoint),
            (outward, outward_date, BoundaryKind::NearMiss),
        ] {
            if quantity <= Decimal::ZERO {
                continue;
            }
            reviews.push(BoundaryReview {
                kind: if clamped {
                    BoundaryKind::Clamp
                } else {
                    unclamped_kind
                },
                boundary_date: edge,
                alternative_date,
                amount: per_share * quantity,
            });
        }
    }

    reviews
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
            let blocked_quantity = quantity * amount / total;
            let deferred_loss = amount * quantity / matched_total;

            // A lot that carries neither shares nor euros blocks nothing, and the disposal path
            // only prunes lots it has drawn down. Both products share the factor `quantity`, so
            // they underflow together once it falls below `Decimal`'s 28-digit scale — far under
            // a share, but reachable by repeated proportional re-attachment. Pushing it would put
            // a deferral of zero shares into the carry-out block a filer pastes into next year's
            // config.
            if blocked_quantity <= Decimal::ZERO && deferred_loss <= Decimal::ZERO {
                continue;
            }

            state.blocked.push(BlockedLot {
                buy_date: date,
                blocked_quantity,
                deferred_loss,
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

    /// Venues covered by an in-force equivalence decision take the two-month limb; everything else
    /// is unsettled, including a statement that names no venue at all.
    #[rstest]
    #[case(&["NASDAQ"], Some(true))]
    #[case(&["NYSE"], Some(true))]
    // EEA venues are regulated markets under Directive 2014/65 itself.
    #[case(&["IBIS"], Some(true))]
    // Switzerland's equivalence decisions lapsed on 30-06-2019 and were never renewed.
    #[case(&["EBS"], Some(false))]
    // The United Kingdom has no decision in force post-Brexit.
    #[case(&["LSE"], Some(false))]
    // A dual listing is only settled if every venue is.
    #[case(&["NASDAQ", "EBS"], Some(false))]
    #[case(&[], None)]
    fn venue_equivalence_follows_the_commission_decisions(
        #[case] venues: &[&str],
        #[case] expected: Option<bool>,
    ) {
        assert_eq!(
            venue_takes_the_two_month_window(venues.iter().copied()),
            expected
        );
    }

    /// A repurchase outside the two months but inside the year defers nothing, and the engine says
    /// how much the one-year limb would defer instead. A repurchase inside the two months is already
    /// deferred, so there is nothing left for the wider window to add.
    #[rstest]
    // Three months after the sale: outside the two-month window, inside the year.
    #[case(date!(2026, 6, 10), dec!(0), dec!(900))]
    // One month after the sale: deferred by the window the tool applies.
    #[case(date!(2026, 4, 10), dec!(900), dec!(0))]
    // Thirteen months after the sale: outside both windows.
    #[case(date!(2027, 4, 10), dec!(0), dec!(0))]
    fn the_wider_window_is_measured_but_never_applied(
        #[case] repurchase: Date,
        #[case] expected_deferred: Decimal,
        #[case] expected_wider: Decimal,
    ) {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(repurchase, dec!(100)),
            ],
            [],
        );

        let outcome = engine.process(&disposal(
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));

        assert_eq!(outcome.deferred_loss, expected_deferred);
        assert_eq!(outcome.wider_window_loss, expected_wider);
    }

    /// A deferral that hangs on a window edge says so; one decided well inside the window does not.
    ///
    /// Sale of 100 shares on 2026-03-10 at a €900 loss, window 2026-01-10 to 2026-05-10.
    #[rstest]
    // Exactly on the posterior terminal day: deferred, and deductible if that ordinal were excluded.
    #[case(date!(2026, 5, 10), Some((BoundaryKind::Endpoint, date!(2026, 5, 10), dec!(900))))]
    // Exactly on the anterior terminal day, same question at the other edge.
    #[case(date!(2026, 1, 10), Some((BoundaryKind::Endpoint, date!(2026, 1, 10), dec!(900))))]
    // One day past the posterior edge: nothing is deferred, and €900 would be under a wider reading.
    #[case(date!(2026, 5, 11), Some((BoundaryKind::NearMiss, date!(2026, 5, 10), dec!(900))))]
    // Well inside the window: the deferral does not turn on either edge.
    #[case(date!(2026, 4, 20), None)]
    // Well outside it, in either direction.
    #[case(date!(2026, 8, 20), None)]
    fn a_deferral_that_turns_on_a_window_edge_is_reported(
        #[case] acquired: Date,
        #[case] expected: Option<(BoundaryKind, Date, Decimal)>,
    ) {
        let mut engine = WashSaleEngine::new([acquisition(acquired, dec!(100))], []);
        let outcome = engine.process(&disposal(date!(2026, 3, 10), dec!(100), dec!(-900), &[]));

        match expected {
            Some((kind, boundary_date, amount)) => {
                assert_eq!(outcome.boundary_reviews.len(), 1);
                let review = &outcome.boundary_reviews[0];
                assert_eq!(review.kind, kind);
                assert_eq!(review.boundary_date, boundary_date);
                assert_eq!(review.amount, amount);
            }
            None => assert!(outcome.boundary_reviews.is_empty()),
        }
    }

    /// A clamped edge is reported as the construct it is: a 31 December sale has no 31 February to
    /// reach forward to, so the window ends on the 28th and a repurchase there is blocked by an edge
    /// the statute never names.
    #[test]
    fn a_month_end_clamp_is_reported_as_a_clamp() {
        let mut engine = WashSaleEngine::new([acquisition(date!(2025, 2, 28), dec!(100))], []);
        let outcome = engine.process(&disposal(date!(2024, 12, 31), dec!(100), dec!(-900), &[]));

        assert_eq!(outcome.deferred_loss, dec!(900));
        assert_eq!(outcome.boundary_reviews.len(), 1);
        let review = &outcome.boundary_reviews[0];
        assert_eq!(review.kind, BoundaryKind::Clamp);
        assert_eq!(review.boundary_date, date!(2025, 2, 28));
        assert_eq!(review.amount, dec!(900));
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
        Acquisition {
            key: KEY.to_owned(),
            date,
            quantity,
        }
    }

    fn disposal(
        date: Date,
        quantity: Decimal,
        result: Decimal,
        consumed: &[(Date, Decimal)],
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
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));

        assert_eq!(outcome.deferred_loss, dec!(360));
        assert!(outcome.reintegrations.is_empty());

        let blocked: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(
            blocked,
            vec![&BlockedLot {
                buy_date: date!(2026, 4, 20),
                blocked_quantity: dec!(40),
                deferred_loss: dec!(360),
                origin_sale_date: date!(2026, 3, 10),
            }]
        );
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
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 10), dec!(100))],
        ));

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
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));

        // 25 of the 40 blocked shares go; 360 × 25/40 becomes integrable again.
        let outcome = engine.process(&disposal(
            date!(2026, 11, 15),
            dec!(25),
            dec!(450),
            &[(date!(2026, 4, 20), dec!(25))],
        ));

        assert_eq!(outcome.reintegrations.len(), 1);
        assert_eq!(outcome.reintegrations[0].amount, dec!(225));
        // Labelled with the sale the deferral came from, not the sale that released it.
        assert_eq!(
            outcome.reintegrations[0].origin_sale_date,
            date!(2026, 3, 10)
        );

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
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));
        assert_eq!(deferral.deferred_loss, dec!(360));

        let blocked: Vec<Decimal> = engine
            .blocked_lots()
            .map(|(_, lot)| lot.deferred_loss)
            .collect();
        assert_eq!(blocked, vec![dec!(225), dec!(135)]);

        // Sell 30: the whole 25-share lot and 5 of the 15-share one.
        let release = engine.process(&disposal(
            date!(2026, 11, 15),
            dec!(30),
            dec!(630),
            &[(date!(2026, 4, 20), dec!(25)), (date!(2026, 5, 5), dec!(5))],
        ));

        let released: Vec<Decimal> = release
            .reintegrations
            .iter()
            .map(|entry| entry.amount)
            .collect();
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
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));

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
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));
        // The 2026-02-01 and 2026-04-20 acquisitions are both in the window: 100 + 60 available,
        // capped at the 100 shares sold.
        assert_eq!(first.deferred_loss, dec!(900));

        let second = engine.process(&disposal(
            date!(2026, 3, 20),
            dec!(100),
            dec!(-500),
            &[(date!(2026, 2, 1), dec!(100))],
        ));
        // Selling the blocking shares releases the whole first deferral, but only 40 of the 100
        // shares left the estate for good: 900 × 40% = 360 becomes integrable.
        assert_eq!(second.reintegrations.len(), 1);
        assert_eq!(second.reintegrations[0].amount, dec!(360));
        assert_eq!(
            second.reintegrations[0].origin_sale_date,
            date!(2026, 3, 10)
        );
        // Only the 60 shares bought on 2026-04-20 are free to block this loss, so 500 × 60/100
        // defers. The shares it just freed are gone, not available to itself.
        assert_eq!(second.deferred_loss, dec!(300));

        // The 60 shares now carry both deferrals: this sale's 300 and the re-attached 540.
        let blocked: Decimal = engine
            .blocked_lots()
            .map(|(_, lot)| lot.deferred_loss)
            .sum();
        assert_eq!(blocked, dec!(840));
        let blocked_quantity: Decimal = engine
            .blocked_lots()
            .map(|(_, lot)| lot.blocked_quantity)
            .sum();
        assert_eq!(
            blocked_quantity,
            dec!(60),
            "one blocked share per matched share"
        );
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
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));

        // Sells every blocking share, but buys 40 back inside the window: nothing is integrable.
        let chained = engine.process(&disposal(
            date!(2026, 8, 10),
            dec!(40),
            dec!(720),
            &[(date!(2026, 4, 20), dec!(40))],
        ));
        assert!(chained.reintegrations.is_empty());

        let blocked: Vec<&BlockedLot> = engine.blocked_lots().map(|(_, lot)| lot).collect();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].buy_date, date!(2026, 9, 15));
        assert_eq!(blocked[0].deferred_loss, dec!(360));
        // Still labelled with the sale it originally came from, not the one that moved it.
        assert_eq!(blocked[0].origin_sale_date, date!(2026, 3, 10));

        // Nothing bought back this time, so the transfer is definitive.
        let definitive = engine.process(&disposal(
            date!(2026, 12, 20),
            dec!(40),
            dec!(720),
            &[(date!(2026, 9, 15), dec!(40))],
        ));
        assert_eq!(definitive.reintegrations.len(), 1);
        assert_eq!(definitive.reintegrations[0].amount, dec!(360));
        assert_eq!(
            definitive.reintegrations[0].origin_sale_date,
            date!(2026, 3, 10)
        );
        assert_eq!(engine.blocked_lots().count(), 0);
    }

    /// Definitiveness is measured on the blocked shares, not on the whole disposal.
    ///
    /// Sell 40 blocked shares together with 60 unblocked ones and buy 40 back inside the window:
    /// every share that was blocking has been replaced, so nothing left the estate for good and the
    /// whole €360 moves onto the new shares. Scaling by the disposal's own matched fraction
    /// (40/100) would integrate €216 of a deferral that is still fully blocked.
    ///
    /// Buying only 20 back replaces half the blocked shares, so half is re-attached and half becomes
    /// integrable.
    #[rstest]
    #[case(dec!(40), dec!(0), dec!(360))]
    #[case(dec!(20), dec!(180), dec!(180))]
    fn definitiveness_is_measured_on_the_blocked_shares(
        #[case] repurchased: Decimal,
        #[case] expected_integrable: Decimal,
        #[case] expected_re_attached: Decimal,
    ) {
        let mut engine = WashSaleEngine::new(
            [
                acquisition(date!(2026, 1, 5), dec!(100)),
                acquisition(date!(2026, 4, 20), dec!(40)),
                // Outside the first sale's window, so these 60 shares never block anything.
                acquisition(date!(2026, 6, 1), dec!(60)),
                acquisition(date!(2026, 10, 1), repurchased),
            ],
            [],
        );

        let first = engine.process(&disposal(
            date!(2026, 3, 10),
            dec!(100),
            dec!(-900),
            &[(date!(2026, 1, 5), dec!(100))],
        ));
        assert_eq!(first.deferred_loss, dec!(360));

        // 40 blocked shares and 60 unblocked ones go together, at a gain, so only the release is in
        // play.
        let mixed = engine.process(&disposal(
            date!(2026, 9, 10),
            dec!(100),
            dec!(1000),
            &[
                (date!(2026, 4, 20), dec!(40)),
                (date!(2026, 6, 1), dec!(60)),
            ],
        ));

        let integrable: Decimal = mixed.reintegrations.iter().map(|entry| entry.amount).sum();
        assert_eq!(integrable, expected_integrable);
        assert_eq!(mixed.deferred_loss, dec!(0));

        let re_attached: Decimal = engine
            .blocked_lots()
            .map(|(_, lot)| lot.deferred_loss)
            .sum();
        assert_eq!(re_attached, expected_re_attached);
        // Whatever moved on is still labelled with the sale it came from.
        for (_, lot) in engine.blocked_lots() {
            assert_eq!(lot.origin_sale_date, date!(2026, 3, 10));
            assert_eq!(lot.buy_date, date!(2026, 10, 1));
        }
        // Nothing is created or lost: what was integrated plus what moved on is the whole deferral.
        assert_eq!(integrable + re_attached, dec!(360));
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
            date!(2026, 5, 10),
            dec!(15),
            dec!(100),
            &[(date!(2025, 12, 20), dec!(15))],
        ));

        assert_eq!(outcome.reintegrations[0].amount, dec!(420));
        assert_eq!(engine.blocked_lots().count(), 0);
    }
}

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

use chrono::Months;

use crate::broker_statement::{StockBuy, StockSource};
use crate::instruments::InstrumentInfo;
use crate::types::Date;

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

/// Whether an acquisition on `acquisition_date` falls inside the sale's window.
pub fn in_window(sale_date: Date, acquisition_date: Date) -> bool {
    let (start, end) = window(sale_date);
    start <= acquisition_date && acquisition_date <= end
}

/// Identity for "valores homogéneos": the ISIN when the statement carries one, the ticker otherwise.
///
/// The ISIN is preferred because tickers get reused and renamed, and two lines of the same issue
/// under different tickers are still homogeneous securities. This is narrower than the statutory
/// definition — homogeneity also covers different issues of the same issuer with the same rights —
/// but a broker statement carries nothing that would let the tool decide that.
// TODO(verify): NF 3/2014 art. 43 defers to the RD 1704/1999 / RIRPF definition of "valores
// homogéneos", which reaches beyond a single ISIN. Same-ISIN is the only test a Flex statement
// supports; a filer holding several homogeneous issues must check those by hand.
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

    /// Both endpoints are inside the window, and the days either side of them are not.
    #[rstest]
    #[case(date!(2026, 3, 15), true)]
    #[case(date!(2026, 1, 15), true)]
    #[case(date!(2026, 1, 14), false)]
    #[case(date!(2026, 5, 15), true)]
    #[case(date!(2026, 5, 16), false)]
    fn window_endpoints_are_inclusive(#[case] acquisition: Date, #[case] expected: bool) {
        assert_eq!(in_window(date!(2026, 3, 15), acquisition), expected);
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
}

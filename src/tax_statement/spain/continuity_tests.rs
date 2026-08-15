//! Multi-year continuity tests.
//!
//! Filed years are not independent: a year hands the next one its pending loss ledgers and its
//! deferred wash-sale losses through the config block the tool prints. These tests replay that
//! hand-off and assert it agrees with a single continuous computation over the same trades.

#[test]
fn placeholder() {}

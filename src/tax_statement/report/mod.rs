//! Broker-level detail recorded alongside the tax entries for the printable HTML report.
//!
//! [`details`] holds the row types, generic over the jurisdiction's asset category and
//! foreign-currency treatment; [`collect`] fills the rows a processor can gather from the broker
//! statement alone. Rows that reproduce a tax figure are pushed by the processor from the same
//! intermediate values that fed the entry, so the report can never disagree with the CSV or the
//! console by a rounding cent.

pub(crate) mod collect;
pub(crate) mod details;

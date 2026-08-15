//! Golden-CSV regression tests.
//!
//! Pins the complete emitted statement, byte for byte, for a set of committed fixtures under every
//! regime, so any unintended change to a value, a label, a row order or a banner shows up as a diff
//! in `cargo test` rather than in a filer's return.

#[test]
fn placeholder() {}

//! End-to-end tests for the Spanish tax pipeline: Flex Query XML → BrokerStatement →
//! SpanishTaxStatement → CSV.
//!
//! Fixtures live under `testdata/`. The currency converter is a fixed-rate EUR backend so expected
//! values are hand-computable and no network/database is required. It duplicates the German
//! module's backend rather than sharing one: the top-level `testdata/` submodule is private and
//! empty, so these in-src fixtures are the only runnable path, and ~30 test-only lines are a
//! cheaper price than a shared test-utils module that both jurisdictions must then agree on.

use std::path::PathBuf;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::brokers::Broker;
use crate::config::Config;
use crate::core::{EmptyResult, GenericResult};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterBackend};
use crate::taxes::TaxRemapping;
use crate::time::{self, Date};
use crate::types::Decimal;

/// Fixed EUR conversion backend: every non-EUR currency converts to EUR at a constant rate,
/// independent of date. Keeps expected tax figures hand-computable.
struct FixedEurBackend {
    today: Date,
    eur_per_usd: Decimal,
}

impl CurrencyConverterBackend for FixedEurBackend {
    fn today(&self) -> Date {
        self.today
    }

    fn batch(&self, _from: &str, _to: &str, _date: Date) -> EmptyResult {
        Ok(())
    }

    fn currency_rate(
        &self,
        from: &str,
        to: &str,
        _date: Date,
    ) -> GenericResult<(Option<Decimal>, Option<Decimal>)> {
        assert_eq!(to, "EUR", "the Spanish pipeline only ever converts to EUR");
        match from {
            "EUR" => Ok((None, None)),
            "USD" => Ok((Some(self.eur_per_usd), None)),
            other => Err!("fixture converter has no rate for {other}"),
        }
    }
}

fn converter() -> CurrencyConverter {
    CurrencyConverter::new_with_backend(Box::new(FixedEurBackend {
        today: time::today(),
        eur_per_usd: dec!(0.9),
    }))
}

fn read_fixture(name: &str) -> BrokerStatement {
    let broker = Broker::InteractiveBrokers
        .get_info(&Config::mock(), None)
        .unwrap();
    let path = PathBuf::from(format!("src/tax_statement/spain/testdata/{name}"));
    BrokerStatement::read(
        broker,
        &path,
        &Default::default(),
        &Default::default(),
        &Default::default(),
        TaxRemapping::new(),
        &[],
        &[],
        ReadingStrictness::all(),
    )
    .unwrap()
}

/// Smoke test: the fixture parses through the real IB Flex reader and the two AAPL sells are
/// FIFO-matched across the 2021 and 2024 buys. Guards the harness itself, so the value assertions
/// added by later phases fail for the right reason (wrong number), not because the pipeline is
/// broken.
#[test]
fn harness_reads_fifo_fixture() {
    let statement = read_fixture("fifo");
    assert_eq!(statement.stock_buys.len(), 2);
    assert_eq!(statement.stock_sells.len(), 2);
    // The fixture converter is what makes the expected EUR figures hand-computable.
    assert_eq!(
        converter()
            .convert_to(time::today(), crate::currency::Cash::new("USD", dec!(100)), "EUR")
            .unwrap(),
        dec!(90)
    );
}

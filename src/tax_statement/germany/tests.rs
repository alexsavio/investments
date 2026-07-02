//! End-to-end tests for the German tax pipeline: Flex Query XML → BrokerStatement →
//! GermanTaxStatement → CSV. These exercise the real code paths (FIFO cost basis, Flex
//! Query ingestion, currency conversion) that the per-module unit tests never touch.
//!
//! Fixtures live under `testdata/germany/`. The currency converter is a fixed-rate EUR
//! backend so expected values are hand-computable and no network/database is required.

use std::path::PathBuf;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::brokers::Broker;
use crate::config::Config;
use crate::core::{EmptyResult, GenericResult};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterBackend};
use crate::taxes::{TaxConfig, TaxRemapping};
use crate::time::{self, Date};
use crate::types::Decimal;

use super::{GermanTaxStatement, process_broker_statement};

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
        assert_eq!(to, "EUR", "the German pipeline only ever converts to EUR");
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
    let path = PathBuf::from(format!("src/tax_statement/germany/testdata/{name}"));
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

/// Run the full German tax pipeline over a fixture and return the finalized statement.
fn run_pipeline(fixture: &str, year: i32) -> GermanTaxStatement {
    let statement = read_fixture(fixture);
    let converter = converter();
    let tax_config = TaxConfig::default();

    let mut german = GermanTaxStatement::new(year, dec!(0), dec!(0));
    process_broker_statement(&mut german, &statement, year, &converter, &tax_config).unwrap();
    german.calculate_totals();
    german
}

/// Smoke test: the fixture parses through the real IB Flex reader and the two AAPL sells are
/// FIFO-matched. Guards the harness itself so the value assertions below fail for the right
/// reason (wrong number), not because the pipeline is broken.
#[test]
fn harness_reads_fifo_fixture() {
    let statement = read_fixture("fifo");
    assert_eq!(statement.stock_buys.len(), 2);
    assert_eq!(statement.stock_sells.len(), 2);
}

/// FIFO cost basis must consume lots. AAPL: buy 100@$10 (Jan), buy 100@$20 (Feb), sell 100
/// (Mar, $15), sell 100 (Apr, $25). At 0.9 EUR/USD each sale nets €450, total €900. The
/// broken `calculate_cost_basis` re-matches both sales against the €10 lot, reporting the
/// April cost as €900 instead of €1800 and inflating the total gain to €1800.
///
/// Enabled by T2 (per-lot FIFO via `StockSell::calculate`).
#[test]
#[ignore = "enabled by T2: per-lot FIFO cost basis"]
fn fifo_cost_basis_consumes_lots() {
    let german = run_pipeline("fifo", 2024);
    assert_eq!(german.total_capital_gains, dec!(900));
    assert_eq!(german.total_capital_losses, dec!(0));
}

// The IB Flex parser's income-dedup and edge-row handling (T3) is verified next to the parser
// itself in `broker_statement::ib::flex_query` (the `FlexQueryResponse` type is private to that
// module), against the shared `income_edge` fixture in this module's `testdata/` directory.

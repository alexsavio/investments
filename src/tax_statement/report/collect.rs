//! Report rows a processor can gather from the broker statement alone: the year's raw buys, the
//! lots still open, the security overview and the foreign-currency ledger.
//!
//! Everything here is report-only detail with no tax effect. Rows that reproduce a tax figure are
//! pushed by the jurisdiction's processor from the values that fed the entry.

use std::collections::BTreeSet;

use chrono::Datelike;

use crate::broker_statement::{BrokerStatement, StockSource};
use crate::core::GenericResult;
use crate::currency::converter::CurrencyConverter;
use crate::tax_statement::fx_fifo::{CurrencyFxResult, FxLedgerKind, FxLedgerRow};
use crate::time::Date;
use crate::types::Decimal;

use super::details::{
    FxRow, LotSource, OpenLotRow, ReportDetails, SecurityRow, TradeRow, TradeSide, ecb_rate,
    isin_country, security_identity,
};

/// Record the year's raw purchases (sells are recorded by the processor, from the sale worksheet).
pub(crate) fn collect_buys<C, T>(
    report: &mut ReportDetails<C, T>,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
) -> GenericResult<()> {
    for buy in &broker_statement.stock_buys {
        if buy.conclusion_time.date.year() != year {
            continue;
        }
        let StockSource::Trade {
            price,
            volume,
            commission,
        } = buy.type_
        else {
            continue;
        };

        let (isin, name) = security_identity(broker_statement, &buy.symbol);
        // Same convention as the FIFO engine: volume converted at settlement, commission at
        // conclusion, each rounded to cents.
        let cost_eur = buy.total_cost("EUR", converter)?.amount;

        report.trades.push(TradeRow {
            date: buy.conclusion_time.date,
            settle_date: buy.execution_date,
            trade_id: buy.trade_id.clone(),
            symbol: buy.symbol.clone(),
            isin,
            name,
            side: TradeSide::Buy,
            quantity: buy.quantity,
            currency: price.currency.to_string(),
            price: price.amount,
            gross: -volume.amount,
            commission: -commission.amount,
            net: -(volume.amount + commission.amount),
            eur_per_unit: ecb_rate(converter, buy.execution_date, price.currency)?,
            amount_eur: -cost_eur,
        });
    }
    Ok(())
}

/// Record the purchase lots still (partly) unsold for the report's open-positions section.
///
/// The unsold quantity is the FIFO engine's view at the statement's last date; the as-of date
/// recorded alongside is the tax year end, or the statement end when the statement ends earlier.
/// A statement that extends past the tax year already has later sales consumed from these lots,
/// which the renderer points out.
///
/// `classify` maps an ISIN to the jurisdiction's asset category; `grant_cost_eur` supplies the
/// cost basis of a vested-grant lot from its original symbol, vest date and quantity.
pub(crate) fn collect_open_lots<C, T>(
    report: &mut ReportDetails<C, T>,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
    classify: &dyn Fn(&str) -> C,
    grant_cost_eur: &dyn Fn(&str, Date, Decimal) -> GenericResult<Decimal>,
) -> GenericResult<()> {
    let year_end = Date::from_ymd_opt(year, 12, 31).expect("31 December is always a valid date");
    report.open_lots_as_of = Some(broker_statement.period.last_date().min(year_end));

    for buy in &broker_statement.stock_buys {
        let unsold = buy.get_unsold();
        if unsold <= dec!(0) || buy.conclusion_time.date > year_end {
            continue;
        }

        let (isin, name) = security_identity(broker_statement, &buy.symbol);
        let category = classify(&isin);

        let (source, currency, price) = match buy.type_ {
            StockSource::Trade { price, .. } => (
                LotSource::Trade,
                price.currency.to_string(),
                Some(price.amount),
            ),
            StockSource::Grant => (LotSource::Grant, String::new(), None),
            StockSource::CorporateAction => (LotSource::CorporateAction, String::new(), None),
        };

        let cost_eur = match buy.type_ {
            StockSource::Grant => {
                grant_cost_eur(&buy.original_symbol, buy.conclusion_time.date, unsold)?
            }
            _ => {
                let total_cost_eur = buy.total_cost("EUR", converter)?.amount;
                if buy.quantity.is_zero() {
                    dec!(0)
                } else {
                    total_cost_eur * unsold / buy.quantity
                }
            }
        };

        report.open_lots.push(OpenLotRow {
            symbol: buy.symbol.clone(),
            isin,
            name,
            category,
            open_date: buy.conclusion_time.date,
            trade_id: buy.trade_id.clone(),
            source,
            quantity: unsold,
            currency,
            price,
            cost_eur,
        });
    }

    report
        .open_lots
        .sort_by(|a, b| (&a.symbol, a.open_date).cmp(&(&b.symbol, b.open_date)));
    Ok(())
}

/// Build the security overview from every symbol the report mentions, plus `extra_symbols` for the
/// entries a jurisdiction shows without a trade or a booking behind them.
pub(crate) fn collect_securities<C, T>(
    report: &mut ReportDetails<C, T>,
    broker_statement: &BrokerStatement,
    extra_symbols: &[&str],
    classify: &dyn Fn(&str) -> C,
) {
    let mut symbols: BTreeSet<&str> = BTreeSet::new();
    symbols.extend(report.trades.iter().map(|row| row.symbol.as_str()));
    symbols.extend(report.sales.iter().map(|row| row.symbol.as_str()));
    symbols.extend(report.open_lots.iter().map(|row| row.symbol.as_str()));
    symbols.extend(
        report
            .bookings
            .iter()
            .filter(|row| !row.symbol.is_empty())
            .map(|row| row.symbol.as_str()),
    );
    symbols.extend(extra_symbols.iter().copied());

    let mut securities = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        let (isin, name) = security_identity(broker_statement, symbol);
        let currency = report
            .trades
            .iter()
            .find(|row| row.symbol == symbol)
            .map(|row| row.currency.clone())
            .or_else(|| {
                report
                    .bookings
                    .iter()
                    .find(|row| row.symbol == symbol)
                    .map(|row| row.currency.clone())
            })
            .unwrap_or_default();

        securities.push(SecurityRow {
            symbol: symbol.to_owned(),
            country_code: isin_country(&isin),
            category: classify(&isin),
            isin,
            name,
            currency,
        });
    }
    report.securities = securities;
}

/// Record every foreign-currency movement portion of the year, labelled with the treatment the
/// jurisdiction's routing gave it.
///
/// `treatment_of` returns the treatment and the realized result for a row that realized one, and
/// `None` for a pure acquisition. It also owns the result's precision: a jurisdiction that rounds
/// a realization into its tax entry must round it here too, so the row carries the filed figure.
pub(crate) fn collect_fx_rows<C, T>(
    report: &mut ReportDetails<C, T>,
    results: &[CurrencyFxResult],
    year: i32,
    treatment_of: &dyn Fn(&FxLedgerRow) -> Option<(T, Decimal)>,
) {
    for result in results {
        for row in &result.ledger {
            if row.date.year() != year {
                continue;
            }

            let opening = match row.kind {
                FxLedgerKind::Acquisition => None,
                FxLedgerKind::Disposal {
                    acquisition_date,
                    acquisition_rate,
                    ..
                }
                | FxLedgerKind::Repayment {
                    acquisition_date,
                    acquisition_rate,
                    ..
                } => Some((acquisition_date, acquisition_rate)),
            };
            let (treatment, gain_loss_eur) = match treatment_of(row) {
                Some((treatment, amount)) => (Some(treatment), Some(amount)),
                None => (None, None),
            };

            report.fx_rows.push(FxRow {
                currency: result.currency.clone(),
                date: row.date,
                transaction_id: row.transaction_id.clone(),
                activity_code: row.activity_code.clone(),
                units: row.units,
                eur_per_unit: row.rate,
                amount_eur: row.units * row.rate,
                open_date: opening.map(|(date, _)| date),
                open_eur_per_unit: opening.map(|(_, rate)| rate),
                open_value_eur: opening.map(|(_, rate)| row.units.abs() * rate),
                gain_loss_eur,
                balance_after: row.balance_after,
                holding_days: opening.map(|(date, _)| (row.date - date).num_days()),
                treatment,
            });
        }
    }
}

// IB Flex Query XML format parser
//
// This module parses the XML format exported from Interactive Brokers Flex Queries.
// The XML format contains more detailed information than the CSV Activity Statements.

use serde::Deserialize;

use std::collections::{HashMap, HashSet};

use crate::broker_statement::grants::StockGrant;
use crate::broker_statement::interest::{IdleCashInterest, InterestKind, ForeignCashFlow};
use crate::broker_statement::partial::PartialBrokerStatement;
use crate::broker_statement::trades::{StockBuy, StockSell};
use crate::broker_statement::{Fee, Withholding};
use crate::taxes::TaxRemapping;
use crate::broker_statement::corporate_actions::{
    CorporateAction as DomainCorporateAction, CorporateActionType as DomainCorporateActionType,
};

use super::corporate_actions;
use crate::core::{EmptyResult, GenericResult};
use crate::currency::{Cash, CashAssets};
use crate::exchanges::Exchange;
use crate::formats::xml;
use crate::instruments::{InstrumentId, parse_isin};
use crate::time::{Date, DateOptTime, Period};
use crate::types::Decimal;

/// Root element of Flex Query response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlexQueryResponse {
    #[serde(rename = "FlexStatements")]
    pub flex_statements: FlexStatements,
}

#[derive(Debug, Deserialize)]
pub struct FlexStatements {
    #[serde(rename = "FlexStatement", default)]
    pub statements: Vec<FlexStatement>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Fields used for parsing, not all are read after
pub struct FlexStatement {
    #[serde(rename = "@accountId")]
    pub account_id: String,

    #[serde(rename = "@fromDate")]
    pub from_date: String,

    #[serde(rename = "@toDate")]
    pub to_date: String,

    #[serde(rename = "CashReport")]
    pub cash_report: Option<CashReport>,

    #[serde(rename = "StmtFunds")]
    pub statement_of_funds: Option<StatementOfFunds>,

    #[serde(rename = "Trades")]
    pub trades: Option<Trades>,

    #[serde(rename = "CashTransactions")]
    pub cash_transactions: Option<CashTransactions>,

    #[serde(rename = "CorporateActions")]
    pub corporate_actions: Option<CorporateActions>,

    #[serde(rename = "OpenPositions")]
    pub open_positions: Option<OpenPositions>,

    #[serde(rename = "SecuritiesInfo")]
    pub securities_info: Option<SecuritiesInfo>,

    #[serde(rename = "FxTransactions")]
    pub fx_transactions: Option<FxTransactions>,

    #[serde(rename = "StockGrantActivities")]
    pub stock_grant_activities: Option<StockGrantActivities>,
}

#[derive(Debug, Deserialize)]
pub struct CashReport {
    #[serde(rename = "CashReportCurrency", default)]
    pub currencies: Vec<CashReportCurrency>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Fields used for parsing, not all are read after
pub struct CashReportCurrency {
    #[serde(rename = "@currency")]
    pub currency: String,

    #[serde(rename = "@startingCash", default)]
    pub starting_cash: Decimal,

    #[serde(rename = "@endingCash")]
    pub ending_cash: Decimal,

    #[serde(rename = "@dividends")]
    pub dividends: Decimal,

    #[serde(rename = "@brokerInterest")]
    pub broker_interest: Decimal,

    #[serde(rename = "@withholdingTax")]
    pub withholding_tax: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct StatementOfFunds {
    #[serde(rename = "StatementOfFundsLine", default)]
    pub lines: Vec<StatementOfFundsLine>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementOfFundsLine {
    #[serde(rename = "@currency")]
    pub currency: String,

    #[serde(rename = "@fxRateToBase", default)]
    pub fx_rate_to_base: String,

    #[serde(rename = "@date", default)]
    pub date: String,

    #[serde(rename = "@settleDate", default)]
    pub settle_date: String,

    #[serde(rename = "@activityCode", default)]
    pub activity_code: String,

    #[serde(rename = "@activityDescription", default)]
    pub activity_description: String,

    #[serde(rename = "@symbol", default)]
    pub symbol: String,

    #[serde(rename = "@isin", default)]
    pub isin: String,

    #[serde(rename = "@assetCategory", default)]
    pub asset_category: String,

    #[serde(rename = "@amount", default)]
    pub amount: Decimal,

    #[serde(rename = "@tradeQuantity", default)]
    pub trade_quantity: Decimal,

    #[serde(rename = "@tradePrice", default)]
    pub trade_price: Decimal,

    #[serde(rename = "@tradeGross", default)]
    pub trade_gross: Decimal,

    #[serde(rename = "@tradeCommission", default)]
    pub trade_commission: Decimal,

    #[serde(rename = "@buySell", default)]
    pub buy_sell: String,

    #[serde(rename = "@levelOfDetail", default)]
    pub level_of_detail: String,

    #[serde(rename = "@balance", default)]
    pub balance: Decimal,

    #[serde(rename = "@transactionID", default)]
    pub transaction_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Trades {
    #[serde(rename = "Trade", default)]
    pub trades: Vec<Trade>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    #[serde(rename = "@currency")]
    pub currency: String,

    #[serde(rename = "@symbol")]
    pub symbol: String,

    #[serde(rename = "@isin", default)]
    pub isin: String,

    #[serde(rename = "@description")]
    pub description: String,

    #[serde(rename = "@assetCategory")]
    pub asset_category: String,

    #[serde(rename = "@dateTime")]
    pub date_time: String,

    #[serde(rename = "@settleDate", default)]
    pub settle_date: String,

    #[serde(rename = "@quantity")]
    pub quantity: Decimal,

    #[serde(rename = "@tradePrice")]
    pub trade_price: Decimal,

    #[serde(rename = "@tradeMoney")]
    pub trade_money: Decimal,

    #[serde(rename = "@ibCommission")]
    pub commission: Decimal,

    #[serde(rename = "@buySell")]
    pub buy_sell: String,

    #[serde(rename = "@openCloseIndicator", default)]
    pub open_close_indicator: String,

    #[serde(rename = "@tradeID", default)]
    pub trade_id: String,

    #[serde(rename = "@origTradeID", default)]
    pub orig_trade_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CashTransactions {
    #[serde(rename = "CashTransaction", default)]
    pub transactions: Vec<CashTransaction>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashTransaction {
    #[serde(rename = "@currency")]
    pub currency: String,

    #[serde(rename = "@symbol", default)]
    pub symbol: String,

    #[serde(rename = "@isin", default)]
    pub isin: String,

    #[serde(rename = "@description")]
    pub description: String,

    #[serde(rename = "@dateTime")]
    pub date_time: String,

    #[serde(rename = "@settleDate", default)]
    pub settle_date: String,

    #[serde(rename = "@amount")]
    pub amount: Decimal,

    #[serde(rename = "@type")]
    pub transaction_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct CorporateActions {
    #[serde(rename = "CorporateAction", default)]
    pub actions: Vec<CorporateAction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorporateAction {
    #[serde(rename = "@currency")]
    pub currency: String,

    #[serde(rename = "@assetCategory", default)]
    pub asset_category: String,

    #[serde(rename = "@reportDate", default)]
    pub report_date: String,

    #[serde(rename = "@proceeds", default)]
    pub proceeds: Decimal,

    #[serde(rename = "@symbol")]
    pub symbol: String,

    #[serde(rename = "@description")]
    pub description: String,

    #[serde(rename = "@dateTime")]
    pub date_time: String,

    #[serde(rename = "@quantity")]
    pub quantity: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct OpenPositions {
    #[serde(rename = "OpenPosition", default)]
    pub positions: Vec<OpenPosition>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPosition {
    #[serde(rename = "@currency")]
    pub currency: String,

    #[serde(rename = "@symbol")]
    pub symbol: String,

    #[serde(rename = "@isin", default)]
    pub isin: String,

    #[serde(rename = "@description")]
    pub description: String,

    #[serde(rename = "@position")]
    pub position: Decimal,

    #[serde(rename = "@markPrice")]
    pub mark_price: Decimal,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SecuritiesInfo {
    #[serde(rename = "SecurityInfo", default)]
    pub securities: Vec<SecurityInfo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityInfo {
    #[serde(rename = "@symbol")]
    pub symbol: String,

    #[serde(rename = "@isin", default)]
    pub isin: String,

    #[serde(rename = "@description")]
    pub description: String,

    #[serde(rename = "@assetCategory")]
    pub asset_category: String,
}

/// FX Transactions section - contains detailed FX P&L with realizedPL field
/// This is the preferred source for FX gains/losses as it has accurate P&L values.
#[derive(Debug, Deserialize)]
pub struct FxTransactions {
    #[serde(rename = "FxTransaction", default)]
    pub transactions: Vec<FxTransactionEntry>,
}

/// Individual FX transaction with realized P&L.
/// The `realizedPL` field contains the actual FX gain/loss for tax purposes.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxTransactionEntry {
    /// Functional currency (usually EUR for German tax residents)
    #[serde(rename = "@functionalCurrency")]
    pub functional_currency: String,

    /// The FX currency being traded (e.g., USD)
    #[serde(rename = "@fxCurrency")]
    pub fx_currency: String,

    /// Report date in YYYYMMDD format
    #[serde(rename = "@reportDate")]
    pub report_date: String,

    /// Date and time of the transaction
    #[serde(rename = "@dateTime")]
    pub date_time: String,

    /// Description of the transaction (e.g., "CASH: -4353.72 EUR.USD" or "STK: 50 STRC")
    #[serde(rename = "@activityDescription")]
    pub activity_description: String,

    /// Quantity of foreign currency involved
    #[serde(rename = "@quantity")]
    pub quantity: Decimal,

    /// The realized FX P&L in functional currency (EUR)
    /// This is the key field for German tax calculations.
    #[serde(rename = "@realizedPL")]
    pub realized_pl: Decimal,

    /// Transaction code:
    /// - "O" = Opening position (typically no realized P&L)
    /// - "C" = Closing position (has realized P&L)
    /// - "C;O" = Both closing and opening in same transaction
    #[serde(rename = "@code", default)]
    pub code: String,
}

/// Stock Grant Activities section - RSUs, stock options, ESPPs
#[derive(Debug, Deserialize)]
pub struct StockGrantActivities {
    #[serde(rename = "StockGrantActivity", default)]
    pub activities: Vec<StockGrantActivity>,
}

/// Individual stock grant activity (RSU vest, option exercise, etc.)
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockGrantActivity {
    /// Stock symbol
    #[serde(rename = "@symbol")]
    pub symbol: String,

    /// Report date (vest date) in YYYYMMDD format
    #[serde(rename = "@reportDate")]
    pub report_date: String,

    /// Number of shares granted/vested
    #[serde(rename = "@quantity")]
    pub quantity: Decimal,

    /// Grant type (e.g., "RSU", "Stock Option", "ESPP")
    #[serde(rename = "@grantType", default)]
    pub grant_type: String,

    /// Fair market value at vest
    #[serde(rename = "@fmv", default)]
    pub fmv: Decimal,

    /// Description
    #[serde(rename = "@description", default)]
    pub description: String,
}

impl FlexQueryResponse {
    pub fn parse(data: &[u8], tax_remapping: &mut TaxRemapping) -> GenericResult<PartialBrokerStatement> {
        let response: FlexQueryResponse = xml::deserialize(data)?;

        if response.flex_statements.statements.is_empty() {
            return Err!("No statements found in Flex Query response");
        }

        // Silently truncating extra accounts would drop income; require one account per query.
        if response.flex_statements.statements.len() > 1 {
            return Err!(
                "Multi-account Flex Query responses are not supported: found {} statements. \
                 Export one account per Flex Query.",
                response.flex_statements.statements.len()
            );
        }

        let statement = &response.flex_statements.statements[0];
        statement.parse(tax_remapping)
    }
}

impl FlexStatement {
    fn parse(&self, tax_remapping: &mut TaxRemapping) -> GenericResult<PartialBrokerStatement> {
        let mut statement = PartialBrokerStatement::new(
            &[Exchange::Us, Exchange::Lse, Exchange::Other],
            false,
        );

        // Parse period
        let from_date = parse_flex_date(&self.from_date)?;
        let to_date = parse_flex_date(&self.to_date)?;
        statement.set_period(Period::new(from_date, to_date)?)?;

        // Set starting assets flag - for XML we assume no starting assets
        // unless we can detect them from CashReport
        statement.set_has_starting_assets(false)?;

        // Parse cash balances from CashReport
        if let Some(ref cash_report) = self.cash_report {
            for currency_report in &cash_report.currencies {
                // IB emits a BASE_SUMMARY aggregate row (sum across currencies); depositing it would
                // double-count cash and later fail to convert a currency that does not exist.
                if currency_report.currency == "BASE_SUMMARY" || currency_report.currency.len() != 3 {
                    if currency_report.currency != "BASE_SUMMARY" {
                        log::warn!(
                            "Skipping cash report row for unexpected currency {:?}.",
                            currency_report.currency
                        );
                    }
                    continue;
                }
                let cash_assets = statement.assets.cash.get_or_insert_with(Default::default);
                let cash = Cash::new(&currency_report.currency, currency_report.ending_cash);
                cash_assets.deposit(cash);
            }
        }

        // Parse trades from Trades section (preferred if it has content).
        // Non-stock instruments are dropped with one warning each rather than one per row: a
        // statement holding hundreds of option legs would otherwise bury everything else.
        let mut skipped_instruments = HashSet::new();
        let mut trades_found = false;
        if let Some(ref trades) = self.trades {
            // A "BUY (Ca.)" / "SELL (Ca.)" row cancels an earlier trade referenced by origTradeID;
            // drop both the cancellation row and the original it voids. A cancellation whose original
            // is not in this statement (e.g. a trade cancelled in the next year's separate export) or
            // that carries no origTradeID has nothing to void here, so warn and drop only the
            // cancellation row rather than aborting the whole statement.
            let mut cancelled_ids = HashSet::new();
            for trade in &trades.trades {
                if !is_cancelled_trade(&trade.buy_sell) {
                    continue;
                }
                if trade.orig_trade_id.is_empty() {
                    log::warn!(
                        "Ignoring cancellation of {} on {} that carries no origTradeID.",
                        trade.symbol, trade.date_time
                    );
                } else if trades.trades.iter().any(|t| t.trade_id == trade.orig_trade_id) {
                    cancelled_ids.insert(trade.orig_trade_id.clone());
                } else {
                    log::warn!(
                        "Ignoring cancellation of {} on {}: original trade ID {} is not in this \
                         statement (likely cancelled in a different reporting period).",
                        trade.symbol, trade.date_time, trade.orig_trade_id
                    );
                }
            }

            for trade in &trades.trades {
                if is_cancelled_trade(&trade.buy_sell) || cancelled_ids.contains(&trade.trade_id) {
                    continue;
                }
                // Only count the section as populated once a trade is actually ingested, so a
                // section holding nothing but skipped non-stock rows still falls back to StmtFunds.
                if parse_trade(&mut statement, trade, &mut skipped_instruments)? {
                    trades_found = true;
                }
            }
        }

        // Parse trades from Statement of Funds if no trades were found in Trades section
        if !trades_found {
            if let Some(ref stmtfunds) = self.statement_of_funds {
                for line in &stmtfunds.lines {
                    parse_statement_of_funds_trade(&mut statement, line, &mut skipped_instruments)?;
                }
            }
        }

        // Parse dividends and withholding taxes from Statement of Funds.
        // The docs tell users to enable both StmtFunds and CashTransactions, so income that appears
        // in both would be double-counted. Suppress a StmtFunds income type only when the
        // CashTransactions section actually carries that same type (per-type, so a present-but-empty
        // — or fees-only — CashTransactions section never silently drops StmtFunds income).
        let cash_income = self
            .cash_transactions
            .as_ref()
            .map(CashTxIncome::from_transactions)
            .unwrap_or_default();
        if let Some(ref stmtfunds) = self.statement_of_funds {
            for line in &stmtfunds.lines {
                parse_statement_of_funds_dividend(&mut statement, line, cash_income)?;
            }

            // Capture the raw per-currency cash-flow ledger, replayed by the German FX FIFO to
            // compute Fremdwährungsgewinne (§20 EStG) instead of IB's net per-trade P&L.
            statement.foreign_cash_flows = build_foreign_cash_flows(stmtfunds)?;
        }

        // Corporate actions. Without these a post-split position keeps its pre-split share
        // count and cost basis, which silently corrupts every FIFO disposal after the split --
        // the statement's own OpenPositions check is what catches it (e.g. CRWD 10 vs 40).
        if let Some(ref actions) = self.corporate_actions {
            let mut parsed = Vec::new();

            for action in &actions.actions {
                // The Flex export uses IB's short asset codes rather than the CSV's words.
                match action.asset_category.as_str() {
                    "STK" => {},

                    // Absent, because the query doesn't export the field. Skipping on that would
                    // drop *every* corporate action and reintroduce the exact FIFO corruption
                    // this block exists to prevent -- and say "non-stock" while doing it. The
                    // fix belongs in the query, so demand it.
                    "" => return Err!(concat!(
                        "The corporate action for {} has no assetCategory, so stock actions ",
                        "can't be told from the rest and applying them blindly would corrupt ",
                        "the cost basis. Add the Asset Category field to the Corporate Actions ",
                        "section of the Flex query and re-run it."), action.symbol),

                    other => {
                        log::warn!(
                            "Skipping corporate action for non-stock instrument {} \
                             (assetCategory {}): {}",
                            action.symbol, other, action.description);
                        continue;
                    },
                }

                let report_date = if action.report_date.is_empty() {
                    None
                } else {
                    Some(parse_flex_date(&action.report_date)?)
                };

                parsed.push(corporate_actions::parse_action(&corporate_actions::ActionRecord {
                    time: parse_flex_datetime(&action.date_time)?.into(),
                    report_date,
                    description: &action.description,
                    currency: &action.currency,
                    quantity: action.quantity,
                    proceeds: action.proceeds,
                })?);
            }

            // A complex split arrives as two rows (withdrawal + deposit) and must be joined
            // before it can be applied. Mirrors CorporateActionsParser::commit.
            let mut splits = Vec::<DomainCorporateAction>::new();
            for action in parsed {
                match action.action {
                    DomainCorporateActionType::StockSplit {..} => {
                        if let Some(last) = splits.last() {
                            if action.time != last.time || action.symbol != last.symbol {
                                statement.corporate_actions.push(
                                    corporate_actions::join_stock_splits(std::mem::take(&mut splits))?);
                            }
                        }
                        splits.push(action);
                    },
                    _ => statement.corporate_actions.push(action),
                }
            }
            if !splits.is_empty() {
                statement.corporate_actions.push(corporate_actions::join_stock_splits(splits)?);
            }
        }

        // Parse cash transactions (dividends, interest, etc.)
        if let Some(ref transactions) = self.cash_transactions {
            for tx in &transactions.transactions {
                parse_cash_transaction(&mut statement, tx, tax_remapping)?;
            }
        }

        // Parse open positions to register instruments
        if let Some(ref positions) = self.open_positions {
            for pos in &positions.positions {
                if pos.symbol.is_empty() || pos.position == Decimal::ZERO {
                    continue;
                }
                // add_open_position enforces strictly-positive quantities. Short positions need
                // manual §20 EStG treatment (Termingeschäfte/Stillhalter), which this tool does not
                // compute, so record them separately for informational reporting instead of feeding
                // them into the cost-basis reconciliation.
                if pos.position < Decimal::ZERO {
                    statement.add_short_position(&pos.symbol, pos.position);
                    continue;
                }
                statement.add_open_position(&pos.symbol, pos.position)?;
            }
        }

        // Extract the functional (base) currency from FX transactions for stock-grant FMV conversion.
        let mut functional_currency: Option<String> = None;
        if let Some(ref fx_transactions) = self.fx_transactions {
            for tx in &fx_transactions.transactions {
                if functional_currency.is_none() && !tx.functional_currency.is_empty() {
                    functional_currency = Some(tx.functional_currency.clone());
                }
            }
        }

        // Parse stock grant activities (RSUs, stock options, ESPPs)
        // If we couldn't determine functional currency from FX transactions, use USD as default
        // (most common for IBKR accounts, user should verify)
        let grant_currency = functional_currency.as_deref().unwrap_or("USD");
        if let Some(ref grants) = self.stock_grant_activities {
            for grant in &grants.activities {
                parse_stock_grant(&mut statement, grant, grant_currency)?;
            }
        }

        statement.validate()
    }
}

/// Parse a trade line from Statement of Funds section
/// Name a non-stock instrument the parser dropped, once per instrument.
///
/// The parser is shared by every jurisdiction, so the message names no tax code: derivatives are out
/// of scope for the tool, not for one country's return. Repeating it per row buries the rest of the
/// output when a statement holds hundreds of option legs.
fn warn_skipped_instrument(symbol: &str, asset_category: &str, warned: &mut HashSet<String>) {
    if warned.insert(symbol.to_owned()) {
        log::warn!(
            "Skipping non-stock instrument {symbol} (assetCategory {asset_category}): the tool \
             computes taxes for stocks only, so derivatives and other categories are out of scope.");
    }
}

fn parse_statement_of_funds_trade(
    statement: &mut PartialBrokerStatement, line: &StatementOfFundsLine,
    warned: &mut HashSet<String>,
) -> EmptyResult {
    // Skip base currency summary lines (these are conversions, not actual trades)
    // Real trades have levelOfDetail="Currency", base currency summaries have "BaseCurrency"
    if line.level_of_detail == "BaseCurrency" {
        return Ok(());
    }

    // Only process BUY and SELL activities
    match line.activity_code.as_str() {
        "BUY" | "SELL" => {}
        _ => return Ok(()),
    }

    // Keep only stocks; warn on skipped derivative trade rows (FR-016).
    if line.asset_category != "STK" {
        if !line.symbol.is_empty() {
            warn_skipped_instrument(&line.symbol, &line.asset_category, warned);
        }
        return Ok(());
    }
    if line.symbol.is_empty() {
        return Ok(());
    }

    let date = if line.date.is_empty() {
        return Ok(());
    } else {
        parse_flex_date(&line.date)?
    };

    let settle_date = if line.settle_date.is_empty() {
        date
    } else {
        parse_flex_date(&line.settle_date)?
    };

    let symbol = &line.symbol;
    let quantity = line.trade_quantity.abs();
    let price = Cash::new(&line.currency, line.trade_price);
    let volume = Cash::new(&line.currency, line.trade_gross.abs());
    let commission = Cash::new(&line.currency, line.trade_commission.abs());
    let conclusion_time: DateOptTime = date.into();

    register_instrument_isin(statement, symbol, &line.isin);

    match line.buy_sell.as_str() {
        "BUY" => {
            statement.stock_buys.push(StockBuy::new_trade(
                symbol, quantity, price, volume, commission, conclusion_time, settle_date,
            ));
        }
        "SELL" => {
            statement.stock_sells.push(StockSell::new_trade(
                symbol, quantity, price, volume, commission, conclusion_time, settle_date, false,
            ));
        }
        _ => {}
    }

    Ok(())
}

/// Parse dividend, withholding tax, and interest entries from the Statement of Funds section.
/// `cash_income` records which income types the CashTransactions section already carries; those
/// types are skipped here (that section is authoritative) so income is not counted twice.
fn parse_statement_of_funds_dividend(statement: &mut PartialBrokerStatement, line: &StatementOfFundsLine, cash_income: CashTxIncome) -> EmptyResult {
    // Only process from base currency lines to avoid duplicates
    // (entries appear in both Currency and BaseCurrency detail levels)
    if line.level_of_detail != "BaseCurrency" {
        return Ok(());
    }

    let date = if line.date.is_empty() {
        return Ok(());
    } else {
        parse_flex_date(&line.date)?
    };

    match line.activity_code.as_str() {
        // Skip a type only when CashTransactions already carries it, so the same income is not
        // counted twice while StmtFunds-only income is still ingested.
        "DIV" if cash_income.dividends => {}
        "FRTAX" if cash_income.withholding => {}
        "CINT" if cash_income.interest => {}
        "DIV" => {
            // Dividend payment - requires symbol
            if line.symbol.is_empty() {
                return Ok(());
            }
            let issuer = InstrumentId::Symbol(line.symbol.clone());
            let amount = Cash::new(&line.currency, line.amount);
            statement.dividend_accruals(date, issuer, true).add(date, amount);
        }
        "FRTAX" => {
            // Foreign tax (withholding tax on dividends) - requires symbol
            if line.symbol.is_empty() {
                return Ok(());
            }
            let issuer = InstrumentId::Symbol(line.symbol.clone());
            let tax_amount = line.amount.abs();
            let tax = Cash::new(&line.currency, tax_amount);
            if line.amount.is_sign_negative() {
                // Tax withheld (negative amount = debit)
                statement.tax_accruals(date, issuer, true).add(date, tax);
            } else {
                // Tax refund (positive amount = credit)
                statement.tax_accruals(date, issuer, true).reverse(date, tax);
            }
        }
        "CINT" => {
            // Credit Interest - broker interest on cash holdings
            // Note: IB may withhold Irish tax (20%) which is NOT creditable for German taxpayers
            // Form 8-3-6 is needed for exemption/refund
            let amount = Cash::new(&line.currency, line.amount);
            statement.idle_cash_interest.push(
                IdleCashInterest::new_typed(date, amount, InterestKind::Received));
            log::debug!("Credit interest: {} on {}", amount, date);
        }
        _ => {}
    }

    Ok(())
}

/// Build the per-currency cash-flow ledger from the Statement of Funds, in document order.
///
/// Only `levelOfDetail="Currency"` lines move a real currency balance; the `BaseCurrency` summary
/// rows are IB's base-currency net view and are excluded. Each forex leg is paired with its EUR
/// counter-leg (same `transactionID`) so the German FX FIFO can value real conversions at the
/// actual execution rate rather than the ECB reference rate.
fn build_foreign_cash_flows(stmtfunds: &StatementOfFunds) -> GenericResult<Vec<ForeignCashFlow>> {
    let mut eur_legs: HashMap<&str, Decimal> = HashMap::new();
    for line in &stmtfunds.lines {
        if line.level_of_detail == "Currency"
            && line.currency == "EUR"
            && line.activity_code == "FOREX"
            && !line.transaction_id.is_empty()
        {
            // A transactionID pairs a forex leg with its EUR counter-leg. A duplicate EUR-leg id
            // (multi-account or merged statements) would silently bind the wrong execution amount to
            // a foreign leg, so refuse rather than pick the last writer.
            if eur_legs
                .insert(line.transaction_id.as_str(), line.amount)
                .is_some()
            {
                return Err(format!(
                    "Duplicate FOREX transactionID {} on the EUR leg of the Statement of Funds: \
                     the execution rate cannot be paired unambiguously (multi-account or merged \
                     statements are not supported).",
                    line.transaction_id
                )
                .into());
            }
        }
    }

    let mut flows = Vec::new();
    for line in &stmtfunds.lines {
        if line.level_of_detail != "Currency" || line.currency.len() != 3 || line.date.is_empty() {
            continue;
        }
        let eur_execution = if line.activity_code == "FOREX" {
            eur_legs.get(line.transaction_id.as_str()).copied()
        } else {
            None
        };
        flows.push(ForeignCashFlow {
            currency: line.currency.clone(),
            date: parse_flex_date(&line.date)?,
            transaction_id: line.transaction_id.clone(),
            activity_code: line.activity_code.clone(),
            amount: line.amount,
            balance: line.balance,
            eur_execution,
        });
    }
    Ok(flows)
}

/// Ingest a stock trade. Returns `true` if a trade was recorded, `false` if the row was skipped as
/// a non-stock instrument. Cancellation rows are filtered out by the caller before this is reached.
fn parse_trade(
    statement: &mut PartialBrokerStatement, trade: &Trade, warned: &mut HashSet<String>,
) -> GenericResult<bool> {
    // Keep only stocks; warn on every skipped derivative/non-stock category (FR-016). Asset-category
    // filtering here replaces the old symbol-pattern guess that dropped real tickers like GLW/WST.
    if trade.asset_category != "STK" {
        warn_skipped_instrument(&trade.symbol, &trade.asset_category, warned);
        return Ok(false);
    }

    let date = parse_flex_datetime(&trade.date_time)?;
    let settle_date = if trade.settle_date.is_empty() {
        date
    } else {
        parse_flex_date(&trade.settle_date)?
    };

    let symbol = &trade.symbol;
    let quantity: Decimal = trade.quantity.abs();
    let price = Cash::new(&trade.currency, trade.trade_price);
    let volume = Cash::new(&trade.currency, trade.trade_money.abs());
    let commission = Cash::new(&trade.currency, trade.commission.abs());
    let conclusion_time: DateOptTime = date.into();

    register_instrument_isin(statement, symbol, &trade.isin);

    match trade.buy_sell.as_str() {
        "BUY" => {
            statement.stock_buys.push(StockBuy::new_trade(
                symbol, quantity, price, volume, commission, conclusion_time, settle_date,
            ));
        }
        "SELL" => {
            statement.stock_sells.push(StockSell::new_trade(
                symbol, quantity, price, volume, commission, conclusion_time, settle_date, false,
            ));
        }
        other => {
            return Err!("Unknown trade direction: {}", other);
        }
    }

    Ok(true)
}

/// Whether an IB trade row is a cancellation (`buySell` = "BUY (Ca.)" / "SELL (Ca.)").
fn is_cancelled_trade(buy_sell: &str) -> bool {
    buy_sell.contains("(Ca.)")
}

/// Register `symbol` and link its ISIN (parsed with the strict ISIN type). Unlike keying an
/// instrument by its raw ISIN string, this keeps the symbol as the primary key and attaches the
/// ISIN, matching the CSV path. Invalid ISINs are warned about, not fatal.
fn register_instrument_isin(statement: &mut PartialBrokerStatement, symbol: &str, isin: &str) {
    if isin.is_empty() {
        return;
    }
    match parse_isin(isin) {
        Ok(isin) => {
            statement.instrument_info.get_or_add(symbol).add_isin(isin);
        }
        Err(e) => {
            log::warn!("Ignoring invalid ISIN {isin:?} for {symbol}: {e}");
        }
    }
}

/// Which income types the CashTransactions section carries. Used to dedup against the StmtFunds
/// section per type: a type is authoritative there only if at least one such row is present.
#[derive(Clone, Copy, Default)]
struct CashTxIncome {
    dividends: bool,
    withholding: bool,
    interest: bool,
}

impl CashTxIncome {
    fn from_transactions(transactions: &CashTransactions) -> CashTxIncome {
        let mut income = CashTxIncome::default();
        for tx in &transactions.transactions {
            match tx.transaction_type.as_str() {
                "Dividends" | "Payment In Lieu Of Dividends" => income.dividends = true,
                "Withholding Tax" => income.withholding = true,
                "Broker Interest Paid" | "Broker Interest Received" => income.interest = true,
                _ => {}
            }
        }
        income
    }
}

fn parse_cash_transaction(
    statement: &mut PartialBrokerStatement, tx: &CashTransaction, tax_remapping: &mut TaxRemapping,
) -> EmptyResult {
    let date = parse_flex_datetime(&tx.date_time)?;
    let amount = Cash::new(&tx.currency, tx.amount);

    match tx.transaction_type.as_str() {
        "Dividends" | "Payment In Lieu Of Dividends" => {
            if tx.symbol.is_empty() {
                return Err!("Dividend without symbol: {:?}", tx.description);
            }

            // Track dividend for later tax matching. A negative amount is a correction/reversal of
            // an earlier accrual (Payments::add would panic on it), so route it through reverse.
            let issuer = InstrumentId::Symbol(tx.symbol.clone());
            let accruals = statement.dividend_accruals(date, issuer, true);
            if tx.amount.is_sign_negative() {
                accruals.reverse(date, -amount);
            } else {
                accruals.add(date, amount);
            }
        }

        "Withholding Tax" => {
            // Tax on dividends - will be matched by the tax accrual system.
            //
            // IB sometimes dates a withholding entry differently from the dividend it belongs to
            // (reclassifications and refunds land in a later statement), which leaves the tax
            // unmatched and aborts the whole statement. `tax_remapping` is the configured escape
            // hatch for that; the CSV reader has always honoured it, so honour it here too rather
            // than telling XML users to write rules that nothing consumes.
            //
            // Remap inside the guard: a symbol-less row is dropped below, and consuming a rule
            // for it would let `ensure_all_mapping_rules_are_used` report a rule as applied that
            // never reached an accrual.
            if !tx.symbol.is_empty() {
                let date = tax_remapping.map(date, &tx.description);
                let issuer = InstrumentId::Symbol(tx.symbol.clone());
                let tax_amount = tx.amount.abs();
                let tax = Cash::new(&tx.currency, tax_amount);
                if tx.amount.is_sign_negative() {
                    statement.tax_accruals(date, issuer, true).add(date, tax);
                } else {
                    // Tax refund
                    statement.tax_accruals(date, issuer, true).reverse(date, tax);
                }
            }
            // Skip interest withholding taxes (handled differently)
        }

        kind @ ("Broker Interest Paid" | "Broker Interest Received") => {
            // Keep IB's own label: a negative "Received" row is a reversal of interest credited
            // earlier, not a financing cost, and the sign cannot tell the two apart.
            let kind = if kind == "Broker Interest Paid" {
                InterestKind::Paid
            } else {
                InterestKind::Received
            };
            statement.idle_cash_interest.push(IdleCashInterest::new_typed(date, amount, kind));
        }

        "Deposits/Withdrawals" | "Deposits" | "Withdrawals" => {
            // Track deposits/withdrawals
            statement.deposits_and_withdrawals.push(CashAssets::new_from_cash(date, amount));
        }

        "Other Fees" | "Commission Adjustments" => {
            // Broker fees (e.g. ADR fees) arrive as a debit (negative amount); store them as a
            // deductible fee the same way the CSV path does so the fee section is not a no-op.
            let description = (!tx.description.is_empty()).then(|| tx.description.clone());
            statement.fees.push(Fee::new(date, Withholding::new(-amount), description));
        }

        _ => {
            log::debug!("Unknown cash transaction type: {} - {}", tx.transaction_type, tx.description);
        }
    }

    Ok(())
}

/// Parse a stock grant activity (RSU vest, stock option exercise, ESPP purchase).
///
/// For German tax purposes:
/// - RSU vesting is taxed as employment income (Arbeitslohn) at vest date
/// - The FMV at vest becomes the cost basis for future capital gains calculations
/// - This tool tracks the grant for cost basis; employment income is typically on payslip
fn parse_stock_grant(statement: &mut PartialBrokerStatement, grant: &StockGrantActivity, functional_currency: &str) -> EmptyResult {
    if grant.symbol.is_empty() || grant.quantity == Decimal::ZERO {
        return Ok(());
    }

    let date = parse_flex_date(&grant.report_date)?;

    // Use FMV if available (non-zero), otherwise create grant without FMV
    if grant.fmv != Decimal::ZERO {
        let fmv_cash = Cash::new(functional_currency, grant.fmv);
        statement.stock_grants.push(StockGrant::with_fmv(date, &grant.symbol, grant.quantity, fmv_cash));
    } else {
        statement.stock_grants.push(StockGrant::new(date, &grant.symbol, grant.quantity));
    }

    log::debug!(
        "Stock grant: {} {} shares on {} (type: {}, FMV: {} {})",
        grant.symbol, grant.quantity, date,
        if grant.grant_type.is_empty() { "unknown" } else { &grant.grant_type },
        grant.fmv, functional_currency
    );

    Ok(())
}

fn parse_flex_date(date_str: &str) -> GenericResult<Date> {
    // Format: YYYYMMDD. Require 8 ASCII digits so the byte slices below can never split a
    // multi-byte UTF-8 sequence and panic on malformed input.
    if date_str.len() != 8 || !date_str.bytes().all(|b| b.is_ascii_digit()) {
        return Err!("Invalid date format: {}", date_str);
    }

    let year: i32 = date_str[0..4].parse().map_err(|_| format!("Invalid year in date: {}", date_str))?;
    let month: u32 = date_str[4..6].parse().map_err(|_| format!("Invalid month in date: {}", date_str))?;
    let day: u32 = date_str[6..8].parse().map_err(|_| format!("Invalid day in date: {}", date_str))?;

    Date::from_ymd_opt(year, month, day).ok_or_else(|| format!("Invalid date: {}", date_str).into())
}

fn parse_flex_datetime(datetime_str: &str) -> GenericResult<Date> {
    // Format: YYYYMMDD;HHMMSS or YYYYMMDD. Use get(..8) so a multi-byte UTF-8 payload errors
    // instead of panicking on a byte slice that lands inside a character.
    let date_part = if let Some(idx) = datetime_str.find(';') {
        &datetime_str[..idx]
    } else {
        datetime_str
            .get(..8)
            .ok_or_else(|| format!("Invalid datetime format: {}", datetime_str))?
    };

    parse_flex_date(date_part)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A malformed date whose bytes total 8 but split a multi-byte UTF-8 character must error, not
    /// panic on a byte slice landing inside that character.
    #[test]
    fn parse_flex_date_rejects_multibyte_without_panicking() {
        assert!(parse_flex_date("202¼315").is_err());
        assert!(parse_flex_date("2025031").is_err());
        assert_eq!(
            parse_flex_date("20250315").unwrap(),
            Date::from_ymd_opt(2025, 3, 15).unwrap()
        );
    }

    /// The parser must not ingest the same income twice when both StmtFunds and CashTransactions
    /// are present (the docs tell users to enable both), must treat a negative dividend as a
    /// reversal instead of panicking, must skip the `BASE_SUMMARY` aggregate cash row, must drop
    /// cancelled trades and keep fees, and must filter short positions.
    ///
    /// Enabled by T3 (parser dedup + reversal/edge-row handling).
    #[test]
    fn dedups_income_and_handles_edge_rows() {
        let data =
            std::fs::read("src/tax_statement/germany/testdata/income_edge/statement.xml").unwrap();
        // Must not panic (negative dividend) or error (cancelled trade).
        let partial = FlexQueryResponse::parse(&data, &mut TaxRemapping::new()).unwrap();

        // The single credit-interest event appears in both StmtFunds (CINT) and CashTransactions
        // (Broker Interest Received). With the StmtFunds section actually parsed, this assertion is
        // load-bearing: the CINT line is ingested unless the per-type dedup skip suppresses it.
        assert_eq!(partial.idle_cash_interest.len(), 1, "interest double-counted");

        // The MSFT dividend also appears in both sections and shares the dedup code path. Assert the
        // net amount (not just the key count) so a double-count — which would sum to 20 USD under
        // the same DividendId — is caught.
        let msft_dividend = partial
            .dividend_accruals
            .iter()
            .find(|(id, _)| id.issuer == InstrumentId::Symbol("MSFT".to_string()))
            .map(|(_, accruals)| accruals.clone().get_result().unwrap().0)
            .expect("MSFT dividend accrual missing");
        assert_eq!(
            msft_dividend,
            Some(Cash::new("USD", dec!(10))),
            "MSFT dividend double-counted across StmtFunds and CashTransactions",
        );

        // The BASE_SUMMARY aggregate row must not become a phantom currency balance.
        assert!(
            !partial.assets.cash.as_ref().is_some_and(|cash| cash.has_assets("BASE_SUMMARY")),
            "BASE_SUMMARY leaked into cash assets",
        );

        // The "Other Fees" cash transaction must be recorded, not dropped.
        assert!(!partial.fees.is_empty(), "fee not ingested");

        // The cancelled BUY and its original must both be dropped.
        assert!(partial.stock_buys.is_empty(), "cancelled trade not voided");

        // Short positions are filtered (out of scope), never passed through as holdings.
        assert!(
            partial.open_positions.values().all(|&position| position > dec!(0)),
            "short position leaked into open positions",
        );
    }

    #[test]
    fn test_parse_flex_date() {
        assert_eq!(
            parse_flex_date("20251023").unwrap(),
            Date::from_ymd_opt(2025, 10, 23).unwrap()
        );
    }

    #[test]
    fn test_parse_flex_datetime() {
        assert_eq!(
            parse_flex_datetime("20251023;075607").unwrap(),
            Date::from_ymd_opt(2025, 10, 23).unwrap()
        );

        assert_eq!(
            parse_flex_datetime("20251023").unwrap(),
            Date::from_ymd_opt(2025, 10, 23).unwrap()
        );
    }

    /// The Statement of Funds is replayed into a per-currency cash-flow ledger: `Currency`-level
    /// rows are captured in document order, and a forex leg is paired with its EUR counter-leg
    /// (same transactionID) so its actual execution value is available. `BaseCurrency` summary rows
    /// are excluded.
    #[test]
    fn stmtfunds_builds_foreign_cash_ledger() {
        let data = r#"<FlexQueryResponse queryName="german-tax-test" type="AF">
  <FlexStatements count="1">
    <FlexStatement accountId="U0000001" fromDate="20250101" toDate="20251231">
      <CashReport>
        <CashReportCurrency currency="USD" startingCash="0" endingCash="0" dividends="0" brokerInterest="0" withholdingTax="0"/>
        <CashReportCurrency currency="EUR" startingCash="0" endingCash="0" dividends="0" brokerInterest="0" withholdingTax="0"/>
      </CashReport>
      <StmtFunds>
        <StatementOfFundsLine currency="USD" date="20251104" activityCode="BUY" symbol="STRC" amount="-4998.50" balance="-4998.50" levelOfDetail="Currency" transactionID="100"/>
        <StatementOfFundsLine currency="USD" date="20251104" activityCode="FOREX" activityDescription="Trading Currency Leg from Forex Trade: -4353.72 EUR.USD" symbol="EUR.USD" amount="4996.94" balance="-1.56" levelOfDetail="Currency" transactionID="200"/>
        <StatementOfFundsLine currency="EUR" date="20251104" activityCode="FOREX" activityDescription="Traded Currency Leg from Forex Trade" amount="-4353.72" balance="0" levelOfDetail="Currency" transactionID="200"/>
        <StatementOfFundsLine currency="EUR" date="20251104" activityCode="FOREX" symbol="EUR.USD" amount="-50" balance="-50" levelOfDetail="BaseCurrency" transactionID="200"/>
      </StmtFunds>
    </FlexStatement>
  </FlexStatements>
</FlexQueryResponse>"#;
        let partial = FlexQueryResponse::parse(data.as_bytes(), &mut TaxRemapping::new()).unwrap();

        // The BaseCurrency summary row is excluded; the three Currency-level rows are captured in order.
        let flows = &partial.foreign_cash_flows;
        assert_eq!(flows.len(), 3);

        let usd: Vec<_> = flows.iter().filter(|f| f.currency == "USD").collect();
        assert_eq!(usd.len(), 2);

        // The buy is an outflow with no EUR execution leg (the FIFO values it at the ECB rate).
        assert_eq!(usd[0].activity_code, "BUY");
        assert_eq!(usd[0].amount, dec!(-4998.50));
        assert_eq!(usd[0].balance, dec!(-4998.50));
        assert_eq!(usd[0].eur_execution, None);

        // The forex leg is paired with its EUR counter-leg (transactionID 200) → execution value.
        assert_eq!(usd[1].activity_code, "FOREX");
        assert_eq!(usd[1].amount, dec!(4996.94));
        assert_eq!(usd[1].eur_execution, Some(dec!(-4353.72)));
    }

    /// Two EUR forex legs sharing a transactionID make the execution-rate pairing ambiguous; the
    /// ledger build refuses rather than silently binding the last-written EUR amount.
    #[test]
    fn stmtfunds_rejects_duplicate_forex_transaction_id() {
        let data = r#"<FlexQueryResponse queryName="german-tax-test" type="AF">
  <FlexStatements count="1">
    <FlexStatement accountId="U0000001" fromDate="20250101" toDate="20251231">
      <CashReport>
        <CashReportCurrency currency="USD" startingCash="0" endingCash="0" dividends="0" brokerInterest="0" withholdingTax="0"/>
        <CashReportCurrency currency="EUR" startingCash="0" endingCash="0" dividends="0" brokerInterest="0" withholdingTax="0"/>
      </CashReport>
      <StmtFunds>
        <StatementOfFundsLine currency="EUR" date="20251104" activityCode="FOREX" amount="-100" balance="-100" levelOfDetail="Currency" transactionID="200"/>
        <StatementOfFundsLine currency="EUR" date="20251104" activityCode="FOREX" amount="-200" balance="-300" levelOfDetail="Currency" transactionID="200"/>
      </StmtFunds>
    </FlexStatement>
  </FlexStatements>
</FlexQueryResponse>"#;
        let err = match FlexQueryResponse::parse(data.as_bytes(), &mut TaxRemapping::new()) {
            Ok(_) => panic!("expected a duplicate FOREX transactionID error"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("Duplicate FOREX transactionID"),
            "{err}"
        );
    }

    fn trade_row(symbol: &str, asset_category: &str) -> Trade {
        Trade {
            currency: "USD".to_string(),
            symbol: symbol.to_string(),
            isin: String::new(),
            description: symbol.to_string(),
            asset_category: asset_category.to_string(),
            date_time: "20240110;100000".to_string(),
            settle_date: "20240112".to_string(),
            quantity: dec!(1),
            trade_price: dec!(10),
            trade_money: dec!(-10),
            commission: dec!(0),
            buy_sell: "BUY".to_string(),
            open_close_indicator: "O".to_string(),
            trade_id: String::new(),
            orig_trade_id: String::new(),
        }
    }

    /// Instrument selection is by IB asset category, not by symbol pattern: a real stock whose
    /// ticker ends in 'W' (GLW) is ingested, while an option (OPT) is excluded.
    ///
    /// Enabled by T8 (remove symbol-pattern derivative detection).
    #[test]
    fn parse_trade_keeps_stocks_and_excludes_derivatives() {
        let mut statement = PartialBrokerStatement::new(&[Exchange::Us], false);

        let stock = trade_row("GLW", "STK");
        assert!(parse_trade(&mut statement, &stock, &mut HashSet::new()).unwrap());
        assert_eq!(statement.stock_buys.len(), 1);

        let option = trade_row("AAPL  240119C00150000", "OPT");
        assert!(!parse_trade(&mut statement, &option, &mut HashSet::new()).unwrap());
        assert_eq!(statement.stock_buys.len(), 1, "option must not be ingested");
    }
}

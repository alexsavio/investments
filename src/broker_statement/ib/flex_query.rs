// IB Flex Query XML format parser
//
// This module parses the XML format exported from Interactive Brokers Flex Queries.
// The XML format contains more detailed information than the CSV Activity Statements.

use serde::Deserialize;

use std::collections::HashSet;

use crate::broker_statement::grants::StockGrant;
use crate::broker_statement::interest::{IdleCashInterest, FxGain};
use crate::broker_statement::partial::PartialBrokerStatement;
use crate::broker_statement::trades::{StockBuy, StockSell};
use crate::broker_statement::{Fee, Withholding};
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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorporateAction {
    #[serde(rename = "@currency")]
    pub currency: String,

    #[serde(rename = "@symbol")]
    pub symbol: String,

    #[serde(rename = "@isin", default)]
    pub isin: String,

    #[serde(rename = "@description")]
    pub description: String,

    #[serde(rename = "@dateTime")]
    pub date_time: String,

    #[serde(rename = "@quantity")]
    pub quantity: Decimal,

    #[serde(rename = "@type")]
    pub action_type: String,

    #[serde(rename = "@value")]
    pub value: Decimal,
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
    pub fn parse(data: &[u8]) -> GenericResult<PartialBrokerStatement> {
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
        statement.parse()
    }
}

impl FlexStatement {
    fn parse(&self) -> GenericResult<PartialBrokerStatement> {
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

        // Per-currency starting balances drive the FX margin-loan classification below.
        let balances = CurrencyBalances::from_cash_report(self.cash_report.as_ref());

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
                if parse_trade(&mut statement, trade)? {
                    trades_found = true;
                }
            }
        }

        // Parse trades from Statement of Funds if no trades were found in Trades section
        if !trades_found {
            if let Some(ref stmtfunds) = self.statement_of_funds {
                for line in &stmtfunds.lines {
                    parse_statement_of_funds_trade(&mut statement, line)?;
                }
            }
        }

        // Parse dividends and withholding taxes from Statement of Funds.
        // The docs tell users to enable both StmtFunds and CashTransactions, so income that appears
        // in both would be double-counted. Suppress a StmtFunds income type only when the
        // CashTransactions section actually carries that same type (per-type, so a present-but-empty
        // — or fees-only — CashTransactions section never silently drops StmtFunds income).
        // Likewise skip FOREX when the richer FxTransactions section exists.
        let has_fx_transactions = self.fx_transactions.is_some();
        let cash_income = self
            .cash_transactions
            .as_ref()
            .map(CashTxIncome::from_transactions)
            .unwrap_or_default();
        if let Some(ref stmtfunds) = self.statement_of_funds {
            for line in &stmtfunds.lines {
                parse_statement_of_funds_dividend(
                    &mut statement,
                    line,
                    has_fx_transactions,
                    cash_income,
                    &balances,
                )?;
            }
        }

        // Parse cash transactions (dividends, interest, etc.)
        if let Some(ref transactions) = self.cash_transactions {
            for tx in &transactions.transactions {
                parse_cash_transaction(&mut statement, tx)?;
            }
        }

        // Parse open positions to register instruments
        if let Some(ref positions) = self.open_positions {
            for pos in &positions.positions {
                if pos.symbol.is_empty() || pos.position == Decimal::ZERO {
                    continue;
                }
                // add_open_position enforces strictly-positive quantities; German tax handling of
                // short positions is out of scope, so warn and skip rather than abort the statement.
                if pos.position < Decimal::ZERO {
                    log::warn!(
                        "Ignoring short position of {} {} (short positions are not handled).",
                        pos.position, pos.symbol
                    );
                    continue;
                }
                statement.add_open_position(&pos.symbol, pos.position)?;
            }
        }

        // Parse FX transactions for realized FX P&L
        // FxTransactions section has accurate realizedPL values (preferred over StmtFunds FOREX)
        // Also extract functional currency for stock grant FMV conversion
        let mut functional_currency: Option<String> = None;
        if let Some(ref fx_transactions) = self.fx_transactions {
            for tx in &fx_transactions.transactions {
                parse_fx_transaction(&mut statement, tx, &balances)?;
                // Get the functional currency from FX transactions (they all should have the same one)
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
fn parse_statement_of_funds_trade(statement: &mut PartialBrokerStatement, line: &StatementOfFundsLine) -> EmptyResult {
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
            log::warn!(
                "Skipping non-stock trade {} (assetCategory {}); German tax handling of \
                 derivatives is out of scope.",
                line.symbol, line.asset_category
            );
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

/// Parse dividend, withholding tax, interest, and FX gain entries from Statement of Funds section.
/// `cash_income` records which income types the CashTransactions section already carries; those
/// types are skipped here (that section is authoritative) so income is not counted twice. If
/// `skip_forex` is true, FOREX entries are skipped because the FxTransactions section is available.
fn parse_statement_of_funds_dividend(statement: &mut PartialBrokerStatement, line: &StatementOfFundsLine, skip_forex: bool, cash_income: CashTxIncome, balances: &CurrencyBalances) -> EmptyResult {
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
            statement.idle_cash_interest.push(IdleCashInterest::new(date, amount));
            log::debug!("Credit interest: {} on {}", amount, date);
        }
        "FOREX" if !skip_forex => {
            // Forex transaction - the amount field contains IB's realized FX P&L. This fallback
            // path is only used when the richer FxTransactions section is absent.
            //
            // For German tax purposes (§20 Abs. 2 Nr. 7 EStG):
            // - FX gains on interest-bearing currency accounts are taxable as capital income
            // - FX gains/losses from margin loan repayments are NOT taxable
            //   (Tilgung eines Fremdwährungskredits - debt repayment is not a taxable event)
            if line.amount != Decimal::ZERO {
                let currency_pair = if !line.symbol.is_empty() {
                    line.symbol.clone()
                } else {
                    "FOREX".to_string()
                };
                let gain_amount = Cash::new(&line.currency, line.amount);

                // Classify by the converted currency's balance sign, not by trade magnitude.
                let mut currencies: Vec<&str> =
                    line.symbol.split('.').filter(|c| c.len() == 3).collect();
                if currencies.is_empty() {
                    currencies.push(line.currency.as_str());
                }
                let is_margin_loan =
                    fx_is_margin_loan(balances, &currencies, &line.activity_description);

                statement.fx_gains.push(FxGain::new(
                    date,
                    gain_amount,
                    currency_pair,
                    line.activity_description.clone(),
                    is_margin_loan,
                ));

                if is_margin_loan {
                    log::debug!("FX margin loan (StmtFunds): {} on {} ({})", gain_amount, date, line.activity_description);
                } else {
                    log::debug!("FX taxable (StmtFunds): {} on {} ({})", gain_amount, date, line.activity_description);
                }
            }
        }
        _ => {}
    }

    Ok(())
}

/// Parse FX transaction from FxTransactions section.
/// This section provides accurate realized P&L values for FX gains/losses.
///
/// For German tax purposes (§20 Abs. 2 Nr. 7 EStG):
/// - FX gains on interest-bearing currency accounts are taxable as capital income
/// - FX gains/losses from margin loan repayments are NOT taxable
///   (Tilgung eines Fremdwährungskredits - debt repayment is not a taxable event)
///
/// The FxTransactions section is preferred over StmtFunds FOREX entries because:
/// 1. It has the actual realizedPL field (not just the position impact)
/// 2. The description indicates what triggered the FX transaction (CASH vs STK)
fn parse_fx_transaction(statement: &mut PartialBrokerStatement, tx: &FxTransactionEntry, balances: &CurrencyBalances) -> EmptyResult {
    // Skip entries with zero realized P&L
    if tx.realized_pl == Decimal::ZERO {
        return Ok(());
    }

    let date = parse_flex_datetime(&tx.date_time)?;

    // Build currency pair from functional currency and FX currency
    let currency_pair = format!("{}.{}", tx.functional_currency, tx.fx_currency);

    // The P&L is in functional currency (account's base currency, e.g., EUR or USD)
    // Conversion to EUR for German tax purposes happens in the tax processor
    let gain_amount = Cash::new(&tx.functional_currency, tx.realized_pl);

    // A conversion of the foreign currency repays a margin loan only when that currency was
    // borrowed (negative balance); otherwise it is a taxable disposal.
    let is_margin_loan =
        fx_is_margin_loan(balances, &[&tx.fx_currency], &tx.activity_description);

    statement.fx_gains.push(FxGain::new(
        date,
        gain_amount,
        currency_pair,
        tx.activity_description.clone(),
        is_margin_loan,
    ));

    if is_margin_loan {
        log::debug!("FX margin loan (FxTransactions): {} on {} ({})", gain_amount, date, tx.activity_description);
    } else {
        log::debug!("FX taxable (FxTransactions): {} on {} ({})", gain_amount, date, tx.activity_description);
    }

    Ok(())
}

/// Per-currency cash balances seeded from the CashReport starting balances.
///
/// Used to classify FX conversions: only a conversion that repays a *borrowed* (negative-balance)
/// currency is a non-taxable margin-loan repayment (Tilgung Fremdwährungskredit,
/// BMF 19.05.2022 Rz. 131). This replaces the previous magnitude heuristic, which silently exempted
/// any large conversion regardless of whether a loan existed.
#[derive(Default)]
struct CurrencyBalances {
    starting: std::collections::HashMap<String, Decimal>,
}

impl CurrencyBalances {
    fn from_cash_report(report: Option<&CashReport>) -> CurrencyBalances {
        let mut starting = std::collections::HashMap::new();
        if let Some(report) = report {
            for row in &report.currencies {
                // Skip the BASE_SUMMARY aggregate and any non-ISO placeholder currency.
                if row.currency == "BASE_SUMMARY" || row.currency.len() != 3 {
                    continue;
                }
                starting.insert(row.currency.clone(), row.starting_cash);
            }
        }
        CurrencyBalances { starting }
    }

    /// `Some(true)` if the currency was carried as a loan (negative starting balance), `Some(false)`
    /// if held non-negative, `None` if no balance is known for it.
    fn borrowed(&self, currency: &str) -> Option<bool> {
        self.starting.get(currency).map(|balance| *balance < Decimal::ZERO)
    }
}

/// Classify an FX conversion as a non-taxable margin-loan repayment or a taxable currency disposal.
///
/// A conversion repays a margin loan only when the account carried a negative (borrowed) balance in
/// one of the converted currencies. The whole conversion is classified by that balance sign — the
/// pro-rata split of a partial repayment is a documented simplification. When no balance is known
/// for any candidate currency we fail open: treat the conversion as taxable and warn, never exempt
/// income on a size heuristic.
fn fx_is_margin_loan(balances: &CurrencyBalances, currencies: &[&str], description: &str) -> bool {
    let mut any_known = false;
    for currency in currencies {
        match balances.borrowed(currency) {
            Some(true) => return true,
            Some(false) => any_known = true,
            None => {}
        }
    }
    if !any_known {
        log::warn!(
            "No cash balance to classify FX conversion {description:?}; treating it as a taxable \
             disposal — review any margin-loan-related conversions manually."
        );
    }
    false
}

/// Ingest a stock trade. Returns `true` if a trade was recorded, `false` if the row was skipped as
/// a non-stock instrument. Cancellation rows are filtered out by the caller before this is reached.
fn parse_trade(statement: &mut PartialBrokerStatement, trade: &Trade) -> GenericResult<bool> {
    // Keep only stocks; warn on every skipped derivative/non-stock category (FR-016). Asset-category
    // filtering here replaces the old symbol-pattern guess that dropped real tickers like GLW/WST.
    if trade.asset_category != "STK" {
        log::warn!(
            "Skipping non-stock instrument {} (assetCategory {}); German tax handling of \
             derivatives is out of scope.",
            trade.symbol, trade.asset_category
        );
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

fn parse_cash_transaction(statement: &mut PartialBrokerStatement, tx: &CashTransaction) -> EmptyResult {
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
            // Tax on dividends - will be matched by the tax accrual system
            if !tx.symbol.is_empty() {
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

        "Broker Interest Paid" | "Broker Interest Received" => {
            statement.idle_cash_interest.push(IdleCashInterest::new(date, amount));
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
    // Format: YYYYMMDD
    if date_str.len() != 8 {
        return Err!("Invalid date format: {}", date_str);
    }

    let year: i32 = date_str[0..4].parse().map_err(|_| format!("Invalid year in date: {}", date_str))?;
    let month: u32 = date_str[4..6].parse().map_err(|_| format!("Invalid month in date: {}", date_str))?;
    let day: u32 = date_str[6..8].parse().map_err(|_| format!("Invalid day in date: {}", date_str))?;

    Date::from_ymd_opt(year, month, day).ok_or_else(|| format!("Invalid date: {}", date_str).into())
}

fn parse_flex_datetime(datetime_str: &str) -> GenericResult<Date> {
    // Format: YYYYMMDD;HHMMSS or YYYYMMDD
    let date_part = if let Some(idx) = datetime_str.find(';') {
        &datetime_str[..idx]
    } else if datetime_str.len() >= 8 {
        &datetime_str[..8]
    } else {
        return Err!("Invalid datetime format: {}", datetime_str);
    };

    parse_flex_date(date_part)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let partial = FlexQueryResponse::parse(&data).unwrap();

        // The single credit-interest event appears in both StmtFunds (CINT) and CashTransactions
        // (Broker Interest Received). Dividends/withholding share this code path.
        assert_eq!(partial.idle_cash_interest.len(), 1, "interest double-counted");

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

    /// Build a minimal statement with one USD FX conversion and the given USD starting balance.
    fn fx_margin_fixture(starting_usd_cash: &str) -> PartialBrokerStatement {
        let data = format!(
            r#"<FlexQueryResponse queryName="german-tax-test" type="AF">
  <FlexStatements count="1">
    <FlexStatement accountId="U0000001" fromDate="20240101" toDate="20241231">
      <CashReport>
        <CashReportCurrency currency="USD" startingCash="{starting_usd_cash}" endingCash="0" dividends="0" brokerInterest="0" withholdingTax="0"/>
      </CashReport>
      <FxTransactions>
        <FxTransaction functionalCurrency="EUR" fxCurrency="USD" reportDate="20240601" dateTime="20240601;120000" activityDescription="CASH: 5000 EUR.USD" quantity="5000" realizedPL="123.45" code="C"/>
      </FxTransactions>
    </FlexStatement>
  </FlexStatements>
</FlexQueryResponse>"#
        );
        FlexQueryResponse::parse(data.as_bytes()).unwrap()
    }

    /// A conversion that repays a borrowed (negative-balance) currency is a non-taxable margin-loan
    /// repayment (Tilgung Fremdwährungskredit), regardless of its size.
    ///
    /// Enabled by T7 (balance-based FX classification).
    #[test]
    fn fx_conversion_repaying_borrowed_currency_is_margin_loan() {
        let partial = fx_margin_fixture("-5000");
        assert_eq!(partial.fx_gains.len(), 1);
        assert!(
            partial.fx_gains[0].is_margin_loan,
            "converting a borrowed (negative-balance) currency is a margin-loan repayment",
        );
    }

    /// The same-size conversion with a positive balance is a taxable currency disposal — the old
    /// magnitude heuristic wrongly exempted it because the quantity exceeded 100.
    ///
    /// Enabled by T7 (balance-based FX classification).
    #[test]
    fn fx_conversion_with_positive_balance_is_taxable() {
        let partial = fx_margin_fixture("5000");
        assert_eq!(partial.fx_gains.len(), 1);
        assert!(
            !partial.fx_gains[0].is_margin_loan,
            "a positive-balance conversion is a taxable disposal, not a margin-loan repayment",
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
        assert!(parse_trade(&mut statement, &stock).unwrap());
        assert_eq!(statement.stock_buys.len(), 1);

        let option = trade_row("AAPL  240119C00150000", "OPT");
        assert!(!parse_trade(&mut statement, &option).unwrap());
        assert_eq!(statement.stock_buys.len(), 1, "option must not be ingested");
    }
}

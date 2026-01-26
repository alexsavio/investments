// IB Flex Query XML format parser
//
// This module parses the XML format exported from Interactive Brokers Flex Queries.
// The XML format contains more detailed information than the CSV Activity Statements.

use serde::Deserialize;

use crate::broker_statement::grants::StockGrant;
use crate::broker_statement::interest::{IdleCashInterest, FxGain};
use crate::broker_statement::partial::PartialBrokerStatement;
use crate::broker_statement::trades::{StockBuy, StockSell};
use crate::core::{EmptyResult, GenericResult};
use crate::currency::{Cash, CashAssets};
use crate::exchanges::Exchange;
use crate::formats::xml;
use crate::instruments::InstrumentId;
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

        // For now, process the first statement
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

        // Parse cash balances from CashReport
        if let Some(ref cash_report) = self.cash_report {
            for currency_report in &cash_report.currencies {
                let cash_assets = statement.assets.cash.get_or_insert_with(Default::default);
                let cash = Cash::new(&currency_report.currency, currency_report.ending_cash);
                cash_assets.deposit(cash);
            }
        }

        // Parse trades from Trades section (preferred if it has content)
        let mut trades_found = false;
        if let Some(ref trades) = self.trades {
            for trade in &trades.trades {
                parse_trade(&mut statement, trade)?;
                trades_found = true;
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

        // Parse dividends and withholding taxes from Statement of Funds
        // (These are often here instead of CashTransactions)
        // Skip FOREX parsing if FxTransactions section is available (more accurate)
        let has_fx_transactions = self.fx_transactions.is_some();
        if let Some(ref stmtfunds) = self.statement_of_funds {
            for line in &stmtfunds.lines {
                parse_statement_of_funds_dividend(&mut statement, line, has_fx_transactions)?;
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
                if !pos.symbol.is_empty() && pos.position != Decimal::ZERO {
                    statement.add_open_position(&pos.symbol, pos.position)?;
                }
            }
        }

        // Parse FX transactions for realized FX P&L
        // FxTransactions section has accurate realizedPL values (preferred over StmtFunds FOREX)
        // Also extract functional currency for stock grant FMV conversion
        let mut functional_currency: Option<String> = None;
        if let Some(ref fx_transactions) = self.fx_transactions {
            for tx in &fx_transactions.transactions {
                parse_fx_transaction(&mut statement, tx)?;
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

    // Skip non-stock entries and forex
    if line.asset_category != "STK" || line.symbol.is_empty() {
        return Ok(());
    }

    // Only process BUY and SELL activities
    match line.activity_code.as_str() {
        "BUY" | "SELL" => {}
        _ => return Ok(()),
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

    // Register the instrument
    if !line.isin.is_empty() {
        let _ = statement.instrument_info.get_or_add(&line.isin);
    }

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

/// Parse dividend, withholding tax, interest, and FX gain entries from Statement of Funds section
/// If `skip_forex` is true, FOREX entries are skipped (because FxTransactions section is available)
fn parse_statement_of_funds_dividend(statement: &mut PartialBrokerStatement, line: &StatementOfFundsLine, skip_forex: bool) -> EmptyResult {
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
            // Forex transaction - the amount field contains IB's realized FX P&L
            // This code path is only used when FxTransactions section is not available
            //
            // For German tax purposes (§20 Abs. 2 Nr. 7 EStG):
            // - FX gains on interest-bearing currency accounts are taxable as capital income
            // - FX gains/losses from margin loan repayments are NOT taxable
            //   (Tilgung eines Fremdwährungskredits - debt repayment is not a taxable event)
            //
            // Heuristic to distinguish margin loan FX from taxable FX:
            // - The activity_description contains the FX trade notional amount
            //   Format: "Net Amount in Base from Forex Trade: -4,353.72 EUR.USD"
            // - Large trades (typically >€100 notional) are usually margin loan conversions
            // - Small amounts (<€100) are usually from interest, dividends, commissions
            if line.amount != Decimal::ZERO {
                let currency_pair = if !line.symbol.is_empty() {
                    line.symbol.clone()
                } else {
                    "FOREX".to_string()
                };
                let gain_amount = Cash::new(&line.currency, line.amount);

                // Parse trade notional from description
                // Format: "Net Amount in Base from Forex Trade: -4,353.72 EUR.USD"
                let is_margin_loan = parse_forex_notional(&line.activity_description)
                    .map(|notional| notional.abs() > dec!(100))
                    .unwrap_or(false);

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
fn parse_fx_transaction(statement: &mut PartialBrokerStatement, tx: &FxTransactionEntry) -> EmptyResult {
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

    // Determine if this is a margin loan FX or taxable FX based on the activity description.
    // Format examples:
    // - "CASH: -4353.72 EUR.USD" - FX conversion, likely margin loan if large
    // - "STK: 50 STRC" - FX from stock purchase, margin loan
    // - "STRC(US5949728530) CASH DIVIDEND USD 0.875 PER SHARE" - FX from dividend, taxable
    // - "STRC(US5949728530) CASH DIVIDEND USD 0.875 PER SHARE - US TAX" - FX from withholding tax, taxable
    //
    // Heuristic:
    // - "CASH:" with large quantity (>€100) = margin loan conversion
    // - "STK:" = stock purchase/sale, margin loan
    // - Dividend/interest related = taxable
    let is_margin_loan = determine_fx_is_margin_loan(&tx.activity_description, tx.quantity);

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

/// Determine if an FX transaction is from margin loan activity (non-taxable) or taxable activity.
///
/// German tax law:
/// - FX gains on Tilgung Fremdwährungskredit (margin loan repayment) are NOT taxable
/// - FX gains on interest-bearing currency accounts (from dividends, interest, commissions) ARE taxable
///
/// Heuristic based on FxTransaction activity description:
/// - "CASH: <amount> EUR.USD" - Currency conversion
///   - Large amounts (>€100) are typically margin loan conversions (not taxable)
///   - Small amounts (<€100) are typically from interest, dividends, or commissions (taxable)
/// - "STK: <qty> <symbol>" - Stock trade, requires margin loan conversion (not taxable)
/// - Contains "DIVIDEND" or "INTEREST" - Income-related FX (taxable)
/// - Contains "TAX" - Withholding tax payment (taxable)
fn determine_fx_is_margin_loan(description: &str, quantity: Decimal) -> bool {
    let desc_upper = description.to_uppercase();

    // Dividend and interest-related FX is taxable
    if desc_upper.contains("DIVIDEND") || desc_upper.contains("INTEREST") || desc_upper.contains(" TAX") {
        return false;
    }

    // Stock trades require margin loan conversion - not taxable
    if desc_upper.starts_with("STK:") {
        return true;
    }

    // Cash conversions - use quantity heuristic
    // Large amounts (>€100) are typically margin loan, small amounts are from income
    if desc_upper.starts_with("CASH:") {
        return quantity.abs() > dec!(100);
    }

    // Default: if large quantity, assume margin loan
    quantity.abs() > dec!(100)
}

fn parse_trade(statement: &mut PartialBrokerStatement, trade: &Trade) -> EmptyResult {
    // Skip non-stock trades (forex, options, etc.)
    if trade.asset_category != "STK" {
        return Ok(());
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

    // Register the instrument
    if !trade.isin.is_empty() {
        let _ = statement.instrument_info.get_or_add(&trade.isin);
    }

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

    Ok(())
}

fn parse_cash_transaction(statement: &mut PartialBrokerStatement, tx: &CashTransaction) -> EmptyResult {
    let date = parse_flex_datetime(&tx.date_time)?;
    let amount = Cash::new(&tx.currency, tx.amount);

    match tx.transaction_type.as_str() {
        "Dividends" | "Payment In Lieu Of Dividends" => {
            if tx.symbol.is_empty() {
                return Err!("Dividend without symbol: {:?}", tx.description);
            }

            // Track dividend for later tax matching
            let issuer = InstrumentId::Symbol(tx.symbol.clone());
            statement.dividend_accruals(date, issuer, true).add(date, amount);
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
            // These are typically small adjustments, log but don't fail
            log::debug!("Skipping cash transaction: {} - {}", tx.transaction_type, tx.description);
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

/// Parse the FX trade notional amount from the activity description.
///
/// Format: "Net Amount in Base from Forex Trade: -4,353.72 EUR.USD"
/// Returns the numeric value (e.g., -4353.72) or None if parsing fails.
fn parse_forex_notional(description: &str) -> Option<Decimal> {
    // Look for the pattern after "Forex Trade: "
    let marker = "Forex Trade: ";
    let idx = description.find(marker)?;
    let after_marker = &description[idx + marker.len()..];

    // Find the space before the currency pair
    let space_idx = after_marker.rfind(' ')?;
    let number_str = &after_marker[..space_idx];

    // Remove commas and parse
    let cleaned = number_str.replace(',', "");
    cleaned.parse::<Decimal>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_parse_forex_notional() {
        // Normal cases
        assert_eq!(
            parse_forex_notional("Net Amount in Base from Forex Trade: -4,353.72 EUR.USD"),
            Some(dec!(-4353.72))
        );
        assert_eq!(
            parse_forex_notional("Net Amount in Base from Forex Trade: 186.5 EUR.USD"),
            Some(dec!(186.5))
        );
        assert_eq!(
            parse_forex_notional("Net Amount in Base from Forex Trade: -0.87 EUR.USD"),
            Some(dec!(-0.87))
        );
        assert_eq!(
            parse_forex_notional("Net Amount in Base from Forex Trade: 0.00771351 EUR.USD"),
            Some(dec!(0.00771351))
        );

        // Edge cases
        assert_eq!(
            parse_forex_notional("Some other description"),
            None
        );
    }
}

pub mod config;

use chrono::Datelike;
use isin::ISIN;

#[cfg(test)] use crate::brokers::Broker;
#[cfg(test)] use crate::config::Config;
use crate::core::{EmptyResult, GenericResult};
use crate::currency::{Cash, CashAssets};
use crate::exchanges::Exchange;
use crate::formatting;
use crate::instruments::{self, InstrumentId};
use crate::time::{self, Period};
use crate::util::{self, DecimalRestrictions};

#[cfg(test)] use super::{BrokerStatement, ReadingStrictness};
use super::partial::PartialBrokerStatement;
use super::payments::Withholding;
use super::trades::{StockBuy, StockSell};

use self::config::{Operation, BuyOperation, SellOperation, DividendOperation};

pub struct StatementParser<'a> {
    currency: &'a str,
    statement: PartialBrokerStatement,
}

impl<'a> StatementParser<'a> {
    pub fn parse(operations: &[Operation], currency: &str) -> GenericResult<PartialBrokerStatement> {
        let mut parser = StatementParser {
            currency: currency,
            statement: PartialBrokerStatement::new(&[Exchange::Otc], true),
        };

        let today = time::today();
        let mut open_date = today;

        for operation in operations {
            let date = match operation {
                Operation::Buy(trade) => {
                    parser.parse_buy(trade).map_err(|e| format!(
                        "Invalid buy operation at {}: {e}", formatting::format_date(trade.date)))?;
                    trade.date
                },
                Operation::Sell(trade) => {
                    parser.parse_sell(trade).map_err(|e| format!(
                        "Invalid sell operation at {}: {e}", formatting::format_date(trade.date)))?;
                    trade.date
                },
                Operation::Dividend(dividend) => {
                    parser.parse_dividend(dividend).map_err(|e| format!(
                        "Invalid dividend operation at {}: {e}", formatting::format_date(dividend.date)))?;
                    dividend.date
                },
            };
            open_date = std::cmp::min(open_date, date);
        }

        let period = Period::new(open_date, today)?;
        parser.statement.period = Some(period);
        parser.statement.set_has_starting_assets(false)?;

        Ok(parser.statement)
    }

    fn parse_buy(&mut self, trade: &BuyOperation) -> EmptyResult {
        self.on_symbol(&trade.symbol)?;

        let conclusion_time = trade.date.into();
        let execution_date = trade.settle_date.unwrap_or(trade.date);

        let quantity = util::validate_named_decimal("quantity", trade.quantity, DecimalRestrictions::StrictlyPositive)?;
        let price = util::validate_named_cash("price", self.currency, trade.price, DecimalRestrictions::StrictlyPositive)?;
        let amount = util::validate_named_cash("amount", self.currency, trade.amount, DecimalRestrictions::StrictlyPositive)?;
        let commission = Cash::zero(self.currency);

        let expected_amount = price * quantity;
        if amount.round() != expected_amount.round() {
            return Err!("Got an unexpected amount: {amount} vs {expected_amount}");
        }

        self.statement.deposits_and_withdrawals.push(
            CashAssets::new_from_cash(trade.date, amount));

        self.statement.stock_buys.push(StockBuy::new_trade(
            &trade.symbol, quantity, price, amount, commission,
            conclusion_time, execution_date));

        Ok(())
    }

    fn parse_sell(&mut self, trade: &SellOperation) -> EmptyResult {
        self.on_symbol(&trade.symbol)?;

        let conclusion_time = trade.date.into();
        let execution_date = trade.settle_date.unwrap_or(trade.date);

        let quantity = util::validate_named_decimal("quantity", trade.quantity, DecimalRestrictions::StrictlyPositive)?;
        let price = util::validate_named_cash("price", self.currency, trade.price, DecimalRestrictions::StrictlyPositive)?;
        let net_amount = util::validate_named_cash("net amount", self.currency, trade.net_amount, DecimalRestrictions::StrictlyPositive)?;
        let tax_withheld = util::validate_named_cash("tax withheld amount", self.currency, trade.tax_withheld, DecimalRestrictions::PositiveOrZero)?;
        let commission = Cash::zero(self.currency);

        let amount = (price * quantity).round();
        if amount - tax_withheld != net_amount {
            return Err!("{price} * {quantity} - {tax_withheld} != {net_amount}");
        }

        self.statement.stock_sells.push(StockSell::new_trade(
            &trade.symbol, quantity, price, amount, commission,
            conclusion_time, execution_date, false));

        if !tax_withheld.is_zero() {
            self.statement.tax_agent_withholdings.add(
                execution_date, execution_date.year(), Withholding::Withholding(tax_withheld))?;
        }

        self.statement.deposits_and_withdrawals.push(
            CashAssets::new_from_cash(execution_date, -net_amount));

        Ok(())
    }

    fn parse_dividend(&mut self, dividend: &DividendOperation) -> EmptyResult {
        let isin = self.on_symbol(&dividend.symbol)?;

        let gross_amount = util::validate_named_cash(
            "gross amount", self.currency, dividend.gross_amount, DecimalRestrictions::StrictlyPositive)?;

        let tax_withheld = util::validate_named_cash(
            "tax withheld amount", self.currency, dividend.tax_withheld, DecimalRestrictions::PositiveOrZero)?;

        let net_amount = util::validate_named_cash(
            "net amount", self.currency, dividend.net_amount, DecimalRestrictions::StrictlyPositive)?;

        if gross_amount - tax_withheld != net_amount {
            return Err!("{gross_amount} - {tax_withheld} != {net_amount}");
        }

        let issuer_id = InstrumentId::Isin(isin);
        self.statement.dividend_accruals(dividend.date, issuer_id.clone(), true).add(dividend.date, gross_amount);
        self.statement.tax_accruals(dividend.date, issuer_id, true).add(dividend.date, tax_withheld);

        self.statement.deposits_and_withdrawals.push(
            CashAssets::new_from_cash(dividend.date, -net_amount));

        Ok(())
    }

    fn on_symbol(&mut self, symbol: &str) -> GenericResult<ISIN> {
        // At this time `other` broker type is implied to be used for closed-end unit investment funds only which don't
        // have tickers, but have ISIN. On the other side, now we need ISIN information for dividend processing. So at
        // least for now make this requirement to simplify the things.
        let isin = instruments::parse_isin(symbol)?;
        self.statement.instrument_info.get_or_add(symbol).add_isin(isin);
        Ok(isin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_real() {
        let config = Config::new("testdata/configs/main", None).unwrap();

        let portfolio = config.get_portfolio("sfn").unwrap();
        assert_eq!(portfolio.broker, Broker::Other);

        let statement = BrokerStatement::load(&config, portfolio, ReadingStrictness::all()).unwrap();

        assert!(statement.assets.cash.is_empty());
        assert!(statement.assets.other.is_none());
        assert!(!statement.deposits_and_withdrawals.is_empty());

        assert!(statement.fees.is_empty());
        assert!(statement.cash_grants.is_empty());
        assert!(statement.idle_cash_interest.is_empty());
        assert!(statement.tax_agent_withholdings.is_empty());

        assert!(statement.forex_trades.is_empty());
        assert!(!statement.stock_buys.is_empty());
        assert!(statement.stock_sells.is_empty());
        assert!(!statement.dividends.is_empty());

        assert!(!statement.open_positions.is_empty());
        assert!(!statement.instrument_info.is_empty());
    }
}
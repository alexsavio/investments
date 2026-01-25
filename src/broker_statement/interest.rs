use crate::core::GenericResult;
use crate::currency::Cash;
use crate::currency::converter::CurrencyConverter;
use crate::localities::Country;
use crate::taxes::{IncomeType, TaxCalculator};
use crate::time::Date;
use chrono::Datelike;

pub struct IdleCashInterest {
    pub date: Date,
    pub amount: Cash, // May be negative
}

impl IdleCashInterest {
    pub fn new(date: Date, amount: Cash) -> IdleCashInterest {
        IdleCashInterest {
            date, amount
        }
    }

    pub fn tax(&self, country: &Country, converter: &CurrencyConverter, calculator: &mut TaxCalculator) -> GenericResult<Cash> {
        let amount = converter.convert_to_cash_rounding(self.date, self.amount, country.currency)?;
        Ok(calculator.tax_income(IncomeType::Interest, self.date.year(), amount, None).expected)
    }
}

/// Realized FX gain or loss from foreign currency holdings.
///
/// For German tax purposes:
/// - FX gains on interest-bearing currency accounts (like IBKR USD/EUR/GBP)
///   are taxable as capital income under §20 EStG (Abgeltungsteuer)
/// - Every use of foreign currency (buying securities, paying fees, converting back)
///   realizes a gain/loss based on the exchange rate difference between acquisition and disposal
/// - Losses from interest-bearing accounts can be offset against other capital income
///
/// IMPORTANT: FX gains/losses from margin loan repayments are NOT taxable!
/// (Tilgung eines Fremdwährungskredits - repaying borrowed currency is not a taxable event)
pub struct FxGain {
    pub date: Date,
    pub amount: Cash,           // Positive = gain, negative = loss (already in EUR)
    pub currency_pair: String,  // e.g., "EUR.USD"
    pub description: String,    // Transaction description
    pub is_margin_loan: bool,   // True if this is from margin loan repayment (not taxable)
}

impl FxGain {
    pub fn new(date: Date, amount: Cash, currency_pair: String, description: String, is_margin_loan: bool) -> FxGain {
        FxGain {
            date,
            amount,
            currency_pair,
            description,
            is_margin_loan,
        }
    }

    /// Create a taxable FX gain (from interest-bearing cash accounts)
    pub fn taxable(date: Date, amount: Cash, currency_pair: String, description: String) -> FxGain {
        FxGain::new(date, amount, currency_pair, description, false)
    }

    /// Create a non-taxable FX gain (from margin loan repayment)
    pub fn margin_loan(date: Date, amount: Cash, currency_pair: String, description: String) -> FxGain {
        FxGain::new(date, amount, currency_pair, description, true)
    }

    /// Convert to EUR using the converter (if not already in EUR)
    pub fn amount_in_eur(&self, converter: &CurrencyConverter) -> GenericResult<Cash> {
        converter.convert_to_cash_rounding(self.date, self.amount, "EUR")
    }
}
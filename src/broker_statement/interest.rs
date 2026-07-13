use crate::core::GenericResult;
use crate::currency::Cash;
use crate::currency::converter::CurrencyConverter;
use crate::localities::Country;
use crate::taxes::{IncomeType, TaxCalculator};
use crate::time::Date;
use crate::types::Decimal;
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

/// A single foreign-currency cash movement extracted from a broker's statement of funds.
///
/// German Fremdwährungsgewinne (§20 EStG) are computed by replaying these movements through a
/// per-currency signed-inventory FIFO: each inflow acquires currency, each outflow disposes it, and
/// a negative running balance marks a Fremdwährungskredit whose repayment is not taxable.
#[derive(Debug, Clone)]
pub struct ForeignCashFlow {
    pub currency: String,
    pub date: Date,
    pub transaction_id: String,
    pub activity_code: String,
    /// Signed amount in `currency`: positive = inflow, negative = outflow.
    pub amount: Decimal,
    /// Running balance in `currency` after this movement (statement-provided; preserves order).
    pub balance: Decimal,
    /// For a real currency exchange, the signed amount of the paired EUR leg (the actual execution
    /// value). `None` for movements without a EUR counter-leg (buys, dividends, fees), which the FX
    /// FIFO values at the ECB reference rate instead.
    pub eur_execution: Option<Decimal>,
}
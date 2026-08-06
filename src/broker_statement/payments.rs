use crate::core::GenericResult;
use crate::currency::{Cash, CashAssets};
use crate::time::Date;

#[derive(Clone, Copy)]
pub enum Withholding {
    Withholding(Cash),
    Refund(Cash),
}

impl Withholding {
    pub fn new(amount: Cash) -> Withholding {
        if amount.is_negative() {
            Withholding::Refund(-amount)
        } else {
            Withholding::Withholding(amount)
        }
    }

    pub fn withholding(self) -> Cash {
        match self {
            Withholding::Withholding(amount) => amount,
            Withholding::Refund(amount) => -amount,
        }
    }

    pub fn refund(self) -> Cash {
        match self {
            Withholding::Withholding(amount) => -amount,
            Withholding::Refund(amount) => amount,
        }
    }
}

/// Calculates result amount from a series of payments and reversals.
#[derive(Clone)]
pub struct Payments {
    strict: bool,
    transactions: Vec<CashAssets>,
}

impl Payments {
    pub fn new(strict: bool) -> Payments {
        Payments {
            strict: strict,
            transactions: Vec::new(),
        }
    }

    pub fn add(&mut self, date: Date, amount: Cash) {
        assert!(amount.is_positive());
        self.transactions.push(CashAssets::new_from_cash(date, amount));
    }

    pub fn reverse(&mut self, date: Date, amount: Cash) {
        assert!(amount.is_positive());
        self.transactions.push(CashAssets::new_from_cash(date, -amount));
    }

    pub fn merge(&mut self, other: &Payments) {
        assert_eq!(self.strict, other.strict);
        self.transactions.extend(other.transactions.iter());
    }

    /// Whether these are reversals only, with no payment anywhere to reverse against.
    ///
    /// The refund-only case: when a broker reclassifies a distribution (a US Return of Capital,
    /// say) it removes the dividend rows and refunds the withholding, so the credit legitimately
    /// has nothing to net against.
    ///
    /// This is deliberately narrower than "[`get_result`](Self::get_result) failed". That also
    /// rejects a reversal whose amount merely fails to match an existing payment (a partial
    /// refund) and a mixed-currency accrual -- both real problems that must stay fatal rather
    /// than be waved through as a reclassification.
    pub fn is_refund_only(&self) -> bool {
        !self.transactions.is_empty() &&
            self.transactions.iter().all(|transaction| transaction.cash.is_negative())
    }

    pub fn get_result(self) -> GenericResult<(Option<Cash>, Vec<CashAssets>)> {
        let Payments { strict, mut transactions } = self;
        transactions.sort_by_key(|transaction| transaction.date);

        if strict {
            // Don't require payments and reversals to be in order because Interactive Brokers
            // statement doesn't guarantee it.

            let (mut payments, reversals) = transactions.iter().cloned().partition::<Vec<_>, _>(|transaction| {
                transaction.cash.is_positive()
            });

            for reversal in reversals {
                let reversal = -reversal.cash;
                let index = payments.iter()
                    .position(|&payment| payment.cash == reversal)
                    .ok_or_else(|| format!("Unexpected reversal: {reversal}"))?;

                payments.remove(index);
            }

            let mut result = match payments.pop() {
                Some(payment) => payment.cash,
                None => return Ok((None, transactions)),
            };

            for payment in payments {
                let amount = payment.cash;
                result.add_assign(amount).map_err(|_| format!(
                    "Mixed currency: {} and {}", result.currency, amount.currency))?;
            }

            Ok((Some(result), transactions))
        } else {
            let mut result: Option<Cash> = None;

            for transaction in &transactions {
                let amount = transaction.cash;

                match result.as_mut() {
                    None => {
                        result.replace(amount);
                    },
                    Some(result) => {
                        result.add_assign(amount).map_err(|_| format!(
                            "Mixed currency: {} and {}", result.currency, amount.currency))?;
                    },
                };
            }

            if let Some(result) = result {
                if result.is_zero() {
                    return Ok((None, transactions));
                } else if result.is_negative() {
                    return Err!("Got a negative result: {}", result);
                }
            }

            Ok((result, transactions))
        }
    }
}
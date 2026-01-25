//! German capital loss carryforward (Verlustvortrag) handling.
//!
//! German tax law allows capital losses to be carried forward to offset
//! future capital gains. This module implements the calculation and
//! application of loss carryforwards.
//!
//! Key rules:
//! - Capital losses can only offset capital gains (not other income)
//! - Losses from stock sales can only offset gains from stock sales
//! - No time limit on carryforward
//! - Must be applied in the order they were incurred

use log::debug;

use crate::types::Decimal;

/// Represents loss carryforward balances from previous years.
#[derive(Debug, Clone, Default)]
pub struct LossCarryforward {
    /// Total loss carryforward balance from previous years (in EUR)
    pub previous_balance: Decimal,
    /// Loss carryforward used to offset current year gains
    pub used: Decimal,
    /// New losses incurred in current year
    pub current_year_loss: Decimal,
    /// Remaining loss carryforward for next year
    pub remaining_balance: Decimal,
}

impl LossCarryforward {
    /// Create a new loss carryforward with initial balance from previous years.
    pub fn new(previous_balance: Decimal) -> Self {
        LossCarryforward {
            previous_balance,
            used: Decimal::ZERO,
            current_year_loss: Decimal::ZERO,
            remaining_balance: previous_balance,
        }
    }

    /// Apply loss carryforward against capital gains.
    ///
    /// # Arguments
    /// * `gross_gain` - Total capital gains for the year (positive value)
    ///
    /// # Returns
    /// The taxable amount after applying loss carryforward
    pub fn apply_to_gains(&mut self, gross_gain: Decimal) -> Decimal {
        if gross_gain <= Decimal::ZERO {
            // No gains to offset, losses will be added to carryforward
            self.current_year_loss = gross_gain.abs();
            debug!(
                "Loss carryforward: No gains to offset, current year loss: €{:.2}",
                self.current_year_loss
            );
            return Decimal::ZERO;
        }

        // Apply previous carryforward to current gains
        let applicable = self.previous_balance.min(gross_gain);
        self.used = applicable;
        let taxable = gross_gain - applicable;

        // Update remaining balance
        self.remaining_balance = self.previous_balance - applicable;

        debug!(
            "Loss carryforward applied: previous=€{:.2}, used=€{:.2}, taxable=€{:.2}, remaining=€{:.2}",
            self.previous_balance, self.used, taxable, self.remaining_balance
        );

        taxable
    }

    /// Record a loss from the current year.
    ///
    /// # Arguments
    /// * `loss` - The loss amount (should be positive)
    pub fn add_current_year_loss(&mut self, loss: Decimal) {
        self.current_year_loss += loss.abs();
        self.remaining_balance += loss.abs();
        debug!(
            "Loss carryforward: Added current year loss €{:.2}, new remaining: €{:.2}",
            loss.abs(),
            self.remaining_balance
        );
    }

    /// Get the total carryforward balance for next year.
    /// This includes unused previous carryforward plus any new losses.
    pub fn balance_for_next_year(&self) -> Decimal {
        self.remaining_balance
    }
}

/// Calculate taxable capital gains after applying loss carryforward.
///
/// # Arguments
/// * `gross_gains` - Total capital gains (positive) or losses (negative)
/// * `loss_carryforward_balance` - Loss carryforward from previous years
///
/// # Returns
/// Tuple of (taxable_amount, loss_carryforward_used, new_carryforward_balance)
pub fn calculate_with_loss_carryforward(
    gross_gains: Decimal,
    loss_carryforward_balance: Decimal,
) -> (Decimal, Decimal, Decimal) {
    let mut cf = LossCarryforward::new(loss_carryforward_balance);

    if gross_gains < Decimal::ZERO {
        // Current year has net loss
        cf.add_current_year_loss(gross_gains.abs());
        (Decimal::ZERO, Decimal::ZERO, cf.balance_for_next_year())
    } else {
        // Current year has gains, apply carryforward
        let taxable = cf.apply_to_gains(gross_gains);
        (taxable, cf.used, cf.balance_for_next_year())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loss_carryforward_basic() {
        // T070: €5,000 carryforward against €10,000 gain = €5,000 taxable
        let (taxable, used, remaining) = calculate_with_loss_carryforward(dec!(10000), dec!(5000));

        assert_eq!(taxable, dec!(5000));
        assert_eq!(used, dec!(5000));
        assert_eq!(remaining, dec!(0));
    }

    #[test]
    fn test_loss_carryforward_exceeds_gains() {
        // €10,000 carryforward against €5,000 gain = €0 taxable, €5,000 remaining
        let (taxable, used, remaining) = calculate_with_loss_carryforward(dec!(5000), dec!(10000));

        assert_eq!(taxable, dec!(0));
        assert_eq!(used, dec!(5000));
        assert_eq!(remaining, dec!(5000));
    }

    #[test]
    fn test_loss_carryforward_no_carryforward() {
        // No carryforward, €10,000 gain = €10,000 taxable
        let (taxable, used, remaining) = calculate_with_loss_carryforward(dec!(10000), dec!(0));

        assert_eq!(taxable, dec!(10000));
        assert_eq!(used, dec!(0));
        assert_eq!(remaining, dec!(0));
    }

    #[test]
    fn test_loss_carryforward_current_year_loss() {
        // T071: €5,000 carryforward, current year loss €3,000 = €8,000 new carryforward
        let (taxable, used, remaining) = calculate_with_loss_carryforward(dec!(-3000), dec!(5000));

        assert_eq!(taxable, dec!(0));
        assert_eq!(used, dec!(0));
        assert_eq!(remaining, dec!(8000)); // Previous €5,000 + new €3,000 loss
    }

    #[test]
    fn test_loss_carryforward_zero_gains() {
        // €5,000 carryforward, no gains/losses
        let (taxable, used, remaining) = calculate_with_loss_carryforward(dec!(0), dec!(5000));

        assert_eq!(taxable, dec!(0));
        assert_eq!(used, dec!(0));
        assert_eq!(remaining, dec!(5000)); // Unchanged
    }

    #[test]
    fn test_loss_carryforward_struct() {
        let mut cf = LossCarryforward::new(dec!(5000));

        // Apply to gains
        let taxable = cf.apply_to_gains(dec!(3000));
        assert_eq!(taxable, dec!(0)); // All gains offset
        assert_eq!(cf.used, dec!(3000));
        assert_eq!(cf.remaining_balance, dec!(2000));
    }

    #[test]
    fn test_loss_carryforward_partial_application() {
        // €2,500 carryforward against €10,000 gain
        let (taxable, used, remaining) = calculate_with_loss_carryforward(dec!(10000), dec!(2500));

        assert_eq!(taxable, dec!(7500));
        assert_eq!(used, dec!(2500));
        assert_eq!(remaining, dec!(0));
    }

    #[test]
    fn test_loss_carryforward_decimal_precision() {
        // Test with cents precision
        let (taxable, used, remaining) =
            calculate_with_loss_carryforward(dec!(1234.56), dec!(500.25));

        assert_eq!(taxable, dec!(734.31));
        assert_eq!(used, dec!(500.25));
        assert_eq!(remaining, dec!(0));
    }
}

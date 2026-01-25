//! FIFO capital gains tracking for German tax calculations.
//!
//! German tax law requires FIFO (First-In-First-Out) method for determining
//! cost basis when selling securities.
//!
//! Note: These structures are designed for full FIFO tracking. The current
//! processor implementation uses a simplified cost basis calculation that
//! relies on the broker statement's existing trade matching.

use std::collections::VecDeque;

use crate::types::{Date, Decimal};

/// Represents a single purchase lot for FIFO tracking.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct FifoLot {
    /// Original purchase date
    pub purchase_date: Date,
    /// Settlement date of purchase
    pub settle_date: Date,
    /// Remaining shares in this lot
    pub quantity: Decimal,
    /// Cost basis per share in EUR
    pub cost_per_share_eur: Decimal,
    /// Whether this lot was purchased before 2009-01-01 (Altbestand - tax exempt)
    pub pre_2009: bool,
}

#[allow(dead_code)]
impl FifoLot {
    pub fn new(
        purchase_date: Date,
        settle_date: Date,
        quantity: Decimal,
        cost_per_share_eur: Decimal,
    ) -> Self {
        let pre_2009 = purchase_date < date!(2009, 1, 1);
        FifoLot {
            purchase_date,
            settle_date,
            quantity,
            cost_per_share_eur,
            pre_2009,
        }
    }

    /// Total cost basis for this lot
    pub fn total_cost(&self) -> Decimal {
        self.quantity * self.cost_per_share_eur
    }
}

/// FIFO queue for tracking cost basis of a single security.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct FifoQueue {
    lots: VecDeque<FifoLot>,
}

#[allow(dead_code)]
impl FifoQueue {
    pub fn new() -> Self {
        FifoQueue {
            lots: VecDeque::new(),
        }
    }

    /// Add a new purchase to the FIFO queue.
    pub fn add_purchase(&mut self, lot: FifoLot) {
        self.lots.push_back(lot);
    }

    /// Match a sale against the FIFO queue, returning matched lots and removing consumed shares.
    /// Returns the matched lots (may be partial) and total cost basis.
    pub fn match_sale(&mut self, quantity: Decimal) -> Vec<(FifoLot, Decimal)> {
        let mut remaining = quantity;
        let mut matched = Vec::new();

        while remaining > Decimal::ZERO && !self.lots.is_empty() {
            let lot = self.lots.front_mut().unwrap();

            if lot.quantity <= remaining {
                // Consume entire lot
                let consumed_qty = lot.quantity;
                remaining -= consumed_qty;
                let consumed_lot = self.lots.pop_front().unwrap();
                matched.push((consumed_lot, consumed_qty));
            } else {
                // Partial consumption
                let partial_lot = FifoLot {
                    purchase_date: lot.purchase_date,
                    settle_date: lot.settle_date,
                    quantity: remaining,
                    cost_per_share_eur: lot.cost_per_share_eur,
                    pre_2009: lot.pre_2009,
                };
                lot.quantity -= remaining;
                matched.push((partial_lot, remaining));
                remaining = Decimal::ZERO;
            }
        }

        matched
    }

    /// Get total quantity across all lots
    pub fn total_quantity(&self) -> Decimal {
        self.lots.iter().map(|lot| lot.quantity).sum()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.lots.is_empty()
    }
}

/// Result of a capital gain calculation.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CapitalGainResult {
    /// Total cost basis of matched lots
    pub cost_basis: Decimal,
    /// Proceeds from sale
    pub proceeds: Decimal,
    /// Gross gain/loss (proceeds - cost_basis)
    pub gross_gain_loss: Decimal,
    /// Whether any matched lot was pre-2009 (Altbestand)
    pub has_pre_2009_lots: bool,
    /// Matched lots with quantities
    pub matched_lots: Vec<(FifoLot, Decimal)>,
}

/// Calculate capital gain for a sale transaction using FIFO method.
///
/// # Arguments
/// * `queue` - FIFO queue for the security
/// * `quantity` - Number of shares sold
/// * `proceeds_per_share` - Sale price per share in EUR
///
/// # Returns
/// Capital gain result with cost basis, proceeds, and gain/loss
#[allow(dead_code)]
pub fn calculate_capital_gain(
    queue: &mut FifoQueue,
    quantity: Decimal,
    proceeds_per_share: Decimal,
) -> CapitalGainResult {
    let proceeds = quantity * proceeds_per_share;
    let matched = queue.match_sale(quantity);

    let cost_basis: Decimal = matched
        .iter()
        .map(|(lot, qty)| lot.cost_per_share_eur * qty)
        .sum();

    let has_pre_2009_lots = matched.iter().any(|(lot, _)| lot.pre_2009);

    CapitalGainResult {
        cost_basis,
        proceeds,
        gross_gain_loss: proceeds - cost_basis,
        has_pre_2009_lots,
        matched_lots: matched,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_basic() {
        let mut queue = FifoQueue::new();

        // Buy 100 shares at €10
        queue.add_purchase(FifoLot::new(
            date!(2020, 1, 1),
            date!(2020, 1, 3),
            dec!(100),
            dec!(10),
        ));

        // Buy 50 shares at €15
        queue.add_purchase(FifoLot::new(
            date!(2020, 6, 1),
            date!(2020, 6, 3),
            dec!(50),
            dec!(15),
        ));

        assert_eq!(queue.total_quantity(), dec!(150));

        // Sell 75 shares - should consume all of first lot (100) partially
        let matched = queue.match_sale(dec!(75));
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].1, dec!(75)); // quantity sold from first lot
        assert_eq!(matched[0].0.cost_per_share_eur, dec!(10)); // cost from first lot

        // Remaining should be 75 (25 from first lot + 50 from second)
        assert_eq!(queue.total_quantity(), dec!(75));
    }

    #[test]
    fn test_fifo_pre_2009() {
        let mut queue = FifoQueue::new();

        // Buy before 2009 (Altbestand)
        queue.add_purchase(FifoLot::new(
            date!(2008, 6, 1),
            date!(2008, 6, 3),
            dec!(100),
            dec!(50),
        ));

        let matched = queue.match_sale(dec!(50));
        assert!(matched[0].0.pre_2009);
    }

    #[test]
    fn test_calculate_capital_gain() {
        let mut queue = FifoQueue::new();

        // Buy 100 shares at €10
        queue.add_purchase(FifoLot::new(
            date!(2020, 1, 1),
            date!(2020, 1, 3),
            dec!(100),
            dec!(10),
        ));

        // Sell 50 shares at €20 each
        let result = calculate_capital_gain(&mut queue, dec!(50), dec!(20));

        assert_eq!(result.cost_basis, dec!(500)); // 50 * €10
        assert_eq!(result.proceeds, dec!(1000)); // 50 * €20
        assert_eq!(result.gross_gain_loss, dec!(500)); // €1000 - €500
        assert!(!result.has_pre_2009_lots);
    }

    #[test]
    fn test_calculate_capital_gain_with_loss() {
        let mut queue = FifoQueue::new();

        // Buy 100 shares at €20
        queue.add_purchase(FifoLot::new(
            date!(2020, 1, 1),
            date!(2020, 1, 3),
            dec!(100),
            dec!(20),
        ));

        // Sell 50 shares at €15 each (loss)
        let result = calculate_capital_gain(&mut queue, dec!(50), dec!(15));

        assert_eq!(result.cost_basis, dec!(1000)); // 50 * €20
        assert_eq!(result.proceeds, dec!(750)); // 50 * €15
        assert_eq!(result.gross_gain_loss, dec!(-250)); // €750 - €1000
    }

    // T087: Test mixed ETF Teilfreistellung on capital gains
    // €1,000 gain × 15% exempt = €150 exempt, €850 taxable
    #[test]
    fn test_capital_gain_with_mixed_etf_teilfreistellung() {
        use crate::taxes::germany::dividends::TeilfreistellungRate;

        let mut queue = FifoQueue::new();

        // Buy 100 shares at €10 (Mixed ETF)
        queue.add_purchase(FifoLot::new(
            date!(2020, 1, 1),
            date!(2020, 1, 3),
            dec!(100),
            dec!(10),
        ));

        // Sell 100 shares at €20 each = €1000 gain
        let result = calculate_capital_gain(&mut queue, dec!(100), dec!(20));

        assert_eq!(result.proceeds, dec!(2000)); // 100 * €20
        assert_eq!(result.cost_basis, dec!(1000)); // 100 * €10
        assert_eq!(result.gross_gain_loss, dec!(1000)); // €2000 - €1000

        // Apply 15% Teilfreistellung to capital gain
        let teilfreistellung = TeilfreistellungRate::Mixed;
        let taxable_gain = result.gross_gain_loss * teilfreistellung.taxable_portion();

        // 15% exempt = €150 exempt, €850 taxable
        assert_eq!(taxable_gain, dec!(850));
    }

    // T087: Test equity ETF Teilfreistellung on capital gains
    // €1,000 gain × 30% exempt = €300 exempt, €700 taxable
    #[test]
    fn test_capital_gain_with_equity_etf_teilfreistellung() {
        use crate::taxes::germany::dividends::TeilfreistellungRate;

        let mut queue = FifoQueue::new();

        // Buy 100 shares at €10 (Equity ETF)
        queue.add_purchase(FifoLot::new(
            date!(2020, 1, 1),
            date!(2020, 1, 3),
            dec!(100),
            dec!(10),
        ));

        // Sell 100 shares at €20 each = €1000 gain
        let result = calculate_capital_gain(&mut queue, dec!(100), dec!(20));

        assert_eq!(result.gross_gain_loss, dec!(1000));

        // Apply 30% Teilfreistellung to capital gain
        let teilfreistellung = TeilfreistellungRate::Equity;
        let taxable_gain = result.gross_gain_loss * teilfreistellung.taxable_portion();

        // 30% exempt = €300 exempt, €700 taxable
        assert_eq!(taxable_gain, dec!(700));
    }

    // T087: Test no Teilfreistellung for regular stocks
    #[test]
    fn test_capital_gain_no_teilfreistellung_for_stocks() {
        use crate::taxes::germany::dividends::TeilfreistellungRate;

        let mut queue = FifoQueue::new();

        // Buy 100 shares at €10 (Regular stock)
        queue.add_purchase(FifoLot::new(
            date!(2020, 1, 1),
            date!(2020, 1, 3),
            dec!(100),
            dec!(10),
        ));

        // Sell 100 shares at €20 each = €1000 gain
        let result = calculate_capital_gain(&mut queue, dec!(100), dec!(20));

        // Apply 0% Teilfreistellung (regular stock)
        let teilfreistellung = TeilfreistellungRate::None;
        let taxable_gain = result.gross_gain_loss * teilfreistellung.taxable_portion();

        // 0% exempt = full €1000 taxable
        assert_eq!(taxable_gain, dec!(1000));
    }
}

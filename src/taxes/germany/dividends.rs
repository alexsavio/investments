//! German dividend tax calculation with Teilfreistellung.
//!
//! Implements:
//! - Teilfreistellung (partial exemption) for ETFs
//! - Foreign tax credit calculation under double taxation treaties

use crate::types::Decimal;

/// Teilfreistellung (partial exemption) rates for different fund types.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TeilfreistellungRate {
    /// Regular stocks - no exemption
    #[default]
    None,
    /// Equity ETFs - 30% exemption
    Equity,
    /// Mixed ETFs - 15% exemption
    Mixed,
    /// Bond ETFs - 0% exemption (same as None)
    Bond,
}

impl TeilfreistellungRate {
    /// Returns the exemption rate as a decimal (0.0, 0.15, or 0.30)
    pub fn rate(&self) -> Decimal {
        match self {
            TeilfreistellungRate::None => dec!(0),
            TeilfreistellungRate::Equity => dec!(0.30),
            TeilfreistellungRate::Mixed => dec!(0.15),
            TeilfreistellungRate::Bond => dec!(0),
        }
    }

    /// Returns the taxable portion after applying exemption (1 - rate)
    pub fn taxable_portion(&self) -> Decimal {
        dec!(1) - self.rate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teilfreistellung_rates() {
        assert_eq!(TeilfreistellungRate::None.rate(), dec!(0));
        assert_eq!(TeilfreistellungRate::Equity.rate(), dec!(0.30));
        assert_eq!(TeilfreistellungRate::Mixed.rate(), dec!(0.15));
        assert_eq!(TeilfreistellungRate::Bond.rate(), dec!(0));

        assert_eq!(TeilfreistellungRate::None.taxable_portion(), dec!(1));
        assert_eq!(TeilfreistellungRate::Equity.taxable_portion(), dec!(0.70));
        assert_eq!(TeilfreistellungRate::Mixed.taxable_portion(), dec!(0.85));
        assert_eq!(TeilfreistellungRate::Bond.taxable_portion(), dec!(1));
    }
}

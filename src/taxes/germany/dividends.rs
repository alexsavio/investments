//! German dividend tax calculation with Teilfreistellung.
//!
//! Implements:
//! - Teilfreistellung (partial exemption) for ETFs
//! - Foreign tax credit calculation under double taxation treaties

use crate::types::Decimal;

/// Teilfreistellung (partial exemption) rates for different fund types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TeilfreistellungRate {
    /// Regular stocks - no exemption
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

impl Default for TeilfreistellungRate {
    fn default() -> Self {
        TeilfreistellungRate::None
    }
}

/// Calculate dividend tax with Teilfreistellung and foreign tax credit.
///
/// # Arguments
/// * `gross_amount_eur` - Gross dividend amount in EUR
/// * `foreign_withholding_tax` - Tax withheld by foreign broker in EUR
/// * `teilfreistellung` - Exemption rate to apply
/// * `tax_rates` - German tax rates (Abgeltungssteuer, Soli, Kirchensteuer)
///
/// # Returns
/// Tuple of (taxable_amount, german_tax, foreign_tax_credit, net_tax)
pub fn calculate_dividend_tax(
    gross_amount_eur: Decimal,
    foreign_withholding_tax: Decimal,
    teilfreistellung: TeilfreistellungRate,
    abgeltungssteuer_rate: Decimal,
    soli_rate: Decimal,
    church_tax_rate: Decimal,
) -> (Decimal, Decimal, Decimal, Decimal) {
    // Apply Teilfreistellung exemption
    let taxable_amount = gross_amount_eur * teilfreistellung.taxable_portion();

    // Calculate German taxes
    let abgeltungssteuer = taxable_amount * abgeltungssteuer_rate;
    let soli = abgeltungssteuer * soli_rate;
    let church_tax = abgeltungssteuer * church_tax_rate;
    let total_german_tax = abgeltungssteuer + soli + church_tax;

    // Foreign tax credit is limited to German tax amount
    let foreign_tax_credit = foreign_withholding_tax.min(total_german_tax);

    // Net tax after credit
    let net_tax = total_german_tax - foreign_tax_credit;

    (
        taxable_amount,
        total_german_tax,
        foreign_tax_credit,
        net_tax,
    )
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
    }

    #[test]
    fn test_dividend_tax_no_exemption() {
        // €1000 dividend, no Teilfreistellung, 15% US withholding
        let (taxable, german_tax, credit, net) = calculate_dividend_tax(
            dec!(1000),
            dec!(150), // 15% US withholding
            TeilfreistellungRate::None,
            dec!(0.25),  // 25% Abgeltungssteuer
            dec!(0.055), // 5.5% Soli
            dec!(0),     // No church tax
        );

        assert_eq!(taxable, dec!(1000));
        // 25% * 1000 = 250, plus 5.5% Soli = 250 * 0.055 = 13.75
        // Total = 263.75
        assert_eq!(german_tax, dec!(263.75));
        assert_eq!(credit, dec!(150)); // Full credit (less than German tax)
        assert_eq!(net, dec!(113.75)); // 263.75 - 150
    }

    #[test]
    fn test_dividend_tax_with_equity_etf() {
        // €1000 dividend from equity ETF (30% exemption)
        let (taxable, german_tax, credit, net) = calculate_dividend_tax(
            dec!(1000),
            dec!(0), // No foreign tax
            TeilfreistellungRate::Equity,
            dec!(0.25),
            dec!(0.055),
            dec!(0),
        );

        // 30% exempt, so 70% taxable = €700
        assert_eq!(taxable, dec!(700));
        // 25% * 700 = 175, plus 5.5% Soli = 175 * 0.055 = 9.625
        assert_eq!(german_tax, dec!(184.625));
        assert_eq!(credit, dec!(0));
        assert_eq!(net, dec!(184.625));
    }

    #[test]
    fn test_foreign_tax_credit_limit() {
        // Foreign tax exceeds German tax - credit should be limited
        let (_, german_tax, credit, net) = calculate_dividend_tax(
            dec!(100),
            dec!(50), // 50% foreign tax (more than German tax)
            TeilfreistellungRate::None,
            dec!(0.25),
            dec!(0.055),
            dec!(0),
        );

        // German tax = 100 * 0.25 * 1.055 = 26.375
        assert_eq!(german_tax, dec!(26.375));
        // Credit limited to German tax
        assert_eq!(credit, dec!(26.375));
        // Net tax = 0 (fully offset by foreign credit)
        assert_eq!(net, dec!(0));
    }
}

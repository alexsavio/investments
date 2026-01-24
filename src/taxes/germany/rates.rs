//! German tax rate calculations.
//!
//! Implements Abgeltungssteuer (25%), Solidaritätszuschlag (5.5%), and Kirchensteuer.

use crate::types::Decimal;

/// German tax rates configuration.
#[derive(Debug, Clone)]
pub struct GermanTaxRates {
    /// Abgeltungssteuer rate (25% since 2009)
    pub abgeltungssteuer: Decimal,
    /// Solidaritätszuschlag rate (5.5% of Abgeltungssteuer)
    pub solidaritaetszuschlag: Decimal,
    /// Kirchensteuer rate (0%, 8%, or 9% depending on region and religious affiliation)
    pub kirchensteuer: Decimal,
}

impl GermanTaxRates {
    /// Create tax rates for a given year.
    /// Note: Abgeltungssteuer has been 25% since 2009. Church tax varies by user.
    pub fn for_year(year: i32, church_tax_rate: Decimal) -> Self {
        if year < 2009 {
            // Before Abgeltungssteuer - different rules applied
            // For now, we don't support pre-2009 calculations
            panic!("German Abgeltungssteuer only applies from 2009 onwards");
        }

        GermanTaxRates {
            abgeltungssteuer: dec!(0.25),
            solidaritaetszuschlag: dec!(0.055),
            kirchensteuer: church_tax_rate,
        }
    }

    /// Create default rates without church tax.
    pub fn default_for_year(year: i32) -> Self {
        Self::for_year(year, dec!(0))
    }

    /// Calculate effective total tax rate including all components.
    /// Formula: abgeltungssteuer * (1 + solidaritaetszuschlag + kirchensteuer)
    pub fn effective_rate(&self) -> Decimal {
        self.abgeltungssteuer * (dec!(1) + self.solidaritaetszuschlag + self.kirchensteuer)
    }
}

impl Default for GermanTaxRates {
    fn default() -> Self {
        GermanTaxRates {
            abgeltungssteuer: dec!(0.25),
            solidaritaetszuschlag: dec!(0.055),
            kirchensteuer: dec!(0),
        }
    }
}

/// Calculate German taxes on a taxable amount.
///
/// # Arguments
/// * `taxable_amount` - Amount subject to taxation in EUR
/// * `rates` - Tax rates to apply
///
/// # Returns
/// Tuple of (abgeltungssteuer, solidaritaetszuschlag, kirchensteuer, total)
pub fn calculate_german_taxes(
    taxable_amount: Decimal,
    rates: &GermanTaxRates,
) -> (Decimal, Decimal, Decimal, Decimal) {
    let abgeltungssteuer = taxable_amount * rates.abgeltungssteuer;
    let solidaritaetszuschlag = abgeltungssteuer * rates.solidaritaetszuschlag;
    let kirchensteuer = abgeltungssteuer * rates.kirchensteuer;
    let total = abgeltungssteuer + solidaritaetszuschlag + kirchensteuer;

    (
        abgeltungssteuer,
        solidaritaetszuschlag,
        kirchensteuer,
        total,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rates() {
        let rates = GermanTaxRates::default();
        assert_eq!(rates.abgeltungssteuer, dec!(0.25));
        assert_eq!(rates.solidaritaetszuschlag, dec!(0.055));
        assert_eq!(rates.kirchensteuer, dec!(0));
    }

    #[test]
    fn test_effective_rate_no_church_tax() {
        let rates = GermanTaxRates::default_for_year(2025);
        // 25% * (1 + 5.5%) = 25% * 1.055 = 26.375%
        assert_eq!(rates.effective_rate(), dec!(0.26375));
    }

    #[test]
    fn test_effective_rate_with_church_tax() {
        let rates = GermanTaxRates::for_year(2025, dec!(0.08)); // 8% church tax
        // 25% * (1 + 5.5% + 8%) = 25% * 1.135 = 28.375%
        assert_eq!(rates.effective_rate(), dec!(0.28375));
    }

    #[test]
    fn test_tax_calculation() {
        let rates = GermanTaxRates::default();
        let (abgelt, soli, kirchen, total) = calculate_german_taxes(dec!(1000), &rates);

        assert_eq!(abgelt, dec!(250)); // 25% of 1000
        assert_eq!(soli, dec!(13.75)); // 5.5% of 250
        assert_eq!(kirchen, dec!(0)); // No church tax
        assert_eq!(total, dec!(263.75)); // Sum
    }

    #[test]
    fn test_tax_calculation_with_church_tax() {
        let rates = GermanTaxRates::for_year(2025, dec!(0.09)); // 9% church tax (Bavaria)
        let (abgelt, soli, kirchen, total) = calculate_german_taxes(dec!(1000), &rates);

        assert_eq!(abgelt, dec!(250)); // 25% of 1000
        assert_eq!(soli, dec!(13.75)); // 5.5% of 250
        assert_eq!(kirchen, dec!(22.5)); // 9% of 250
        assert_eq!(total, dec!(286.25)); // Sum
    }
}

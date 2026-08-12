//! German tax rate calculations.
//!
//! Implements Abgeltungssteuer (25%), Solidaritätszuschlag (5.5%), and Kirchensteuer.

use crate::core::GenericResult;
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

/// Per-item (or aggregate) result of the §32d(1) EStG flat-tax computation, all in EUR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbgeltungsteuerBreakdown {
    pub abgeltungssteuer: Decimal,
    pub solidaritaetszuschlag: Decimal,
    pub kirchensteuer: Decimal,
    pub total: Decimal,
}

impl GermanTaxRates {
    /// Create tax rates for a given year.
    /// Note: Abgeltungssteuer has been 25% since 2009. Church tax varies by user.
    pub fn for_year(year: i32, church_tax_rate: Decimal) -> GenericResult<Self> {
        if year < 2009 {
            return Err!("German Abgeltungssteuer only applies from 2009 onwards (year: {year})");
        }

        Ok(GermanTaxRates {
            abgeltungssteuer: dec!(0.25),
            solidaritaetszuschlag: dec!(0.055),
            kirchensteuer: church_tax_rate,
        })
    }

    /// Create default rates without church tax.
    pub fn default_for_year(year: i32) -> GenericResult<Self> {
        Self::for_year(year, dec!(0))
    }

    /// Flat capital-income tax under §32d(1) EStG.
    ///
    /// Church tax is deductible as a Sonderausgabe *inside* the flat-rate formula, and creditable
    /// foreign tax `q` (§32d(5), already capped by the caller) is credited *before* Soli/KiSt:
    ///
    /// ```text
    /// abgeltungsteuer = max(0, taxable − 4·q) / (4 + k)   // 4 = 1 / 0.25, k = church fraction
    /// ```
    ///
    /// Results are full-precision; round once per reported figure at output (half-up).
    pub fn compute_taxes(
        &self,
        taxable: Decimal,
        creditable_foreign_tax: Decimal,
    ) -> AbgeltungsteuerBreakdown {
        // 4 is the reciprocal of the 25% flat rate; both 4's in (e − 4q)/(4 + k) are 1/0.25.
        let four = dec!(4);
        let base = (taxable - four * creditable_foreign_tax).max(dec!(0));
        let abgeltungssteuer = base / (four + self.kirchensteuer);
        let kirchensteuer = abgeltungssteuer * self.kirchensteuer;
        let solidaritaetszuschlag = abgeltungssteuer * self.solidaritaetszuschlag;
        let total = abgeltungssteuer + kirchensteuer + solidaritaetszuschlag;

        AbgeltungsteuerBreakdown {
            abgeltungssteuer,
            solidaritaetszuschlag,
            kirchensteuer,
            total,
        }
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

#[cfg(test)]
mod tests {
    use rust_decimal::RoundingStrategy;

    use super::*;

    /// Round to cents half-up (German per-line tax-form practice) so the full-precision §32d(1)
    /// outputs can be compared against the Appendix A worked examples.
    fn round2(value: Decimal) -> Decimal {
        value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
    }

    #[test]
    fn test_default_rates() {
        let rates = GermanTaxRates::default();
        assert_eq!(rates.abgeltungssteuer, dec!(0.25));
        assert_eq!(rates.solidaritaetszuschlag, dec!(0.055));
        assert_eq!(rates.kirchensteuer, dec!(0));
    }

    #[test]
    fn for_year_rejects_pre_2009() {
        assert!(GermanTaxRates::for_year(2008, dec!(0)).is_err());
        assert!(GermanTaxRates::for_year(2009, dec!(0)).is_ok());
    }

    // §32d(1) EStG worked examples (Appendix A): full-precision results compared at cent scale.

    #[test]
    fn abgeltungsteuer_no_church_no_credit() {
        let rates = GermanTaxRates::default_for_year(2025).unwrap();
        let t = rates.compute_taxes(dec!(1000), dec!(0));
        assert_eq!(round2(t.abgeltungssteuer), dec!(250.00));
        assert_eq!(round2(t.kirchensteuer), dec!(0.00));
        assert_eq!(round2(t.solidaritaetszuschlag), dec!(13.75));
        assert_eq!(round2(t.total), dec!(263.75));
    }

    #[test]
    fn abgeltungsteuer_church_9pct() {
        // 9% applies outside Bavaria and Baden-Württemberg.
        let rates = GermanTaxRates::for_year(2025, dec!(0.09)).unwrap();
        let t = rates.compute_taxes(dec!(1000), dec!(0));
        assert_eq!(round2(t.abgeltungssteuer), dec!(244.50));
        assert_eq!(round2(t.kirchensteuer), dec!(22.00));
        assert_eq!(round2(t.solidaritaetszuschlag), dec!(13.45));
        assert_eq!(round2(t.total), dec!(279.95));
    }

    #[test]
    fn abgeltungsteuer_church_8pct() {
        // 8% church tax (Bavaria, Baden-Württemberg).
        let rates = GermanTaxRates::for_year(2025, dec!(0.08)).unwrap();
        let t = rates.compute_taxes(dec!(1000), dec!(0));
        assert_eq!(round2(t.abgeltungssteuer), dec!(245.10));
        assert_eq!(round2(t.kirchensteuer), dec!(19.61));
        assert_eq!(round2(t.solidaritaetszuschlag), dec!(13.48));
        assert_eq!(round2(t.total), dec!(278.19));
    }

    #[test]
    fn abgeltungsteuer_credits_foreign_tax_before_soli() {
        // €1,000 US dividend with €150 creditable WHT, no church tax:
        // (1000 − 4·150)/4 = 100 Abgeltungsteuer, Soli 5.50, total 105.50.
        let rates = GermanTaxRates::default_for_year(2025).unwrap();
        let t = rates.compute_taxes(dec!(1000), dec!(150));
        assert_eq!(round2(t.abgeltungssteuer), dec!(100.00));
        assert_eq!(round2(t.kirchensteuer), dec!(0.00));
        assert_eq!(round2(t.solidaritaetszuschlag), dec!(5.50));
        assert_eq!(round2(t.total), dec!(105.50));
    }

    #[test]
    fn abgeltungsteuer_floors_at_zero() {
        // Credit larger than the tax base cannot produce a negative tax.
        let rates = GermanTaxRates::default_for_year(2025).unwrap();
        let t = rates.compute_taxes(dec!(100), dec!(50));
        assert_eq!(t.abgeltungssteuer, dec!(0));
        assert_eq!(t.total, dec!(0));
    }
}

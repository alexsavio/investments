//! German tax statement data structures.
//!
//! Contains the main GermanTaxStatement struct and entry types for capital gains,
//! dividends, and interest.

use crate::taxes::germany::{GermanTaxRates, TeilfreistellungRate};
use crate::types::{Date, Decimal};

/// Entry for a capital gain/loss transaction.
#[derive(Debug, Clone)]
pub struct CapitalGainEntry {
    pub transaction_date: Date,
    pub settle_date: Date,
    pub symbol: String,
    pub isin: String,
    pub description: String,
    pub quantity: Decimal,
    pub cost_basis_eur: Decimal,
    pub proceeds_eur: Decimal,
    pub gross_gain_loss: Decimal,
    pub teilfreistellung_rate: TeilfreistellungRate,
    pub taxable_amount: Decimal,
    pub foreign_tax: Decimal,
    pub abgeltungssteuer: Decimal,
    pub solidaritaetszuschlag: Decimal,
    pub kirchensteuer: Decimal,
    pub total_tax: Decimal,
    pub pre_2009_holding: bool,
    pub notes: Option<String>,
}

/// Entry for dividend income.
#[derive(Debug, Clone)]
pub struct DividendEntry {
    pub payment_date: Date,
    pub symbol: String,
    pub isin: String,
    pub description: String,
    pub quantity: Decimal,
    pub gross_amount_eur: Decimal,
    pub foreign_withholding_tax: Decimal,
    pub teilfreistellung_rate: TeilfreistellungRate,
    pub taxable_amount: Decimal,
    pub abgeltungssteuer: Decimal,
    pub solidaritaetszuschlag: Decimal,
    pub kirchensteuer: Decimal,
    pub foreign_tax_credit: Decimal,
    pub total_tax: Decimal,
    pub net_tax: Decimal,
    pub notes: Option<String>,
}

/// Entry for interest income.
#[derive(Debug, Clone)]
pub struct InterestEntry {
    pub payment_date: Date,
    pub description: String,
    pub gross_amount_eur: Decimal,
    pub foreign_withholding_tax: Decimal,
    pub taxable_amount: Decimal,
    pub abgeltungssteuer: Decimal,
    pub solidaritaetszuschlag: Decimal,
    pub kirchensteuer: Decimal,
    pub foreign_tax_credit: Decimal,
    pub total_tax: Decimal,
    pub net_tax: Decimal,
    pub notes: Option<String>,
}

/// Complete German tax statement for a single tax year.
#[derive(Debug)]
pub struct GermanTaxStatement {
    pub year: i32,
    pub tax_rates: GermanTaxRates,

    pub capital_gains: Vec<CapitalGainEntry>,
    pub dividends: Vec<DividendEntry>,
    pub interest: Vec<InterestEntry>,

    // Loss carryforward tracking
    pub loss_carryforward_used: Decimal,
    pub loss_carryforward_remaining: Decimal,

    // Summary totals (calculated)
    pub total_capital_gains: Decimal,
    pub total_capital_losses: Decimal,
    pub total_dividend_income: Decimal,
    pub total_interest_income: Decimal,
    pub total_taxable_income: Decimal,
    pub total_foreign_tax: Decimal,
    pub total_abgeltungssteuer: Decimal,
    pub total_solidaritaetszuschlag: Decimal,
    pub total_kirchensteuer: Decimal,
    pub total_german_tax: Decimal,
    pub total_foreign_tax_credit: Decimal,
    pub net_tax_due: Decimal,
}

impl GermanTaxStatement {
    /// Create a new German tax statement for the given year.
    pub fn new(year: i32, church_tax_rate: Decimal, loss_carryforward: Decimal) -> Self {
        GermanTaxStatement {
            year,
            tax_rates: GermanTaxRates::for_year(year, church_tax_rate),

            capital_gains: Vec::new(),
            dividends: Vec::new(),
            interest: Vec::new(),

            loss_carryforward_used: dec!(0),
            loss_carryforward_remaining: loss_carryforward,

            total_capital_gains: dec!(0),
            total_capital_losses: dec!(0),
            total_dividend_income: dec!(0),
            total_interest_income: dec!(0),
            total_taxable_income: dec!(0),
            total_foreign_tax: dec!(0),
            total_abgeltungssteuer: dec!(0),
            total_solidaritaetszuschlag: dec!(0),
            total_kirchensteuer: dec!(0),
            total_german_tax: dec!(0),
            total_foreign_tax_credit: dec!(0),
            net_tax_due: dec!(0),
        }
    }

    /// Add a capital gain entry.
    pub fn add_capital_gain(&mut self, entry: CapitalGainEntry) {
        self.capital_gains.push(entry);
    }

    /// Add a dividend entry.
    pub fn add_dividend(&mut self, entry: DividendEntry) {
        self.dividends.push(entry);
    }

    /// Add an interest entry.
    pub fn add_interest(&mut self, entry: InterestEntry) {
        self.interest.push(entry);
    }

    /// Calculate all summary totals.
    pub fn calculate_totals(&mut self) {
        // Reset totals
        self.total_capital_gains = dec!(0);
        self.total_capital_losses = dec!(0);
        self.total_dividend_income = dec!(0);
        self.total_interest_income = dec!(0);
        self.total_foreign_tax = dec!(0);
        self.total_abgeltungssteuer = dec!(0);
        self.total_solidaritaetszuschlag = dec!(0);
        self.total_kirchensteuer = dec!(0);
        self.total_foreign_tax_credit = dec!(0);

        // Sum capital gains
        for entry in &self.capital_gains {
            if entry.gross_gain_loss >= dec!(0) {
                self.total_capital_gains += entry.taxable_amount;
            } else {
                self.total_capital_losses += entry.taxable_amount.abs();
            }
            self.total_foreign_tax += entry.foreign_tax;
            self.total_abgeltungssteuer += entry.abgeltungssteuer;
            self.total_solidaritaetszuschlag += entry.solidaritaetszuschlag;
            self.total_kirchensteuer += entry.kirchensteuer;
        }

        // Sum dividends
        for entry in &self.dividends {
            self.total_dividend_income += entry.taxable_amount;
            self.total_foreign_tax += entry.foreign_withholding_tax;
            self.total_abgeltungssteuer += entry.abgeltungssteuer;
            self.total_solidaritaetszuschlag += entry.solidaritaetszuschlag;
            self.total_kirchensteuer += entry.kirchensteuer;
            self.total_foreign_tax_credit += entry.foreign_tax_credit;
        }

        // Sum interest
        for entry in &self.interest {
            self.total_interest_income += entry.taxable_amount;
            self.total_foreign_tax += entry.foreign_withholding_tax;
            self.total_abgeltungssteuer += entry.abgeltungssteuer;
            self.total_solidaritaetszuschlag += entry.solidaritaetszuschlag;
            self.total_kirchensteuer += entry.kirchensteuer;
            self.total_foreign_tax_credit += entry.foreign_tax_credit;
        }

        // Calculate final totals
        self.total_taxable_income =
            self.total_capital_gains + self.total_dividend_income + self.total_interest_income
                - self.total_capital_losses
                - self.loss_carryforward_used;

        self.total_german_tax = self.total_abgeltungssteuer
            + self.total_solidaritaetszuschlag
            + self.total_kirchensteuer;

        self.net_tax_due = self.total_german_tax - self.total_foreign_tax_credit;

        // Ensure net tax is not negative
        if self.net_tax_due < dec!(0) {
            self.net_tax_due = dec!(0);
        }
    }
}

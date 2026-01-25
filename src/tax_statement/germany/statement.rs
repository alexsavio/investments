//! German tax statement data structures.
//!
//! Contains the main GermanTaxStatement struct and entry types for capital gains,
//! dividends, interest, and FX gains/losses.

use crate::taxes::germany::{
    GermanTaxRates, TeilfreistellungRate, calculate_with_loss_carryforward,
};
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

/// Entry for FX (foreign currency) gain/loss.
///
/// For German tax purposes:
/// - FX gains on interest-bearing currency accounts (like IBKR) fall under §20 EStG
/// - They are taxed as capital income (Abgeltungsteuer)
/// - Every forex transaction realizes a gain/loss based on exchange rate differences
/// - Losses can offset other capital income in the general loss bucket
#[derive(Debug, Clone)]
pub struct FxGainEntry {
    pub transaction_date: Date,
    pub currency_pair: String, // e.g., "EUR.USD"
    pub description: String,
    pub gross_amount_eur: Decimal, // Positive = gain, negative = loss
    pub taxable_amount: Decimal,
    pub abgeltungssteuer: Decimal,
    pub solidaritaetszuschlag: Decimal,
    pub kirchensteuer: Decimal,
    pub total_tax: Decimal,
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
    pub fx_gains: Vec<FxGainEntry>,

    // Loss carryforward tracking
    pub loss_carryforward_used: Decimal,
    pub loss_carryforward_remaining: Decimal,

    // Summary totals (calculated)
    pub total_capital_gains: Decimal,
    pub total_capital_losses: Decimal,
    pub total_dividend_income: Decimal,
    pub total_interest_income: Decimal,
    pub total_fx_gains: Decimal,
    pub total_fx_losses: Decimal,
    pub total_taxable_income: Decimal,
    pub total_foreign_tax: Decimal,
    pub total_abgeltungssteuer: Decimal,
    pub total_solidaritaetszuschlag: Decimal,
    pub total_kirchensteuer: Decimal,
    pub total_german_tax: Decimal,
    pub total_foreign_tax_credit: Decimal,
    pub net_tax_due: Decimal,

    // Non-taxable amounts (for information/reporting)
    /// FX gains/losses from margin loan repayments (not taxable - Tilgung Fremdwährungskredit)
    pub non_taxable_margin_fx: Decimal,

    // Anlage KAP form line values
    // These map to specific lines on the German tax form "Anlage KAP"
    /// KAP Zeile 19: Foreign capital income (dividends, interest, FX gains from abroad)
    pub kap_zeile_19: Decimal,
    /// KAP Zeile 22: Losses from non-stock capital income (FX losses, interest losses)
    pub kap_zeile_22: Decimal,
    /// KAP Zeile 23: Losses from stock sales
    pub kap_zeile_23: Decimal,
    /// KAP Zeile 41: Creditable foreign withholding tax (anrechenbare ausländische Quellensteuer)
    pub kap_zeile_41: Decimal,
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
            fx_gains: Vec::new(),

            loss_carryforward_used: dec!(0),
            loss_carryforward_remaining: loss_carryforward,

            total_capital_gains: dec!(0),
            total_capital_losses: dec!(0),
            total_dividend_income: dec!(0),
            total_interest_income: dec!(0),
            total_fx_gains: dec!(0),
            total_fx_losses: dec!(0),
            total_taxable_income: dec!(0),
            total_foreign_tax: dec!(0),
            total_abgeltungssteuer: dec!(0),
            total_solidaritaetszuschlag: dec!(0),
            total_kirchensteuer: dec!(0),
            total_german_tax: dec!(0),
            total_foreign_tax_credit: dec!(0),
            net_tax_due: dec!(0),

            non_taxable_margin_fx: dec!(0),

            // KAP line values (calculated)
            kap_zeile_19: dec!(0),
            kap_zeile_22: dec!(0),
            kap_zeile_23: dec!(0),
            kap_zeile_41: dec!(0),
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

    /// Add an FX gain/loss entry.
    pub fn add_fx_gain(&mut self, entry: FxGainEntry) {
        self.fx_gains.push(entry);
    }

    /// Calculate all summary totals.
    pub fn calculate_totals(&mut self) {
        // Reset totals
        self.total_capital_gains = dec!(0);
        self.total_capital_losses = dec!(0);
        self.total_dividend_income = dec!(0);
        self.total_interest_income = dec!(0);
        self.total_fx_gains = dec!(0);
        self.total_fx_losses = dec!(0);
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

        // Sum FX gains/losses
        // FX gains on interest-bearing accounts (§20 EStG) are capital income
        // and can be offset against other capital income
        for entry in &self.fx_gains {
            if entry.gross_amount_eur >= dec!(0) {
                self.total_fx_gains += entry.taxable_amount;
            } else {
                self.total_fx_losses += entry.taxable_amount.abs();
            }
            self.total_abgeltungssteuer += entry.abgeltungssteuer;
            self.total_solidaritaetszuschlag += entry.solidaritaetszuschlag;
            self.total_kirchensteuer += entry.kirchensteuer;
        }

        // Calculate net capital gain/loss for carryforward calculation
        // Note: FX losses from interest-bearing accounts can be included in the general loss bucket
        let net_capital_gain_loss = self.total_capital_gains - self.total_capital_losses
            + self.total_fx_gains
            - self.total_fx_losses;

        // Apply loss carryforward to capital gains
        // Note: Loss carryforward only applies to capital gains, not dividends or interest
        let previous_carryforward = self.loss_carryforward_remaining;
        let (taxable_capital_gains, cf_used, cf_remaining) =
            calculate_with_loss_carryforward(net_capital_gain_loss, previous_carryforward);

        self.loss_carryforward_used = cf_used;
        self.loss_carryforward_remaining = cf_remaining;

        // Calculate final totals
        self.total_taxable_income =
            taxable_capital_gains + self.total_dividend_income + self.total_interest_income;

        self.total_german_tax = self.total_abgeltungssteuer
            + self.total_solidaritaetszuschlag
            + self.total_kirchensteuer;

        self.net_tax_due = self.total_german_tax - self.total_foreign_tax_credit;

        // Ensure net tax is not negative
        if self.net_tax_due < dec!(0) {
            self.net_tax_due = dec!(0);
        }

        // Calculate Anlage KAP form line values
        //
        // KAP Zeile 19: Foreign capital income from abroad (ausländische Kapitalerträge)
        // This is the GROSS income before loss offsetting
        // Includes: dividends, interest, FX gains, and capital gains (all from foreign sources)
        // Note: For IBKR accounts, all income is considered "foreign" (ausländisch)
        // Note: This uses taxable amounts (after Teilfreistellung) not gross amounts

        // Sum gross dividend income
        let gross_dividend_income: Decimal =
            self.dividends.iter().map(|e| e.gross_amount_eur).sum();

        // Sum gross interest income
        let gross_interest_income: Decimal = self.interest.iter().map(|e| e.gross_amount_eur).sum();

        // Sum gross capital gains (only gains, not losses)
        let gross_capital_gains: Decimal = self
            .capital_gains
            .iter()
            .filter(|e| e.gross_gain_loss > dec!(0))
            .map(|e| e.gross_gain_loss)
            .sum();

        // Sum gross FX gains (only gains, not losses)
        let gross_fx_gains: Decimal = self
            .fx_gains
            .iter()
            .filter(|e| e.gross_amount_eur > dec!(0))
            .map(|e| e.gross_amount_eur)
            .sum();

        self.kap_zeile_19 =
            gross_dividend_income + gross_interest_income + gross_fx_gains + gross_capital_gains;

        // KAP Zeile 22: Losses from non-stock capital transactions (sonstige Verluste)
        // This includes: FX losses (§20 Abs. 2 Nr. 7 EStG) - but NOT stock losses
        // Stock sale losses go to Zeile 23 and have restricted offsetting rules
        let gross_fx_losses: Decimal = self
            .fx_gains
            .iter()
            .filter(|e| e.gross_amount_eur < dec!(0))
            .map(|e| e.gross_amount_eur.abs())
            .sum();
        self.kap_zeile_22 = gross_fx_losses;

        // KAP Zeile 23: Losses from stock sales (Aktien-Verluste)
        // These can only be offset against future stock gains (Verlusttopf Aktien)
        // Note: Uses gross loss amounts, not taxable amounts
        let gross_capital_losses: Decimal = self
            .capital_gains
            .iter()
            .filter(|e| e.gross_gain_loss < dec!(0))
            .map(|e| e.gross_gain_loss.abs())
            .sum();
        self.kap_zeile_23 = gross_capital_losses;

        // KAP Zeile 41: Creditable foreign withholding tax (anrechenbare ausländische Steuer)
        // This is the amount of foreign tax that can be credited against German tax liability
        // Limited to the German tax on the same income (proportional crediting)
        self.kap_zeile_41 = self.total_foreign_tax_credit;
    }
}

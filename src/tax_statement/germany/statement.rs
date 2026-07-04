//! German tax statement data structures.
//!
//! Contains the main GermanTaxStatement struct and entry types for capital gains,
//! dividends, interest, and FX gains/losses.

use crate::core::GenericResult;
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

/// Entry for broker fees.
///
/// Broker fees are deductible from capital gains (Werbungskosten / Anschaffungsnebenkosten).
/// Note: Trade commissions are already included in capital gain calculations.
/// This covers standalone fees like account maintenance, data fees, etc.
#[derive(Debug, Clone)]
pub struct FeeEntry {
    pub date: Date,
    pub description: String,
    pub amount_eur: Decimal, // Positive = fee paid, negative = refund received
    pub notes: Option<String>,
}

/// Entry for stock grants (RSUs, stock options exercised, etc.).
///
/// In Germany, stock grants have TWO taxable events:
/// 1. At vesting: Taxed as employment income (geldwerter Vorteil) at marginal income tax rate
///    - This is NOT Abgeltungssteuer but regular income tax
///    - The taxable amount is the FMV at vest date
/// 2. At sale: Capital gains tax (Abgeltungssteuer) only on gain above vest-date FMV
///    - Cost basis = FMV at vest date (not zero!)
#[derive(Debug, Clone)]
pub struct StockGrantEntry {
    pub vest_date: Date,
    pub symbol: String,
    pub isin: String,
    pub quantity: Decimal,
    pub fmv_per_share_eur: Decimal, // Fair market value at vest date
    pub total_fmv_eur: Decimal,     // Total employment income = quantity × FMV
    pub notes: Option<String>,
}

/// Entry for cash grants (broker bonuses, promotional cash, etc.).
///
/// Cash grants are "sonstige Einkünfte" (other income) under German tax law.
/// - Taxable at marginal income tax rate (not Abgeltungssteuer)
/// - Only taxable if total other income >€256/year
#[derive(Debug, Clone)]
pub struct CashGrantEntry {
    pub date: Date,
    pub description: String,
    pub amount_eur: Decimal,
    pub notes: Option<String>,
}

/// Entry for corporate actions that have tax implications.
///
/// - Spinoffs: May require cost basis allocation between parent and new company
/// - Liquidations: Treated as sale, triggers capital gain/loss
/// - Stock splits: No tax event (handled separately by StockSplitController)
/// - Mergers: May trigger gain if cash received
#[derive(Debug, Clone)]
pub struct CorporateActionEntry {
    pub date: Date,
    pub action_type: CorporateActionType,
    pub symbol: String,
    pub description: String,
    pub tax_impact_eur: Option<Decimal>, // Capital gain/loss if applicable
    pub notes: Option<String>,
}

/// Types of corporate actions for tax purposes.
#[derive(Debug, Clone, PartialEq)]
pub enum CorporateActionType {
    Spinoff,
    Liquidation,
    Merger,
    Delisting,
    Other(String),
}

impl std::fmt::Display for CorporateActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorporateActionType::Spinoff => write!(f, "Spinoff"),
            CorporateActionType::Liquidation => write!(f, "Liquidation"),
            CorporateActionType::Merger => write!(f, "Merger"),
            CorporateActionType::Delisting => write!(f, "Delisting"),
            CorporateActionType::Other(s) => write!(f, "Other: {}", s),
        }
    }
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
    pub fees: Vec<FeeEntry>,
    pub stock_grants: Vec<StockGrantEntry>,
    pub cash_grants: Vec<CashGrantEntry>,
    pub corporate_actions: Vec<CorporateActionEntry>,

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
    pub total_fees: Decimal,
    pub total_stock_grant_income: Decimal, // Employment income, NOT Abgeltungssteuer
    pub total_cash_grant_income: Decimal,  // Other income, NOT Abgeltungssteuer
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
    pub fn new(
        year: i32,
        church_tax_rate: Decimal,
        loss_carryforward: Decimal,
    ) -> GenericResult<Self> {
        Ok(GermanTaxStatement {
            year,
            tax_rates: GermanTaxRates::for_year(year, church_tax_rate)?,

            capital_gains: Vec::new(),
            dividends: Vec::new(),
            interest: Vec::new(),
            fx_gains: Vec::new(),
            fees: Vec::new(),
            stock_grants: Vec::new(),
            cash_grants: Vec::new(),
            corporate_actions: Vec::new(),

            loss_carryforward_used: dec!(0),
            loss_carryforward_remaining: loss_carryforward,

            total_capital_gains: dec!(0),
            total_capital_losses: dec!(0),
            total_dividend_income: dec!(0),
            total_interest_income: dec!(0),
            total_fx_gains: dec!(0),
            total_fx_losses: dec!(0),
            total_fees: dec!(0),
            total_stock_grant_income: dec!(0),
            total_cash_grant_income: dec!(0),
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
        })
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

    /// Add a fee entry.
    pub fn add_fee(&mut self, entry: FeeEntry) {
        self.fees.push(entry);
    }

    /// Add a stock grant entry.
    pub fn add_stock_grant(&mut self, entry: StockGrantEntry) {
        self.stock_grants.push(entry);
    }

    /// Add a cash grant entry.
    pub fn add_cash_grant(&mut self, entry: CashGrantEntry) {
        self.cash_grants.push(entry);
    }

    /// Add a corporate action entry.
    pub fn add_corporate_action(&mut self, entry: CorporateActionEntry) {
        self.corporate_actions.push(entry);
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
        self.total_fees = dec!(0);
        self.total_stock_grant_income = dec!(0);
        self.total_cash_grant_income = dec!(0);
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

        // Sum fees (these are deductible from capital income)
        // Note: Fees reduce taxable income but are tracked separately for reporting
        for entry in &self.fees {
            self.total_fees += entry.amount_eur;
        }

        // Sum stock grants (employment income - NOT Abgeltungssteuer)
        // This is reported separately from capital income
        for entry in &self.stock_grants {
            self.total_stock_grant_income += entry.total_fmv_eur;
        }

        // Sum cash grants (other income - NOT Abgeltungssteuer)
        // Only taxable if >€256/year total
        for entry in &self.cash_grants {
            self.total_cash_grant_income += entry.amount_eur;
        }

        // Calculate net capital gain/loss for carryforward calculation
        // Note: FX losses from interest-bearing accounts can be included in the general loss bucket
        // Note: Fees can be deducted from capital gains
        let net_capital_gain_loss = self.total_capital_gains - self.total_capital_losses
            + self.total_fx_gains
            - self.total_fx_losses
            - self.total_fees; // Fees reduce taxable capital gains

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

        // Creditable foreign tax is already folded into each entry's tax via the §32d(1) formula
        // (tax = (e − 4q)/(4 + k)), so total_german_tax is already net of the credit — do not
        // subtract it a second time.
        self.net_tax_due = self.total_german_tax.max(dec!(0));

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

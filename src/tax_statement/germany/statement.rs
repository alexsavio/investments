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
    /// Gain/loss after the §19 InvStG Vorabpauschale reduction and the Altbestand exclusion, but
    /// before Teilfreistellung. This is the value reported on Anlage KAP-INV (the Finanzamt applies
    /// Teilfreistellung); for a non-fund entry with neither adjustment it equals `gross_gain_loss`.
    pub taxable_before_exemption: Decimal,
    pub teilfreistellung_rate: TeilfreistellungRate,
    /// True for direct shares (Aktien) — no fund classification. Drives the §20(6) stock loss pot;
    /// fund/ETF units (any Teilfreistellung classification, incl. Bond) are false and use the
    /// general pot.
    pub is_stock: bool,
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

/// Entry for a fund's Vorabpauschale (§18 InvStG) — the advance lump sum a fund held at year end is
/// deemed to distribute.
///
/// The lump sum arises from holding the fund at the end of `arising_year` and is deemed received
/// (and taxable) on `deemed_received`, the first business day of the following year. It is therefore
/// income of that following year's return, not `arising_year`'s.
#[derive(Debug, Clone)]
pub struct VorabpauschaleEntry {
    pub arising_year: i32,
    pub deemed_received: Date,
    pub symbol: String,
    pub isin: String,
    pub quantity: Decimal,
    pub nav_jan1: Decimal,
    pub nav_dec31: Decimal,
    pub distributions: Decimal,
    pub basiszins: Decimal,
    pub teilfreistellung_rate: TeilfreistellungRate,
    /// Gross Vorabpauschale (pre-Teilfreistellung); the figure that reduces the sale gain (§19).
    pub gross_vorabpauschale: Decimal,
    /// Taxable amount after Teilfreistellung.
    pub taxable_amount: Decimal,
    pub abgeltungssteuer: Decimal,
    pub solidaritaetszuschlag: Decimal,
    pub kirchensteuer: Decimal,
    pub total_tax: Decimal,
    /// Accumulated gross Vorabpauschale to carry into next year's config for this still-held fund
    /// (prior carryforward + this year's gross).
    pub accumulated_after: Decimal,
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
    pub fees: Vec<FeeEntry>,
    pub stock_grants: Vec<StockGrantEntry>,
    pub cash_grants: Vec<CashGrantEntry>,
    pub corporate_actions: Vec<CorporateActionEntry>,
    pub vorabpauschale: Vec<VorabpauschaleEntry>,

    /// Year-end fund holdings whose year-boundary NAVs are not configured, so their Vorabpauschale
    /// could not be computed (ISIN, or symbol when the ISIN is absent).
    pub vorabpauschale_missing_nav: Vec<String>,

    /// Short (negative-quantity) positions held at the statement's end, reported for information
    /// only. Their §20 EStG treatment (Termingeschäfte/Stillhaltergeschäfte) is not computed and
    /// requires manual review. Each entry is `(symbol, quantity)` with a negative quantity.
    pub short_positions: Vec<(String, Decimal)>,

    // Loss carryforward pots (§20(6) EStG). `prior` is the festgestellter Verlustvortrag brought
    // in from the previous year; `next` is what carries to the following year after this year's
    // offsetting. Share-sale losses live in their own pot and never offset the general pot.
    pub loss_carryforward_stock_prior: Decimal,
    pub loss_carryforward_other_prior: Decimal,
    pub loss_carryforward_stock_next: Decimal,
    pub loss_carryforward_other_next: Decimal,

    // Sparer-Pauschbetrag (saver's allowance) for the year and the portion actually consumed.
    pub sparer_pauschbetrag: Decimal,
    pub sparer_pauschbetrag_used: Decimal,

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
    // Vorabpauschale (§18 InvStG). Deemed received the first business day of the following year, so
    // it is that year's income and is NOT folded into this year's taxable base above.
    pub total_vorabpauschale_gross: Decimal,
    pub total_vorabpauschale_taxable: Decimal,
    pub total_vorabpauschale_tax: Decimal,
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

    // Anlage KAP form line values (non-fund entries only; fund income is declared on KAP-INV).
    //
    // Pinned to the official forms of the Bundesfinanzverwaltung (Formular-Management-System,
    // formulare-bfinv.de): "Anlage KAP 2024" (print id 2024AnlKAP051NET, September 2024) and
    // "Anlage KAP 2025" (2025AnlKAP051NET, Oktober 2025). PDFs retrieved 2026-08-11 from
    // https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2024/Anlage_KAP_steuern.de_01.pdf
    // and https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2025/Anlage_KAP_2025_steuern-de.pdf
    // All five lines below carry the same number in both years; the 2025 form only voids the
    // Termingeschäfte lines (21, 24 and 25 are marked "frei") without renumbering the rest.
    /// KAP Zeile 19 "Ausländische Kapitalerträge": net foreign capital income (dividends + interest
    /// + share gains/losses + FX).
    pub kap_zeile_19: Decimal,
    /// KAP Zeile 20 "In den Zeilen 18 und 19 enthaltene Gewinne aus Aktienveräußerungen i. S. d.
    /// § 20 Abs. 2 Satz 1 Nr. 1 EStG": share-sale gains contained in Zeile 19.
    pub kap_zeile_20: Decimal,
    /// KAP Zeile 22 "In den Zeilen 18 und 19 enthaltene Verluste ohne Verluste aus der Veräußerung
    /// von Aktien": contained losses excluding share-sale losses (FX / other §20 losses).
    pub kap_zeile_22: Decimal,
    /// KAP Zeile 23 "In den Zeilen 18 und 19 enthaltene Verluste aus der Veräußerung von Aktien
    /// i. S. d. § 20 Abs. 2 Satz 1 Nr. 1 EStG": contained share-sale losses.
    pub kap_zeile_23: Decimal,
    /// KAP Zeile 41 "Anrechenbare noch nicht angerechnete ausländische Steuern": creditable foreign
    /// withholding tax.
    pub kap_zeile_41: Decimal,

    // Anlage KAP-INV: gross (pre-Teilfreistellung) investment-fund figures by fund type. The
    // Finanzamt applies the Teilfreistellung to these values itself.
    pub kap_inv_equity: KapInvGroup,
    pub kap_inv_mixed: KapInvGroup,
    pub kap_inv_other: KapInvGroup,

    /// §23 EStG results for a non-interest-bearing foreign-currency account (Anlage SO). Empty for
    /// the default §20 treatment.
    pub section23: Section23,
}

/// Gross (pre-Teilfreistellung) Anlage KAP-INV figures for one fund-type group.
#[derive(Debug, Default, Clone, Copy)]
pub struct KapInvGroup {
    /// Gross fund distributions received.
    pub distributions: Decimal,
    /// Gross gains from fund-unit sales.
    pub sale_gains: Decimal,
    /// Gross losses from fund-unit sales (positive magnitude).
    pub sale_losses: Decimal,
}

/// §23 EStG (private Veräußerungsgeschäfte) foreign-currency results, reported for manual Anlage SO
/// declaration. The tool computes no tax here — §23 income is taxed at the filer's personal income
/// rate, which it cannot know — so these are informational totals, mirroring the Anlage N / §22
/// grant handling.
#[derive(Debug, Default, Clone, Copy)]
pub struct Section23 {
    /// Gains on currency held ≤ 1 year (taxable as Sonstige Einkünfte).
    pub short_term_gains: Decimal,
    /// Losses on currency held ≤ 1 year (positive magnitude; offset §23 gains only).
    pub short_term_losses: Decimal,
    /// Gains on currency held > 1 year — tax-free (Spekulationsfrist met).
    pub long_term_tax_free: Decimal,
    /// Borrowed-currency (negative-balance) FX realized under §23 — flagged for manual review, as
    /// the §20 Fremdwährungskredit rule (BMF 19.05.2022 Rz. 131) does not carry over to §23.
    pub borrowed_review: Decimal,
}

impl Section23 {
    /// Net short-term (≤ 1 year) result: gains minus losses. Positive = taxable base before the
    /// Freigrenze; ≤ 0 = a §23 loss (deductible against §23 gains in other years only).
    pub fn short_term_net(&self) -> Decimal {
        self.short_term_gains - self.short_term_losses
    }

    /// Whether there is anything to report.
    pub fn is_empty(&self) -> bool {
        self.short_term_gains == dec!(0)
            && self.short_term_losses == dec!(0)
            && self.long_term_tax_free == dec!(0)
            && self.borrowed_review == dec!(0)
    }

    /// The §23 Freigrenze for `year`: €600 through 2023, €1000 from 2024. If total §23 gains (across
    /// all private sales, not only currency) stay below it, they are entirely tax-free.
    pub fn freigrenze(year: i32) -> Decimal {
        if year >= 2024 { dec!(1000) } else { dec!(600) }
    }
}

impl GermanTaxStatement {
    /// Create a new German tax statement for the given year.
    ///
    /// `loss_carryforward_stock` / `loss_carryforward_other` are the prior-year festgestellter
    /// Verlustvortrag of each §20(6) pot; `sparer_pauschbetrag` is the saver's allowance.
    pub fn new(
        year: i32,
        church_tax_rate: Decimal,
        loss_carryforward_stock: Decimal,
        loss_carryforward_other: Decimal,
        sparer_pauschbetrag: Decimal,
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
            vorabpauschale: Vec::new(),
            vorabpauschale_missing_nav: Vec::new(),
            short_positions: Vec::new(),

            loss_carryforward_stock_prior: loss_carryforward_stock,
            loss_carryforward_other_prior: loss_carryforward_other,
            loss_carryforward_stock_next: dec!(0),
            loss_carryforward_other_next: dec!(0),

            sparer_pauschbetrag,
            sparer_pauschbetrag_used: dec!(0),

            total_capital_gains: dec!(0),
            total_capital_losses: dec!(0),
            total_dividend_income: dec!(0),
            total_interest_income: dec!(0),
            total_fx_gains: dec!(0),
            total_fx_losses: dec!(0),
            total_fees: dec!(0),
            total_stock_grant_income: dec!(0),
            total_cash_grant_income: dec!(0),
            total_vorabpauschale_gross: dec!(0),
            total_vorabpauschale_taxable: dec!(0),
            total_vorabpauschale_tax: dec!(0),
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
            kap_zeile_20: dec!(0),
            kap_zeile_22: dec!(0),
            kap_zeile_23: dec!(0),
            kap_zeile_41: dec!(0),

            kap_inv_equity: KapInvGroup::default(),
            kap_inv_mixed: KapInvGroup::default(),
            kap_inv_other: KapInvGroup::default(),

            section23: Section23::default(),
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

    /// Add a Vorabpauschale entry.
    pub fn add_vorabpauschale(&mut self, entry: VorabpauschaleEntry) {
        self.vorabpauschale.push(entry);
    }

    /// Calculate all summary totals.
    pub fn calculate_totals(&mut self) {
        // Reset reporting totals.
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
        self.total_foreign_tax_credit = dec!(0);

        // §20(6) EStG loss pots. Share-sale (Aktien) results form their own pot; everything else —
        // fund-unit sales, FX, dividends, interest — forms the general pot, which also offsets
        // dividends and interest. Gain vs. loss is decided by the sign of `taxable_amount`
        // (Altbestand- and Teilfreistellung-adjusted), never the raw gross_gain_loss, so a mixed
        // pre-2009/post-2009 sale files into the correct side.
        let mut stock_gains = dec!(0);
        let mut stock_losses = dec!(0);
        let mut general_gains = dec!(0);
        let mut general_losses = dec!(0);

        for entry in &self.capital_gains {
            self.total_foreign_tax += entry.foreign_tax;
            let amount = entry.taxable_amount;
            let (gains, losses) = if entry.is_stock {
                (&mut stock_gains, &mut stock_losses)
            } else {
                (&mut general_gains, &mut general_losses)
            };
            if amount >= dec!(0) {
                *gains += amount;
                self.total_capital_gains += amount;
            } else {
                *losses += -amount;
                self.total_capital_losses += -amount;
            }
        }

        for entry in &self.dividends {
            self.total_dividend_income += entry.taxable_amount;
            self.total_foreign_tax += entry.foreign_withholding_tax;
            self.total_foreign_tax_credit += entry.foreign_tax_credit;
            general_gains += entry.taxable_amount;
        }

        for entry in &self.interest {
            self.total_interest_income += entry.taxable_amount;
            self.total_foreign_tax += entry.foreign_withholding_tax;
            self.total_foreign_tax_credit += entry.foreign_tax_credit;
            general_gains += entry.taxable_amount;
        }

        // FX gains/losses on interest-bearing accounts (§20 Abs. 2 Nr. 7 EStG) join the general pot.
        for entry in &self.fx_gains {
            if entry.gross_amount_eur >= dec!(0) {
                self.total_fx_gains += entry.taxable_amount;
                general_gains += entry.taxable_amount;
            } else {
                self.total_fx_losses += entry.taxable_amount.abs();
                general_losses += entry.taxable_amount.abs();
            }
        }

        // Fees are collected for information only. §20(9) EStG bars deducting actual expenses under
        // the Abgeltungsteuer (only the Sparer-Pauschbetrag applies), so they never reduce the base.
        for entry in &self.fees {
            self.total_fees += entry.amount_eur;
        }

        for entry in &self.stock_grants {
            self.total_stock_grant_income += entry.total_fmv_eur;
        }
        for entry in &self.cash_grants {
            self.total_cash_grant_income += entry.amount_eur;
        }

        // Vorabpauschale is income of the following year (deemed received on its first business day),
        // so it is summed for reporting but never added to this year's pots or taxable base.
        self.total_vorabpauschale_gross = dec!(0);
        self.total_vorabpauschale_taxable = dec!(0);
        self.total_vorabpauschale_tax = dec!(0);
        for entry in &self.vorabpauschale {
            self.total_vorabpauschale_gross += entry.gross_vorabpauschale;
            self.total_vorabpauschale_taxable += entry.taxable_amount;
            self.total_vorabpauschale_tax += entry.total_tax;
        }

        // Offset within each pot and apply the prior-year carryforward; a net loss becomes next
        // year's carryforward for that pot (no cross-pot offset, no carry-back).
        let (stock_result, _stock_used, stock_next) = calculate_with_loss_carryforward(
            stock_gains - stock_losses,
            self.loss_carryforward_stock_prior,
        );
        let (general_result, _general_used, general_next) = calculate_with_loss_carryforward(
            general_gains - general_losses,
            self.loss_carryforward_other_prior,
        );
        self.loss_carryforward_stock_next = stock_next;
        self.loss_carryforward_other_next = general_next;

        // Sparer-Pauschbetrag reduces the combined positive result; unused allowance is not carried.
        let taxable_before_allowance = stock_result + general_result;
        self.sparer_pauschbetrag_used = self.sparer_pauschbetrag.min(taxable_before_allowance);
        let taxable = (taxable_before_allowance - self.sparer_pauschbetrag).max(dec!(0));
        self.total_taxable_income = taxable;

        // Summary tax is computed ONCE on the final taxable base with the aggregate creditable
        // foreign tax. It is deliberately NOT the sum of the per-row taxes — those are informational
        // and ignore the pots, the carryforward, and the allowance, which only apply year-wide.
        let taxes = self
            .tax_rates
            .compute_taxes(taxable, self.total_foreign_tax_credit);
        self.total_abgeltungssteuer = taxes.abgeltungssteuer;
        self.total_solidaritaetszuschlag = taxes.solidaritaetszuschlag;
        self.total_kirchensteuer = taxes.kirchensteuer;
        self.total_german_tax = taxes.total;
        self.net_tax_due = taxes.total.max(dec!(0));

        // Anlage KAP (non-fund entries) and Anlage KAP-INV (investment-fund entries).
        //
        // Investment-fund income — any Teilfreistellung classification, including Bond — is declared
        // GROSS on Anlage KAP-INV (the Finanzamt applies the exemption). Everything else stays on
        // Anlage KAP. KAP capital-gain figures are built from the Altbestand-adjusted taxable_amount
        // (for non-fund entries that equals the post-Altbestand gross, since Teilfreistellung is
        // None), never the raw gross_gain_loss — a pure pre-2009 sale then lands on neither line.

        // --- Anlage KAP: non-fund entries, losses INCLUDED in Zeile 19 ---
        let mut kap_positive = dec!(0);
        let mut zeile_20 = dec!(0); // share-sale gains contained in Zeile 19
        let mut zeile_23 = dec!(0); // share-sale losses (positive magnitude)
        let mut zeile_22 = dec!(0); // non-share losses (FX / other §20)

        for entry in &self.capital_gains {
            if entry.teilfreistellung_rate != TeilfreistellungRate::None {
                continue; // fund unit → KAP-INV
            }
            let amount = entry.taxable_amount;
            if amount > dec!(0) {
                zeile_20 += amount;
                kap_positive += amount;
            } else if amount < dec!(0) {
                zeile_23 += -amount;
            }
        }
        for entry in &self.dividends {
            if entry.teilfreistellung_rate == TeilfreistellungRate::None {
                kap_positive += entry.gross_amount_eur;
            }
        }
        for entry in &self.interest {
            kap_positive += entry.gross_amount_eur;
        }
        for entry in &self.fx_gains {
            let amount = entry.taxable_amount;
            if amount >= dec!(0) {
                kap_positive += amount;
            } else {
                zeile_22 += -amount;
            }
        }

        self.kap_zeile_19 = kap_positive - zeile_22 - zeile_23;
        self.kap_zeile_20 = zeile_20;
        self.kap_zeile_22 = zeile_22;
        self.kap_zeile_23 = zeile_23;
        // Fund distributions carry no creditable foreign tax (q = 0), so the aggregate credit is
        // already non-fund only.
        self.kap_zeile_41 = self.total_foreign_tax_credit;

        // --- Anlage KAP-INV: fund entries, GROSS (pre-Teilfreistellung) figures by fund type ---
        let mut equity = KapInvGroup::default();
        let mut mixed = KapInvGroup::default();
        let mut other = KapInvGroup::default();

        for entry in &self.dividends {
            let group = match entry.teilfreistellung_rate {
                TeilfreistellungRate::Equity => &mut equity,
                TeilfreistellungRate::Mixed => &mut mixed,
                TeilfreistellungRate::Bond => &mut other,
                TeilfreistellungRate::None => continue,
            };
            group.distributions += entry.gross_amount_eur;
        }
        for entry in &self.capital_gains {
            let group = match entry.teilfreistellung_rate {
                TeilfreistellungRate::Equity => &mut equity,
                TeilfreistellungRate::Mixed => &mut mixed,
                TeilfreistellungRate::Bond => &mut other,
                TeilfreistellungRate::None => continue,
            };
            // Report the §19-reduced, Altbestand-adjusted gross (pre-Teilfreistellung), so the filed
            // KAP-INV figure matches the tool's own tax estimate and does not re-tax the Vorabpauschale.
            if entry.taxable_before_exemption >= dec!(0) {
                group.sale_gains += entry.taxable_before_exemption;
            } else {
                group.sale_losses += -entry.taxable_before_exemption;
            }
        }

        self.kap_inv_equity = equity;
        self.kap_inv_mixed = mixed;
        self.kap_inv_other = other;
    }
}

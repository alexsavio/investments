//! The Spanish tax statement model and its year-level totals.

use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::spain::carryforward::{LedgerApplication, LossLedger};
use crate::taxes::spain::compensation::compensate_savings_base;
use crate::taxes::spain::credit::double_taxation_credit;
use crate::taxes::spain::scale::SavingsScale;
use crate::types::{Date, Decimal};

/// One FIFO lot consumed by a sale, with its actualization arithmetic laid out step by step.
///
/// Kept per lot rather than collapsed into a sale-level total because the coefficient is a
/// per-lot figure — it depends on when *that* lot was acquired — and a filer asked to justify an
/// actualized cost has to be able to follow the multiplication line by line.
#[derive(Clone, Debug)]
pub struct SpanishLotDetail {
    pub acquisition_date: Date,
    pub quantity: Decimal,
    /// Acquisition cost in EUR, including buy-side commissions.
    pub cost_eur: Decimal,
    /// NF 3/2014 art. 45.2 coefficient for this lot's acquisition year; 1 under Territorio Común.
    pub coefficient: Decimal,
    pub actualized_cost_eur: Decimal,
    /// Share of the sale's net proceeds attributed to this lot, pro rata by quantity.
    pub proceeds_eur: Decimal,
    pub gain_eur: Decimal,
}

/// A disposal producing a ganancia or pérdida patrimonial.
#[derive(Clone, Debug)]
pub struct CapitalGainEntry {
    pub symbol: String,
    pub isin: String,
    pub description: String,
    /// Fecha de transmisión, taken as the trade's conclusion date.
    pub sale_date: Date,
    pub settle_date: Date,
    pub quantity: Decimal,
    /// Sale proceeds net of the sell-side commission.
    pub proceeds_eur: Decimal,
    /// Acquisition cost before actualization.
    pub cost_eur: Decimal,
    pub actualized_cost_eur: Decimal,
    /// Result after actualization: proceeds − actualized cost.
    pub fiscal_gain_loss: Decimal,
    /// Portion of a loss deferred under the valores-homogéneos rule, as a positive magnitude.
    pub deferred_loss: Decimal,
    /// What actually enters the savings base this year.
    pub integrable_amount: Decimal,
    pub lots: Vec<SpanishLotDetail>,
    pub notes: Option<String>,
}

/// A dividend, taxed as rendimiento del capital mobiliario.
#[derive(Clone, Debug)]
pub struct DividendEntry {
    pub symbol: String,
    pub isin: String,
    pub description: String,
    pub date: Date,
    pub gross_eur: Decimal,
    /// Foreign tax actually withheld at source.
    pub withheld_eur: Decimal,
    /// Withholding capped at the treaty rate — the first of the two limbs of the double-taxation
    /// credit. Informational per row: the credit is a year-level figure, because its second limb is
    /// the average savings rate, which only exists once the whole year is known.
    pub treaty_capped_credit: Decimal,
}

/// Interest, taxed as rendimiento del capital mobiliario.
#[derive(Clone, Debug)]
pub struct InterestEntry {
    pub date: Date,
    pub description: String,
    pub gross_eur: Decimal,
}

/// A realized foreign-currency conversion result.
#[derive(Clone, Debug)]
pub struct FxGainEntry {
    pub date: Date,
    pub currency: String,
    /// Acquisition date of the FIFO lot consumed.
    pub acquisition_date: Date,
    /// Positive for a gain, negative for a loss.
    pub amount_eur: Decimal,
    pub activity_code: String,
}

/// A broker fee.
#[derive(Clone, Debug)]
pub struct FeeEntry {
    pub date: Date,
    pub description: String,
    /// Positive for a charge, negative for a refund.
    pub amount_eur: Decimal,
    /// Whether this fee reduces the RCM net result. Regime-dependent: Territorio Común allows
    /// custody and administration fees (LIRPF art. 26.1.a), Gipuzkoa allows nothing (NF 3/2014
    /// art. 39 is a closed list that does not reach securities income).
    pub deductible: bool,
    pub notes: Option<String>,
}

/// A Spanish IRPF savings-income tax statement for one tax year.
#[derive(Clone, Debug)]
pub struct SpanishTaxStatement {
    pub year: i32,
    pub regime: SpanishTaxRegime,
    /// The savings scale in force, kept so the sell simulation prices a disposal against the same
    /// scale the statement itself used.
    pub scale: SavingsScale,

    pub capital_gains: Vec<CapitalGainEntry>,
    pub dividends: Vec<DividendEntry>,
    pub interest: Vec<InterestEntry>,
    pub fees: Vec<FeeEntry>,
    /// Conversion results realized on a held foreign-currency balance; these enter the ganancias
    /// group.
    pub fx_gains: Vec<FxGainEntry>,

    /// Results realized on a **borrowed** foreign-currency balance. Not taxed here: repaying a
    /// currency loan is not obviously a transfer of a patrimonial element, and neither the foral
    /// nor the state text settles it. Reported for manual review instead of being silently taxed
    /// or silently dropped.
    pub fx_borrowed_review: Vec<FxGainEntry>,

    /// Short (negative-quantity) positions held at the statement's end, reported for information
    /// only. Their treatment is not computed and needs manual review.
    pub short_positions: Vec<(String, Decimal)>,

    pub total_dividend_income: Decimal,
    pub total_interest_income: Decimal,
    /// Fees that reduce the RCM result, as a positive magnitude.
    pub total_deductible_fees: Decimal,
    /// Fees reported for information only, as a positive magnitude.
    pub total_informational_fees: Decimal,
    pub total_foreign_withholding: Decimal,
    pub total_fx_gains: Decimal,
    pub total_fx_losses: Decimal,
    /// Net borrowed-balance result awaiting manual review.
    pub total_fx_borrowed_review: Decimal,

    /// Net rendimientos del capital mobiliario before compensation.
    pub rcm_net: Decimal,
    /// Net ganancias y pérdidas patrimoniales before compensation.
    pub gyp_net: Decimal,

    /// Each group after compensation; these sum to the savings base.
    pub rcm_taxable: Decimal,
    pub gyp_taxable: Decimal,

    /// Prior-year pending balances brought in, and what this year consumed of them.
    pub rcm_ledger_prior: LossLedger,
    pub gyp_ledger_prior: LossLedger,
    pub rcm_applied: LedgerApplication,
    pub gyp_applied: LedgerApplication,

    /// Current-year negative of one group set against the other (Territorio Común only).
    pub cross_offset_rcm_to_gyp: Decimal,
    pub cross_offset_gyp_to_rcm: Decimal,

    /// Balances to carry into next year's config, and those lost to the four-year window.
    pub rcm_ledger_next: LossLedger,
    pub gyp_ledger_next: LossLedger,
    pub rcm_expired: Decimal,
    pub gyp_expired: Decimal,

    pub total_foreign_tax_credit: Decimal,

    pub savings_base: Decimal,
    pub savings_quota: Decimal,
    /// Tipo medio de gravamen del ahorro, the cap on the double-taxation credit.
    pub average_savings_rate: Decimal,
    pub net_tax_due: Decimal,

    /// Fraction of the other group's positive balance a negative one may offset: 0 under Gipuzkoa,
    /// 0.25 under Territorio Común.
    cross_offset_fraction: Decimal,
    /// Treaty cap on the source state's withholding, used for the credit's first limb.
    treaty_rate: Decimal,
}

impl SpanishTaxStatement {
    pub fn new(
        year: i32,
        regime: SpanishTaxRegime,
        scale: SavingsScale,
        rcm_ledger: LossLedger,
        gyp_ledger: LossLedger,
        cross_offset_fraction: Decimal,
        treaty_rate: Decimal,
    ) -> SpanishTaxStatement {
        SpanishTaxStatement {
            year,
            regime,
            scale,
            cross_offset_fraction,
            treaty_rate,
            rcm_ledger_prior: rcm_ledger.clone(),
            gyp_ledger_prior: gyp_ledger.clone(),
            rcm_ledger_next: rcm_ledger,
            gyp_ledger_next: gyp_ledger,
            rcm_taxable: Decimal::ZERO,
            gyp_taxable: Decimal::ZERO,
            rcm_applied: LedgerApplication::default(),
            gyp_applied: LedgerApplication::default(),
            cross_offset_rcm_to_gyp: Decimal::ZERO,
            cross_offset_gyp_to_rcm: Decimal::ZERO,
            rcm_expired: Decimal::ZERO,
            gyp_expired: Decimal::ZERO,
            total_foreign_tax_credit: Decimal::ZERO,
            capital_gains: Vec::new(),
            dividends: Vec::new(),
            interest: Vec::new(),
            fees: Vec::new(),
            fx_gains: Vec::new(),
            fx_borrowed_review: Vec::new(),
            short_positions: Vec::new(),
            total_dividend_income: Decimal::ZERO,
            total_interest_income: Decimal::ZERO,
            total_deductible_fees: Decimal::ZERO,
            total_informational_fees: Decimal::ZERO,
            total_foreign_withholding: Decimal::ZERO,
            total_fx_gains: Decimal::ZERO,
            total_fx_losses: Decimal::ZERO,
            total_fx_borrowed_review: Decimal::ZERO,
            rcm_net: Decimal::ZERO,
            gyp_net: Decimal::ZERO,
            savings_base: Decimal::ZERO,
            savings_quota: Decimal::ZERO,
            average_savings_rate: Decimal::ZERO,
            net_tax_due: Decimal::ZERO,
        }
    }

    /// Roll the entries up into the year's savings base and its cuota.
    ///
    /// The tax is computed once on the final base, never summed from per-entry figures: the scale
    /// is progressive, so a sum of separately-taxed entries is not the tax on their total.
    pub fn calculate_totals(&mut self) {
        self.total_dividend_income = self.dividends.iter().map(|entry| entry.gross_eur).sum();
        self.total_interest_income = self.interest.iter().map(|entry| entry.gross_eur).sum();
        self.total_foreign_withholding = self.dividends.iter().map(|entry| entry.withheld_eur).sum();

        self.total_deductible_fees = self
            .fees
            .iter()
            .filter(|fee| fee.deductible)
            .map(|fee| fee.amount_eur)
            .sum();
        self.total_informational_fees = self
            .fees
            .iter()
            .filter(|fee| !fee.deductible)
            .map(|fee| fee.amount_eur)
            .sum();

        self.rcm_net =
            self.total_dividend_income + self.total_interest_income - self.total_deductible_fees;

        self.total_fx_gains = self
            .fx_gains
            .iter()
            .map(|entry| std::cmp::max(Decimal::ZERO, entry.amount_eur))
            .sum();
        self.total_fx_losses = self
            .fx_gains
            .iter()
            .map(|entry| std::cmp::max(Decimal::ZERO, -entry.amount_eur))
            .sum();
        self.total_fx_borrowed_review = self
            .fx_borrowed_review
            .iter()
            .map(|entry| entry.amount_eur)
            .sum();

        // A currency conversion transfers a patrimonial element, so its result joins the ganancias
        // group rather than the RCM one.
        let capital_gains: Decimal = self
            .capital_gains
            .iter()
            .map(|entry| entry.integrable_amount)
            .sum();
        let fx: Decimal = self.fx_gains.iter().map(|entry| entry.amount_eur).sum();

        self.gyp_net = capital_gains + fx;

        let compensation = compensate_savings_base(
            self.year,
            self.rcm_net,
            self.gyp_net,
            self.rcm_ledger_prior.clone(),
            self.gyp_ledger_prior.clone(),
            self.cross_offset_fraction,
        );

        self.rcm_taxable = compensation.rcm_taxable;
        self.gyp_taxable = compensation.gyp_taxable;
        self.rcm_applied = compensation.rcm_applied;
        self.gyp_applied = compensation.gyp_applied;
        self.cross_offset_rcm_to_gyp = compensation.cross_offset_rcm_to_gyp;
        self.cross_offset_gyp_to_rcm = compensation.cross_offset_gyp_to_rcm;
        self.rcm_ledger_next = compensation.rcm_ledger_next;
        self.gyp_ledger_next = compensation.gyp_ledger_next;
        self.rcm_expired = compensation.rcm_expired;
        self.gyp_expired = compensation.gyp_expired;

        self.savings_base = compensation.savings_base;
        self.savings_quota = self.scale.tax(self.savings_base);
        self.average_savings_rate = self.scale.average_rate(self.savings_base);

        // The credit is computed once on the year's aggregates. Its second limb is the average
        // savings rate, which does not exist until the whole base is known, so the per-row
        // `treaty_capped_credit` figures are informational only.
        self.total_foreign_tax_credit = double_taxation_credit(
            self.total_foreign_withholding,
            self.foreign_taxed_income(),
            self.treaty_rate,
            self.average_savings_rate,
        );

        self.net_tax_due = std::cmp::max(
            Decimal::ZERO,
            self.savings_quota - self.total_foreign_tax_credit,
        );
    }

    /// Foreign-source income the double-taxation credit is measured against.
    // TODO(verify): only dividends carry foreign withholding in the supported statements, so this
    // is their gross sum. NF 3/2014 art. 91.b applies the average rate to "la renta obtenida en el
    // extranjero" and LIRPF art. 80.1.b to "la parte de base liquidable gravada en el extranjero";
    // whether that is gross or net of attributable expenses is not settled by either text.
    fn foreign_taxed_income(&self) -> Decimal {
        self.dividends.iter().map(|entry| entry.gross_eur).sum()
    }
}

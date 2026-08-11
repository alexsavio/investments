//! The Spanish tax statement model and its year-level totals.

use crate::taxes::spain::SpanishTaxRegime;
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

    /// Net rendimientos del capital mobiliario before compensation.
    pub rcm_net: Decimal,
    /// Net ganancias y pérdidas patrimoniales before compensation.
    pub gyp_net: Decimal,
    pub savings_base: Decimal,
    pub savings_quota: Decimal,
    /// Tipo medio de gravamen del ahorro, the cap on the double-taxation credit.
    pub average_savings_rate: Decimal,
    pub net_tax_due: Decimal,
}

impl SpanishTaxStatement {
    pub fn new(year: i32, regime: SpanishTaxRegime, scale: SavingsScale) -> SpanishTaxStatement {
        SpanishTaxStatement {
            year,
            regime,
            scale,
            capital_gains: Vec::new(),
            dividends: Vec::new(),
            interest: Vec::new(),
            fees: Vec::new(),
            short_positions: Vec::new(),
            total_dividend_income: Decimal::ZERO,
            total_interest_income: Decimal::ZERO,
            total_deductible_fees: Decimal::ZERO,
            total_informational_fees: Decimal::ZERO,
            total_foreign_withholding: Decimal::ZERO,
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

        self.gyp_net = self
            .capital_gains
            .iter()
            .map(|entry| entry.integrable_amount)
            .sum();

        // Each group is integrated "exclusivamente entre sí": a negative balance in one does not
        // reduce the other, it carries forward.
        self.savings_base = std::cmp::max(Decimal::ZERO, self.rcm_net)
            + std::cmp::max(Decimal::ZERO, self.gyp_net);
        self.savings_quota = self.scale.tax(self.savings_base);
        self.average_savings_rate = self.scale.average_rate(self.savings_base);
        self.net_tax_due = self.savings_quota;
    }
}

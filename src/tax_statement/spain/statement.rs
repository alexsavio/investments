//! The Spanish tax statement model and its year-level totals.

use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::spain::carryforward::{LedgerApplication, LossLedger};
use crate::taxes::spain::compensation::{CrossOffset, compensate_savings_base};
use crate::taxes::spain::credit::{double_taxation_credit, treaty_capped_credit};
use crate::taxes::spain::exemption::{GLOBAL_TRANSMISSION_LIMIT, small_disposals_exemption};
use crate::taxes::DeferredLossConfig;
use crate::taxes::spain::scale::SavingsScale;
use crate::types::{Date, Decimal};

use super::wash_sale::BoundaryKind;

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

/// A previously deferred loss becoming integrable again.
///
/// Reintegration is triggered by the disposal of the securities that blocked the loss, so the entry
/// is dated to that disposal and labelled with the sale the loss originally came from — a return can
/// otherwise not tell real activity from the unwinding of an earlier deferral.
#[derive(Clone, Debug)]
pub struct WashSaleReintegrationEntry {
    pub symbol: String,
    pub isin: String,
    /// Disposal of the blocking shares, i.e. when the loss became integrable again.
    pub date: Date,
    /// Acquisition date of the blocking shares.
    pub acquisition_date: Date,
    /// The loss-making sale the deferral came from.
    pub origin_sale_date: Date,
    /// Loss released, as a positive magnitude. It enters the ganancias group as a negative amount.
    pub released_eur: Decimal,
}

/// A loss the deferral rule could not be settled for, because the statement ends before its
/// repurchase window does.
#[derive(Clone, Debug)]
pub struct WashSaleWindowGap {
    pub symbol: String,
    pub sale_date: Date,
    /// Last date on which a repurchase could still defer this loss.
    pub window_end: Date,
    /// Loss at risk, as a positive magnitude.
    pub loss_eur: Decimal,
}

/// A loss whose deferral turns on where exactly a window edge falls.
///
/// The arithmetic itself is settled — months counted de fecha a fecha, both ends inclusive, clamped
/// to the last day of a short month — but no authority applies it to art. 33.5.f with concrete
/// dates, so a deferral decided by one day is named rather than left implicit.
#[derive(Clone, Debug)]
pub struct WashSaleBoundaryReview {
    pub symbol: String,
    pub sale_date: Date,
    /// The window edge as the tool computes it.
    pub boundary_date: Date,
    /// The edge an alternative reading would use.
    pub alternative_date: Date,
    pub kind: BoundaryKind,
    /// Loss that moves between deferred and deductible under that reading, positive magnitude.
    pub amount_eur: Decimal,
}

impl WashSaleBoundaryReview {
    /// The sentence every surface reports this with, so the console, the log and the CSV cannot
    /// drift apart.
    pub fn message(&self) -> String {
        format!(
            "€{} of the {} loss of {} turns on window-boundary arithmetic: {}. The tool puts that \
             edge on {}; the other reading puts it on {}. Two months are counted de fecha a fecha \
             with both ends inclusive (Código Civil art. 5.1; STS 552/2022), clamping to the last \
             day of a short month (Ley 39/2015 art. 30.4). See the open-interpretations register in \
             docs/spain-taxes.md.",
            super::format_eur(self.amount_eur),
            self.symbol,
            self.sale_date,
            self.kind.description(),
            self.boundary_date,
            self.alternative_date
        )
    }
}

/// A loss whose deferral turns on which of the statute's two windows the listing venue takes.
///
/// Only raised when homogeneous securities were acquired inside the year but outside the two months
/// — the two-month limb defers nothing there, the one-year limb would — and the venue is not one the
/// DGT has settled the two-month limb for.
#[derive(Clone, Debug)]
pub struct WashSaleVenueReview {
    pub symbol: String,
    pub sale_date: Date,
    /// Listing venue as the statement names it, or `None` when it names none.
    pub venue: Option<String>,
    /// Loss the one-year limb would defer on top of what was deferred, as a positive magnitude.
    pub loss_eur: Decimal,
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
    /// Whether this dividend counts towards the Gipuzkoa €1,500 exemption (NF 3/2014 art. 9.24).
    /// Always false under Territorio Común, which lost the relief in 2015.
    pub exemption_eligible: bool,
    pub notes: Option<String>,
}

/// Interest, taxed as rendimiento del capital mobiliario.
#[derive(Clone, Debug)]
pub struct InterestEntry {
    pub date: Date,
    pub description: String,
    /// Positive for interest received, negative for interest paid on a borrowed balance.
    pub gross_eur: Decimal,
    /// Whether this entry enters the RCM result. Interest **paid** does not: neither regime allows
    /// an expense against securities income beyond LIRPF art. 26.1.a's administration and custody,
    /// and NF 3/2014 art. 39 is narrower still.
    pub taxable: bool,
    pub notes: Option<String>,
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

/// A vested stock grant, reported only.
///
/// An RSU vest is employment income and belongs to the **general** base (NF 3/2014 art. 15 et seq. /
/// LIRPF art. 17), which this tool does not compute — it only handles the savings base. The row
/// exists so the figure is not silently absent from the filer's view of the year; the vest-date
/// value is what later prices the shares' acquisition cost when they are sold.
#[derive(Clone, Debug)]
pub struct StockGrantEntry {
    pub date: Date,
    pub symbol: String,
    pub description: String,
    pub quantity: Decimal,
    /// Vest-date fair market value of the whole vest, when the statement carries a per-share FMV.
    pub value_eur: Option<Decimal>,
    pub notes: String,
}

/// A corporate action, reported only.
///
/// Splits are already applied to the FIFO queue by the shared broker-statement engine; everything
/// else is surfaced so the filer can check whether it changed a cost basis the tool then used.
#[derive(Clone, Debug)]
pub struct CorporateActionEntry {
    pub date: Date,
    pub symbol: String,
    pub description: String,
    pub notes: String,
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
    /// The sentence every surface reports an unsettled fee type with, so the console, the log and
    /// the CSV note cannot drift apart. `None` for every type the doctrine classifies.
    pub review: Option<String>,
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
    /// Deferred losses this year's disposals made integrable again.
    pub wash_sale_reintegrations: Vec<WashSaleReintegrationEntry>,
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

    /// Vested stock grants, reported only: employment income belongs to the general base, which
    /// this tool does not compute.
    pub stock_grants: Vec<StockGrantEntry>,

    /// Corporate actions in the filing year, reported only.
    pub corporate_actions: Vec<CorporateActionEntry>,

    /// Short (negative-quantity) positions held at the statement's end, reported for information
    /// only. Their treatment is not computed and needs manual review.
    pub short_positions: Vec<(String, Decimal)>,

    /// FIFO lots acquired before 31 December 1994, by symbol and acquisition date. Those fall under
    /// an abatement regime this tool does not compute; the entry is factual for every regime, and
    /// only the ones with such a regime say anything about it.
    pub abatement_lots: Vec<(String, Date)>,

    /// Disposal years up to and including the filing year that the statement contains but the tool
    /// ships no actualization table for. Sales in those years were replayed for the
    /// valores-homogéneos rule — they still consume lots and release earlier deferrals — but their
    /// own result could not be priced, so a loss in one of them was never tested for deferral.
    ///
    /// Later years are left out: a disposal after 31 December of the filing year belongs to the
    /// next return, and this one only ever replays it to release deferrals.
    pub wash_sale_unpriced_years: Vec<i32>,

    /// Blocked lots still standing at the end of the statement, in the shape next year's
    /// `taxes.spain.deferred_losses` takes.
    pub deferred_losses_next: Vec<DeferredLossConfig>,

    /// Loss-making disposals whose +2-month repurchase window reaches past the statement's last
    /// date, so a repurchase that would defer the loss cannot be seen yet.
    pub wash_sale_window_gaps: Vec<WashSaleWindowGap>,

    /// Filing-year losses whose deferral would change under the one-year limb, on an instrument
    /// whose listing venue the two-month limb is not settled for.
    pub wash_sale_venue_reviews: Vec<WashSaleVenueReview>,

    /// Filing-year losses decided by exactly where a window edge falls.
    pub wash_sale_boundary_reviews: Vec<WashSaleBoundaryReview>,

    /// Loss deferred by this year's disposals, as a positive magnitude.
    pub total_deferred_loss: Decimal,
    /// Deferred loss this year's disposals released, as a positive magnitude.
    pub total_reintegrated_loss: Decimal,

    pub total_dividend_income: Decimal,
    /// Dividends exempt under the Gipuzkoa €1,500 annual relief (NF 3/2014 art. 9.24). Zero under
    /// Territorio Común.
    pub total_dividend_exemption: Decimal,
    pub total_interest_income: Decimal,
    /// Interest paid on a borrowed balance, as a positive magnitude. Reported only — it never
    /// reduces the RCM result.
    pub total_paid_interest: Decimal,
    /// Fees that reduce the RCM result, as a positive magnitude, after any regime cap.
    pub total_deductible_fees: Decimal,
    /// Fees reported for information only, as a positive magnitude. Excludes what the cap
    /// disallowed, which is reported on its own so the two reasons are never conflated.
    pub total_informational_fees: Decimal,
    /// Ceiling the regime puts on deductible custody and administration fees, and the part of them
    /// it disallowed. `None` where the regime sets no ceiling.
    pub custody_fee_cap: Option<Decimal>,
    pub total_capped_fees: Decimal,
    pub total_foreign_withholding: Decimal,
    /// Integrable result of the year's disposals — what the ganancias group takes from them.
    pub total_capital_gains: Decimal,
    /// The year's global importe of onerous securities transmissions, and the taxable increments
    /// they produced: the two figures TRLFIRPF art. 39.5.d's conditions are measured on.
    pub small_disposals_proceeds: Decimal,
    pub small_disposals_gains: Decimal,
    /// What art. 39.5.d exempted, as a positive magnitude. Zero outside Navarra.
    pub small_disposals_exemption: Decimal,
    /// Set when the year would have been tested against art. 39.5.d but a foreign-currency
    /// conversion made the global transmission amount unmeasurable.
    pub small_disposals_unmeasurable: bool,
    pub total_fx_gains: Decimal,
    pub total_fx_losses: Decimal,
    /// Net realized foreign-currency result on **held** balances: `total_fx_gains − total_fx_losses`.
    pub total_fx_result: Decimal,
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

    /// Current-year negative of one group set against the other. Zero under Gipuzkoa; the AEAT
    /// Fase 1ª under Común, and TRLFIRPF art. 54.2's own cross under Navarra.
    pub cross_offset_rcm_to_gyp: Decimal,
    pub cross_offset_gyp_to_rcm: Decimal,

    /// Prior-year balance of one group its own group could not absorb, set against the other's
    /// remainder. Zero under Gipuzkoa; the AEAT Fase 2ª-2º under Común, and under Navarra the part
    /// of the cross that only the broad reading of art. 54.2 allows.
    pub prior_cross_offset_rcm_to_gyp: Decimal,
    pub prior_cross_offset_gyp_to_rcm: Decimal,

    /// Balances to carry into next year's config, and those lost to the four-year window.
    pub rcm_ledger_next: LossLedger,
    pub gyp_ledger_next: LossLedger,
    pub rcm_expired: Decimal,
    pub gyp_expired: Decimal,

    /// Gross foreign-source income the source state withheld on; the treaty limb's base.
    pub foreign_gross_income: Decimal,
    /// The same income net of attributable expenses and of whatever compensation removed from the
    /// group it sits in; the average-rate limb's base (TEAC RG 00/08643/2023).
    pub foreign_taxable_income: Decimal,
    pub total_foreign_tax_credit: Decimal,

    pub savings_base: Decimal,
    pub savings_quota: Decimal,
    /// Tipo medio de gravamen del ahorro, the cap on the double-taxation credit.
    pub average_savings_rate: Decimal,
    pub net_tax_due: Decimal,

    /// Annual dividend exemption in force: €1,500 under Gipuzkoa (NF 3/2014 art. 9.24), 0 under
    /// Territorio Común.
    dividend_exemption_limit: Decimal,
    /// How far, and in what order, a negative balance in one savings-base group may reach the other.
    cross_offset: CrossOffset,
    /// Treaty cap on the source state's withholding, used for the credit's first limb.
    treaty_rate: Decimal,
    /// Fraction of the non-exempt gross securities income that caps deductible custody fees:
    /// `Some(0.03)` under Navarra (TRLFIRPF art. 32.1.a), `None` where the regime has no ceiling.
    custody_fee_cap_fraction: Option<Decimal>,
    /// Whether the regime exempts a year of small onerous transmissions (TRLFIRPF art. 39.5.d).
    small_disposals_exemption_applies: bool,
}

impl SpanishTaxStatement {
    pub fn new(
        year: i32,
        regime: SpanishTaxRegime,
        scale: SavingsScale,
        rcm_ledger: LossLedger,
        gyp_ledger: LossLedger,
        cross_offset: CrossOffset,
        treaty_rate: Decimal,
        dividend_exemption_limit: Decimal,
        custody_fee_cap_fraction: Option<Decimal>,
        small_disposals_exemption_applies: bool,
    ) -> SpanishTaxStatement {
        SpanishTaxStatement {
            year,
            regime,
            scale,
            cross_offset,
            treaty_rate,
            dividend_exemption_limit,
            custody_fee_cap_fraction,
            small_disposals_exemption_applies,
            custody_fee_cap: None,
            total_capped_fees: Decimal::ZERO,
            small_disposals_proceeds: Decimal::ZERO,
            small_disposals_gains: Decimal::ZERO,
            small_disposals_exemption: Decimal::ZERO,
            small_disposals_unmeasurable: false,
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
            prior_cross_offset_rcm_to_gyp: Decimal::ZERO,
            prior_cross_offset_gyp_to_rcm: Decimal::ZERO,
            rcm_expired: Decimal::ZERO,
            gyp_expired: Decimal::ZERO,
            foreign_gross_income: Decimal::ZERO,
            foreign_taxable_income: Decimal::ZERO,
            total_foreign_tax_credit: Decimal::ZERO,
            capital_gains: Vec::new(),
            dividends: Vec::new(),
            interest: Vec::new(),
            fees: Vec::new(),
            fx_gains: Vec::new(),
            fx_borrowed_review: Vec::new(),
            stock_grants: Vec::new(),
            corporate_actions: Vec::new(),
            short_positions: Vec::new(),
            abatement_lots: Vec::new(),
            wash_sale_unpriced_years: Vec::new(),
            wash_sale_reintegrations: Vec::new(),
            deferred_losses_next: Vec::new(),
            wash_sale_window_gaps: Vec::new(),
            wash_sale_venue_reviews: Vec::new(),
            wash_sale_boundary_reviews: Vec::new(),
            total_deferred_loss: Decimal::ZERO,
            total_reintegrated_loss: Decimal::ZERO,
            total_dividend_income: Decimal::ZERO,
            total_dividend_exemption: Decimal::ZERO,
            total_interest_income: Decimal::ZERO,
            total_paid_interest: Decimal::ZERO,
            total_deductible_fees: Decimal::ZERO,
            total_informational_fees: Decimal::ZERO,
            total_foreign_withholding: Decimal::ZERO,
            total_capital_gains: Decimal::ZERO,
            total_fx_gains: Decimal::ZERO,
            total_fx_losses: Decimal::ZERO,
            total_fx_result: Decimal::ZERO,
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
        self.total_interest_income = self
            .interest
            .iter()
            .filter(|entry| entry.taxable)
            .map(|entry| entry.gross_eur)
            .sum();
        self.total_paid_interest = self
            .interest
            .iter()
            .filter(|entry| !entry.taxable)
            .map(|entry| -entry.gross_eur)
            .sum();
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

        // NF 3/2014 art. 9.24 exempts the first €1,500 of qualifying dividends each year, so that
        // slice never enters the base at all. Territorio Común lost the same relief when Ley
        // 26/2014 repealed LIRPF art. 7.y with effect from 2015, and carries a zero limit.
        let eligible_dividends: Decimal = self
            .dividends
            .iter()
            .filter(|entry| entry.exemption_eligible)
            .map(|entry| entry.gross_eur)
            .sum();
        // Clamped at zero: a reversed dividend can leave the eligible pool net-negative, and a
        // negative exemption would be *added* to the base below.
        self.total_dividend_exemption = std::cmp::min(
            self.dividend_exemption_limit,
            std::cmp::max(Decimal::ZERO, eligible_dividends),
        );

        self.apply_custody_fee_cap();

        self.rcm_net = self.total_dividend_income + self.total_interest_income
            - self.total_deductible_fees
            - self.total_dividend_exemption;

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
        self.total_capital_gains = self
            .capital_gains
            .iter()
            .map(|entry| entry.integrable_amount)
            .sum();
        self.total_fx_result = self.fx_gains.iter().map(|entry| entry.amount_eur).sum();

        self.total_deferred_loss = self
            .capital_gains
            .iter()
            .map(|entry| entry.deferred_loss)
            .sum();
        self.total_reintegrated_loss = self
            .wash_sale_reintegrations
            .iter()
            .map(|entry| entry.released_eur)
            .sum();

        self.collect_abatement_lots();
        self.apply_small_disposals_exemption();

        // A released deferral is a loss that was blocked when it arose and is deductible now, so it
        // enters the group as a negative amount in the year the blocking shares left the estate.
        self.gyp_net = self.total_capital_gains + self.total_fx_result
            - self.total_reintegrated_loss
            - self.small_disposals_exemption;

        let compensation = compensate_savings_base(
            self.year,
            self.rcm_net,
            self.gyp_net,
            self.rcm_ledger_prior.clone(),
            self.gyp_ledger_prior.clone(),
            self.cross_offset,
        );

        self.rcm_taxable = compensation.rcm_taxable;
        self.gyp_taxable = compensation.gyp_taxable;
        self.rcm_applied = compensation.rcm_applied;
        self.gyp_applied = compensation.gyp_applied;
        self.cross_offset_rcm_to_gyp = compensation.cross_offset_rcm_to_gyp;
        self.cross_offset_gyp_to_rcm = compensation.cross_offset_gyp_to_rcm;
        self.prior_cross_offset_rcm_to_gyp = compensation.prior_cross_offset_rcm_to_gyp;
        self.prior_cross_offset_gyp_to_rcm = compensation.prior_cross_offset_gyp_to_rcm;
        self.rcm_ledger_next = compensation.rcm_ledger_next;
        self.gyp_ledger_next = compensation.gyp_ledger_next;
        self.rcm_expired = compensation.rcm_expired;
        self.gyp_expired = compensation.gyp_expired;

        self.savings_base = compensation.savings_base;
        self.savings_quota = self.scale.tax(self.savings_base);
        self.average_savings_rate = self.scale.average_rate(self.savings_base);

        // The credit's second limb is the average savings rate, which does not exist until the
        // whole base is known, so the credit itself can only be computed here.
        self.foreign_gross_income = self.dividends.iter().map(|entry| entry.gross_eur).sum();
        self.foreign_taxable_income = self.foreign_income_reaching_the_base();

        self.total_foreign_tax_credit = double_taxation_credit(
            self.treaty_limb(),
            self.foreign_taxable_income,
            self.average_savings_rate,
        );

        self.net_tax_due = std::cmp::max(
            Decimal::ZERO,
            self.savings_quota - self.total_foreign_tax_credit,
        );
    }

    /// Cut-off of the abatement regimes: TRLFIRPF DT 7.ª reaches "elementos patrimoniales adquiridos
    /// **antes de** 31 de diciembre de 1994", so an acquisition on that day itself is outside it.
    const ABATEMENT_CUTOFF: (i32, u32, u32) = (1994, 12, 31);

    /// Note every FIFO lot old enough to fall under an abatement regime.
    ///
    /// One entry per (symbol, acquisition date): several sales can consume the same lot, and naming
    /// the same 1993 purchase three times would only make the message harder to act on.
    fn collect_abatement_lots(&mut self) {
        let (year, month, day) = Self::ABATEMENT_CUTOFF;
        let Some(cutoff) = Date::from_ymd_opt(year, month, day) else {
            return;
        };

        let mut lots: Vec<(String, Date)> = self
            .capital_gains
            .iter()
            .flat_map(|entry| {
                entry
                    .lots
                    .iter()
                    .filter(|lot| lot.acquisition_date < cutoff)
                    .map(|lot| (entry.symbol.clone(), lot.acquisition_date))
            })
            .collect();

        lots.sort();
        lots.dedup();
        self.abatement_lots = lots;
    }

    /// The sentence every surface reports an uncomputed abatement regime with. `None` where the
    /// regime has none the tool leaves out, or where no lot is old enough to reach it.
    ///
    /// TRLFIRPF DT 7.ª reduces, and above a holding period exempts outright, the part of a gain
    /// generated before 31 December 2006 on an element acquired before 31 December 1994. Navarra put
    /// no €400,000 lifetime cap on it, unlike the state regime. It is not computed here because DT
    /// 7.ª.3 measures the pre-2006 part against the element's 2006 Impuesto sobre el Patrimonio
    /// value, which no broker statement carries.
    pub fn abatement_message(&self) -> Option<String> {
        if self.regime != SpanishTaxRegime::Navarra || self.abatement_lots.is_empty() {
            return None;
        }

        let lots = self
            .abatement_lots
            .iter()
            .map(|(symbol, date)| format!("{symbol} acquired {date}"))
            .collect::<Vec<_>>()
            .join(", ");

        Some(format!(
            "The year's disposals consumed FIFO lots acquired before 31 December 1994 ({lots}). \
             TRLFIRPF DT 7.ª reduces the part of such a gain that was generated before 31 December \
             2006, and exempts it entirely above a holding period — with no €400,000 lifetime cap, \
             unlike the state regime. The tool does NOT compute it: DT 7.ª.3 measures that part \
             against the element's 2006 Impuesto sobre el Patrimonio value, which a broker statement \
             does not carry. The gains above are therefore OVERSTATED. See the \
             open-interpretations register in docs/spain-taxes.md."
        ))
    }

    /// Exempt a year of small onerous transmissions (TRLFIRPF art. 39.5.d).
    ///
    /// The two figures the article's conditions are measured on are the year's global transmission
    /// amount and the taxable increments those transmissions produced. Proceeds are counted for
    /// every disposal, gain- or loss-making, because 1.º measures the transmissions; only positive
    /// integrable results feed the increment, because a loss is a *disminución*. A wash-sale
    /// deferral therefore leaves the proceeds alone and keeps the blocked amount out of both.
    ///
    /// Foreign-currency conversions are transmissions too (art. 54.1.b), but the shared FX FIFO
    /// records only the realized result, never the amount converted. Counting the securities alone
    /// would understate the global amount and could hand the exemption to a year that does not
    /// qualify, so the exemption is withheld and the reason reported. It is withheld only where it
    /// could have applied: once the securities' own proceeds exceed €3,000 the condition has
    /// definitively failed, since the missing conversions can only add to the total.
    fn apply_small_disposals_exemption(&mut self) {
        if !self.small_disposals_exemption_applies {
            return;
        }

        self.small_disposals_proceeds = self
            .capital_gains
            .iter()
            .map(|entry| entry.proceeds_eur)
            .sum();
        self.small_disposals_gains = self
            .capital_gains
            .iter()
            .map(|entry| std::cmp::max(Decimal::ZERO, entry.integrable_amount))
            .sum();

        if self.small_disposals_proceeds > GLOBAL_TRANSMISSION_LIMIT {
            return;
        }

        if !self.fx_gains.is_empty()
            && (self.small_disposals_gains > Decimal::ZERO || self.total_fx_gains > Decimal::ZERO)
        {
            self.small_disposals_unmeasurable = true;
            return;
        }

        self.small_disposals_exemption =
            small_disposals_exemption(self.small_disposals_proceeds, self.small_disposals_gains);
    }

    /// The sentence every surface reports art. 39.5.d with, so the console, the log and the CSV
    /// cannot drift apart. `None` when the article changed nothing this year.
    pub fn small_disposals_message(&self) -> Option<String> {
        if self.small_disposals_unmeasurable {
            return Some(format!(
                "The year's securities transmissions came to €{}, under the €3,000 that TRLFIRPF \
                 art. 39.5.d exempts, but the exemption was NOT applied: the year also contains \
                 foreign-currency conversions, which are transmissions too, and the tool records \
                 only their result, not the amount converted. The global transmission amount is \
                 therefore unknown and may well exceed €3,000. Not applying it overstates the tax \
                 rather than understating it — add the converted amounts by hand to check. See the \
                 open-interpretations register in docs/spain-taxes.md.",
                super::format_eur(self.small_disposals_proceeds)
            ));
        }

        if self.small_disposals_exemption <= Decimal::ZERO {
            return None;
        }

        Some(format!(
            "€{} of transmission gains was exempted under TRLFIRPF art. 39.5.d: the year's onerous \
             transmissions came to €{}, at or under the €3,000 the article allows, so the gain is \
             exempt up to half that global amount and only the excess is taxed. The year's taxable \
             increments were €{}. See the open-interpretations register in docs/spain-taxes.md.",
            super::format_eur(self.small_disposals_exemption),
            super::format_eur(self.small_disposals_proceeds),
            super::format_eur(self.small_disposals_gains)
        ))
    }

    /// Clamp deductible custody and administration fees to the regime's ceiling.
    ///
    /// TRLFIRPF art. 32.1.a allows them "con el límite del 3 por 100 de los ingresos íntegros, que
    /// no hayan resultado exentos, procedentes de dichos valores". Two readings are pinned here:
    ///
    /// - the ceiling is measured on the **securities'** income, which in a broker statement means
    ///   dividends. Interest credited on a cash balance is a cesión de capitales propios under art.
    ///   29, not income from a valor negociable, so it does not raise the ceiling;
    /// - "que no hayan resultado exentos" removes any exempt slice from that base. Navarra has no
    ///   dividend exemption, so the subtraction is a no-op there and exists for the rule's sake.
    ///
    /// A regime with no ceiling leaves everything untouched. What the ceiling disallows is reported
    /// as its own figure rather than folded into the informational fees, which are a different
    /// thing: those were never deductible, this was, up to a limit.
    fn apply_custody_fee_cap(&mut self) {
        let Some(fraction) = self.custody_fee_cap_fraction else {
            return;
        };

        let base = std::cmp::max(
            Decimal::ZERO,
            self.total_dividend_income - self.total_dividend_exemption,
        );
        let cap = base * fraction;

        self.custody_fee_cap = Some(cap);
        if self.total_deductible_fees > cap {
            self.total_capped_fees = self.total_deductible_fees - cap;
            self.total_deductible_fees = cap;
        }
    }

    /// The sentence every surface reports a binding fee ceiling with, so the console, the log and
    /// the CSV cannot drift apart. `None` when the ceiling did not bite.
    pub fn custody_fee_cap_message(&self) -> Option<String> {
        if self.total_capped_fees <= Decimal::ZERO {
            return None;
        }

        Some(format!(
            "€{} of otherwise deductible custody and administration fees was NOT deducted: \
             TRLFIRPF art. 32.1.a caps them at 3% of the non-exempt gross income from the \
             securities, which is €{} here, and the year's qualifying fees came to €{}. The \
             ceiling is measured on dividend income; interest credited on a cash balance is not \
             income from a valor negociable and does not raise it. See the open-interpretations \
             register in docs/spain-taxes.md.",
            super::format_eur(self.total_capped_fees),
            super::format_eur(self.custody_fee_cap.unwrap_or_default()),
            super::format_eur(self.total_deductible_fees + self.total_capped_fees)
        ))
    }

    /// Prior-year RCM balances this year applied **within their own group** — the Fase 2ª-1º
    /// consumption alone.
    ///
    /// `rcm_applied.used_total` is the whole ledger consumption and already contains what Fase 2ª-2º
    /// crossed into the ganancias group, which `prior_cross_offset_rcm_to_gyp` reports on its own.
    /// Reporting both in full prints the crossed amount twice, and a filer transcribing the two
    /// rows claims it twice. The rows are therefore disjoint, and their sum is the consumption.
    pub fn rcm_own_group_losses_applied(&self) -> Decimal {
        self.rcm_applied.used_total - self.prior_cross_offset_rcm_to_gyp
    }

    /// The ganancias mirror of [`Self::rcm_own_group_losses_applied`].
    pub fn gyp_own_group_losses_applied(&self) -> Decimal {
        self.gyp_applied.used_total - self.prior_cross_offset_gyp_to_rcm
    }

    /// Prior-year saldos this year crossed into the other savings-base group.
    pub fn prior_cross_offset(&self) -> Decimal {
        self.prior_cross_offset_rcm_to_gyp + self.prior_cross_offset_gyp_to_rcm
    }

    /// The sentence every surface reports the Navarra carried-saldo cross with, so the console, the
    /// log and the CSV cannot drift apart. `None` when nothing turned on the open reading.
    ///
    /// TRLFIRPF art. 54.2 opens its 25% cross-offset for a negative *current-year* result. The
    /// carry sentence sends what is left into the following four years "en el mismo orden
    /// establecido en los párrafos anteriores", which the tool reads as repeating that cross for a
    /// carried saldo too. Nothing published by the Hacienda Foral de Navarra settles the point, and
    /// the amount below is exactly what the reading is responsible for: on the narrow one it would
    /// stay pending and the savings base would be that much higher.
    pub fn carried_cross_offset_message(&self) -> Option<String> {
        if self.cross_offset != CrossOffset::NavarraOrdered {
            return None;
        }

        let crossed = self.prior_cross_offset();
        if crossed <= Decimal::ZERO {
            return None;
        }

        Some(format!(
            "€{} of prior-year negative savings-base saldos was set against the other group under \
             TRLFIRPF art. 54.2. The article opens that 25% cross-offset for a negative \
             current-year result; the tool reads the four-year carry rule's \"en el mismo orden \
             establecido en los párrafos anteriores\" as repeating it for a saldo carried in from an \
             earlier year, which is what allowed this amount. No Hacienda Foral de Navarra manual or \
             consulta settles the point. On the narrower reading the amount would stay pending and \
             the savings base would be €{} higher. See the open-interpretations register in \
             docs/spain-taxes.md.",
            super::format_eur(crossed),
            super::format_eur(crossed)
        ))
    }

    /// The credit's first limb, summed payment by payment.
    ///
    /// Two things make this a per-row figure rather than a year-level one:
    ///
    /// - a treaty caps what the **source** state may levy on each payment, so pooling the year's
    ///   withholding against the year's gross would let a dividend withheld at 0% lend its unused
    ///   headroom to one withheld at 30%;
    /// - income Spain exempts bears no Spanish tax, so it carries no credit either. Each row is
    ///   therefore measured on the slice of it that survives the exemption. The exempt slice is
    ///   spread pro rata across the exemption-eligible rows, which is where the relief lands; a row
    ///   the anti-abuse clause excluded keeps its full gross.
    ///
    /// The per-row `DividendEntry.treaty_capped_credit` is a different figure with a different use:
    /// it is measured on the full gross, because that is what a reclaim from the source state is
    /// measured against.
    fn treaty_limb(&self) -> Decimal {
        let eligible: Decimal = self
            .dividends
            .iter()
            .filter(|entry| entry.exemption_eligible)
            .map(|entry| entry.gross_eur)
            .sum();

        self.dividends
            .iter()
            .map(|entry| {
                let taxed_gross = if entry.exemption_eligible && eligible > Decimal::ZERO {
                    entry.gross_eur - self.total_dividend_exemption * entry.gross_eur / eligible
                } else {
                    entry.gross_eur
                };

                treaty_capped_credit(
                    entry.withheld_eur,
                    std::cmp::max(Decimal::ZERO, taxed_gross),
                    self.treaty_rate,
                )
            })
            .sum()
    }

    /// Foreign-source income as it actually reaches the base liquidable.
    ///
    /// TEAC RG 00/08643/2023 (20-10-2025, unificación de criterio, binding per LGT art. 239.8):
    /// the average rate applies to **rentas netas**, the foreign income "una vez deducidos los
    /// gastos y compensadas las rentas". Two reductions follow, in that order:
    ///
    /// 1. deductible expenses, pro-rated by the foreign share of the RCM income they were incurred
    ///    against (Común only — Gipuzkoa deducts nothing, so this is a no-op there);
    /// 2. whatever compensation removed from the RCM group, pro-rated the same way. A group the
    ///    prior-year balances wiped carries no foreign income into the base at all, so there is
    ///    nothing left for a credit to attach to.
    ///
    /// Only dividends carry foreign withholding in the statements this tool supports, so the group
    /// in question is always RCM.
    fn foreign_income_reaching_the_base(&self) -> Decimal {
        let gross = self.foreign_gross_income;
        if gross <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        // Exempt income bears no Spanish tax, so it carries no credit either. Only dividends are
        // exempted and only dividends carry foreign withholding, so the whole exemption comes off
        // this base.
        let gross = std::cmp::max(Decimal::ZERO, gross - self.total_dividend_exemption);
        if gross.is_zero() {
            return Decimal::ZERO;
        }

        // Both sides of the pro-ration are measured after the exemption, so the fraction is the
        // foreign share of the RCM income that is actually taxed.
        let rcm_gross =
            self.total_dividend_income + self.total_interest_income - self.total_dividend_exemption;
        let net = if rcm_gross > Decimal::ZERO {
            gross - self.total_deductible_fees * gross / rcm_gross
        } else {
            gross
        };

        if self.rcm_net <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        let surviving = self.rcm_taxable / self.rcm_net;
        std::cmp::max(Decimal::ZERO, net * surviving)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn statement() -> SpanishTaxStatement {
        let regime = SpanishTaxRegime::Gipuzkoa;
        SpanishTaxStatement::new(
            2026,
            regime,
            SavingsScale::for_year(regime, 2026).unwrap(),
            LossLedger::default(),
            LossLedger::default(),
            CrossOffset::None,
            dec!(0.15),
            Decimal::ZERO,
            None,
            false,
        )
    }

    fn disposal(result: Decimal) -> CapitalGainEntry {
        CapitalGainEntry {
            symbol: "AAPL".to_string(),
            isin: String::new(),
            description: String::new(),
            sale_date: Date::from_ymd_opt(2026, 6, 15).unwrap(),
            settle_date: Date::from_ymd_opt(2026, 6, 17).unwrap(),
            quantity: dec!(1),
            proceeds_eur: result,
            cost_eur: Decimal::ZERO,
            actualized_cost_eur: Decimal::ZERO,
            fiscal_gain_loss: result,
            deferred_loss: Decimal::ZERO,
            integrable_amount: result,
            lots: Vec::new(),
            notes: None,
        }
    }

    fn dividend(gross: Decimal, withheld: Decimal, exemption_eligible: bool) -> DividendEntry {
        DividendEntry {
            symbol: "AAPL".to_string(),
            isin: String::new(),
            description: String::new(),
            date: Date::from_ymd_opt(2026, 5, 20).unwrap(),
            gross_eur: gross,
            withheld_eur: withheld,
            treaty_capped_credit: Decimal::ZERO,
            exemption_eligible,
            notes: None,
        }
    }

    /// The chain `simulate-sell` builds: the year's tax after each disposal is added in turn.
    /// Mirrors `SpanishTaxSimulation::observe`, which recomputes the year once per emulated sale.
    fn tax_after_each(statement: &mut SpanishTaxStatement, results: &[Decimal]) -> Vec<Decimal> {
        results
            .iter()
            .map(|&result| {
                statement.capital_gains.push(disposal(result));
                statement.calculate_totals();
                statement.net_tax_due
            })
            .collect()
    }

    /// The pricing rule behind `simulate-sell`: a hypothetical disposal costs what it *adds* to the
    /// year. Under a progressive savings scale three identical disposals therefore cost three
    /// different amounts, rising as the base climbs through the brackets — which is exactly what the
    /// flat `localities::spain` approximation cannot express.
    #[test]
    fn hypothetical_disposal_is_priced_at_what_it_adds_to_the_year() {
        let mut spain = statement();
        let taxes = tax_after_each(&mut spain, &[dec!(7500), dec!(7500), dec!(7500)]);

        // Gipuzkoa 2026: 19% to 7,500, then 20% to 15,000, then 22%.
        assert_eq!(taxes[0], dec!(1425));
        assert_eq!(taxes[1], dec!(2925));
        assert_eq!(taxes[2], dec!(4575));

        let marginal: Vec<Decimal> = (0..3)
            .map(|index| taxes[index] - if index == 0 { Decimal::ZERO } else { taxes[index - 1] })
            .collect();
        assert_eq!(marginal, vec![dec!(1425), dec!(1500), dec!(1650)]);

        // The standalone comparator the deduction column is measured against is the same €1,425 for
        // every one of them: taxed alone, each disposal starts from the first bracket.
        for result in &marginal {
            assert!(*result >= spain.scale.tax(dec!(7500)));
        }
        assert_eq!(spain.scale.tax(dec!(7500)), dec!(1425));
    }

    /// A loss-making disposal is worth *negative* tax when the year holds a gain for it to shelter.
    /// That is the in-year compensation within the ganancias group, and the reason a trim can be
    /// cheaper than it looks.
    #[test]
    fn hypothetical_loss_shelters_a_gain_booked_earlier_in_the_year() {
        let mut spain = statement();
        let taxes = tax_after_each(&mut spain, &[dec!(10000), dec!(-4000)]);

        // 1,425 + 2,500 × 20%.
        assert_eq!(taxes[0], dec!(1925));
        // Base falls to 6,000, entirely in the 19% bracket.
        assert_eq!(taxes[1], dec!(1140));
        assert_eq!(taxes[1] - taxes[0], dec!(-785));
    }

    /// Under Territorio Común a prior-year RCM balance that wipes the group the foreign dividends
    /// sit in leaves nothing for the credit to attach to — even though the year still pays tax on
    /// its ganancias, so the average rate is nowhere near zero.
    ///
    /// TEAC RG 00/08643/2023: the average-rate limb takes the income "una vez deducidos los gastos
    /// y compensadas las rentas". Measuring it on the gross would credit €135 against foreign
    /// income the Spanish return never taxed.
    #[test]
    fn compensation_that_wipes_the_group_leaves_no_credit() {
        let regime = SpanishTaxRegime::Comun;
        let prior_rcm = LossLedger::from_config(
            &BTreeMap::from([(2024, dec!(5000))]),
            2026,
            "rcm",
        )
        .unwrap();

        let mut spain = SpanishTaxStatement::new(
            2026,
            regime,
            SavingsScale::for_year(regime, 2026).unwrap(),
            prior_rcm,
            LossLedger::default(),
            CrossOffset::AeatTwoPhase,
            dec!(0.15),
            Decimal::ZERO,
            None,
            false,
        );

        spain.dividends.push(DividendEntry {
            symbol: "AAPL".to_string(),
            isin: String::new(),
            description: String::new(),
            date: Date::from_ymd_opt(2026, 5, 20).unwrap(),
            gross_eur: dec!(900),
            withheld_eur: dec!(270),
            treaty_capped_credit: dec!(135),
            exemption_eligible: false,
            notes: None,
        });
        spain.capital_gains.push(disposal(dec!(10000)));
        spain.calculate_totals();

        // The whole €900 of RCM is absorbed, and the leftover crosses into the ganancias group up
        // to 25% of it.
        assert_eq!(spain.rcm_taxable, dec!(0));
        assert_eq!(spain.gyp_taxable, dec!(7500));
        assert_eq!(spain.savings_base, dec!(7500));
        // State scale: 6,000 × 19% + 1,500 × 21% = 1,455, so the average rate is 19.40%.
        assert_eq!(spain.savings_quota, dec!(1455));
        assert_eq!(spain.average_savings_rate, dec!(0.194));

        assert_eq!(spain.foreign_gross_income, dec!(900));
        assert_eq!(spain.foreign_taxable_income, dec!(0));
        assert_eq!(spain.total_foreign_tax_credit, dec!(0));
        assert_eq!(spain.net_tax_due, dec!(1455));
    }

    /// Interest paid on a margin loan is reported but never netted off the interest received:
    /// neither statute allows an expense against securities income beyond LIRPF art. 26.1.a's
    /// administration and custody, so the RCM result is the credit interest alone.
    #[test]
    fn paid_interest_is_reported_outside_the_rcm_result() {
        let mut spain = statement();
        spain.interest.push(InterestEntry {
            date: Date::from_ymd_opt(2026, 6, 30).unwrap(),
            description: "Broker interest".to_string(),
            gross_eur: dec!(90),
            taxable: true,
            notes: None,
        });
        spain.interest.push(InterestEntry {
            date: Date::from_ymd_opt(2026, 9, 30).unwrap(),
            description: "Broker interest paid".to_string(),
            gross_eur: dec!(-225),
            taxable: false,
            notes: Some("informational".to_string()),
        });
        spain.calculate_totals();

        assert_eq!(spain.total_interest_income, dec!(90));
        assert_eq!(spain.total_paid_interest, dec!(225));
        assert_eq!(spain.rcm_net, dec!(90));
    }

    /// A reversed dividend can leave the exemption-eligible pool net-negative. An exemption is
    /// relief, never a charge, so it floors at zero — the unclamped `min(1_500, −200)` would have
    /// *added* €200 to the base.
    #[test]
    fn a_net_negative_eligible_pool_exempts_nothing() {
        let regime = SpanishTaxRegime::Gipuzkoa;
        let mut spain = SpanishTaxStatement::new(
            2026,
            regime,
            SavingsScale::for_year(regime, 2026).unwrap(),
            LossLedger::default(),
            LossLedger::default(),
            CrossOffset::None,
            dec!(0.15),
            dec!(1500),
            None,
            false,
        );

        spain.dividends.push(dividend(dec!(300), dec!(45), true));
        spain.dividends.push(dividend(dec!(-500), dec!(-75), true));
        spain.calculate_totals();

        assert_eq!(spain.total_dividend_income, dec!(-200));
        assert_eq!(spain.total_dividend_exemption, dec!(0));
        assert_eq!(spain.rcm_net, dec!(-200));
        assert_eq!(spain.savings_base, dec!(0));
        assert_eq!(spain.total_foreign_tax_credit, dec!(0));
    }

    /// A loss the valores-homogéneos rule deferred shelters nothing: the disposal enters the base at
    /// its integrable amount, not its gross result, so the simulation prices it at what the return
    /// would actually allow.
    #[test]
    fn a_deferred_loss_is_priced_at_its_integrable_amount() {
        let mut spain = statement();
        spain.capital_gains.push(disposal(dec!(10000)));

        let mut deferred = disposal(dec!(-4000));
        deferred.deferred_loss = dec!(4000);
        deferred.integrable_amount = Decimal::ZERO;
        spain.capital_gains.push(deferred);
        spain.calculate_totals();

        // The whole loss was blocked, so the year is unchanged by it.
        assert_eq!(spain.gyp_net, dec!(10000));
        assert_eq!(spain.net_tax_due, dec!(1925));
    }
}

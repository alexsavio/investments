//! The Spanish tax statement model and its year-level totals.

use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::spain::carryforward::{LedgerApplication, LossLedger};
use crate::taxes::spain::compensation::compensate_savings_base;
use crate::taxes::spain::credit::double_taxation_credit;
use crate::taxes::DeferredLossConfig;
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

    /// Short (negative-quantity) positions held at the statement's end, reported for information
    /// only. Their treatment is not computed and needs manual review.
    pub short_positions: Vec<(String, Decimal)>,

    /// Disposal years the statement contains but the tool ships no actualization table for. Sales
    /// in those years were replayed for the valores-homogéneos rule — they still consume lots and
    /// release earlier deferrals — but their own result could not be priced, so a loss in one of
    /// them was never tested for deferral.
    pub wash_sale_unpriced_years: Vec<i32>,

    /// Blocked lots still standing at the end of the statement, in the shape next year's
    /// `taxes.spain.deferred_losses` takes.
    pub deferred_losses_next: Vec<DeferredLossConfig>,

    /// Loss-making disposals whose +2-month repurchase window reaches past the statement's last
    /// date, so a repurchase that would defer the loss cannot be seen yet.
    pub wash_sale_window_gaps: Vec<WashSaleWindowGap>,

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

    /// Current-year negative of one group set against the other (Territorio Común only, Fase 1ª).
    pub cross_offset_rcm_to_gyp: Decimal,
    pub cross_offset_gyp_to_rcm: Decimal,

    /// Prior-year balance of one group its own group could not absorb, set against the other's
    /// remainder (Territorio Común only, Fase 2ª-2º).
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
        dividend_exemption_limit: Decimal,
    ) -> SpanishTaxStatement {
        SpanishTaxStatement {
            year,
            regime,
            scale,
            cross_offset_fraction,
            treaty_rate,
            dividend_exemption_limit,
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
            short_positions: Vec::new(),
            wash_sale_unpriced_years: Vec::new(),
            wash_sale_reintegrations: Vec::new(),
            deferred_losses_next: Vec::new(),
            wash_sale_window_gaps: Vec::new(),
            total_deferred_loss: Decimal::ZERO,
            total_reintegrated_loss: Decimal::ZERO,
            total_dividend_income: Decimal::ZERO,
            total_dividend_exemption: Decimal::ZERO,
            total_interest_income: Decimal::ZERO,
            total_paid_interest: Decimal::ZERO,
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
        self.total_dividend_exemption =
            std::cmp::min(self.dividend_exemption_limit, eligible_dividends);

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
        let capital_gains: Decimal = self
            .capital_gains
            .iter()
            .map(|entry| entry.integrable_amount)
            .sum();
        let fx: Decimal = self.fx_gains.iter().map(|entry| entry.amount_eur).sum();

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

        // A released deferral is a loss that was blocked when it arose and is deductible now, so it
        // enters the group as a negative amount in the year the blocking shares left the estate.
        self.gyp_net = capital_gains + fx - self.total_reintegrated_loss;

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
        self.prior_cross_offset_rcm_to_gyp = compensation.prior_cross_offset_rcm_to_gyp;
        self.prior_cross_offset_gyp_to_rcm = compensation.prior_cross_offset_gyp_to_rcm;
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
        self.foreign_gross_income = self.dividends.iter().map(|entry| entry.gross_eur).sum();
        self.foreign_taxable_income = self.foreign_income_reaching_the_base();

        self.total_foreign_tax_credit = double_taxation_credit(
            self.total_foreign_withholding,
            self.foreign_gross_income,
            self.foreign_taxable_income,
            self.treaty_rate,
            self.average_savings_rate,
        );

        self.net_tax_due = std::cmp::max(
            Decimal::ZERO,
            self.savings_quota - self.total_foreign_tax_credit,
        );
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

        let rcm_gross = self.total_dividend_income + self.total_interest_income;
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
            Decimal::ZERO,
            dec!(0.15),
            Decimal::ZERO,
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
            dec!(0.25),
            dec!(0.15),
            Decimal::ZERO,
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

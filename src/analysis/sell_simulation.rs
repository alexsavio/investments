use std::collections::BTreeMap;

use chrono::Datelike;
use itertools::Itertools;
use static_table_derive::StaticTable;

use crate::broker_statement::{BrokerStatement, StockSell, StockSellType};
use crate::commissions::CommissionCalc;
use crate::config::PortfolioConfig;
use crate::core::{EmptyResult, GenericResult};
use crate::currency::{Cash, MultiCurrencyCashAccount};
use crate::currency::converter::{CurrencyConverter, CurrencyConverterRc};
use crate::formatting::table::Cell;
use crate::instruments::InstrumentInfo;
use crate::localities::{Country, Jurisdiction};
use crate::quotes::Quotes;
use crate::tax_statement::germany::{self, GermanTaxStatement};
use crate::tax_statement::spain::{self, SpanishTaxStatement};
use crate::taxes::{IncomeType, LtoDeduction, long_term_ownership::LtoDeductionCalculator, Tax, TaxCalculator, TaxConfig};
use crate::trades;
use crate::types::{Date, Decimal};
use crate::util;

pub fn simulate_sell(
    country: &Country, portfolio: &PortfolioConfig, tax_config: &TaxConfig,
    mut statement: BrokerStatement, converter: CurrencyConverterRc, quotes: &Quotes,
    positions: Option<Vec<(String, Option<Decimal>)>>, base_currency: Option<&str>,
) -> EmptyResult {
    let (positions, all_positions) = match positions {
        Some(positions) => (positions, false),
        None => {
            let positions: Vec<_> = statement.open_positions.keys()
                .map(|symbol| (symbol.to_owned(), None))
                .sorted_unstable()
                .collect();

            if positions.is_empty() {
                println!("The portfolio has no open positions.");
                return Ok(())
            }

            (positions, true)
        }
    };

    for (symbol, _quantity) in &positions {
        if !all_positions {
            if !statement.open_positions.contains_key(symbol) {
                return Err!("The portfolio has no open {:?} positions", symbol);
            }
        }
        quotes.batch(statement.get_quote_query(symbol))?;
    }

    // Snapshot the tax year *before* anything is emulated: the disposals are priced at the
    // difference they make to it, so the untouched year is the reference point.
    let mut simulation = TaxSimulation::new(country, &statement, &converter, tax_config, portfolio)?;

    let net_value = statement.net_value(
        &converter, quotes, portfolio.currency(),
        all_positions // To be able to simulate sell for portfolio with symbols for which quotes aren't available
    )?;

    let mut commission_calc = CommissionCalc::new(
        converter.clone(), statement.broker.commission_spec.clone(), net_value)?;

    for (symbol, quantity) in &positions {
        let quantity = *match quantity {
            Some(quantity) => quantity,
            None => statement.open_positions.get(symbol).ok_or_else(|| format!(
                "The portfolio has no open {symbol:?} positions"))?,
        };

        let mut price = quotes.get(statement.get_quote_query(symbol))?;
        if let Some(base_currency) = base_currency {
            price = trades::convert_price(price, quantity, base_currency, &converter)?;
        }

        statement.emulate_sell(symbol, quantity, price, &mut commission_calc)?;

        if let Some(simulation) = simulation.as_mut() {
            // Price this disposal on a statement carrying exactly the sales made so far.
            // `emulate_sell` also draws down `open_positions`, which the German processor reads as
            // the year-end holdings behind the §19 InvStG "fully disposed" test, and each Spanish
            // sale can create or release a valores-homogéneos block the next one sees — so
            // deferring every step to the end of the loop would judge each disposal against a book
            // where all the others had already been sold, and pile their combined effect onto the
            // first row.
            statement.process_trades(None, false)?;
            simulation.observe(&statement, &converter, tax_config, portfolio)?;
        }
    }

    statement.process_trades(None, false)?;
    let additional_commissions = statement.emulate_commissions(commission_calc)?;

    let stock_sells = statement.stock_sells.iter()
        .filter(|stock_sell| stock_sell.emulation)
        .cloned().collect::<Vec<_>>();
    assert_eq!(stock_sells.len(), positions.len());

    print_results(
        country, portfolio, &statement.instrument_info, stock_sells, additional_commissions,
        &converter, simulation.as_ref())
}

/// The jurisdictions whose tax cannot be priced per disposal.
///
/// Germany and Spain both compute the charge over the *whole* year — German loss pots plus the
/// Sparer-Pauschbetrag, Spanish group compensation plus a progressive savings scale — so a disposal
/// is priced at the difference it makes to the year, not at a flat rate. Everywhere else the
/// per-trade estimate in `print_results` still applies.
// Both payloads are boxed: each carries one or two whole tax statements, and an unboxed enum would
// be as large as the bigger of the two on every stack frame that touches it.
enum TaxSimulation {
    German(Box<GermanTaxSimulation>),
    Spanish(Box<SpanishTaxSimulation>),
}

impl TaxSimulation {
    fn new(
        country: &Country, statement: &BrokerStatement, converter: &CurrencyConverter,
        tax_config: &TaxConfig, portfolio: &PortfolioConfig,
    ) -> GenericResult<Option<TaxSimulation>> {
        Ok(match country.jurisdiction {
            Jurisdiction::Germany => Some(TaxSimulation::German(Box::new(
                GermanTaxSimulation::new(statement, converter, tax_config, portfolio)?))),
            Jurisdiction::Spain => Some(TaxSimulation::Spanish(Box::new(
                SpanishTaxSimulation::new(statement, converter, tax_config)?))),
            _ => None,
        })
    }

    fn observe(
        &mut self, statement: &BrokerStatement, converter: &CurrencyConverter,
        tax_config: &TaxConfig, portfolio: &PortfolioConfig,
    ) -> EmptyResult {
        match self {
            TaxSimulation::German(german) =>
                german.observe(statement, converter, tax_config, portfolio),
            TaxSimulation::Spanish(spanish) => spanish.observe(statement, converter, tax_config),
        }
    }

    /// Tax the `index`-th simulated disposal adds to the year, given the ones before it.
    fn marginal(&self, index: usize) -> Decimal {
        match self {
            TaxSimulation::German(german) => german.marginal(index),
            TaxSimulation::Spanish(spanish) => spanish.marginal(index),
        }
    }

    /// What the `index`-th disposal would cost taxed on its own.
    fn standalone(&self, index: usize) -> Decimal {
        match self {
            TaxSimulation::German(german) => german.standalone(index),
            TaxSimulation::Spanish(spanish) => spanish.standalone(index),
        }
    }

    /// Tax the simulated disposals add to the year as a whole.
    fn total(&self) -> Decimal {
        match self {
            TaxSimulation::German(german) => german.total(),
            TaxSimulation::Spanish(spanish) => spanish.total(),
        }
    }

    fn print(&self) {
        match self {
            TaxSimulation::German(german) => german.print(),
            TaxSimulation::Spanish(spanish) => spanish.print(),
        }
    }
}

/// Year-level German tax for the disposals being simulated.
///
/// A German disposal has no tax price of its own. The charge falls out of the *whole* tax year:
/// the §20(6) EStG loss pots (share losses live in their own pot and offset only share gains), the
/// prior-year festgestellter Verlustvortrag, and whatever is left of the Sparer-Pauschbetrag.
/// Pricing a trim at the flat 26.375% `localities::germany` stand-in therefore invents tax that is
/// not owed — a gain is free while the year's Aktien pot is still negative, and only becomes
/// payable once the pot and the allowance are exhausted.
///
/// So the year is recomputed after each simulated sale is emulated, and each disposal is priced at
/// the difference it makes. A loss-making trim then shows as *negative* tax — correctly, because it
/// shelters a gain booked elsewhere in the same year.
///
/// Every step must observe a statement carrying exactly the sales made so far, never the finished
/// set: `emulate_sell` draws down `open_positions`, which the processor reads back as the year-end
/// holdings behind the §19 InvStG "fully disposed" test and the §18 Vorabpauschale. Computing the
/// steps after the fact would judge each disposal against a book where all the others had already
/// gone, and collapse their combined effect onto the first row.
///
/// Each row is rounded to cents on its own while the total is taken end-to-end, so summing the
/// rows can miss the total by a cent. That is the same trade-off `format_eur` documents: one
/// rounding per reported figure, rather than a total that disagrees with its own arithmetic.
struct GermanTaxSimulation {
    year: i32,

    /// `taxes[k]` is the year's net German tax when the first `k` simulated disposals happen, so
    /// `taxes[0]` is the untouched year. Always one longer than the number of disposals observed.
    taxes: Vec<Decimal>,

    /// What each disposal would cost taxed on its own at the §32d(1) flat rate, church tax
    /// included — the comparator the deduction column is measured against.
    standalone: Vec<Decimal>,

    /// The year before the simulated sales, and as of the last one observed, kept so the pots and
    /// the allowance that produced these figures can be shown alongside them.
    before: GermanTaxStatement,
    after: Option<GermanTaxStatement>,
}

impl GermanTaxSimulation {
    /// Snapshot the tax year before any sale is emulated. `statement` must be untouched.
    fn new(
        statement: &BrokerStatement, converter: &CurrencyConverter, tax_config: &TaxConfig,
        portfolio: &PortfolioConfig,
    ) -> GenericResult<GermanTaxSimulation> {
        let year = crate::exchanges::today_trade_conclusion_time().date.year();

        let (before, _has_income) = germany::compute_tax_year(
            statement, year, converter, tax_config,
            portfolio.foreign_currency_taxation, &portfolio.opening_foreign_currency)?;

        Ok(GermanTaxSimulation {
            year,
            taxes: vec![before.net_tax_due],
            standalone: Vec::new(),
            before,
            after: None,
        })
    }

    /// Record the year as it stands after one more sale has been emulated onto `statement`.
    fn observe(
        &mut self, statement: &BrokerStatement, converter: &CurrencyConverter,
        tax_config: &TaxConfig, portfolio: &PortfolioConfig,
    ) -> EmptyResult {
        let (year, _has_income) = germany::compute_tax_year(
            statement, self.year, converter, tax_config,
            portfolio.foreign_currency_taxation, &portfolio.opening_foreign_currency)?;

        // The processor emits one capital-gain entry per qualifying trade, in `stock_sells` order,
        // and `emulate_sell` appends — so this sale owns the last entry. Verify rather than trust
        // it: mispairing them would silently price the wrong trade.
        let trades: Vec<&StockSell> = statement.stock_sells.iter()
            .filter(|trade| germany::produces_capital_gain(trade, self.year))
            .collect();

        if trades.len() != year.capital_gains.len() {
            return Err!(
                "German tax simulation: {} yielded {} capital gain entries for {} trades",
                self.year, year.capital_gains.len(), trades.len());
        }

        let simulated = self.standalone.len() + 1;
        let real = trades.len().checked_sub(simulated).ok_or_else(|| format!(
            "German tax simulation: {} has only {} trades for {simulated} simulated sales",
            self.year, trades.len()))?;

        if !trades[real..].iter().all(|trade| trade.emulation) ||
           trades[..real].iter().any(|trade| trade.emulation) {
            return Err!(
                "German tax simulation: the simulated sales aren't the trailing {} trades",
                self.year);
        }

        // What this disposal costs on its own, at the same §32d(1) rates the year uses — so the
        // deduction column compares like with like. Deriving it from the flat
        // `localities::germany` stand-in instead would omit church tax and understate the
        // comparator, turning the deduction negative for an 8-9% filer.
        let entry = year.capital_gains.last().expect(
            "the checks above guarantee at least one capital gain entry for the simulated sale");
        self.standalone.push(year.tax_rates.compute_taxes(entry.taxable_amount, dec!(0)).total);

        self.taxes.push(year.net_tax_due);
        self.after = Some(year);

        Ok(())
    }

    /// Tax the `index`-th simulated disposal adds to the year, given the ones before it.
    fn marginal(&self, index: usize) -> Decimal {
        germany::round_eur(self.taxes[index + 1] - self.taxes[index])
    }

    /// What the `index`-th disposal would cost taxed on its own, before the year's pots and
    /// allowance are brought to bear.
    fn standalone(&self, index: usize) -> Decimal {
        germany::round_eur(self.standalone[index])
    }

    /// The year as of the last observed sale, falling back to the untouched year if none was
    /// observed (no simulated sale reached the German path).
    fn after(&self) -> &GermanTaxStatement {
        self.after.as_ref().unwrap_or(&self.before)
    }

    /// Tax the simulated disposals add to the year as a whole. Taken end-to-end rather than by
    /// summing the rows, so it stays exact; rounded rows may differ from it by a cent.
    fn total(&self) -> Decimal {
        germany::round_eur(self.taxes[self.taxes.len() - 1] - self.taxes[0])
    }

    /// Explain the marginal figures: without the pots and the allowance a reader cannot tell a
    /// genuinely untaxed disposal from a broken one.
    fn print(&self) {
        let mut table = GermanContextTable::new();

        let after = self.after();

        let mut row = |name: &str, before: Decimal, after: Decimal| {
            table.add_row(GermanContextRow {
                name: name.to_owned(),
                before: germany::format_eur(before),
                after: germany::format_eur(after),
            });
        };

        row("Taxable income (§20 EStG)",
            self.before.total_taxable_income, after.total_taxable_income);
        row("Sparer-Pauschbetrag used",
            self.before.sparer_pauschbetrag_used, after.sparer_pauschbetrag_used);
        row("Sparer-Pauschbetrag left",
            self.before.sparer_pauschbetrag - self.before.sparer_pauschbetrag_used,
            after.sparer_pauschbetrag - after.sparer_pauschbetrag_used);
        // Carried forward as positive magnitudes, exactly as the filed statement reports them.
        // A gain eats into the matching pot, so these shrink as the sale is added.
        row("Loss carryforward, shares (§20(6))",
            self.before.loss_carryforward_stock_next, after.loss_carryforward_stock_next);
        row("Loss carryforward, general",
            self.before.loss_carryforward_other_next, after.loss_carryforward_other_next);
        row("Net German tax due", self.before.net_tax_due, after.net_tax_due);

        table.print(&format!(
            "German tax year {} — the sale is priced at what it changes here, not at a flat rate",
            self.year));

        if self.taxes.len() > 2 {
            // Each row is the tax the *next* disposal adds once the ones above it have already
            // eaten into the pot and the allowance, so the split across rows depends on their
            // order and no single row is that position's standalone cost. Only the total is
            // order-independent. Say so — someone reading one row in isolation would be misled.
            println!(
                "Per-position tax is marginal and depends on row order: each is what that sale \n\
                 adds on top of the ones above it. Only the total is order-independent; simulate \n\
                 a position on its own to see its standalone cost.\n");
        }
    }
}

#[derive(StaticTable)]
#[table(name="GermanContextTable")]
struct GermanContextRow {
    #[column(name="")]
    name: String,
    #[column(name="Without the sale", align="right")]
    before: String,
    #[column(name="With the sale", align="right")]
    after: String,
}

/// Year-level Spanish tax for the disposals being simulated.
///
/// A Spanish disposal has no tax price of its own either, for reasons of its own. The savings scale
/// is **progressive**, so what a gain costs depends on how much base sits under it; the two
/// savings-base groups are compensated against prior-year balances (and, under Territorio Común,
/// against each other) before the scale is applied at all; under Gipuzkoa each FIFO lot's cost is
/// actualized by its acquisition year; and a loss on a holding repurchased inside the
/// valores-homogéneos window is deferred rather than deducted. Pricing a trim at the
/// `localities::spain` approximation would miss every one of those.
///
/// So the year is recomputed after each simulated sale and the disposal is priced at the difference
/// it makes. A loss-making trim shows as *negative* tax — correctly, because it shelters a gain
/// booked elsewhere in the same year — and a loss whose shares were just repurchased shows as no
/// deduction at all, which is exactly what the deferral rule does to it.
struct SpanishTaxSimulation {
    year: i32,

    /// `taxes[k]` is the year's net Spanish tax when the first `k` simulated disposals happen, so
    /// `taxes[0]` is the untouched year. Always one longer than the number of disposals observed.
    taxes: Vec<Decimal>,

    /// What each disposal would cost taxed on its own, from the first bracket up — the comparator
    /// the deduction column is measured against.
    standalone: Vec<Decimal>,

    before: SpanishTaxStatement,
    after: Option<SpanishTaxStatement>,
}

impl SpanishTaxSimulation {
    /// Snapshot the tax year before any sale is emulated. `statement` must be untouched.
    fn new(
        statement: &BrokerStatement, converter: &CurrencyConverter, tax_config: &TaxConfig,
    ) -> GenericResult<SpanishTaxSimulation> {
        let year = crate::exchanges::today_trade_conclusion_time().date.year();
        let (before, _has_income) = spain::compute_tax_year(statement, year, converter, tax_config)?;

        Ok(SpanishTaxSimulation {
            year,
            taxes: vec![before.net_tax_due],
            standalone: Vec::new(),
            before,
            after: None,
        })
    }

    /// Record the year as it stands after one more sale has been emulated onto `statement`.
    fn observe(
        &mut self, statement: &BrokerStatement, converter: &CurrencyConverter,
        tax_config: &TaxConfig,
    ) -> EmptyResult {
        let (year, _has_income) = spain::compute_tax_year(
            statement, self.year, converter, tax_config)?;

        // The processor emits one capital-gain entry per qualifying trade, in `stock_sells` order,
        // and `emulate_sell` appends — so this sale owns the last entry. Verify rather than trust
        // it: mispairing them would silently price the wrong trade.
        let trades: Vec<&StockSell> = statement.stock_sells.iter()
            .filter(|trade| spain::produces_capital_gain(trade, self.year))
            .collect();

        if trades.len() != year.capital_gains.len() {
            return Err!(
                "Spanish tax simulation: {} yielded {} capital gain entries for {} trades",
                self.year, year.capital_gains.len(), trades.len());
        }

        let simulated = self.standalone.len() + 1;
        let real = trades.len().checked_sub(simulated).ok_or_else(|| format!(
            "Spanish tax simulation: {} has only {} trades for {simulated} simulated sales",
            self.year, trades.len()))?;

        if !trades[real..].iter().all(|trade| trade.emulation) ||
           trades[..real].iter().any(|trade| trade.emulation) {
            return Err!(
                "Spanish tax simulation: the simulated sales aren't the trailing {} trades",
                self.year);
        }

        // What this disposal costs on its own, taxed from the first bracket up on the amount that
        // actually enters the base — so a loss the valores-homogéneos rule deferred is compared at
        // the deferred figure, not the gross one. Deriving it from the flat `localities::spain`
        // approximation instead would price it at a rate the return never uses.
        let entry = year.capital_gains.last().expect(
            "the checks above guarantee at least one capital gain entry for the simulated sale");
        self.standalone.push(year.scale.tax(entry.integrable_amount));

        self.taxes.push(year.net_tax_due);
        self.after = Some(year);

        Ok(())
    }

    fn marginal(&self, index: usize) -> Decimal {
        spain::round_eur(self.taxes[index + 1] - self.taxes[index])
    }

    fn standalone(&self, index: usize) -> Decimal {
        spain::round_eur(self.standalone[index])
    }

    /// The year as of the last observed sale, falling back to the untouched year if none was
    /// observed.
    fn after(&self) -> &SpanishTaxStatement {
        self.after.as_ref().unwrap_or(&self.before)
    }

    /// Taken end-to-end rather than by summing the rows, so it stays exact; rounded rows may differ
    /// from it by a cent.
    fn total(&self) -> Decimal {
        spain::round_eur(self.taxes[self.taxes.len() - 1] - self.taxes[0])
    }

    /// Explain the marginal figures: without the groups, the pending balances and the base a reader
    /// cannot tell a genuinely untaxed disposal from a broken one.
    fn print(&self) {
        let mut table = SpanishContextTable::new();
        let after = self.after();

        let mut row = |name: &str, before: Decimal, after: Decimal| {
            table.add_row(SpanishContextRow {
                name: name.to_owned(),
                before: spain::format_eur(before),
                after: spain::format_eur(after),
            });
        };

        row("Ganancias y pérdidas patrimoniales (neto)", self.before.gyp_net, after.gyp_net);
        row("Rendimientos del capital mobiliario (neto)", self.before.rcm_net, after.rcm_net);
        // Carried forward as positive magnitudes, exactly as the filed statement reports them. A
        // gain eats into the matching group's pending balance, so these shrink as the sale is added.
        row("Saldos negativos pendientes (ganancias)",
            self.before.gyp_ledger_next.total(), after.gyp_ledger_next.total());
        row("Base liquidable del ahorro", self.before.savings_base, after.savings_base);
        row("Cuota íntegra del ahorro", self.before.savings_quota, after.savings_quota);
        row(
            "Cuota líquida del ahorro",
            self.before.net_tax_due,
            after.net_tax_due,
        );

        let regime = match after.regime {
            crate::taxes::spain::SpanishTaxRegime::Gipuzkoa => "Gipuzkoa",
            crate::taxes::spain::SpanishTaxRegime::Comun => "Territorio Común",
            crate::taxes::spain::SpanishTaxRegime::Navarra => "Navarra",
        };
        table.print(&format!(
            "Spanish tax year {} ({regime}) — the sale is priced at what it changes here, not at a \
             flat rate", self.year));

        if self.taxes.len() > 2 {
            // The savings scale is progressive and the groups compensate, so the split across rows
            // depends on their order and no single row is that position's standalone cost. Only the
            // total is order-independent. Say so — someone reading one row in isolation would be
            // misled.
            println!(
                "Per-position tax is marginal and depends on row order: each is what that sale \n\
                 adds on top of the ones above it. Only the total is order-independent; simulate \n\
                 a position on its own to see its standalone cost.\n");
        }
    }
}

#[derive(StaticTable)]
#[table(name="SpanishContextTable")]
struct SpanishContextRow {
    #[column(name="")]
    name: String,
    #[column(name="Without the sale", align="right")]
    before: String,
    #[column(name="With the sale", align="right")]
    after: String,
}

struct TaxYearTotals {
    local_profit: Cash,
    taxable_local_profit: Cash,
    lto_calculator: Option<LtoDeductionCalculator>,
}

impl TaxYearTotals {
    fn new(country: &Country) -> TaxYearTotals {
        TaxYearTotals {
            local_profit: Cash::zero(country.currency),
            taxable_local_profit: Cash::zero(country.currency),
            lto_calculator: None,
        }
    }
}

fn print_results(
    country: &Country, portfolio: &PortfolioConfig, instrument_info: &InstrumentInfo,
    stock_sells: Vec<StockSell>, additional_commissions: MultiCurrencyCashAccount,
    converter: &CurrencyConverter, simulation: Option<&TaxSimulation>,
) -> EmptyResult {
    let mut trades_table = TradesTable::new();
    let mut fifo_table = FifoTable::new();

    let mut tax_calculator = TaxCalculator::new(country.clone());

    let mut total_purchase_cost = MultiCurrencyCashAccount::new();
    let mut total_purchase_local_cost = Cash::zero(country.currency);

    let mut total_revenue = MultiCurrencyCashAccount::new();
    let mut total_local_revenue = Cash::zero(country.currency);

    let mut total_profit = MultiCurrencyCashAccount::new();
    let mut total_commission = MultiCurrencyCashAccount::new();
    let mut tax_year_totals: BTreeMap<i32, TaxYearTotals> = BTreeMap::new();

    let mut same_currency = true;
    let mut tax_exemptions = false;

    // Summed from the rows, so the year-level deduction total is the sum of the deductions shown.
    // The year-level `tax_deductible_income` aggregation below is derived differently — it nets
    // additional commissions and LTO — so mixing the two would leave a structural gap, not a
    // rounding one.
    let mut total_row_tax_expected = Cash::zero(country.currency);

    let sell_date = stock_sells.iter()
        .map(|trade| trade.conclusion_time.date)
        .reduce(|prev, next| {
            assert_eq!(prev, next);
            prev
        })
        .unwrap();

    for (index, trade) in stock_sells.into_iter().enumerate() {
        let (sell_price, commission) = match trade.type_ {
            StockSellType::Trade {price, commission, ..} => {
                same_currency &=
                    price.currency == country.currency &&
                    commission.currency == country.currency;

                (price, commission.round())
            },
            _ => unreachable!(),
        };
        total_commission.deposit(commission);

        let (tax_year, _) = portfolio.tax_payment_day().get(trade.execution_date, true);
        let totals = tax_year_totals.entry(tax_year).or_insert_with(|| TaxYearTotals::new(country));

        let instrument = instrument_info.get_or_empty(&trade.symbol);
        let details = trade.calculate(country, &instrument, &portfolio.tax_exemptions, converter)?;

        let tax = match simulation {
            // What the disposal actually adds to the tax year, given everything that year-wide
            // mechanics do to it. `expected` is what it would have cost taxed on its own under the
            // same rules, so the deduction column shows exactly what those mechanics saved.
            Some(simulation) => {
                let expected = country.cash(simulation.standalone(index));
                let to_pay = country.cash(simulation.marginal(index));
                Tax {
                    expected,
                    withheld: Cash::zero(country.currency),
                    deduction: expected - to_pay,
                    to_pay,
                }
            },
            None => details.estimate_tax(&tax_calculator, tax_year),
        };

        let real = details.real_profit(converter, &tax)?;
        tax_exemptions |= details.tax_exemption_applied();

        total_row_tax_expected += tax.expected;

        total_purchase_cost.deposit(details.purchase_cost);
        total_purchase_local_cost += details.purchase_local_cost;

        total_revenue.deposit(details.revenue);
        total_local_revenue += details.local_revenue;

        total_profit.deposit(details.profit);
        totals.local_profit += details.local_profit;
        totals.taxable_local_profit += details.taxable_local_profit;

        let price_precision = std::cmp::max(2, util::decimal_precision(sell_price.amount));
        let mut purchase_cost = Cash::zero(sell_price.currency);

        for (index, buy_trade) in details.fifo.iter().enumerate() {
            let buy_price = buy_trade.price(sell_price.currency, converter)?;
            purchase_cost += buy_trade.cost(purchase_cost.currency, converter)?;

            if let Some(ref deductible) = buy_trade.long_term_ownership_deductible {
                let lto_calculator = totals.lto_calculator.get_or_insert_with(LtoDeductionCalculator::new);
                lto_calculator.add(deductible.profit, deductible.years, false);
            }

            fifo_table.add_row(FifoRow {
                symbol: if index == 0 {
                   Some(trade.symbol.clone())
                } else {
                   None
                },
                date: buy_trade.conclusion_time.date,
                quantity: (buy_trade.quantity * buy_trade.multiplier).normalize(),
                price: (buy_price / buy_trade.multiplier).normalize(),
                long_term_ownership: buy_trade.long_term_ownership_deductible.is_some(),
                tax_free: buy_trade.tax_exemption_applied,
            });
        }

        trades_table.add_row(TradeRow {
            symbol: trade.symbol,
            quantity: trade.quantity,
            buy_price: (purchase_cost / trade.quantity).round_to(price_precision).normalize(),
            sell_price,
            commission,

            revenue: details.revenue,
            local_revenue: details.local_revenue,

            profit: details.profit,
            local_profit: details.local_profit,
            taxable_local_profit: details.taxable_local_profit,

            tax_to_pay: tax.to_pay,
            tax_deduction: tax.deduction,

            real_profit: real.profit_ratio.map(Cell::new_ratio),
            real_tax: real.tax_ratio.map(Cell::new_ratio),
            real_local_profit: real.local_profit_ratio.map(Cell::new_ratio),
        });
    }

    for commission in additional_commissions.iter() {
        let totals = tax_year_totals.values_mut().next().unwrap();
        let local_commission = converter.convert_to_cash_rounding(sell_date, commission, country.currency)?;

        total_profit.withdraw(commission);
        total_commission.deposit(commission.round());

        totals.local_profit -= local_commission;
        totals.taxable_local_profit -= local_commission;
    }

    let mut total_local_profit = Cash::zero(country.currency);
    let mut total_taxable_local_profit = Cash::zero(country.currency);

    let mut total_tax_to_pay = Cash::zero(country.currency);
    let mut total_tax_deduction = Cash::zero(country.currency);

    let mut lto_deductions: BTreeMap<i32, LtoDeduction> = BTreeMap::new();

    for (tax_year, mut totals) in tax_year_totals {
        if let Some(lto_calculator) = totals.lto_calculator.take() {
            let lto = lto_calculator.calculate();
            totals.taxable_local_profit.amount -= lto.deduction;
            lto_deductions.insert(tax_year, lto);
        }

        let tax = tax_calculator.tax_deductible_income(
            IncomeType::Trading, tax_year, totals.local_profit, totals.taxable_local_profit);

        total_local_profit += totals.local_profit;
        total_taxable_local_profit += totals.taxable_local_profit;

        total_tax_to_pay += tax.to_pay;
        total_tax_deduction += tax.deduction;
    }

    if let Some(simulation) = simulation {
        // Taken end-to-end over the year rather than by summing the rows, so the total stays exact
        // even where rounded row taxes drift from it by a cent. The deduction is then measured
        // against the same per-row standalone figures the rows show, so the column adds up.
        total_tax_to_pay = country.cash(simulation.total());
        total_tax_deduction = total_row_tax_expected - total_tax_to_pay;
    }

    let total_real = trades::calculate_real_profit(
        converter.real_time_date(), total_purchase_cost, total_purchase_local_cost,
        total_profit.clone(), total_local_profit, total_tax_to_pay, converter)?;

    let mut totals = trades_table.add_empty_row();
    totals.set_commission(total_commission);
    totals.set_revenue(total_revenue);
    totals.set_local_revenue(total_local_revenue);
    totals.set_profit(total_profit);
    totals.set_local_profit(total_local_profit);
    totals.set_taxable_local_profit(total_taxable_local_profit);
    totals.set_tax_to_pay(total_tax_to_pay);
    totals.set_tax_deduction(total_tax_deduction);
    totals.set_real_profit(total_real.profit_ratio.map(Cell::new_ratio));
    totals.set_real_tax(total_real.tax_ratio.map(Cell::new_ratio));
    totals.set_real_local_profit(total_real.local_profit_ratio.map(Cell::new_ratio));

    if same_currency {
        trades_table.hide_local_revenue();
        trades_table.hide_local_profit();
        trades_table.hide_real_local_profit();
    }
    if same_currency && lto_deductions.is_empty() {
        trades_table.hide_real_tax();
    }
    if !tax_exemptions && lto_deductions.is_empty() {
        trades_table.hide_taxable_local_profit();

        // Where a year-level simulation runs, the deduction is what the year's own mechanics took
        // off the standalone charge — the whole point of the exercise, so keep the column whenever
        // it says anything.
        if simulation.is_none() || total_tax_deduction.is_zero() {
            trades_table.hide_tax_deduction();
        }
    }
    if !tax_exemptions {
        fifo_table.hide_tax_free();
    }
    if lto_deductions.is_empty() {
        fifo_table.hide_long_term_ownership();
    }

    trades_table.print("Sell simulation results");
    fifo_table.print("FIFO details");

    for (tax_year, lto) in &lto_deductions {
        let mut title = s!("Long term ownership deduction");
        if lto_deductions.len() > 1 {
            title = format!("{title} ({tax_year})")
        }
        lto.print(&title);
    }

    if let Some(simulation) = simulation {
        simulation.print();
    }

    Ok(())
}

#[derive(StaticTable)]
#[table(name="TradesTable")]
struct TradeRow {
    #[column(name="Symbol")]
    symbol: String,
    #[column(name="Quantity")]
    quantity: Decimal,
    #[column(name="Buy price")]
    buy_price: Cash,
    #[column(name="Sell price")]
    sell_price: Cash,
    #[column(name="Commission")]
    commission: Cash,
    #[column(name="Revenue")]
    revenue: Cash,
    #[column(name="Local revenue")]
    local_revenue: Cash,
    #[column(name="Profit")]
    profit: Cash,
    #[column(name="Local profit")]
    local_profit: Cash,
    #[column(name="Taxable profit")]
    taxable_local_profit: Cash,
    #[column(name="Tax to pay")]
    tax_to_pay: Cash,
    #[column(name="Tax deduction")]
    tax_deduction: Cash,
    #[column(name="Real profit %")]
    real_profit: Option<Cell>,
    #[column(name="Real tax %")]
    real_tax: Option<Cell>,
    #[column(name="Real local profit %")]
    real_local_profit: Option<Cell>,
}

#[derive(StaticTable)]
#[table(name="FifoTable")]
struct FifoRow {
    #[column(name="Symbol")]
    symbol: Option<String>,
    #[column(name="Date")]
    date: Date,
    #[column(name="Quantity")]
    quantity: Decimal,
    #[column(name="Price")]
    price: Cash,
    #[column(name="LTO", align="center")]
    long_term_ownership: bool,
    #[column(name="Tax free", align="center")]
    tax_free: bool,
}
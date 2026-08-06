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

    // Snapshot the German tax year *before* anything is emulated: the disposals are priced at the
    // difference they make to it, so the untouched year is the reference point.
    let german_baseline = (country.jurisdiction == Jurisdiction::Germany).then(|| {
        let year = crate::exchanges::today_trade_conclusion_time().date.year();
        germany::compute_tax_year(
            &statement, year, &converter, tax_config,
            portfolio.foreign_currency_taxation, &portfolio.opening_foreign_currency,
        ).map(|(baseline, _has_income)| (year, baseline))
    }).transpose()?;

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
    }

    statement.process_trades(None)?;
    let additional_commissions = statement.emulate_commissions(commission_calc)?;

    let stock_sells = statement.stock_sells.iter()
        .filter(|stock_sell| stock_sell.emulation)
        .cloned().collect::<Vec<_>>();
    assert_eq!(stock_sells.len(), positions.len());

    let german = german_baseline.map(|(year, baseline)| {
        GermanTaxSimulation::new(baseline, &statement, year, &converter, tax_config, portfolio)
    }).transpose()?;

    print_results(
        country, portfolio, &statement.instrument_info, stock_sells, additional_commissions,
        &converter, german.as_ref())
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
/// So the year is computed once as it stands, then once more per simulated disposal added in turn,
/// and each disposal is priced at the difference it makes. A loss-making trim then shows as
/// *negative* tax — correctly, because it shelters a gain booked elsewhere in the same year.
///
/// Each row is rounded to cents on its own while the total is taken end-to-end, so summing the
/// rows can miss the total by a cent. That is the same trade-off `format_eur` documents: one
/// rounding per reported figure, rather than a total that disagrees with its own arithmetic.
struct GermanTaxSimulation {
    year: i32,

    /// `taxes[k]` is the year's net German tax when the first `k` simulated disposals happen, so
    /// `taxes[0]` is the untouched year. Always one longer than the number of disposals.
    taxes: Vec<Decimal>,

    /// The year before and after the simulated disposals, kept so the pots and the allowance that
    /// produced these figures can be shown alongside them.
    before: GermanTaxStatement,
    after: GermanTaxStatement,
}

impl GermanTaxSimulation {
    /// `before` is the year computed prior to any emulation; `statement` is the broker statement
    /// after it, so it carries the simulated sells appended to `stock_sells`.
    fn new(
        before: GermanTaxStatement, statement: &BrokerStatement, year: i32,
        converter: &CurrencyConverter, tax_config: &TaxConfig, portfolio: &PortfolioConfig,
    ) -> GenericResult<GermanTaxSimulation> {
        let (mut after, _has_income) = germany::compute_tax_year(
            statement, year, converter, tax_config,
            portfolio.foreign_currency_taxation, &portfolio.opening_foreign_currency)?;

        // The processor emits one capital-gain entry per qualifying trade, in `stock_sells` order,
        // and `emulate_sell` appends — so the simulated disposals own the trailing entries. Verify
        // rather than trust it: mispairing them would silently price the wrong trades.
        let trades: Vec<&StockSell> = statement.stock_sells.iter()
            .filter(|trade| germany::produces_capital_gain(trade, year))
            .collect();

        if trades.len() != after.capital_gains.len() {
            return Err!(
                "German tax simulation: the {} year yielded {} capital gain entries for {} trades",
                year, after.capital_gains.len(), trades.len());
        }

        let real = trades.iter().position(|trade| trade.emulation).unwrap_or(trades.len());
        if !trades[real..].iter().all(|trade| trade.emulation) {
            return Err!("German tax simulation: emulated sales aren't the trailing {year} trades");
        }

        let simulated = trades.len() - real;
        let emulated_total = statement.stock_sells.iter().filter(|trade| trade.emulation).count();
        if simulated != emulated_total {
            return Err!(
                "German tax simulation: {simulated} of {emulated_total} simulated sales fall in {year}");
        }

        let disposals = after.capital_gains.split_off(real);

        let mut taxes = Vec::with_capacity(simulated + 1);
        taxes.push(before.net_tax_due);
        taxes.extend(after.tax_with_disposals(&disposals));

        Ok(GermanTaxSimulation {year, taxes, before, after})
    }

    /// Tax the `index`-th simulated disposal adds to the year, given the ones before it.
    fn marginal(&self, index: usize) -> Decimal {
        germany::round_eur(self.taxes[index + 1] - self.taxes[index])
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

        let mut row = |name: &str, before: Decimal, after: Decimal| {
            table.add_row(GermanContextRow {
                name: name.to_owned(),
                before: germany::format_eur(before),
                after: germany::format_eur(after),
            });
        };

        row("Taxable income (§20 EStG)",
            self.before.total_taxable_income, self.after.total_taxable_income);
        row("Sparer-Pauschbetrag used",
            self.before.sparer_pauschbetrag_used, self.after.sparer_pauschbetrag_used);
        row("Sparer-Pauschbetrag left",
            self.before.sparer_pauschbetrag - self.before.sparer_pauschbetrag_used,
            self.after.sparer_pauschbetrag - self.after.sparer_pauschbetrag_used);
        // Carried forward as positive magnitudes, exactly as the filed statement reports them.
        // A gain eats into the matching pot, so these shrink as the sale is added.
        row("Loss carryforward, shares (§20(6))",
            self.before.loss_carryforward_stock_next, self.after.loss_carryforward_stock_next);
        row("Loss carryforward, general",
            self.before.loss_carryforward_other_next, self.after.loss_carryforward_other_next);
        row("Net German tax due", self.before.net_tax_due, self.after.net_tax_due);

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
    converter: &CurrencyConverter, german: Option<&GermanTaxSimulation>,
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

        let tax = match german {
            // What the disposal actually adds to the German tax year, given the pots, the
            // carryforward and the remaining allowance. `expected` stays the flat-rate figure, so
            // the deduction column shows exactly what those mechanics saved.
            Some(german) => {
                let flat = details.estimate_tax(&tax_calculator, tax_year);
                let to_pay = country.cash(german.marginal(index));
                Tax {
                    expected: flat.expected,
                    paid: Cash::zero(country.currency),
                    deduction: flat.expected - to_pay,
                    to_pay,
                }
            },
            None => details.estimate_tax(&tax_calculator, tax_year),
        };

        let real = details.real_profit(converter, &tax)?;
        tax_exemptions |= details.tax_exemption_applied();

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

    let mut total_tax_expected = Cash::zero(country.currency);
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

        total_tax_expected += tax.expected;
        total_tax_to_pay += tax.to_pay;
        total_tax_deduction += tax.deduction;
    }

    if let Some(german) = german {
        // Taken end-to-end over the year rather than by summing the rows, so the total stays exact
        // even where rounded row taxes drift from it by a cent.
        total_tax_to_pay = country.cash(german.total());
        total_tax_deduction = total_tax_expected - total_tax_to_pay;
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

        // For Germany the deduction is what the loss pots and the allowance took off the flat
        // rate — the whole point of the exercise, so keep the column whenever it says anything.
        if german.is_none() || total_tax_deduction.is_zero() {
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

    if let Some(german) = german {
        german.print();
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
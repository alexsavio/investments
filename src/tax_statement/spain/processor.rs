//! Turns a broker statement into a Spanish tax statement.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;

use log::warn;

use crate::broker_statement::{
    BrokerStatement, FifoDetails, StockSell, StockSellType, StockSourceDetails,
};
use crate::core::GenericResult;
use crate::currency::Cash;
use crate::currency::converter::CurrencyConverter;
use crate::tax_statement::fx_fifo::compute_fx_fifo;
use crate::taxes::spain::SpanishTaxRegime;
use crate::taxes::spain::scale::SavingsScale;
use crate::taxes::{DeferredLossConfig, SpanishTaxConfig, TaxConfig};
use crate::types::{Date, Decimal};

use super::wash_sale;
use super::statement::{
    CapitalGainEntry, DividendEntry, FeeEntry, FxGainEntry, InterestEntry, SpanishLotDetail,
    SpanishTaxStatement, WashSaleReintegrationEntry, WashSaleWindowGap,
};

/// Everything about the filer's regime and tax year that the per-income processors need, resolved
/// once so no processor re-matches the regime enum and risks the two disagreeing.
struct SpanishTaxParams<'a> {
    config: &'a SpanishTaxConfig,
    regime: SpanishTaxRegime,
    year: i32,
    scale: SavingsScale,
    /// Cap the source state may levy on a dividend under the applicable double-taxation treaty.
    // TODO(verify): 15% is the dividend rate in the Spain-US and Spain-Germany treaties and in most
    // of Spain's network, but it is treaty-specific and neither NF 3/2014 art. 91 nor LIRPF art. 80
    // mentions a cap at all — the limit comes from the treaty itself.
    treaty_rate: Decimal,
    /// Whether custody and administration fees reduce the RCM result.
    custody_fees_deductible: bool,
    /// Fraction of the other group's positive balance a negative one may offset.
    cross_offset_fraction: Decimal,
}

impl<'a> SpanishTaxParams<'a> {
    fn resolve(tax_config: &'a TaxConfig, year: i32) -> GenericResult<SpanishTaxParams<'a>> {
        let config = tax_config.spanish()?;
        Ok(SpanishTaxParams {
            config,
            regime: config.regime,
            year,
            scale: config.savings_scale(year)?,
            treaty_rate: dec!(0.15),
            // LIRPF art. 26.1.a allows custody and administration fees; the Gipuzkoa equivalent
            // does not exist — NF 3/2014 art. 39 is a closed list that never reaches securities
            // income, so nothing is deductible there.
            custody_fees_deductible: match config.regime {
                SpanishTaxRegime::Gipuzkoa => false,
                SpanishTaxRegime::Comun => true,
            },
            // Gipuzkoa integrates the two groups "exclusivamente entre sí"; Territorio Común lets
            // a negative balance in one reach 25% of the other's positive (LIRPF art. 49.1).
            cross_offset_fraction: match config.regime {
                SpanishTaxRegime::Gipuzkoa => Decimal::ZERO,
                SpanishTaxRegime::Comun => dec!(0.25),
            },
        })
    }
}

/// Whether a sale produces a ganancia/pérdida patrimonial in `year`.
///
/// The tax year is keyed off the conclusion date, not settlement.
// TODO(verify): whether the "fecha de transmisión" for listed securities is the trade date or the
// settlement date was not resolvable from the foral or state texts consulted. The conclusion date
// is used, matching the German treatment and the economic reality of the transfer.
pub fn produces_capital_gain(trade: &StockSell, year: i32) -> bool {
    trade.conclusion_time.date.year() == year && matches!(trade.type_, StockSellType::Trade { .. })
}

/// Compute a Spanish tax year from a broker statement.
///
/// Returns the statement and whether any income was found, mirroring the German entry point so the
/// filing path and the sell simulation price the same year the same way.
pub fn compute_tax_year(
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
    tax_config: &TaxConfig,
) -> GenericResult<(SpanishTaxStatement, bool)> {
    let params = SpanishTaxParams::resolve(tax_config, year)?;
    let (rcm_ledger, gyp_ledger) = params.config.loss_ledgers(year)?;

    let mut statement = SpanishTaxStatement::new(
        year,
        params.regime,
        params.scale.clone(),
        rcm_ledger,
        gyp_ledger,
        params.cross_offset_fraction,
        params.treaty_rate,
    );

    let has_income = process_broker_statement(&mut statement, broker_statement, &params, converter)?;
    statement.calculate_totals();

    Ok((statement, has_income))
}

fn process_broker_statement(
    statement: &mut SpanishTaxStatement,
    broker_statement: &BrokerStatement,
    params: &SpanishTaxParams,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let has_trades = process_trades(statement, broker_statement, params, converter)?;
    let has_dividends = process_dividends(statement, broker_statement, params, converter)?;
    let has_interest = process_interest(statement, broker_statement, params, converter)?;
    let has_fx = process_fx_gains(statement, broker_statement, params, converter)?;
    process_fees(statement, broker_statement, params, converter)?;

    // Short positions get no automatic treatment; surface them for manual review.
    statement.short_positions = broker_statement
        .short_positions
        .iter()
        .map(|(symbol, &quantity)| (symbol.clone(), quantity))
        .collect();
    statement.short_positions.sort_by(|a, b| a.0.cmp(&b.0));

    if !statement.short_positions.is_empty() {
        let listed = statement
            .short_positions
            .iter()
            .map(|(symbol, quantity)| format!("{symbol}: {quantity}"))
            .collect::<Vec<_>>()
            .join(", ");
        warn!(
            "Short positions held at the end of the statement are not tax-computed and need manual \
             review: {listed}."
        );
    }

    Ok(has_trades || has_dividends || has_interest || has_fx)
}

/// One disposal priced against its own disposal year, ready for the valores-homogéneos replay.
struct PricedSale {
    key: String,
    symbol: String,
    isin: String,
    description: String,
    sale_date: Date,
    settle_date: Date,
    quantity: Decimal,
    proceeds_eur: Decimal,
    cost_eur: Decimal,
    actualized_cost_eur: Decimal,
    /// `None` when no actualization table is shipped for this sale's disposal year.
    fiscal_gain_loss: Option<Decimal>,
    lots: Vec<SpanishLotDetail>,
    consumed: Vec<(Date, Decimal)>,
    deferred_loss: Decimal,
}

/// Turn each qualifying sale into a capital-gain entry with per-lot actualization and the
/// valores-homogéneos deferral applied.
///
/// Every disposal in the statement is priced, not just the filing year's: a deferral created by one
/// year's sale is released by another year's, and a repurchase two months after a December sale
/// lands in the following year. The replay therefore walks the whole history in date order, and only
/// the filing year's entries are emitted.
fn process_trades(
    statement: &mut SpanishTaxStatement,
    broker_statement: &BrokerStatement,
    params: &SpanishTaxParams,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    // Spain files in EUR; build the jurisdiction once so the shared trade engine converts through
    // the ECB rates supplied by the caller.
    let country = crate::localities::spain(&TaxConfig {
        spain: Some(params.config.clone()),
        ..Default::default()
    });

    let mut unpriced_years = BTreeSet::new();
    let mut sales = Vec::new();

    for trade in &broker_statement.stock_sells {
        if !matches!(trade.type_, StockSellType::Trade { .. }) || !trade.is_processed() {
            continue;
        }
        sales.push(price_sale(
            trade, broker_statement, &country, params, converter, &mut unpriced_years)?);
    }

    let replay = apply_wash_sale_rule(&mut sales, broker_statement, params)?;
    statement.wash_sale_unpriced_years = unpriced_years.into_iter().collect();
    statement.deferred_losses_next = replay.carry_out;

    // Only the filing year's releases are this year's income; the rest belong to the returns their
    // disposals fall in.
    statement.wash_sale_reintegrations = replay
        .reintegrations
        .into_iter()
        .filter(|entry| entry.date.year() == params.year)
        .collect();

    let mut has_income = false;

    for sale in sales {
        if sale.sale_date.year() != params.year {
            continue;
        }
        has_income = true;

        // A filing-year sale is always priced: `SpanishTaxParams::resolve` already errored if the
        // filing year itself had no table.
        let fiscal_gain_loss = sale.fiscal_gain_loss.expect("the filing year is always priced");

        statement.capital_gains.push(CapitalGainEntry {
            symbol: sale.symbol,
            isin: sale.isin,
            description: sale.description,
            sale_date: sale.sale_date,
            settle_date: sale.settle_date,
            quantity: sale.quantity,
            proceeds_eur: sale.proceeds_eur,
            cost_eur: sale.cost_eur,
            actualized_cost_eur: sale.actualized_cost_eur,
            fiscal_gain_loss,
            deferred_loss: sale.deferred_loss,
            integrable_amount: fiscal_gain_loss + sale.deferred_loss,
            lots: sale.lots,
            notes: (sale.deferred_loss > Decimal::ZERO).then(|| format!(
                "€{} of this loss is deferred: homogeneous securities were acquired within two \
                 months of the sale (NF 3/2014 art. 43.g / LIRPF art. 33.5.f)",
                super::format_eur(sale.deferred_loss))),
        });
    }

    flag_open_wash_sale_windows(statement, broker_statement);

    Ok(has_income)
}

/// Name the losses whose repurchase window is still open when the statement ends.
///
/// A repurchase up to two months after the sale defers the loss, so a statement that stops before
/// the window closes cannot settle it. The figure stands as computed — the deduction is taken in
/// full — which is the direction that **overstates** it, so say so. Re-run once the statement
/// extends past the window, or the return will claim a loss the rule may have deferred.
fn flag_open_wash_sale_windows(
    statement: &mut SpanishTaxStatement,
    broker_statement: &BrokerStatement,
) {
    let last_date = broker_statement.period.last_date();

    statement.wash_sale_window_gaps = statement
        .capital_gains
        .iter()
        .filter(|entry| entry.integrable_amount < Decimal::ZERO)
        .filter_map(|entry| {
            let window_end = wash_sale::window(entry.sale_date).1;
            (window_end > last_date).then(|| WashSaleWindowGap {
                symbol: entry.symbol.clone(),
                sale_date: entry.sale_date,
                window_end,
                loss_eur: -entry.integrable_amount,
            })
        })
        .collect();

    for gap in &statement.wash_sale_window_gaps {
        warn!(
            "The valores-homogéneos window for the {} loss of {} stays open until {}, past the \
             statement's last date ({last_date}). A repurchase in that period would defer €{} of \
             the loss, which is deducted in full below. Re-run once the statement covers the window.",
            gap.symbol, gap.sale_date, gap.window_end, gap.loss_eur
        );
    }
}

/// Price one disposal against its own disposal year's actualization table.
#[allow(clippy::too_many_arguments)]
fn price_sale(
    trade: &StockSell,
    broker_statement: &BrokerStatement,
    country: &crate::localities::Country,
    params: &SpanishTaxParams,
    converter: &CurrencyConverter,
    unpriced_years: &mut BTreeSet<i32>,
) -> GenericResult<PricedSale> {
    let sale_year = trade.conclusion_time.date.year();

    // Per-lot FIFO cost basis from the shared broker-statement engine: it consumes lots in
    // acquisition order (art. 47.2 / art. 37.2, "los adquiridos en primer lugar") and folds in
    // buy-side commissions. FX follows SellDetails conventions — revenue at settlement, each
    // lot's purchase cost at its own settlement — a documented simplification.
    let instrument = broker_statement.instrument_info.get_or_empty(&trade.symbol);
    let details = trade.calculate(country, &instrument, &[], converter)?;

    let net_proceeds_eur = details.local_revenue.amount - details.local_commission.amount;
    let total_quantity: Decimal = details
        .fifo
        .iter()
        .map(|lot| lot.quantity * lot.multiplier)
        .sum();

    let mut lots = Vec::with_capacity(details.fifo.len());
    let mut consumed = Vec::with_capacity(details.fifo.len());
    let mut cost_eur = Decimal::ZERO;
    let mut actualized_cost_eur = Decimal::ZERO;
    let mut priced = true;

    for lot in &details.fifo {
        let lot_cost_eur = lot_cost_basis_eur(broker_statement, lot, converter)?;

        // The coefficient is a per-lot figure keyed to that lot's acquisition year, and it is the
        // *sale's* year that selects the table — a 2025 disposal actualizes by DF 61/2024 even when
        // the return being filed is 2026.
        let coefficient = match params
            .config
            .actualization_coefficient(sale_year, lot.conclusion_time.date)
        {
            Ok(coefficient) => coefficient,
            // A disposal in a year the tool ships no table for cannot be priced. It still has to be
            // replayed, because it consumes lots and may release earlier deferrals, but it can
            // never create one.
            Err(_) => {
                priced = false;
                unpriced_years.insert(sale_year);
                Decimal::ONE
            }
        };
        let lot_actualized_cost = lot_cost_eur * coefficient;

        let lot_quantity = lot.quantity * lot.multiplier;
        let lot_proceeds = if total_quantity.is_zero() {
            Decimal::ZERO
        } else {
            net_proceeds_eur * lot_quantity / total_quantity
        };

        cost_eur += lot_cost_eur;
        actualized_cost_eur += lot_actualized_cost;
        consumed.push((lot.conclusion_time.date, lot_quantity));

        lots.push(SpanishLotDetail {
            acquisition_date: lot.conclusion_time.date,
            quantity: lot_quantity,
            cost_eur: lot_cost_eur,
            coefficient,
            actualized_cost_eur: lot_actualized_cost,
            proceeds_eur: lot_proceeds,
            gain_eur: lot_proceeds - lot_actualized_cost,
        });
    }

    // Summed from the lots rather than recomputed, so the entry total and the audit lines that
    // justify it can never disagree.
    let fiscal_gain_loss: Decimal = lots.iter().map(|lot| lot.gain_eur).sum();

    let isin = broker_statement
        .instrument_info
        .get(&trade.symbol)
        .and_then(|info| info.isin.iter().next())
        .map(|isin| isin.to_string())
        .unwrap_or_default();

    Ok(PricedSale {
        key: wash_sale::instrument_key(&broker_statement.instrument_info, &trade.symbol),
        symbol: trade.symbol.clone(),
        isin,
        description: broker_statement.instrument_info.get_name(&trade.symbol).to_string(),
        sale_date: trade.conclusion_time.date,
        settle_date: trade.execution_date,
        quantity: total_quantity,
        proceeds_eur: net_proceeds_eur,
        cost_eur,
        actualized_cost_eur,
        fiscal_gain_loss: priced.then_some(fiscal_gain_loss),
        lots,
        consumed,
        deferred_loss: Decimal::ZERO,
    })
}

/// What the valores-homogéneos replay produced, beyond the per-sale deferrals it wrote back.
#[derive(Default)]
struct ReplayResult {
    /// Deferrals released by disposals of the shares blocking them, over the whole statement.
    reintegrations: Vec<WashSaleReintegrationEntry>,
    /// Blocked lots still standing when the statement ends, in next year's config shape.
    carry_out: Vec<DeferredLossConfig>,
}

/// Replay the whole statement through the valores-homogéneos engine and write each sale's deferral
/// back onto it.
fn apply_wash_sale_rule(
    sales: &mut [PricedSale],
    broker_statement: &BrokerStatement,
    params: &SpanishTaxParams,
) -> GenericResult<ReplayResult> {
    let instruments = &broker_statement.instrument_info;

    // Symbol and ISIN per instrument key, so a surviving blocked lot can be printed back as config
    // the filer recognises rather than as a bare ISIN.
    let mut identities: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();

    fn identify(
        identities: &mut BTreeMap<String, (String, Option<String>)>,
        instruments: &crate::instruments::InstrumentInfo,
        symbol: &str,
    ) -> String {
        let key = wash_sale::instrument_key(instruments, symbol);
        let isin = instruments
            .get(symbol)
            .and_then(|info| info.isin.iter().next())
            .map(|isin| isin.to_string());
        identities
            .entry(key.clone())
            .or_insert_with(|| (symbol.to_owned(), isin));
        key
    }

    let acquisitions: Vec<wash_sale::Acquisition> = broker_statement
        .stock_buys
        .iter()
        .filter(|buy| wash_sale::is_acquisition(buy))
        .map(|buy| wash_sale::Acquisition {
            key: identify(&mut identities, instruments, &buy.symbol),
            date: buy.conclusion_time.date,
            quantity: buy.quantity,
        })
        .collect();

    let mut opening = Vec::new();
    for deferred in &params.config.deferred_losses {
        if deferred.loss < Decimal::ZERO || deferred.blocked_quantity <= Decimal::ZERO {
            return Err!(
                "taxes.spain.deferred_losses entry for {} is invalid: record the loss as a positive \
                 magnitude and the blocked quantity as a positive number of shares",
                deferred.symbol);
        }

        // The configured ISIN wins over anything the statement knows: a deferral carried in from an
        // earlier return may name an instrument this statement never traded.
        let key = deferred
            .isin
            .clone()
            .unwrap_or_else(|| wash_sale::instrument_key(instruments, &deferred.symbol));
        identities
            .entry(key.clone())
            .or_insert_with(|| (deferred.symbol.clone(), deferred.isin.clone()));

        opening.push((key, wash_sale::BlockedLot {
            buy_date: deferred.acquisition_date,
            blocked_quantity: deferred.blocked_quantity,
            deferred_loss: deferred.loss,
            origin_sale_date: deferred.sale_date,
        }));
    }

    for sale in sales.iter_mut() {
        sale.key = identify(&mut identities, instruments, &sale.symbol);
    }

    let mut engine = wash_sale::WashSaleEngine::new(acquisitions, opening);

    // Date order, not statement order: the rule is about what happened when, and a deferral created
    // by one sale is released by a later one.
    let mut order: Vec<usize> = (0..sales.len()).collect();
    order.sort_by_key(|&position| sales[position].sale_date);

    let mut result = ReplayResult::default();

    for position in order {
        let sale = &sales[position];
        let outcome = engine.process(&wash_sale::Disposal {
            key: sale.key.clone(),
            date: sale.sale_date,
            quantity: sale.quantity,
            fiscal_result: sale.fiscal_gain_loss,
            consumed: sale.consumed.clone(),
        });

        for reintegration in outcome.reintegrations {
            result.reintegrations.push(WashSaleReintegrationEntry {
                symbol: sale.symbol.clone(),
                isin: sale.isin.clone(),
                date: sale.sale_date,
                acquisition_date: reintegration.buy_date,
                origin_sale_date: reintegration.origin_sale_date,
                released_eur: reintegration.amount,
            });
        }

        sales[position].deferred_loss = outcome.deferred_loss;
    }

    for (key, lot) in engine.blocked_lots() {
        if lot.deferred_loss <= Decimal::ZERO {
            continue;
        }
        let (symbol, isin) = identities
            .get(key)
            .cloned()
            .unwrap_or_else(|| (key.to_owned(), None));

        result.carry_out.push(DeferredLossConfig {
            symbol,
            isin,
            loss: lot.deferred_loss,
            blocked_quantity: lot.blocked_quantity,
            acquisition_date: lot.buy_date,
            sale_date: lot.origin_sale_date,
        });
    }

    Ok(result)
}

/// Acquisition cost of one FIFO lot in EUR.
fn lot_cost_basis_eur(
    broker_statement: &BrokerStatement,
    lot: &FifoDetails,
    converter: &CurrencyConverter,
) -> GenericResult<Decimal> {
    match lot.source {
        // Vested shares carry no trade cost in `SellDetails`. Their acquisition value is the
        // vest-date FMV, which was already taxed as employment income in the general base.
        StockSourceDetails::Grant => grant_lot_cost_basis_eur(broker_statement, lot, converter),
        _ => Ok(lot.total_cost("EUR", converter)?.amount),
    }
}

fn grant_lot_cost_basis_eur(
    broker_statement: &BrokerStatement,
    lot: &FifoDetails,
    converter: &CurrencyConverter,
) -> GenericResult<Decimal> {
    let vest_date = lot.conclusion_time.date;

    let Some(grant) = broker_statement
        .stock_grants
        .iter()
        .find(|grant| grant.symbol == lot.original_symbol && grant.date == vest_date)
    else {
        warn!(
            "Stock grant lot for {} vested {} has no matching grant record; using €0 acquisition \
             value.",
            lot.original_symbol, vest_date
        );
        return Ok(Decimal::ZERO);
    };

    let Some(fmv) = grant.fmv_per_share else {
        warn!(
            "Stock grant {} vested {}: vest-date FMV unavailable; using €0 acquisition value \
             (overstates the gain).",
            lot.original_symbol, vest_date
        );
        return Ok(Decimal::ZERO);
    };

    // fmv is per original (un-split) share; lot.quantity is likewise the pre-multiplier count.
    let cost = Cash::new(fmv.currency, fmv.amount * lot.quantity);
    Ok(converter
        .convert_to_cash_rounding(vest_date, cost, "EUR")
        .map_err(|e| {
            format!("Converting vest-date FMV for stock grant {} on {vest_date}: {e}", lot.original_symbol)
        })?
        .amount)
}

/// Convert a foreign-currency amount to EUR, naming what failed if the rate is missing.
fn convert_to_eur(
    converter: &CurrencyConverter,
    date: Date,
    cash: Cash,
    context: &str,
) -> GenericResult<Decimal> {
    converter
        .convert_to_cash_rounding(date, cash, "EUR")
        .map(|cash| cash.amount)
        .map_err(|e| {
            format!(
                "{context}: failed to convert {cash} to EUR on {date}. This usually means the ECB \
                 reference rate for that date is missing: {e}"
            )
            .into()
        })
}

/// Dividends, taxed as rendimientos del capital mobiliario.
fn process_dividends(
    statement: &mut SpanishTaxStatement,
    broker_statement: &BrokerStatement,
    params: &SpanishTaxParams,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_income = false;

    for dividend in &broker_statement.dividends {
        if dividend.date.year() != params.year {
            continue;
        }

        has_income = true;

        let context = format!(
            "Processing dividend from {} on {}",
            dividend.issuer, dividend.date
        );
        let gross_eur = convert_to_eur(converter, dividend.date, dividend.amount, &context)?;
        let withheld_eur = convert_to_eur(converter, dividend.date, dividend.paid_tax, &context)?;

        let instrument_info = broker_statement.instrument_info.get(&dividend.issuer);
        let isin = instrument_info
            .and_then(|info| info.isin.iter().next())
            .map(|isin| isin.to_string())
            .unwrap_or_default();
        let description = broker_statement
            .instrument_info
            .get_name(&dividend.issuer)
            .to_string();

        // First limb of the double-taxation credit: a treaty caps what the source state may levy,
        // so anything withheld above it is not creditable here and must be reclaimed from that
        // state instead. The second limb (the average savings rate) is a year-level figure, applied
        // in `calculate_totals`.
        let treaty_capped_credit = std::cmp::min(withheld_eur, gross_eur * params.treaty_rate);

        statement.dividends.push(DividendEntry {
            symbol: dividend.issuer.clone(),
            isin,
            description,
            date: dividend.date,
            gross_eur,
            withheld_eur,
            treaty_capped_credit: std::cmp::max(Decimal::ZERO, treaty_capped_credit),
        });
    }

    Ok(has_income)
}

/// Broker interest, taxed as rendimientos del capital mobiliario.
///
/// Interest **received** is income. Interest **paid** — IB's "Broker Interest Paid", which arrives
/// as a negative accrual in the same ledger — is not deductible under either regime: LIRPF art.
/// 26.1.a is a closed list reaching only administration and custody of negotiable securities, and
/// NF 3/2014 art. 39 is narrower still. Netting the two would silently deduct a financing cost the
/// return does not allow, so paid interest is reported and left out of the result.
fn process_interest(
    statement: &mut SpanishTaxStatement,
    broker_statement: &BrokerStatement,
    params: &SpanishTaxParams,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_income = false;
    let mut paid_total = Decimal::ZERO;

    for interest in &broker_statement.idle_cash_interest {
        if interest.date.year() != params.year {
            continue;
        }

        has_income = true;

        let context = format!("Processing interest payment on {}", interest.date);
        let gross_eur = convert_to_eur(converter, interest.date, interest.amount, &context)?;

        let taxable = gross_eur >= Decimal::ZERO;
        if !taxable {
            paid_total -= gross_eur;
        }

        statement.interest.push(InterestEntry {
            date: interest.date,
            description: if taxable {
                "Broker interest received".to_string()
            } else {
                "Broker interest paid (borrowed balance)".to_string()
            },
            gross_eur,
            taxable,
            notes: (!taxable).then(|| {
                "Informational: interest paid on a borrowed balance is not deductible from the \
                 savings base — LIRPF art. 26.1.a allows only administration and custody of \
                 negotiable securities, and NF 3/2014 art. 39 allows nothing"
                    .to_string()
            }),
        });
    }

    if paid_total > Decimal::ZERO {
        warn!(
            "€{} of broker interest paid on a borrowed (margin) balance is reported but NOT \
             deducted from the savings base: neither LIRPF art. 26.1.a nor NF 3/2014 art. 39 \
             allows a financing cost against rendimientos del capital mobiliario.",
            super::format_eur(paid_total)
        );
    }

    Ok(has_income)
}

/// Keywords that mark a fee as a custody or administration charge.
///
/// LIRPF art. 26.1.a allows only "gastos de administración y depósito de valores negociables", and
/// explicitly excludes the fee for discretionary portfolio management. A broker statement carries
/// nothing but a free-text description, so the match is on that.
// TODO(verify): the keyword list is a best-effort reading of IB's fee descriptions against art.
// 26.1.a; the article names the service, not the wording a broker happens to use. The failure
// direction is deliberately conservative — an unrecognised fee is reported but not deducted, which
// overstates tax rather than understating it.
const CUSTODY_FEE_KEYWORDS: &[&str] = &[
    "custody",
    "safekeeping",
    "administration",
    "custodia",
    "administración",
    "administracion",
];

fn is_custody_fee(description: &str) -> bool {
    let description = description.to_lowercase();
    CUSTODY_FEE_KEYWORDS
        .iter()
        .any(|keyword| description.contains(keyword))
}

/// Broker fees. Deductibility from RCM is regime-dependent.
fn process_fees(
    statement: &mut SpanishTaxStatement,
    broker_statement: &BrokerStatement,
    params: &SpanishTaxParams,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_fees = false;

    for fee in &broker_statement.fees {
        if fee.date.year() != params.year {
            continue;
        }

        has_fees = true;

        let context = format!("Processing broker fee on {}", fee.date);
        let amount_eur = convert_to_eur(converter, fee.date, fee.amount.withholding(), &context)?;

        let description = fee
            .description
            .clone()
            .unwrap_or_else(|| "Broker fee".to_string());

        let custody = is_custody_fee(&description);
        let deductible = params.custody_fees_deductible && custody;

        let notes = if !params.custody_fees_deductible {
            Some(
                "Informational: Gipuzkoa has no equivalent of LIRPF art. 26.1.a — NF 3/2014 art. \
                 39 does not allow expenses against securities income"
                    .to_string(),
            )
        } else if custody {
            None
        } else {
            Some(
                "Informational: not recognised as a custody or administration fee (LIRPF art. \
                 26.1.a); check whether it qualifies"
                    .to_string(),
            )
        };

        statement.fees.push(FeeEntry {
            date: fee.date,
            description,
            amount_eur,
            deductible,
            notes,
        });
    }

    Ok(has_fees)
}


/// Foreign-currency conversion results.
///
/// The shared signed-inventory FIFO replays the cash ledger and splits realizations by whether the
/// balance was held or borrowed. Held-balance results are transfers of a patrimonial element and
/// join the ganancias group; borrowed-balance results are referred for manual review.
fn process_fx_gains(
    statement: &mut SpanishTaxStatement,
    broker_statement: &BrokerStatement,
    params: &SpanishTaxParams,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let results = compute_fx_fifo(
        &broker_statement.foreign_cash_flows,
        &[],
        |date, currency| {
            converter.currency_rate(date, currency, "EUR").map_err(|e| {
                format!(
                    "Failed to convert {currency} to EUR on {date}. This usually means the ECB \
                     reference rate for that date is missing: {e}"
                )
                .into()
            })
        },
    )?;

    let mut has_income = false;

    for result in &results {
        for realization in &result.taxable {
            if realization.date.year() != params.year {
                continue;
            }
            has_income = true;
            statement.fx_gains.push(FxGainEntry {
                date: realization.date,
                currency: result.currency.clone(),
                acquisition_date: realization.acquisition_date,
                amount_eur: realization.amount,
                activity_code: realization.activity_code.clone(),
            });
        }

        for realization in &result.non_taxable {
            if realization.date.year() != params.year {
                continue;
            }
            statement.fx_borrowed_review.push(FxGainEntry {
                date: realization.date,
                currency: result.currency.clone(),
                acquisition_date: realization.acquisition_date,
                amount_eur: realization.amount,
                activity_code: realization.activity_code.clone(),
            });
        }
    }

    if !statement.fx_borrowed_review.is_empty() {
        let total: Decimal = statement
            .fx_borrowed_review
            .iter()
            .map(|entry| entry.amount_eur)
            .sum();
        warn!(
            "€{total} of foreign-currency results were realized on a borrowed (margin) balance and \
             are NOT included in the savings base. Repaying a currency loan is not clearly a \
             transfer of a patrimonial element and neither NF 3/2014 nor the LIRPF settles it — \
             review these manually."
        );
    }

    Ok(has_income)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Under Territorio Común only custody and administration fees qualify (LIRPF art. 26.1.a).
    /// Anything else is reported but not deducted — the conservative direction, since a wrong
    /// deduction understates tax.
    #[test]
    fn custody_fees_are_recognised_by_description() {
        assert!(is_custody_fee("CUSTODY FEE"));
        assert!(is_custody_fee("Monthly safekeeping charge"));
        assert!(is_custody_fee("Comisión de administración"));
        assert!(is_custody_fee("SECURITIES ADMINISTRATION"));

        assert!(!is_custody_fee("ADR FEE"));
        assert!(!is_custody_fee("Monthly Minimum Activity Fee"));
        assert!(!is_custody_fee("Commission Adjustments"));
        // Discretionary portfolio management is excluded by art. 26.1.a by name.
        assert!(!is_custody_fee("Discretionary portfolio management fee"));
    }
}

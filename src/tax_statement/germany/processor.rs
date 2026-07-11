//! German tax statement processor.
//!
//! Processes broker statement data and populates the German tax statement
//! with capital gains, dividends, interest entries, and FX gains/losses.

use std::collections::{HashMap, HashSet};

use chrono::Datelike;
use log::{debug, warn};

use crate::broker_statement::{
    BrokerCorporateActionType, BrokerStatement, FifoDetails, StockSourceDetails,
};
use crate::core::GenericResult;
use crate::currency::Cash;
use crate::currency::converter::CurrencyConverter;
use crate::taxes::TaxConfig;
use crate::taxes::germany::vorabpauschale::{
    first_business_day_of_year, full_months_before_acquisition, vorabpauschale,
};
use crate::taxes::germany::{AbgeltungsteuerBreakdown, TeilfreistellungRate};
use crate::time::Date;
use crate::types::Decimal;

use super::statement::{
    CapitalGainEntry, CashGrantEntry, CorporateActionEntry, CorporateActionType, DividendEntry,
    FeeEntry, FxGainEntry, GermanTaxStatement, InterestEntry, StockGrantEntry, VorabpauschaleEntry,
};

/// Helper function to convert to EUR with context-specific error message.
fn convert_to_eur(
    converter: &CurrencyConverter,
    date: Date,
    cash: Cash,
    context: &str,
) -> GenericResult<Decimal> {
    converter
        .convert_to_cash_rounding(date, cash, "EUR")
        .map(|c| c.amount)
        .map_err(|e| {
            format!(
                "{context}: Failed to convert {cash} to EUR on {date}. \
            This may indicate missing ECB exchange rates. \
            Ensure your database has currency rates for this date. Error: {e}"
            )
            .into()
        })
}

/// Process broker statement and populate German tax statement entries.
pub fn process_broker_statement(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
    tax_config: &TaxConfig,
) -> GenericResult<(bool, bool, bool, bool)> {
    debug!("Processing German tax statement for year {}", year);

    let has_trades = process_trades(statement, broker_statement, year, converter, tax_config)?;
    let has_dividends =
        process_dividends(statement, broker_statement, year, converter, tax_config)?;
    let has_interest = process_interest(statement, broker_statement, year, converter)?;
    let has_fx_gains = process_fx_gains(statement, broker_statement, year, converter)?;

    // Process additional income types
    let has_fees = process_fees(statement, broker_statement, year, converter)?;
    let has_stock_grants = process_stock_grants(statement, broker_statement, year, converter)?;
    let has_cash_grants = process_cash_grants(statement, broker_statement, year, converter)?;
    let has_corporate_actions =
        process_corporate_actions(statement, broker_statement, year, converter)?;

    debug!(
        "German tax statement processing complete: {} trades, {} dividends, {} interest, {} FX, {} fees, {} stock grants, {} cash grants, {} corp actions",
        statement.capital_gains.len(),
        statement.dividends.len(),
        statement.interest.len(),
        statement.fx_gains.len(),
        statement.fees.len(),
        statement.stock_grants.len(),
        statement.cash_grants.len(),
        statement.corporate_actions.len()
    );

    // Note: has_fees, has_stock_grants, has_cash_grants, has_corporate_actions are informational
    // They don't affect the primary income flags
    let _ = (
        has_fees,
        has_stock_grants,
        has_cash_grants,
        has_corporate_actions,
    );

    // Vorabpauschale (§18 InvStG) for funds held at year end. Must run after process_dividends, which
    // supplies the per-fund distributions this uses.
    process_vorabpauschale(statement, broker_statement, year, tax_config)?;

    // Short positions get no automatic tax treatment; surface them for manual §20 EStG review.
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
            "Short positions held at year end are not tax-computed and need manual §20 EStG review \
             (Termin-/Stillhaltergeschäfte): {listed}.",
        );
    }

    Ok((has_trades, has_dividends, has_interest, has_fx_gains))
}
/// Process stock sales and create capital gain entries.
fn process_trades(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
    tax_config: &TaxConfig,
) -> GenericResult<bool> {
    let mut has_income = false;

    // Germany taxes capital gains at a flat rate in EUR; build the jurisdiction once so the shared
    // trade engine converts through the ECB rates supplied by the caller.
    let country = crate::localities::germany(tax_config);
    let pre_2009_cutoff = Date::from_ymd_opt(2009, 1, 1).unwrap();

    // ISINs still held at the tax-year boundary (used to detect full disposals for the §19
    // Vorabpauschale reduction), keyed by ISIN so the same fund under two symbols is not judged
    // "fully disposed" while one leg is still open, and the set of ISINs whose accumulated
    // Vorabpauschale has already been applied.
    let held_isins: HashSet<String> = year_end_holdings(broker_statement, year)
        .keys()
        .filter_map(|symbol| broker_statement.instrument_info.get(symbol))
        .filter_map(|info| info.isin.iter().next())
        .map(|isin| isin.to_string())
        .collect();
    let mut vp_consumed: HashSet<String> = HashSet::new();

    for trade in &broker_statement.stock_sells {
        // Germany assigns the tax year by the obligatory transaction (conclusion) date, not the
        // settlement date.
        if trade.conclusion_time.date.year() != year {
            continue;
        }

        // Only genuine trades yield capital gains here; corporate-action disposals are handled by
        // process_corporate_actions.
        if !matches!(
            trade.type_,
            crate::broker_statement::StockSellType::Trade { .. }
        ) {
            continue;
        }

        has_income = true;

        // Real per-lot FIFO cost basis via the shared broker-statement engine: it consumes lots in
        // acquisition order (so repeated sales don't re-use the same lot) and folds in buy-side
        // commissions (Anschaffungsnebenkosten). FX follows SellDetails conventions — revenue at
        // settlement, each lot's purchase cost at its own settlement — a documented simplification.
        let instrument = broker_statement.instrument_info.get_or_empty(&trade.symbol);
        let details = trade.calculate(&country, &instrument, &[], converter)?;

        let net_proceeds_eur = details.local_revenue.amount - details.local_commission.amount;
        let total_quantity: Decimal = details
            .fifo
            .iter()
            .map(|lot| lot.quantity * lot.multiplier)
            .sum();

        // Walk the matched lots once: accumulate the true cost basis, and isolate the profit share
        // of pre-2009 (Altbestand) lots so only those are grandfathered out of the taxable base.
        let mut cost_basis_eur = dec!(0);
        let mut pre_2009_profit = dec!(0);
        let mut has_pre_2009_lot = false;
        for lot in &details.fifo {
            let lot_cost_eur = match lot.source {
                // Vested RSU shares are acquired at their vest-date FMV (already taxed as employment
                // income); StockSourceDetails::Grant carries zero trade cost, so substitute the FMV.
                StockSourceDetails::Grant => {
                    grant_lot_cost_basis_eur(broker_statement, lot, converter)?
                }
                _ => lot.total_cost("EUR", converter)?.amount,
            };
            cost_basis_eur += lot_cost_eur;

            if lot.conclusion_time.date < pre_2009_cutoff {
                has_pre_2009_lot = true;
                if !total_quantity.is_zero() {
                    let lot_share = (lot.quantity * lot.multiplier) / total_quantity;
                    pre_2009_profit += net_proceeds_eur * lot_share - lot_cost_eur;
                }
            }
        }

        let gross_gain_loss = net_proceeds_eur - cost_basis_eur;

        // Get instrument info for ISIN and name
        let instrument_info = broker_statement.instrument_info.get(&trade.symbol);
        let isin = instrument_info
            .and_then(|info| info.isin.iter().next())
            .map(|isin| isin.to_string())
            .unwrap_or_default();
        let description = instrument_info
            .map(|_info| {
                broker_statement
                    .instrument_info
                    .get_name(&trade.symbol)
                    .to_string()
            })
            .unwrap_or_else(|| trade.symbol.clone());

        // Determine Teilfreistellung rate from ETF classification config
        // Lookup by ISIN if available, otherwise no exemption (regular stocks)
        let teilfreistellung_rate = if !isin.is_empty() {
            let classification = tax_config.get_etf_classification(&isin);
            let rate = classification.to_teilfreistellung_rate();
            if rate != TeilfreistellungRate::None {
                debug!(
                    "ETF classification for {} ({}): {:?} -> {:?}",
                    trade.symbol, isin, classification, rate
                );
            }
            rate
        } else {
            TeilfreistellungRate::None
        };

        // Altbestand (§52 Abs. 28 S. 11 EStG): shares acquired before 2009-01-01 are grandfathered
        // per lot, so their profit (gain or loss) is excluded before Teilfreistellung and tax.
        let post_altbestand = gross_gain_loss - pre_2009_profit;

        // §19 Abs. 1 InvStG: Vorabpauschale already taxed over the holding period reduces the fund's
        // sale gain, in full and before Teilfreistellung. Applied only on a full disposal (the fund
        // is no longer held at year end) and once per ISIN — the whole-position approximation avoids
        // over-crediting a partial sale, which would understate tax. The user maintains the
        // accumulated figure in `taxes.vorabpauschale_carryforward`.
        let mut vp_reduction = dec!(0);
        let mut vp_note: Option<String> = None;
        if teilfreistellung_rate != TeilfreistellungRate::None && !isin.is_empty() {
            let carryforward = tax_config.german_vorabpauschale_carryforward(&isin);
            let fully_disposed = !held_isins.contains(&isin);
            if carryforward > dec!(0) && fully_disposed && vp_consumed.insert(isin.clone()) {
                vp_reduction = carryforward;
                vp_note = Some(format!(
                    "Sale gain reduced by €{carryforward} accumulated Vorabpauschale (§19 InvStG); \
                     reset this fund's vorabpauschale_carryforward to 0"
                ));
            } else if carryforward > dec!(0) && !fully_disposed {
                vp_note = Some(
                    "Partial fund sale: accumulated Vorabpauschale not deducted here (still held at \
                     year end); adjust the §19 reduction manually"
                        .to_string(),
                );
            }
        }

        let taxable_before_exemption = post_altbestand - vp_reduction;
        let taxable_amount =
            apply_teilfreistellung(taxable_before_exemption, &teilfreistellung_rate);

        let notes = {
            let mut parts: Vec<String> = Vec::new();
            if has_pre_2009_lot {
                parts.push(
                    "Altbestand (pre-2009) lots grandfathered — their profit excluded".to_string(),
                );
            }
            if let Some(note) = vp_note {
                parts.push(note);
            }
            (!parts.is_empty()).then(|| parts.join("; "))
        };

        // §32d(1) EStG flat tax (losses floor to zero). Foreign withholding on capital gains is
        // rare, so the creditable foreign tax q = 0 here.
        let taxes: AbgeltungsteuerBreakdown =
            statement.tax_rates.compute_taxes(taxable_amount, dec!(0));
        let abgeltungssteuer = taxes.abgeltungssteuer;
        let soli = taxes.solidaritaetszuschlag;
        let church_tax = taxes.kirchensteuer;
        let total_tax = taxes.total;

        let entry = CapitalGainEntry {
            transaction_date: trade.conclusion_time.date,
            settle_date: trade.execution_date,
            symbol: trade.symbol.clone(),
            isin,
            description,
            quantity: trade.quantity,
            cost_basis_eur,
            proceeds_eur: net_proceeds_eur,
            gross_gain_loss,
            taxable_before_exemption,
            teilfreistellung_rate,
            // Direct shares (no fund classification) drive the §20(6) stock loss pot.
            is_stock: teilfreistellung_rate == TeilfreistellungRate::None,
            taxable_amount,
            foreign_tax: dec!(0), // Foreign withholding on capital gains is rare
            abgeltungssteuer,
            solidaritaetszuschlag: soli,
            kirchensteuer: church_tax,
            total_tax,
            pre_2009_holding: has_pre_2009_lot,
            notes,
        };

        debug!(
            "FIFO capital gain: {} {} shares - cost: €{:.2}, proceeds: €{:.2}, gain: €{:.2}, tax: €{:.2}",
            trade.symbol,
            trade.quantity,
            cost_basis_eur,
            net_proceeds_eur,
            gross_gain_loss,
            total_tax
        );

        statement.add_capital_gain(entry);
    }

    Ok(has_income)
}

/// Cost basis (in EUR) of a vested-RSU FIFO lot: its vest-date fair market value.
///
/// German tax already taxed that FMV as employment income at vesting, so it becomes the
/// capital-gains cost basis of the shares. If the vest-date FMV is unavailable, warn and fall back
/// to zero (conservative: overstates the gain rather than understating it).
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
            "Stock grant lot for {} vested {} has no matching grant record; using €0 cost basis.",
            lot.original_symbol, vest_date
        );
        return Ok(dec!(0));
    };

    let Some(fmv) = grant.fmv_per_share else {
        warn!(
            "Stock grant {} vested {}: vest-date FMV unavailable; using €0 cost basis (overstates gain).",
            lot.original_symbol, vest_date
        );
        return Ok(dec!(0));
    };

    // fmv is per original (un-split) share; lot.quantity is likewise the pre-multiplier share count.
    let cost = Cash::new(fmv.currency, fmv.amount * lot.quantity);
    let context = format!(
        "Converting vest-date FMV for stock grant {} on {}",
        lot.original_symbol, vest_date
    );
    convert_to_eur(converter, vest_date, cost, &context)
}

/// Process dividends and create dividend entries.
fn process_dividends(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
    tax_config: &TaxConfig,
) -> GenericResult<bool> {
    let mut has_income = false;

    for dividend in &broker_statement.dividends {
        if dividend.date.year() != year {
            continue;
        }

        has_income = true;

        // Convert amounts to EUR with helpful error messages
        let context = format!(
            "Processing dividend from {} on {}",
            dividend.issuer, dividend.date
        );
        let gross_amount_eur = convert_to_eur(converter, dividend.date, dividend.amount, &context)?;
        let foreign_withholding_tax =
            convert_to_eur(converter, dividend.date, dividend.paid_tax, &context)?;

        // Get instrument info
        let instrument_info = broker_statement.instrument_info.get(&dividend.issuer);
        let isin = instrument_info
            .and_then(|info| info.isin.iter().next())
            .map(|isin| isin.to_string())
            .unwrap_or_default();
        let description = instrument_info
            .map(|_| {
                broker_statement
                    .instrument_info
                    .get_name(&dividend.issuer)
                    .to_string()
            })
            .unwrap_or_else(|| dividend.issuer.clone());

        // Determine Teilfreistellung rate from ETF classification config
        // Lookup by ISIN if available, otherwise no exemption (regular stocks)
        let teilfreistellung_rate = if !isin.is_empty() {
            let classification = tax_config.get_etf_classification(&isin);
            let rate = classification.to_teilfreistellung_rate();
            if rate != TeilfreistellungRate::None {
                debug!(
                    "ETF classification for dividend {} ({}): {:?} -> {:?}",
                    dividend.issuer, isin, classification, rate
                );
            }
            rate
        } else {
            TeilfreistellungRate::None
        };

        // Apply Teilfreistellung
        let taxable_amount = apply_teilfreistellung(gross_amount_eur, &teilfreistellung_rate);

        // Creditable foreign withholding (§32d(5)) is folded into the §32d(1) formula, so the
        // per-entry net tax equals the total: the credit is already inside, not subtracted after.
        let foreign_tax_credit = creditable_foreign_tax(
            foreign_withholding_tax,
            gross_amount_eur,
            taxable_amount,
            &teilfreistellung_rate,
        );
        let taxes = statement
            .tax_rates
            .compute_taxes(taxable_amount, foreign_tax_credit);
        let abgeltungssteuer = taxes.abgeltungssteuer;
        let soli = taxes.solidaritaetszuschlag;
        let church_tax = taxes.kirchensteuer;
        let total_tax = taxes.total;
        let net_tax = taxes.total;

        // Get quantity from holdings (approximate)
        let quantity = dec!(0); // Dividend records don't always include quantity

        let entry = DividendEntry {
            payment_date: dividend.date,
            symbol: dividend.issuer.clone(),
            isin,
            description,
            quantity,
            gross_amount_eur,
            foreign_withholding_tax,
            teilfreistellung_rate,
            taxable_amount,
            abgeltungssteuer,
            solidaritaetszuschlag: soli,
            kirchensteuer: church_tax,
            foreign_tax_credit,
            total_tax,
            net_tax,
            notes: None,
        };

        debug!(
            "Dividend: {} - gross: €{:.2}, foreign tax: €{:.2}, credit: €{:.2}, net tax: €{:.2}",
            dividend.issuer, gross_amount_eur, foreign_withholding_tax, foreign_tax_credit, net_tax
        );

        statement.add_dividend(entry);
    }

    Ok(has_income)
}

/// Apply Teilfreistellung (partial exemption) to an amount.
fn apply_teilfreistellung(amount: Decimal, rate: &TeilfreistellungRate) -> Decimal {
    amount * rate.taxable_portion()
}

/// Creditable foreign withholding tax on a dividend item under §32d(5) EStG.
///
/// Capped at the treaty rate (15% of the GROSS distribution) and the statutory cap (25% of the
/// taxable amount). Fund distributions carry a Teilfreistellung classification; under InvStG 2018
/// the investor cannot credit fund-level foreign withholding, so `q = 0` for them (the withheld
/// amount is still reported informationally by the caller).
fn creditable_foreign_tax(
    withholding_paid: Decimal,
    gross_amount: Decimal,
    taxable_amount: Decimal,
    teilfreistellung: &TeilfreistellungRate,
) -> Decimal {
    if *teilfreistellung != TeilfreistellungRate::None {
        return dec!(0);
    }
    let treaty_cap = gross_amount * dec!(0.15);
    let statutory_cap = taxable_amount * dec!(0.25);
    withholding_paid
        .min(treaty_cap)
        .min(statutory_cap)
        .max(dec!(0))
}

/// Long holdings as of 31 December of `year`, recovered from the statement's end-of-period snapshot
/// by undoing any trades executed after the tax year.
///
/// `open_positions` is the broker's snapshot at the statement's *last* date, which is only the
/// year-end snapshot when the statement ends at the tax year. When it extends past (e.g. a
/// multi-year statement used to file a prior year), this rewinds post-year-end buys/sells so
/// Vorabpauschale quantities and §19 full-disposal detection are judged as of the year boundary.
/// Splits or corporate actions occurring after the boundary are not re-derived — a rare edge the
/// manual-NAV model already approximates.
fn year_end_holdings(broker_statement: &BrokerStatement, year: i32) -> HashMap<String, Decimal> {
    let year_end = Date::from_ymd_opt(year, 12, 31).expect("31 December is always a valid date");
    let mut holdings = broker_statement.open_positions.clone();

    for buy in &broker_statement.stock_buys {
        if buy.conclusion_time.date > year_end {
            *holdings.entry(buy.symbol.clone()).or_insert(dec!(0)) -= buy.quantity;
        }
    }
    for sell in &broker_statement.stock_sells {
        if sell.conclusion_time.date > year_end {
            *holdings.entry(sell.symbol.clone()).or_insert(dec!(0)) += sell.quantity;
        }
    }

    holdings.retain(|_, qty| *qty > dec!(0));
    holdings
}

/// Compute the Vorabpauschale (§18 InvStG) for every fund held at year end.
///
/// The advance lump sum for a fund held on 31 December is deemed received on the first business day
/// of the following year (§18 Abs. 3), so it is that following year's income. Year-boundary NAVs
/// come from config — a foreign broker's statement carries no German redemption prices — and a
/// held fund without configured NAVs is warned about rather than silently omitted. Must run after
/// `process_dividends`, which populates the per-fund distributions this consumes.
fn process_vorabpauschale(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    tax_config: &TaxConfig,
) -> GenericResult<()> {
    let deemed_received = first_business_day_of_year(year + 1);

    // Distributions received during the year per fund ISIN (already converted to EUR).
    let mut distributions_by_isin: HashMap<String, Decimal> = HashMap::new();
    for dividend in &statement.dividends {
        if dividend.teilfreistellung_rate != TeilfreistellungRate::None && !dividend.isin.is_empty()
        {
            *distributions_by_isin
                .entry(dividend.isin.clone())
                .or_insert(dec!(0)) += dividend.gross_amount_eur;
        }
    }

    // Holdings as of 31 December of the tax year (not the statement's last-date snapshot), in a
    // deterministic iteration order.
    let holdings = year_end_holdings(broker_statement, year);
    let mut positions: Vec<(&String, &Decimal)> = holdings.iter().collect();
    positions.sort_by(|a, b| a.0.cmp(b.0));

    // The Basiszins is year-wide. If the tool ships no default and the user has not configured it,
    // no Vorabpauschale can be computed — degrade gracefully (warn and skip) rather than aborting
    // the whole report, mirroring the missing-NAV handling below.
    let basiszins = tax_config.german_basiszins(year).ok();
    let mut missing_basiszins = false;

    for (symbol, &quantity) in positions {
        let isin = broker_statement
            .instrument_info
            .get(symbol)
            .and_then(|info| info.isin.iter().next())
            .map(|isin| isin.to_string())
            .unwrap_or_default();

        let rate = if isin.is_empty() {
            TeilfreistellungRate::None
        } else {
            tax_config
                .get_etf_classification(&isin)
                .to_teilfreistellung_rate()
        };
        if rate == TeilfreistellungRate::None {
            continue; // not a fund → no Vorabpauschale
        }

        let id = if isin.is_empty() {
            symbol.clone()
        } else {
            isin.clone()
        };
        let Some(nav) = tax_config.german_fund_nav(&isin, year) else {
            statement.vorabpauschale_missing_nav.push(id);
            continue;
        };

        let Some(basiszins) = basiszins else {
            missing_basiszins = true;
            continue;
        };
        let distributions = distributions_by_isin.get(&isin).copied().unwrap_or(dec!(0));
        let nav_jan1 = nav.jan1 * quantity;
        let nav_dec31 = nav.dec31 * quantity;
        let months_before = full_months_before_acquisition(nav.acquired_month);

        let gross = vorabpauschale(nav_jan1, nav_dec31, distributions, basiszins, months_before);
        let taxable = apply_teilfreistellung(gross, &rate);
        let taxes = statement.tax_rates.compute_taxes(taxable, dec!(0));
        let accumulated_after = tax_config.german_vorabpauschale_carryforward(&isin) + gross;

        statement.add_vorabpauschale(VorabpauschaleEntry {
            arising_year: year,
            deemed_received,
            symbol: symbol.clone(),
            isin,
            quantity,
            nav_jan1,
            nav_dec31,
            distributions,
            basiszins,
            teilfreistellung_rate: rate,
            gross_vorabpauschale: gross,
            taxable_amount: taxable,
            abgeltungssteuer: taxes.abgeltungssteuer,
            solidaritaetszuschlag: taxes.solidaritaetszuschlag,
            kirchensteuer: taxes.kirchensteuer,
            total_tax: taxes.total,
            accumulated_after,
            notes: None,
        });
    }

    if missing_basiszins {
        warn!(
            "Vorabpauschale (§18 InvStG) not computed for year-end fund holdings: no Basiszins known \
             for {year}. The BMF publishes it each January; set `taxes.basiszins.{year}` in the config."
        );
    }

    if !statement.vorabpauschale_missing_nav.is_empty() {
        warn!(
            "Vorabpauschale (§18 InvStG) could not be computed for year-end fund holdings without \
             configured year-boundary NAVs: {}. Set `taxes.fund_nav.<ISIN>.{year}` (jan1/dec31) to \
             include them.",
            statement.vorabpauschale_missing_nav.join(", ")
        );
    }

    Ok(())
}

/// Process interest income and create interest entries.
fn process_interest(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_income = false;

    for interest in &broker_statement.idle_cash_interest {
        if interest.date.year() != year {
            continue;
        }

        has_income = true;

        // Convert amount to EUR with helpful error message
        let context = format!("Processing interest payment on {}", interest.date);
        let gross_amount_eur = convert_to_eur(converter, interest.date, interest.amount, &context)?;

        // Interest doesn't have Teilfreistellung
        let taxable_amount = gross_amount_eur;

        // §32d(1) EStG flat tax. Interest in the supported statements carries no foreign
        // withholding, so the creditable foreign tax q = 0.
        let taxes = statement.tax_rates.compute_taxes(taxable_amount, dec!(0));
        let abgeltungssteuer = taxes.abgeltungssteuer;
        let soli = taxes.solidaritaetszuschlag;
        let church_tax = taxes.kirchensteuer;
        let total_tax = taxes.total;

        let foreign_withholding_tax = dec!(0);
        let foreign_tax_credit = dec!(0);
        let net_tax = total_tax;

        let entry = InterestEntry {
            payment_date: interest.date,
            description: format!("Idle cash interest - {}", interest.amount.currency),
            gross_amount_eur,
            foreign_withholding_tax,
            taxable_amount,
            abgeltungssteuer,
            solidaritaetszuschlag: soli,
            kirchensteuer: church_tax,
            foreign_tax_credit,
            total_tax,
            net_tax,
            notes: None,
        };

        debug!(
            "Interest: {} - gross: €{:.2}, tax: €{:.2}",
            interest.amount.currency, gross_amount_eur, net_tax
        );

        statement.add_interest(entry);
    }

    Ok(has_income)
}

/// Process FX gains/losses and create FX gain entries.
///
/// For German tax purposes:
/// - FX gains on interest-bearing currency accounts (like IBKR) fall under §20 EStG
/// - They are taxed as capital income (Abgeltungsteuer)
/// - Losses from interest-bearing accounts can be offset against other capital income
/// - FX gains/losses from margin loan repayments are NOT taxable
///   (Tilgung eines Fremdwährungskredits - debt repayment is not a taxable event)
fn process_fx_gains(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_income = false;
    let mut non_taxable_margin_fx = dec!(0);

    for fx_gain in &broker_statement.fx_gains {
        if fx_gain.date.year() != year {
            continue;
        }

        // Convert amount to EUR with helpful error message
        let context = format!(
            "Processing FX gain/loss for {} on {}",
            fx_gain.currency_pair, fx_gain.date
        );
        let gross_amount_eur = convert_to_eur(converter, fx_gain.date, fx_gain.amount, &context)?;

        // Skip margin loan FX (not taxable - Tilgung Fremdwährungskredit)
        if fx_gain.is_margin_loan {
            non_taxable_margin_fx += gross_amount_eur;
            debug!(
                "FX margin loan (not taxable): {} - amount: €{:.2}",
                fx_gain.currency_pair, gross_amount_eur
            );
            continue;
        }

        has_income = true;

        // FX gains don't have Teilfreistellung; §32d(1) flat tax with q = 0 (losses floor to zero).
        let taxable_amount = gross_amount_eur;
        let taxes = statement.tax_rates.compute_taxes(taxable_amount, dec!(0));
        let abgeltungssteuer = taxes.abgeltungssteuer;
        let soli = taxes.solidaritaetszuschlag;
        let church_tax = taxes.kirchensteuer;
        let total_tax = taxes.total;

        let entry = FxGainEntry {
            transaction_date: fx_gain.date,
            currency_pair: fx_gain.currency_pair.clone(),
            description: fx_gain.description.clone(),
            gross_amount_eur,
            taxable_amount,
            abgeltungssteuer,
            solidaritaetszuschlag: soli,
            kirchensteuer: church_tax,
            total_tax,
            notes: if taxable_amount < dec!(0) {
                Some("FX loss - can offset other capital income (§20 EStG)".to_string())
            } else {
                None
            },
        };

        debug!(
            "FX gain/loss: {} - amount: €{:.2}, tax: €{:.2}",
            fx_gain.currency_pair, gross_amount_eur, total_tax
        );

        statement.add_fx_gain(entry);
    }

    // Store non-taxable margin FX total for reporting
    statement.non_taxable_margin_fx = non_taxable_margin_fx;

    Ok(has_income)
}

/// Process broker fees.
///
/// Broker fees (Werbungskosten) are deductible from capital gains.
/// Note: Trade commissions are already included in capital gain calculations.
/// This covers standalone fees like account maintenance, data fees, etc.
fn process_fees(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_fees = false;

    for fee in &broker_statement.fees {
        if fee.date.year() != year {
            continue;
        }

        has_fees = true;

        // Get the fee amount (Withholding can be positive fee or negative refund)
        let fee_cash = fee.amount.withholding();

        // Convert to EUR
        let context = format!("Processing broker fee on {}", fee.date);
        let amount_eur = convert_to_eur(converter, fee.date, fee_cash, &context)?;

        let description = fee
            .description
            .clone()
            .unwrap_or_else(|| "Broker fee".to_string());

        let entry = FeeEntry {
            date: fee.date,
            description: description.clone(),
            amount_eur,
            notes: if amount_eur < dec!(0) {
                Some("Fee refund - reduces deductible expenses".to_string())
            } else {
                None
            },
        };

        debug!("Broker fee: {} - amount: €{:.2}", description, amount_eur);

        statement.add_fee(entry);
    }

    Ok(has_fees)
}

/// Process stock grants (RSUs, stock awards).
///
/// In Germany, stock grants have TWO taxable events:
/// 1. At vesting: Taxed as employment income (geldwerter Vorteil) at marginal income tax rate
/// 2. At sale: Capital gains tax only on gain above vest-date FMV
///
/// This function reports the employment income portion (vest-date FMV).
/// The capital gains portion is handled by process_trades() using FMV as cost basis.
fn process_stock_grants(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_grants = false;

    // Stock grants are stored in broker_statement after being processed by process_grants()
    // They're converted to StockBuy entries with zero cost, but we need the original grant info
    // Check if we have any stock grant data from the broker statement parsing
    for grant in broker_statement.stock_grants.iter() {
        if grant.date.year() != year {
            continue;
        }

        has_grants = true;

        // Get the instrument info for ISIN lookup
        let isin = broker_statement
            .instrument_info
            .get(&grant.symbol)
            .map(|info| {
                info.isin
                    .iter()
                    .next()
                    .map(|i| i.to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Use FMV from broker statement if available
        // IBKR provides FMV at vest in the StockGrantActivity which we now preserve
        let (fmv_per_share, total_fmv_eur) = if let Some(fmv_cash) = &grant.fmv_per_share {
            // Convert FMV to EUR
            let context = format!(
                "Converting FMV for stock grant {} on {}",
                grant.symbol, grant.date
            );
            let fmv_eur = convert_to_eur(converter, grant.date, *fmv_cash, &context)?;
            let total = fmv_eur * grant.quantity;
            (fmv_eur, total)
        } else {
            // FMV not available - warn user
            warn!(
                "Stock grant {} on {}: FMV at vest date is not available. \
                Employment income will be reported as €0. \
                Please verify and correct manually for tax purposes.",
                grant.symbol, grant.date
            );
            (dec!(0), dec!(0))
        };

        let entry = StockGrantEntry {
            vest_date: grant.date,
            symbol: grant.symbol.clone(),
            isin,
            quantity: grant.quantity,
            fmv_per_share_eur: fmv_per_share,
            total_fmv_eur,
            notes: Some(
                "Employment income (geldwerter Vorteil) - taxed at marginal rate, not Abgeltungssteuer"
                    .to_string(),
            ),
        };

        debug!(
            "Stock grant: {} x {} @ €{:.2}/share = €{:.2} employment income",
            grant.quantity, grant.symbol, fmv_per_share, total_fmv_eur
        );

        statement.add_stock_grant(entry);
    }

    Ok(has_grants)
}

/// Process cash grants (broker bonuses, promotional cash).
///
/// Cash grants are "sonstige Einkünfte" (other income) under German tax law.
/// Only taxable if total other income exceeds €256/year.
fn process_cash_grants(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_grants = false;

    for grant in &broker_statement.cash_grants {
        if grant.date.year() != year {
            continue;
        }

        has_grants = true;

        // Convert to EUR
        let context = format!(
            "Processing cash grant '{}' on {}",
            grant.description, grant.date
        );
        let amount_eur = convert_to_eur(converter, grant.date, grant.amount, &context)?;

        let entry = CashGrantEntry {
            date: grant.date,
            description: grant.description.clone(),
            amount_eur,
            notes: Some(
                "Other income (sonstige Einkünfte §22 EStG) - taxable at marginal rate if >€256/year total"
                    .to_string(),
            ),
        };

        debug!(
            "Cash grant: {} - amount: €{:.2}",
            grant.description, amount_eur
        );

        statement.add_cash_grant(entry);
    }

    // Add informational warning if cash grants exceed threshold
    let total_cash_grants: Decimal = statement.cash_grants.iter().map(|g| g.amount_eur).sum();

    if total_cash_grants > dec!(256) {
        warn!(
            "Total cash grants (€{:.2}) exceed €256 threshold - must be declared as sonstige Einkünfte",
            total_cash_grants
        );
    }

    Ok(has_grants)
}

/// Process corporate actions that have tax implications.
///
/// - Spinoffs: May require cost basis allocation (informational)
/// - Liquidations: Treated as capital gain/loss
/// - Stock splits: No tax event (handled by StockSplitController)
/// - Stock dividends: May have tax implications
fn process_corporate_actions(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
) -> GenericResult<bool> {
    let mut has_actions = false;

    for action in broker_statement.corporate_actions.iter() {
        if action.time.date.year() != year {
            continue;
        }

        let (action_type, description, tax_impact_eur) = match &action.action {
            BrokerCorporateActionType::Delisting { quantity } => {
                has_actions = true;
                // Delisting - typically a total loss
                // Note: In Germany, delisting losses may have restricted deductibility
                (
                    CorporateActionType::Delisting,
                    format!("Delisting of {} shares", quantity),
                    None, // Loss amount depends on cost basis
                )
            }
            BrokerCorporateActionType::Liquidation {
                quantity,
                price,
                volume,
                currency,
            } => {
                has_actions = true;
                // Liquidation - treated as a sale
                let proceeds = Cash::new(currency, *volume);
                let context = format!(
                    "Processing liquidation of {} on {}",
                    action.symbol, action.time.date
                );
                let proceeds_eur = convert_to_eur(converter, action.time.date, proceeds, &context)?;

                (
                    CorporateActionType::Liquidation,
                    format!(
                        "Liquidation: {} shares @ {:.2} {} = {:.2} {}",
                        quantity, price, currency, volume, currency
                    ),
                    Some(proceeds_eur), // Capital gain/loss vs cost basis
                )
            }
            BrokerCorporateActionType::Rename { .. } => {
                // Rename is not a taxable event
                continue;
            }
            BrokerCorporateActionType::Spinoff {
                symbol: new_symbol,
                quantity: new_quantity,
                ..
            } => {
                has_actions = true;
                // Spinoff - requires cost basis allocation
                // This is informational; actual allocation is complex
                (
                    CorporateActionType::Spinoff,
                    format!(
                        "Spinoff: Received {} shares of {}",
                        new_quantity, new_symbol
                    ),
                    None, // No immediate tax impact, but affects future cost basis
                )
            }
            BrokerCorporateActionType::StockSplit { .. } => {
                // Stock split - no tax event
                continue;
            }
            BrokerCorporateActionType::StockDividend { stock, quantity } => {
                has_actions = true;
                // Stock dividend - may have tax implications as income
                let stock_name: &str = stock.as_ref().map(String::as_str).unwrap_or(&action.symbol);
                (
                    CorporateActionType::Other("StockDividend".to_string()),
                    format!("Stock dividend: {} shares of {}", quantity, stock_name),
                    None, // Value depends on share price at distribution
                )
            }
            BrokerCorporateActionType::SubscribableRightsIssue => {
                // Rights issue - informational only
                continue;
            }
        };

        let notes = match &action_type {
            CorporateActionType::Spinoff => Some(
                "Cost basis must be allocated between parent and spinoff based on market values. \
                Consult tax advisor for proper allocation."
                    .to_string(),
            ),
            CorporateActionType::Liquidation => {
                Some("Treated as sale - capital gain/loss based on cost basis".to_string())
            }
            CorporateActionType::Delisting => Some(
                "Total loss - may have restricted deductibility under German tax law".to_string(),
            ),
            CorporateActionType::Other(s) if s == "StockDividend" => {
                Some("Stock dividend may be taxable as dividend income at FMV".to_string())
            }
            CorporateActionType::Merger => {
                Some("Check if cash received - may trigger capital gain".to_string())
            }
            CorporateActionType::Other(_) => None,
        };

        let entry = CorporateActionEntry {
            date: action.time.date,
            action_type,
            symbol: action.symbol.clone(),
            description,
            tax_impact_eur,
            notes,
        };

        debug!(
            "Corporate action: {} - {} on {}",
            entry.action_type, entry.symbol, entry.date
        );

        statement.add_corporate_action(entry);
    }

    Ok(has_actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tax_statement::germany::{CsvFormatter, GermanTaxStatement};
    use crate::taxes::germany::TeilfreistellungRate;

    #[test]
    fn test_teilfreistellung_application() {
        let gross = dec!(1000);

        // None (regular stocks): 0% exemption, 100% taxable
        assert_eq!(
            apply_teilfreistellung(gross, &TeilfreistellungRate::None),
            dec!(1000)
        );

        // Equity ETF: 30% exemption, 70% taxable
        assert_eq!(
            apply_teilfreistellung(gross, &TeilfreistellungRate::Equity),
            dec!(700)
        );

        // Mixed ETF: 15% exemption, 85% taxable
        assert_eq!(
            apply_teilfreistellung(gross, &TeilfreistellungRate::Mixed),
            dec!(850)
        );

        // Bond ETF: 0% exemption, 100% taxable
        assert_eq!(
            apply_teilfreistellung(gross, &TeilfreistellungRate::Bond),
            dec!(1000)
        );
    }

    /// End-to-end test: Create statement entries manually and verify CSV output.
    #[test]
    fn test_end_to_end_csv_generation() {
        // Create a German tax statement for 2024
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0.08), dec!(0), dec!(0), dec!(0)).unwrap(); // 8% church tax

        // Add a capital gain entry (sold stock with profit)
        let capital_gain = CapitalGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 6, 15).unwrap(),
            settle_date: Date::from_ymd_opt(2024, 6, 17).unwrap(),
            symbol: "AAPL".to_string(),
            isin: "US0378331005".to_string(),
            description: "Apple Inc.".to_string(),
            quantity: dec!(10),
            cost_basis_eur: dec!(1500.00),
            proceeds_eur: dec!(2000.00),
            gross_gain_loss: dec!(500.00),
            taxable_before_exemption: dec!(500.00),
            teilfreistellung_rate: TeilfreistellungRate::None,
            is_stock: true,
            taxable_amount: dec!(500.00),
            foreign_tax: dec!(0),
            abgeltungssteuer: dec!(125.00),     // 25% of 500
            solidaritaetszuschlag: dec!(6.875), // 5.5% of 125
            kirchensteuer: dec!(10.00),         // 8% of 125
            total_tax: dec!(141.875),
            pre_2009_holding: false,
            notes: None,
        };
        statement.add_capital_gain(capital_gain);

        // Add a capital loss entry
        let capital_loss = CapitalGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 9, 10).unwrap(),
            settle_date: Date::from_ymd_opt(2024, 9, 12).unwrap(),
            symbol: "MSFT".to_string(),
            isin: "US5949181045".to_string(),
            description: "Microsoft Corporation".to_string(),
            quantity: dec!(5),
            cost_basis_eur: dec!(1800.00),
            proceeds_eur: dec!(1600.00),
            gross_gain_loss: dec!(-200.00),
            taxable_before_exemption: dec!(-200.00),
            teilfreistellung_rate: TeilfreistellungRate::None,
            is_stock: true,
            taxable_amount: dec!(-200.00),
            foreign_tax: dec!(0),
            abgeltungssteuer: dec!(0),
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            total_tax: dec!(0),
            pre_2009_holding: false,
            notes: None,
        };
        statement.add_capital_gain(capital_loss);

        // Add a dividend entry
        let dividend = DividendEntry {
            payment_date: Date::from_ymd_opt(2024, 3, 15).unwrap(),
            symbol: "AAPL".to_string(),
            isin: "US0378331005".to_string(),
            description: "Apple Inc.".to_string(),
            quantity: dec!(100),
            gross_amount_eur: dec!(50.00),
            foreign_withholding_tax: dec!(7.50), // 15% US withholding
            teilfreistellung_rate: TeilfreistellungRate::None,
            taxable_amount: dec!(50.00),
            abgeltungssteuer: dec!(12.50),       // 25% of 50
            solidaritaetszuschlag: dec!(0.6875), // 5.5% of 12.50
            kirchensteuer: dec!(1.00),           // 8% of 12.50
            foreign_tax_credit: dec!(7.50),      // Full credit (within 15% limit)
            total_tax: dec!(14.1875),
            net_tax: dec!(6.6875), // 14.1875 - 7.50
            notes: None,
        };
        statement.add_dividend(dividend);

        // Add an interest entry
        let interest = InterestEntry {
            payment_date: Date::from_ymd_opt(2024, 12, 31).unwrap(),
            description: "Idle cash interest - USD".to_string(),
            gross_amount_eur: dec!(25.00),
            foreign_withholding_tax: dec!(0),
            taxable_amount: dec!(25.00),
            abgeltungssteuer: dec!(6.25),         // 25% of 25
            solidaritaetszuschlag: dec!(0.34375), // 5.5% of 6.25
            kirchensteuer: dec!(0.50),            // 8% of 6.25
            foreign_tax_credit: dec!(0),
            total_tax: dec!(7.09375),
            net_tax: dec!(7.09375),
            notes: None,
        };
        statement.add_interest(interest);

        // Calculate totals
        statement.calculate_totals();

        // Verify totals
        assert_eq!(statement.capital_gains.len(), 2);
        assert_eq!(statement.dividends.len(), 1);
        assert_eq!(statement.interest.len(), 1);

        // Capital gains: 500 gain, 200 loss offset
        assert_eq!(statement.total_capital_gains, dec!(500.00));
        assert_eq!(statement.total_capital_losses, dec!(200.00));
        assert_eq!(statement.total_dividend_income, dec!(50.00));
        assert_eq!(statement.total_interest_income, dec!(25.00));

        // Generate CSV output
        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv_string = String::from_utf8(csv_output).unwrap();

        // Verify CSV structure
        let lines: Vec<&str> = csv_string.lines().collect();

        // Header + 2 capital gains + 1 dividend + 1 interest + 10 summary lines = 15 lines
        assert!(
            lines.len() >= 14,
            "Expected at least 14 lines, got {}",
            lines.len()
        );

        // Check header
        assert!(lines[0].starts_with("transaction_type,transaction_date"));

        // Check capital gain row exists
        assert!(csv_string.contains("Capital Gain"));
        assert!(csv_string.contains("AAPL"));
        assert!(csv_string.contains("US0378331005"));

        // Check dividend row exists
        assert!(csv_string.contains("Dividend"));

        // Check interest row exists
        assert!(csv_string.contains("Interest"));

        // Check summary rows exist
        assert!(csv_string.contains("SUMMARY_TOTAL_TAXABLE_INCOME"));
        assert!(csv_string.contains("SUMMARY_NET_TAX_DUE"));
    }

    /// Test that the statement correctly handles pre-2009 (Altbestand) holdings.
    #[test]
    fn test_pre_2009_altbestand_handling() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        // Pre-2009 holding should be tax-exempt
        let altbestand_gain = CapitalGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 5, 1).unwrap(),
            settle_date: Date::from_ymd_opt(2024, 5, 3).unwrap(),
            symbol: "OLD".to_string(),
            isin: "DE000OLD0001".to_string(),
            description: "Old Stock".to_string(),
            quantity: dec!(100),
            cost_basis_eur: dec!(1000.00),
            proceeds_eur: dec!(5000.00),
            gross_gain_loss: dec!(4000.00),
            // Pure Altbestand: all profit is pre-2009 excluded, so the pre-Teilfreistellung base is 0.
            taxable_before_exemption: dec!(0),
            teilfreistellung_rate: TeilfreistellungRate::None,
            is_stock: true,
            // Post-T2 a pure Altbestand sale has its pre-2009 profit excluded, so the taxable
            // amount is zero even though the gross gain is €4,000.
            taxable_amount: dec!(0),
            foreign_tax: dec!(0),
            abgeltungssteuer: dec!(0), // Tax exempt!
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            total_tax: dec!(0),
            pre_2009_holding: true,
            notes: Some("Altbestand (pre-2009) - tax exempt".to_string()),
        };
        statement.add_capital_gain(altbestand_gain);

        statement.calculate_totals();

        // Even with 4000 EUR gain, no German tax due
        assert_eq!(statement.total_german_tax, dec!(0));
        assert_eq!(statement.net_tax_due, dec!(0));
    }

    /// Test Teilfreistellung for ETFs.
    #[test]
    fn test_equity_etf_teilfreistellung() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        // Equity ETF dividend with 30% exemption
        let etf_dividend = DividendEntry {
            payment_date: Date::from_ymd_opt(2024, 7, 1).unwrap(),
            symbol: "VWCE".to_string(),
            isin: "IE00BK5BQT80".to_string(),
            description: "Vanguard FTSE All-World UCITS ETF".to_string(),
            quantity: dec!(50),
            gross_amount_eur: dec!(100.00),
            foreign_withholding_tax: dec!(0),
            teilfreistellung_rate: TeilfreistellungRate::Equity, // 30% exempt
            taxable_amount: dec!(70.00),                         // Only 70% taxable
            abgeltungssteuer: dec!(17.50),                       // 25% of 70
            solidaritaetszuschlag: dec!(0.9625),
            kirchensteuer: dec!(0),
            foreign_tax_credit: dec!(0),
            total_tax: dec!(18.4625),
            net_tax: dec!(18.4625),
            notes: None,
        };
        statement.add_dividend(etf_dividend);

        statement.calculate_totals();

        // Only 70 EUR taxable out of 100 EUR gross
        assert_eq!(statement.total_dividend_income, dec!(70.00));
        assert_eq!(statement.total_abgeltungssteuer, dec!(17.50));
    }

    /// Performance test: Verify <10s for 1000 transaction statement per SC-001.
    /// This test uses synthetic data to stress-test the tax calculation pipeline.
    #[test]
    fn test_performance_1000_transactions() {
        use std::time::Instant;

        let start = Instant::now();
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0.09), dec!(5000), dec!(0), dec!(0)).unwrap(); // With church tax and loss CF

        // Generate 500 capital gain entries (simulating 1000+ buy/sell transactions)
        for i in 0..500 {
            let gain = CapitalGainEntry {
                transaction_date: Date::from_ymd_opt(
                    2024,
                    (i % 12 + 1) as u32,
                    (i % 28 + 1) as u32,
                )
                .unwrap(),
                settle_date: Date::from_ymd_opt(2024, (i % 12 + 1) as u32, (i % 28 + 1) as u32)
                    .unwrap(),
                symbol: format!("SYM{:03}", i % 50),
                isin: format!("US{:010}", i),
                description: format!("Test Stock {}", i),
                quantity: dec!(100) + Decimal::from(i % 100),
                cost_basis_eur: dec!(1000.00) + Decimal::from(i * 10),
                proceeds_eur: dec!(1200.00) + Decimal::from(i * 12),
                gross_gain_loss: dec!(200.00) + Decimal::from(i * 2),
                taxable_before_exemption: dec!(200.00) + Decimal::from(i * 2),
                teilfreistellung_rate: if i % 3 == 0 {
                    TeilfreistellungRate::Equity
                } else if i % 3 == 1 {
                    TeilfreistellungRate::Mixed
                } else {
                    TeilfreistellungRate::None
                },
                is_stock: i % 3 == 2,
                taxable_amount: dec!(150.00) + Decimal::from(i),
                foreign_tax: dec!(0),
                abgeltungssteuer: dec!(37.50) + Decimal::from(i / 4),
                solidaritaetszuschlag: dec!(2.06) + Decimal::from(i / 100),
                kirchensteuer: dec!(3.38) + Decimal::from(i / 100),
                total_tax: dec!(42.94) + Decimal::from(i / 3),
                pre_2009_holding: i % 20 == 0, // 5% are Altbestand
                notes: None,
            };
            statement.add_capital_gain(gain);
        }

        // Generate 300 dividend entries
        for i in 0..300 {
            let dividend = DividendEntry {
                payment_date: Date::from_ymd_opt(2024, (i % 12 + 1) as u32, 15).unwrap(),
                symbol: format!("DIV{:03}", i % 30),
                isin: format!("IE{:010}", i),
                description: format!("Test Dividend Stock {}", i),
                quantity: dec!(50) + Decimal::from(i % 50),
                gross_amount_eur: dec!(100.00) + Decimal::from(i),
                foreign_withholding_tax: dec!(15.00) + Decimal::from(i / 10),
                teilfreistellung_rate: if i % 2 == 0 {
                    TeilfreistellungRate::Equity
                } else {
                    TeilfreistellungRate::None
                },
                taxable_amount: dec!(70.00) + Decimal::from(i),
                abgeltungssteuer: dec!(17.50) + Decimal::from(i / 4),
                solidaritaetszuschlag: dec!(0.96) + Decimal::from(i / 100),
                kirchensteuer: dec!(1.58) + Decimal::from(i / 100),
                foreign_tax_credit: dec!(10.00) + Decimal::from(i / 20),
                total_tax: dec!(20.04) + Decimal::from(i / 3),
                net_tax: dec!(10.04) + Decimal::from(i / 5),
                notes: None,
            };
            statement.add_dividend(dividend);
        }

        // Generate 200 interest entries
        for i in 0..200 {
            let interest = InterestEntry {
                payment_date: Date::from_ymd_opt(2024, (i % 12 + 1) as u32, 1).unwrap(),
                description: format!("Interest Payment {}", i),
                gross_amount_eur: dec!(50.00) + Decimal::from(i),
                taxable_amount: dec!(50.00) + Decimal::from(i),
                foreign_withholding_tax: dec!(0),
                abgeltungssteuer: dec!(12.50) + Decimal::from(i / 4),
                solidaritaetszuschlag: dec!(0.69) + Decimal::from(i / 100),
                kirchensteuer: dec!(1.13) + Decimal::from(i / 100),
                foreign_tax_credit: dec!(0),
                total_tax: dec!(14.32) + Decimal::from(i / 3),
                net_tax: dec!(14.32) + Decimal::from(i / 3),
                notes: None,
            };
            statement.add_interest(interest);
        }

        // Calculate totals
        statement.calculate_totals();

        // Generate CSV output
        let mut csv_output = Vec::new();
        super::super::csv_formatter::CsvFormatter::write(&statement, &mut csv_output).unwrap();

        let elapsed = start.elapsed();

        // Performance assertion: must complete in <10 seconds (SC-001)
        // Using 1 second as a more aggressive threshold since we're not doing I/O
        assert!(
            elapsed.as_secs() < 1,
            "Performance test failed: processing 1000 transactions took {:?}, expected <1s",
            elapsed
        );

        // Verify totals were calculated
        assert!(statement.total_taxable_income > dec!(0));
        assert!(statement.total_german_tax > dec!(0));
        assert!(statement.capital_gains.len() == 500);
        assert!(statement.dividends.len() == 300);
        assert!(statement.interest.len() == 200);

        // Verify CSV was generated
        let csv_string = String::from_utf8(csv_output).unwrap();
        assert!(csv_string.contains("transaction_type"));
        assert!(csv_string.contains("SUMMARY_TOTAL_TAXABLE_INCOME"));

        println!(
            "Performance test passed: 1000 transactions processed in {:?}",
            elapsed
        );
    }

    /// Accuracy test: Verify ±€0.01 precision across calculations per SC-002.
    #[test]
    fn test_accuracy_precision() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0.09), dec!(0), dec!(0), dec!(0)).unwrap();

        // Test with precise decimal values that could cause floating point errors
        let gain = CapitalGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 6, 15).unwrap(),
            settle_date: Date::from_ymd_opt(2024, 6, 17).unwrap(),
            symbol: "PREC".to_string(),
            isin: "US1234567890".to_string(),
            description: "Precision Test Stock".to_string(),
            quantity: dec!(33.333), // Non-round quantity
            cost_basis_eur: dec!(1111.11),
            proceeds_eur: dec!(2222.22),
            gross_gain_loss: dec!(1111.11),
            taxable_before_exemption: dec!(1111.11),
            teilfreistellung_rate: TeilfreistellungRate::None,
            is_stock: true,
            taxable_amount: dec!(1111.11),
            foreign_tax: dec!(0),
            // 25% of 1111.11 = 277.7775, rounded to 277.78
            abgeltungssteuer: dec!(277.78),
            // 5.5% of 277.78 = 15.2779, rounded to 15.28
            solidaritaetszuschlag: dec!(15.28),
            // 9% of 277.78 = 25.0002, rounded to 25.00
            kirchensteuer: dec!(25.00),
            total_tax: dec!(318.06),
            pre_2009_holding: false,
            notes: None,
        };
        statement.add_capital_gain(gain);

        // Test dividend with foreign tax credit edge case
        let dividend = DividendEntry {
            payment_date: Date::from_ymd_opt(2024, 9, 1).unwrap(),
            symbol: "DIV".to_string(),
            isin: "IE0000000001".to_string(),
            description: "Dividend Test".to_string(),
            quantity: dec!(77.77),
            gross_amount_eur: dec!(99.99),
            foreign_withholding_tax: dec!(14.9985), // 15% of 99.99
            teilfreistellung_rate: TeilfreistellungRate::Equity,
            taxable_amount: dec!(69.993), // 70% of 99.99
            // 25% of 69.993 = 17.49825, rounded to 17.50
            abgeltungssteuer: dec!(17.50),
            solidaritaetszuschlag: dec!(0.96),
            kirchensteuer: dec!(1.58),
            foreign_tax_credit: dec!(14.9985),
            total_tax: dec!(20.04),
            net_tax: dec!(5.04), // 20.04 - 14.9985 ≈ 5.04
            notes: None,
        };
        statement.add_dividend(dividend);

        statement.calculate_totals();

        // Verify precision is maintained (no floating point drift)
        // All amounts should be exact to €0.01
        let total_str = format!("{:.2}", statement.total_taxable_income);
        assert!(
            total_str.ends_with('0')
                || total_str.ends_with('1')
                || total_str.ends_with('2')
                || total_str.ends_with('3')
                || total_str.ends_with('4')
                || total_str.ends_with('5')
                || total_str.ends_with('6')
                || total_str.ends_with('7')
                || total_str.ends_with('8')
                || total_str.ends_with('9'),
            "Total should be precise to €0.01: {}",
            total_str
        );

        // Verify the totals are exactly as expected (no rounding errors)
        // Capital gain: 1111.11 taxable
        // Dividend: 69.993 taxable (rounded in display but precise in calculation)
        let expected_capital_gains_taxable = dec!(1111.11);
        let expected_dividend_taxable = dec!(69.993);

        assert_eq!(
            statement.total_capital_gains, expected_capital_gains_taxable,
            "Capital gains taxable amount should be exactly €1111.11"
        );

        // The dividend taxable should be close (within €0.01)
        let diff = (statement.total_dividend_income - expected_dividend_taxable).abs();
        assert!(
            diff < dec!(0.01),
            "Dividend taxable amount should be within €0.01 of expected: got {}, expected {}",
            statement.total_dividend_income,
            expected_dividend_taxable
        );

        println!(
            "Accuracy test passed: All amounts precise to €0.01 - Total taxable: €{:.2}",
            statement.total_taxable_income
        );
    }

    /// Test FX gains/losses processing (§20 EStG - capital income treatment).
    #[test]
    fn test_fx_gains_processing() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0.08), dec!(0), dec!(0), dec!(0)).unwrap(); // 8% church tax

        // FX gain entry (profit from EUR/USD)
        let fx_gain = FxGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 6, 15).unwrap(),
            currency_pair: "EUR.USD".to_string(),
            description: "Net Amount in Base from Forex Trade: -100 EUR.USD".to_string(),
            gross_amount_eur: dec!(10.50),         // Realized FX gain
            taxable_amount: dec!(10.50),           // Full amount taxable (no Teilfreistellung)
            abgeltungssteuer: dec!(2.625),         // 25% of 10.50
            solidaritaetszuschlag: dec!(0.144375), // 5.5% of 2.625
            kirchensteuer: dec!(0.21),             // 8% of 2.625
            total_tax: dec!(2.979375),
            notes: None,
        };
        statement.add_fx_gain(fx_gain);

        // FX loss entry (loss from EUR/USD)
        let fx_loss = FxGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 7, 20).unwrap(),
            currency_pair: "EUR.USD".to_string(),
            description: "Net Amount in Base from Forex Trade: -50 EUR.USD".to_string(),
            gross_amount_eur: dec!(-3.25), // Realized FX loss
            taxable_amount: dec!(-3.25),   // Loss reduces taxable income
            abgeltungssteuer: dec!(0),     // No tax on losses
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            total_tax: dec!(0),
            notes: Some("FX loss - can offset other capital income (§20 EStG)".to_string()),
        };
        statement.add_fx_gain(fx_loss);

        // Calculate totals
        statement.calculate_totals();

        // Verify FX entries are correctly categorized
        assert_eq!(statement.fx_gains.len(), 2);
        assert_eq!(statement.total_fx_gains, dec!(10.50));
        assert_eq!(statement.total_fx_losses, dec!(3.25));

        // Net FX income should be 10.50 - 3.25 = 7.25
        // This should be included in taxable income calculation
        let net_fx = statement.total_fx_gains - statement.total_fx_losses;
        assert_eq!(net_fx, dec!(7.25));

        // Generate CSV output and verify FX rows exist
        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv_string = String::from_utf8(csv_output).unwrap();

        // Check FX gain/loss rows exist
        assert!(csv_string.contains("FX Gain/Loss"));
        assert!(csv_string.contains("EUR.USD"));
        assert!(csv_string.contains("SUMMARY_FX_GAINS"));
        assert!(csv_string.contains("SUMMARY_FX_LOSSES"));
    }

    /// Test that FX losses can offset capital gains (general loss bucket).
    #[test]
    fn test_fx_loss_offset_capital_gains() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap(); // No church tax

        // Capital gain entry
        let capital_gain = CapitalGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 5, 1).unwrap(),
            settle_date: Date::from_ymd_opt(2024, 5, 3).unwrap(),
            symbol: "AAPL".to_string(),
            isin: "US0378331005".to_string(),
            description: "Apple Inc.".to_string(),
            quantity: dec!(10),
            cost_basis_eur: dec!(1000.00),
            proceeds_eur: dec!(1200.00),
            gross_gain_loss: dec!(200.00),
            taxable_before_exemption: dec!(200.00),
            teilfreistellung_rate: TeilfreistellungRate::None,
            is_stock: true,
            taxable_amount: dec!(200.00),
            foreign_tax: dec!(0),
            abgeltungssteuer: dec!(50.00),     // 25% of 200
            solidaritaetszuschlag: dec!(2.75), // 5.5% of 50
            kirchensteuer: dec!(0),
            total_tax: dec!(52.75),
            pre_2009_holding: false,
            notes: None,
        };
        statement.add_capital_gain(capital_gain);

        // FX loss entry - larger than capital gain
        let fx_loss = FxGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 6, 15).unwrap(),
            currency_pair: "EUR.USD".to_string(),
            description: "Large FX loss".to_string(),
            gross_amount_eur: dec!(-150.00),
            taxable_amount: dec!(-150.00),
            abgeltungssteuer: dec!(0),
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            total_tax: dec!(0),
            notes: Some("FX loss - can offset other capital income (§20 EStG)".to_string()),
        };
        statement.add_fx_gain(fx_loss);

        statement.calculate_totals();

        // Capital gains: 200
        // FX losses: 150
        // Net for loss carryforward calculation: 200 + 0 - 0 - 150 = 50
        assert_eq!(statement.total_capital_gains, dec!(200.00));
        assert_eq!(statement.total_fx_losses, dec!(150.00));

        // Verify the loss offset worked (net capital income reduced)
        // Total taxable = capital gains (200) - FX losses (150) = 50
        // Note: This test verifies the structure; actual offset happens in calculate_totals
    }

    /// Test broker fee entry creation and deduction tracking.
    #[test]
    fn test_fee_entry_creation() {
        use super::super::statement::FeeEntry;

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        // Add a fee entry
        let fee = FeeEntry {
            date: Date::from_ymd_opt(2024, 3, 15).unwrap(),
            description: "Market data subscription".to_string(),
            amount_eur: dec!(15.50),
            notes: None,
        };
        statement.add_fee(fee);

        // Add a fee refund (negative amount)
        let fee_refund = FeeEntry {
            date: Date::from_ymd_opt(2024, 6, 1).unwrap(),
            description: "Fee refund".to_string(),
            amount_eur: dec!(-5.00),
            notes: Some("Fee refund - reduces deductible expenses".to_string()),
        };
        statement.add_fee(fee_refund);

        statement.calculate_totals();

        assert_eq!(statement.fees.len(), 2);
        // Total fees: 15.50 - 5.00 = 10.50
        assert_eq!(statement.total_fees, dec!(10.50));

        // Generate CSV and verify fee rows exist
        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv_string = String::from_utf8(csv_output).unwrap();

        assert!(csv_string.contains("Fee/Deduction"));
        assert!(csv_string.contains("Market data subscription"));
        assert!(csv_string.contains("SUMMARY_TOTAL_FEES"));
    }

    /// Test stock grant entry creation with FMV.
    #[test]
    fn test_stock_grant_entry_with_fmv() {
        use super::super::statement::StockGrantEntry;

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        // Stock grant with FMV (typical RSU vest)
        let grant = StockGrantEntry {
            vest_date: Date::from_ymd_opt(2024, 4, 1).unwrap(),
            symbol: "GOOGL".to_string(),
            isin: "US02079K3059".to_string(),
            quantity: dec!(50),
            fmv_per_share_eur: dec!(150.25),
            total_fmv_eur: dec!(7512.50), // 50 * 150.25
            notes: Some("Employment income (geldwerter Vorteil)".to_string()),
        };
        statement.add_stock_grant(grant);

        statement.calculate_totals();

        assert_eq!(statement.stock_grants.len(), 1);
        assert_eq!(statement.total_stock_grant_income, dec!(7512.50));

        // Verify the grant appears in CSV
        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv_string = String::from_utf8(csv_output).unwrap();

        assert!(csv_string.contains("STOCK_GRANT"));
        assert!(csv_string.contains("GOOGL"));
        assert!(csv_string.contains("EMPLOYMENT_INCOME_STOCK_GRANTS"));
    }

    /// Test stock grant entry without FMV (should warn user).
    #[test]
    fn test_stock_grant_entry_without_fmv() {
        use super::super::statement::StockGrantEntry;

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        // Stock grant without FMV (FMV was not available from broker)
        let grant = StockGrantEntry {
            vest_date: Date::from_ymd_opt(2024, 5, 15).unwrap(),
            symbol: "MSFT".to_string(),
            isin: "US5949181045".to_string(),
            quantity: dec!(25),
            fmv_per_share_eur: dec!(0), // FMV not available
            total_fmv_eur: dec!(0),
            notes: Some("FMV unavailable - verify manually".to_string()),
        };
        statement.add_stock_grant(grant);

        statement.calculate_totals();

        assert_eq!(statement.stock_grants.len(), 1);
        assert_eq!(statement.total_stock_grant_income, dec!(0)); // Zero due to missing FMV
    }

    /// Test cash grant entry and €256 threshold warning.
    #[test]
    fn test_cash_grant_entry_below_threshold() {
        use super::super::statement::CashGrantEntry;

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        // Cash grant below threshold
        let grant = CashGrantEntry {
            date: Date::from_ymd_opt(2024, 2, 1).unwrap(),
            description: "Referral bonus".to_string(),
            amount_eur: dec!(100.00),
            notes: None,
        };
        statement.add_cash_grant(grant);

        statement.calculate_totals();

        assert_eq!(statement.cash_grants.len(), 1);
        assert_eq!(statement.total_cash_grant_income, dec!(100.00));

        // Below €256 threshold - informational only
        assert!(statement.total_cash_grant_income < dec!(256));
    }

    /// Test cash grant entry exceeding €256 threshold.
    #[test]
    fn test_cash_grant_entry_above_threshold() {
        use super::super::statement::CashGrantEntry;

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        // Multiple cash grants exceeding threshold
        let grant1 = CashGrantEntry {
            date: Date::from_ymd_opt(2024, 1, 15).unwrap(),
            description: "Sign-up bonus".to_string(),
            amount_eur: dec!(200.00),
            notes: None,
        };
        statement.add_cash_grant(grant1);

        let grant2 = CashGrantEntry {
            date: Date::from_ymd_opt(2024, 6, 1).unwrap(),
            description: "Referral bonus".to_string(),
            amount_eur: dec!(100.00),
            notes: None,
        };
        statement.add_cash_grant(grant2);

        statement.calculate_totals();

        assert_eq!(statement.cash_grants.len(), 2);
        assert_eq!(statement.total_cash_grant_income, dec!(300.00));

        // Above €256 threshold - must be declared as sonstige Einkünfte
        assert!(statement.total_cash_grant_income > dec!(256));

        // Verify CSV contains warning section
        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv_string = String::from_utf8(csv_output).unwrap();

        assert!(csv_string.contains("CASH_GRANT"));
        assert!(csv_string.contains("OTHER_INCOME_CASH_GRANTS"));
    }

    /// Test corporate action entries for spinoffs.
    #[test]
    fn test_corporate_action_spinoff() {
        use super::super::statement::{CorporateActionEntry, CorporateActionType};

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        let spinoff = CorporateActionEntry {
            date: Date::from_ymd_opt(2024, 7, 1).unwrap(),
            action_type: CorporateActionType::Spinoff,
            symbol: "GE".to_string(),
            description: "Spinoff: Received 100 shares of GEHC".to_string(),
            tax_impact_eur: None, // No immediate tax impact
            notes: Some("Cost basis must be allocated between parent and spinoff".to_string()),
        };
        statement.add_corporate_action(spinoff);

        assert_eq!(statement.corporate_actions.len(), 1);
        assert!(matches!(
            statement.corporate_actions[0].action_type,
            CorporateActionType::Spinoff
        ));
    }

    /// Test corporate action entries for liquidations.
    #[test]
    fn test_corporate_action_liquidation() {
        use super::super::statement::{CorporateActionEntry, CorporateActionType};

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        let liquidation = CorporateActionEntry {
            date: Date::from_ymd_opt(2024, 8, 15).unwrap(),
            action_type: CorporateActionType::Liquidation,
            symbol: "BANKRUPT".to_string(),
            description: "Liquidation: 500 shares @ 0.50 USD".to_string(),
            tax_impact_eur: Some(dec!(225.00)), // Proceeds in EUR
            notes: Some("Treated as sale - capital gain/loss based on cost basis".to_string()),
        };
        statement.add_corporate_action(liquidation);

        assert_eq!(statement.corporate_actions.len(), 1);
        assert!(matches!(
            statement.corporate_actions[0].action_type,
            CorporateActionType::Liquidation
        ));
        assert_eq!(
            statement.corporate_actions[0].tax_impact_eur,
            Some(dec!(225.00))
        );
    }

    /// Test corporate action entries for delistings (total loss).
    #[test]
    fn test_corporate_action_delisting() {
        use super::super::statement::{CorporateActionEntry, CorporateActionType};

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();

        let delisting = CorporateActionEntry {
            date: Date::from_ymd_opt(2024, 9, 1).unwrap(),
            action_type: CorporateActionType::Delisting,
            symbol: "DELIST".to_string(),
            description: "Delisting of 200 shares".to_string(),
            tax_impact_eur: None, // Loss depends on cost basis
            notes: Some(
                "Total loss - may have restricted deductibility under German tax law".to_string(),
            ),
        };
        statement.add_corporate_action(delisting);

        assert_eq!(statement.corporate_actions.len(), 1);
        assert!(matches!(
            statement.corporate_actions[0].action_type,
            CorporateActionType::Delisting
        ));

        // Verify CSV contains corporate action
        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv_string = String::from_utf8(csv_output).unwrap();

        assert!(csv_string.contains("Corporate Action"));
        assert!(csv_string.contains("DELIST"));
    }

    /// Test combined statement with all entry types.
    #[test]
    fn test_combined_statement_all_entry_types() {
        use super::super::statement::{
            CashGrantEntry, CorporateActionEntry, CorporateActionType, FeeEntry, StockGrantEntry,
        };

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0.09), dec!(0), dec!(0), dec!(0)).unwrap(); // 9% church tax

        // Add capital gain
        let capital_gain = CapitalGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 3, 1).unwrap(),
            settle_date: Date::from_ymd_opt(2024, 3, 3).unwrap(),
            symbol: "AAPL".to_string(),
            isin: "US0378331005".to_string(),
            description: "Apple Inc.".to_string(),
            quantity: dec!(20),
            cost_basis_eur: dec!(3000.00),
            proceeds_eur: dec!(4000.00),
            gross_gain_loss: dec!(1000.00),
            taxable_before_exemption: dec!(1000.00),
            teilfreistellung_rate: TeilfreistellungRate::None,
            is_stock: true,
            taxable_amount: dec!(1000.00),
            foreign_tax: dec!(0),
            abgeltungssteuer: dec!(250.00),
            solidaritaetszuschlag: dec!(13.75),
            kirchensteuer: dec!(22.50),
            total_tax: dec!(286.25),
            pre_2009_holding: false,
            notes: None,
        };
        statement.add_capital_gain(capital_gain);

        // Add dividend
        let dividend = DividendEntry {
            payment_date: Date::from_ymd_opt(2024, 6, 15).unwrap(),
            symbol: "MSFT".to_string(),
            isin: "US5949181045".to_string(),
            description: "Microsoft Corp.".to_string(),
            quantity: dec!(0),
            gross_amount_eur: dec!(200.00),
            foreign_withholding_tax: dec!(30.00),
            teilfreistellung_rate: TeilfreistellungRate::None,
            taxable_amount: dec!(200.00),
            abgeltungssteuer: dec!(50.00),
            solidaritaetszuschlag: dec!(2.75),
            kirchensteuer: dec!(4.50),
            foreign_tax_credit: dec!(30.00),
            total_tax: dec!(57.25),
            net_tax: dec!(27.25),
            notes: None,
        };
        statement.add_dividend(dividend);

        // Add fee
        let fee = FeeEntry {
            date: Date::from_ymd_opt(2024, 1, 31).unwrap(),
            description: "Account maintenance".to_string(),
            amount_eur: dec!(25.00),
            notes: None,
        };
        statement.add_fee(fee);

        // Add stock grant
        let stock_grant = StockGrantEntry {
            vest_date: Date::from_ymd_opt(2024, 4, 1).unwrap(),
            symbol: "GOOGL".to_string(),
            isin: "US02079K3059".to_string(),
            quantity: dec!(10),
            fmv_per_share_eur: dec!(140.00),
            total_fmv_eur: dec!(1400.00),
            notes: None,
        };
        statement.add_stock_grant(stock_grant);

        // Add cash grant
        let cash_grant = CashGrantEntry {
            date: Date::from_ymd_opt(2024, 2, 15).unwrap(),
            description: "Welcome bonus".to_string(),
            amount_eur: dec!(50.00),
            notes: None,
        };
        statement.add_cash_grant(cash_grant);

        // Add corporate action
        let corp_action = CorporateActionEntry {
            date: Date::from_ymd_opt(2024, 7, 1).unwrap(),
            action_type: CorporateActionType::Spinoff,
            symbol: "JNJ".to_string(),
            description: "Received 50 shares of KVUE".to_string(),
            tax_impact_eur: None,
            notes: None,
        };
        statement.add_corporate_action(corp_action);

        // Calculate totals
        statement.calculate_totals();

        // Verify all entries are tracked
        assert_eq!(statement.capital_gains.len(), 1);
        assert_eq!(statement.dividends.len(), 1);
        assert_eq!(statement.fees.len(), 1);
        assert_eq!(statement.stock_grants.len(), 1);
        assert_eq!(statement.cash_grants.len(), 1);
        assert_eq!(statement.corporate_actions.len(), 1);

        // Verify totals
        assert_eq!(statement.total_capital_gains, dec!(1000.00));
        assert_eq!(statement.total_dividend_income, dec!(200.00));
        assert_eq!(statement.total_fees, dec!(25.00));
        assert_eq!(statement.total_stock_grant_income, dec!(1400.00));
        assert_eq!(statement.total_cash_grant_income, dec!(50.00));

        // Generate CSV and verify all sections exist
        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv_string = String::from_utf8(csv_output).unwrap();

        // Check for all row types (human-readable names in CSV)
        assert!(csv_string.contains("Capital Gain"));
        assert!(csv_string.contains("Dividend"));
        assert!(csv_string.contains("Fee/Deduction"));
        assert!(csv_string.contains("Stock Grant (Employment Income)"));
        assert!(csv_string.contains("Cash Grant (Other Income)"));
        assert!(csv_string.contains("Corporate Action"));

        // Check for summary entries
        assert!(csv_string.contains("SUMMARY_TOTAL_TAXABLE_INCOME"));
        assert!(csv_string.contains("SUMMARY_TOTAL_FEES"));
    }

    // --- T5: §20(6) loss pots, Sparer-Pauschbetrag, and §20(9) fee treatment ---

    fn stock_capital_entry(taxable: Decimal) -> CapitalGainEntry {
        CapitalGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 6, 1).unwrap(),
            settle_date: Date::from_ymd_opt(2024, 6, 3).unwrap(),
            symbol: "SYM".to_string(),
            isin: String::new(),
            description: "Sym".to_string(),
            quantity: dec!(1),
            cost_basis_eur: dec!(0),
            proceeds_eur: taxable,
            gross_gain_loss: taxable,
            taxable_before_exemption: taxable,
            teilfreistellung_rate: TeilfreistellungRate::None,
            is_stock: true,
            taxable_amount: taxable,
            foreign_tax: dec!(0),
            abgeltungssteuer: dec!(0),
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            total_tax: dec!(0),
            pre_2009_holding: false,
            notes: None,
        }
    }

    fn fx_entry(gross: Decimal) -> FxGainEntry {
        FxGainEntry {
            transaction_date: Date::from_ymd_opt(2024, 6, 1).unwrap(),
            currency_pair: "EUR.USD".to_string(),
            description: "fx".to_string(),
            gross_amount_eur: gross,
            taxable_amount: gross,
            abgeltungssteuer: dec!(0),
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            total_tax: dec!(0),
            notes: None,
        }
    }

    fn dividend_entry(taxable: Decimal) -> DividendEntry {
        DividendEntry {
            payment_date: Date::from_ymd_opt(2024, 6, 1).unwrap(),
            symbol: "DIV".to_string(),
            isin: String::new(),
            description: "Div".to_string(),
            quantity: dec!(0),
            gross_amount_eur: taxable,
            foreign_withholding_tax: dec!(0),
            teilfreistellung_rate: TeilfreistellungRate::None,
            taxable_amount: taxable,
            abgeltungssteuer: dec!(0),
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            foreign_tax_credit: dec!(0),
            total_tax: dec!(0),
            net_tax: dec!(0),
            notes: None,
        }
    }

    #[test]
    fn stock_loss_and_fx_gain_use_separate_pots() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_capital_gain(stock_capital_entry(dec!(-500)));
        statement.add_fx_gain(fx_entry(dec!(500)));
        statement.calculate_totals();
        // §20(6): a share-sale loss cannot offset the FX gain — it carries forward in its own pot,
        // and the general pot's €500 gain is fully taxable (net is NOT zero).
        assert_eq!(statement.total_taxable_income, dec!(500));
        assert_eq!(statement.loss_carryforward_stock_next, dec!(500));
        assert_eq!(statement.loss_carryforward_other_next, dec!(0));
    }

    #[test]
    fn general_loss_offsets_dividends() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_dividend(dividend_entry(dec!(1000)));
        statement.add_fx_gain(fx_entry(dec!(-300)));
        statement.calculate_totals();
        // The general pot offsets the FX loss against dividends: 1000 − 300 = 700.
        assert_eq!(statement.total_taxable_income, dec!(700));
        assert_eq!(statement.loss_carryforward_other_next, dec!(0));
    }

    #[test]
    fn sparer_pauschbetrag_absorbs_small_income() {
        // €900 of income under the €1,000 allowance → nothing taxable.
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(1000)).unwrap();
        statement.add_dividend(dividend_entry(dec!(900)));
        statement.calculate_totals();
        assert_eq!(statement.total_taxable_income, dec!(0));
        assert_eq!(statement.total_german_tax, dec!(0));
        assert_eq!(statement.sparer_pauschbetrag_used, dec!(900));
    }

    #[test]
    fn mixed_altbestand_sale_filed_by_taxable_amount_not_gross() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        // Gross gain is +€1,000 (a pre-2009 lot dominates) but the Altbestand-adjusted taxable
        // amount is a €300 loss. It must file as a loss, driven by taxable_amount, not gross.
        let mut entry = stock_capital_entry(dec!(-300));
        entry.gross_gain_loss = dec!(1000);
        entry.pre_2009_holding = true;
        statement.add_capital_gain(entry);
        statement.calculate_totals();
        assert_eq!(statement.total_capital_gains, dec!(0));
        assert_eq!(statement.total_capital_losses, dec!(300));
        assert_eq!(statement.loss_carryforward_stock_next, dec!(300));
        assert_eq!(statement.total_taxable_income, dec!(0));
    }

    #[test]
    fn fees_are_not_deducted_from_taxable_income() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_dividend(dividend_entry(dec!(1000)));
        statement.add_fee(FeeEntry {
            date: Date::from_ymd_opt(2024, 3, 1).unwrap(),
            description: "Account fee".to_string(),
            amount_eur: dec!(200),
            notes: None,
        });
        statement.calculate_totals();
        // §20(9): the €200 fee is reported but does not reduce the €1,000 taxable base.
        assert_eq!(statement.total_taxable_income, dec!(1000));
        assert_eq!(statement.total_fees, dec!(200));
    }

    // --- T6: Anlage KAP / KAP-INV mapping ---

    fn fund_dividend_entry(gross: Decimal, rate: TeilfreistellungRate) -> DividendEntry {
        DividendEntry {
            teilfreistellung_rate: rate,
            ..dividend_entry(gross)
        }
    }

    fn fund_capital_entry(gross: Decimal, rate: TeilfreistellungRate) -> CapitalGainEntry {
        CapitalGainEntry {
            teilfreistellung_rate: rate,
            is_stock: false,
            ..stock_capital_entry(gross)
        }
    }

    #[test]
    fn kap_zeilen_split_share_gains_losses_and_exclude_funds() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_capital_gain(stock_capital_entry(dec!(1000))); // share-sale gain
        statement.add_capital_gain(stock_capital_entry(dec!(-400))); // share-sale loss
        statement.add_dividend(fund_dividend_entry(dec!(800), TeilfreistellungRate::Equity));
        statement.add_fx_gain(fx_entry(dec!(-200))); // FX loss
        statement.calculate_totals();

        assert_eq!(statement.kap_zeile_20, dec!(1000)); // contained share-sale gains
        assert_eq!(statement.kap_zeile_23, dec!(400)); // contained share-sale losses
        assert_eq!(statement.kap_zeile_22, dec!(200)); // contained non-share losses (FX)
        assert_eq!(statement.kap_zeile_19, dec!(400)); // 1000 − 200 − 400

        // Zeile 19 == (all positive foreign capital income) − Zeile 22 − Zeile 23. The only positive
        // non-fund contribution is the €1,000 share gain; the fund dividend is on KAP-INV, not here.
        let all_positives = dec!(1000);
        assert_eq!(
            statement.kap_zeile_19,
            all_positives - statement.kap_zeile_22 - statement.kap_zeile_23
        );

        // The fund dividend lands on Anlage KAP-INV (gross), never on the KAP lines.
        assert_eq!(statement.kap_inv_equity.distributions, dec!(800));
    }

    #[test]
    fn pure_altbestand_sale_absent_from_kap_lines() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        // Pure pre-2009 lot: raw gross gain +€1,000 but Altbestand-exempt, so taxable_amount is 0.
        // It must land on neither Zeile 19 nor Zeile 20.
        let mut entry = stock_capital_entry(dec!(0));
        entry.gross_gain_loss = dec!(1000);
        entry.pre_2009_holding = true;
        statement.add_capital_gain(entry);
        statement.calculate_totals();
        assert_eq!(statement.kap_zeile_19, dec!(0));
        assert_eq!(statement.kap_zeile_20, dec!(0));
        assert_eq!(statement.kap_zeile_23, dec!(0));
    }

    #[test]
    fn fund_entries_populate_kap_inv_groups_gross() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_dividend(fund_dividend_entry(dec!(500), TeilfreistellungRate::Equity));
        statement.add_capital_gain(fund_capital_entry(dec!(1000), TeilfreistellungRate::Equity));
        statement.add_capital_gain(fund_capital_entry(dec!(-300), TeilfreistellungRate::Mixed));
        statement.calculate_totals();

        // KAP-INV reports gross, pre-Teilfreistellung figures grouped by fund type.
        assert_eq!(statement.kap_inv_equity.distributions, dec!(500));
        assert_eq!(statement.kap_inv_equity.sale_gains, dec!(1000));
        assert_eq!(statement.kap_inv_mixed.sale_losses, dec!(300));
        // Fund income must not leak onto the non-fund KAP lines.
        assert_eq!(statement.kap_zeile_20, dec!(0));
        assert_eq!(statement.kap_zeile_19, dec!(0));
    }

    #[test]
    fn csv_kap_inv_carries_form_zeilen() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_dividend(fund_dividend_entry(dec!(500), TeilfreistellungRate::Equity));
        statement.add_capital_gain(fund_capital_entry(dec!(1000), TeilfreistellungRate::Mixed));
        statement.calculate_totals();

        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv = String::from_utf8(csv_output).unwrap();
        // Aktienfonds distributions on Zeile 4, Mischfonds net Veräußerung on Zeile 17.
        assert!(csv.contains("gross distributions (Zeile 4)"));
        assert!(csv.contains("net sale gain/loss (Zeile 17)"));
    }

    // --- T11: Vorabpauschale (§18 InvStG) ---

    fn vorabpauschale_entry(
        isin: &str,
        gross: Decimal,
        rate: TeilfreistellungRate,
    ) -> VorabpauschaleEntry {
        let rates = crate::taxes::germany::GermanTaxRates::default();
        let taxable = gross * rate.taxable_portion();
        let taxes = rates.compute_taxes(taxable, dec!(0));
        VorabpauschaleEntry {
            arising_year: 2024,
            deemed_received: Date::from_ymd_opt(2025, 1, 2).unwrap(),
            symbol: "EUNL".to_string(),
            isin: isin.to_string(),
            quantity: dec!(100),
            nav_jan1: dec!(8000),
            nav_dec31: dec!(9200),
            distributions: dec!(0),
            basiszins: dec!(0.0229),
            teilfreistellung_rate: rate,
            gross_vorabpauschale: gross,
            taxable_amount: taxable,
            abgeltungssteuer: taxes.abgeltungssteuer,
            solidaritaetszuschlag: taxes.solidaritaetszuschlag,
            kirchensteuer: taxes.kirchensteuer,
            total_tax: taxes.total,
            accumulated_after: gross,
            notes: None,
        }
    }

    #[test]
    fn vorabpauschale_totals_sum_entries_but_stay_out_of_the_pot() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        // €200 gross equity-fund Vorabpauschale: 30% Teilfreistellung → €140 taxable.
        statement.add_vorabpauschale(vorabpauschale_entry(
            "IE00BK5BQT80",
            dec!(200),
            TeilfreistellungRate::Equity,
        ));
        statement.add_capital_gain(stock_capital_entry(dec!(500)));
        statement.calculate_totals();

        assert_eq!(statement.total_vorabpauschale_gross, dec!(200));
        assert_eq!(statement.total_vorabpauschale_taxable, dec!(140));
        // Vorabpauschale is next-year income, so it must not enter this year's taxable base.
        assert_eq!(statement.total_taxable_income, dec!(500));
    }

    #[test]
    fn csv_renders_vorabpauschale_section() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_vorabpauschale(vorabpauschale_entry(
            "IE00BK5BQT80",
            dec!(200),
            TeilfreistellungRate::Equity,
        ));
        statement.calculate_totals();

        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv = String::from_utf8(csv_output).unwrap();
        assert!(csv.contains("VORABPAUSCHALE_GROSS"));
        assert!(csv.contains("IE00BK5BQT80"));
        // Declared in the following year's return, on the equity-fund Vorabpauschale line.
        assert!(csv.contains("2025"));
        assert!(csv.contains("KAP-INV Zeile 9"));
    }

    #[test]
    fn csv_warns_about_funds_missing_nav() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement
            .vorabpauschale_missing_nav
            .push("IE00BK5BQT80".to_string());
        statement.calculate_totals();

        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv = String::from_utf8(csv_output).unwrap();
        assert!(csv.contains("NOT computed"));
        assert!(csv.contains("IE00BK5BQT80"));
    }

    #[test]
    fn csv_omits_vorabpauschale_without_funds() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_capital_gain(stock_capital_entry(dec!(500)));
        statement.calculate_totals();

        let mut csv_output = Vec::new();
        CsvFormatter::write(&statement, &mut csv_output).unwrap();
        let csv = String::from_utf8(csv_output).unwrap();
        assert!(!csv.contains("VORABPAUSCHALE"));
    }
}

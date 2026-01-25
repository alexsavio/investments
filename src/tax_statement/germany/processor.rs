//! German tax statement processor.
//!
//! Processes broker statement data and populates the German tax statement
//! with capital gains, dividends, and interest entries.

use chrono::Datelike;
use log::{debug, warn};

use crate::broker_statement::{BrokerStatement, StockSource};
use crate::core::GenericResult;
use crate::currency::Cash;
use crate::currency::converter::CurrencyConverter;
use crate::taxes::TaxConfig;
use crate::taxes::germany::TeilfreistellungRate;
use crate::time::Date;
use crate::types::Decimal;

use super::statement::{CapitalGainEntry, DividendEntry, GermanTaxStatement, InterestEntry};

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

/// Check if a symbol appears to be a derivative instrument.
/// Derivatives have different tax treatment and are not supported.
fn is_derivative(symbol: &str) -> bool {
    let symbol_upper = symbol.to_uppercase();

    // Common derivative patterns:
    // - Options often have strike prices and expiry dates encoded
    // - Options on US exchanges often end with digits (strike) and letters (month code)
    // - Futures have month codes like F, G, H, J, K, M, N, Q, U, V, X, Z followed by year
    // - Warrants often end with 'W' or contain 'WS', 'WT'

    // Check for warrant patterns
    if symbol_upper.ends_with('W')
        || symbol_upper.contains("WS")
        || symbol_upper.contains("WT")
        || symbol_upper.contains("WARRANT")
    {
        return true;
    }

    // Check for option-like patterns (symbol followed by date codes)
    // e.g., AAPL230120C00150000 (AAPL Jan 20 2023 Call $150)
    if symbol.len() > 6 {
        let chars: Vec<char> = symbol.chars().collect();
        // If we have many digits after the ticker, might be an option
        let digit_count = chars.iter().filter(|c| c.is_ascii_digit()).count();
        if digit_count > 6 {
            return true;
        }
    }

    // Check for structured product / certificate indicators
    if symbol_upper.contains("CERT")
        || symbol_upper.contains("NOTE")
        || symbol_upper.contains("STRUC")
    {
        return true;
    }

    false
}

/// Emit warning for derivative instrument and return true if it's a derivative.
fn warn_if_derivative(symbol: &str) -> bool {
    if is_derivative(symbol) {
        warn!(
            "Derivative instrument '{}' detected - skipping. German tax treatment for \
             derivatives differs from stocks and requires specialized handling.",
            symbol
        );
        true
    } else {
        false
    }
}

/// Process broker statement and populate German tax statement entries.
pub fn process_broker_statement(
    statement: &mut GermanTaxStatement,
    broker_statement: &BrokerStatement,
    year: i32,
    converter: &CurrencyConverter,
    tax_config: &TaxConfig,
) -> GenericResult<(bool, bool, bool)> {
    debug!("Processing German tax statement for year {}", year);

    let has_trades = process_trades(statement, broker_statement, year, converter, tax_config)?;
    let has_dividends =
        process_dividends(statement, broker_statement, year, converter, tax_config)?;
    let has_interest = process_interest(statement, broker_statement, year, converter)?;

    debug!(
        "German tax statement processing complete: {} trades, {} dividends, {} interest entries",
        statement.capital_gains.len(),
        statement.dividends.len(),
        statement.interest.len()
    );

    Ok((has_trades, has_dividends, has_interest))
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

    for trade in &broker_statement.stock_sells {
        let trade_year = trade.execution_date.year();
        if trade_year != year {
            continue;
        }

        // Skip derivatives with warning (FR-016)
        if warn_if_derivative(&trade.symbol) {
            continue;
        }

        has_income = true;

        // Get trade details
        let (_price, volume, commission) = match &trade.type_ {
            crate::broker_statement::StockSellType::Trade {
                price,
                volume,
                commission,
                ..
            } => (*price, *volume, *commission),
            crate::broker_statement::StockSellType::CorporateAction => continue, // Skip corporate actions
        };

        // Convert amounts to EUR with helpful error messages
        let context = format!(
            "Processing sale of {} on {}",
            trade.symbol, trade.execution_date
        );
        let proceeds_eur = convert_to_eur(converter, trade.execution_date, volume, &context)?;
        let commission_eur = convert_to_eur(converter, trade.execution_date, commission, &context)?;

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

        // Calculate cost basis from FIFO lots
        // Note: The broker statement should have already processed FIFO matching
        let cost_basis_eur = calculate_cost_basis(trade, broker_statement, converter)?;

        // Calculate gross gain/loss
        let gross_gain_loss = proceeds_eur - commission_eur - cost_basis_eur;

        // Apply Teilfreistellung
        let taxable_amount = apply_teilfreistellung(gross_gain_loss, &teilfreistellung_rate);

        // Check for pre-2009 holdings (Altbestand)
        let pre_2009_holding = check_pre_2009_holding(trade, broker_statement);

        // Calculate German taxes
        let (abgeltungssteuer, soli, church_tax, total_tax) =
            if pre_2009_holding || taxable_amount <= dec!(0) {
                (dec!(0), dec!(0), dec!(0), dec!(0))
            } else {
                let rates = &statement.tax_rates;
                let abgelt = taxable_amount * rates.abgeltungssteuer;
                let soli = abgelt * rates.solidaritaetszuschlag;
                let church = abgelt * rates.kirchensteuer;
                (abgelt, soli, church, abgelt + soli + church)
            };

        let entry = CapitalGainEntry {
            transaction_date: trade.conclusion_time.date,
            settle_date: trade.execution_date,
            symbol: trade.symbol.clone(),
            isin,
            description,
            quantity: trade.quantity,
            cost_basis_eur,
            proceeds_eur: proceeds_eur - commission_eur,
            gross_gain_loss,
            teilfreistellung_rate,
            taxable_amount,
            foreign_tax: dec!(0), // Foreign withholding on capital gains is rare
            abgeltungssteuer,
            solidaritaetszuschlag: soli,
            kirchensteuer: church_tax,
            total_tax,
            pre_2009_holding,
            notes: if pre_2009_holding {
                Some("Altbestand (pre-2009) - tax exempt".to_string())
            } else {
                None
            },
        };

        debug!(
            "FIFO capital gain: {} {} shares - cost: €{:.2}, proceeds: €{:.2}, gain: €{:.2}, tax: €{:.2}",
            trade.symbol,
            trade.quantity,
            cost_basis_eur,
            proceeds_eur - commission_eur,
            gross_gain_loss,
            total_tax
        );

        statement.add_capital_gain(entry);
    }

    Ok(has_income)
}

/// Calculate cost basis for a trade using FIFO data from broker statement.
fn calculate_cost_basis(
    trade: &crate::broker_statement::StockSell,
    broker_statement: &BrokerStatement,
    converter: &CurrencyConverter,
) -> GenericResult<Decimal> {
    // Find matching buy transactions using FIFO
    // For now, use a simplified approach - the actual FIFO matching is complex
    // and already done by the broker statement processing

    // Look through stock_buys to find matching purchases
    let mut total_cost = dec!(0);
    let mut remaining_qty = trade.quantity;

    // Sort buys by date for FIFO
    let mut buys: Vec<_> = broker_statement
        .stock_buys
        .iter()
        .filter(|buy| {
            buy.symbol == trade.symbol && buy.conclusion_time.date <= trade.conclusion_time.date
        })
        .collect();
    buys.sort_by_key(|buy| buy.conclusion_time.date);

    for buy in buys {
        if remaining_qty <= dec!(0) {
            break;
        }

        let buy_qty = buy.quantity.min(remaining_qty);
        let (price_per_share, currency) = match &buy.type_ {
            StockSource::Trade { price, .. } => (price.amount, &price.currency),
            StockSource::Grant => continue,
            _ => continue,
        };

        let cost = buy_qty * price_per_share;
        let cost_cash = crate::currency::Cash::new(currency, cost);
        let context = format!(
            "Calculating cost basis for {} purchase on {}",
            buy.symbol, buy.conclusion_time.date
        );
        let cost_eur = convert_to_eur(converter, buy.conclusion_time.date, cost_cash, &context)?;

        total_cost += cost_eur;
        remaining_qty -= buy_qty;
    }

    Ok(total_cost)
}

/// Check if the trade involves pre-2009 holdings (Altbestand).
fn check_pre_2009_holding(
    trade: &crate::broker_statement::StockSell,
    broker_statement: &BrokerStatement,
) -> bool {
    let cutoff_date = Date::from_ymd_opt(2009, 1, 1).unwrap();

    // Check if any of the source buys are from before 2009
    for buy in &broker_statement.stock_buys {
        if buy.symbol == trade.symbol && buy.conclusion_time.date < cutoff_date {
            return true;
        }
    }

    false
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

        // Skip derivatives with warning (FR-016)
        if warn_if_derivative(&dividend.issuer) {
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

        // Calculate German taxes
        let rates = &statement.tax_rates;
        let abgeltungssteuer = taxable_amount * rates.abgeltungssteuer;
        let soli = abgeltungssteuer * rates.solidaritaetszuschlag;
        let church_tax = abgeltungssteuer * rates.kirchensteuer;
        let total_tax = abgeltungssteuer + soli + church_tax;

        // Foreign tax credit (limited to German tax rate)
        let max_credit = taxable_amount * dec!(0.15); // 15% typical DTA limit
        let foreign_tax_credit = foreign_withholding_tax.min(max_credit);

        let net_tax = (total_tax - foreign_tax_credit).max(dec!(0));

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

        // Calculate German taxes
        let rates = &statement.tax_rates;
        let abgeltungssteuer = taxable_amount * rates.abgeltungssteuer;
        let soli = abgeltungssteuer * rates.solidaritaetszuschlag;
        let church_tax = abgeltungssteuer * rates.kirchensteuer;
        let total_tax = abgeltungssteuer + soli + church_tax;

        // No foreign tax credit for interest (typically)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tax_statement::germany::{GermanCsvFormatter, GermanTaxStatement};
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
        let mut statement = GermanTaxStatement::new(2024, dec!(0.08), dec!(0)); // 8% church tax

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
            teilfreistellung_rate: TeilfreistellungRate::None,
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
            teilfreistellung_rate: TeilfreistellungRate::None,
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
        GermanCsvFormatter::write(&statement, &mut csv_output).unwrap();
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
        let mut statement = GermanTaxStatement::new(2024, dec!(0), dec!(0));

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
            teilfreistellung_rate: TeilfreistellungRate::None,
            taxable_amount: dec!(4000.00),
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
        let mut statement = GermanTaxStatement::new(2024, dec!(0), dec!(0));

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

    /// Test derivative detection (FR-016).
    #[test]
    fn test_derivative_detection() {
        // Regular stocks should NOT be detected as derivatives
        assert!(!is_derivative("AAPL"));
        assert!(!is_derivative("VTI"));
        assert!(!is_derivative("MSFT"));
        assert!(!is_derivative("BRK.B"));
        assert!(!is_derivative("VWCE"));
        assert!(!is_derivative("IE00BK5BQT80")); // ISIN

        // Warrants should be detected
        assert!(is_derivative("AAPLW")); // Warrant suffix
        assert!(is_derivative("MSFTWS")); // WS pattern
        assert!(is_derivative("TESTWARRANT")); // Contains WARRANT

        // Options should be detected (long symbols with many digits)
        assert!(is_derivative("AAPL230120C00150000")); // Call option
        assert!(is_derivative("AAPL230120P00150000")); // Put option

        // Structured products should be detected
        assert!(is_derivative("TESTCERT")); // Certificate
        assert!(is_derivative("TESTNOTE")); // Structured note
    }
}

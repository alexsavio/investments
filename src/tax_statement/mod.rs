mod dividends;
pub mod germany;
mod interest;
mod statement;
mod tax_agent;
mod trades;

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use ansi_term::Color;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::config::Config;
use crate::core::GenericResult;
use crate::currency::converter::CurrencyConverter;
use crate::db;
use crate::localities::Jurisdiction;
use crate::taxes::TaxCalculator;
use crate::telemetry::TelemetryRecordBuilder;
use crate::types::Decimal;

pub use self::statement::TaxStatement;

pub fn generate_tax_statement(
    config: &Config,
    portfolio_name: &str,
    year: Option<i32>,
    tax_statement_path: Option<&Path>,
) -> GenericResult<TelemetryRecordBuilder> {
    let country = config.get_tax_country();
    let portfolio = config.get_portfolio(portfolio_name)?;

    // Route to Germany-specific implementation if jurisdiction is Germany
    if country.jurisdiction == Jurisdiction::Germany {
        return generate_german_tax_statement(config, portfolio_name, year, tax_statement_path);
    }

    let broker_statement = BrokerStatement::load(config, portfolio,
        ReadingStrictness::TRADE_SETTLE_DATE | ReadingStrictness::OTC_INSTRUMENTS | ReadingStrictness::TAX_EXEMPTIONS |
        ReadingStrictness::REPO_TRADES | ReadingStrictness::GRANTS)?;

    if let Some(year) = year {
        broker_statement.check_period_against_tax_year(year)?;
    }

    let mut tax_statement = match tax_statement_path {
        Some(path) => {
            let year = year.ok_or("Tax year must be specified when tax statement is specified")?;

            let statement = TaxStatement::read(path, Some(year))
                .map_err(|e| format!("Error while reading tax statement {path:?}: {e}"))?;

            if statement.year != year {
                return Err!(
                    "Tax statement year ({}) doesn't match the requested year {year}",
                    statement.year
                );
            }

            Some(statement)
        }
        None => None,
    };

    let database = db::connect(&config.db_path)?;
    let converter = CurrencyConverter::new(database, None, true);
    let mut tax_calculator = TaxCalculator::new(country.clone());

    let (trades_tax, has_trading_income, has_trading_income_to_declare) = trades::process_income(
        &country,
        portfolio,
        &broker_statement,
        year,
        &mut tax_calculator,
        tax_statement.as_mut(),
        &converter,
    )
    .map_err(|e| format!("Failed to process income from stock trading: {e}"))?;

    let (dividends_tax, has_dividend_income, has_dividend_income_to_declare) =
        dividends::process_income(
            &country,
            &broker_statement,
            year,
            &mut tax_calculator,
            tax_statement.as_mut(),
            &converter,
        )
        .map_err(|e| format!("Failed to process dividend income: {e}"))?;

    let (interest_tax, has_interest_income, has_interest_income_to_declare) =
        interest::process_income(
            &country,
            &broker_statement,
            year,
            &mut tax_calculator,
            tax_statement.as_mut(),
            &converter,
        )
        .map_err(|e| format!("Failed to process income from idle cash interest: {e}"))?;

    let has_income = has_trading_income | has_dividend_income | has_interest_income;
    let has_income_to_declare = has_trading_income_to_declare
        | has_dividend_income_to_declare
        | has_interest_income_to_declare;

    if broker_statement.broker.type_.jurisdiction() == Jurisdiction::Russia {
        let total_tax = trades_tax + dividends_tax + interest_tax;
        tax_agent::process_tax_agent_withholdings(&broker_statement, year, has_income, total_tax)?;
    }

    if let Some(ref tax_statement) = tax_statement {
        assert_eq!(tax_statement.new_income_added, has_income_to_declare);

        if has_income_to_declare {
            tax_statement.save()?;
            println!(
                "{}",
                Color::Green.paint("The income has been added to the tax statement.")
            );
        }
    } else if has_income_to_declare {
        println!(
            "{}",
            Color::Yellow.paint("The income must be declared to tax inspection.")
        );
    }

    if !has_income_to_declare {
        println!(
            "{}",
            Color::Green.paint("There is no any income to declare.")
        );
    }

    Ok(TelemetryRecordBuilder::new_with_broker(portfolio.broker))
}

/// Generate a German tax statement in CSV format.
fn generate_german_tax_statement(
    config: &Config,
    portfolio_name: &str,
    year: Option<i32>,
    output_path: Option<&Path>,
) -> GenericResult<TelemetryRecordBuilder> {
    let year = year.ok_or("Tax year must be specified for German tax statement")?;
    let portfolio = config.get_portfolio(portfolio_name)?;
    let broker = portfolio
        .broker
        .get_info(config, portfolio.plan.as_deref())?;

    let broker_statement = BrokerStatement::read(
        broker,
        portfolio.statements_path()?,
        &portfolio.symbol_remapping,
        &portfolio.instrument_internal_ids,
        &portfolio.instrument_names,
        portfolio.get_tax_remapping()?,
        &portfolio.tax_exemptions,
        &portfolio.corporate_actions,
        ReadingStrictness::TRADE_SETTLE_DATE
            | ReadingStrictness::OTC_INSTRUMENTS
            | ReadingStrictness::TAX_EXEMPTIONS
            | ReadingStrictness::REPO_TRADES
            | ReadingStrictness::GRANTS,
    )?;

    broker_statement.check_period_against_tax_year(year)?;

    // Get German tax configuration
    let church_tax_rate = config.taxes.church_tax_rate.unwrap_or(Decimal::ZERO);
    let loss_carryforward = config
        .taxes
        .loss_carryforward
        .values()
        .copied()
        .sum::<Decimal>();

    // Create the German tax statement
    let mut statement = germany::GermanTaxStatement::new(year, church_tax_rate, loss_carryforward);

    // Connect to database for currency conversion
    let database = db::connect(&config.db_path)?;
    let converter = CurrencyConverter::new(database, None, true);

    // Process broker statement and populate entries
    let (has_trades, has_dividends, has_interest) =
        germany::process_broker_statement(&mut statement, &broker_statement, year, &converter)?;

    let has_income = has_trades || has_dividends || has_interest;

    // Calculate totals
    statement.calculate_totals();

    // Output the statement
    if let Some(path) = output_path {
        let file = File::create(path)
            .map_err(|e| format!("Failed to create output file {path:?}: {e}"))?;
        let mut writer = BufWriter::new(file);

        germany::GermanCsvFormatter::write(&statement, &mut writer)?;

        println!(
            "{}",
            Color::Green.paint(format!("German tax statement written to {path:?}"))
        );

        // Print summary
        println!("\n{}", Color::Cyan.paint("=== Tax Statement Summary ==="));
        println!("Year: {}", year);
        println!("Capital gains entries: {}", statement.capital_gains.len());
        println!("Dividend entries: {}", statement.dividends.len());
        println!("Interest entries: {}", statement.interest.len());
        println!(
            "Total taxable income: €{:.2}",
            statement.total_taxable_income
        );
        println!("Total German tax: €{:.2}", statement.total_german_tax);
        if statement.total_foreign_tax > Decimal::ZERO {
            println!(
                "Foreign tax credit: €{:.2}",
                statement.total_foreign_tax_credit
            );
        }
        println!("Net tax due: €{:.2}", statement.net_tax_due);
    } else if has_income {
        // No output file specified, just print summary
        println!(
            "{}",
            Color::Yellow
                .paint("Income found but no output file specified. Use --output to save CSV.")
        );
        println!("\nCapital gains: {} entries", statement.capital_gains.len());
        println!("Dividends: {} entries", statement.dividends.len());
        println!("Interest: {} entries", statement.interest.len());
    } else {
        println!(
            "{}",
            Color::Green.paint("There is no income to declare for German taxes.")
        );
    }

    Ok(TelemetryRecordBuilder::new_with_broker(portfolio.broker))
}

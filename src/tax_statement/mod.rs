mod dividends;
mod eur;
mod fx_fifo;
pub mod germany;
mod interest;
pub mod spain;
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
use crate::taxes::spain::carryforward::CARRYFORWARD_YEARS;
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

    if country.jurisdiction == Jurisdiction::Spain {
        return generate_spanish_tax_statement(config, portfolio_name, year, tax_statement_path);
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
    let converter = CurrencyConverter::for_jurisdiction(
        country.jurisdiction, database, None, true);
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

/// Generate a Spanish tax statement.
fn generate_spanish_tax_statement(
    config: &Config,
    portfolio_name: &str,
    year: Option<i32>,
    output_path: Option<&Path>,
) -> GenericResult<TelemetryRecordBuilder> {
    let year = year.ok_or("Tax year must be specified for the Spanish tax statement")?;
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

    // Spanish tax authorities accept ECB reference rates, so the converter comes from the
    // jurisdiction rather than from the command being run.
    let database = db::connect(&config.db_path)?;
    let converter = CurrencyConverter::for_jurisdiction(
        config.get_tax_country().jurisdiction, database, None, true);

    // The same year-level computation the sell simulation prices its disposals against, so a
    // simulated trim and the filed statement can never disagree about the base or the scale.
    let (statement, has_income) = spain::compute_tax_year(
        &broker_statement,
        year,
        &converter,
        &config.taxes,
    )?;

    if !has_income {
        println!(
            "{}",
            Color::Green.paint("There is no income to declare for Spanish taxes.")
        );
        return Ok(TelemetryRecordBuilder::new_with_broker(portfolio.broker));
    }

    if let Some(path) = output_path {
        let file = File::create(path)
            .map_err(|e| format!("Failed to create output file {path:?}: {e}"))?;
        let mut writer = BufWriter::new(file);

        spain::CsvFormatter::write(&statement, &mut writer)?;

        println!(
            "{}",
            Color::Green.paint(format!("Spanish tax statement written to {path:?}"))
        );
    }

    println!("\n{}", Color::Cyan.paint("=== Spanish Tax Statement Summary ==="));
    println!("Year: {year}");
    println!("Regime: {}", statement.regime.description());
    println!(
        "Ganancias y pérdidas patrimoniales entries: {}",
        statement.capital_gains.len()
    );
    println!(
        "Net ganancias y pérdidas: €{}",
        eur::format_eur(statement.gyp_net)
    );
    println!(
        "Base liquidable del ahorro: €{}",
        eur::format_eur(statement.savings_base)
    );
    println!(
        "Cuota íntegra del ahorro: €{}",
        eur::format_eur(statement.savings_quota)
    );
    if statement.total_foreign_withholding > Decimal::ZERO {
        println!(
            "Foreign withholding: €{} → creditable €{}",
            eur::format_eur(statement.total_foreign_withholding),
            eur::format_eur(statement.total_foreign_tax_credit)
        );
    }
    println!(
        "Cuota líquida del ahorro: €{}",
        eur::format_eur(statement.net_tax_due)
    );

    let has_ledgers = !statement.rcm_ledger_next.is_empty() || !statement.gyp_ledger_next.is_empty();
    if has_ledgers || !statement.deferred_losses_next.is_empty() {
        println!("\n{}", Color::Cyan.paint("=== Next year's taxes.spain config ==="));
    }

    if has_ledgers {
        println!("    loss_carryforward:");
        for (group, ledger) in [
            ("rcm", &statement.rcm_ledger_next),
            ("gyp", &statement.gyp_ledger_next),
        ] {
            if ledger.is_empty() {
                continue;
            }
            let entries = ledger
                .balances()
                .iter()
                .map(|(origin, amount)| format!("{origin}: '{}'", eur::format_eur(*amount)))
                .collect::<Vec<_>>()
                .join(", ");
            println!("      {group}: {{{entries}}}");
        }
    }

    if !statement.deferred_losses_next.is_empty() {
        println!("    deferred_losses:");
        for deferred in &statement.deferred_losses_next {
            let isin = deferred
                .isin
                .as_deref()
                .map(|isin| format!(", isin: {isin}"))
                .unwrap_or_default();
            println!(
                "      - {{symbol: {}{isin}, loss: '{}', blocked_quantity: {}, \
                 acquisition_date: {}, sale_date: {}}}",
                deferred.symbol,
                eur::format_eur(deferred.loss),
                deferred.blocked_quantity.normalize(),
                deferred.acquisition_date.format("%Y-%m-%d"),
                deferred.sale_date.format("%Y-%m-%d")
            );
        }
    }

    for (group, expired) in [
        ("RCM", statement.rcm_expired),
        ("ganancias", statement.gyp_expired),
    ] {
        if expired > Decimal::ZERO {
            println!(
                "{}",
                Color::Yellow.paint(format!(
                    "€{} of pending {group} losses expired unused: a negative savings-base \
                     balance may be offset only in the {} following years.",
                    eur::format_eur(expired),
                    CARRYFORWARD_YEARS
                ))
            );
        }
    }

    if statement.total_deferred_loss > Decimal::ZERO
        || statement.total_reintegrated_loss > Decimal::ZERO
    {
        println!(
            "Valores homogéneos: €{} deferred, €{} reintegrated",
            eur::format_eur(statement.total_deferred_loss),
            eur::format_eur(statement.total_reintegrated_loss)
        );
    }

    for gap in &statement.wash_sale_window_gaps {
        println!(
            "{}",
            Color::Yellow.paint(format!(
                "WARNING: the valores-homogéneos window for the {} loss of {} stays open until {}, \
                 past the statement's end. A repurchase in that period would defer €{} of the loss, \
                 which is deducted in full above. Re-run once the statement covers the window.",
                gap.symbol,
                gap.sale_date,
                gap.window_end,
                eur::format_eur(gap.loss_eur)
            ))
        );
    }

    for review in &statement.wash_sale_venue_reviews {
        println!(
            "{}",
            Color::Yellow.paint(format!(
                "WARNING: the {} loss of {} is deducted in full, but that turns on which \
                 valores-homogéneos window {} takes: homogeneous securities were bought back inside \
                 the year and outside the two months, so the one-year limb (NF 3/2014 art. 43.h / \
                 LIRPF art. 33.5.g) would defer €{} of it. DGT V0778-25 and V0951-25 settle the \
                 two-month limb only for venues covered by an in-force MiFID II equivalence \
                 decision. See the open-interpretations register in docs/spain-taxes.md.",
                review.symbol,
                review.sale_date,
                review.venue.as_deref().unwrap_or("an unnamed listing venue"),
                eur::format_eur(review.loss_eur)
            ))
        );
    }

    if !statement.wash_sale_unpriced_years.is_empty() {
        println!(
            "\n{}",
            Color::Yellow.paint(format!(
                "WARNING: sales in {} were not tested for the valores-homogéneos rule — no \
                 actualization table is shipped for those disposal years, so their result could \
                 not be priced. Set taxes.spain.coefficients.<year>, or carry the deferral in \
                 taxes.spain.deferred_losses from that year's return.",
                statement
                    .wash_sale_unpriced_years
                    .iter()
                    .map(i32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        );
    }

    if !statement.stock_grants.is_empty() {
        println!(
            "\n{}",
            Color::Yellow.paint(format!(
                "{} stock vest(s) are reported for information only: employment income belongs to \
                 the GENERAL base, which this tool does not compute. Declare them separately.",
                statement.stock_grants.len()
            ))
        );
    }

    if !statement.corporate_actions.is_empty() {
        println!(
            "Corporate actions reported for review: {}",
            statement
                .corporate_actions
                .iter()
                .map(|action| format!("{} — {}", action.symbol, action.description))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }

    if statement.total_dividend_exemption > Decimal::ZERO {
        println!(
            "{}",
            Color::Yellow.paint(format!(
                "€{} of dividends were exempted under NF 3/2014 art. 9.24 (limit €1,500/year). The \
                 exemption does NOT cover distributions from instituciones de inversión colectiva \
                 (funds, ETFs, SICAVs), which a broker statement does not distinguish — check each \
                 payer and reduce it by hand if any of them is a fund.",
                eur::format_eur(statement.total_dividend_exemption)
            ))
        );
    }

    if statement.total_paid_interest > Decimal::ZERO {
        println!(
            "{}",
            Color::Yellow.paint(format!(
                "€{} of broker interest paid on a borrowed (margin) balance is reported but NOT \
                 deducted: neither LIRPF art. 26.1.a nor NF 3/2014 art. 39 allows a financing cost \
                 against rendimientos del capital mobiliario.",
                eur::format_eur(statement.total_paid_interest)
            ))
        );
    }

    if statement.total_fx_borrowed_review != Decimal::ZERO {
        println!(
            "{}",
            Color::Yellow.paint(format!(
                "€{} of foreign-currency results were realized on a borrowed (margin) balance and \
                 are excluded from the savings base pending manual review.",
                eur::format_eur(statement.total_fx_borrowed_review)
            ))
        );
    }

    if !statement.short_positions.is_empty() {
        println!(
            "\n{}",
            Color::Yellow.paint(format!(
                "Short positions are not tax-computed and need manual review: {}",
                statement
                    .short_positions
                    .iter()
                    .map(|(symbol, quantity)| format!("{symbol}: {quantity}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
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
    let broker_statement = BrokerStatement::load(config, portfolio,
        ReadingStrictness::TRADE_SETTLE_DATE | ReadingStrictness::OTC_INSTRUMENTS | ReadingStrictness::TAX_EXEMPTIONS |
        ReadingStrictness::REPO_TRADES | ReadingStrictness::GRANTS)?;

    broker_statement.check_period_against_tax_year(year)?;

    // Connect to database for currency conversion using official ECB reference rates (required by
    // German tax authorities), not the Central Bank of Russia rates used for other jurisdictions.
    let database = db::connect(&config.db_path)?;
    let converter = CurrencyConverter::for_jurisdiction(
        config.get_tax_country().jurisdiction, database, None, true);

    // The same year-level computation the sell simulation prices its disposals against, so a
    // simulated trim and the filed statement can never disagree about the pots or the allowance.
    let (statement, has_income) = germany::compute_tax_year(
        &broker_statement,
        year,
        &converter,
        &config.taxes,
        portfolio.foreign_currency_taxation,
        &portfolio.opening_foreign_currency,
    )?;

    // Output the statement
    if let Some(path) = output_path {
        let file = File::create(path)
            .map_err(|e| format!("Failed to create output file {path:?}: {e}"))?;
        let mut writer = BufWriter::new(file);

        germany::CsvFormatter::write(&statement, &mut writer)?;

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
        println!("FX gain/loss entries: {}", statement.fx_gains.len());
        println!(
            "Total taxable income: €{}",
            germany::format_eur(statement.total_taxable_income)
        );
        if statement.total_fx_gains > Decimal::ZERO || statement.total_fx_losses > Decimal::ZERO {
            println!(
                "FX gains: €{}, FX losses: €{}",
                germany::format_eur(statement.total_fx_gains),
                germany::format_eur(statement.total_fx_losses)
            );
        }
        println!(
            "Total German tax: €{}",
            germany::format_eur(statement.total_german_tax)
        );
        if statement.total_foreign_tax > Decimal::ZERO {
            println!(
                "Foreign tax credit: €{}",
                germany::format_eur(statement.total_foreign_tax_credit)
            );
        }
        println!(
            "Net tax due: €{}",
            germany::format_eur(statement.net_tax_due)
        );

        // Print Anlage KAP form values
        println!("\n{}", Color::Cyan.paint("=== Anlage KAP Form Values ==="));
        println!(
            "KAP Zeile 19 (Ausländische Kapitalerträge): €{}",
            germany::format_eur(statement.kap_zeile_19)
        );
        if statement.kap_zeile_22 > Decimal::ZERO {
            println!(
                "KAP Zeile 22 (Sonstige Verluste): €{}",
                germany::format_eur(statement.kap_zeile_22)
            );
        }
        if statement.kap_zeile_23 > Decimal::ZERO {
            println!(
                "KAP Zeile 23 (Aktien-Verluste): €{}",
                germany::format_eur(statement.kap_zeile_23)
            );
        }
        if statement.kap_zeile_41 > Decimal::ZERO {
            println!(
                "KAP Zeile 41 (Anrechenbare ausländische Steuer): €{}",
                germany::format_eur(statement.kap_zeile_41)
            );
        }

        // Show non-taxable amounts if present
        if statement.non_taxable_margin_fx != Decimal::ZERO {
            println!("\n{}", Color::Yellow.paint("=== Non-Taxable (Nicht steuerbar) ==="));
            println!(
                "Tilgung Fremdwährungskredit (Margin Loan FX): €{}",
                germany::format_eur(statement.non_taxable_margin_fx)
            );
        }

        // Anlage SO (§23 EStG): private Veräußerungsgeschäfte from a non-interest-bearing
        // foreign-currency account. Reported for manual declaration — no tax is computed here.
        let section23 = &statement.section23;
        if !section23.is_empty() {
            println!("\n{}", Color::Cyan.paint("=== Anlage SO (§23 EStG) Form Values ==="));
            println!(
                "Zeile 41-47 (Veräußerungsgeschäfte, gehalten ≤ 1 Jahr): Gewinn €{} / Verlust €{} → netto €{}",
                germany::format_eur(section23.short_term_gains),
                germany::format_eur(section23.short_term_losses),
                germany::format_eur(section23.short_term_net())
            );
            let freigrenze = germany::Section23::freigrenze(year);
            println!(
                "Freigrenze {year}: €{} (Gesamtgewinn aus allen privaten Veräußerungen ≤ Freigrenze → steuerfrei; sonst voll steuerpflichtig)",
                germany::format_eur(freigrenze)
            );
            if section23.long_term_tax_free != Decimal::ZERO {
                println!(
                    "Steuerfrei (> 1 Jahr gehalten, Spekulationsfrist erfüllt): €{}",
                    germany::format_eur(section23.long_term_tax_free)
                );
            }
            if section23.borrowed_review != Decimal::ZERO {
                println!(
                    "{}",
                    Color::Yellow.paint(format!(
                        "Prüfen: Fremdwährungskredit-Realisierung €{} — die §20-Ausnahme (BMF 19.05.2022 Rz. 131) gilt nicht für §23; ggf. als privates Veräußerungsgeschäft anzusetzen.",
                        germany::format_eur(section23.borrowed_review)
                    ))
                );
            }
        }
    } else if has_income {
        // No output file specified, just print summary
        println!(
            "{}",
            Color::Yellow
                .paint("Income found but no output file specified. Pass a file path after the \
                        year to save the CSV.")
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

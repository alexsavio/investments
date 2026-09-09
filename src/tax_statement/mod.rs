mod dividends;
mod eur;
mod fx_fifo;
pub mod germany;
#[cfg(test)]
mod golden;
mod html;
mod interest;
mod report;
pub mod spain;
mod statement;
mod tax_agent;
mod trades;

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use ansi_term::Color;

use crate::broker_statement::{BrokerStatement, ReadingStrictness};
use crate::config::Config;
use crate::core::{EmptyResult, GenericError, GenericResult};
use crate::currency::converter::CurrencyConverter;
use crate::db;
use crate::localities::Jurisdiction;
use crate::taxes::TaxCalculator;
use crate::taxes::spain::carryforward::CARRYFORWARD_YEARS;
use crate::telemetry::TelemetryRecordBuilder;
use crate::time;
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

    let broker_statement = BrokerStatement::load(config, portfolio,
        ReadingStrictness::TRADE_SETTLE_DATE | ReadingStrictness::OTC_INSTRUMENTS | ReadingStrictness::TAX_EXEMPTIONS |
        ReadingStrictness::REPO_TRADES | ReadingStrictness::GRANTS)?;

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

    // Output the statement. The extension selects the format: `.html` is the printable A4 report,
    // anything else the CSV.
    if let Some(path) = output_path {
        let format = OutputFormat::from_path(path);
        write_atomically(path, |writer| match format {
            OutputFormat::Csv => spain::CsvFormatter::write(&statement, writer),
            OutputFormat::Html => {
                let meta = spain::ReportMeta {
                    year,
                    broker_name: broker_statement.broker.name.to_owned(),
                    portfolio_name: portfolio_name.to_owned(),
                    account_id: broker_statement.account_id.clone(),
                    period: broker_statement.period,
                    generated_at: time::now(),
                };
                spain::HtmlReport::write(&statement, &meta, writer)
            }
        })?;

        println!(
            "{}",
            Color::Green.paint(format!("Spanish tax {} written to {path:?}", format.description()))
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
    if statement.small_disposals_exemption > Decimal::ZERO {
        println!(
            "Exención de transmisiones hasta 3.000 € (art. 39.5.d): €{}",
            eur::format_eur(statement.small_disposals_exemption)
        );
    }
    if statement.total_capped_fees > Decimal::ZERO {
        println!(
            "Gastos de administración y depósito: €{} deducidos, €{} excluidos por el límite del 3%",
            eur::format_eur(statement.total_deductible_fees),
            eur::format_eur(statement.total_capped_fees)
        );
    }
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

    for review in &statement.wash_sale_boundary_reviews {
        println!("{}", Color::Yellow.paint(format!("WARNING: {}", review.message())));
    }

    if let Some(message) = statement.abatement_message() {
        println!("{}", Color::Yellow.paint(format!("WARNING: {message}")));
    }

    if let Some(message) = statement.small_disposals_message() {
        println!("{}", Color::Yellow.paint(format!("WARNING: {message}")));
    }

    if let Some(message) = statement.custody_fee_cap_message() {
        println!("{}", Color::Yellow.paint(format!("WARNING: {message}")));
    }

    if let Some(message) = statement.carried_cross_offset_message() {
        println!("{}", Color::Yellow.paint(format!("WARNING: {message}")));
    }

    for review in &statement.wash_sale_venue_reviews {
        println!(
            "{}",
            Color::Yellow.paint(format!(
                "WARNING: the {} loss of {} is deducted in full, but that turns on which \
                 valores-homogéneos window {} takes: homogeneous securities were bought back inside \
                 the year and outside the two months, so the one-year limb (NF 3/2014 art. 43.h / \
                 LIRPF art. 33.5.g / TRLFIRPF art. 39.6.g) would defer €{} of it. DGT V0778-25 and \
                 V0951-25 settle the \
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

    if !statement.cash_grants.is_empty() {
        println!(
            "\n{}",
            Color::Yellow.paint(format!(
                "€{} of cash award(s) are reported for information only: they belong to the \
                 GENERAL base, which this tool does not compute. Declare them separately.",
                eur::format_eur(statement.total_cash_grants)
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

    if let Some(message) = statement.margin_interest_message() {
        println!("{}", Color::Yellow.paint(message));
    }

    for fee in &statement.fees {
        if let Some(review) = &fee.review {
            println!("{}", Color::Yellow.paint(format!("WARNING: {review}")));
        }
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

    // Output the statement. The extension selects the format: `.html` is the printable A4 report,
    // anything else the CSV.
    if let Some(path) = output_path {
        let format = OutputFormat::from_path(path);
        write_atomically(path, |writer| match format {
            OutputFormat::Csv => germany::CsvFormatter::write(&statement, writer),
            OutputFormat::Html => {
                let meta = germany::ReportMeta {
                    year,
                    broker_name: broker_statement.broker.name.to_owned(),
                    portfolio_name: portfolio_name.to_owned(),
                    account_id: broker_statement.account_id.clone(),
                    period: broker_statement.period,
                    generated_at: time::now(),
                };
                germany::HtmlReport::write(&statement, &meta, writer)
            }
        })?;

        println!(
            "{}",
            Color::Green.paint(format!("German tax {} written to {path:?}", format.description()))
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
                .paint("Income found but no output file specified. Pass an output path (*.csv, or *.html for the printable report).")
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

#[derive(Clone, Copy)]
enum OutputFormat {
    Csv,
    Html,
}

impl OutputFormat {
    fn from_path(path: &Path) -> OutputFormat {
        let extension = path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase());
        match extension.as_deref() {
            Some("html") | Some("htm") => OutputFormat::Html,
            _ => OutputFormat::Csv,
        }
    }

    fn description(self) -> &'static str {
        match self {
            OutputFormat::Csv => "statement (CSV)",
            OutputFormat::Html => "report (HTML)",
        }
    }
}

// Writes through a per-process temporary file and renames it into place, so an aborted run never
// leaves a truncated report behind and two concurrent runs never share a temp file
fn write_atomically<F>(path: &Path, write: F) -> EmptyResult
    where F: FnOnce(&mut BufWriter<File>) -> EmptyResult
{
    // A rename replaces a symlink instead of following it, which would leave the file the user
    // actually tracks stale while the run reports success. Resolve the link first so the replace
    // lands on its target. A path that does not exist yet has nothing to resolve.
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = resolved.as_path();

    let mut temp_path = path.as_os_str().to_os_string();
    temp_path.push(format!(".{}.tmp", std::process::id()));
    let temp_path = std::path::PathBuf::from(temp_path);

    File::create(&temp_path).map_err(GenericError::from).and_then(|file| {
        let mut writer = BufWriter::new(file);
        write(&mut writer)?;
        writer.flush()?;
        Ok(())
    }).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to write {temp_path:?}: {e}")
    })?;

    carry_permissions(path, &temp_path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to carry the permissions of {path:?} over to {temp_path:?}: {e}")
    })?;

    fs::rename(&temp_path, path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to rename {temp_path:?} to {path:?}: {e}")
    })?;

    Ok(())
}

// A tax statement holds real trading data and the user may well have restricted it, but the rename
// carries the temporary file's mode, not the target's, so an existing 0600 file would come back
// 0644 unless its mode is copied over first.
#[cfg(unix)]
fn carry_permissions(target: &Path, temp_path: &Path) -> EmptyResult {
    if let Ok(metadata) = fs::metadata(target) {
        fs::set_permissions(temp_path, metadata.permissions())?;
    }
    Ok(())
}

// Windows Permissions carry only the read-only flag, which cannot help here (a read-only target
// already refuses the rename) and can hurt: a read-only temporary file also refuses the cleanup
// that follows a failed rename, leaving it behind.
#[cfg(not(unix))]
fn carry_permissions(_target: &Path, _temp_path: &Path) -> EmptyResult {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn output_format_follows_the_extension() {
        for path in ["a.html", "a.htm", "a.HTML", "a.Htm"] {
            assert!(matches!(OutputFormat::from_path(Path::new(path)), OutputFormat::Html),
                    "{path} should select the HTML report");
        }
        for path in ["a.csv", "a.CSV", "a.txt", "a", "a.html.csv"] {
            assert!(matches!(OutputFormat::from_path(Path::new(path)), OutputFormat::Csv),
                    "{path} should select the CSV statement");
        }
    }

    #[test]
    fn write_atomically_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("statement.csv");

        write_atomically(&path, |writer| Ok(writer.write_all(b"first")?)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");

        // A failing write leaves the previous contents in place and cleans up after itself.
        let error = write_atomically(&path, |writer| {
            writer.write_all(b"partial")?;
            Err!("the formatter gave up")
        }).unwrap_err().to_string();
        assert!(error.contains("the formatter gave up"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");

        let leftovers: Vec<String> = fs::read_dir(dir.path()).unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != "statement.csv")
            .collect();
        assert!(leftovers.is_empty(), "temporary files survived: {leftovers:?}");
    }

    /// A symlinked output path must reach the file it points at. A bare rename would replace the
    /// link with a regular file and leave that target holding stale figures, while the run still
    /// reported success.
    #[cfg(unix)]
    #[test]
    fn write_atomically_follows_a_symlinked_output_path() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("statement.csv");
        let link = dir.path().join("latest.csv");

        write_atomically(&target, |writer| Ok(writer.write_all(b"first")?)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_atomically(&link, |writer| Ok(writer.write_all(b"second")?)).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "second");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(),
                "the link was replaced instead of followed");
    }

    /// A mode the user tightened by hand must survive the next run; the rename would otherwise
    /// hand the target the temp file's fresh 0644.
    #[cfg(unix)]
    #[test]
    fn write_atomically_keeps_the_permissions_of_an_existing_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("statement.csv");

        write_atomically(&path, |writer| Ok(writer.write_all(b"first")?)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        write_atomically(&path, |writer| Ok(writer.write_all(b"second")?)).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the statement became readable by others");
    }
}

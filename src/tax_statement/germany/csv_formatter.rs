//! CSV output formatter for German tax statements.
//!
//! Generates comprehensive CSV format per contracts/csv-output.md specification.

use std::io::Write;

use crate::core::GenericResult;
use crate::formatting;
use crate::types::Decimal;

use super::statement::{CapitalGainEntry, DividendEntry, FxGainEntry, GermanTaxStatement, InterestEntry};

/// CSV formatter for German tax statements.
pub struct GermanCsvFormatter;

impl GermanCsvFormatter {
    /// Write the complete tax statement to CSV format.
    pub fn write<W: Write>(statement: &GermanTaxStatement, writer: &mut W) -> GenericResult<()> {
        // Write header row
        Self::write_header(writer)?;

        // Write transaction rows
        for entry in &statement.capital_gains {
            Self::write_capital_gain_row(writer, entry)?;
        }

        for entry in &statement.dividends {
            Self::write_dividend_row(writer, entry)?;
        }

        for entry in &statement.interest {
            Self::write_interest_row(writer, entry)?;
        }

        for entry in &statement.fx_gains {
            Self::write_fx_gain_row(writer, entry)?;
        }

        // Write summary rows
        Self::write_summary_rows(writer, statement)?;

        Ok(())
    }

    fn write_header<W: Write>(writer: &mut W) -> GenericResult<()> {
        writeln!(
            writer,
            "transaction_type,transaction_date,settle_date,symbol,isin,description,quantity,cost_basis_eur,proceeds_eur,gross_amount_eur,gross_gain_loss_eur,teilfreistellung_pct,taxable_amount_eur,foreign_tax_eur,abgeltungssteuer_eur,solidaritaetszuschlag_eur,kirchensteuer_eur,foreign_tax_credit_eur,total_tax_eur,net_tax_eur,notes"
        )?;
        Ok(())
    }

    fn write_capital_gain_row<W: Write>(
        writer: &mut W,
        entry: &CapitalGainEntry,
    ) -> GenericResult<()> {
        writeln!(
            writer,
            "Capital Gain,{},{},{},{},{},{},{},{},{},,{},{},{},{},{},{},{},{},{},{}",
            formatting::format_date(entry.transaction_date),
            formatting::format_date(entry.settle_date),
            Self::escape_csv(&entry.symbol),
            Self::escape_csv(&entry.isin),
            Self::escape_csv(&entry.description),
            Self::format_decimal(entry.quantity),
            Self::format_decimal(entry.cost_basis_eur),
            Self::format_decimal(entry.proceeds_eur),
            Self::format_decimal(entry.gross_gain_loss),
            Self::format_decimal(entry.teilfreistellung_rate.rate() * dec!(100)),
            Self::format_decimal(entry.taxable_amount),
            Self::format_decimal(entry.foreign_tax),
            Self::format_decimal(entry.abgeltungssteuer),
            Self::format_decimal(entry.solidaritaetszuschlag),
            Self::format_decimal(entry.kirchensteuer),
            Self::format_decimal(dec!(0)), // No foreign tax credit for capital gains
            Self::format_decimal(entry.total_tax),
            Self::format_decimal(entry.total_tax), // Net = total for capital gains
            entry.notes.as_deref().unwrap_or("")
        )?;
        Ok(())
    }

    fn write_dividend_row<W: Write>(writer: &mut W, entry: &DividendEntry) -> GenericResult<()> {
        writeln!(
            writer,
            "Dividend,{},{},{},{},{},{},,,,{},{},{},{},{},{},{},{},{},{},{}",
            formatting::format_date(entry.payment_date),
            formatting::format_date(entry.payment_date), // settle_date = payment_date for dividends
            Self::escape_csv(&entry.symbol),
            Self::escape_csv(&entry.isin),
            Self::escape_csv(&entry.description),
            Self::format_decimal(entry.quantity),
            Self::format_decimal(entry.gross_amount_eur),
            Self::format_decimal(entry.teilfreistellung_rate.rate() * dec!(100)),
            Self::format_decimal(entry.taxable_amount),
            Self::format_decimal(entry.foreign_withholding_tax),
            Self::format_decimal(entry.abgeltungssteuer),
            Self::format_decimal(entry.solidaritaetszuschlag),
            Self::format_decimal(entry.kirchensteuer),
            Self::format_decimal(entry.foreign_tax_credit),
            Self::format_decimal(entry.total_tax),
            Self::format_decimal(entry.net_tax),
            entry.notes.as_deref().unwrap_or("")
        )?;
        Ok(())
    }

    fn write_interest_row<W: Write>(writer: &mut W, entry: &InterestEntry) -> GenericResult<()> {
        writeln!(
            writer,
            "Interest,{},{},CASH,N/A,{},,,,,{},0.00,{},{},{},{},{},{},{},{},{}",
            formatting::format_date(entry.payment_date),
            formatting::format_date(entry.payment_date),
            Self::escape_csv(&entry.description),
            Self::format_decimal(entry.gross_amount_eur),
            Self::format_decimal(entry.taxable_amount),
            Self::format_decimal(entry.foreign_withholding_tax),
            Self::format_decimal(entry.abgeltungssteuer),
            Self::format_decimal(entry.solidaritaetszuschlag),
            Self::format_decimal(entry.kirchensteuer),
            Self::format_decimal(entry.foreign_tax_credit),
            Self::format_decimal(entry.total_tax),
            Self::format_decimal(entry.net_tax),
            entry.notes.as_deref().unwrap_or("")
        )?;
        Ok(())
    }

    fn write_fx_gain_row<W: Write>(writer: &mut W, entry: &FxGainEntry) -> GenericResult<()> {
        writeln!(
            writer,
            "FX Gain/Loss,{},{},{},N/A,{},,,,,{},0.00,{},{},{},{},{},{},{},{},{}",
            formatting::format_date(entry.transaction_date),
            formatting::format_date(entry.transaction_date),
            Self::escape_csv(&entry.currency_pair),
            Self::escape_csv(&entry.description),
            Self::format_decimal(entry.gross_amount_eur),
            Self::format_decimal(entry.taxable_amount),
            Self::format_decimal(dec!(0)), // No foreign withholding tax for FX
            Self::format_decimal(entry.abgeltungssteuer),
            Self::format_decimal(entry.solidaritaetszuschlag),
            Self::format_decimal(entry.kirchensteuer),
            Self::format_decimal(dec!(0)), // No foreign tax credit for FX
            Self::format_decimal(entry.total_tax),
            Self::format_decimal(entry.total_tax), // Net = total for FX
            entry.notes.as_deref().unwrap_or("")
        )?;
        Ok(())
    }

    fn write_summary_rows<W: Write>(
        writer: &mut W,
        statement: &GermanTaxStatement,
    ) -> GenericResult<()> {
        writeln!(
            writer,
            "SUMMARY_TOTAL_TAXABLE_INCOME,Total Taxable Income,{}",
            Self::format_decimal(statement.total_taxable_income)
        )?;
        writeln!(
            writer,
            "SUMMARY_TOTAL_FOREIGN_TAX,Total Foreign Tax Paid,{}",
            Self::format_decimal(statement.total_foreign_tax)
        )?;
        writeln!(
            writer,
            "SUMMARY_TOTAL_ABGELTUNGSSTEUER,Total Abgeltungssteuer,{}",
            Self::format_decimal(statement.total_abgeltungssteuer)
        )?;
        writeln!(
            writer,
            "SUMMARY_TOTAL_SOLIDARITAETSZUSCHLAG,Total Solidaritätszuschlag,{}",
            Self::format_decimal(statement.total_solidaritaetszuschlag)
        )?;
        writeln!(
            writer,
            "SUMMARY_TOTAL_KIRCHENSTEUER,Total Kirchensteuer,{}",
            Self::format_decimal(statement.total_kirchensteuer)
        )?;
        writeln!(
            writer,
            "SUMMARY_FX_GAINS,Total FX Gains,{}",
            Self::format_decimal(statement.total_fx_gains)
        )?;
        writeln!(
            writer,
            "SUMMARY_FX_LOSSES,Total FX Losses,{}",
            Self::format_decimal(statement.total_fx_losses)
        )?;
        writeln!(
            writer,
            "SUMMARY_TOTAL_GERMAN_TAX,Total German Tax,{}",
            Self::format_decimal(statement.total_german_tax)
        )?;
        writeln!(
            writer,
            "SUMMARY_FOREIGN_TAX_CREDIT,Foreign Tax Credit Applied,{}",
            Self::format_decimal(statement.total_foreign_tax_credit)
        )?;
        writeln!(
            writer,
            "SUMMARY_NET_TAX_DUE,Net German Tax Due,{}",
            Self::format_decimal(statement.net_tax_due)
        )?;
        writeln!(
            writer,
            "SUMMARY_LOSS_CF_USED,Loss Carryforward Used,{}",
            Self::format_decimal(statement.loss_carryforward_used)
        )?;
        writeln!(
            writer,
            "SUMMARY_LOSS_CF_NEW,New Loss Carryforward,{}",
            Self::format_decimal(statement.loss_carryforward_remaining)
        )?;

        // Anlage KAP form line values
        writeln!(writer)?;
        writeln!(writer, "# ANLAGE KAP - German Tax Form Values")?;
        writeln!(writer, "# These values can be transferred directly to the Anlage KAP form")?;
        writeln!(
            writer,
            "KAP_ZEILE_19,Ausländische Kapitalerträge (Foreign Capital Income),{}",
            Self::format_decimal(statement.kap_zeile_19)
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_22,Sonstige Verluste ohne Aktien (Non-Stock Losses),{}",
            Self::format_decimal(statement.kap_zeile_22)
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_23,Verluste aus Aktienveräußerungen (Stock Sale Losses),{}",
            Self::format_decimal(statement.kap_zeile_23)
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_41,Anrechenbare ausländische Steuer (Creditable Foreign Tax),{}",
            Self::format_decimal(statement.kap_zeile_41)
        )?;

        // Non-taxable amounts (for reference)
        if statement.non_taxable_margin_fx != dec!(0) {
            writeln!(writer)?;
            writeln!(writer, "# NON-TAXABLE AMOUNTS (Nicht steuerbar)")?;
            writeln!(
                writer,
                "NON_TAXABLE_MARGIN_FX,Tilgung Fremdwährungskredit / Margin Loan FX (nicht steuerbar),{}",
                Self::format_decimal(statement.non_taxable_margin_fx)
            )?;
        }

        Ok(())
    }

    fn format_decimal(value: Decimal) -> String {
        format!("{:.2}", value)
    }

    fn escape_csv(value: &str) -> String {
        if value.contains(',') || value.contains('"') || value.contains('\n') {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    }
}

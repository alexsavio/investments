//! CSV output formatter for German tax statements.
//!
//! Generates comprehensive CSV format per contracts/csv-output.md specification.

use std::io::Write;

use rust_decimal::RoundingStrategy;

use crate::core::GenericResult;
use crate::formatting;
use crate::types::Decimal;

use super::statement::{
    CapitalGainEntry, CashGrantEntry, CorporateActionEntry, DividendEntry, FeeEntry, FxGainEntry,
    GermanTaxStatement, InterestEntry, StockGrantEntry,
};

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

        // Write additional income types
        for entry in &statement.fees {
            Self::write_fee_row(writer, entry)?;
        }

        for entry in &statement.stock_grants {
            Self::write_stock_grant_row(writer, entry)?;
        }

        for entry in &statement.cash_grants {
            Self::write_cash_grant_row(writer, entry)?;
        }

        for entry in &statement.corporate_actions {
            Self::write_corporate_action_row(writer, entry)?;
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
            Self::escape_csv(entry.notes.as_deref().unwrap_or(""))
        )?;
        Ok(())
    }

    fn write_dividend_row<W: Write>(writer: &mut W, entry: &DividendEntry) -> GenericResult<()> {
        // Dividend records carry no share quantity; leave that cell empty rather than a misleading 0.
        writeln!(
            writer,
            "Dividend,{},{},{},{},{},,,,,{},{},{},{},{},{},{},{},{},{},{}",
            formatting::format_date(entry.payment_date),
            formatting::format_date(entry.payment_date), // settle_date = payment_date for dividends
            Self::escape_csv(&entry.symbol),
            Self::escape_csv(&entry.isin),
            Self::escape_csv(&entry.description),
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
            Self::escape_csv(entry.notes.as_deref().unwrap_or(""))
        )?;
        Ok(())
    }

    fn write_interest_row<W: Write>(writer: &mut W, entry: &InterestEntry) -> GenericResult<()> {
        writeln!(
            writer,
            "Interest,{},{},CASH,,{},,,,,{},0.00,{},{},{},{},{},{},{},{},{}",
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
            Self::escape_csv(entry.notes.as_deref().unwrap_or(""))
        )?;
        Ok(())
    }

    fn write_fx_gain_row<W: Write>(writer: &mut W, entry: &FxGainEntry) -> GenericResult<()> {
        writeln!(
            writer,
            "FX Gain/Loss,{},{},{},,{},,,,,{},0.00,{},{},{},{},{},{},{},{},{}",
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
            Self::escape_csv(entry.notes.as_deref().unwrap_or(""))
        )?;
        Ok(())
    }

    fn write_fee_row<W: Write>(writer: &mut W, entry: &FeeEntry) -> GenericResult<()> {
        // Fees are informational only: §20(9) EStG bars deducting expenses under the Abgeltungsteuer,
        // so the fee is reported but the taxable-amount column stays zero (no deduction).
        writeln!(
            writer,
            "Fee/Deduction,{},{},,,{},,,,,{},0.00,{},0.00,0.00,0.00,0.00,0.00,0.00,0.00,{}",
            formatting::format_date(entry.date),
            formatting::format_date(entry.date),
            Self::escape_csv(&entry.description),
            Self::format_decimal(entry.amount_eur), // informational fee amount
            Self::format_decimal(dec!(0)),          // not deductible (§20(9) EStG)
            Self::escape_csv(
                entry
                    .notes
                    .as_deref()
                    .unwrap_or("Informational — not deductible under §20(9) EStG")
            )
        )?;
        Ok(())
    }

    fn write_stock_grant_row<W: Write>(
        writer: &mut W,
        entry: &StockGrantEntry,
    ) -> GenericResult<()> {
        // Stock grants are employment income, NOT capital income
        // They are reported separately for manual declaration as employment income
        writeln!(
            writer,
            "Stock Grant (Employment Income),{},{},{},{},RSU/Stock Grant,{},,,,,,{},0.00,,,,0.00,,,{}",
            formatting::format_date(entry.vest_date),
            formatting::format_date(entry.vest_date),
            Self::escape_csv(&entry.symbol),
            Self::escape_csv(&entry.isin),
            Self::format_decimal(entry.quantity),
            Self::format_decimal(entry.total_fmv_eur),
            Self::escape_csv(
                entry
                    .notes
                    .as_deref()
                    .unwrap_or("Geldwerter Vorteil - declare as employment income (Anlage N)")
            )
        )?;
        Ok(())
    }

    fn write_cash_grant_row<W: Write>(writer: &mut W, entry: &CashGrantEntry) -> GenericResult<()> {
        // Cash grants are other income, NOT capital income
        // They are reported separately for manual declaration
        writeln!(
            writer,
            "Cash Grant (Other Income),{},{},,,{},,,,,{},,{},0.00,,,,0.00,,,{}",
            formatting::format_date(entry.date),
            formatting::format_date(entry.date),
            Self::escape_csv(&entry.description),
            Self::format_decimal(entry.amount_eur),
            Self::format_decimal(entry.amount_eur),
            Self::escape_csv(
                entry
                    .notes
                    .as_deref()
                    .unwrap_or("Sonstige Einkünfte §22 EStG - taxable if >€256/year")
            )
        )?;
        Ok(())
    }

    fn write_corporate_action_row<W: Write>(
        writer: &mut W,
        entry: &CorporateActionEntry,
    ) -> GenericResult<()> {
        // Corporate actions are informational - tax impact varies
        let tax_impact = entry
            .tax_impact_eur
            .map(Self::format_decimal)
            .unwrap_or_default();
        writeln!(
            writer,
            "Corporate Action,{},{},{},,{} - {},,,,,{},,{},0.00,0.00,0.00,0.00,0.00,0.00,0.00,{}",
            formatting::format_date(entry.date),
            formatting::format_date(entry.date),
            Self::escape_csv(&entry.symbol),
            entry.action_type,
            Self::escape_csv(&entry.description),
            tax_impact,
            tax_impact,
            Self::escape_csv(entry.notes.as_deref().unwrap_or(""))
        )?;
        Ok(())
    }

    fn write_summary_rows<W: Write>(
        writer: &mut W,
        statement: &GermanTaxStatement,
    ) -> GenericResult<()> {
        writeln!(writer)?;
        writeln!(
            writer,
            "# Summary tax is computed on the year's net taxable base after the §20(6) loss pots,"
        )?;
        writeln!(
            writer,
            "# carryforward, and the Sparer-Pauschbetrag — NOT the sum of the per-row tax columns."
        )?;
        // The summary/KAP block is a separate section with its own 3-column shape.
        writeln!(writer, "summary_key,label,value_eur")?;
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
            "SUMMARY_TOTAL_FEES,Total Fees (informational — not deductible under §20(9) EStG),{}",
            Self::format_decimal(statement.total_fees)
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
            "SUMMARY_SPARER_PAUSCHBETRAG,Sparer-Pauschbetrag applied,{}",
            Self::format_decimal(statement.sparer_pauschbetrag_used)
        )?;
        writeln!(
            writer,
            "SUMMARY_LOSS_CF_STOCK_NEXT,Stock loss carryforward to next year (§20(6) Aktien),{}",
            Self::format_decimal(statement.loss_carryforward_stock_next)
        )?;
        writeln!(
            writer,
            "SUMMARY_LOSS_CF_GENERAL_NEXT,General loss carryforward to next year,{}",
            Self::format_decimal(statement.loss_carryforward_other_next)
        )?;

        // Anlage KAP form line values (non-fund income only).
        writeln!(writer)?;
        writeln!(
            writer,
            "# ANLAGE KAP - German Tax Form Values (non-fund income)"
        )?;
        writeln!(
            writer,
            "# Line numbers follow the 2024/2025 Anlage KAP; re-check against the year's form."
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_19,Ausländische Kapitalerträge (net foreign capital income),{}",
            Self::format_decimal(statement.kap_zeile_19)
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_20,Enthaltene Gewinne aus Aktienveräußerungen (contained share-sale gains),{}",
            Self::format_decimal(statement.kap_zeile_20)
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_22,Enthaltene Verluste ohne Aktien (contained non-share losses),{}",
            Self::format_decimal(statement.kap_zeile_22)
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_23,Enthaltene Verluste aus Aktienveräußerungen (contained share-sale losses),{}",
            Self::format_decimal(statement.kap_zeile_23)
        )?;
        writeln!(
            writer,
            "KAP_ZEILE_41,Anrechenbare ausländische Steuer (Creditable Foreign Tax),{}",
            Self::format_decimal(statement.kap_zeile_41)
        )?;

        // Anlage KAP-INV: investment-fund income, reported GROSS (pre-Teilfreistellung).
        Self::write_kap_inv_section(writer, statement)?;
        Self::write_vorabpauschale_warning(writer, statement)?;

        // Non-capital income (reported separately)
        if statement.total_stock_grant_income > dec!(0)
            || statement.total_cash_grant_income > dec!(0)
        {
            writeln!(writer)?;
            writeln!(
                writer,
                "# NON-CAPITAL INCOME (Not Abgeltungssteuer - requires separate declaration)"
            )?;
            if statement.total_stock_grant_income > dec!(0) {
                writeln!(
                    writer,
                    "EMPLOYMENT_INCOME_STOCK_GRANTS,Stock Grants (geldwerter Vorteil - Anlage N),{}",
                    Self::format_decimal(statement.total_stock_grant_income)
                )?;
            }
            if statement.total_cash_grant_income > dec!(0) {
                let taxable_note = if statement.total_cash_grant_income > dec!(256) {
                    " (TAXABLE - exceeds €256 threshold)"
                } else {
                    " (not taxable - below €256 threshold)"
                };
                writeln!(
                    writer,
                    "OTHER_INCOME_CASH_GRANTS,Cash Grants (sonstige Einkünfte §22 EStG){},{}",
                    taxable_note,
                    Self::format_decimal(statement.total_cash_grant_income)
                )?;
            }
        }

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

    fn write_kap_inv_section<W: Write>(
        writer: &mut W,
        statement: &GermanTaxStatement,
    ) -> GenericResult<()> {
        let groups = [
            (
                "AKTIENFONDS",
                "Aktienfonds (equity 30% Teilfreistellung)",
                &statement.kap_inv_equity,
            ),
            (
                "MISCHFONDS",
                "Mischfonds (mixed 15% Teilfreistellung)",
                &statement.kap_inv_mixed,
            ),
            (
                "SONSTIGE",
                "Sonstige Fonds (bond/other 0% Teilfreistellung)",
                &statement.kap_inv_other,
            ),
        ];

        let is_empty = |g: &super::statement::KapInvGroup| {
            g.distributions == dec!(0) && g.sale_gains == dec!(0) && g.sale_losses == dec!(0)
        };
        if groups.iter().all(|(_, _, g)| is_empty(g)) {
            return Ok(());
        }

        writeln!(writer)?;
        writeln!(
            writer,
            "# ANLAGE KAP-INV - Investment fund income. Enter these GROSS values; the tax office"
        )?;
        writeln!(writer, "# applies the Teilfreistellung itself.")?;
        for (key, label, group) in groups {
            if is_empty(group) {
                continue;
            }
            writeln!(
                writer,
                "KAP_INV_{key}_DISTRIBUTIONS,{label} — gross distributions,{}",
                Self::format_decimal(group.distributions)
            )?;
            writeln!(
                writer,
                "KAP_INV_{key}_SALE_GAINS,{label} — gross sale gains,{}",
                Self::format_decimal(group.sale_gains)
            )?;
            writeln!(
                writer,
                "KAP_INV_{key}_SALE_LOSSES,{label} — gross sale losses,{}",
                Self::format_decimal(group.sale_losses)
            )?;
        }
        Ok(())
    }

    fn write_vorabpauschale_warning<W: Write>(
        writer: &mut W,
        statement: &GermanTaxStatement,
    ) -> GenericResult<()> {
        let funds = statement.fund_identifiers();
        if funds.is_empty() {
            return Ok(());
        }
        writeln!(writer)?;
        writeln!(
            writer,
            "# WARNING: Vorabpauschale (§18 InvStG) is NOT computed. Accumulating funds owe a yearly"
        )?;
        writeln!(
            writer,
            "# advance lump-sum tax; review these fund holdings separately: {}",
            funds.join(", ")
        )?;
        Ok(())
    }

    /// Round a reported figure to two places, half away from zero (German tax-form practice for
    /// per-line amounts). Each figure is rounded once from full precision; a one-cent gap between
    /// rounded components and a rounded total is acceptable, unlike the silent truncation of `{:.2}`.
    fn format_decimal(value: Decimal) -> String {
        let mut value = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
        value.rescale(2);
        value.to_string()
    }

    fn escape_csv(value: &str) -> String {
        if value.contains(',') || value.contains('"') || value.contains('\n') {
            format!("\"{}\"", value.replace('"', "\"\""))
        } else {
            value.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxes::germany::TeilfreistellungRate;
    use crate::types::Date;

    /// Reported figures are rounded half-up to two places, not truncated: `{:.2}` on rust_decimal
    /// truncates (0.518 → "0.51"), understating or overstating cents on the tax form.
    #[test]
    fn format_decimal_rounds_half_up_to_two_places() {
        assert_eq!(GermanCsvFormatter::format_decimal(dec!(0.518)), "0.52");
        assert_eq!(GermanCsvFormatter::format_decimal(dec!(0.015675)), "0.02");
        assert_eq!(GermanCsvFormatter::format_decimal(dec!(1.5)), "1.50");
        assert_eq!(GermanCsvFormatter::format_decimal(dec!(2)), "2.00");
        assert_eq!(GermanCsvFormatter::format_decimal(dec!(-0.125)), "-0.13");
    }

    const COLUMNS: usize = 21;

    fn date() -> Date {
        Date::from_ymd_opt(2024, 6, 15).unwrap()
    }

    fn render(write: impl FnOnce(&mut Vec<u8>) -> GenericResult<()>) -> String {
        let mut buf = Vec::new();
        write(&mut buf).unwrap();
        String::from_utf8(buf).unwrap().trim_end().to_string()
    }

    /// Every transaction row carries the full 21-column shape and uses empty cells, never the
    /// "N/A" placeholder, for inapplicable numeric fields.
    #[test]
    fn transaction_rows_are_21_columns_without_na() {
        let rows = [
            render(|w| {
                GermanCsvFormatter::write_interest_row(
                    w,
                    &InterestEntry {
                        payment_date: date(),
                        description: "idle cash".to_string(),
                        gross_amount_eur: dec!(25),
                        foreign_withholding_tax: dec!(0),
                        taxable_amount: dec!(25),
                        abgeltungssteuer: dec!(6.25),
                        solidaritaetszuschlag: dec!(0.34),
                        kirchensteuer: dec!(0),
                        foreign_tax_credit: dec!(0),
                        total_tax: dec!(6.59),
                        net_tax: dec!(6.59),
                        notes: None,
                    },
                )
            }),
            render(|w| {
                GermanCsvFormatter::write_fx_gain_row(
                    w,
                    &FxGainEntry {
                        transaction_date: date(),
                        currency_pair: "EUR.USD".to_string(),
                        description: "conversion".to_string(),
                        gross_amount_eur: dec!(10),
                        taxable_amount: dec!(10),
                        abgeltungssteuer: dec!(2.5),
                        solidaritaetszuschlag: dec!(0.14),
                        kirchensteuer: dec!(0),
                        total_tax: dec!(2.64),
                        notes: None,
                    },
                )
            }),
            render(|w| {
                GermanCsvFormatter::write_fee_row(
                    w,
                    &FeeEntry {
                        date: date(),
                        description: "ADR fee".to_string(),
                        amount_eur: dec!(2),
                        notes: None,
                    },
                )
            }),
            render(|w| {
                GermanCsvFormatter::write_stock_grant_row(
                    w,
                    &StockGrantEntry {
                        vest_date: date(),
                        symbol: "ACME".to_string(),
                        isin: "US0000000001".to_string(),
                        quantity: dec!(10),
                        fmv_per_share_eur: dec!(5),
                        total_fmv_eur: dec!(50),
                        notes: None,
                    },
                )
            }),
            render(|w| {
                GermanCsvFormatter::write_cash_grant_row(
                    w,
                    &CashGrantEntry {
                        date: date(),
                        description: "bonus".to_string(),
                        amount_eur: dec!(100),
                        notes: None,
                    },
                )
            }),
            render(|w| {
                GermanCsvFormatter::write_corporate_action_row(
                    w,
                    &CorporateActionEntry {
                        date: date(),
                        action_type: super::super::statement::CorporateActionType::Spinoff,
                        symbol: "ACME".to_string(),
                        description: "spinoff".to_string(),
                        tax_impact_eur: None,
                        notes: None,
                    },
                )
            }),
        ];

        for row in &rows {
            assert_eq!(row.split(',').count(), COLUMNS, "wrong column count: {row}");
            assert!(
                !row.contains("N/A"),
                "row still emits N/A placeholder: {row}"
            );
        }
    }

    /// A dividend record carries no share quantity, so the quantity cell is empty, not "0.00".
    #[test]
    fn dividend_quantity_cell_is_empty() {
        let row = render(|w| {
            GermanCsvFormatter::write_dividend_row(
                w,
                &DividendEntry {
                    payment_date: date(),
                    symbol: "ACME".to_string(),
                    isin: "US0000000001".to_string(),
                    description: "dividend".to_string(),
                    quantity: dec!(0),
                    gross_amount_eur: dec!(50),
                    foreign_withholding_tax: dec!(0),
                    teilfreistellung_rate: TeilfreistellungRate::None,
                    taxable_amount: dec!(50),
                    abgeltungssteuer: dec!(12.5),
                    solidaritaetszuschlag: dec!(0.69),
                    kirchensteuer: dec!(0),
                    foreign_tax_credit: dec!(0),
                    total_tax: dec!(13.19),
                    net_tax: dec!(13.19),
                    notes: None,
                },
            )
        });
        assert_eq!(row.split(',').count(), COLUMNS);
        assert_eq!(
            row.split(',').nth(6),
            Some(""),
            "quantity cell should be empty"
        );
    }

    /// KAP-INV rows live in the summary block, whose header declares 3 columns
    /// (`summary_key,label,value_eur`). The group labels must not carry their own commas or each
    /// row splits into 4+ fields, breaking any comma-delimited import of the block.
    #[test]
    fn kap_inv_rows_are_three_columns() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.kap_inv_equity.distributions = dec!(700);
        statement.kap_inv_mixed.sale_gains = dec!(120.5);
        statement.kap_inv_other.sale_losses = dec!(40);

        let output = render(|w| GermanCsvFormatter::write_kap_inv_section(w, &statement));
        let kap_rows: Vec<&str> = output
            .lines()
            .filter(|line| line.starts_with("KAP_INV_"))
            .collect();

        assert_eq!(kap_rows.len(), 9, "three groups × three figures");
        for row in kap_rows {
            assert_eq!(
                row.split(',').count(),
                3,
                "KAP-INV row must stay within the summary block's 3 columns: {row}"
            );
        }
    }
}

//! CSV output formatter for German tax statements.
//!
//! Generates comprehensive CSV format per contracts/csv-output.md specification.

use std::io::Write;

use crate::core::GenericResult;
use crate::formatting;
use crate::taxes::germany::TeilfreistellungRate;
use crate::types::Decimal;

use super::statement::{
    CapitalGainEntry, CashGrantEntry, CorporateActionEntry, DividendEntry, FeeEntry, FxGainEntry,
    GermanTaxStatement, InterestEntry, Section23, StockGrantEntry,
};

/// Anlage KAP-INV line numbers for one fund type: (distributions, Vorabpauschale, sale gains/losses).
///
/// The tool classifies funds as equity / mixed / bond; on the form those map to Aktienfonds,
/// Mischfonds, and sonstige Investmentfonds.
///
/// Pinned to the official forms of the Bundesfinanzverwaltung (Formular-Management-System,
/// formulare-bfinv.de): "Anlage KAP-INV 2024" (print id 2024AnlKAP-INV361NET, September 2024) and
/// "Anlage KAP-INV 2025" (2025AnlKAP-INV361NET, September 2025). PDFs retrieved 2026-08-11 from
/// <https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2024/Anlage_KAP_INV_Steuern.de_01.pdf>
/// and <https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2025/Anlage_KAP_INV_2025_steuern-de.pdf>.
/// Both years are identical: "Ausschüttungen nach § 2 Abs. 11 InvStG" on Zeilen 4 / 5 / 8,
/// "Vorabpauschalen nach § 18 InvStG" on Zeilen 9 / 10 / 13, and "Gewinne und Verluste aus der
/// Veräußerung von Investmentanteilen" on Zeilen 14 / 17 / 26 (Aktien- / Misch- / sonstige Fonds).
fn kap_inv_zeilen(rate: TeilfreistellungRate) -> Option<(u32, u32, u32)> {
    match rate {
        TeilfreistellungRate::Equity => Some((4, 9, 14)),
        TeilfreistellungRate::Mixed => Some((5, 10, 17)),
        TeilfreistellungRate::Bond => Some((8, 13, 26)),
        TeilfreistellungRate::None => None,
    }
}

/// CSV formatter for German tax statements.
pub struct CsvFormatter;

impl CsvFormatter {
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
            "Capital Gain,{},{},{},{},{},{},{},{},,{},{},{},{},{},{},{},{},{},{},{}",
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
            "Dividend,{},{},{},{},{},,,,{},,{},{},{},{},{},{},{},{},{},{}",
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
            "Interest,{},{},CASH,,{},,,,{},,0.00,{},{},{},{},{},{},{},{},{}",
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
            "FX Gain/Loss,{},{},{},,{},,,,{},,0.00,{},{},{},{},{},{},{},{},{}",
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
            "Corporate Action,{},{},{},,{},,,,,{},,{},0.00,0.00,0.00,0.00,0.00,0.00,0.00,{}",
            formatting::format_date(entry.date),
            formatting::format_date(entry.date),
            Self::escape_csv(&entry.symbol),
            Self::escape_csv(&format!("{} - {}", entry.action_type, entry.description)),
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
            "# Zeilen verified against the official Anlage KAP 2024 and 2025 (identical in both);"
        )?;
        writeln!(
            writer,
            "# re-check them if you file a later year — see docs/germany-taxes.md."
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
        Self::write_vorabpauschale(writer, statement)?;
        Self::write_short_positions(writer, statement)?;
        Self::write_section23(writer, statement)?;

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
                TeilfreistellungRate::Equity,
                &statement.kap_inv_equity,
            ),
            (
                "MISCHFONDS",
                "Mischfonds (mixed 15% Teilfreistellung)",
                TeilfreistellungRate::Mixed,
                &statement.kap_inv_mixed,
            ),
            (
                "SONSTIGE",
                "Sonstige Fonds (bond/other 0% Teilfreistellung)",
                TeilfreistellungRate::Bond,
                &statement.kap_inv_other,
            ),
        ];

        let is_empty = |g: &super::statement::KapInvGroup| {
            g.distributions == dec!(0) && g.sale_gains == dec!(0) && g.sale_losses == dec!(0)
        };
        if groups.iter().all(|(_, _, _, g)| is_empty(g)) {
            return Ok(());
        }

        writeln!(writer)?;
        writeln!(
            writer,
            "# ANLAGE KAP-INV - Investment fund income. Enter these GROSS values; the tax office"
        )?;
        writeln!(writer, "# applies the Teilfreistellung itself.")?;
        writeln!(
            writer,
            "# Zeilen verified against the official Anlage KAP-INV 2024 and 2025 (identical in both);"
        )?;
        writeln!(
            writer,
            "# re-check them if you file a later year — see docs/germany-taxes.md."
        )?;
        for (key, label, rate, group) in groups {
            if is_empty(group) {
                continue;
            }
            let (zeile_dist, _zeile_vap, zeile_sale) = kap_inv_zeilen(rate).unwrap_or((0, 0, 0));
            writeln!(
                writer,
                "KAP_INV_{key}_DISTRIBUTIONS,{label} — gross distributions (Zeile {zeile_dist}),{}",
                Self::format_decimal(group.distributions)
            )?;
            // The form carries one net Gewinn/Verlust line per fund type; the gross split below is
            // informational context.
            let net_sale = group.sale_gains - group.sale_losses;
            writeln!(
                writer,
                "KAP_INV_{key}_VERAEUSSERUNG,{label} — net sale gain/loss (Zeile {zeile_sale}),{}",
                Self::format_decimal(net_sale)
            )?;
            writeln!(
                writer,
                "KAP_INV_{key}_SALE_GAINS,{label} — gross sale gains (informational),{}",
                Self::format_decimal(group.sale_gains)
            )?;
            writeln!(
                writer,
                "KAP_INV_{key}_SALE_LOSSES,{label} — gross sale losses (informational),{}",
                Self::format_decimal(group.sale_losses)
            )?;
        }
        Ok(())
    }

    fn write_vorabpauschale<W: Write>(
        writer: &mut W,
        statement: &GermanTaxStatement,
    ) -> GenericResult<()> {
        if !statement.vorabpauschale.is_empty() {
            let next_year = statement.year + 1;
            writeln!(writer)?;
            writeln!(
                writer,
                "# VORABPAUSCHALE (§18 InvStG) — advance lump-sum income of funds held at year end."
            )?;
            writeln!(
                writer,
                "# Deemed received on the first business day of {next_year}; declare it in the \
                 {next_year} return (Anlage KAP-INV, Vorabpauschale Zeilen 9/10/13 by fund type). \
                 Values in EUR."
            )?;
            writeln!(
                writer,
                "# The estimated tax below is a gross figure at the full rate; it ignores next \
                 year's Sparer-Pauschbetrag and any loss offset, so the actual liability may be lower."
            )?;
            for entry in &statement.vorabpauschale {
                let who = if entry.isin.is_empty() {
                    entry.symbol.clone()
                } else {
                    format!("{} ({})", entry.symbol, entry.isin)
                };
                let vap_zeile = kap_inv_zeilen(entry.teilfreistellung_rate)
                    .map(|z| z.1)
                    .unwrap_or(0);
                writeln!(
                    writer,
                    "VORABPAUSCHALE_GROSS,{},{}",
                    Self::escape_csv(&format!(
                        "{who} — gross Vorabpauschale (KAP-INV Zeile {vap_zeile})"
                    )),
                    Self::format_decimal(entry.gross_vorabpauschale)
                )?;
                writeln!(
                    writer,
                    "VORABPAUSCHALE_TAXABLE,{},{}",
                    Self::escape_csv(&format!(
                        "{} — taxable after Teilfreistellung",
                        entry.symbol
                    )),
                    Self::format_decimal(entry.taxable_amount)
                )?;
                writeln!(
                    writer,
                    "VORABPAUSCHALE_TAX,{},{}",
                    Self::escape_csv(&format!(
                        "{} — estimated tax (Abgelt.+Soli+KiSt)",
                        entry.symbol
                    )),
                    Self::format_decimal(entry.total_tax)
                )?;
                writeln!(
                    writer,
                    "VORABPAUSCHALE_CARRYFORWARD,{},{}",
                    Self::escape_csv(&format!(
                        "{} — accumulated gross for next year's config",
                        entry.symbol
                    )),
                    Self::format_decimal(entry.accumulated_after)
                )?;
            }
            writeln!(
                writer,
                "VORABPAUSCHALE_TOTAL_TAX,Total estimated Vorabpauschale tax,{}",
                Self::format_decimal(statement.total_vorabpauschale_tax)
            )?;
        }

        if !statement.vorabpauschale_missing_nav.is_empty() {
            writeln!(writer)?;
            writeln!(
                writer,
                "# WARNING: Vorabpauschale (§18 InvStG) NOT computed for year-end fund holdings without"
            )?;
            writeln!(
                writer,
                "# configured NAVs: {}. Set taxes.fund_nav.<ISIN>.{} (jan1/dec31) to include them.",
                statement.vorabpauschale_missing_nav.join(", "),
                statement.year
            )?;
        }

        Ok(())
    }

    fn write_short_positions<W: Write>(
        writer: &mut W,
        statement: &GermanTaxStatement,
    ) -> GenericResult<()> {
        if statement.short_positions.is_empty() {
            return Ok(());
        }

        writeln!(writer)?;
        writeln!(
            writer,
            "# SHORT POSITIONS — MANUAL §20 EStG REVIEW REQUIRED. No tax is computed for these."
        )?;
        writeln!(
            writer,
            "# Short sales / open written options are Termin-/Stillhaltergeschäfte; classify and \
             declare them by hand (Anlage KAP). Quantities are negative (units still open at year end)."
        )?;
        for (symbol, quantity) in &statement.short_positions {
            let label = format!("{symbol} — open short quantity (manual review)");
            writeln!(
                writer,
                "SHORT_POSITION,{},{}",
                Self::escape_csv(&label),
                Self::format_decimal(*quantity)
            )?;
        }

        Ok(())
    }

    /// Anlage SO (§23 EStG) figures for a non-interest-bearing foreign-currency account. Reported
    /// for manual declaration only — §23 income is taxed at the filer's personal rate, which the
    /// tool cannot know, so no tax is computed here.
    fn write_section23<W: Write>(
        writer: &mut W,
        statement: &GermanTaxStatement,
    ) -> GenericResult<()> {
        let section = &statement.section23;
        if section.is_empty() {
            return Ok(());
        }

        writeln!(writer)?;
        writeln!(
            writer,
            "# ANLAGE SO (§23 EStG) — private Veräußerungsgeschäfte from a non-interest-bearing"
        )?;
        writeln!(
            writer,
            "# foreign-currency account. No tax is computed: §23 income is taxed at your personal rate."
        )?;
        writeln!(
            writer,
            "SECTION23_SHORT_TERM_GAIN,Veräußerungsgeschäfte gehalten ≤ 1 Jahr — Gewinn,{}",
            Self::format_decimal(section.short_term_gains)
        )?;
        writeln!(
            writer,
            "SECTION23_SHORT_TERM_LOSS,Veräußerungsgeschäfte gehalten ≤ 1 Jahr — Verlust,{}",
            Self::format_decimal(section.short_term_losses)
        )?;
        writeln!(
            writer,
            "SECTION23_SHORT_TERM_NET,Veräußerungsgeschäfte gehalten ≤ 1 Jahr — netto,{}",
            Self::format_decimal(section.short_term_net())
        )?;
        writeln!(
            writer,
            "SECTION23_FREIGRENZE,Freigrenze {} (Gesamtgewinn ≤ Freigrenze → steuerfrei; sonst voll steuerpflichtig),{}",
            statement.year,
            Self::format_decimal(Section23::freigrenze(statement.year))
        )?;
        if section.long_term_tax_free != dec!(0) {
            writeln!(
                writer,
                "SECTION23_LONG_TERM_TAX_FREE,Steuerfrei (> 1 Jahr gehalten — Spekulationsfrist erfüllt),{}",
                Self::format_decimal(section.long_term_tax_free)
            )?;
        }
        if section.borrowed_review != dec!(0) {
            writeln!(
                writer,
                "SECTION23_BORROWED_REVIEW,Fremdwährungskredit-Realisierung — MANUAL REVIEW (§20-Ausnahme BMF 19.05.2022 Rz. 131 gilt nicht für §23),{}",
                Self::format_decimal(section.borrowed_review)
            )?;
        }

        Ok(())
    }

    /// Round a reported figure to two places, half away from zero. Delegates to the module-shared
    /// [`super::format_eur`] so the CSV and the console cannot disagree by a rounding cent.
    fn format_decimal(value: Decimal) -> String {
        super::format_eur(value)
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
        assert_eq!(CsvFormatter::format_decimal(dec!(0.518)), "0.52");
        assert_eq!(CsvFormatter::format_decimal(dec!(0.015675)), "0.02");
        assert_eq!(CsvFormatter::format_decimal(dec!(1.5)), "1.50");
        assert_eq!(CsvFormatter::format_decimal(dec!(2)), "2.00");
        assert_eq!(CsvFormatter::format_decimal(dec!(-0.125)), "-0.13");
    }

    /// The KAP-INV Zeilen are pinned to the official forms (Anlage KAP-INV 2024
    /// `2024AnlKAP-INV361NET` and 2025 `2025AnlKAP-INV361NET`, both identical): Ausschüttungen on
    /// 4 / 5 / 8, Vorabpauschalen on 9 / 10 / 13, Veräußerung on 14 / 17 / 26. A silent drift here
    /// sends the filer's figures to the wrong box, so the mapping is asserted, not just documented.
    #[test]
    fn kap_inv_zeilen_match_the_official_form() {
        assert_eq!(kap_inv_zeilen(TeilfreistellungRate::Equity), Some((4, 9, 14)));
        assert_eq!(kap_inv_zeilen(TeilfreistellungRate::Mixed), Some((5, 10, 17)));
        assert_eq!(kap_inv_zeilen(TeilfreistellungRate::Bond), Some((8, 13, 26)));
        assert_eq!(kap_inv_zeilen(TeilfreistellungRate::None), None);
    }

    /// The Anlage KAP block emits exactly the five lines the tool fills, each pinned to the official
    /// form (Anlage KAP 2024 `2024AnlKAP051NET` and 2025 `2025AnlKAP051NET`, both identical):
    /// 19 ausländische Kapitalerträge, 20 Gewinne aus Aktienveräußerungen, 22 Verluste ohne Aktien,
    /// 23 Verluste aus Aktienveräußerungen, 41 anrechenbare ausländische Steuern.
    #[test]
    fn kap_summary_rows_carry_the_official_zeilen() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.kap_zeile_19 = dec!(1000);
        statement.kap_zeile_20 = dec!(400);
        statement.kap_zeile_22 = dec!(50);
        statement.kap_zeile_23 = dec!(25);
        statement.kap_zeile_41 = dec!(15);

        let output = render(|w| CsvFormatter::write_summary_rows(w, &statement));
        let keys: Vec<&str> = output
            .lines()
            .filter(|line| line.starts_with("KAP_ZEILE_"))
            .map(|line| line.split(',').next().unwrap())
            .collect();
        assert_eq!(
            keys,
            [
                "KAP_ZEILE_19",
                "KAP_ZEILE_20",
                "KAP_ZEILE_22",
                "KAP_ZEILE_23",
                "KAP_ZEILE_41",
            ]
        );
        assert!(output.contains("KAP_ZEILE_19,Ausländische Kapitalerträge"));
        assert!(output.contains("KAP_ZEILE_41,Anrechenbare ausländische Steuer"));
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
                CsvFormatter::write_interest_row(
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
                CsvFormatter::write_fx_gain_row(
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
                CsvFormatter::write_fee_row(
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
                CsvFormatter::write_stock_grant_row(
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
                CsvFormatter::write_cash_grant_row(
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
                CsvFormatter::write_corporate_action_row(
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
            CsvFormatter::write_dividend_row(
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

    /// The header carries two distinct gross columns: `gross_gain_loss_eur` (capital-gains/FX
    /// before exemptions) and `gross_amount_eur` (dividend/interest gross). Each row type must
    /// populate its own column and leave the other empty, or a consumer summing a named column
    /// double-counts or misses figures.
    #[test]
    fn gross_value_lands_in_the_named_column() {
        let header = render(CsvFormatter::write_header);
        let columns: Vec<String> = header.split(',').map(str::to_string).collect();
        let cell = |row: &str, name: &str| -> String {
            let idx = columns
                .iter()
                .position(|c| c == name)
                .expect("column present");
            row.split(',').nth(idx).expect("cell present").to_string()
        };

        let capital_gain = render(|w| {
            CsvFormatter::write_capital_gain_row(
                w,
                &CapitalGainEntry {
                    transaction_date: date(),
                    settle_date: date(),
                    symbol: "ACME".to_string(),
                    isin: "US0000000001".to_string(),
                    description: "sold 10 ACME".to_string(),
                    quantity: dec!(10),
                    cost_basis_eur: dec!(1000),
                    proceeds_eur: dec!(1250),
                    gross_gain_loss: dec!(250),
                    taxable_before_exemption: dec!(250),
                    teilfreistellung_rate: TeilfreistellungRate::None,
                    is_stock: true,
                    taxable_amount: dec!(250),
                    foreign_tax: dec!(0),
                    abgeltungssteuer: dec!(62.5),
                    solidaritaetszuschlag: dec!(3.44),
                    kirchensteuer: dec!(0),
                    total_tax: dec!(65.94),
                    pre_2009_holding: false,
                    notes: None,
                },
            )
        });
        assert_eq!(cell(&capital_gain, "gross_gain_loss_eur"), "250.00");
        assert_eq!(cell(&capital_gain, "gross_amount_eur"), "");

        let dividend = render(|w| {
            CsvFormatter::write_dividend_row(
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
        assert_eq!(cell(&dividend, "gross_amount_eur"), "50.00");
        assert_eq!(cell(&dividend, "gross_gain_loss_eur"), "");

        let interest = render(|w| {
            CsvFormatter::write_interest_row(
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
        });
        assert_eq!(cell(&interest, "gross_amount_eur"), "25.00");
        assert_eq!(cell(&interest, "gross_gain_loss_eur"), "");

        let fx = render(|w| {
            CsvFormatter::write_fx_gain_row(
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
        });
        assert_eq!(cell(&fx, "gross_amount_eur"), "10.00");
        assert_eq!(cell(&fx, "gross_gain_loss_eur"), "");
    }

    /// A corporate action whose type or description contains a comma must not break the row: the
    /// combined `type - description` cell is escaped as a unit, so a quote-aware CSV parser still
    /// reads 21 fields with the comma preserved inside the cell.
    #[test]
    fn corporate_action_cell_is_escaped_as_a_unit() {
        let row = render(|w| {
            CsvFormatter::write_corporate_action_row(
                w,
                &CorporateActionEntry {
                    date: date(),
                    action_type: super::super::statement::CorporateActionType::Spinoff,
                    symbol: "ACME".to_string(),
                    description: "spinoff of ACME, Inc. sub-unit".to_string(),
                    tax_impact_eur: None,
                    notes: None,
                },
            )
        });
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(row.as_bytes());
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(record.len(), COLUMNS, "escaped comma broke the field count");
        assert_eq!(&record[5], "Spinoff - spinoff of ACME, Inc. sub-unit");
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

        let output = render(|w| CsvFormatter::write_kap_inv_section(w, &statement));
        let kap_rows: Vec<&str> = output
            .lines()
            .filter(|line| line.starts_with("KAP_INV_"))
            .collect();

        assert_eq!(
            kap_rows.len(),
            12,
            "three groups × four figures (distributions, net Veräußerung, gross gains, gross losses)"
        );
        for row in kap_rows {
            assert_eq!(
                row.split(',').count(),
                3,
                "KAP-INV row must stay within the summary block's 3 columns: {row}"
            );
        }
    }

    /// Short positions render one flagged row per holding under a manual-review banner, carrying the
    /// signed quantity so the reader sees it is an open short, and no such section appears when there
    /// are none.
    #[test]
    fn short_positions_render_a_manual_review_section() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.short_positions = vec![("TSLA".to_string(), dec!(-50))];

        let output = render(|w| CsvFormatter::write_short_positions(w, &statement));
        assert!(output.contains("MANUAL §20 EStG REVIEW REQUIRED"));
        let rows: Vec<&str> = output
            .lines()
            .filter(|line| line.starts_with("SHORT_POSITION,"))
            .collect();
        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].ends_with("-50.00"),
            "row must carry the signed quantity: {}",
            rows[0]
        );

        let empty = GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        let empty_output = render(|w| CsvFormatter::write_short_positions(w, &empty));
        assert!(
            empty_output.is_empty(),
            "no section without short positions"
        );
    }

    /// A symbol containing a comma must not split the SHORT_POSITION row: the whole label cell is
    /// escaped as a unit, so a quote-aware parser still reads exactly three fields.
    #[test]
    fn short_position_symbol_with_comma_stays_one_field() {
        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.short_positions = vec![("AB,C".to_string(), dec!(-10))];

        let output = render(|w| CsvFormatter::write_short_positions(w, &statement));
        let row = output
            .lines()
            .find(|line| line.starts_with("SHORT_POSITION,"))
            .expect("short-position row present");
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(row.as_bytes());
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(
            record.len(),
            3,
            "comma in symbol broke the field count: {row}"
        );
        assert!(
            record[1].contains("AB,C"),
            "label must preserve the symbol: {}",
            &record[1]
        );
        assert_eq!(&record[2], "-10.00");
    }

    /// A fund symbol containing a comma must not split a VORABPAUSCHALE_* row: the whole label cell
    /// is escaped as a unit, so a quote-aware parser still reads exactly three fields.
    #[test]
    fn vorabpauschale_symbol_with_comma_stays_one_field() {
        use super::super::statement::VorabpauschaleEntry;

        let mut statement =
            GermanTaxStatement::new(2024, dec!(0), dec!(0), dec!(0), dec!(0)).unwrap();
        statement.add_vorabpauschale(VorabpauschaleEntry {
            arising_year: 2024,
            deemed_received: Date::from_ymd_opt(2025, 1, 2).unwrap(),
            symbol: "AB,C".to_string(),
            isin: "IE00B4L5Y983".to_string(),
            quantity: dec!(100),
            nav_jan1: dec!(8000),
            nav_dec31: dec!(9200),
            distributions: dec!(0),
            basiszins: dec!(0.0229),
            teilfreistellung_rate: TeilfreistellungRate::Equity,
            gross_vorabpauschale: dec!(128.24),
            taxable_amount: dec!(89.77),
            abgeltungssteuer: dec!(0),
            solidaritaetszuschlag: dec!(0),
            kirchensteuer: dec!(0),
            total_tax: dec!(0),
            accumulated_after: dec!(128.24),
            notes: None,
        });

        let output = render(|w| CsvFormatter::write_vorabpauschale(w, &statement));
        let row = output
            .lines()
            .find(|line| line.starts_with("VORABPAUSCHALE_GROSS,"))
            .expect("Vorabpauschale gross row present");
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(row.as_bytes());
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(
            record.len(),
            3,
            "comma in symbol broke the field count: {row}"
        );
        assert!(
            record[1].contains("AB,C"),
            "label must preserve the symbol: {}",
            &record[1]
        );
        assert_eq!(&record[2], "128.24");
    }
}

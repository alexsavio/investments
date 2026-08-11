# CSV Output Contract: Spain Tax Statement

**Version**: 1.0
**Format**: CSV (Comma-Separated Values)
**Encoding**: UTF-8
**Date**: 2026-08-11

## Overview

The CSV written by `investments tax-statement --output <path>` when `taxes.jurisdiction: spain`. It
covers both regimes — Gipuzkoa (Norma Foral 3/2014) and Territorio Común (Ley 35/2006) — and the
regime is named in the summary block, because the same trades produce different numbers under each.

The file is a working paper, not a filing. Spain has no CSV import path: the figures are transcribed
by hand into Modelo 109 (Gipuzkoa, via Zergabidea) or Modelo 100 (AEAT).

## File Structure

- **Header row**: column names, always first
- **Transaction rows**: one row per capital gain, dividend, interest, FX result, fee — each in the
  full 19-column shape
- **Lot rows**: immediately after the capital-gain row they belong to, one per FIFO lot consumed,
  in the same 19-column shape with `transaction_type = Lot`. Placed inline rather than in a separate
  section so the actualization arithmetic can be read line by line under the sale it justifies
- **Summary section**: after a blank line, its own 3-column header
  (`summary_key,label,value_eur`). Aggregates, group compensation, the carry-forward block, and the
  Modelo box mapping live here — **not** in the 19-column shape
- **Delimiter**: comma (`,`)
- **Quote character**: double quote (`"`) for any field containing a comma, quote, or newline
- **Empty cells**: inapplicable columns are left empty, never `N/A` and never a misleading `0.00`
- **Line terminator**: LF (`\n`)

## Column specification

| # | Column | Type | Meaning | Populated by |
|---|--------|------|---------|--------------|
| 1 | `transaction_type` | String | `Capital Gain`, `Lot`, `Wash Sale Reintegration`, `Dividend`, `Interest`, `FX Gain/Loss`, `FX Borrowed (review)`, `Fee` | all |
| 2 | `transaction_date` | Date | Fecha de transmisión / payment date; acquisition date on a `Lot` row | all |
| 3 | `settle_date` | Date | Settlement date | capital gains, dividends, interest, fees |
| 4 | `symbol` | String | Ticker, `CASH` for interest, currency code for FX | most |
| 5 | `isin` | String | ISIN when the statement carries one | capital gains, dividends |
| 6 | `description` | String | Free text | most |
| 7 | `quantity` | Decimal | Shares disposed of (sale row) or consumed (lot row) | capital gains, lots |
| 8 | `cost_eur` | Decimal | Acquisition cost **before** actualization, commissions included | capital gains, lots |
| 9 | `coefficient` | Decimal (3 dp) | NF 3/2014 art. 45.2 coefficient for that lot's acquisition year; `1.000` under Común. Empty on the sale row — the coefficient is a per-lot figure and a single sale can consume lots of several vintages | lots |
| 10 | `actualized_cost_eur` | Decimal | `cost_eur × coefficient` | capital gains, lots |
| 11 | `proceeds_eur` | Decimal | Sale proceeds net of the sell-side commission | capital gains, lots |
| 12 | `gross_amount_eur` | Decimal | Gross dividend / interest / fee amount | dividends, interest, fees |
| 13 | `gain_loss_eur` | Decimal | Fiscal result: `proceeds_eur − actualized_cost_eur` | capital gains, lots, FX |
| 14 | `deferred_loss_eur` | Decimal | Portion of a loss deferred under the valores-homogéneos rule, as a positive magnitude | capital gains |
| 15 | `integrable_amount_eur` | Decimal | What actually enters the savings base this year: `gain_loss_eur + deferred_loss_eur` | capital gains, reintegrations, FX |
| 16 | `foreign_tax_eur` | Decimal | Tax actually withheld at source | dividends |
| 17 | `treaty_capped_credit_eur` | Decimal | Withholding capped at the treaty rate — the credit's first limb, **informational per row** (see below) | dividends |
| 18 | `savings_group` | String | `RCM` (rendimientos del capital mobiliario) or `GyP` (ganancias y pérdidas patrimoniales); empty when the row enters neither | most |
| 19 | `notes` | String | Caveats, the per-lot actualization arithmetic, regime-specific deductibility notes | optional |

### Per-row figures that are not per-row taxes

Unlike the German statement, no row carries a tax figure. The Spanish savings scale is progressive
and applies to the year's whole base after group compensation, so a per-row tax would be arithmetic
nobody can add up. The same applies to `treaty_capped_credit_eur`: the deducción por doble
imposición internacional (NF 3/2014 art. 91 / LIRPF art. 80) is capped by the **average savings
rate**, which only exists once the whole year is known. The per-row figure is the treaty limb alone.

## Summary section

Rows are `summary_key,label,value_eur`. Keys are stable; labels are prose and may change.

| Key | Meaning |
|---|---|
| `SUMMARY_REGIME` | Regime and tax year (value column carries the year) |
| `SUMMARY_RCM_DIVIDENDS` / `_INTEREST` / `_DEDUCTIBLE_FEES` / `_NET` | The RCM group, built up |
| `SUMMARY_GYP_CAPITAL_GAINS` / `_FX` / `_NET` | The ganancias group, built up |
| `SUMMARY_RCM_LOSSES_APPLIED` / `SUMMARY_GYP_LOSSES_APPLIED` | Prior-year balances consumed this year |
| `SUMMARY_CROSS_OFFSET_RCM_TO_GYP` / `_GYP_TO_RCM` | The 25% cross-group offset (Común only; always 0 under Gipuzkoa) |
| `SUMMARY_RCM_TAXABLE` / `SUMMARY_GYP_TAXABLE` | Each group after compensation |
| `SUMMARY_SAVINGS_BASE` | Base liquidable del ahorro |
| `SUMMARY_SAVINGS_QUOTA` | Cuota íntegra del ahorro |
| `SUMMARY_AVERAGE_SAVINGS_RATE` | Tipo medio del ahorro (not a EUR amount) |
| `SUMMARY_FOREIGN_WITHHOLDING` / `SUMMARY_FOREIGN_TAX_CREDIT` | Withheld abroad, and what is creditable |
| `SUMMARY_NET_TAX_DUE` | `max(0, quota − credit)` |
| `SUMMARY_INFORMATIONAL_FEES` | Fees reported but not deducted |
| `SUMMARY_FX_BORROWED_REVIEW` | Borrowed-balance FX results excluded from the base |
| `CARRYFORWARD_<GROUP>_<YEAR>` | Pending negative balance to put in next year's config, by origin year |
| `CARRYFORWARD_<GROUP>_EXPIRED` | Balance that ran out of its four-year window this year |
| `DEFERRED_LOSS_<n>` | One surviving wash-sale block: symbol, blocked quantity, sale date, loss |

## Modelo box mapping

A `# MODELO` banner introduces the box mapping, followed by `MODELO_109_*` (Gipuzkoa) or
`MODELO_100_*` (Común) rows.

**Every box number is preceded by a `# WARNING:` line.** Gipuzkoa publishes no static numbered form
— the return is generated by Zergabidea — and the only official box map reachable is explicitly
dated **AÑO 2019**. The 2026 reform added a cuota-íntegra column to art. 76.1 and Anexo 3 gained a
crypto section, both of which plausibly renumbered boxes. The casilla for the deducción por doble
imposición internacional is not published anywhere reachable, for any year, and is emitted without a
number. Treat every number as indicative and verify against the year's own form.

Territorio Común rows carry the ejercicio-2025 numbers from Anexo I of the Orden HAC/277/2026
consultation draft: `MODELO_100_0027` interest, `0029` dividends, `0037` gastos de administración y
depósito, `0326_0340` ganancias por transmisión de acciones cotizadas, `0460` base liquidable del
ahorro, `0588` deducción por doble imposición internacional.

**No retenciones row is emitted for either form.** Foreign withholding is not a Spanish retención; it
is relieved only through the deducción por doble imposición internacional. A `# WARNING:` names the
casilla the figure does not belong in (109: `07+22`, 100: `0597`), because entering it in both places
claims the same tax twice.

## Warning lines

Comment lines start with `#`. A consumer must skip them.

| Banner | Emitted when |
|---|---|
| `# WARNING: valores-homogéneos window extends past the statement` | A loss sale's +2-month repurchase window reaches beyond the statement's last date, so a repurchase that would defer the loss cannot be seen yet |
| `# WARNING: sales before <year> were not replayed` | The statement contains disposals in a year with no shipped actualization table, so they were not tested for deferral |
| `# SHORT POSITIONS` | Open short positions exist; no tax is computed for them |
| `# FX BORROWED BALANCE` | Foreign-currency results realized on a borrowed balance, excluded from the base pending manual review |

## Validation rules

- Header row first; column names and order exactly as specified
- Every transaction and lot row has exactly 19 fields under a quote-aware parser
- Decimal amounts: exactly 2 decimal places, rounded half away from zero (never truncated), except
  `coefficient` (3 dp) and `SUMMARY_AVERAGE_SAVINGS_RATE` (full precision)
- Dates: ISO 8601 (`YYYY-MM-DD`)
- No thousands separators — a comma inside a number would break the delimiter
- Negative amounts carry a `-` prefix; currency symbols are never included (all amounts EUR)

## Compatibility

- Excel / LibreOffice Calc / Google Sheets: compatible
- `pandas`: `pd.read_csv(path, comment="#", decimal=".")` reads the transaction block; the summary
  block needs a second pass because it has its own header
- Spanish tax software: manual entry only

## Versioning

- **Version 1.0**: initial implementation (2026-08-11)

## Related contracts

- German CSV output: `specs/001-germany-tax/contracts/csv-output.md`
- Configuration schema: `specs/002-spain-tax/plan.md`, "Config schema"

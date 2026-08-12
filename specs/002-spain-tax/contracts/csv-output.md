# CSV Output Contract: Spain Tax Statement

**Version**: 1.3
**Format**: CSV (Comma-Separated Values)
**Encoding**: UTF-8
**Date**: 2026-08-12

## Overview

The CSV written by `investments tax-statement <portfolio> <year> <path>` when `taxes.jurisdiction: spain`. It
covers all three regimes — Gipuzkoa (Norma Foral 3/2014), Territorio Común (Ley 35/2006) and Navarra
(Decreto Foral Legislativo 4/2008) — and the regime is named in the summary block, because the same
trades produce different numbers under each.

The file is a working paper, not a filing. Spain has no CSV import path: the figures are transcribed
by hand into Modelo 109 (Gipuzkoa, via Zergabidea), Modelo 100 (AEAT) or Modelo F-93 (Navarra).

## File Structure

- **Header row**: column names, always first
- **Transaction rows**: one row per capital gain, wash-sale reintegration, dividend, interest, FX
  result, fee, stock-grant vest and corporate action — each in the full 19-column shape, emitted in
  that order
- **Lot rows**: immediately after the capital-gain row they belong to, one per FIFO lot consumed,
  in the same 19-column shape with `transaction_type = Lot`. Placed inline rather than in a separate
  section so the actualization arithmetic can be read line by line under the sale it justifies
- **Summary section**: after a blank line, two `#` preamble lines explaining that the cuota is
  computed once on the year's base and not summed from the rows, then its own 3-column header
  (`summary_key,label,value_eur`). Aggregates, group compensation, the carry-forward block, the
  deferred-loss block and the Modelo box mapping live here — **not** in the 19-column shape
- **Carry-forward block**: after a blank line, a `# SALDOS NEGATIVOS PENDIENTES` banner and its
  `CARRYFORWARD_*` rows. Omitted entirely when both ledgers are empty and nothing expired
- **Deferred-loss block**: after a blank line, a `# PÉRDIDAS DIFERIDAS PENDIENTES` banner and its
  `DEFERRED_LOSS_<n>` rows. Omitted when nothing is still blocked
- **Modelo box block**: after a blank line, a `# MODELO` banner and the box rows
- **Warning blocks**: last, each introduced by its own banner
- **Delimiter**: comma (`,`)
- **Quote character**: double quote (`"`) for any field containing a comma, quote, or newline
- **Empty cells**: inapplicable columns are left empty, never `N/A` and never a misleading `0.00`
- **Line terminator**: LF (`\n`)

## Column specification

| # | Column | Type | Meaning | Populated by |
|---|--------|------|---------|--------------|
| 1 | `transaction_type` | String | `Capital Gain`, `Lot`, `Wash Sale Reintegration`, `Dividend`, `Interest`, `FX Gain/Loss`, `FX Borrowed (review)`, `Fee`, `Stock Grant`, `Corporate Action` | all |
| 2 | `transaction_date` | Date | Fecha de transmisión / payment date; acquisition date on a `Lot` row | all |
| 3 | `settle_date` | Date | Settlement date | capital gains, dividends, interest, fees |
| 4 | `symbol` | String | Ticker, `CASH` for interest, currency code for FX | most |
| 5 | `isin` | String | ISIN when the statement carries one | capital gains, reintegrations, dividends |
| 6 | `description` | String | Free text | most |
| 7 | `quantity` | Decimal | Shares disposed of (sale row), consumed (lot row), or **vested** (stock-grant row — the column is reused) | capital gains, lots, stock grants |
| 8 | `cost_eur` | Decimal | Acquisition cost **before** actualization, commissions included | capital gains, lots |
| 9 | `coefficient` | Decimal (3 dp) | NF 3/2014 art. 45.2 coefficient for that lot's acquisition year; `1.000` under Común. Empty on the sale row — the coefficient is a per-lot figure and a single sale can consume lots of several vintages | lots |
| 10 | `actualized_cost_eur` | Decimal | `cost_eur × coefficient` | capital gains, lots |
| 11 | `proceeds_eur` | Decimal | Sale proceeds net of the sell-side commission | capital gains, lots |
| 12 | `gross_amount_eur` | Decimal | Gross dividend / interest / fee amount, and the **vest-date value** on a stock-grant row (the column is reused; empty when the statement carries no per-share FMV) | dividends, interest, fees, stock grants |
| 13 | `gain_loss_eur` | Decimal | Fiscal result: `proceeds_eur − actualized_cost_eur`; on a reintegration row, the released loss as a **negative** amount | capital gains, lots, reintegrations, FX |
| 14 | `deferred_loss_eur` | Decimal | Portion of a loss deferred under the valores-homogéneos rule, as a positive magnitude | capital gains |
| 15 | `integrable_amount_eur` | Decimal | What actually enters the savings base this year: `gain_loss_eur + deferred_loss_eur` | capital gains, reintegrations, FX |
| 16 | `foreign_tax_eur` | Decimal | Tax actually withheld at source | dividends |
| 17 | `treaty_capped_credit_eur` | Decimal | Withholding capped at the treaty rate on the **full** gross — what a reclaim from the source state is measured against, **informational per row** (see below) | dividends |
| 18 | `savings_group` | String | `RCM` (rendimientos del capital mobiliario) or `GyP` (ganancias y pérdidas patrimoniales); empty when the row enters neither | most |
| 19 | `notes` | String | Caveats, the per-lot actualization arithmetic, regime-specific deductibility notes, and on a capital-gain row any window-boundary review for that sale. Several notes on one row are joined with ` · ` | optional |

### Per-row figures that are not per-row taxes

Unlike the German statement, no row carries a tax figure. The Spanish savings scale is progressive
and applies to the year's whole base after group compensation, so a per-row tax would be arithmetic
nobody can add up. The same applies to `treaty_capped_credit_eur`: the deducción por doble
imposición internacional (NF 3/2014 art. 91 / LIRPF art. 80) is capped by the **average savings
rate**, which only exists once the whole year is known.

The column is not the credit's first limb either, and does not sum to it. It is measured on the full
gross of each payment, because that is what an over-withholding reclaim from the source state is
measured against. The credit's first limb is measured on the slice of each payment Spain actually
taxes, so wherever the Gipuzkoa €1,500 exemption applies the two diverge; only
`SUMMARY_FOREIGN_TAX_CREDIT` is the figure that goes on the return.

## Summary section

Two `#` preamble lines come first, saying that the cuota is computed once on the year's base after
group compensation and is not summed from the rows. Then the block's own header,
`summary_key,label,value_eur`. Keys are stable; labels are prose and may change.

`SUMMARY_REGIME` is the one row whose value column is not a EUR amount: its label is the regime and
its value is the tax year. `SUMMARY_AVERAGE_SAVINGS_RATE` is the other exception — a ratio, printed
at full precision.

| Key | Meaning |
|---|---|
| `SUMMARY_REGIME` | Regime (label) and tax year (value) |
| `SUMMARY_RCM_DIVIDENDS` | Gross dividends |
| `SUMMARY_RCM_DIVIDEND_EXEMPTION` | Dividends exempt under the Gipuzkoa €1,500 relief (NF 3/2014 art. 9.24); always 0 under Común |
| `SUMMARY_RCM_INTEREST` | Interest **received**, net of any reversal of interest credited earlier |
| `SUMMARY_RCM_INTEREST_PAID` | Interest paid on a borrowed balance, as a positive magnitude. Reported only — it never reduces the RCM result |
| `SUMMARY_RCM_DEDUCTIBLE_FEES` | Custody and administration fees that reduce the RCM result, after any regime ceiling (zero under Gipuzkoa) |
| `SUMMARY_RCM_FEE_CAP` | The 3%-of-non-exempt-gross ceiling of TRLFIRPF art. 32.1.a. **Navarra only** — the row is absent under the other two regimes, which have no ceiling |
| `SUMMARY_RCM_FEES_OVER_CAP` | Qualifying fees the ceiling disallowed. **Navarra only**; disjoint from `SUMMARY_INFORMATIONAL_FEES`, which carries fees that never qualified |
| `SUMMARY_RCM_NET` | `dividends + interest − deductible fees − exemption` |
| `SUMMARY_GYP_CAPITAL_GAINS` | Integrable result of the year's disposals |
| `SUMMARY_GYP_FX` | Realized foreign-currency results on a **held** balance |
| `SUMMARY_GYP_SMALL_DISPOSALS_EXEMPTION` | Transmission gains exempted under TRLFIRPF art. 39.5.d. **Navarra only**, and emitted only when non-zero |
| `SUMMARY_GYP_DEFERRED` | Loss this year's disposals deferred under the valores-homogéneos rule, as a positive magnitude |
| `SUMMARY_GYP_REINTEGRATED` | Deferred loss this year's disposals released, as a positive magnitude |
| `SUMMARY_GYP_NET` | `capital gains + FX − reintegrated − small-disposals exemption` |
| `SUMMARY_RCM_LOSSES_APPLIED` / `SUMMARY_GYP_LOSSES_APPLIED` | Prior-year balances consumed **inside their own group** this year: Fase 2ª-1º under Común and Gipuzkoa, TRLFIRPF art. 54.2's own-group absorption under Navarra |
| `SUMMARY_CROSS_OFFSET_RCM_TO_GYP` / `_GYP_TO_RCM` | Current-year cross-group offset: Fase 1ª under Común, TRLFIRPF art. 54.2's own cross under Navarra; always 0 under Gipuzkoa |
| `SUMMARY_PRIOR_CROSS_OFFSET_RCM_TO_GYP` / `_GYP_TO_RCM` | Prior-year balance crossed into the other group: Fase 2ª-2º under Común, and under Navarra the part of the cross that only the broad reading of art. 54.2 allows |

The label on all **six** rows of the compensation block is **per regime**, because the amount comes
from a different statute in each. Común and Gipuzkoa carry the AEAT numbering (`fase 2ª-1º`,
`fase 1ª (25% — sólo Territorio Común)` and `fase 2ª-2º (saldos de ejercicios anteriores)`); Navarra
carries TRLFIRPF art. 54.2's own, where letra a) is the capital-mobiliario group and letra b) the
transmisiones one, so every RCM row cites `art. 54.2.a` and every ganancias row `art. 54.2.b`. The
own-group rows name the statute's two conditions on that absorption — it happens only when the
group's own result is positive and it cannot drive that result below zero — and the cross rows name
the 25% of the *other* letra's positive after that letra absorbed its own carryforwards. Gipuzkoa's
cross rows are structurally zero — it integrates the groups "exclusivamente entre sí" — and keep the
state wording rather than earning a third vocabulary for a row that cannot fire. The keys never
change; only the label cell does.
| `SUMMARY_RCM_TAXABLE` / `SUMMARY_GYP_TAXABLE` | Each group after compensation |
| `SUMMARY_SAVINGS_BASE` | Base liquidable del ahorro |
| `SUMMARY_SAVINGS_QUOTA` | Cuota íntegra del ahorro |
| `SUMMARY_AVERAGE_SAVINGS_RATE` | Tipo medio del ahorro — a ratio, not a EUR amount, printed at full precision |
| `SUMMARY_FOREIGN_WITHHOLDING` | Tax actually withheld abroad |
| `SUMMARY_FOREIGN_TAXABLE_INCOME` | Foreign income as it reaches the base liquidable: the rate limb's base (TEAC RG 00/08643/2023) |
| `SUMMARY_FOREIGN_TAX_CREDIT` | What is actually creditable — the figure that goes on the return |
| `SUMMARY_NET_TAX_DUE` | `max(0, quota − credit)` |
| `SUMMARY_INFORMATIONAL_FEES` | Fees reported but not deducted |
| `SUMMARY_FX_BORROWED_REVIEW` | Borrowed-balance FX results excluded from the base |
| `CARRYFORWARD_<GROUP>_<YEAR>` | Pending negative balance to put in next year's config, by origin year |
| `CARRYFORWARD_<GROUP>_EXPIRED` | Balance that ran out of its four-year window this year |
| `DEFERRED_LOSS_<n>` | One surviving wash-sale block: symbol, blocked quantity, sale date, loss |
| `WASH_SALE_WINDOW_OPEN` | One loss whose repurchase window reaches past the statement's last date |
| `WASH_SALE_VENUE_REVIEW` | One loss whose deferral turns on which of the statute's two windows the listing venue takes: the value is what the one-year limb would defer on top of what was deferred |
| `SHORT_POSITION` | One open short position and its quantity |

`<GROUP>` is `RCM` or `GYP`. The `CARRYFORWARD_*` rows sit under a
`# SALDOS NEGATIVOS PENDIENTES` banner and the `DEFERRED_LOSS_*` rows under a
`# PÉRDIDAS DIFERIDAS PENDIENTES` banner; each block, banner included, is omitted when it would be
empty. `WASH_SALE_WINDOW_OPEN` and `SHORT_POSITION` sit under their own warning banners at the end of
the file.

### Reporting the two prior-year compensation steps disjointly

`SUMMARY_RCM_LOSSES_APPLIED` carries only what Fase 2ª-1º applied inside the RCM group;
`SUMMARY_PRIOR_CROSS_OFFSET_RCM_TO_GYP` carries only what Fase 2ª-2º crossed out of it. The two are
**disjoint**, and their sum is the prior-year RCM balance the year consumed. Printing the ledger
total in both rows would show the crossed amount twice, and a filer transcribing both would claim it
twice. The ganancias pair works the same way.

## Modelo box mapping

A `# MODELO` banner introduces the box mapping, followed by `MODELO_109_*` (Gipuzkoa),
`MODELO_100_*` (Común) or `MODELO_F93_*` (Navarra) rows.

Gipuzkoa publishes no blank numbered form — the return is generated by Zergabidea — so the numbers
are read off the Hacienda Foral "Propuesta de autoliquidación" specimens, one per ejercicio
(`gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/<ejercicio>/propuesta-autoliquidacion`, retrieved
2026-08-11). A `#` preamble names that source in the file itself.

**Two sheets, two numbering spaces.** The keys say which sheet a casilla belongs to:
`MODELO_109_HOJA_<n>` for the Hoja de liquidación and `MODELO_109_ANEXO3_<n>` for Anexo 3. They must
never be cross-read: Hoja 28 is the rendimiento neto del capital mobiliario while Anexo 4's own 28 is
ganancias. Común is different again — `MODELO_100_<n>`, no sheet segment. Match on the prefix, not on
a shared pattern.

Hoja de liquidación rows, stable from ejercicio 2019 through 2025:

| Key | Casilla | Row |
|---|---|---|
| `MODELO_109_HOJA_28` | 28 | Rendimiento neto del capital mobiliario (base del ahorro) |
| `MODELO_109_HOJA_29` | 29 | Compensación de rendimientos negativos de ejercicios anteriores |
| `MODELO_109_HOJA_30` | 30 | Ganancias y pérdidas patrimoniales por transmisiones |
| `MODELO_109_HOJA_31` | 31 | Compensación de pérdidas de ejercicios anteriores |
| `MODELO_109_HOJA_33` | 33 | Base liquidable del ahorro |
| `MODELO_109_HOJA_38` | 38 | Cuota íntegra del ahorro |

The deduction block was renumbered by NF 1/2025, so two rows move with the filing year:

| Row | Ejercicio ≤ 2024 | Ejercicio ≥ 2025 |
|---|---|---|
| Deducción por doble imposición internacional | `MODELO_109_HOJA_60` | `MODELO_109_HOJA_70` |
| Cuota líquida del ahorro | `MODELO_109_HOJA_64` | `MODELO_109_HOJA_74` |
| Pagos a cuenta del capital mobiliario (named in the withholding warning, never emitted) | 67 | 77 |

Anexo 3 rows (`MODELO_109_ANEXO3_06+16` rendimientos íntegros, `MODELO_109_ANEXO3_17` gastos) are
emitted for **ejercicios up to 2024 only**: those internals are confirmed through 2024 and were
renumbered in 2025. From ejercicio 2025 a `# WARNING:` replaces them. A filing year later than the
last published specimen (2026 onwards) gets that specimen's layout under a `# WARNING:` saying so.

No casilla is emitted without a number any more.

Territorio Común rows carry the ejercicio-2025 numbers from Anexo I of the Orden HAC/277/2026
consultation draft: `MODELO_100_0027` interest, `0029` dividends, `0037` gastos de administración y
depósito, `0326_0340` ganancias por transmisión de acciones cotizadas, `0460` base liquidable del
ahorro, `0588` deducción por doble imposición internacional.

Navarra rows carry the ejercicio-2025 numbers, read from the fully numbered form the Boletín Oficial
de Navarra publishes as Anexo I of the campaign's Orden Foral (Orden Foral 24/2026, BON nº 66 of
07-04-2026, retrieved 2026-08-12). The key is `MODELO_F93_<casilla>`; where the form repeats a figure
in its own summary the casilla segment carries both (`815 (= 524)`, `829 (= 527)`), and where the
figure is entered per transmission it names the block (`1658-1672`).

| Key | Row |
|---|---|
| `MODELO_F93_031` | Dividendos y participación en beneficios (art. 28.a y b) — importe íntegro |
| `MODELO_F93_037` | Intereses de cuentas y otros rendimientos por cesión de capitales propios |
| `MODELO_F93_047` | Gastos de administración y depósito, after the 3% ceiling |
| `MODELO_F93_050` | Rendimiento neto del capital mobiliario (`= 031 + 037 − 047`) |
| `MODELO_F93_706` | Incremento o disminución de la parte especial del ahorro por transmisiones |
| `MODELO_F93_1658-1672` | Incremento exento, otros supuestos — the art. 39.5.d relief, one cell per transmission. Emitted only when non-zero |
| `MODELO_F93_8808` / `809` / `8815` / `8809` | Apartado H1: positive transmissions saldo, own-group compensation, RCM losses crossed in, and the net (`8809 = 8808 − 809 − 810 − 8815`) |
| `MODELO_F93_8810` / `8825` / `8805` / `8840` | Apartado H2, the RCM mirror (`8840 = 8810 − 8825 − 8835 − 8805`) |
| `MODELO_F93_8816` | Apartado H3, saldo negativo procedente de transmisiones, as a **positive magnitude** — the row carries `max(0, −gyp_net)`. Emitted only when the transmissions result is negative, i.e. when H3 replaces H1 |
| `MODELO_F93_8850` | Apartado H4, saldo negativo del capital mobiliario, as a **positive magnitude** — the row carries `max(0, −rcm_net)`, not a negative number. Emitted only when the RCM result is negative |
| `MODELO_F93_8841` | Total parte especial del ahorro (`= 8809 + 8840`) |
| `MODELO_F93_815 (= 524)` | Base liquidable especial del ahorro |
| `MODELO_F93_829 (= 527)` | Cuota íntegra especial del ahorro |
| `MODELO_F93_572` | Deducción por doble imposición internacional. **Not** 613, which is the transparencia fiscal internacional deduction |
| `MODELO_F93_818` / `MODELO_F93_8875` | Saldos negativos a compensar en los ejercicios siguientes, transmissions and RCM. Emitted only when non-zero; the per-year cells are labelled on the form itself |

Every `MODELO_F93_*` row stays inside the summary block's three fields. (`MODELO_109_*` rows are the
standing exception: their `(hoja, casilla <n>)` label suffix has carried a comma since the first
release, so those rows are four fields wide.)

`810` and `8835` are the joint-return rows and are never emitted: the tool computes an individual
return, so they are always zero. Casillas 569 (cuota líquida), 576 and 582 are **not** emitted
either — they net the whole return, including the general part this tool does not compute.

A filing year other than 2025 keeps these numbers under a `# WARNING:` saying either that the
earlier form's own numbering was not checked against a specimen, or that no form is published for
that ejercicio yet.

**No retenciones row is emitted for any form.** Foreign withholding is not a Spanish retención; it
is relieved only through the deducción por doble imposición internacional. A `# WARNING:` names the
casilla the figure does not belong in (109: the year's pagos-a-cuenta casilla, 67 through ejercicio
2024 and 77 from 2025, plus Anexo 3's `07+22` while that sheet is emitted; 100: `0597`), because
entering it in both places claims the same tax twice. That warning is emitted **only when the year's foreign withholding is
non-zero** — there is no trap to name otherwise.

## Warning lines

Comment lines start with `#`. A consumer must skip them.

| Banner (opening words) | Emitted when |
|---|---|
| `# The cuota is computed once on the year's base after group compensation, NOT summed…` | Always, immediately before the summary header |
| `# SALDOS NEGATIVOS PENDIENTES — put these in next year's taxes.spain.loss_carryforward,…` | Either group has a pending balance to carry, or something expired this year |
| `# PÉRDIDAS DIFERIDAS PENDIENTES — put these in next year's taxes.spain.deferred_losses.` | A wash-sale deferral is still blocked at the end of the filing year |
| `# MODELO 109 — autoliquidación del IRPF de Gipuzkoa` | Regime is `gipuzkoa` |
| `# The numbers below are read off the Hacienda Foral "Propuesta de autoliquidación"…` | Regime is `gipuzkoa`, always, before the box rows |
| `# WARNING: no form is published for ejercicio <year> yet — it is filed in <year+1>.` | Regime is `gipuzkoa` and the filing year is later than the last published specimen (2026 onwards) |
| `# WARNING: the rendimientos íntegros / gastos breakdown lives in Anexo 3,…` | Regime is `gipuzkoa` and the filing year is 2025 or later, in place of the Anexo 3 rows |
| `# MODELO 100 — declaración del IRPF (AEAT)` | Regime is `comun` |
| `# MODELO F-93 — declaración del IRPF de la Comunidad Foral de Navarra` | Regime is `navarra` |
| `# WARNING: the ejercicio-<year> form has the same structure but its own…` | Regime is `navarra` and the filing year is earlier than 2025 |
| `# WARNING: no form is published for ejercicio <year> yet — it is filed in <year+1>.` | Regime is `navarra` and the filing year is later than 2025 |
| `# WARNING: The year's disposals consumed FIFO lots acquired before 31 December 1994…` | Regime is `navarra` and a FIFO lot consumed this year was acquired before 31-12-1994, so TRLFIRPF DT 7.ª's abatement applies and the tool does not compute it |
| `# WARNING: €<x> of transmission gains was exempted under TRLFIRPF art. 39.5.d…` | The Navarra small-disposals exemption applied |
| `# WARNING: The year's securities transmissions came to €<x>, under the €3,000…` | The exemption was withheld because a conversion on a **held** balance made the global transmission amount unmeasurable. A year with no securities transmissions at all is silent: the relief would have been zero regardless, so there is nothing to report as withheld |
| `# WARNING: €<x> of otherwise deductible custody and administration fees was NOT deducted…` | The Navarra 3% ceiling disallowed part of the year's qualifying fees |
| `# WARNING: €<x> of prior-year negative savings-base saldos was set against the other group…` | Under Navarra, a carried saldo crossed into the other group |
| `# WARNING: these are the ejercicio-2025 box numbers, read from Anexo I of the…` | Regime is `comun`, always, before the box rows |
| `# WARNING: the €<x> withheld abroad is NOT a Spanish retención and does NOT go in…` | The year's foreign withholding is **non-zero** (under whichever regime's box block) |
| `# WARNING: valores-homogéneos window still open when the statement ends. A…` | A loss sale's +2-month repurchase window reaches beyond the statement's last date, so a repurchase that would defer the loss cannot be seen yet |
| `# WARNING: the deduction below turns on which valores-homogéneos window the listing…` | A filing-year loss had homogeneous securities bought back outside the two months but inside the year, on a venue no in-force MiFID II equivalence decision covers (or one the statement does not name) |
| `# WARNING: €<x> of dividends were exempted under NF 3/2014 art. 9.24. The exemption…` | The Gipuzkoa exemption was applied to anything (it cannot tell a fund distribution from a company dividend) |
| `# WARNING: sales in <years> were not tested for the valores-homogéneos rule: no…` | The statement contains disposals **up to the filing year** in a year with no shipped actualization table. Disposals in later years are excluded: they belong to the next return and can only release deferrals here |
| `# SHORT POSITIONS — no tax is computed for these; they need manual review.` | Open short positions exist |

There is no FX-borrowed banner in the CSV. Borrowed-balance results appear as
`FX Borrowed (review)` transaction rows and as the `SUMMARY_FX_BORROWED_REVIEW` total; the prose
warning about them is printed on the console only.

## Validation rules

- Header row first; column names and order exactly as specified
- Every transaction and lot row has exactly 19 fields under a quote-aware parser
- Decimal amounts: exactly 2 decimal places, rounded half away from zero (never truncated), except
  `coefficient` (3 dp, **also** half away from zero) and `SUMMARY_AVERAGE_SAVINGS_RATE` (a ratio,
  printed at full precision because it is the multiplier behind the credit cap)
- Dates: ISO 8601 (`YYYY-MM-DD`)
- No thousands separators — a comma inside a number would break the delimiter
- Negative amounts carry a `-` prefix; currency symbols are never included (all amounts EUR)

## Compatibility

- Excel / LibreOffice Calc / Google Sheets: compatible
- `pandas`: `pd.read_csv(path, comment="#", decimal=".")` reads the transaction block; the summary
  block needs a second pass because it has its own header — and that pass needs `comment="#"` too,
  since the summary preamble, the carry-forward and deferred-loss banners, the Modelo block and the
  warnings are all `#` lines inside it
- Spanish tax software: manual entry only

## Versioning

- **Version 1.3**: Navarra round (2026-08-12) — the `MODELO_F93_<casilla>` key family for the third
  regime, two Navarra-only summary rows for the art. 32.1.a fee ceiling
  (`SUMMARY_RCM_FEE_CAP`, `SUMMARY_RCM_FEES_OVER_CAP`), one for the art. 39.5.d exemption
  (`SUMMARY_GYP_SMALL_DISPOSALS_EXEMPTION`), and five warning banners (DT 7.ª abatement not
  computed, small-disposals exemption applied, exemption withheld for an unmeasurable conversion,
  fee ceiling bound, carried saldo crossed). `SUMMARY_GYP_NET` now subtracts the exemption.
  The six labels of the compensation block (`SUMMARY_*_LOSSES_APPLIED` and
  `SUMMARY_*CROSS_OFFSET_*`) became per-regime: Común and Gipuzkoa keep the AEAT "fase" wording,
  Navarra names TRLFIRPF art. 54.2.a / 54.2.b
- **Version 1.2**: open-items round (2026-08-11) — Modelo 109 keys carry the sheet
  (`MODELO_109_HOJA_<n>` / `MODELO_109_ANEXO3_<n>`, replacing `MODELO_109_CASILLA_<n>`), the
  double-taxation and cuota-líquida casillas follow the ejercicio (60/64 through 2024, 70/74 from
  2025) so `MODELO_109_CASILLA_UNKNOWN` is gone, Anexo 3 rows stop after ejercicio 2024, the Hoja
  gains the rendimiento-neto and compensation rows, and two warning rows were added
  (`WASH_SALE_VENUE_REVIEW`, plus window-boundary notes on capital-gain rows)
- **Version 1.1**: brought in line with the emitted format (2026-08-11) — `Stock Grant` and
  `Corporate Action` row types, the reintegration row, the full summary-key list, disjoint
  compensation rows, per-regime warning banners, the `MODELO_109_CASILLA_<n>` / `MODELO_100_<n>` key
  asymmetry, and half-away-from-zero coefficients
- **Version 1.0**: initial implementation (2026-08-11)

## Related contracts

- German CSV output: `specs/001-germany-tax/contracts/csv-output.md`
- Configuration schema: `specs/002-spain-tax/plan.md`, "Config schema"

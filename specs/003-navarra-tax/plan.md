# Navarra Tax Regime — Implementation Plan

**Date**: 2026-08-12
**Branch**: `003-navarra-tax` (off `002-spain-tax` @ `9cf53cce`; stacked PR → `002-spain-tax`)
**Goal**: add **Navarra** (Comunidad Foral, Convenio Económico; IRPF per Decreto Foral Legislativo 4/2008 "TRLFIRPF") as the third `SpanishTaxRegime`, reusing the whole Spanish engine. The regime enum has 63 exhaustive non-test match sites across 9 files and all per-regime behavior resolves once in `SpanishTaxParams::resolve` — adding the variant makes the compiler enumerate every decision point.

Work exactly like the prior plans (`specs/002-spain-tax/*`): phases in order, failing test first, per-task gate (`cargo check` → `cargo test spain` → `./check`), one Conventional Commit per task with the given message, statuses updated in-place via `docs(plan)` commits, smallest diff. Baselines after the v8.4.0 rebase: **34** `parse_real` lib-test failures (empty private testdata submodule) and **3** upstream clippy errors (`statistics.rs`, `xls/table.rs`, `rebalancing.rs`) are pre-existing and untouchable; `cargo test spain --lib` currently **267**.

## Verified legal basis (sources retrieved 2026-08-12; consolidated texts saved in the session scratchpad: `trlfirpf.txt`, `regl.txt`, `notas/`, `F93_2025.pdf` under `/private/tmp/claude-501/-Users-alexandre-projects-alexsavio-investments/864d554e-7895-4b99-9187-48e9e68b5156/scratchpad/`)

| Dimension | Navarra rule | Source |
|---|---|---|
| Savings scale (**identical 2024, 2025, 2026**) | art. 60: 20% ≤ 6,000 / 22% ≤ 10,000 / 24% ≤ 15,000 / 26% ≤ 200,000 / 27% ≤ 300,000 / 28% above. The law publishes the cumulative cuota column — **golden vectors**: tax(6,000)=1,200 · tax(10,000)=2,080 · tax(15,000)=3,280 · tax(200,000)=51,380 · tax(300,000)=78,380. LF 17/2025 (2026 effects) does NOT touch art. 60 | lexnavarra r=29657 art. 60 (wording LF 36/2022, effects 1-1-2023; nota o=158) |
| Savings-base contents | RCM arts. 28/29/30.1/30.2/**30.3.e** + transmission gains (art. 54.1). Related-party excess interest (>3× equity, ≥25% participation) → general base — **doc note only** (not detectable from a broker statement; register entry) | art. 54.1 (LF 29/2014 + LF 16/2017) |
| Integration/compensation | art. 54.2 (LF 23/2015), a **third ordering**, distinct from both existing modes: per group (1) net the current year; (2) **only if that result is positive**, absorb **own-group prior-year saldos, oldest first**, "sin que en ningún caso el resultado de esta compensación pueda ser negativo" (floor 0) — a group whose current result is negative leaves its own prior saldos untouched; (3) **"Si el resultado fuese negativo, su importe se compensará con el saldo positivo resultante de la letra b) de este apartado, con el límite del 25 por 100 de dicho saldo positivo"** — the cap is 25% of the other group's result **after** that group absorbed its own carryforwards, not of its raw positive; (4) whatever negative is left carries 4 years "en el mismo orden establecido en los párrafos anteriores". Verified against the text 2026-08-12; see Appendix A §A.2 for the ambiguity in (4) and the reading implemented | art. 54.2–54.3; DA 48.ª (25% since 2018) |
| FIFO | art. 43.2 ("adquirió en primer lugar"); valores homogéneos defined in Reglamento art. 7 (same shape as RIRPF art. 8) | lexnavarra r=29657 / r=10615 |
| Wash sale | art. 39.6.f (listed, **2 months**, cites **MiFID II 2014/65/UE directly** — Navarra updated the reference, unlike the state) / 39.6.g (unlisted, 1 year); deferral "a medida que se transmitan los valores que permanezcan". Same venue-equivalence logic as the DGT doctrine already implemented; existing engine + venue warning apply unchanged | art. 39.6 (LF 36/2022); notas o=90/91 |
| Actualization coefficients | **None** (art. 41 never amended; nothing in law/reglamento/presupuestos) — like Común, coefficient = 1 | art. 41 |
| Pre-1994 lots | DT 7.ª abatement regime (pre-2007 gain slice reduced/exempted; **no €400k cap**, and DT 7.ª.3.a) needs the 2006 Impuesto sobre el Patrimonio value a broker statement cannot supply) — NOT computed. Its own trigger is "elementos patrimoniales adquiridos **antes de 31 de diciembre de 1994**", so the warning fires on `acquisition_date < 1994-12-31`, not on "before 1995": an acquisition **on** 31-12-1994 is outside the regime (N9) | DT 7.ª (LMV cite updated by LF 17/2025) |
| €1,500 dividend exemption | **Repealed** by LF 29/2014 (gone since 2015; old art. 7.v). Navarra = Común here, NOT Gipuzkoa | nota o=1 |
| Custody/admin fees | Deductible from savings RCM **with a cap**: "los gastos de administración y depósito de valores negociables, **con el límite del 3 por 100 de los ingresos íntegros, que no hayan resultado exentos, procedentes de dichos valores**" (art. 32.1.a; second paragraph excludes discretionary portfolio management, same exclusion the classifier already applies). Reuses the doctrine-based classifier; the cap is Navarra-only. Cap base = non-exempt gross income **from the securities**, which in this tool's data means dividends — broker cash-account interest is a cesión de capitales propios, not income from a valor negociable (Appendix A §A.3) | art. 32.1.a |
| Foreign WHT credit | art. 67: lesser of (a) tax paid abroad, (b) **tipo medio efectivo** × foreign-taxed base-liquidable slice; tipo = **cuota líquida / base liquidable × 100**, split general vs savings, **two decimals** (art. 67.2). In our scope savings cuota líquida = savings cuota íntegra (no savings-side deductions are modelled) — implement with the existing rounded-average-rate machinery and record the líquida-vs-íntegra proxy in the register with a `TODO(verify)`. Treaty cap: no express clause; Convenio Económico art. 2.1.c binds Navarra to the treaties — keep the per-dividend treaty-capped limb | art. 67; Ley 28/1990 art. 2.1.c |
| **€3,000 small-disposals exemption** | art. 39.5.d verbatim: "Estarán exentos del impuesto los incrementos de patrimonio que se pongan de manifiesto: … d) Con ocasión de transmisiones onerosas en las que concurran los siguientes requisitos: **1.º Que el importe global de las citadas transmisiones no exceda de 3.000 euros durante el año natural. 2.º Que la cuantía gravable del incremento de patrimonio no exceda del 50 por 100 del importe global de la transmisión. En los supuestos en los que la cuantía gravable del incremento de patrimonio exceda del referido porcentaje únicamente se someterá a gravamen el citado exceso.**" Unique to Navarra; both conditions are year-global. Mechanics, edges and the residual ambiguity are pinned in Appendix A §A.4 | art. 39.5.d |
| Minima | Personal/family minima are quota credits (art. 62.9) — the savings quota is exactly `scale(base)`; nothing to model | art. 62.9 |
| Tax year | Calendar year, accrual 31-Dec (art. 76); FX gains on conversion (art. 78.7, same as implemented) | arts. 76/78 |
| Form | **F-93**, approved per campaign by Orden Foral (FY2025: OF 24/2026, BON nº 66 07-04-2026; FY2024: OF 28/2025). The **BON Anexo I publishes the fully numbered official form** — FY2025 casillas verified from the specimen (`F93_2025.pdf`): dividends gross **031**, gastos admin y depósito **047**, RCM neto **050**, Spanish-withholdings total **030**→**579** (foreign WHT NEVER there), transfers savings-part total **706**/Anexo-1 blocks, compensation blocks **8808/809/8815/8809** and **8810/8825/8805/8840** (carryforward boxes are **year-labeled**: 8091–8094, 8880/8820s), TOTAL **8841** → base **815** (= summary **524**) → cuota **829** (= **527**), **DDI = 572** (do NOT use 613 — that is international fiscal transparency), cuota líquida 569, resultado 582. FY2024 layout unverified (same structure, box-level identity unconfirmed); 2026 unpublished → emit FY2025 layout labeled, warn | BON Anexo I PDF (URL pattern `O{YY}-{NNN}_AnexoI_Modelo_F-93.pdf`) |
| Rounding | No stated amount rule (cents like the state); **rates two decimals** (arts. 59.2/67.2) — existing `format_eur` + rounded-rate machinery fit | OF 24/2026 full-text grep |
| Informational | Navarra files **foral Modelo 720** (OF 80/2013, mod. OF 58/2026) and its **own crypto Modelo 721** (OF 29/2023), both 1 Jan–31 Mar — doc notes only | navarra.es trámites |
| Out of scope (doc + register) | DT 7.ª abatement computation; exit tax DA 46.ª; fund-traspaso deferral history (LF 19/2021 + DT 29.ª grandfathering); joint-return saldo sharing (art. 74); related-party interest reclassification; debt-instrument (bond) transfer results being RCM not GyP (art. 29 — also true in state law). **Corrected in N9 against the code**: the shared IB parser already drops every non-`STK` `assetCategory` with its own "Skipping non-stock instrument" warning, so a bond disposal never reaches the Spanish processor and is not taxed as GyP — it is absent entirely. No new runtime warning is warranted; the fact is a register entry instead | — |

## Design

- `SpanishTaxRegime::Navarra` (serde `navarra`). `SpanishTaxParams::resolve` gains: Navarra scale table; `applies_coefficients = false`; `custody_fees_deductible = true` **plus new `custody_fee_cap_fraction: Option<Decimal>`** (None for Común, `Some(0.03)` for Navarra, field unused by Gipuzkoa); `dividend_exemption_limit = 0`; compensation mode = `Navarra`; small-disposals exemption enabled; form mapping = F-93.
- Compensation becomes a three-mode enum (`CrossOffset::None | AeatTwoPhase | NavarraOrdered`) rather than boolean flags — `compensate_savings_base` grows the Navarra arm implementing the art. 54.2 literal order; Gipuzkoa and Común outputs must stay bit-identical (assert with the existing fixtures).
- Config: `regime: navarra` accepted; `taxes.spain.coefficients` under Navarra → validation error naming the regime (coefficients don't exist there); everything else (`loss_carryforward` year-labeled ledgers, `deferred_losses`) works unchanged.
- The wash-sale engine, venue/boundary warnings, treaty-capped credit limb, FX FIFO, and simulate-sell are regime-parameterized already — they must work for Navarra through `SpanishTaxParams` with no engine edits (verify via fixtures, don't restructure).

## Tasks

- **N1 — Statute verification from the saved texts.** Status: DONE (`8ded6bbd`) — read art. 39.5.d, art. 54.2, art. 32.1.a, art. 60 and art. 67 verbatim from `trlfirpf.txt`; pin the €3,000-exemption mechanics and the compensation ordering in this file (update the tables above in-place if the summaries differ from the text); write Appendix A worked numbers for every planned fixture (scale golden vectors incl. mid-bracket + average rate; a compensation case exercising the Navarra ordering vs the Común answer on the same inputs; a 3%-cap fee case; a €3,000-exemption case; a credit case at the Navarra rate).
  Commit: `docs(navarra-tax): pin the statutory mechanics and worked numbers`
- **N2 — Regime wiring.** Status: DONE (`e8ffc1dc`) — `SpanishTaxRegime::Navarra`, `SpanishTaxParams::resolve`, compiler-driven sweep of all match sites, config acceptance + the coefficients-under-Navarra rejection, `localities::spain` approximation regime arm. Failing test: resolve for (Navarra, 2025) yields the right params; config with coefficients + navarra errors naming both.
  Commit: `feat(navarra-tax): add the Navarra regime with resolved parameters`
- **N3 — Scale.** Status: DONE (`70cb8a9b`) — art. 60 table (one table, three years), golden cuota vectors, `average_rate` checks.
  Commit: `feat(navarra-tax): add the Navarra savings scale`
- **N4 — Fixtures harness.** Status: DONE (`83a80f42`) — rerun the regime-independent existing fixtures (`fifo`, `income`, `fx_gain`, wash-sale set) under `regime: navarra` asserting Appendix A numbers (coefficient 1, no exemption, fees per N6 once landed).
  Commit: `test(navarra-tax): exercise the shared fixtures under the Navarra regime`
- **N5 — Compensation mode.** Status: DONE (`8186da8a`) — the `NavarraOrdered` arm per art. 54.2 literal reading (own-group carryforwards first; current-year negative crosses at 25% of the other group's post-carryforward positive; carried saldos repeat the order in later years and may cross — emit a warning naming the open nuance and the euro amount whenever a **carried** saldo crosses, register entry). Fixture: the Appendix A case where Navarra's answer ≠ Común's two-phase answer on identical inputs; bit-identical assertions for the other two regimes.
  Commit: `feat(navarra-tax): apply the art. 54.2 compensation ordering`
- **N6 — Fee cap.** Status: DONE (`50ea3ad3`) — 3%-of-non-exempt-gross cap on deductible custody/admin fees (classifier unchanged; cap applied at totals level with a capped-amount note row). Fixture: fees above the cap → deduction clamped, remainder informational.
  Commit: `feat(navarra-tax): cap deductible custody fees at three percent of gross`
- **N7 — Small-disposals exemption.** Status: DONE (`3748b5bc`) — art. 39.5.d per N1's pinned mechanics: detect year-total onerous-transmission proceeds ≤ €3,000, apply the exemption to qualifying gains, keep losses/wash-sale interactions correct (a wash-deferred loss is not proceeds; document the interaction), report an `ExemptionEntry`/summary row + warning explaining what was exempted. Fixture: small year fully under the threshold; boundary year just above (no exemption).
  Commit: `feat(navarra-tax): apply the small-disposals exemption`
- **N8 — F-93 mapping + credit.** Status: DONE (`064ed237`) — per-campaign casilla tables (FY2025 verified per the specimen; FY2024 same-labels flagged unverified; 2026 = FY2025 layout + warning), keys `MODELO_F93_<box>` with the established sheet-labeling discipline; DDI 572; foreign WHT excluded from 030/579 with the standard warning; credit at the Navarra rounded savings rate with the líquida-vs-íntegra proxy `TODO(verify)`. Update the CSV contract (version bump) and console summary.
  Commit: `feat(navarra-tax): emit the F-93 statement mapping`
- **N9 — Warnings + register.** Status: DONE (`af54fd86`) — new warning: FIFO lot acquired **before 31-12-1994** detected (DT 7.ª abatement not computed — names the lots and dates; Navarra only, since the Gipuzkoa and state equivalents were not researched this round). The planned bond/debt-instrument warning was **dropped after checking the code**: non-`STK` instruments are discarded by the shared IB parser before the Spanish processor sees them, so there is nothing to warn about at this layer — it became a register entry instead. Carried-saldo cross-offset (from N5) and the F-93 ejercicio caveats (from N8) are already emitted. Register entries in `docs/spain-taxes.md` for each, plus doc notes: foral 720/721, traspaso grandfathering, exit tax, related-party interest, joint returns.
  Commit: `feat(navarra-tax): warn on unmodelled Navarra rules and register them`
- **N10 — Docs, gate, close-out.** Status: DONE (`40d1eae0`) — `docs/spain-taxes.md` Navarra sections (config, scale table with sources, the three-regime differences table from this plan, fee cap, exemption, F-93 usage) + README + `config-example.yaml`; full gate; smoke test re-run under `regime: gipuzkoa` (must be bit-identical — Navarra work must not move the user's own figures) and a second run with a scratch `regime: navarra` config on the same statement, figures explained against Appendix A reasoning; close-out appended here.
  Commit: `docs(navarra-tax): document the Navarra regime and close the round`

## Verification
Per task: its failing test; `cargo test spain --lib`; `./check`. End-to-end: all three regimes on the shared fixtures produce their Appendix A numbers; the user's real-statement Gipuzkoa run is unchanged; simulate-sell works under Navarra. After the round: adversarial review + consistency audit (separate session, established protocol) before the PR is created.

## Appendix A — worked numbers (the acceptance contract)

Written in N1 from the statute text itself, before any code. Every later fixture asserts a number
from here; if an implementation disagrees with Appendix A, the implementation is wrong until this
appendix is re-derived and amended in its own `docs(plan)` commit.

Shared fixture conventions, unchanged from the 002 round: the test converter is a flat 0.9 EUR/USD
(1.0 after a named revaluation date), commissions are zero, and the filing year is 2026 unless a
case says otherwise.

### A.1 — The savings scale (art. 60)

One table, all three shipped years (2024, 2025, 2026): LF 17/2025 does not touch art. 60.

| Base liquidable hasta | Cuota íntegra | Resto base hasta | Tipo |
|---|---|---|---|
| — | — | 6.000 | 20% |
| 6.000 | 1.200 | 4.000 | 22% |
| 10.000 | 2.080 | 5.000 | 24% |
| 15.000 | 3.280 | 185.000 | 26% |
| 200.000 | 51.380 | 100.000 | 27% |
| 300.000 | 78.380 | Resto | 28% |

**Golden vectors** (the statute's own cuota column, so these are not hand-derived):
`tax(6 000)=1 200` · `tax(10 000)=2 080` · `tax(15 000)=3 280` · `tax(200 000)=51 380` ·
`tax(300 000)=78 380`. Above the last threshold there is no published cuota:
`tax(400 000) = 78 380 + 100 000 × 28% = 106 380`.

**Mid-bracket vectors** (hand-derived, arithmetic shown):

| Base | Arithmetic | Cuota |
|---|---|---|
| 963 | 963 × 20% | 192.60 |
| 1 000 | 1 000 × 20% | 200.00 |
| 8 000 | 1 200 + 2 000 × 22% | 1 640.00 |
| 12 000 | 2 080 + 2 000 × 24% | 2 560.00 |
| 22 500 | 3 280 + 7 500 × 26% | 5 230.00 |
| 5 400 | 5 400 × 20% | 1 080.00 |
| 9 000 | 1 200 + 3 000 × 22% | 1 860.00 |
| 225 | 225 × 20% | 45.00 |
| 2 250 | 2 250 × 20% | 450.00 |

**Average rate** (art. 67.2 / 59.2: two decimals as a percentage, i.e. four as a fraction, rounded
half away from zero exactly as the existing `SavingsScale::average_rate` does):
`963 → 0.2000` · `8 000 → 1 640/8 000 = 0.2050` · `12 000 → 2 560/12 000 = 0.213333… → 0.2133` ·
`22 500 → 5 230/22 500 = 0.232444… → 0.2324` · `400 000 → 106 380/400 000 = 0.26595 → 0.2660`.
A non-positive base gives 0 for both.

### A.2 — Compensation ordering (art. 54.2)

The implemented order, per group, from the statute text quoted in the Verified-legal-basis table:

1. `cur` = sum of the group's own current-year items.
2. If `cur > 0`: absorb the group's own prior-year saldos oldest first, floored at 0; the unabsorbed
   remainder stays pending. If `cur ≤ 0`: the group's own prior-year saldos are **not** touched
   (the statute only opens that branch for a positive result) and stay pending in full.
3. Cross: the group's negative is set against **the other group's post-step-2 positive**, capped at
   25% of that post-step-2 figure.
4. What is left carries four years "en el mismo orden establecido en los párrafos anteriores".

**The residual ambiguity in step 4**, and the reading implemented. Step 3's trigger is worded
"Si **el resultado** fuese negativo", and "el resultado" is the current-period sum, so a strictly
narrow reading lets only a **current-year** negative cross. The carry sentence, however, sends the
surviving saldo into the following four years "en el mismo orden establecido en los párrafos
anteriores" — the order being both own-group absorption *and* the 25% cross. No HFN manual or
consulta was located that settles it. The tool implements the **broad** reading (a carried saldo may
also cross, at 25% of the other group's post-carryforward positive) because that is what "el mismo
orden" says on its face, and it names the euro amount in a warning whenever a *carried* saldo — as
opposed to a current-year negative — is what crossed. `TODO(verify)` in the register.

**Case 1 — the three regimes diverge on identical inputs.** Filing 2026; current RCM −800, current
ganancias +4 000, prior-year RCM saldo 500 (2024), prior-year ganancias saldo 2 800 (2024). These
are the AEAT manual's own numbers, so the Común answer is already pinned by the 002 round.

- **Gipuzkoa** (no cross at all): ganancias +4 000 absorbs its own 2 800 → 1 200. RCM −800 becomes a
  2026 saldo; the 500 @2024 survives. **Base 1 200.**
- **Común** (Fase 1ª → 2ª-1º → 2ª-2º, one 25% allowance measured on the *original* 4 000): cross 800,
  then 2 800, then 200 of the prior RCM saldo. **Base 200**, RCM ledger keeps 300 @2024.
- **Navarra**: letra a) RCM `cur = −800` is negative, so its own 500 @2024 is untouched. Letra b)
  ganancias `cur = +4 000` is positive, absorbs 2 800 → post = 1 200. Cross: min(800, 25% × 1 200 =
  **300**) = 300 → ganancias 900. RCM keeps 500 @2026 plus 500 @2024. **Base 900.**

Three regimes, three answers — 1 200 / 200 / 900 — from one set of inputs. This is the N5 acceptance
test.

**Case 2 — a carried saldo crosses (the warning case).** Current RCM +500, current ganancias +4 000,
prior-year RCM saldo 2 000 (2024), no prior ganancias saldo.

- **Navarra**: letra a) +500 absorbs 500 of the 2 000 → post 0, 1 500 @2024 pending. Letra b) +4 000,
  nothing to absorb → post 4 000. Under the implemented broad reading the pending 1 500 crosses at
  25% × 4 000 = **1 000** → ganancias 3 000, 500 @2024 still pending. **Base 3 000**, and the warning
  names the €1 000 that crossed. Under the narrow reading the base would be 4 000; the €1 000 gap is
  exactly the amount the warning is about.
- **Común** reaches the same 3 000 by a different route (Fase 2ª-1º uses 500, Fase 2ª-2º crosses
  1 000), and **Gipuzkoa** gives 4 000 with 1 500 @2024 carried.

**Case 3 — no carryforwards, so Navarra and Común agree.** With empty ledgers the two orderings only
differ in step 2, which does nothing, so the existing `cross_offset` fixture must give Navarra the
same base as Común: ganancias −9 000 against RCM +7 200 (dividends, no Navarra exemption) crosses
min(9 000, 25% × 7 200 = 1 800) → **base 5 400**, cuota `5 400 × 20% = 1 080.00`, 7 200 @2026 carried.
Stating this explicitly is the guard against "Navarra ≠ Común" being asserted where it must not be.

### A.3 — The 3% fee cap (art. 32.1.a)

`cap = 3% × (non-exempt gross income from the securities)`; `deductible = min(classified custody and
administration fees, cap)`; the excess is reported, never silently dropped. Cap base = gross dividend
income less any exempt slice (zero in Navarra). Broker cash-account interest is **excluded** from the
cap base: art. 29 income from a cesión de capitales propios is not "procedente de dichos valores".
The failure direction is a smaller cap, i.e. more tax — the tool's standing convention. `TODO(verify)`
in the register.

**`income` fixture under Navarra** (dividend $1 000 → €900 gross, $300 → €270 withheld; broker
interest $100 → €90; custody fee $50 → €45):

| Figure | Arithmetic | Value |
|---|---|---|
| Cap | 3% × 900 | 27.00 |
| Deductible fees | min(45, 27) | 27.00 |
| Disallowed by the cap | 45 − 27 | 18.00 |
| `rcm_net` | 900 + 90 − 27 | 963.00 |
| Base | — | 963.00 |
| Cuota | 963 × 20% | 192.60 |
| Average rate | 192.60 / 963 | 0.2000 |
| Treaty limb | min(270, 900 × 15%) | 135.00 |
| Rate limb | (900 − 27 × 900/990) × 0.2000 = 875.4545… × 0.2000 | 175.09 |
| Credit | lesser of the two | 135.00 |
| Net due | 192.60 − 135 | 57.60 |

Reference points on the same fixture: Común `rcm_net` 945 → cuota 179.55 → due 44.55; Gipuzkoa
exempts the whole €900 dividend, deducts nothing, `rcm_net` 90.

**Cap binds at zero.** The `cross_offset_rcm` fixture has a €3 600 custody fee and **no** dividends,
so the Navarra cap base is 0 → cap €0 → nothing deductible, €3 600 reported as disallowed,
`rcm_net = 0`, ganancias +9 000, **base 9 000**, cuota `1 200 + 3 000 × 22% = 1 860.00`. Común on the
same fixture deducts the full €3 600 and reaches a base of 6 750; Gipuzkoa deducts nothing and
reaches 9 000 at its own scale (1 725.00).

**Cap does not bind** (pure-function vector): gross dividends 10 000, fees 200 → cap 300 → deductible
200, disallowed 0.

### A.4 — The €3 000 small-disposals exemption (art. 39.5.d)

Pinned reading:

- `G` = "el importe global de las citadas transmisiones" = the year's total **proceeds** of onerous
  securities transmissions, gain-making and loss-making alike. 1.º measures the transmissions, not
  their results.
- `I` = "la cuantía gravable del incremento de patrimonio" = the sum of the year's **positive**
  integrable results from those transmissions. A loss is a *disminución*, not an *incremento*: it is
  outside the exemption and stays fully in the group.
- Condition 1.º: `0 < G ≤ 3 000`.
- Condition 2.º: `exempt = min(I, 0.50 × G)`, `taxed = I − exempt`. "No exceda" makes the boundary
  inclusive, so `I = 0.50 × G` exactly is still fully exempt.

Three edges pinned with it:

1. **Singular vs plural.** 2.º says "el importe global de **la transmisión**" where 1.º says "de las
   citadas transmisiones". The tool reads both as the same year-global figure: 1.º has already fixed
   "importe global" as the year total, and a per-disposal numerator against a global denominator is
   incoherent. Differs from a per-disposal reading only in a multi-disposal year. `TODO(verify)`.
2. **Wash sale.** A deferred loss does not change `G` (the transmission happened, at its proceeds)
   and does not enter `I` (a *disminución*, and a blocked one). A reintegrated loss likewise stays
   out of `I`.
3. **Foreign-currency conversions.** Art. 54.1.b makes them transmissions too, so their importe
   belongs in `G`, but the shared FX FIFO records only the realized result, never the converted
   principal. Counting securities proceeds alone would understate `G` and could hand the exemption to
   a year that does not qualify, so the exemption is **suppressed** for any year with an FX
   conversion realization, with a warning saying so. Failure direction: more tax.

**`small_disposal` fixture** — buy 10 AAPL @ $100 on 2025-03-10 ($1 000 → €900); sell 5 @ $250 on
2026-05-15 (€1 125) and 5 @ $250 on 2026-09-15 (€1 125). Two sales, so the year-aggregate reading is
what is under test.

| Regime | Coefficient | `I` | `G` | 50% × G | Exempt | `gyp_net` | Cuota |
|---|---|---|---|---|---|---|---|
| Navarra | 1.000 | 1 350 | 2 250 | 1 125 | 1 125 | 225 | 225 × 20% = **45.00** |
| Común | 1.000 | 1 350 | 2 250 | — | 0 | 1 350 | 1 350 × 19% = **256.50** |
| Gipuzkoa | 1.020 | 1 332 | 2 250 | — | 0 | 1 332 | 1 332 × 19% = **253.08** |

(Gipuzkoa: cost 900 × 1.020 = 918, i.e. 459 against each €1 125 sale → 666 per sale. Its 2026
scale opens at 19%, not 20%, so the cuota is 253.08.)

**`small_disposal_boundary` fixture** — same buy, one sell of 10 @ $350 on 2026-05-15 (€3 150).
`G = 3 150 > 3 000`, so 1.º fails and no regime exempts anything. Navarra: `I = 3 150 − 900 = 2 250`,
base 2 250, cuota **450.00**.

**Pure-function vectors** (`G`, `I`) → exempt:

| `G` | `I` | Exempt | Why |
|---|---|---|---|
| 3 000.00 | 1 000 | 1 000 | 1.º inclusive; 1 000 ≤ 1 500 |
| 3 000.01 | 1 000 | 0 | 1.º fails by one cent |
| 2 250 | 1 350 | 1 125 | only the excess over 50% is taxed |
| 2 000 | 1 000 | 1 000 | exactly 50%, "no exceda" is inclusive |
| 2 000 | 1 000.01 | 1 000 | 0.01 taxed |
| 1 000 | 0 | 0 | a loss-only year has no incremento |
| 0 | 0 | 0 | no transmission |

### A.5 — The credit at the Navarra rate (art. 67)

Art. 67.1 takes the lesser of the tax actually paid abroad and `tipo medio efectivo × the slice of
base liquidable corresponding to the foreign income`; art. 67.2 defines the tipo as
`100 × cuota líquida / base liquidable`, split general vs ahorro, **two decimals**. The existing
rounded-average-rate machinery is exactly this, so the credit needs no new arithmetic — only the
Navarra scale behind it. A.3's `income` run is the worked case (treaty limb binds at €135 against a
rate limb of €175.09).

Two points recorded rather than computed:

- **Two decimals is operative.** On a €22 500 base the rate is 0.2324, not 0.232444…; a €6 000 slice
  of foreign income therefore gives a limb of `6 000 × 0.2324 = 1 394.40`, where the unrounded rate
  would give 1 394.67. The €0.27 is what art. 67.2 costs, and it is the reason rounding happens
  before the multiplication.
- **Líquida-vs-íntegra proxy.** Art. 67.2 divides *cuota líquida*, and the tool models no savings-side
  deduction (the art. 62.9 minima are quota credits against the general part), so within this tool's
  scope savings cuota líquida = savings cuota íntegra and the existing `average_rate` is exact.
  `TODO(verify)` in the register, since a filer with a savings-side deduction would need a lower rate.

### A.6 — The regime-independent fixtures under Navarra (N4)

| Fixture | Year | `rcm_net` | `gyp_net` | Base | Cuota | Note |
|---|---|---|---|---|---|---|
| `fifo` | 2026 | 0 | 22 500 | 22 500 | **5 230.00** | coefficient 1, as Común; `G = 49 500` so no exemption |
| `income` | 2026 | 963 | 0 | 963 | **192.60** | see A.3; net due 57.60 after the €135 credit |
| `fx_gain` (revaluing) | 2026 | 0 | 1 000 | 1 000 | **200.00** | exemption suppressed: the year has an FX realization |
| `loss` | 2026 | 0 | −9 000 | 0 | **0.00** | 9 000 @2026 carried, identical to Común |
| `cross_offset` | 2026 | 7 200 | −9 000 | 5 400 | **1 080.00** | see A.2 case 3 |
| `cross_offset_rcm` | 2026 | 0 | 9 000 | 9 000 | **1 860.00** | cap base 0, see A.3 |

`fifo` cross-check across regimes on one set of trades: Gipuzkoa 19 692 → 3 957.24, Común 22 500 →
4 605.00, Navarra 22 500 → 5 230.00. Navarra shares Común's base and differs only by the scale; it
shares neither with Gipuzkoa.

The wash-sale fixtures assert deferral and reintegration amounts, not tax, and the engine is
regime-parameterized only through the coefficient and the exemption — Navarra takes coefficient 1
like Común, and every wash-sale fixture's proceeds exceed €3 000, so its deferral figures must equal
Común's to the cent.

## Close-out (2026-08-12)

### Gate

| Check | Result |
|---|---|
| `cargo check --all-targets` | clean |
| `cargo test spain --lib` | **344 passed**, 0 failed (baseline 267 → +77) |
| `cargo test --lib` | 823 passed, **34 failed** — the pre-existing `parse_real` set, unchanged |
| `./check` | the same **3** upstream clippy errors (`statistics.rs`, `xls/table.rs`, `rebalancing.rs`), nothing new |

### Smoke runs

Config at `<scratchpad>/es-smoke`, portfolio `ibkr-miren`, year 2025.

**`regime: gipuzkoa`** — base **€17.32**, cuota íntegra **€3.46**, cuota líquida **€3.46**, exactly
as before, and the emitted CSV is **byte-identical** to `es-final3.csv`, the file the 002 round left
behind. Nothing in this round moved the user's own figures.

**`regime: navarra`** on the same statement (scratch config at `<scratchpad>/es-smoke-navarra`):

| Figure | Gipuzkoa | Navarra | Why |
|---|---|---|---|
| Dividends | 82.23 | 82.23 | same statement |
| Dividend exemption | 82.23 | **0.00** | LF 29/2014 repealed Navarra's relief in 2015 |
| Interest | 1.14 | 1.14 | — |
| Deductible fees | 0.00 | 0.00 | the year has no custody fee; the 3% ceiling never engages |
| RCM neto | 1.14 | **83.37** | the whole dividend is income under Navarra |
| Ganancias netas | 16.18 | 16.18 | coefficient 1 vs Gipuzkoa's — the year's lots are all 2025, so the coefficient was 1 either way |
| Base liquidable | 17.32 | **99.55** | 1.14 + 16.18 vs 83.37 + 16.18 |
| Cuota íntegra | 3.46 | **19.91** | 99.55 × 20% (art. 60's first bracket) |
| Foreign credit | 0.00 | **12.33** | under Gipuzkoa the dividend is exempt, so it bears no Spanish tax and carries no credit; under Navarra it is taxed, and the treaty limb min(12.33, 82.23 × 15% = 12.33) binds |
| Cuota líquida | 3.46 | **7.58** | 19.91 − 12.33 |

The whole gap is the €1,500 exemption and the scale: on this statement the two regimes differ by
exactly the tax on the €82.23 of dividends Gipuzkoa exempts, less the credit that becomes available
once they are taxed. Every figure follows Appendix A's rules.

One Navarra-only warning fired, and it is the one the round expected to be noisy on a real statement:
the year's securities transmissions came to **€149.12** — under the €3,000 of art. 39.5.d — but the
statement also contains foreign-currency conversions, so the global transmission amount cannot be
measured and the exemption was withheld (Appendix A §A.4 edge 3). That overstates the tax rather than
granting a relief the year may not be entitled to, and the message says how to check by hand.

### Statute-text corrections made to this plan

All in N1 (`8ded6bbd`) unless noted, after reading `trlfirpf.txt` verbatim:

1. **art. 54.2** — the summary did not say that own-group absorption fires **only** when the current
   year's result is positive ("si el resultado fuera positivo"); a group whose result is negative
   leaves its own prior saldos untouched. Added, with the statute quoted.
2. **art. 54.2** — confirmed and made explicit that the 25% is measured on "el saldo positivo
   **resultante de la letra b)**", i.e. the other group's figure *after* it absorbed its own
   carryforwards. That is the whole difference from the AEAT order and it was worth quoting.
3. **DT 7.ª** — the plan said "warn when any FIFO lot predates 1995". The article's own trigger is
   "elementos patrimoniales adquiridos **antes de 31 de diciembre de 1994**", so a lot acquired *on*
   31-12-1994 is outside it. The warning fires on `acquisition_date < 1994-12-31` and the fixture
   pins the one-day boundary.
4. **art. 32.1.a** — the cap base is "los ingresos íntegros, que no hayan resultado exentos,
   **procedentes de dichos valores**". Pinned that this means dividend income and not broker
   cash-account interest, which is art. 29 income from a cesión de capitales propios rather than
   from a valor negociable.
5. **art. 39.5.d** — quoted verbatim; the plan's summary was accurate. The residual ambiguity is
   2.º's singular "el importe global de **la transmisión**" against 1.º's plural, pinned to the
   year-global reading in Appendix A §A.4.
6. **art. 60 and art. 67** — read verbatim, no divergence from the plan's summary. The five golden
   cuota vectors are the statute's own published column and all five reproduce.
7. **Appendix A arithmetic fix** (N7, `3748b5bc`): the Gipuzkoa row of the `small_disposal` table
   said 1 332 × 20% = 266.40. The reformed 2026 foral scale opens at **19%**, so it is 253.08.

### Codebase corrections made to this plan

- **N9's bond warning was dropped.** The plan asked for an all-regime warning on debt-instrument
  sales, on the premise that "the tool taxes them as GyP". It does not: the shared Interactive
  Brokers parser discards every instrument whose `assetCategory` is not `STK`, with its own warning,
  before the Spanish processor sees anything. A bond disposal is therefore absent from both
  savings-base groups rather than misclassified, and there is nothing at this layer to detect. It
  became register entry §17 instead.

### Left open

1. **A pre-existing four-column defect in the Gipuzkoa Modelo rows.** `MODELO_109_*` labels carry a
   `(hoja, casilla <n>)` suffix whose comma splits the row into four fields, breaking the summary
   block's three-column shape. It predates this round and fixing it would move Gipuzkoa output, which
   this round had to leave byte-identical. `modelo_rows_are_three_columns` asserts the shape as it is,
   per regime, so it cannot regress unnoticed; the contract records it. **Recommended as the first
   item of a follow-up round.**
2. **The coefficients-under-Navarra rejection is asymmetric.** A `taxes.spain.coefficients` block is
   an error under `navarra` and silently ignored under `comun`, which has no actualization either.
   The plan asked for the Navarra rejection specifically and the bit-identical constraint ruled out
   touching Común. Worth unifying in a follow-up.
3. **The abatement warning is Navarra-only.** LIRPF DT 9.ª and NF 3/2014 have their own abatement
   regimes for pre-1994 acquisitions, with a €400,000 lifetime cap the Navarra one lacks. They were
   not researched this round, so the tool says nothing about them rather than citing the wrong
   statute. `abatement_lots` is populated for every regime, so extending the warning is a
   message-builder change once the research is done.
4. **`SpanishTaxStatement::new` now takes ten positional arguments.** Each is regime-derived and
   already lives together on `SpanishTaxParams`; grouping them into one struct would be the natural
   cleanup, but it is the kind of structural change this plan explicitly reserved.
5. **F-93 apartado H3.** The negative-transmissions block's casillas could not be disambiguated from
   the specimen's extracted layout, so no number is emitted for it — a `#` comment names the block
   instead. A filer with a loss-making year has to read it off their own form.
6. **The `verified` ejercicio is 2025 only.** FY2024 and FY2026 reuse its numbering under an explicit
   warning. Verifying the FY2024 Anexo I (Orden Foral 28/2025) would remove one caveat.

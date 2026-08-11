# Spanish Tax Feature — Remediation Work Plan

**Date**: 2026-08-11
**Branch**: `002-spain-tax` (continue on it)
**Inputs**: an adversarial code review of the full 27-commit branch (verdict: BLOCK until R1–R3 fixed) and a legal-research pass over the 11 `TODO(verify)` markers (3 REFUTED, 5 CONFIRMED with citations, 3 unresolved). This plan consolidates both. Work it exactly like `plan.md`: phases in order, failing test first, per-task gate (`cargo check` → `cargo test spain` → `./check`), one Conventional Commit per task, statuses updated in-place via `docs(plan)` commits.

The review also **confirmed correct** (do not touch): FIFO/coefficient money path (all Appendix A vectors reproduce independently), wash-sale ordering/blocking/self-block guards, window arithmetic, ingestion dedup, config validation of ledgers, the one-formatter invariant, and all jurisdiction wiring. Gipuzkoa compensation is unaffected by R4 ("exclusivamente entre sí" re-confirmed against the Gipuzkoa Manual de Renta cap. 9).

## Phase R0 — Filing blockers

- **R1 — Margin interest must not reduce RCM.** Status: ✅ Done (`925163d4`)
  `InterestEntry` gains `taxable` + `notes`; a negative accrual is reported as an informational row, excluded from `total_interest_income`, and summed into the new `total_paid_interest`. One `warn!`, one console line, one `SUMMARY_RCM_INTEREST_PAID` row, and the CSV row carries no `savings_group`. New `margin_interest` fixture ($100 received, $250 paid) asserted under both regimes.
  Commit: `fix(spain-tax): exclude paid margin interest from the savings base`
- **R2 — Wash-sale carry-out: snapshot at filing-year end and guard config overlap.** Status: ✅ Done (`085f90a9`)
  (a) `apply_wash_sale_rule` snapshots blocked lots the moment the replay passes 31 Dec of the filing year (`snapshot_blocked_lots`); later disposals still run, but only to report releases. New `wash_sale_carry_out` fixture (loss 2026-12-10, repurchase 2027-01-15, disposal 2027-03-20) pins a €360/40-share carry-out that the old code emptied. (b) An opening entry whose `sale_date` matches a replayed disposal of the same instrument now errors naming the date. (c) `an_opening_deferred_loss_reintegrates_on_disposal` rewritten to a genuine carry-in whose origin sale is outside the statement (R9 later moved it onto the `loss` fixture, where the releasing disposal is definitive).
  Commit: `fix(spain-tax): snapshot wash-sale carry-out at year end and reject overlapping config`
- **R3 — Double-taxation credit: taxable-net base + 2-dp rate.** Status: ✅ Done (`b441d230`) — implemented after R4 so it reads the corrected compensation outputs.
  (a) `double_taxation_credit` regains its `taxable_eur` limb; `SpanishTaxStatement::foreign_income_reaching_the_base` subtracts pro-rated deductible fees, then pro-rates by `rcm_taxable / rcm_net` so a wiped group carries no foreign income. New statement fields `foreign_gross_income` / `foreign_taxable_income` + a `SUMMARY_FOREIGN_TAXABLE_INCOME` row. (b) `SavingsScale::average_rate` rounds to 4 dp (2 dp as a percentage), half away from zero; its `TODO(verify)` is replaced by the art. 76.2 / 80.2 citation plus the AEAT cap. 18 example. Tests: the AEAT 996 vector, the net-limb vector (430 × 19% = 81.70), a Común year where a €5,000 prior RCM balance wipes the group → credit 0 at a 19.40% average rate, and the Gipuzkoa gross case still credits €135 unchanged.
  Commit: `fix(spain-tax): base the double-taxation credit on taxed net income at the two-decimal rate`
- **R4 — Común compensation order per AEAT Manual cap. 12.** Status: ✅ Done (`8d5ed408`) — implemented **before** R3, because the credit's taxable-net base is evaluated after compensation and R3 has to read R4's corrected outputs.
  `compensate_savings_base` now runs Fase 1ª (current-year cross) → Fase 2ª-1º (own-group ledgers) → Fase 2ª-2º (prior-year leftovers cross), with a single per-group allowance measured on the **original** current-year positive and consumed across both cross steps. `LossLedger::apply`'s `cap` is live in Fase 2ª-2º; `LedgerApplication::merge` folds the two passes over the same ledger. `CompensationResult`/`SpanishTaxStatement` gain `prior_cross_offset_*`, with two new CSV summary rows. Manual example (+4,000 / −800 / 2,800 / 500) reproduces base **200** exactly, and a cap test proves the allowance is measured pre-absorption (500 vs the old 1,500). Gipuzkoa: `gipuzkoa_is_unchanged_by_the_two_phase_order` pins the same inputs at 1,200 with every cross figure zero, and all pre-existing Gipuzkoa unit + fixture assertions passed untouched.
  Commit: `fix(spain-tax): follow the AEAT two-phase compensation order for Territorio Común`

## Phase R1 — Correctness (High/Medium)

- **R5 — Post-split units in the wash-sale replay.** Status: ✅ Done (`2182d62d`)
  Every quantity the engine sees is normalized to units as of the statement's last date (`split_reference`): acquisitions via `stock_splits.get_multiplier(buy → reference)`, consumed lots the same way, and `PricedSale.normalized_quantity` as the disposal quantity. Reported figures are untouched. New `wash_sale_split` fixture (100 + 50, 2-for-1, loss sale of 200) deferred €225 before and €450 after; the release mirror leaves 50 shares still blocking €225. Carried-in config quantities stay as given — documented.
- **R6 — Validate coefficient overrides.** Status: ✅ Done (`64a6a4db`) — `coefficients::validate_overrides` rejects `<= 0` and `> 10` naming `taxes.spain.coefficients.<disposal>.<acquisition>`, wired through `SpanishTaxConfig::validate_coefficients` in `SpanishTaxParams::resolve`, with unit + pipeline tests and a plausible-value guard (0.98 / 1 / 2.156 / 10 accepted).
- **R7 — Fee-only / ledger-only years still produce a statement.** Status: ✅ Done (`4e6db148`) — `process_fees`' return value is now used, and `compute_tax_year` also reports income when either prior ledger is non-empty or `deferred_losses` is configured. New `fee_only` fixture plus carryforward-only and deferral-only tests, and a negative test that a genuinely empty year still reports nothing.
- **R8 — Casilla corrections.** Status: ✅ Done (`3916de65`) — no retenciones row is emitted on either form; a `# WARNING` names the casilla (109 `07+22`, 100 `0597`) the foreign withholding does *not* belong in and points at the DDI box. Modelo 100 rows carry the ejercicio-2025 numbers 0027 / 0029 / 0037 / 0326-0340 / 0460 / 0588 under a warning naming the Orden HAC/277/2026 consultation draft. Casilla 33 relabelled "base liquidable del ahorro (tras compensación)". Contract doc and `docs/spain-taxes.md` updated.
- **R9 — Chained wash-sale ("transmisión definitiva", DGT V3282-18).** Status: ✅ Done (`f9b70322`)
  `WashSaleEngine::process` now computes the window match once and splits every released amount by the matched fraction: the matched part is re-attached to the new blocking lots keeping its original `origin_sale_date`, the rest becomes integrable. `defer` split into `match_window` + `block`; `block` shares the matched quantity between placements in proportion to their amounts, so total blocked quantity stays exactly one blocked share per matched share. New `wash_sale_chained` fixture (defer → non-definitive disposal → definitive disposal) plus two unit tests. Two pre-existing tests were re-pinned to the corrected semantics: `shares_already_blocking_cannot_block_again` (900 → 360 integrable, 540 re-attached) and `an_opening_deferred_loss_reintegrates_on_disposal` (moved to the `loss` fixture, where the releasing disposal is definitive).

## Phase R2 — Verification, polish, docs

- **R10 — Gipuzkoa €1,500 dividend exemption (art. 9.24): verify, then implement or refute.** Status: ✅ Done (`6f656eb2`) — verdict **CONFIRMED, implemented**.
  **Sources** (all retrieved 2026-08-11): Diputación Foral de Gipuzkoa, Modelo 109 "Exenciones" pages for ejercicios [2023](https://www.gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/2023/exenciones), [2024](https://www.gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/2024/exenciones) and [2025](https://www.gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/2025/exenciones) — item **24** verbatim: *"Los dividendos y participaciones en beneficios a que se refieren las letras a) y b) del apartado 1 del artículo 34 de esta Norma Foral, con el límite de 1.500 euros anuales"*, with the exclusions for instituciones de inversión colectiva, the two-month acquire-then-transfer clause, and cooperative interest. No residence restriction on the payer, so foreign dividends qualify. Full texts of [NF 1/2025](https://primeralecturaediciones.com/documentos_diana/LEYES_2025/GUIPUZKOA/NF_1_2025_Gipuzkoa_IRPF.pdf) and [NF 2/2025](https://www.primeralecturaediciones.com/documentos_diana/LEYES_2025/GUIPUZKOA/GIPUZKOA_NF2_2025.pdf) checked directly: both only **add** números to art. 9 (38, 39, 40) and amend 1, 5, 6, 7 — número 24 is untouched, so the relief stands for 2026.
  **Implementation**: `dividend_exemption_limit` (€1,500 Gipuzkoa / 0 Común) on `SpanishTaxParams` and the statement; `total_dividend_exemption = min(limit, Σ eligible gross)` comes off `rcm_net`; the anti-abuse clause is computed per instrument (`dividend_is_washed`); the exempt slice is also removed from the credit's rate-limb base, because exempt income bears no Spanish tax. A `warn!`, a console line and a CSV `# WARNING` name the exemption because the tool cannot tell an IIC distribution from a company dividend. New `dividend_exemption` fixture; four pre-existing `income`/`cross_offset` assertions re-pinned (Gipuzkoa `income` RCM 990 → 90, credit €135 → 0; `cross_offset` Gipuzkoa base 7,200 → 5,700).
- **R11 — Report-only stock grants and corporate actions.** Status: ✅ Done (`f8cbe379`) — `StockGrantEntry` / `CorporateActionEntry` + `process_stock_grants` / `process_corporate_actions`, both informational and both counted as activity so a vest-only or action-only year still produces a statement. Vests carry the vest-date EUR value and a general-base reminder (and a louder note when the statement has no FMV, since the shares would then be costed at zero). Corporate actions get a per-type note saying whether the FIFO queue already handled it (split, rename) or it needs manual review (delisting, liquidation, spinoff, scrip dividend, rights issue). New 19-column CSV rows, console lines, `grants` fixture, and a corporate-action assertion on `wash_sale_split`.
- **R12 — Mechanical batch.** Status: ✅ Done (`c9bf4a0c`)
  (a) the `expect` is now a named error, and `coefficients::has_table` / `SpanishTaxConfig::has_actualization_table` separate "no table shipped for that disposal year" from a real fault, so "acquisition after disposal" propagates instead of silently marking the sale unpriced; (b) `loss <= 0` `deferred_losses` entries rejected, with an rstest covering all three malformed shapes; (c) the two IB flex skip-warnings are jurisdiction-neutral and rate-limited to once per instrument via a `HashSet` threaded through `parse_trade` / `parse_statement_of_funds_trade`; (d) trailing newline restored; (e) four `TODO(verify)` markers replaced by citations — rounding (AEAT "se redondeará por exceso"; foral silent), fecha de transmisión (LIRPF art. 14.1.c + NF art. 57.1.b + DGT V0152-26, trade date), homogeneity (RIRPF art. 8 + DF 33/2014 art. 47 + DGT V0796-26), reintegration keying (DGT V0913-08 + V3282-18, recompra pool with FIFO inside it); (f) the treaty-rate field documents the UK 10% / US-REIT / interest caveats and `docs/spain-taxes.md` gains a table for them; (g) plan.md corrected to NF 2/2025 for the fungible-crypto FIFO wording; (h) the fee-keyword and window-endpoint markers stay open by design.
- **R13 — Docs + final gate.** Status: ✅ Done — see the remediation report at the end of this file.
  Task text: update `docs/spain-taxes.md` (margin interest, carry-out overlap rule, chained wash-sale, treaty-rate table with UK/REIT caveats, credit net-base per TEAC, Común two-phase compensation with the manual's example, R10 outcome); re-run the full gate; re-run the real-statement smoke test (`cargo run -- --config /private/tmp/claude-501/-Users-alexandre-projects-alexsavio-investments/864d554e-7895-4b99-9187-48e9e68b5156/scratchpad/es-smoke tax-statement ibkr-miren 2025 <scratch>/es2.csv` — expect the same €7.58 unless R3's rate rounding moves it by cents, and NO foreign WHT under retenciones); append a remediation report to this file (fixed/remaining, new totals if changed).
  Commit: `docs(spain-tax): document remediation outcomes and close the review`

## Explicitly accepted, not fixed (record only)

- L5 `simulate-sell` errors loudly for years without a shipped scale — correct failure direction.
- L6 sub-1e-27 per-lot allocation residual — invisible at 2 dp.
- Settlement-vs-conclusion FX date simplification (L2) — inherited from the German path, documented.
- Modelo 109 casillas remain 2019-vintage with warnings; DDI casilla stays `CASILLA_UNKNOWN` (nothing newer is published; OF 112/2026 has no casilla annex).

## Remediation report (R13)

**Date**: 2026-08-11 · Branch `002-spain-tax`, 13 remediation tasks on top of the 27-commit feature.

### Gate

| Check | Result |
|---|---|
| `cargo check --lib --all-targets` | clean |
| `cargo test spain --lib` | **215 passed / 0 failed** (169 at the start of remediation) |
| `cargo test --lib` | **681 passed / 33 failed** — every failure is a pre-existing `broker_statement::*::parse_real` case from the private, empty `testdata/` submodule. Filtering the failure list for anything that is not a `parse_real` case yields **0** rows, and the count is unchanged from the base |
| `cargo test --no-fail-fast` (all targets) | lib as above; `tests/generate.rs` 1 passed; binary target has no tests |
| `./check` (clippy, dev + release, `-Dwarnings`) | the same **4 pre-existing errors** in untouched files: `portfolio/rebalancing.rs`, `quotes/cbr/mod.rs`, `analysis/performance/statistics.rs`, `formats/xls/table.rs`. No new lint at any commit |

### Real-statement smoke test

`cargo run -- --config <scratch>/es-smoke tax-statement ibkr-miren 2025 <scratch>/es2.csv`, Gipuzkoa 2025.

| Figure | Before | After | Why |
|---|---|---|---|
| Base liquidable | €99.55 | **€17.32** | R10: €82.23 of dividends are exempt under NF 3/2014 art. 9.24 |
| Cuota íntegra | €19.91 | **€3.46** | 17.32 × 20% (the pre-reform 2025 foral scale) |
| Credit (DDII) | €12.33 | **€0.00** | R3 + R10: the rate limb runs on the income that reaches the base, and the dividends are wholly exempt, so there is no Spanish tax to credit the €12.33 against. The excess is reclaimed from the source state |
| Net tax due | €7.58 | **€3.46** | |

Everything else is unchanged: ganancias y pérdidas stay at €16.18 (capital gains −12.25 + FX 28.43),
so R5 (post-split units) and R9 (chained releases) moved nothing on this statement — it holds no
split and no chained deferral. R1 found no paid margin interest here (`SUMMARY_RCM_INTEREST_PAID`
0.00). The €17.18 of borrowed-balance FX is still excluded and flagged.

Verified on the output:

- **No retenciones row on either form.** The €12.33 appears in `SUMMARY_FOREIGN_WITHHOLDING`
  (informational) and in a `# WARNING` naming casilla 07+22 as the box it does *not* belong in, and
  the DDII row carries the credit.
- The Modelo 109 boxes reconcile: `06+16` 1.14 − `17` 0.00 + `28` 16.18 = `33` 17.32. The íntegros
  box is net of the exemption, because an exención is never declared as income (a formatter change
  made while running this smoke test, with a regression test pinning the identity).

### What changed, by task

| Task | Commit | Effect on a filed number |
|---|---|---|
| R1 margin interest | `925163d4` | Paid interest no longer reduces RCM |
| R2 carry-out snapshot + overlap guard | `085f90a9` | A carry-out is no longer emptied by post-year-end disposals; a double-deducting config is refused |
| R4 Común two-phase compensation | `8d5ed408` | Común bases change wherever prior-year balances and a current-year cross-offset meet; Gipuzkoa bit-identical |
| R3 credit net base + 2-dp rate | `b441d230` | Credits shrink where expenses or compensation reduced the foreign income; the rate is rounded before multiplying |
| R5 post-split units | `2182d62d` | Deferrals across a split match the right fraction |
| R6 coefficient validation | `64a6a4db` | A nonsensical override is refused instead of silently pricing |
| R7 fee/ledger-only years | `4e6db148` | Those years now produce a statement |
| R8 casillas | `3916de65` | Foreign WHT off the retenciones box; Modelo 100 numbered |
| R9 definitive-disposal releases | `f9b70322` | A chained sale no longer integrates a loss that is still blocked |
| R10 dividend exemption | `6f656eb2` | **Largest effect**: −€1,500/year of Gipuzkoa dividend income, and the matching loss of credit |
| R11 grants + corporate actions | `f8cbe379` | Reporting only |
| R12 mechanical batch | `c9bf4a0c` | No panics on an unpriced filing year; real coefficient faults propagate; markers cited |
| R13 docs + gate | this commit | |

### Left open

- **Two `TODO(verify)` markers remain by design**: the custody-fee keyword list (art. 26.1.a names
  the service, not a broker's wording; unrecognised fees are reported, not deducted) and whether the
  two-month window's endpoints are inside it (treated as inclusive).
- **Instituciones de inversión colectiva.** The €1,500 exemption does not cover fund/ETF/SICAV
  distributions, and an IB statement does not distinguish them from company dividends. The tool
  exempts them anyway and names every payer it exempted in a `warn!`, a console line and a CSV
  `# WARNING`. A filer holding distributing funds must reduce the exemption by hand.
- **Modelo 109 casillas stay 2019-vintage** and the DDII casilla stays `CASILLA_UNKNOWN`; Modelo 100
  numbers come from a consultation draft. All under `# WARNING` banners.
- Items recorded as accepted below (L5, L6, the settlement-vs-conclusion FX date) are unchanged.
- Carried-in `deferred_losses` quantities are taken in the units the prior return reported; a split
  between that acquisition and the current statement is not applied to them.

# Navarra Round — Review Findings & Fix Plan

**Date**: 2026-08-12
**Branch**: `003-navarra-tax` (continue; after N10 close-out)
**Inputs**: adversarial review of the 14-commit diff (verdict **WARN**, no CRITICAL/HIGH; the Gipuzkoa/Común byte-identity invariant was verified empirically against base-and-HEAD binaries, including 4-vintage carryforward configs) and a consistency audit (7 mismatches). This plan fixes all of it. House rules as ever: failing test first where a behavior changes, per-task gate, one Conventional Commit per task, statuses in-place. Gipuzkoa/Común bytes must remain identical except where a fix explicitly targets a shared label (V1 — regime-gate it so they don't move).

Verified-correct by the reviewers (do not touch): the `NavarraOrdered` compensation arm (recomputed from the statute; the at-most-one-direction property was proven, the warning amount is exact), the scale vectors, the F-93 mapping cell-for-cell against the specimen (including 8815=8885+8895 semantics and the aggregate 818/8875 carry-out boxes), the fee-cap base and its exemption interactions, the pre-1994 `<` boundary, sign conventions, and the absence of any new panic paths.

## Tasks

- **V1 — Regime-correct summary labels.** Status: DONE (`78e813e6`)
  `csv_formatter.rs:569/575` label the `SUMMARY_CROSS_OFFSET_*` rows "fase 1ª (25% — sólo Territorio Común)" — emitted with non-zero values under Navarra (a passing test asserts €1,300). Make the four cross/prior-cross row labels regime-dependent (AEAT "fase" vocabulary for Común; "art. 54.2" vocabulary for Navarra; Gipuzkoa keeps its current never-fires wording) via the params, so Común/Gipuzkoa bytes are unchanged. Update the contract's row descriptions to note the per-regime label.
  Commit: `fix(navarra-tax): label the cross-offset rows with the regime's own statute`
- **V2 — H3 loss-year guidance.** Status: DONE (`f05dca17`)
  `csv_formatter.rs:1150-1165`: the fallback comment tells a loss-year filer the H3 amount is "the H1 row above with the sign reversed" — but that row is `max(0, gyp_net)` = 0.00 exactly then. Emit the real figure: a `MODELO_F93_8816` row carrying the negative-transmissions saldo (specimen evidence: the H3 box column mirrors H4's, running `8816 / 817`; cite it and mark the number medium-confidence in the label note), and fix the comment to point at it. Close plan Left-open item 5 accordingly.
  Commit: `fix(navarra-tax): report the H3 negative-transmissions saldo in its own box`
- **V3 — Discriminating fixtures for the pinned readings.** Status: DONE (`f3e66f75`; Appendix A hand-computation in `b96e6d6e`, no discrepancy found)
  (a) `small_disposal` uses two identical sales, so the year-global vs per-transmission readings coincide — replace with two UNEQUAL sales where the readings differ, pin the year-global numbers in Appendix A and the fixture (adversarial report has the arithmetic shape). (b) Add a mixed gain/loss year (sale A +700 on €1,200, sale B −400 on €800): pin that the exemption eats only the gain and the surviving figure is the loss (−400), matching the form's own column arithmetic (656/1658/657) — the reviewer believes current behavior is right; the fixture proves it and Appendix A records it. (c) The proceeds measure `G` is currently net of sell commissions and untested: pin the current choice with a non-zero-commission fixture variant, document gross-vs-net as an OPEN register entry (art. 40 "importe real" vs "valor de transmisión"; F-93 box 651 "Valor de transmisión" favors net), and cite it in the code.
  Commit: `test(navarra-tax): pin the small-disposals readings with discriminating fixtures`
- **V4 — Warning and register polish.** Status: DONE (`6bd23cd7`)
  (a) LOW-6: the exemption-withheld warning fires with "€0.00 of securities transmissions" when only FX gains exist — suppress it when `small_disposals_gains == 0` AND securities proceeds are 0 (nothing was withheld), keep it otherwise; re-pin the test. (b) LOW-8: state the `MODELO_F93_8850` sign convention (positive magnitude of the negative saldo) in its label and the contract. (c) LOW-11: document the `fx_borrowed_review` asymmetry in the register's suppression entry. (d) LOW-10: extend `navarra_params_come_from_the_foral_statute` to assert `custody_fee_cap_fraction` and `small_disposals_exemption`. (e) LOW-9: add the fractional-cent carry-out note to the plan's Left-open list (pre-existing, all regimes).
  Commit: `fix(navarra-tax): correct the exemption-withheld warning and pin conventions`
- **V5 — Three-regime citation sweep (consistency M4).** Status: DONE (`bc3a517f`)
  Shared surfaces still citing only two statutes now serve three: margin-interest note/warning + register §7 (`processor.rs:1136-1150`, docs:625/968) gain the TRLFIRPF cite; the fee note/warning cites art. 32.1.a under Navarra instead of LIRPF art. 26.1.a (message builder takes the regime's article); venue-review and deferral notes name the Navarra articles (39.6.f/g); wash-sale doc section (docs:199-288) gains the art. 39.6 citations incl. the direct MiFID II reference; FX section wording; stale "both regimes"/"either regime" comments (`scale.rs:226`, `taxes/mod.rs:656`).
  Commit: `docs(navarra-tax): cite all three statutes on shared surfaces`
- **V6 — Contract and plan corrections (consistency M1/M2/M5/M6, adversarial LOW-7).** Status: DONE (`9920e72e`)
  Contract: add the DT 7.ª abatement banner (fifth) + the H3 block/8816 row; fix the "four warning banners" count; document the per-regime cross-offset labels (V1). Plan: N10 hash `40d1eae0` → `0b61bf86` (the self-referential-amend orphan); the F-93 carryforward-box inventory row corrected to what shipped (aggregates 818/8875, per-year cells named as on-form-only); replace the plan's "TODO(verify) in the register" phrasing with the register's actual "OPEN" convention (M6); note (M7) that N1's corrections 1–6 predate the plan's first commit by design.
  Commit: `docs(navarra-tax): correct the contract and plan against the audits`
- **V7 — Gate + close-out.** Status: DONE
  Full gate (`cargo test spain --lib` ≥ 344 with the new fixtures; `./check`; German suites untouched); byte-identity re-check of the Gipuzkoa smoke CSV (V1's regime-gating must keep it byte-identical — the cross rows are zero there, but the label must not change for Gipuzkoa/Común); Navarra smoke re-run explained; close-out appended here listing fixed/accepted.
  Commit: `docs(navarra-tax): close the review round`

## Second pass — findings on the V-round diff (adversarial WARN + consistency ≤ LOW), fix tasks

Everything below is text/contract/coverage; both reviewers independently reproduced every euro of the V round.

- **W1 — Regime-gate the two rows V1 missed.** Status: DONE (`c2c01629`)
  `csv_formatter.rs:559-569`: `SUMMARY_RCM_LOSSES_APPLIED` / `SUMMARY_GYP_LOSSES_APPLIED` still say "fase 2ª-1º" and carry non-zero Navarra amounts (test pins €2,000 through one). Same regime-gated treatment as V1 (Navarra: art. 54.2 own-group vocabulary; Gipuzkoa/Común strings byte-unchanged); also the "Fase 2ª-2º" doc comment at `statement.rs:837`. Widen `the_cross_offset_rows_name_the_regimes_own_statute` past its `CROSS_OFFSET_` filter so the whole compensation block is covered. Contract row descriptions updated.
  Commit: `fix(navarra-tax): label the own-group compensation rows with the regime's statute`
- **W2 — Withheld-exemption caveat: gains-only predicate.** Status: DONE (`bea4392b`)
  `statement.rs:736-745`: an all-loss securities year under €3,000 plus an FX gain still triggers the caveat, though `I = 0` makes the relief zero for any `G` — the counterfactual is closed and the text is false. Failing test first (all-loss + FX-gain fixture or config variant → no caveat; `small_disposal_fx` keeps firing); replace the proceeds disjunct with `small_disposals_gains > 0` and delete the then-provably-dead `|| total_fx_gains > 0` clause. Behavior is message-only (verify no euro moves).
  Commit: `fix(navarra-tax): silence the withheld-exemption caveat when no gain exists`
- **W3 — Re-derive Appendix A §A.4 edge 3.** Status: DONE (`bfcf9a90`)
  plan.md:229-233 still says the exemption is "suppressed for any year with an FX conversion realization" — V4a narrowed that, and the plan's own primacy rule demands the amendment in its own `docs(plan)` commit. State the three boundaries as shipped (docs §13 already has them).
  Commit: `docs(plan): re-derive Appendix A for the narrowed exemption caveat`
- **W4 — Discriminate and register the exemption-ceiling choice.** Status: DONE (`7f2a66a5`; Appendix A hand-computation in `e6b69ef3`, no discrepancy found)
  The implementation measures 2.º's 50% ceiling on ALL transmissions' proceeds while `I` is gains-only; no fixture can distinguish that from a gains-only ceiling, and the choice's failure direction is LESS tax (non-conservative), unregistered. Add the discriminating fixture (gain sale ~€400 proceeds/+€300; loss sale ~€2,000/−€600 → all-proceeds ceiling exempts 300, gains-only 200; use converter-representable euros, record in Appendix A first), pin the implemented all-proceeds reading, and add the register §13 entry (art. 39.5.d.2.º "importe global de la transmisión", direction noted).
  Commit: `test(navarra-tax): pin the exemption ceiling on all transmissions' proceeds`
- **W5 — Low/record batch.** Status: TODO — one commit:
  (a) register the 8816/8850 positive-magnitude convention (form defines the boxes as sums < 0; the tool emits magnitudes — a stated choice, register + contract cross-ref); (b) `tests.rs:483-487` doc comment → three statutes; (c) `tests.rs:235` "both regimes" → name the two compared regimes; (d) review.md record polish: annotate the V2 task line (label ships the specimen note, not a medium-confidence hedge — deviation recorded in close-out), fix the "every other box carries" phrasing, annotate V3's task-prose euros as superseded by the recorded substitution.
  Commit: `chore(navarra-tax): close the second-pass low findings`
- **W6 — Gate + byte identity + close-out.** Status: TODO — full gate; `cmp` the Gipuzkoa smoke CSV against `es-final3.csv` (W1 must keep Gipuzkoa/Común bytes identical); Navarra smoke re-run (expect only the two W1 label lines to differ from V7's CSV); close-out appended here.
  Commit: `docs(navarra-tax): close the second review pass`

## Accepted as-is (recorded, not fixed)
- Double console reporting of new messages — matches the established pattern.
- The broad "en el mismo orden" reading + its warning — the flagged register OPEN item; the strictly narrow reading would make carried-saldo crossing impossible, which is its own argument for the broad reading.
- Plan Left-open items 1–4, 6 (Modelo 109 four-column defect, asymmetric coefficient rejection, Navarra-only abatement warning scope, 10-arg constructor, FY2024/2026 box caveats) — follow-up-round material, already recorded.

## Close-out (2026-08-12)

### Gate

| Check | Result |
|---|---|
| `cargo check --all-targets` | clean |
| `cargo test spain --lib` | **352 passed**, 0 failed (round baseline 344 → +8) |
| `cargo test --lib` | 831 passed, **34 failed** — every one of them a `parse_real` case, the pre-existing empty-submodule set, unchanged |
| `cargo test german --lib` / `germany --lib` | 97 / 84 passed, 0 failed — untouched |
| `./check` | the same **3** upstream clippy errors (`statistics.rs:70`, `xls/table.rs:23`, `rebalancing.rs:519`), nothing new |

### Byte identity

V1 changes a label four regimes' worth of rows share, so the invariant was re-checked empirically
against every reference the prior rounds left behind, all with the release binary built at V6:

| Run | Reference | Result |
|---|---|---|
| `es-smoke`, `ibkr-miren` 2025, `regime: gipuzkoa` | `es-final3.csv` (the 002 round's file) | **byte-identical** (`cmp` clean; md5 `9e30f5bf…`) |
| `adv-gipuzkoa` | `base-gipuzkoa.csv` | **byte-identical** |
| `adv-comun` | `base-comun.csv` | **byte-identical** |
| `adv-cf-gipuzkoa` (4-vintage carryforwards) | `basecf-gipuzkoa.csv` | **byte-identical** |
| `adv-cf-comun` (4-vintage carryforwards) | `basecf-comun.csv` | **byte-identical** |

### Navarra smoke re-run

Same statement, `regime: navarra`. Every figure is the one the N10 close-out recorded — base
**€99.55**, cuota íntegra **€19.91**, foreign credit **€12.33**, cuota líquida **€7.58**, ganancias
netas €16.18, RCM neto €83.37 — and the art. 39.5.d warning still fires on the year's **€149.12** of
securities transmissions being unmeasurable against the statement's conversions.

Diffed against the N10 run's CSV, exactly **four lines** moved and no value changed: the four
`SUMMARY_*CROSS_OFFSET_*` labels, from the AEAT "fase" wording to `art. 54.2.a` / `art. 54.2.b`.
That is V1 and nothing else. The V4 warning narrowing does not reach this statement — it has
securities transmissions, so its withholding caveat is unaffected — and the V2 H3 row does not
appear because the year's transmissions are positive; that path is covered by
`a_negative_transmissions_year_reports_its_own_h3_saldo` instead.

### Fixed

| Task | Commit | What moved |
|---|---|---|
| V1 | `78e813e6` | The four cross-offset labels are per regime. Navarra names art. 54.2.a / 54.2.b and the 25% base each is measured on; Común and Gipuzkoa keep their bytes. Contract records the split. |
| V2 | `f05dca17` | A loss-making year emits `MODELO_F93_8816` with the H3 saldo as a positive magnitude, and the `#` comment points at that row instead of at casilla 8808, which is 0.00 exactly then. Plan Left-open 5 closed. |
| V3 | `b96e6d6e` (Appendix A) + `f3e66f75` | `small_disposal` rebuilt on two **unequal** sales so the year-global and per-transmission readings of 2.º give different euros (1 035 vs 900); `small_disposal_mixed` pins that the exemption eats only the incremento and the disminución survives whole (`gyp_net = −450`); `small_disposal_commission` pins `G` net of the sell commission on a case where the commission alone decides condition 1.º. Gross-vs-net recorded OPEN in register §13 and cited in the code. |
| V4 | `6bd23cd7` | The withholding caveat no longer fires on a year with no securities transmissions at all; `MODELO_F93_8850`'s positive-magnitude convention is stated in its label and the contract; the `fx_borrowed_review` asymmetry is written into §13's suppression entry; the params test asserts the two Navarra-only parameters and a new case asserts they are off elsewhere; the fractional-cent carry-out defect is Left-open item 7. |
| V5 | `bc3a517f` | Margin-interest note and warning, the fee notes and the unsettled-fee warning (now taking the regime's own article through a substitution token), the venue-review and deferral notes, the wash-sale and FX doc sections, register §7, and the stale "both regimes" comments in `scale.rs`, `taxes/mod.rs`, `processor.rs` and `statement.rs` all name three statutes. TRLFIRPF art. 39.6.f's direct MiFID II citation is called out where the state text's superseded chain is discussed. |
| V6 | `9920e72e` | Contract gains the DT 7.ª banner row and the `MODELO_F93_8816` row, the banner count goes four → five, and the withholding-caveat condition is restated. Plan: N10's hash corrected to `0b61bf86`, the F-93 carryforward inventory row rewritten to what shipped (aggregates 818/8875; per-year cells named as on-form-only), every `TODO(verify)` phrasing replaced by the register's `OPEN` convention with its entry number, and N1's corrections 1–6 explained as first-commit-by-design. |
| V7 | this commit | Gate, byte identity, smoke re-run, close-out. |

### Findings worth carrying forward

- **The F-93 H3 casilla is better evidenced than the review assumed.** The review marked 8816 a
  medium-confidence inference from a flat text extraction. Re-checked with word bounding boxes on
  page 9 of the specimen, `8816` sits at y=620.7 against H3's y=621.3 — the same 0.6pt offset `8850`
  has from H4 (694.4 vs 695.0) — and `817` / `818` line up with H3's remaining two lines exactly as
  `8865` / `8875` do with H4's. It rests on the same evidence as every other box the reviewers
  verified cell-for-cell, so the label carries the FY2025-specimen caveat every other box carries
  rather than a separate confidence hedge.
- **No hand-computation disagreed with the implementation.** All three V3 fixtures were worked in
  Appendix A first and every asserted figure passed on the first run — proceeds, increments,
  exemption, `gyp_net`, cuota and carryforward, in all three regimes. Nothing was re-pinned.
- **The adversarial report's euros for the mixed case were not representable.** "+700 on €1,200,
  −400 on €800" needs $1 333.33… through the shared flat-0.9 fixture converter. The fixture keeps the
  shape with euros the converter can produce (+900 on €1,800, −450 on €900) and Appendix A says why.
- **Left open, new:** the fractional-cent carry-out (plan item 7). Deciding it means choosing where
  the statutory rounding point sits, not changing a format string.

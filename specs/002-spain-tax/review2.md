# Spanish Tax Feature — Second Review Round: Fix Plan

**Date**: 2026-08-11
**Branch**: `002-spain-tax` (continue; HEAD `b5138036`)
**Inputs**: an adversarial review of the remediation diff (`9435e4d6..HEAD`; verdict BLOCK: 2 HIGH, 6 MEDIUM, 9 LOW) and a consistency audit of the whole feature (2 HIGH, 5 MEDIUM, plus LOW/cosmetic). This plan fixes **all** of it. Work it like the prior plans: phases in order, failing test first, per-task gate (`cargo check` → `cargo test spain` → `./check`), one Conventional Commit per task, statuses updated in-place. The ~33 `parse_real` failures and 4 clippy errors in untouched files are pre-existing.

Reviewer-verified-correct (do not restructure): the Común two-phase compensation (checked against the AEAT manual primary source, all adversarial corners), wash-sale re-attachment conservation (euros and shares, algebraically), R5 split normalization, R1 per-entry interest handling, R2 snapshot semantics, all legal citations, all claimed commits.

## Phase F0 — Blockers (credit correctness)

- **F1 — Credit: per-dividend treaty limb, net of the exemption.** Status: ✅ Done (`f015b348`)
  Three coupled defects in `statement.rs` credit assembly + `credit.rs`:
  (a) the treaty limb uses **pre-exemption** gross, so withholding on exempt income still lifts the cap — the shipped `dividend_exemption` fixture nets €85.50 where €115.50 is due, and `tests.rs:547` pins the wrong figure;
  (b) the treaty limb is `min(Σ withheld, Σ gross × 15%)` across payers — a 0%-withheld dividend raises the cap for a 30%-withheld one; the correct per-row `DividendEntry.treaty_capped_credit` already exists and is discarded;
  (c) `total_dividend_exemption` is not clamped ≥ 0 (a net-negative eligible pool mints a negative exemption).
  Do: build the treaty limb as `Σ` over dividend rows of `min(withheldᵢ, grossᵢ × treaty_rate)` computed on each row's **taxed** portion (exempt slice allocated pro-rata across exemption-eligible foreign dividends; washed/ineligible rows keep full gross); clamp the exemption at 0; while in there, fix the latent fee pro-ration denominator (`statement.rs:533-537` divides a post-exemption numerator by pre-exemption `rcm_gross` — use one consistent basis). The rate limb (taxable-net) stays as R3 built it.
  Tests: re-pin `dividend_exemption` to credit €112.50 / net tax €115.50 (reviewer's hand computation: AAPL €1,800/€270 eligible, MSFT €450/€67.50 washed, exemption €1,500 ⇒ taxed foreign €750, attributable tax 45 + 67.50); NEW `mixed_withholding` fixture (Común: US €1,000 @30% + IE/UK €1,000 @0% ⇒ credit €150, not €300); negative-eligible-pool unit test (exemption 0, not negative).
  Commit: `fix(spain-tax): compute the treaty limb per dividend net of the exemption`
- **F2 — Overlap guard vs unpriced years.** Status: ✅ Done (`47ce1b56`)
  The R2 overlap guard rejects a carried-in `deferred_losses` entry whose `sale_date` matches ANY same-instrument sale in the statement — including sales in years with no shipped coefficient table, which the replay cannot self-defer (`fiscal_gain_loss: None`); removing the entry then silently loses the deduction. Also: the guard matches on date alone (false positives on unrelated same-day disposals), and the "sales in {years} were not tested" warning fires for years AFTER the filing year (Jan–Mar of year+1 under the documented statement-window workflow — pure noise).
  Do: the guard skips (does not reject against) sales the run could not price; both the guard error and the unpriced-year warning name the `taxes.spain.coefficients.<year>` escape hatch; the unpriced-year warning only fires for years ≤ filing year; tighten the overlap match to (instrument, sale_date, and quantity compatibility) to cut false positives.
  Tests: carried-in 2023 deferral + statement containing the unpriced 2023 loss sale ⇒ accepted, released on the blocking lot's disposal; the year+1 noise case asserts no warning.
  Commit: `fix(spain-tax): let carried-in deferrals coexist with unpriced statement years`

## Phase F1 — Correctness (mediums)

- **F3 — Interest reversals are income corrections, not margin interest.** Status: ✅ Done (`b0d505c4`)
  `processor.rs:975` classifies paid-vs-received by sign alone; IB emits reversals of previously credited interest as negative "Broker Interest Received", which R1 then excludes from income (overstates RCM) and mislabels as margin interest. The ingestion layer (`flex_query.rs:1031`, `ib/interest.rs`) discards the type. Do: carry the distinction through `IdleCashInterest` minimally (additive field; German path must be behaviorally untouched — its processor sums signed amounts regardless); in the Spanish processor, a negative RECEIVED-type entry nets against interest income; a PAID-type entry keeps the R1 informational treatment. Where the statement format genuinely lacks the type, keep the sign heuristic and say so in a comment.
  Tests: fixture with received €100 + reversal −€100 + paid −€250 ⇒ `total_interest_income` 0, `total_paid_interest` 250.
  Commit: `fix(spain-tax): net interest reversals instead of treating them as margin interest`
- **F4 — Disjoint compensation reporting.** Status: ✅ Done (`870881b9`)
  `SUMMARY_RCM_LOSSES_APPLIED` includes the Fase 2ª-2º crossed amount that `SUMMARY_PRIOR_CROSS_OFFSET_RCM_TO_GYP` also reports (AEAT example: €200 printed twice; a filer transcribing both claims €400). Do: make the own-group row exclude the crossed subset (rows become disjoint; their sum is the ledger consumption); update labels/contract/docs so the relationship is explicit. Ledger arithmetic itself is verified correct — reporting only.
  Commit: `fix(spain-tax): report own-group and crossed compensation amounts disjointly`
- **F5 — Wash-sale definitiveness measured on the blocked shares.** Status: ✅ Done (`ff8b2e5a`)
  `wash_sale.rs:228-255` applies `matched_total / disposal.quantity` uniformly, so selling 40 blocked + 60 unblocked shares with 40 repurchased releases €216 of a €360 deferral even though every blocked share was replaced. Do: attribute the window match to the blocked shares first — the re-attached fraction is `min(matched, blocked_consumed) / blocked_consumed` for the deferred amount (document this attribution choice in the code and docs: V3282-18 gives no allocation rule; blocked-first is the conservative reading).
  Tests: the 40+60/40 scenario ⇒ full €360 re-attached, €0 integrated; mirror partial case.
  Commit: `fix(spain-tax): measure wash-sale definitiveness on the blocked shares`
- **F6 — Anti-abuse clause: homogeneity key + unlisted-variant disclosure.** Status: ✅ Done (`802039f4`)
  (a) `processor.rs:930-947` (`dividend_is_washed` and the exemption anti-abuse) match on raw ticker; everything else uses `wash_sale::instrument_key` (ISIN-preferred) — a rename or dual line defeats the check. Route both through `instrument_key`.
  (b) The statute's fourth limb (1-**year** acquire/transfer window for securities not admitted on Directive 2014/65 regulated markets) is unimplemented and undisclosed. IB instruments are listed, but note honestly (code comment + `docs/spain-taxes.md` + `TODO(verify)`) that: the 1-year variant is not modelled, and strictly the 2014/65 "regulated market" definition covers EEA venues — whether US-listed shares fall under the 2-month or 1-year rule rests on equivalence doctrine; the tool applies 2 months to all statement instruments.
  Commit: `fix(spain-tax): match the dividend anti-abuse rule on the homogeneity key`

## Phase F2 — Documentation & consistency

- **F7 — Documentation corrections.** Status: ✅ Done (`543d9de5`) — one commit:
  (a) `docs/spain-taxes.md:326-332`: the US-dividend credit example is impossible post-exemption (a €1,000 Gipuzkoa dividend is fully exempt ⇒ credit €0, contradicting the doc's own §364-367). Replace with two examples: the Gipuzkoa case showing credit €0 + IRS reclaim, and a Común case (base €10,000 ⇒ 19.80% avg rate) showing the real min() chain with F1's per-row limb.
  (b) `--output` flag doesn't exist (positional arg): fix `docs/spain-taxes.md:71`, `csv-output.md:10`, and the same pre-existing falsehood in the German console hint (`src/tax_statement/mod.rs:611`).
  (c) Unify the expired-loss message wording across `carryforward.rs:87-91`, `compensation.rs:130-134`, console (`mod.rs:317-323`), and the doc quote (`spain-taxes.md:502`) — one phrasing everywhere.
  (d) Fee keyword doc adds the unaccented `administracion` (doc lists 5 of 6).
  (e) `remediation.md`: TODO(verify) close-out says "two remain" — three remain (`processor.rs` fee keywords, `wash_sale.rs:33` endpoints, `csv_formatter.rs:22` casillas); R12(e) replaced five markers not four; annotate the R3 note ("credits €135 unchanged") as superseded by R10.
  (f) `plan.md:424`: "27 commits" → point at the actual branch state and at `remediation.md`/this file.
  (g) Add the Código Civil art. 5.1 "de fecha a fecha" basis at the `wash_sale.rs:33` marker (currently only in the docs); quote the CSV banner in `spain-taxes.md:222` with an ellipsis; fix the stale test comment (`tests.rs` "whole €7,200 is taxed" above post-R10 assertions taxing 5,700).
  Commit: `docs(spain-tax): correct the credit example and align documentation with behavior`
- **F8 — CSV contract refresh.** Status: ✅ Done (`4ddfe82f`) — bring `specs/002-spain-tax/contracts/csv-output.md` up to the emitted format:
  row types `Stock Grant` + `Corporate Action` + reintegration in the enum and file-structure prose; the nine undocumented summary keys (`SUMMARY_RCM_DIVIDEND_EXEMPTION`, `SUMMARY_RCM_INTEREST_PAID`, `SUMMARY_GYP_DEFERRED`, `SUMMARY_GYP_REINTEGRATED`, both `SUMMARY_PRIOR_CROSS_OFFSET_*`, `SUMMARY_FOREIGN_TAXABLE_INCOME`, `WASH_SALE_WINDOW_OPEN`, `SHORT_POSITION`) plus F4's revised rows; delete the phantom `# FX BORROWED BALANCE` banner; align banner wording with the code's actual text; document per-regime warning banners and the withholding-warning's non-zero condition; complete "populated by" for reintegration (`isin`, `gain_loss_eur`) and Stock Grant (`quantity`, `gross_amount_eur` — note the column reuse and extend the column-placement test to the grant row); document the summary preamble and `# SALDOS NEGATIVOS PENDIENTES` / `# PÉRDIDAS DIFERIDAS PENDIENTES` banners and the key-shape asymmetry (`MODELO_109_CASILLA_<n>` vs `MODELO_100_<n>`); pandas recipe gains `comment="#"` on the summary read; make the `coefficient` cell use half-away-from-zero like every other numeric cell (it currently `rescale(3)`s half-to-even) and say so.
  Commit: `docs(spain-tax): bring the CSV contract up to date with the emitted format`
- **F9 — Mechanical code batch.** Status: ✅ Done (`4e93a094`) — one commit:
  (a) exemption `warn!` payer list: sort + dedup fully (`Vec::dedup` only kills consecutive duplicates — "STRC, GOOG, NVDA, STRC");
  (b) missing `regime` errors as serde's bare "missing field" — surface it as a proper error naming `taxes.spain.regime` (the one Spanish validation that breaks the name-the-path convention);
  (c) FX-borrowed `warn!`: route the amount through `format_eur` and align its wording with the console twin;
  (d) re-home the orphaned doc comment on `MAX_COEFFICIENT` back onto `gipuzkoa_coefficient`;
  (e) delete the stale "printed at full precision" comment in `csv_formatter.rs:497-503`;
  (f) extend the casilla-identity regression test with a compensation case (box 28 pre- vs box 33 post-compensation) so the pinned identity states its scope;
  (g) `SUMMARY_GYP_CAPITAL_GAINS` / `SUMMARY_GYP_FX` read from stored statement fields instead of recomputing in the formatter; unify the `net_tax_due` label ("Cuota líquida del ahorro") across console, CSV and sell-simulation; deduplicate the regime display strings into one place;
  (h) sharpen the `eur.rs` rounding comment: name the negative-exact-half-cent divergence between the cited "por exceso" text and `MidpointAwayFromZero`, and same note on the 4-dp rate rounding (`scale.rs:146`).
  Commit: `chore(spain-tax): close the second-review low-severity findings`
- **F10 — Gate, smoke, close-out.** Status: ✅ Done — see the close-out at the end of this file. Task text: full `./check` + `cargo test spain`; re-run the real-statement smoke test (`--config /private/tmp/claude-501/-Users-alexandre-projects-alexsavio-investments/864d554e-7895-4b99-9187-48e9e68b5156/scratchpad/es-smoke`, portfolio `ibkr-miren`, year 2025): expect base €17.32 / net €3.46 unchanged (credit already 0 there — F1 must not move it) and the exemption warning payer list deduplicated; append a close-out section here (fixed/accepted list, final numbers); update memory-worthy statuses in `plan.md`/`remediation.md` if any cross-references changed.
  Commit: `docs(spain-tax): close the second review round`

## Accepted as-is (record, don't fix)

- Chained re-attachment never terminating while repurchases continue — correct per V3282-18.
- `processor.rs:704` 31-Dec `expect` — unreachable behind the scale gate (reviewer-confirmed).
- Console prints fewer rows than the CSV (summary-vs-audit split) — by design; F4/F9(g) fix only the double-count and recomputation.
- Carried-in `deferred_losses` quantities not split-adjusted across statements — documented limitation.
- YAML-equivalent one-line vs two-line carry-out block formatting; unquoted-YAML-floats convention note.

## Close-out (F10)

**Date**: 2026-08-11 · Branch `002-spain-tax`, ten tasks on top of `b5138036`.

### Commits

| Task | Commit |
|---|---|
| F1 per-dividend treaty limb, net of the exemption | `f015b348` |
| F2 carried-in deferrals vs unpriced statement years | `47ce1b56` |
| F3 interest reversals net against income | `b0d505c4` |
| F4 disjoint own-group / crossed compensation rows | `870881b9` |
| F5 wash-sale definitiveness on the blocked shares | `ff8b2e5a` |
| F6 anti-abuse rule on the homogeneity key | `802039f4` |
| F7 documentation corrections | `543d9de5` |
| F8 CSV contract refresh | `4ddfe82f` |
| F9 low-severity batch | `4e93a094` |

Plus three `docs(plan)` status commits, one per phase.

### Gate

| Check | Result |
|---|---|
| `cargo check --lib --all-targets` | clean |
| `cargo test spain --lib` | **226 passed / 0 failed** (215 at the start of this round) |
| `cargo test --lib` | **692 passed / 33 failed** — every failure is a pre-existing `broker_statement::*::parse_real` case from the private, empty `testdata/` submodule; filtering the failure list for anything that is not a `parse_real` case yields 0 rows, and the count is unchanged |
| `cargo test --no-fail-fast` (all targets) | lib as above; `tests/generate.rs` 1 passed; binary target has no tests |
| `./check` (clippy, dev + release, `-Dwarnings`) | the same **4 pre-existing errors** in untouched files: `portfolio/rebalancing.rs`, `quotes/cbr/mod.rs`, `analysis/performance/statistics.rs`, `formats/xls/table.rs`. No new lint at any commit |

### Real-statement smoke test

`cargo run -- --config <scratch>/es-smoke tax-statement ibkr-miren 2025 <scratch>/es-review2.csv`,
Gipuzkoa 2025. Every filed figure is **unchanged**: base liquidable €17.32, cuota íntegra €3.46,
credit €0.00, cuota líquida €3.46, ganancias y pérdidas €16.18. F1 could not move the credit there
because every dividend on that statement is exempt, so the treaty limb runs on a taxed slice of zero
and the credit was already 0.

A diff of the CSV against the pre-round output shows three changed lines, all labels, no numbers:
the two `SUMMARY_*_LOSSES_APPLIED` rows now say "aplicados al propio grupo … fase 2ª-1º" (F4) and
casilla 64 is relabelled "Cuota líquida del ahorro" (F9g). The exemption warning's payer list is now
`GOOG, NVDA, STRC` — sorted and fully deduplicated, against `STRC, GOOG, NVDA, STRC` before (F9a) —
and the FX-borrowed `warn!` prints `€-17.18` through `format_eur`, matching its console twin (F9c).

### Numbers

F1's acceptance numbers reproduce exactly as the reviewer hand-computed them:

- `dividend_exemption` (Gipuzkoa): the €1,500 exemption lands wholly on AAPL, the only eligible
  payer, so the treaty limb is `min(270, 300 × 15%) + min(67.50, 450 × 15%) = 45 + 67.50 = 112.50`
  against a rate limb of `750 × 19% = 142.50` ⇒ **credit €112.50**, net tax `228 − 112.50 =` **€115.50**.
- `mixed_withholding` (Común, new fixture): `min(300, 1,000 × 15%) + min(0, 1,000 × 15%) = 150`
  against a rate limb of `2,000 × 19% = 380` ⇒ **credit €150**, where the pooled cap would have
  allowed €300.

### Deviations from the letter of the plan

- **F2** also requires the clashing sale to be a **loss** before the overlap guard fires, on top of
  the specified instrument / date / quantity-compatibility match. A gain never creates a deferral, so
  a same-day same-instrument gain cannot be the sale a carried-in entry describes; the extra
  condition only removes false rejections and cannot admit a double deduction.
- **F1** leaves the per-row `treaty_capped_credit_eur` column measured on the **full** gross rather
  than overwriting it with the post-exemption limb. That column is what an over-withholding reclaim
  from the source state is measured against, and the `income` fixture pins it as such. The
  divergence is documented in the field doc, in the CSV contract and in `docs/spain-taxes.md`.
- **F6**'s fixture is `dividend_homogeneity`, a dual-line rename where both tickers appear in the
  statement. A dividend-only ticker cannot carry an ISIN: the Flex reader registers ISINs from trade
  rows and statement-of-funds lines, not from cash transactions, so an instrument that never trades
  falls back to its ticker as its homogeneity key.

### Left open

- The four `TODO(verify)` markers: the custody-fee keyword list (`processor.rs`), the window
  endpoints (`wash_sale.rs`, now carrying the Código Civil art. 5.1 "de fecha a fecha" basis), the
  Modelo 109 casillas (`csv_formatter.rs`), and the new one F6 added — whether a US-listed share
  falls under the two-month or the one-year limb under Directive 2014/65's EEA-only definition of a
  regulated market. The tool applies two months to every instrument and says so in the docs.
- Everything under "Accepted as-is" above, unchanged.
- The pre-existing 33 `parse_real` failures and 4 clippy errors in untouched files.

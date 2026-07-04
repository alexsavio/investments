# German Tax Feature — Remediation Work Plan

- **Branch**: `001-germany-tax`
- **Audit date**: 2026-07-01
- **Audience**: implementing agent (Claude Opus). This document is self-contained: it embeds the audit findings, the German tax rules being targeted, the existing codebase APIs to reuse, and per-task acceptance criteria.
- **Audit verdict being remediated**: BLOCK — the feature produces wrong numbers on real data. Build is clean, all 46 German-tax tests pass, but the tests construct entries by hand and never exercise the cost-basis calculation, the Flex Query ingestion, or currency conversion.

---

## How to work this plan

1. Execute phases in order. Within a phase, tasks are ordered by dependency. Do not reorder Phase 1 — every later number depends on it.
2. **Write the failing test first** for every task that has a "Tests" section, then fix, then verify. The whole audit happened because the existing 46 tests only cover hand-built entries.
3. After each task: `cargo check`, then `cargo test germany`, then `cargo clippy --all-targets -- -D warnings` scoped to touched files. Full gate before each commit: `cargo fmt --check && cargo clippy --all-targets && cargo test`.
4. Note: `cargo test --lib` has 33 pre-existing failures in `broker_statement::{bcs, firstrade, ib, open, sber, tbank}::parse_real` — they need private `testdata/` fixtures that are not in the repo. They are NOT yours to fix; do not touch them and do not let them block you.
5. One conventional commit per task (`fix(germany-tax): ...`, `feat(germany-tax): ...`). No `Co-Authored-By` trailers.
6. Where this plan says "verify against the law/form", and you cannot verify, implement it as written here, and add a `// TODO(verify): <what>` comment plus a note in the final report. Do not silently guess something different.
7. Smallest diff that works. Do not reformat untouched code. Do not bundle refactors into fix commits.

Key existing infrastructure you must reuse (details in Appendix B):

- `StockSell::calculate(&Country, &Instrument, &[TaxExemption], &CurrencyConverter) -> SellDetails` — already computes correct FIFO with per-lot details. **This replaces the broken hand-rolled cost basis.**
- `Payments::add/reverse` (`src/broker_statement/payments.rs`) — reversal-aware accumulator for dividends/taxes.
- `localities::germany(&config.taxes) -> Country` — Germany `Country` with `currency = "EUR"`.
- `instrument_info.get_or_empty(&symbol) -> MaybeOwned<Instrument>` (`src/instruments.rs:136`).

---

## Phase 0 — Test harness (do this first)

### T0. Integration fixture test for the Flex Query → German CSV pipeline

**Status: ✅ Done** (commit `457fdf10`) — fixtures + harness under `src/tax_statement/germany/testdata/{fifo,income_edge}` and `tests.rs`; parser-edge test in `flex_query.rs`.

**Problem**: no test exercises the real pipeline. All five CRITICAL bugs live in code paths with zero coverage.

**Do**:

1. Create `testdata/germany/` (new, committed — synthetic data only) with a hand-written IBKR Flex Query XML fixture containing at minimum:
   - Two buys and two sells of the same symbol (e.g., buy 100 AAPL @ $10 Jan, buy 100 @ $20 Feb, sell 100 Mar, sell 100 Apr) — catches the FIFO bug (T2).
   - One dividend + its withholding tax present in **both** `StmtFunds` and `CashTransactions` sections — catches double-ingestion (T3).
   - One negative dividend row (`amount="-8.75"`, a reversal) — catches the panic (T3).
   - One RSU vest (`StockGrantActivity` with FMV) followed by a sale of those shares — catches grant cost basis (T2).
   - A `CashReport` with a `currency="BASE_SUMMARY"` row — catches the phantom currency (T3).
   - A cancelled trade row (`buySell="SELL (Ca.)"` with `origTradeID`) — catches the hard parse failure (T3).
   Model the XML structure on the serde structs in `src/broker_statement/ib/flex_query.rs` and IBKR's Flex documentation; keep amounts round so expected values are hand-computable.
2. Add `tests/germany_tax.rs` (or a `#[cfg(test)]` module in `src/tax_statement/germany/`) that: parses the fixture, runs `process_broker_statement` + `calculate_totals` + `GermanCsvFormatter::write` with a **mock converter** (the test backend in `src/currency/converter.rs:202-228` shows the pattern — fixed rates, no network), and asserts exact expected EUR values computed by hand.
3. Until T1–T5 are fixed the assertions will fail. Write the assertions against the **correct** values (computed per Appendix A), mark the test `#[ignore = "germany-tax remediation in progress"]`, and remove the ignore attribute as tasks land. The final state of this plan is: test enabled, all assertions green.

**Acceptance**: fixture committed; test compiles; each subsequent task removes at least one wrong-value assertion failure.

---

## Phase 1 — Fix the money pipeline (every downstream number depends on this)

### T1. Wire ECB exchange rates; stop converting EUR amounts through the Russian Central Bank

**Status: ✅ Done** (commit `9e2938a6`) — `pub mod ecb;` wired; rate direction inverted (`price = 1/parsed_rate`); `CurrencyConverter::new_ecb` with a cache-key-namespaced EUR-base backend; German flow switched to it.

**Problem (CRITICAL)**: `src/quotes/ecb.rs` is dead code — there is no `mod ecb;` declaration in `src/quotes/mod.rs`, so the file is never compiled. The German flow (`src/tax_statement/mod.rs:203`) builds `CurrencyConverter::new(database, None, true)` whose backend is hardwired to CBR (`src/currency/converter.rs:139`, `cbr::Cbr::new("https://www.cbr.ru")`, `cbr::BASE_CURRENCY = "RUB"`). Every USD→EUR conversion is a USD→RUB→EUR cross-rate from cbr.ru, with fallback windows governed by `localities::get_russian_central_bank_min_last_working_day`. Docs (`docs/germany-taxes.md:180`) claim ECB rates; the error message in `processor.rs:36` blames "missing ECB exchange rates".

**Known latent bugs inside `ecb.rs` you must fix while wiring it** (found in audit, currently unreachable):

- **Rate direction inversion**: `get_historical_currency_rates` (`ecb.rs:117-159`) returns ECB series values as-is. ECB series `D.USD.EUR.SP00.A` is *USD per EUR* (~1.08). The rate-cache convention (set by CBR, `src/quotes/cbr/mod.rs:119-123`) is *base-currency units per 1 unit of foreign currency* (e.g., RUB per USD). For an EUR-base cache the stored price for USD must be **EUR per USD**, i.e. `1 / ecb_rate`. If you store the raw ECB value, every conversion is wrong by ~rate² (≈17% for USD). The single-quote path `get_quote` (`ecb.rs:213-229`) already divides correctly — mirror that.
- `get_currency_rates` (`ecb.rs:57-58`) queries `EXR/D..EUR.SP00.A` with no `startPeriod`/`lastNObservations` → downloads the full 1999–present history for ~30 currencies to read one observation. Add query bounds.
- `ecb.rs:38, 185-189`: a `OnceLock<GenericResult<...>>` caches transient HTTP **errors** for the process lifetime. Cache only successes.
- `ecb.rs:102-105`: comment says "3 business days", code checks `days_old > 5` calendar days. Make code and comment agree.

**Design** (keep it minimal — do not build a generic multi-provider abstraction):

1. Declare `pub mod ecb;` in `src/quotes/mod.rs`.
2. Parameterize `CurrencyRateCacheBackend` (`src/currency/converter.rs:128-201`) with a rate source instead of the hardwired `cbr` field. A small internal enum is enough:
   ```rust
   enum RateSource { Cbr(cbr::Cbr), Ecb(ecb::Ecb) }
   ```
   with `base_currency(&self) -> &'static str` ("RUB" / "EUR") and `get_historical_currency_rates(...)`. Replace the two uses of `cbr::BASE_CURRENCY` in the conversion math (`converter.rs:253-278`) with `self.base_currency()`.
3. **Cache separation**: `CurrencyRateCache` rows are currently implicitly RUB-based. ECB rows must not mix with them. Choose the simplest safe option and document it: add a diesel migration adding a `base_currency` column to the currency-rates table (default `'RUB'` for existing rows) and include it in the cache key. If the schema/migration machinery makes this heavy, an acceptable fallback is namespacing the currency key (store `"EUR:USD"`), but the column is cleaner.
4. Fallback window: replace the Russian-holiday bound for the EUR base with an ECB equivalent — ECB reference rates are published on TARGET business days; a backward search bounded at ~7 calendar days covers Christmas/New Year TARGET closures. Keep the existing **backward-only** search semantics (`converter.rs:250-283` — verified correct in audit: no look-ahead, no future rates).
5. Selection: `generate_german_tax_statement` constructs the converter with the ECB source; the Russia/USA flow keeps CBR. Thread this through `CurrencyConverter::new` (e.g., a `base: RateSourceKind` parameter or a `CurrencyConverter::new_ecb` constructor) — pick whichever produces the smaller diff at the existing call sites.
6. Fix the misleading error text in `processor.rs:36` only if it can still lie (after this task it becomes true).

**Tests**:
- Unit: EUR-base backend converts USD→EUR by *dividing* by the ECB USD-per-EUR rate: with rate 1.08, USD 108 on a covered date → EUR 100.00 (not 116.64).
- Unit: weekend date falls back to Friday's rate; a date more than the bound in the past with no rate errors out.
- The `#[cfg(test)]` mock-rates pattern in `converter.rs:202-228` needs an EUR-base variant for T0's integration test.

**Acceptance**: `grep -rn "cbr" src/tax_statement/` returns nothing reachable from the Germany path; ECB unit tests (currently never compiled!) run and pass; T0 fixture EUR amounts match hand-computed ECB-rate values.

### T2. Replace the hand-rolled cost basis with `StockSell::calculate()` / per-lot FIFO

**Status: ✅ Done** (commit `8494c7ef`) — `process_trades` uses `StockSell::calculate()`; grant lots costed at vest-date FMV; per-lot Altbestand; tax year keyed off conclusion date; dead `taxes/germany/capital_gains.rs` deleted.

**Problem (CRITICAL)**: `calculate_cost_basis` (`src/tax_statement/germany/processor.rs:285-333`):
- Re-matches every sale against **all** buys from the beginning without consuming lots used by earlier sales. Buy 100 @ €10 (Jan), buy 100 @ €20 (Feb), sell 100 (Mar), sell 100 (Apr) → both sales get the €10 lot; April's true cost basis (€2,000) is reported as €1,000, overstating the gain by €1,000.
- Skips `StockSource::Grant` lots via `continue` **without decrementing `remaining_qty`** → RSU shares get costed from later purchases instead of vest-date FMV (contradicting `docs/germany-taxes.md:230-238`).
- Ignores buy-side commissions (Anschaffungsnebenkosten) → gains overstated.

Also (CRITICAL): `check_pre_2009_holding` (`processor.rs:336-350`) marks the **entire sale** tax-exempt if *any* historical buy of the symbol predates 2009-01-01. Ten shares bought 2008 + 1,000 bought 2020 → every sale of the symbol becomes fully tax-free. Altbestand (§52 Abs. 28 S. 11 EStG grandfathering) applies **per acquired lot**, not per symbol.

**The fix — use what the codebase already computes.** `BrokerStatement::read` runs `statement.process_trades(None)` (`src/broker_statement/mod.rs:226`), which performs real FIFO matching. Each `StockSell` then has private matched `sources`; the public API is:

```rust
// src/broker_statement/trades.rs
pub fn calculate(&self, country: &Country, instrument: &Instrument,
                 tax_exemptions: &[TaxExemption], converter: &CurrencyConverter)
    -> GenericResult<SellDetails>
```

With `country = localities::germany(&config.taxes)` (currency EUR) and `tax_exemptions = &[]`, `SellDetails` gives you, already in EUR:
- `local_revenue` (converted at `execution_date`), `local_commission` (at `conclusion_time.date`), `total_local_cost` (per-lot purchase cost incl. buy commissions), `local_profit`;
- `fifo: Vec<FifoDetails>` — per matched lot: `quantity`, `multiplier` (split-adjusted), `conclusion_time`, `execution_date`, `source` (`Trade { cost, local_cost, commission, local_commission, .. }` | `Grant` | `CorporateAction`), and `cost(currency, converter)`.

**Do** in `process_trades` (`processor.rs:146-282`):

1. Build the Germany `Country` once (pass it in or construct from `tax_config`), get `let instrument = broker_statement.instrument_info.get_or_empty(&trade.symbol);`, call `trade.calculate(&country, &instrument, &[], converter)?`.
2. Replace `proceeds_eur`/`commission_eur`/`cost_basis_eur`/`gross_gain_loss` with `SellDetails.local_revenue`, `.local_commission`, `.total_local_cost`, `.local_profit`. Delete `calculate_cost_basis` entirely.
3. **Altbestand per lot**: from `details.fifo`, compute the profit share of lots with `lot.conclusion_time.date < date!(2009, 1, 1)` and exclude only that share from the taxable amount (attribute revenue to lots proportionally by `quantity * multiplier`; each lot's cost comes from `lot.cost("EUR", converter)?`). Keep a note in the entry when a pre-2009 lot participated. Delete `check_pre_2009_holding`.
4. **Grant lots**: `StockSourceDetails::Grant` lots have zero trade cost in `SellDetails`. For German tax the cost basis of vested RSU shares is the vest-date FMV (that FMV was already taxed as employment income). Look up the matching `broker_statement.stock_grants` entry (symbol + `lot.conclusion_time.date` == vest date) and use `fmv_per_share * quantity` converted at vest date as that lot's cost. If FMV is missing, warn (as `process_stock_grants` already does) and use zero — conservative direction.
5. **Tax-year assignment**: filter by `trade.conclusion_time.date.year() == year` (the obligatory transaction date governs the tax year in Germany), not `execution_date.year()` (`processor.rs:156`). Keep `transaction_date`/`settle_date` columns as they are. Document in one comment that FX conversion dates follow `SellDetails` conventions (revenue at settlement, cost at purchase settlement) — a documented simplification; do not invent a third convention.
6. Delete the dead FIFO module `src/taxes/germany/capital_gains.rs` (`FifoQueue`, `FifoLot`, `calculate_capital_gain` — all `#[allow(dead_code)]`) and its tests, or rewrite its tests against the real pipeline. Do not leave two FIFO implementations in the tree.

**Tests** (in T0's fixture): the two-sale scenario must yield cost bases €1,000 (hypothetical EUR-converted) for sale 1 and €2,000 for sale 2; the RSU sale's gain must be measured against vest FMV; a synthetic pre-2009 + post-2009 mixed sale must tax exactly the post-2009 lots' profit share.

### T3. Flex Query parser: stop double-ingesting income; handle reversals and real-world rows

**Status: ✅ Done** (commit `7eeb0281`) — all 9 sub-items: income dedup when CashTransactions present, negative-dividend reversal, BASE_SUMMARY skip, cancelled-trade voiding, `trades_found` fix, short-position filter, fee ingestion, multi-account error, symbol-keyed ISIN.

**Problem (CRITICAL)**: `src/broker_statement/ib/flex_query.rs` parses dividends (`DIV`), withholding tax (`FRTAX`), and broker interest (`CINT`) from the `StmtFunds` section (lines 609-640) **and** from the `CashTransactions` section (lines 832-860), unconditionally. `docs/ibkr-flex-query.md:39-43` instructs users to enable **both** sections. `Payments::add` just accumulates → same-currency entries are silently **doubled**; mixed-currency entries die with "Mixed currency" (`payments.rs:93`). Trades and forex already have exactly the dedup guards this needs (`trades_found` at 461-477, `skip_forex` at 482) — dividends/WHT/interest were simply missed.

**Do**:

1. Make section presence detectable: deserialize `CashTransactions` as `Option<...>` (present-but-empty ≠ absent). When the `CashTransactions` section is **present**, it is the authoritative source for dividends/WHT/interest; skip the `DIV`/`FRTAX`/`CINT` activity codes in `parse_statement_of_funds`. When absent, fall back to StmtFunds. Mirror the naming/pattern of `skip_forex`.
2. **Negative dividends (HIGH)**: both ingestion paths pass raw amounts into `Payments::add`, which `assert!(amount.is_positive())` → a dividend correction row (`type="Dividends" amount="-8.75"`, common in real IB data) panics the process. Copy the CSV parser's handling (`src/broker_statement/ib/dividends.rs:39-43`): negative → `accruals.reverse(date, -amount)`, positive → `add`. Apply the same to WHT rows (note the existing FRTAX sign convention at lines 619-633 is already reversal-aware — verified in audit; make the CashTransactions path match it).
3. **`BASE_SUMMARY` phantom currency (HIGH)**: `flex_query.rs:453-459` deposits every `CashReportCurrency` row, including IB's `currency="BASE_SUMMARY"` aggregate → cash double-counted and later conversion of a fake currency fails. Skip rows where `currency == "BASE_SUMMARY"` (and any non-ISO placeholder).
4. **Cancelled trades (MEDIUM)**: `buySell="BUY (Ca.)"` / `"SELL (Ca.)"` currently hits `Err!("Unknown trade direction")` (`flex_query.rs:819-821`) and kills the whole statement. Correct handling: a cancellation row voids its original — drop the cancellation row *and* the original trade it references (`origTradeID` attribute). If `origTradeID` is missing/unmatched, return a clear error naming the trade.
5. **`trades_found` false positive (MEDIUM)**: `flex_query.rs:461-468` sets `trades_found = true` even when every trade was skipped as non-STK, so the StmtFunds fallback never runs. Set the flag only when at least one trade was actually ingested.
6. **Short positions (MEDIUM)**: `flex_query.rs:497-503` lets negative `position` values through to `add_open_position`, which enforces `StrictlyPositive` and aborts. Filter `pos.position > 0` and `warn!` on short positions (German tax handling of shorts is out of scope; do not silently drop without a warning).
7. **Fees (MEDIUM)**: `flex_query.rs:867-870` drops "Other Fees"/"Commission Adjustments" at debug level, so `statement.fees` is always empty for XML statements and the German fee section is a no-op. Ingest them into `statement.fees` (matching the CSV path) — note T5 changes what fees *mean* for the tax math, but they must still be parsed and reported.
8. **Multi-statement (MEDIUM)**: only `statements[0]` is processed (`flex_query.rs:429-432`). Either process all `FlexStatement` elements or return an explicit error for multi-account queries. Silent truncation is not acceptable.
9. **ISIN keying (MEDIUM)**: `flex_query.rs:572-574, 803-806` call `instrument_info.get_or_add(&trade.isin)` — creating phantom instruments whose *symbol is the ISIN* and never linking symbol→ISIN. Match the CSV path (`ib/dividends.rs:28`): `get_or_add(&symbol).add_isin(parsed_isin)` (parse with the existing `ISIN` type; warn on invalid).

**Tests**: T0 fixture assertions — dividend total counted once; reversal reduces the accrual instead of panicking; parse succeeds with the cancelled-trade and BASE_SUMMARY rows present; fee appears in the CSV report section.

---

## Phase 2 — Align the tax math with German law

Read Appendix A before starting this phase; every formula and worked example you need is there.

### T4. Church tax: fix config scaling and implement the §32d(1) formula

**Status: ✅ Done** (commit `b7f8e237`) — one shared `GermanTaxRates::compute_taxes` implements `max(0, e−4q)/(4+k)` with Soli and KiSt off the post-credit tax; config accepts 0/8/9 or 0.08/0.09 and rejects the rest; per-item `q = min(WHT, 15% gross, 25% taxable)` with `q=0` for funds; pre-2009 returns an error not a panic; dead `calculate_german_taxes`/`calculate_dividend_tax` deleted; docs and effective-rate table corrected.

**Problem (CRITICAL)**:
1. Docs (`docs/germany-taxes.md:19`, `docs/config-example.yaml:193`) tell users `church_tax_rate: 9`, but the value flows raw into the math (`src/tax_statement/mod.rs:190` → `rates.rs:31`), where it is used as a *fraction*: `kirchensteuer = abgeltungssteuer * 9` → **900% church tax**. All unit tests pass `dec!(0.09)`, masking it.
2. Even with a correct fraction the formula is wrong: §32d(1) S.3-4 EStG *reduces* the income tax when church tax applies — `tax = income / (4 + k)` (k = 0.08 or 0.09) because church tax is deductible as a Sonderausgabe inside the flat-rate formula. The code instead computes 25% and stacks KiSt on top (28.625% effective at 9%; correct is ≈27.99%). Additionally, per the full statutory formula `tax = (e − 4q) / (4 + k)`, creditable foreign tax `q` reduces the Abgeltungsteuer **before** Soli — Soli = 5.5% of the post-credit tax. The current code computes Soli on the pre-credit tax.

**Do**:

1. Config normalization in one place (`TaxConfig` validation or where it is read in `tax_statement/mod.rs:190`): accept the documented percent form. Rule: value must be one of 0, 8, 9 (or 0.08/0.09 for backward compat) — normalize to fraction, reject anything else with a clear error listing valid values. Update `docs/config-example.yaml` and `docs/germany-taxes.md` to state the accepted forms.
2. Replace the per-entry tax computation (three copies: `processor.rs:232-241` capital gains, `419-422` dividends, `492-496` interest, plus `574-583` FX) with **one** shared function implementing §32d(1):
   ```text
   abgeltungsteuer = max(0, (taxable − 4·q)) / (4 + k)        // k = church fraction (0/0.08/0.09), q = creditable foreign tax on THIS item
   kirchensteuer   = abgeltungsteuer · k
   soli            = abgeltungsteuer · 0.055
   total           = abgeltungsteuer + kirchensteuer + soli
   ```
   Put it in `src/taxes/germany/rates.rs` as the *only* tax function; the per-item creditable `q` comes from T4.3 below. Note this makes the separate "foreign_tax_credit subtracted afterwards" step obsolete for the *tax* columns — keep reporting the credit amount, but `net_tax` per entry is simply `total` from the formula (the credit is already inside). Update entry fields/CSV semantics accordingly and say so in the column docs.
3. **Creditable foreign tax per item** (fixes the MEDIUM cap bug at `processor.rs:425`): `q = min(withholding_paid, treaty_cap, statutory_cap)` where `treaty_cap = 15% of the GROSS dividend` (not of the post-Teilfreistellung amount) and `statutory_cap = 25% of the taxable amount` (§32d(5)). For **fund** distributions (any entry with a Teilfreistellung classification ≠ None): under InvStG 2018 the investor cannot credit fund-level foreign WHT — set `q = 0`, report the withheld amount informationally with a note. (Irish/Lux UCITS distribute without WHT anyway; this mainly guards correctness.)
4. Delete the two dead, mutually inconsistent implementations: `rates::calculate_german_taxes` (`rates.rs:65-81`) and `dividends::calculate_dividend_tax` (`dividends.rs:50-80` — its credit cap incl. Soli is wrong). Port anything their tests still cover onto the new shared function.
5. Replace the `panic!` for pre-2009 years (`rates.rs:25`) with a returned error (`GenericResult`), propagated from `GermanTaxStatement::new`.
6. Fix the misdocumented rates in `docs/germany-taxes.md:85-91` (the "28.625%" table) with the correct effective rates (Appendix A) and the test comment `rates.rs:122` claiming 9% is "Bavaria" (Bavaria/BW are 8%).

**Tests**: worked examples from Appendix A as unit tests — €1,000 income at k=0.09 → Abgeltungsteuer 244.50, KiSt 22.00, Soli 13.45 (2dp, half-up); €1,000 US dividend with $150 WHT and k=0 → tax 100.00, Soli 5.50, total 105.50 (the current code says 113.75 — this delta is the test's reason for existing).

### T5. Loss offsetting pots, Sparer-Pauschbetrag, and removal of the illegal fee deduction

**Status: ✅ Done** (commit `1466588e`) — `calculate_totals` keeps separate §20(6) stock and general pots (gain/loss by `taxable_amount` sign), each with its own prior-year carryforward and next-year residual; the general pot offsets dividends and interest; Sparer-Pauschbetrag (config split into stock/other single amounts) reduces the combined positive result; fees are informational only (§20(9)); the summary tax is computed once on the final base, not summed from rows.

**Problem (HIGH ×3)** in `GermanTaxStatement::calculate_totals` (`src/tax_statement/germany/statement.rs:318-492`):
1. `net_capital_gain_loss = gains − losses + fx_gains − fx_losses − fees` merges everything into one pot and applies one carryforward. §20(6) S.4 EStG: losses from **share** sales (Aktien — direct stock only, *not* fund/ETF units) offset only share-sale gains. Everything else (fund-sale losses, FX losses, general losses) lives in the general pot, which offsets **all** capital income *including dividends and interest* — which the code explicitly refuses to do (`statement.rs:409` comment).
2. Fees are deducted from gains as "Werbungskosten". §20(9) EStG **prohibits** deducting actual expenses under the Abgeltungsteuer — only the Sparer-Pauschbetrag applies. This understates tax (the dangerous direction). `docs/germany-taxes.md:217-224` asserts the opposite of the law.
3. Sparer-Pauschbetrag (€1,000 single since 2023; €801 before) is entirely absent → "Net Tax Due" overstated for everyone.
4. Consequence bug (HIGH): `total_german_tax`/`net_tax_due` are sums of *per-entry* taxes (computed on gains only), while `total_taxable_income` applies the carryforward — the two disagree whenever losses or carryforward exist.

**Do** — restructure `calculate_totals` into an explicit pipeline (this is the one place where a rewrite, not a patch, is correct):

1. Classify every entry into pots. `CapitalGainEntry` needs an `is_stock` flag: true when the instrument has no fund classification (`TeilfreistellungRate::None` *and* not classified as `Bond` in the ETF config — an explicit `bond` classification means it IS a fund, general pot). Fund-sale gains/losses (any Teilfreistellung classification incl. Bond) → general pot, using post-Teilfreistellung amounts (TF applies symmetrically to losses — the existing `taxable_amount` handling is correct).
2. Offsetting order (per year):
   - Stock pot: stock gains − stock losses − stock-loss carryforward → `stock_result` (≥ 0 taxable; a negative remainder becomes next year's stock carryforward).
   - General pot: fund gains/losses + FX gains/losses + interest + dividends + other §20 income − general-loss carryforward → `general_result` (same carryforward semantics).
   - `taxable_before_allowance = stock_result + general_result`.
   - `taxable = max(0, taxable_before_allowance − sparer_pauschbetrag)`.
3. Config: split `taxes.loss_carryforward` into `loss_carryforward_stock` and `loss_carryforward_other` — each a single EUR amount for the *festgestellter Verlustvortrag* as of Dec 31 of the prior year (not a per-year map; the current `BTreeMap<year, amount>` summed over **all** years including future ones is both semantically undefined and buggy). Add `sparer_pauschbetrag: Option<Decimal>` defaulting to €1,000 for years ≥ 2023, €801 before (allow 0 for users whose allowance is consumed at a German bank). Keep reading the old key with a deprecation error message telling the user how to migrate.
4. Compute the **summary** tax by applying T4's shared formula to the final `taxable` figure (with aggregate creditable foreign tax), replacing the sum-of-entries approach. Keep per-entry tax columns as *informational per-item* values, and add a CSV comment line saying summary tax ≠ sum of rows because pots/carryforward/allowance apply at year level.
5. Remove `− self.total_fees` from the pool. Keep collecting and reporting fees (they're useful information and the Finanzamt sometimes accepts specific transaction-linked costs), but label the CSV section "informational — not deductible under §20(9) EStG (Abgeltungsteuer)" and fix `docs/germany-taxes.md`.
6. Emit both next-year carryforwards (stock / general) as separate summary lines.

**⚠ Altbestand interaction (from T2 code review — must handle here):** after T2 a `CapitalGainEntry`'s `taxable_amount` already excludes the pre-2009 (Altbestand) profit share, but `gross_gain_loss` does **not**. The current `calculate_totals` classifies gain-vs-loss by the sign of `gross_gain_loss` (`statement.rs:336-341`) while summing `taxable_amount` — so a mixed pre-2009/post-2009 sale (gross positive, `taxable_amount` negative) is misfiled into the gains pot. When rebuilding the pots, take the gain/loss sign and the pot amounts from `taxable_amount` (or add an explicit `pre_2009_excluded` field on the entry), never from raw `gross_gain_loss`.

**Tests**: stock loss €500 + FX gain €500 → taxable general €500, stock carryforward €500 (NOT net zero); general loss €300 + dividends €1,000 → dividends reduced to €700 before allowance; €900 total income − €1,000 Pauschbetrag → tax €0; a mixed Altbestand-gain + post-2009-loss sale is filed by `taxable_amount` sign, not gross; the T0 fixture's summary line values.

### T6. Fix the Anlage KAP mapping

**Status: ✅ Done** (commit `9283aeb7`) — Anlage KAP reports non-fund income only (Zeile 19 net of contained losses, Zeile 20 share-sale gains, Zeilen 22/23 split non-share vs share-sale losses, Zeile 41 credit); fund income (any Teilfreistellung class, incl. bond) moves to a gross-value Anlage KAP-INV section by fund type; Zeile 19/20 built from Altbestand-adjusted `taxable_amount` so a pure pre-2009 sale lands on neither line; docs describe the KAP/KAP-INV split and the not-yet-computed Vorabpauschale.

**Problem (HIGH)** (`statement.rs:432-491`): Zeile 19 is computed from **gains only**, while Zeilen 22/23 declare losses that the official form defines as "contained in" lines 18/19 — a Finanzamt processing these numbers double-counts the losses. Zeile 20 (contained gains from *share* sales, needed to operate the stock pot) is missing entirely. Fund income is reported on KAP although foreign-held **investment fund** income belongs on **Anlage KAP-INV** (gross, pre-Teilfreistellung — the Finanzamt applies the exemption itself).

**Do** (line numbers per the 2024/2025 Anlage KAP; add `// TODO(verify): re-check line numbers against the <year> form` since forms shift):

1. Split entries: **fund entries** (Teilfreistellung classification ≠ None, incl. Bond) go to a new KAP-INV section; **non-fund** entries stay on KAP.
2. KAP values (non-fund only, gross amounts, losses INCLUDED in the totals):
   - `Zeile 19` (ausländische Kapitalerträge): net sum of ALL foreign capital income — dividends + interest + stock-sale gains *and* losses + FX results.
   - `Zeile 20`: gains from share sales contained in Zeile 19 (gross gains only).
   - `Zeile 22`: contained losses **without** share-sale losses (FX losses, other §20 losses).
   - `Zeile 23`: contained share-sale losses.
   - `Zeile 41`: creditable foreign tax (T4.3 per-item `q` summed, non-fund entries).
   - Sanity invariant to assert in a test: `zeile_19 == (all positives) − zeile_22 − zeile_23`.
3. New KAP-INV section: per fund-type group (Aktienfonds / Mischfonds / sonstige), report **gross** distributions and **gross** sale gains/losses (pre-Teilfreistellung), each as its own labeled summary line. Do not apply Teilfreistellung to these reported values (keep applying it in the tool's own tax *estimate*). Label the section clearly: "Anlage KAP-INV — enter gross values; the tax office applies Teilfreistellung".
4. Update `docs/germany-taxes.md` to describe the KAP vs KAP-INV split and that Vorabpauschale (T11) is not yet included.

**⚠ Altbestand interaction (from T2 code review — must handle here):** `kap_zeile_19` currently sums raw `gross_gain_loss` for entries with `gross_gain_loss > 0` (`statement.rs:448-453`). Post-T2, a pure Altbestand sale has `gross_gain_loss > 0` but `taxable_amount == 0`, so it over-declares the tax-free pre-2009 gain into the Zeile the user actually files. Build Zeile 19/20 from the Altbestand-adjusted amount (`taxable_amount`, or an explicit `pre_2009_excluded` field), never from raw `gross_gain_loss`; pre-2009 gains/losses belong on neither KAP line.

**Tests**: fixture with one stock gain, one stock loss, one fund dividend, one FX loss → assert each Zeile value and the invariant above; a pure Altbestand sale contributes 0 to Zeile 19/20.

---

## Phase 3 — Correctness hardening

### T7. Replace the FX margin-loan magnitude heuristic

**Problem (HIGH)**: `determine_fx_is_margin_loan` (`flex_query.rs:760-781`) classifies any FX conversion with `|quantity| > 100` as a non-taxable margin-loan repayment; `processor.rs:559` then drops the gain from taxable income. Converting $5,000 of accumulated dividend cash to EUR at a profit → silently untaxed (§20 Abs. 2 EStG exposure). The threshold is also unit-confused (FX units, not EUR as the comments claim) and the StmtFunds fallback path (lines 666-668) uses a *different* heuristic.

**Do**: a conversion repays a margin loan only if the **currency balance was negative** (borrowed). Track the running per-currency cash balance while parsing (the statement has ending balances and all cash flows; IB Flex also exposes `levelOfDetail`/balance fields — use what the fixture provides). Classification: the portion of a sold currency amount that closes a negative balance is margin-loan repayment; the rest is a taxable disposal. If balance tracking is genuinely impossible from the available sections, then **fail open for tax purposes**: classify everything as taxable and let a warning tell the user to review margin-related conversions — never silently exempt income on a size heuristic. Use one code path for both parse paths.

**Tests**: negative starting USD balance + conversion → margin (non-taxable); positive balance + same-size conversion → taxable.

### T8. Remove symbol-pattern derivative detection

**Problem (HIGH)**: `is_derivative` (`processor.rs:45-83`) drops any symbol ending in `W` or containing `WS`/`WT`/`NOTE`/`CERT` — real tickers (GLW Corning, WST West Pharmaceutical, …) silently vanish from the tax report (income omission). For the Flex path the check is redundant: IB provides `assetCategory` and `parse_trade` already keeps only `STK`.

**Do**: delete `is_derivative`/`warn_if_derivative` and rely on asset-category filtering at the parser level. To preserve FR-016 (warn on skipped derivatives), emit the warning **in the parser** when a non-STK category (`OPT`, `FUT`, `WAR`, `CFD`, …) is skipped, naming the symbol and category. Update `test_derivative_detection` into a parser-level test (STK passes; OPT row warns and is excluded).

### T9. Explicit rounding at output

**Problem (MEDIUM, empirically verified)**: `GermanCsvFormatter::format_decimal` uses `format!("{:.2}", value)`, which on `rust_decimal` **truncates** (verified: `0.518 → "0.51"`, `0.015675 → "0.01"`). The shipped sample CSV contains a row whose components don't sum (GOOG: 1.60 + 0.08 + 0.00 vs. total 1.69).

**Do**: in `csv_formatter.rs:398-400`:
```rust
fn format_decimal(value: Decimal) -> String {
    let mut v = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    v.rescale(2);
    v.to_string()
}
```
Round **once** per reported figure (components and totals each rounded from full-precision values; a one-cent add-up difference between rounded components and the rounded total is acceptable and standard — but truncation is not). Half-up (`MidpointAwayFromZero`) matches German tax-form practice for per-line amounts.

**Tests**: `format_decimal(dec!(0.518)) == "0.51"`? No — `"0.52"`. Assert `"0.52"`, `"0.02"` for `0.015675`, `"1.50"` for `1.5`.

### T10. Cleanup batch (one commit, mechanical)

1. `jurisdiction` config: replace the `Option<String>` + `Some("germany") | Some("Germany")` match (`config.rs`, `get_tax_country`) with a serde enum `Jurisdiction { Russia, Germany }` (lowercase rename attr) so a typo is a config **error**, not a silent fallback to Russia.
2. CSV structure (`csv_formatter.rs`): give summary/KAP rows the full 21-column shape (pad with empty fields) or move them to a clearly separated second block after a blank line with their own 3-column header; escape `notes` through `escape_csv`; replace `N/A` in numeric columns with empty fields. Update `contracts/csv-output.md` to match.
3. Dividend entry `quantity` is hardcoded `dec!(0)` (`processor.rs:431`) — leave the field empty in CSV instead of a misleading zero.
4. `flex_query.rs:928`: `&datetime_str[..8]` byte-slice can panic on multi-byte UTF-8 — use `datetime_str.get(..8).ok_or_else(...)`.
5. `flex_query.rs:656`: fix the one clippy warning on the branch (collapsible `if`).
6. Add `german-tax-*.csv` to `.gitignore` — `german-tax-2025.csv` in the repo root is real personal trading data and must never be committed.
7. `ib/taxes.rs` `is_interest_withholding_tax`: the fully anchored regex aborts the CSV parse on any description variation — degrade to a warning + skip instead of a hard error.
8. `localities::germany` (`localities.rs`): add a comment that the flat 26.375% `FixedTaxRate` is an approximation for analysis/rebalancing views only (no church tax, no Teilfreistellung, no allowance) — the statement module owns the real math.

---

## Phase 4 — Deferred (do NOT start without explicit go-ahead)

### T11. Vorabpauschale (advance lump-sum taxation for funds)

Missing entirely and *not* in the spec's out-of-scope list; for accumulating ETFs at a foreign broker this is a mandatory yearly taxable event since 2023. It is deferred because it needs year-start/year-end NAVs per fund (new data dependency).

Sketch for when approved: per fund holding — `basisertrag = nav_jan1 × basiszins(year) × 0.7`, prorated by 1/12 for each full month before purchase in the acquisition year; `vorabpauschale = max(0, min(basisertrag − distributions_of_year, max(0, nav_dec31 − nav_jan1)))`; taxed (with Teilfreistellung) as income of the **first business day of the following year**; accumulated Vorabpauschalen reduce the taxable gain at sale. Basiszins: 2023 = 2.55%, 2024 = 2.29%, 2025 = 2.53% (BMF publishes each January — make it a config table with these defaults). Verify against §18 InvStG / current BMF letter before implementing. **Until implemented**: emit a prominent warning in the CSV + console when any configured fund classification appears in the statement, saying Vorabpauschale is not computed.

### T12. Multi-account Flex statements, short-position support, KAP-INV per-line form numbers

Each needs either product decisions or external verification; tracked here so they aren't forgotten.

---

## Appendix A — German tax rules cheat sheet (with worked numbers)

All amounts EUR. `k` = church-tax fraction (0, 0.08, 0.09), `q` = creditable foreign tax, `e` = taxable capital income.

**§32d(1) flat tax formula**: `tax = (e − 4q) / (4 + k)`, floored at 0. Soli = 5.5% × tax. KiSt = k × tax.

| Case | Input | Abgeltungsteuer | KiSt | Soli | Total | Effective |
|---|---|---|---|---|---|---|
| No church, no credit | e=1000, k=0, q=0 | 250.00 | 0 | 13.75 | 263.75 | 26.375% |
| Church 9% | e=1000, k=0.09, q=0 | 244.50 | 22.00 | 13.45 | 279.95 | ≈27.99% |
| Church 8% | e=1000, k=0.08, q=0 | 245.10 | 19.61 | 13.48 | 278.19 | ≈27.82% |
| US dividend, 15% WHT | e=1000, k=0, q=150 | 100.00 | 0 | 5.50 | 105.50 | — |

(Current code produces 286.25 for the k=0.09 row and 113.75 for the WHT row — both wrong.)

**Creditable foreign tax (per item)**: `q = min(WHT paid, 15% × gross dividend [treaty], 25% × taxable amount [§32d(5)])`; `q = 0` for investment-fund distributions (InvStG 2018).

**Teilfreistellung (InvStG §20)**: equity fund (≥51% equity) 30%, mixed (≥25%) 15%, other/bond 0%. Applies to distributions, sale gains, sale **losses** (symmetric), and Vorabpauschale. The taxpayer reports gross on KAP-INV; the Finanzamt applies it.

**Loss pots (§20(6))**: share-sale losses (direct stock only — fund units are NOT Aktien here) offset only share-sale gains, carryforward indefinitely in their own pot. All other §20 losses form the general pot, offsetting dividends, interest, fund gains, FX gains. No carry-back.

**Werbungskosten (§20(9))**: actual expenses NOT deductible; only the Sparer-Pauschbetrag — €1,000 single / €2,000 joint since 2023 (€801/€1,602 before).

**Altbestand**: lots acquired before 2009-01-01 — sale gains tax-free (§52(28) S.11 EStG); decided per FIFO lot.

**FIFO (§20(4) S.7)**: per security per depot, first-in-first-out.

**FX (BMF 19.05.2022, Rz. 131)**: currency gains in interest-bearing accounts = §20 Abs. 2 Nr. 7 capital income; repayment of a currency **loan** (negative balance) is not a disposal event.

**RSUs**: vest-date FMV = employment income (Anlage N, marginal rate); sale taxed under §20 with cost basis = vest FMV.

**Cash grants**: sonstige Einkünfte §22 Nr. 3, Freigrenze €256/year (Freigrenze = exceed it and the WHOLE amount is taxable, not just the excess), Anlage SO.

## Appendix B — Existing APIs to reuse (verified signatures)

```rust
// src/broker_statement/trades.rs
pub struct StockSell { pub symbol, pub quantity, pub type_: StockSellType,
    pub conclusion_time: DateOptTime, pub execution_date: Date, /* sources: private */ }
impl StockSell {
    pub fn calculate(&self, country: &Country, instrument: &Instrument,
        tax_exemptions: &[TaxExemption], converter: &CurrencyConverter) -> GenericResult<SellDetails>;
}
pub struct SellDetails {
    pub revenue: Cash, pub local_revenue: Cash, pub local_commission: Cash,
    pub purchase_cost: Cash, pub purchase_local_cost: Cash, pub total_local_cost: Cash,
    pub profit: Cash, pub local_profit: Cash, pub taxable_local_profit: Cash,
    pub fifo: Vec<FifoDetails>,
}
pub struct FifoDetails {
    pub quantity: Decimal, pub multiplier: Decimal,
    pub conclusion_time: DateOptTime, pub execution_date: Date,
    pub source: StockSourceDetails,   // Trade{price,commission,local_commission,cost,local_cost} | CorporateAction | Grant
    pub fn cost(&self, currency: &str, converter: &CurrencyConverter) -> GenericResult<Cash>;
    pub fn price(&self, currency: &str, converter: &CurrencyConverter) -> GenericResult<Cash>;
}
// "local" = country.currency → EUR when country = localities::germany(...)

// src/instruments.rs
pub fn get_or_empty(&self, symbol: &str) -> MaybeOwned<'_, Instrument>;  // line 136

// src/broker_statement/payments.rs
impl Payments { pub fn add(&mut self, date: Date, amount: Cash);      // asserts positive!
                pub fn reverse(&mut self, date: Date, amount: Cash); } // asserts positive!
pub enum Withholding { Withholding(Cash), Refund(Cash) }  // Withholding::new(amount) handles sign

// src/currency/converter.rs
pub trait CurrencyConverterBackend { fn today(); fn batch(from,to,date); fn currency_rate(from,to,date); }
// CurrencyRateCacheBackend: cbr field + get_rates() are the two CBR touchpoints (lines 130, 198);
// conversion math uses cbr::BASE_CURRENCY at lines 254, 266; #[cfg(test)] mock rates at 202-228.

// src/localities.rs
pub fn germany(config: &TaxConfig) -> Country;  // currency "EUR", tax_precision 2
```

## Appendix C — Audit finding → task index

| Audit finding | Severity | Task |
|---|---|---|
| ECB provider dead code; conversions via CBR/RUB | CRITICAL | T1 |
| ECB rate direction inverted (latent) | CRITICAL (latent) | T1 |
| FIFO cost basis re-matches consumed lots | CRITICAL | T2 |
| Grant lots skipped without consuming qty; buy commissions ignored | CRITICAL | T2 |
| Whole-position Altbestand exemption | CRITICAL | T2 |
| Dividends/WHT/interest double-ingested (StmtFunds + CashTransactions) | CRITICAL | T3 |
| Negative dividend → assert panic | HIGH | T3 |
| BASE_SUMMARY phantom currency | HIGH | T3 |
| Church tax ×100 config scaling; §32d(1) formula; Soli pre-credit | CRITICAL | T4 |
| Foreign credit cap on post-TF amount; fund WHT credited | MEDIUM | T4 |
| Fees deducted contra §20(9) | HIGH | T5 |
| Single merged loss pot; carryforward not applied to dividends/interest | HIGH | T5 |
| Sparer-Pauschbetrag missing | HIGH | T5 |
| Summary tax ignores loss offsetting | HIGH | T5 |
| loss_carryforward config sums all years | MEDIUM | T5 |
| KAP Zeile 19 gains-only; Zeile 20 missing; funds not on KAP-INV | HIGH | T6 |
| FX margin heuristic (\|qty\| > 100) drops taxable gains | HIGH | T7 |
| is_derivative false positives drop real tickers | HIGH | T8 |
| Output truncation via format!("{:.2}") | MEDIUM | T9 |
| Settlement-date tax year | MEDIUM | T2.5 |
| jurisdiction string silent fallback; CSV structure; unwraps; misc parser robustness | MEDIUM | T3, T10 |
| Vorabpauschale missing | HIGH (deferred) | T11 |

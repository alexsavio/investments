# Spanish Tax Feature (Gipuzkoa + Territorio Común) — Implementation Plan

**Date**: 2026-08-11
**Base branch**: `001-germany-tax` → create feature branch `002-spain-tax`
**Goal**: an `investments tax-statement` path (plus `simulate-sell` support) that computes Spanish IRPF savings-income tax on IB broker statements, first-class for the **Gipuzkoa foral regime** (Norma Foral 3/2014, as amended by Norma Foral 1/2025), with **Territorio Común** (Ley 35/2006 LIRPF) sharing the same engine behind a config switch.

Decisions already made with the user — do not re-litigate:

- Both regimes, selected by `taxes.spain.regime: gipuzkoa | comun`. Gipuzkoa is implemented and verified first.
- Priority tax year **2026** (the user's first Spanish filing; new 9-bracket Gipuzkoa savings scale applies). 2024/2025 tables ship too.
- Wash-sale rule ("valores homogéneos", ±2-month window): **full deferral + automatic re-integration** on later disposal.

## How to work this plan

1. Work phases in order; within a phase, tasks in order. Do not start a task before its predecessors' commits exist.
2. Failing test first: every task names its test(s); write them, watch them fail, then implement.
3. Per-task gate: `cargo check` → `cargo test spain` → `cargo clippy --workspace --all-targets --all-features --no-deps -- -D warnings` (i.e. `./check`). The ~33 pre-existing `broker_statement::*::parse_real` failures come from the private, empty `testdata/` submodule — they are not yours; never "fix" them.
4. One task = one Conventional Commit, message given per task. No `Co-Authored-By` trailers.
5. When a legal detail cannot be verified from an official source, implement the documented best interpretation and mark the code line `// TODO(verify): <what> — <source consulted>`.
6. Update task **Status** lines in this file in-place via `docs(plan)` commits as you complete phases.
7. Smallest diff that works. No bundled refactors beyond the two explicitly ordered ones (S5, S10). Mirror the German feature's structure and conventions everywhere a choice exists.

## Verified legal basis (S1 complete — verified 2026-08-11 against the sources in "S1 source manifest" below)

| Rule | Gipuzkoa (NF 3/2014) | Territorio Común (LIRPF) |
|---|---|---|
| Savings-base groups | RCM = rendimientos del capital mobiliario (dividends, interest); GyP = ganancias y pérdidas patrimoniales from transfers, incl. FX conversion gains. `TODO(verify)`: foral article numbers for the two groups (assumed arts. 65–66) were not reachable | arts. 46.a / 46.b, integrated per art. 49 — **verified** |
| Savings scale, 2026+ | art. 76.1 (per **NF 1/2025 art. 13.Tres**, effective 2026-01-01): 19% ≤ 7,500 / 20% ≤ 15,000 / 22% ≤ 30,000 / 24% ≤ 50,000 / 25.5% ≤ 90,000 / 26% ≤ 120,000 / 26.5% ≤ 240,000 / 27% ≤ 300,000 / 28% above — **confirmed exactly, incl. the law's own cuota-íntegra column** | 19% ≤ 6,000 / 21% ≤ 50,000 / 23% ≤ 200,000 / 27% ≤ 300,000 / **30%** above (arts. 66.1.1º + 76, aggregate = art. 66.2). Unchanged for 2026: no PGE 2026 (prorogued), arts. 66/76 untouched since Ley 7/2024 |
| Savings scale, ≤ 2025 | 20% ≤ 2,500 / 21% ≤ 10,000 / 22% ≤ 15,000 / 23% ≤ 30,000 / 25% above — **confirmed exactly** for 2024 **and** 2025. Structural note: the pre-reform scale publishes **no** cuota-íntegra column (bands of the base, not limit/cuota/resto) | **2025**: 19 / 21 / 23 / 27 / **30**. **2024**: 19 / 21 / 23 / 27 / **28** ⚠️ — the > 300,000 bracket was created at **28%** by Ley 31/2022 (PGE 2023) and only rose to 30% from 2025 via Ley 7/2024 df 7ª. The plan previously had 30% for 2024; that was wrong |
| FIFO | **art. 47.2** — confirmed; current wording also covers "criptoactivos fungibles del mismo tipo", added by **NF 2/2025 de 24 de noviembre** (not NF 1/2025, as this table originally said). Official foral guidance: sole-owner and jointly-held holdings run **separate** FIFO queues (out of scope, doc note) | **art. 37.2** — confirmed, scoped to art. 37.1 letters a)/b)/c) |
| Wash sale | **art. 43.g)** listed (MiFID II, Dir. 2014/65/UE): homogeneous acquired **2 months before or after**. **art. 43.h)** unlisted: **1 year** (out of scope, doc note). **art. 43.i)** fungible crypto, 2 months (out of scope). Loss is **deferred, not destroyed** | **art. 33.5.f)** listed ("mercados secundarios oficiales… Directiva 2004/39/CE", 2 months); **art. 33.5.g)** unlisted (1 year); art. 33.5.e) is a separate 1-year rule for non-securities assets |
| Wash-sale reintegration trigger | Both regimes, verbatim: losses "se integrarán **a medida que se transmitan los activos que permanezcan en el patrimonio** de la persona contribuyente" — i.e. keyed to the securities **remaining in the estate**, which is broader than "the repurchased lot". See the S14–S16 note below | same (art. 33.5, closing paragraph) |
| Cross-group offset in savings base | **None** — groups compensate "exclusivamente entre sí". `TODO(verify)`: foral article number | Negative balance of one group offsets the other's positive balance up to **25%** of it (art. 49.1) — **verified in force for 2024, 2025 and 2026**. (Phase-in was 10/15/20% in **2015/2016/2017** per DA duodécima, so 25% has applied since **2018** — the plan's "phased in 2018–2021" was wrong) |
| Loss carryforward | 4 following years, oldest first, "en la cuantía máxima que permita cada uno de los ejercicios siguientes" | **art. 49.1, confirmed**: "se compensará en los cuatro años siguientes"; art. 49.2 forbids deferring beyond the window by stacking onto later losses |
| Actualization coefficients | **Yes — art. 45.2** (not 46/47 as this plan assumed). Table approved annually by Decreto Foral: **DF 58/2023** → disposals 2024, **DF 61/2024** → 2025, **DF 27/2025** → 2026. Applied **per FIFO lot** by the lot's acquisition year. ⚠️ Trap: an asset acquired **exactly on 31-12-1994** takes the **1995** coefficient, so the table is non-monotonic at that seam | **Abolished** by Ley 26/2014 art. 1.21 (deleted LIRPF art. 35.2) with effect 2015-01-01 → coefficient = 1 always. Even before then it applied **only to inmuebles**, never to securities |
| Foreign WHT credit ("deducción por doble imposición internacional") | **art. 91**, confirmed: lesser of (a) tax actually paid abroad, (b) **the savings average rate** applied to "la renta obtenida en el extranjero". Refinement: art. 91.b splits general vs **savings** average rate explicitly; savings rate is art. 76.2 = cuota ÷ base liquidable del ahorro, 2 dp | **art. 80**, confirmed: lesser of (a) tax paid abroad, (b) "tipo medio efectivo de gravamen" × "la parte de **base liquidable** gravada en el extranjero"; art. 80.2 requires separating general vs savings rates |
| Custody/administration fees | ⚠️ **REFUTED — NOT deductible.** Gipuzkoa has **no** equivalent of state art. 26.1.a. **art. 39** is a closed list: RCM net = gross income, with deductions only for asistencia técnica / arrendamiento de bienes muebles, negocios o minas / subarrendamientos (art. 39.2). Confirmed by the Diputación Foral's own Renta manual ch. 4 §4.5 | **art. 26.1.a**, confirmed deductible: "gastos de administración y depósito de valores negociables", **excluding** the fee for "gestión discrecional e individualizada de carteras" |
| Return form | **Modelo 109** "Autoliquidación del IRPF", confirmed (Orden Foral, BOG nº 53, 20-03-2026); filed via the Zergabidea platform | **Modelo 100**, confirmed (AEAT procedimiento G600) |

**Plan-changing consequence of the custody-fee finding (affects S9 and the architecture):** fee deductibility is now **regime-dependent**, not a Spain-wide constant. `SpanishTaxParams` gains `custody_fees_deductible: bool` (`false` for Gipuzkoa, `true` for Común), and `process_fees` routes on it. Under Gipuzkoa every fee is informational — which happens to match Germany's §20(9) treatment, the opposite of what this plan originally assumed. Note the Común deduction is also **narrower** than "any broker fee": custody/administration only, never discretionary-management fees, and never commissions (those are already inside the FIFO cost basis via `SellDetails`).

**Consequence of the reintegration wording (affects S14–S16):** the statutes key reintegration to the disposal of *securities remaining in the estate*, not specifically to the disposal of the *repurchased lot*. This plan's design (attach the deferred loss to the blocked lots and release pro-rata as those lots are consumed by later FIFO-matched sales) is a defensible reading and is what gets implemented, but it is an interpretation: mark it `// TODO(verify): reintegration keyed to blocked lots vs. any remaining homogeneous holding — NF 3/2014 art. 43 closing ¶ / LIRPF art. 33.5 closing ¶`. Under FIFO with a single instrument the two readings coincide in the common case; they diverge when the taxpayer holds untouched pre-existing lots alongside the repurchased ones.

### S1 source manifest (all retrieved 2026-08-11)

| Item | Source | Tier |
|---|---|---|
| Gipuzkoa savings scale 2026 (art. 76.1) + 2024/25 scale + enacting chain NF 1/2025 art. 13.Tres | NF 1/2025 full text PDF (`primeralecturaediciones.com/documentos_diana/LEYES_2025/GUIPUZKOA/NF_1_2025_Gipuzkoa_IRPF.pdf`); `iberley.es/legislacion/articulo-76-irpf-gipuzkoa`; `noticias.juridicas.com/base_datos/CCAA/521360-...`. Underlying act BOG nº 90, 15-05-2025 | SECONDARY ×3, mutually consistent; table is arithmetically self-consistent across all 8 cuota steps |
| Gipuzkoa coefficients, disposals **2026** — DF **27/2025** de 23 dic, disp. adic. 1ª | `egoitza.gipuzkoa.eus/gao-bog/castell/bog/2025/12/30/c2508991.htm` (BOG nº 249, 30-12-2025) | **OFFICIAL** |
| Gipuzkoa coefficients, disposals **2025** — DF **61/2024** de 27 dic | `egoitza.gipuzkoa.eus/gao-bog/castell/bog/2024/12/30/c2409648.htm` (BOG nº 250, 30-12-2024); cross-checked against `gipuzkoa.eus/es/web/ogasuna/impuestos/renta/coeficientes-actualizacion-2025` | **OFFICIAL** ×2 |
| Gipuzkoa coefficients, disposals **2024** — DF **58/2023** de 28 dic | `egoitza.gipuzkoa.eus/gao-bog/castell/bog/2023/12/29/c2309618.htm` (BOG nº 250, 29-12-2023); cross-checked against `gipuzkoa.eus/.../coeficientes-actualizacion-2024` | **OFFICIAL** ×2 |
| Gipuzkoa arts. 39 (RCM net), 43 (wash sale), 45.2 (coefficients), 47.2 (FIFO), 91 (DDII) | `noticias.juridicas.com` consolidated NF 3/2014 | SECONDARY |
| Gipuzkoa custody fees **not** deductible | `gipuzkoa.eus/documents/2456431/80349127/04+-+Rend+capital+mobiliario.pdf` (Manual de Renta, cap. 4 §4.5) | **OFFICIAL** |
| Gipuzkoa FIFO operational guidance (separate queues per ownership form) | `gipuzkoa.eus/es/web/ogasuna/-/balore-homogeneoak-eta-ondare-galera-edo-irabaziak` | **OFFICIAL** |
| Modelo 109 for period 2025 + campaign dates | `egoitza.gipuzkoa.eus/gao-bog/castell/bog/2026/03/20/c2601821.pdf` (BOG nº 53, 20-03-2026) | **OFFICIAL** |
| Modelo 109 casilla map | `egoitza.gipuzkoa.eus/documents/39465/25500467/5.FE-Errenta_maila_PFEZ-es.pdf` | **OFFICIAL but AÑO 2019** — see `TODO(verify)` below |
| LIRPF arts. 26, 33, 35, 37, 46, 49, 66, 76, 80, DA duodécima | BOE consolidated Ley 35/2006 `boe.es/buscar/act.php?id=BOE-A-2006-20764` + BOE Open Data API `/legislacion-consolidada/id/BOE-A-2006-20764/texto/bloque/a<N>`; consolidation `fecha_actualizacion` 2026-04-29, tracking norms up to RDL 10/2026 | **OFFICIAL** |
| State scale 2024 (28% top) | AEAT Manual Práctico IRPF 2024, `sede.agenciatributaria.gob.es/.../gravamen-base-liquidable-ahorro.html`; Ley 31/2022 art. 63; Ley 7/2024 df 7ª | **OFFICIAL** |
| No PGE 2026 (prorogued 2025 budget) | `sepg.pap.hacienda.gob.es/.../PGE2025Prorroga` | **OFFICIAL** |
| Modelo 100 | AEAT Sede, procedimiento G600 | **OFFICIAL** |

**Open `TODO(verify)` items from S1** (nothing else was left unverified):

1. **Modelo 109 casilla numbers** — Gipuzkoa publishes no static numbered form (the return is generated by Zergabidea). The only official box map reachable is explicitly dated **AÑO 2019**: RCM casillas 06+16, RCM deductible expenses 17, RCM retenciones 07+22, ganancias patrimoniales 28, base imponible general 19, **base imponible del ahorro 33**, cuota líquida 64. Treat as indicative only for 2024–2026 — the 2026 reform added a cuota-íntegra column and Anexo 3 gained a crypto section, both of which plausibly renumbered boxes.
2. **Modelo 109 casilla for the deducción por doble imposición internacional** — not published anywhere reachable. Unknown for every year.
3. **Gipuzkoa article numbers for the two savings-base groups and for the "exclusivamente entre sí" cross-offset prohibition** (assumed arts. 65–66) — not reachable; the *substance* (no cross-offset in the foral regime) is not in doubt, only the citation.
4. **Treaty rate cap on the WHT credit** — neither NF 3/2014 art. 91 nor LIRPF art. 80 mentions a treaty cap; it comes from the bilateral DTA itself (15% on dividends under the US and German treaties). Implemented as a configurable `treaty_rate` defaulting to 0.15.
5. **DF 27/2025 coefficient table** — verified OFFICIAL from BOG directly, and independently reproduced identically by a second research pass, so this is high-confidence. Noted only because Gipuzkoa's own summary index page still 404s for 2026 (site lag). A *Corrección de errores* (BOG nº 38, 26-02-2026) exists but only renumbers a disposición transitoria and does **not** touch the coefficient table.
6. **`fecha de transmisión` for listed securities** — which of conclusion/execution date governs the Spanish tax year was not researched; the plan's conclusion-date choice stands with its existing `TODO(verify)`.

Other facts: tax year = calendar year; tax currency EUR; ECB reference rates acceptable (already wired via `RateSourceKind::Ecb`); no Vorabpauschale equivalent (accumulating funds simply defer); Spanish fund-traspaso deferral does **not** apply at foreign brokers (doc note); Modelo 720 is an informational obligation only (doc reminder, never computed).

Golden vectors for the 2026 Gipuzkoa scale — the law's own cumulative cuota íntegra at each threshold (use in S6 unit tests):
`tax(7500)=1425 · tax(15000)=2925 · tax(30000)=6225 · tax(50000)=11025 · tax(90000)=21225 · tax(120000)=29025 · tax(240000)=60825 · tax(300000)=77025`, plus `tax(400000)=105025` and mid-bracket `tax(10000)=1925` (⇒ `average_rate(10000)=0.1925`). See Appendix A for the other scales' vectors.

## Architecture

Mirror the German two-layer split exactly. **No cross-jurisdiction trait** — Germany and Spain are two structurally different regimes (pots+allowance+flat formula vs groups+ledgers+progressive scale); revisit abstraction only if a third jurisdiction lands.

### Layer 1: `src/taxes/spain/` — pure law primitives (no `BrokerStatement` dependency)

| File | Contents |
|---|---|
| `mod.rs` | module wiring; `pub enum SpanishTaxRegime { Gipuzkoa, Comun }` — `Clone, Copy, Debug, PartialEq, Eq, Deserialize`, `#[serde(rename_all = "lowercase")]` |
| `scale.rs` | `pub struct SavingsScale { brackets: Vec<(Decimal, Decimal)> }` (floor, marginal rate). `for_year(regime, year) -> GenericResult<SavingsScale>` — ships 2024–2026 both regimes, errors on other years naming the supported range; `tax(base) -> Decimal` (full precision, no per-slice rounding); `average_rate(base) -> Decimal` (= tax/base, 0 when base ≤ 0; needed by the WHT credit cap). Do **not** reuse `taxes::rates::ProgressiveTaxRate` here: it is stateful across calls (`tax_base` accumulates), rounds per slice, and exposes no average rate. It IS the right tool for the `localities::spain` analysis approximation (S3). |
| `coefficients.rs` | `pub fn gipuzkoa_coefficient(disposal_year, acquisition_year, overrides: &BTreeMap<i32, BTreeMap<i32, Decimal>>) -> GenericResult<Decimal>` — ships the official DF tables for disposal years 2024/2025/2026 (populated in S1/S7); acquisition years older than the table's oldest row clamp to that row; unknown disposal year errors naming `taxes.spain.coefficients.<year>`; overrides win over shipped tables. Callers pass coefficient 1 for Común. |
| `carryforward.rs` | `pub struct LossLedger { balances: BTreeMap<i32, Decimal> }` — origin-year-labeled negative balances (positive magnitudes). `from_config(&BTreeMap<i32, Decimal>, filing_year)` rejects origin ≥ filing year and origin < filing_year − 4 ("expired — remove it from the config"); `apply(amount, cap: Option<Decimal>) -> LedgerApplication { used_total, used_by_year }` consumes oldest-first (cap carries the Común 25% cross-offset limit); `add(origin_year, loss)`; `total()`; `expiring_after(filing_year)`. The German `taxes/germany/loss_carryforward.rs` helper is a single unlabeled balance and cannot express 4-year expiry — leave it untouched with its only consumer. |
| `compensation.rs` | `pub fn compensate_savings_base(regime, filing_year, rcm_net, gyp_net, rcm_ledger, gyp_ledger) -> CompensationResult`. Sequence: (1) within-group ledger application to each group's positive net; (2) **Común only**: remaining negative of one group offsets the other group's remaining positive, capped at 25% of that positive (one cap per group per year); Gipuzkoa skips this entirely; (3) remaining negatives labeled with the filing year join the next-year ledgers; entries falling out of the 4-year window are dropped with `warn!` naming the expired amount. Result carries: `rcm_taxable`, `gyp_taxable`, `savings_base`, per-group `LedgerApplication`s, `cross_offset_*`, and both `*_ledger_next`. |
| `credit.rs` | `pub fn double_taxation_credit(withheld_eur, gross_eur, taxable_eur, treaty_rate, average_savings_rate) -> Decimal` = `min(min(withheld, gross × treaty_rate), taxable × average_savings_rate)`, floored at 0. Treaty rate default 0.15. |

### Layer 2: `src/tax_statement/spain/` — statement pipeline

| File | Contents |
|---|---|
| `mod.rs` | module wiring; `pub(crate) use crate::tax_statement::eur::{format_eur, round_eur};` (after S5) |
| `processor.rs` | `pub fn compute_tax_year(broker_statement, year, converter, tax_config, opening_foreign_currency) -> GenericResult<SpanishTaxStatement>` mirroring `germany::compute_tax_year`'s shape. Internal `struct SpanishTaxParams { regime, year, scale, applies_coefficients, cross_offset_fraction, treaty_rate, custody_fees_deductible }` resolved **once** via `SpanishTaxParams::resolve(tax_config, year)` so processors never re-match the enum. (`custody_fees_deductible` added by S1: `false` for Gipuzkoa, `true` for Común — see the legal-basis table.) Per-income processors (keep this file < 800 lines; wash-sale logic lives in `wash_sale.rs`): `process_trades` (below), `process_dividends` + `process_interest` (RCM; store the treaty-capped credit *candidate* per entry — final credit is a year-level number), `process_fees` (custody/administration fees deductible from RCM **only under Común** per art. 26.1.a, and never the discretionary-portfolio-management fee; under **Gipuzkoa nothing is deductible** — art. 39 is a closed list — so every fee is informational. Other fee types informational under both regimes, conservative), `process_fx_gains` (shared `fx_fifo` engine after S10; **all held-balance realizations → GyP**; borrowed-balance/margin results → `fx_borrowed_review` for manual review, not taxed), `process_stock_grants` (report-only — employment income belongs to the general base, out of scope), `process_corporate_actions` (report-only, mirror Germany). Tax-year predicate `produces_capital_gain(trade, year)`: conclusion-date year + `StockSellType::Trade`, `// TODO(verify)` fecha de transmisión for listed securities. |
| `wash_sale.rs` | the valores-homogéneos engine — algorithm below |
| `statement.rs` | `SpanishTaxStatement` + entry types + `calculate_totals()` — model below |
| `csv_formatter.rs` | CSV per the contract you will write at `specs/002-spain-tax/contracts/csv-output.md` (S12), modeled on the German contract: transaction rows, then per-FIFO-lot `lot` rows (acquisition_date, quantity, cost_eur, coefficient, actualized_cost_eur, proceeds_eur, gain_eur — the coefficient math must be auditable line by line), blank line, `summary_key,label,value_eur` block, `# WARNING:` caveat lines for unverified Modelo boxes |
| `tests.rs` + `testdata/*/statement.xml` | committed synthetic IB Flex XML fixtures + a **duplicated** `FixedEurBackend` mock converter (~30 test-only lines copied from `germany/tests.rs:22-51`; the top-level `testdata/` submodule is private and empty, so in-src fixtures are the only runnable path) + `run_pipeline`/`run_pipeline_with_config` helpers |

#### Trade processing (S8)

For each qualifying `StockSell`:

1. `let details = trade.calculate(&country, &instrument, &[], converter)?` — the shared engine (`broker_statement/trades.rs`: `SellDetails { local_revenue, local_commission, fifo: Vec<FifoDetails> }`) already yields EUR amounts and per-lot acquisition data. Grant lots (`StockSourceDetails::Grant`) are costed at vest-date FMV exactly as the German processor does (`processor.rs:409`).
2. Per lot: `proceeds_share = local_revenue × lot_qty/total_qty` (commission share likewise); `cost = lot.total_cost("EUR", converter)?`; `coefficient = params.applies_coefficients ? gipuzkoa_coefficient(year, lot.conclusion_time.date.year(), overrides)? : 1`; `lot_gain = proceeds_share − commission_share − cost × coefficient`. `// TODO(verify)` an actualized cost may enlarge a loss (historic foral practice: yes — implement as written).
3. Emit one `CapitalGainEntry` per sale carrying `lots: Vec<SpanishLotDetail> { acquisition_date, quantity, cost_eur, coefficient, actualized_cost_eur, proceeds_eur, gain_eur }`, `fiscal_gain_loss` (Σ lot gains), later mutated by wash-sale: `deferred_loss`, `integrable_amount = fiscal_gain_loss + deferred_loss`.

#### Wash-sale algorithm (S14–S16)

Identity: ISIN via `instrument_info`, symbol fallback. Per instrument, `WashSaleState { blocked: Vec<BlockedLot { buy_date, blocked_qty, deferred_loss, origin_sale_date } > }`.

Statement-wide chronological replay over **all** history (this is what makes multi-year statements and `simulate-sell` correct), then year-filter the outputs:

1. Seed states from `taxes.spain.deferred_losses` (synthetic pre-statement blocked lots).
2. Replay every sell in `conclusion_time` order. Its `FifoDetails` identify exactly which buy lots it consumed (match by lot conclusion date + quantity).
3. **Reintegration first**: a consumed lot matching a `BlockedLot` releases `deferred_loss × consumed_qty / blocked_qty`; decrement the blocked lot. Releases dated in the filing year become `WashSaleReintegrationEntry` items (negative GyP amounts).
4. **Deferral second**: if the sale's fiscal result (post-coefficient) is a loss, find same-instrument buys in `[sale − 2 months, sale + 2 months]` (`chrono::Months::new(2)`, calendar months): repurchase-**before** buys block only shares not already consumed by this or earlier sales; repurchase-**after** buys match when replay reaches them. Each bought share blocks at most one sold share (consume window buys FIFO). `deferred = loss × min(matched_qty, sold_qty) / sold_qty`, attached pro-rata to the matched buys as `BlockedLot`s.
5. Outputs: per-sale `deferred_loss`, filing-year reintegration entries, `carry_out: Vec<DeferredLossConfig>` (surviving blocked lots) printed as next-year config.

Caveat to document: a buy after the statement's end date cannot be seen — the statement must extend ≥ 2 months past year-end for final deferral status, or re-run when it does (mirrors the German year-boundary caveats).

#### Statement model & totals (S8/S11)

`SpanishTaxStatement`: `year`, `regime`; vectors `capital_gains`, `wash_sale_reintegrations`, `dividends`, `interest`, `fx_gains`, `fees` (with `deductible: bool`), `stock_grants`, `corporate_actions`, `short_positions` (manual review, as Germany); compensation state (`rcm_net`, `gyp_net`, prior/used/next ledgers per group, `cross_offset_applied`, `deferred_losses_next`); totals (`total_deductible_fees`, `savings_base`, `savings_quota`, `average_savings_rate`, `total_foreign_withholding`, `total_foreign_tax_credit`, `net_tax_due`, `fx_borrowed_review`); `modelo_boxes: Vec<(String, String, Decimal)>` (`// TODO(verify)` numbers until the year's form exists).

`calculate_totals()` order: RCM net = dividends + interest − deductible fees; GyP net = Σ `integrable_amount` + FX + reintegrations; `compensate_savings_base(...)`; `savings_quota = scale.tax(savings_base)`; `average_savings_rate = scale.average_rate(savings_base)`; **credit computed at year level** (per-entry rows carry the treaty-capped candidate informationally — mirror Germany's "per-row figures are informational" comment); `net_tax_due = max(0, quota − credit)`.

### Integration wiring (complete list)

| File | Change |
|---|---|
| `src/localities.rs` | `Jurisdiction::Spain` variant; exhaustive `traits()` arm: `{ name: "Spain", code: "ES", currency: "EUR", tax_precision: 2, rate_source: RateSourceKind::Ecb }` (ECB converter comes free via `CurrencyConverter::for_jurisdiction`); new `pub fn spain(config: &TaxConfig) -> Country` — analysis/metrics **approximation only** (copy the tone of `germany()`'s warning comment): real per-year savings scales as `ProgressiveTaxRate` (its statefulness is correct there), no compensation/coefficients/wash-sale |
| `src/taxes/mod.rs` | `TaxJurisdiction::Spain` (+ `#[serde(alias = "Spain")]`); `pub mod spain;`; `#[serde(default)] pub spain: Option<SpanishTaxConfig>` on `TaxConfig`; accessor `pub fn spanish(&self) -> GenericResult<&SpanishTaxConfig>` erroring with a config hint when `jurisdiction: spain` but the block is missing |
| `src/config.rs:184` | `Some(TaxJurisdiction::Spain) => localities::spain(&self.taxes)` |
| `src/tax_statement/mod.rs:36` | second early-return branch: `if country.jurisdiction == Jurisdiction::Spain { return generate_spanish_tax_statement(...); }` — new fn mirrors `generate_german_tax_statement` (read with same `ReadingStrictness` flags → `check_period_against_tax_year` → `CurrencyConverter::for_jurisdiction` → `spain::compute_tax_year` → CSV + console summary incl. the "next year's config" block: ledgers + deferred losses) |
| `src/tax_statement/trades.rs:200`, `src/tax_statement/interest.rs:92` | exhaustive matches on `Jurisdiction` — widen the Germany arm to `Jurisdiction::Germany \| Jurisdiction::Spain` (the Spain filer path early-returns before this code; no broker has Spain jurisdiction) |
| `src/analysis/sell_simulation.rs:56` | `SpanishTaxSimulation` parallel to `GermanTaxSimulation` (~150 lines): snapshot `spain::compute_tax_year`, `observe()` after each emulated sell (re-run `statement.process_trades(None)` first — same reason as the German comment at `:88`), `marginal(i)` = year-delta, `standalone(i)` = `scale.tax(integrable_amount)`. Inherits compensation, coefficients **and wash-sale deferral** automatically — a simulated loss sale near a recent repurchase correctly prices at zero deductible loss |
| `src/config.rs:455`, `src/taxes/mod.rs:218`, `src/cash_flow/mod.rs:45` | **No change** — all three key off the *broker's* jurisdiction (Russia/USA), not the filer's. Recorded here so a later audit does not re-flag them. |

### Config schema — nested `taxes.spain:` block

First nested per-jurisdiction block (the flat `german_*` fields are the older pattern; nesting is the pattern for new jurisdictions — note this in `docs/spain-taxes.md`).

```rust
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct SpanishTaxConfig {
    pub regime: SpanishTaxRegime,                                  // required, no default
    #[serde(default)] pub loss_carryforward: SpanishLossCarryforward, // { rcm, gyp: BTreeMap<i32 origin year, Decimal> }
    #[serde(default)] pub deferred_losses: Vec<DeferredLossConfig>,   // { symbol, isin?, loss, blocked_quantity, sale_date }
    #[serde(default)] pub coefficients: BTreeMap<i32, BTreeMap<i32, Decimal>>, // disposal year -> acq year -> coef
}
```

`config-example.yaml` block (add in S19):

```yaml
taxes:
  jurisdiction: spain
  spain:
    regime: gipuzkoa            # or: comun
    loss_carryforward:          # saldos negativos pendientes, keyed by origin year (4-year expiry)
      gyp: {2024: 1200.50}
      rcm: {2025: 80.00}
    deferred_losses:            # wash-sale losses carried in from prior years/tools
      - {symbol: VUSA, isin: IE00B3XXRP09, loss: 420.00, blocked_quantity: 15, sale_date: 2025-12-10}
    coefficients:               # optional: override/extend the shipped Decreto Foral tables
      2027: {2020: 1.11}
```

### Ordered shared-code refactors (only these two)

- **S5**: extract `format_eur`/`round_eur` from `tax_statement/germany/mod.rs` into `src/tax_statement/eur.rs` (`pub(crate)`); `germany/mod.rs` re-exports so zero call sites change. The one-formatter invariant (commit `363d6dc7`) becomes cross-jurisdiction. Spanish forms also use 2-dp EUR half-away-from-zero (`// TODO(verify)`).
- **S10**: move `tax_statement/germany/fx_fifo.rs` → `src/tax_statement/fx_fifo.rs` (`pub(crate)`); the signed-inventory engine is generic — only the §20/§23 routing is German and stays in `germany/processor.rs`; neutralize doc comments to held-balance/borrowed-balance vocabulary.

Explicitly **skipped** (do not do): moving German `loss_carryforward.rs`; any `TaxRegime` trait; changes to `ProgressiveTaxRate`; a shared test-utils module (duplicate `FixedEurBackend` instead).

## Tasks

### Phase 0 — Law verification & harness

- **S1 — Verify the law from official sources.** Status: ✅ Done
  Outcome: Gipuzkoa 2026 art. 76.1 scale and the 2024/25 scale **confirmed exactly** as hypothesized; all three Decreto Foral coefficient tables verified **OFFICIAL** against the Boletín Oficial de Gipuzkoa and independently reproduced identically by a second research pass; the whole state-side chapter verified OFFICIAL against the BOE consolidated text. Four plan corrections landed above: (1) **custody fees are NOT deductible in Gipuzkoa** (no analogue of state art. 26.1.a; art. 39 is a closed list) — this makes deductibility regime-dependent and changes S9; (2) the **state 2024 top bracket is 28%**, not 30%; (3) coefficients live at **art. 45.2**, not 46/47, and Gipuzkoa's wash-sale rule is **art. 43.g/h**, not 33.5 (that is the state cite); (4) the 25% cross-offset has applied since **2018**, not 2021. Six `TODO(verify)` items remain, listed under the source manifest — the only ones that touch computed numbers are the treaty-rate cap and the `fecha de transmisión` choice; the rest are citation/reporting-layer gaps (Modelo 109 casillas).
  Fetch and record (source URL + retrieval date, in `docs/spain-taxes.md` sources section and Appendix A here): official NF 1/2025 text for art. 76 (confirm the 2026 scale above and that actualization coefficients survive the reform), DF 27/2025 coefficient table for 2026 + DF 61/2024 for 2025 (+ the 2024 DF), the state 2026 savings scale, the foral custody-fee deductibility article, the latest published Modelo 109 box layout, DGT/foral consulta references for wash-sale proportionality. Update the tables above and Appendix A in-place if anything differs; leave `TODO(verify)` markers for anything unobtainable.
  Commit: `docs(spain-tax): record verified savings scales and coefficients with sources`
- **S2 — Fixture harness.** Status: ✅ Done (`ecb95c10`)
  Deviation from the task text, and the reason: `tests.rs` ships `FixedEurBackend`, `read_fixture` and a harness smoke test only. The `run_pipeline*` helpers and the Appendix-A value assertions land in S8 instead, because an `#[ignore]`d test still has to **compile**, and referencing `spain::compute_tax_year` before it exists would break the `cargo check` gate for every task from S2 through S7. The fixture itself is committed in full.
  `src/tax_statement/spain/testdata/fifo/statement.xml` (synthetic Flex XML: 2 buys 2021/2024, 2 sells 2026, round numbers, fixed 0.9 EUR/USD) + `tests.rs` with `FixedEurBackend`, `read_fixture`, `run_pipeline*`; assertions per Appendix A, `#[ignore]`d until S8.
  Commit: `test(spain-tax): add Flex-fixture harness for the Spanish pipeline`

### Phase 1 — Wiring skeleton

- **S3 — Jurisdiction.** Status: ✅ Done (`cec97e2f`)
  Scope note: `taxes/spain/scale.rs` is introduced here (bracket tables + `for_year`) rather than waiting for S6, because `localities::spain` needs the real scales and duplicating them would create two sources of truth. S6 adds `tax()` / `average_rate()` and the cuota-íntegra golden vectors on top. `localities::spain` keys its earliest scale at `i32::MIN` so `Country::tax_rate` cannot panic on a pre-2024 query — it takes `range(..=year).last().unwrap()`. Verified independently: driving the shipped brackets through `ProgressiveTaxRate` reproduces the hand-computed cumulative figures (Gipuzkoa 2026 €10k → 1,925; Común 2026 €10k → 1,980; Gipuzkoa 2025 €10k → 2,075; Común 2024/2025 €400k → 99,880 / 101,880). — localities + TaxJurisdiction + `get_tax_country` + widened match arms (`trades.rs:200`, `interest.rs:92`). Test: `jurisdiction: spain` yields `Country { code: "ES", currency: "EUR" }` with ECB rate source; unknown-year `SavingsScale` error message named.
  Commit: `feat(spain-tax): add Spain jurisdiction with ECB rates and analysis approximation`
- **S4 — Config block.** Status: ✅ Done (`f50f5d92`)
  `LossLedger` is complete here (`from_config` + `add` / `apply` / `total` / `expiring_after` / `balances`), not split with S11: `mod taxes` is private in `lib.rs`, so a `pub` item with no reachable consumer is a `dead_code` error under `-D warnings`. The validation entry point is `SpanishTaxConfig::loss_ledgers(filing_year)`, which is reachable via `Config::taxes` and is also where the check belongs — the 4-year window is relative to the year being filed, so the same config is valid for one return and expired for the next, and it cannot be a `serde` validation. — `SpanishTaxConfig` + accessors + `LossLedger::from_config` validation (origin ≥ filing year rejected; origin < filing−4 rejected with "expired" message). serde tests mirror `taxes/mod.rs:265-390` patterns.
  Commit: `feat(spain-tax): add taxes.spain config block`
- **S5 — Shared EUR formatter.** Status: ✅ Done (`c72d5947`)
  `src/tax_statement/eur.rs` holds `format_eur` / `round_eur`; `germany/mod.rs` re-exports both, so zero call sites changed and all 91 German tests stayed green untouched. Net diff: +4 / −22 lines outside the new file. Doc comments neutralized to cover both jurisdictions, with the `TODO(verify)` for the Spanish rounding convention attached to `format_eur`. — extraction as specified; German tests stay green untouched.
  Commit: `refactor(tax-statement): move the EUR formatter to a shared module`

### Phase 2 — Gipuzkoa 2026 core

- **S6 — SavingsScale.** Status: ✅ Done (`f5c80448`)
  `tax()` reproduces the law's own cuota-íntegra column at all nine 2026 Gipuzkoa thresholds, plus the pre-reform foral and both state scales. Full precision, no per-slice rounding (unit-tested: `tax(7500.555) = 1425.111`). `average_rate` carries a `TODO(verify)`: both statutes say the rate is expressed to two decimals but neither says whether the credit cap is computed from the rounded rate or the exact quotient; the exact quotient is used. Reachability note: `SpanishTaxConfig::savings_scale(year)` is the config accessor that keeps these methods out of `dead_code`. — all shipped tables + `average_rate`; rstest vectors from Appendix A incl. the cuota-íntegra golden vectors and bracket-edge cases.
  Commit: `feat(spain-tax): add savings-base scales for Gipuzkoa and Territorio Común`
- **S7 — Coefficients.** Status: ✅ Done (`4965bfdd`)
  All three Decreto Foral tables shipped (96 values). Beyond spot checks, the tests assert structural invariants that would catch a transcription slip: contiguous acquisition years 1994→disposal year, every value ≥ 1, last row exactly 1.000, and — across tables — a later disposal actualizing at least as much for the same acquisition year. **Signature deviation**: takes the acquisition `Date`, not the year, because the statutory seam (31-12-1994 → the 1995 coefficient) cannot be expressed year-only; the call site has the date anyway. Exposed to the processor via `SpanishTaxConfig::actualization_coefficient`, which returns 1 unconditionally under Común. — shipped DF tables (from S1), override map, clamping, unknown-year error.
  Commit: `feat(spain-tax): add Gipuzkoa actualization coefficients`
- **S8 — Statement + trades + routing.** Status: ✅ Done (`28bff2d3`)
  End-to-end from Flex XML through FIFO to the cuota. The `fifo` fixture is asserted under **both** regimes rather than adding a separate `coefficients` fixture — the fixture is regime-independent by construction, so a second copy of the same trades would be duplicated data. Gipuzkoa GyP €19,692 vs Común €22,500, differing by exactly €2,808. `produces_capital_gain` carries the `fecha de transmisión` `TODO(verify)`. — `SpanishTaxStatement`, `compute_tax_year`, `process_trades` with per-lot coefficients, `generate_spanish_tax_statement` routing. Un-ignore `fifo`; add `coefficients` fixture (same trades run under both regimes differ exactly by the coefficient delta).
  Commit: `feat(spain-tax): compute capital gains with per-lot FIFO and coefficients`
- **S9 — RCM income.** Status: ✅ Done (`c1a8494b`)
  `income` fixture asserted under both regimes: RCM net €990 (Gipuzkoa, nothing deductible) vs €945 (Común, custody fee deducted). Fee classification is a description keyword match with a `TODO(verify)` — art. 26.1.a names the *service*, not the wording a broker uses, so an unrecognised fee is reported but not deducted (overstates tax rather than understating it). — dividends, interest, **regime-dependent** custody-fee deduction (S1: deductible under Común art. 26.1.a, **not** deductible under Gipuzkoa art. 39), per-entry treaty-capped credit candidates. `income` fixture (US dividend 30% WHT, broker interest, custody fee) asserted under **both** regimes so the deductibility split is covered.
  Commit: `feat(spain-tax): process dividends, interest and deductible custody fees`
- **S10 — FX.** Status: ✅ Done (`2d4a2928` refactor, `f624983d` feature)
  The move was pure: 589 lib tests before and after, zero German test changes. Doc comments neutralized to held-balance/borrowed-balance vocabulary. Held-balance results join the ganancias group; borrowed-balance results go to `fx_borrowed_review` with a warning rather than being taxed or dropped. The `fx_gain` fixture needed a **date-aware** test converter (`revaluing_converter`) — the flat-rate backend can never realize an FX result. Note the engine emits a zero-amount realization for a flat rate rather than no entry; the test asserts that rather than absence. — two commits: `refactor(tax-statement): share the foreign-currency FIFO engine` then `feat(spain-tax): tax FX conversion gains in the savings base` (`fx_gain` fixture; borrowed-balance → `fx_borrowed_review`).
- **S11 — Compensation + credit + totals.** Status: ✅ Done (`0d3f71c7`)
  `compensate_savings_base` implements **both** regimes' paths, so S18's 25% cross-offset engine is already written and unit-tested in both directions here; only its end-to-end fixture is outstanding. Carries a `TODO(verify)` on the order of operations: art. 49.1 caps the cross-offset at "el 25 por ciento de dicho saldo positivo" without settling whether that positive is measured before or after prior-year balances are absorbed — measuring it after is implemented, the conservative reading. Expired vintages are dropped with a warning rather than carried. `LossLedger::drop_expired` added. The credit is a year-level figure with a `TODO(verify)` on whether "renta obtenida en el extranjero" is gross or net of attributable expenses. — `LossLedger`, `compensate_savings_base` (Gipuzkoa path: no cross-offset), `calculate_totals`, year-level credit, `net_tax_due`. `loss_carryforward` fixture (per-group origin-year ledgers, oldest-first, expiry boundary: origin = year−4 usable, older rejected); completes `income` assertions.
  Commit: `feat(spain-tax): compensate savings-base groups with 4-year loss ledgers`
- **S12 — Output.** Status: ✅ Done (`27c52b15`)
  19-column transaction shape with each capital gain's `Lot` rows **inline** beneath it rather than in a separate section, so the actualization multiplication reads line by line under the sale it justifies. Two deviations. (1) **No `modelo_boxes` field on the statement**: the box map is a reporting-layer concern with no computed content of its own, so it lives in the formatter and the statement stays a record of computed facts. (2) **Dates are ISO 8601**, not the `DD.MM.YYYY` that `formatting::format_date` (and therefore the German CSV) emits — the carry-out block has to round-trip back into `taxes.spain.deferred_losses`, whose deserializer prefers ISO. Every Modelo box is preceded by a `# WARNING` banner: the Gipuzkoa casillas come from an AÑO-2019 map and the double-taxation casilla is emitted as `CASILLA_UNKNOWN` because no year's number is published anywhere reachable; Modelo 100 rows carry labels and no numbers at all. Tests pin the 19-column shape under a quote-aware parser, that no label carries its own comma inside the 3-column summary block, and that the coefficient cell is empty on the sale row (a sale can consume lots of several vintages). — CSV formatter + contract doc `specs/002-spain-tax/contracts/csv-output.md` + console summary + Modelo 109 mapping with `# WARNING` caveats + next-year config block printout.
  Commit: `feat(spain-tax): emit the Spanish CSV statement and Modelo 109 summary`
- **S13 — Sell simulation.** Status: ✅ Done (`34c6fa86`)
  Deviation: rather than adding a second `Option<&SpanishTaxSimulation>` parameter alongside the German one, `simulate_sell` now holds a single `Option<TaxSimulation>` enum (`German(Box<…>) | Spanish(Box<…>)`) exposing `observe`/`marginal`/`standalone`/`total`/`print`. That keeps `print_results` at one simulation parameter and the three `if let Some(german)` sites unduplicated; both variants are boxed because each carries whole tax statements (clippy `large_enum_variant`). `standalone(i)` is `scale.tax(integrable_amount)` — the **integrable** amount, so a loss the valores-homogéneos rule deferred is compared at the figure the return would actually allow. Tested at statement level, mirroring the German `hypothetical_disposal_is_priced_at_what_it_adds_to_the_year`: three identical €7,500 disposals cost €1,425 / €1,500 / €1,650 as the base climbs the Gipuzkoa 2026 brackets, a loss booked after a gain prices at −€785, and a fully deferred loss changes the year by nothing. — `SpanishTaxSimulation` in `simulate-sell`.
  Commit: `feat(spain-tax): price simulated sales at their marginal Spanish tax`

### Phase 3 — Wash sale (hardest correctness surface; own phase)

- **S14 — Primitives.** Status: ✅ Done (`fbb6f924`)
  Deviation: the primitives landed with a consumer rather than alone. A `pub fn` in the private `wash_sale` module with no reachable caller is a `dead_code` error under `-D warnings` (the same constraint S4 and S6 hit), so S14 also narrows the existing `wash_sale_unchecked` flag from "every loss-making instrument" to "only losses with a homogeneous acquisition actually inside the ±2-month window" — which is exactly what the three helpers are for, and a strict improvement (the same safety, without burying the losses that matter). The `wash_sale_after` fixture moved here from S15 to give that narrowing a positive case. `window()` uses `chrono::Months`, so the month-end clamping is the statute's calendar-month semantics rather than 60 days; rstest pins the seams (31 Dec → 28 Feb, 30 Apr → 29/28 Feb by leap year, 29 Feb itself). Both endpoints are treated as inside the window with a `TODO(verify)`: neither text settles it, and inclusive defers more, which understates the deduction rather than overstating it. — window arithmetic (`chrono::Months`), instrument identity, matching helpers; rstest month-end/leap edges (Dec-31 sale, Jan/Feb windows).
  Commit: `feat(spain-tax): add valores-homogéneos window matching`
- **S15 — Deferral.** Status: ✅ Done (`a059c937`)
  `process_trades` now prices **every** disposal in the statement, not just the filing year's, and replays them in date order through `WashSaleEngine`; only the filing year's entries are emitted. Design points worth recording: (1) a disposal in a year with no shipped actualization table cannot be priced, so it is replayed for its lot consumption but can never create a deferral, and the year is reported in `wash_sale_unpriced_years` rather than erroring the whole run — a 2026 filing must not fail because the statement also contains a 2022 sale; (2) same-date acquisitions share one bucket, because a FIFO lot identifies itself by its conclusion date and same-day buys are always both inside or both outside any window; (3) availability is uniformly `quantity − consumed − blocking`, which makes the "a repurchase before the sale blocks only shares the sale did not itself consume" rule fall out rather than needing a special case; (4) corporate-action buys are excluded from acquisitions (a split re-expresses shares, it does not acquire them) and corporate-action sells are not replayed at all — a blocked lot carried across a split is a documented gap. Knowingly incomplete in one direction at this commit, with a `warn!`: released deferrals are computed but not yet integrated into the base, which **overstates** tax (the safe direction). S16 closes it. — replay + deferral in the pipeline. Fixtures `wash_sale_after`, `wash_sale_before` (repurchase 1 month before, shares still held).
  Commit: `feat(spain-tax): defer losses on repurchased homogeneous securities`
- **S16 — Reintegration.** Status: ✅ Done (`76b947c4`)
  **Config deviation**: `DeferredLossConfig` gains a required `acquisition_date`. Without it the carry-out cannot round-trip — reintegration matches a blocked lot to the FIFO lot that consumes it by acquisition date, so a config carrying only `sale_date` could never be re-imported and released. Release is pro rata to the share of the blocked lot consumed, oldest deferral first, and is marked with the plan's `TODO(verify)` on the blocked-lot-vs-remaining-estate reading; it also settles by assumption which shares of a partly-blocked acquisition date a sale consumes first (the blocked ones). Two further additions the task text did not name but the rule needs: `wash_sale_window_gaps` flags a loss whose +2-month window reaches past the statement's last date (deducted in full, so possibly overstated — the statement-window caveat the plan documents), and the console prints a paste-ready `deferred_losses:` block beside the existing `loss_carryforward:` one. Fixture `year_end_loss` covers the open window; `wash_sale_multi_lot` covers the pro-rata split and release. — release on disposal, `carry_out`, opening `deferred_losses`. Fixture `wash_sale_multi_lot` (partial repurchase → proportional deferral; pro-rata multi-lot release; surviving carry-out asserted).
  Commit: `feat(spain-tax): reintegrate deferred losses when repurchased lots are sold`

### Phase 4 — Territorio Común

- **S17 — Regime end-to-end.** Status: ✅ Done (`3f061239`)
  Tests only: the regime switch itself landed in S8/S9 and the fixtures already ran under both regimes, but nothing asserted the **state scale** end to end — every Común assertion sat in the first bracket, where the two regimes happen to agree at 19%. Three tests close that: `fifo` under Común pays €4,605 (1,140 + 16,500 × 21%) against Gipuzkoa's €3,957.24 on the same trades, `income` under Común carries the credit through the state scale to €44.55 net, and `loss` under Común carries €9,000 forward unactualized. Note the `coefficients` fixture the task text names never existed — S8 asserted the `fifo` fixture under both regimes instead, because it is regime-independent by construction. — `regime: comun` (coefficient 1, state scales); rerun `fifo`/`coefficients`/`income` fixtures under Común.
  Commit: `feat(spain-tax): support the Territorio Común regime`
- **S18 — Cross-offset.** Status: ✅ Done (`db4d832a`)
  Two fixtures rather than one, because the two directions cannot coexist in a single year: a group is either positive or negative. `cross_offset` drives ganancias → RCM (a €9,000 loss against a €7,200 dividend: Común crosses €1,800, Gipuzkoa crosses nothing and taxes €7,200); `cross_offset_rcm` drives RCM → ganancias (a €9,000 gain against a €3,600 custody fee: Común crosses €2,250, while under Gipuzkoa the fee is not deductible at all so RCM is zero and there is nothing to cross). Both acquisitions sit outside their sale's window, so no deferral interferes and the cross-offset is the only thing separating the runs. The engine itself was already written and unit-tested in S11. — 25% rule in `compensate_savings_base`; `cross_offset` fixture asserting both directions and the Gipuzkoa run showing no crossing.
  Commit: `feat(spain-tax): apply the 25% cross-group offset for Territorio Común`

### Phase 5 — Docs & polish

- **S19 — Docs.** Status: ✅ Done (`1c967306`) — `docs/spain-taxes.md` (mirror `germany-taxes.md` structure: overview, config, usage, scales with sources, FIFO + coefficients worked example, wash sale + carry-out workflow + statement-window caveat, loss compensation + Común cross-offset, foreign tax credit US example, custody fees, FX, grants, Modelo 109/100 mapping + warning, informational notes: Modelo 720, no traspaso at foreign brokers, no Vorabpauschale equivalent; out of scope: wealth tax, partial-year residency, Beckham regime, unlisted 1-year window) + README + `config-example.yaml`.
  Commit: `docs(spain-tax): add the Spanish tax statement guide`
- **S20 — Final gate.** Status: ✅ Done — see the closing report at the end of this file.
  Commit: `chore(spain-tax): final gate and verification report`

## Appendix A — Worked numbers (fixture/unit-test expectations)

Scales (cuota íntegra, EUR, full precision). **All vectors below re-derived from the S1-verified bracket tables; every one checks out.**

- Gipuzkoa 2026: see golden vectors above (they reproduce the law's own cuota-íntegra column exactly); also `tax(0)=0`, `tax(-100)=0`.
- Gipuzkoa 2025 **and** 2024 (identical scale): `tax(2500)=500 · tax(10000)=2075 · tax(15000)=3175 · tax(30000)=6625 · tax(50000)=11625` (11,625 = 6,625 + 20,000×0.25).
- State 2026 **and** 2025 (identical scale): `tax(6000)=1140 · tax(50000)=10380 · tax(200000)=44880 · tax(300000)=71880 · tax(400000)=101880`.
- State **2024** (differs only in the top bracket, 28% not 30%): `tax(6000)=1140 · tax(50000)=10380 · tax(200000)=44880 · tax(300000)=71880 · tax(400000)=`**`99880`**. Ship this as a separate table and unit-test the 2024-vs-2025 divergence above 300,000 — it is the one place where "the state scale" is year-sensitive in the shipped range.

Credit example (`income` fixture): US dividend gross €1,000, withheld €300 (30%); treaty cap = 1,000×0.15 = €150; with savings base €10,000 under Gipuzkoa 2026, avg rate = 1,925/10,000 = 0.1925 → cap₂ = €192.50 → **credit €150**; the €150 excess over treaty is not creditable (reclaim via IRS, doc note).

Wash-sale example (`wash_sale_after` / `wash_sale_multi_lot`): sell 100 sh on 2026-03-10 at fiscal loss €500; buy 40 sh on 2026-04-20 (inside +2 months) → deferred = 500×40/100 = **€200**, integrable loss **−€300**; later sell 25 of the 40 blocked shares → release 200×25/40 = **€125** as reintegration; carry-out: 15 blocked shares, €75.

Cross-offset example (`cross_offset`): RCM net −2,000, GyP net +6,000. Común: cross-offset = min(2,000, 25%×6,000=1,500) → GyP taxable 4,500, RCM −500 carries with filing-year label. Gipuzkoa: GyP taxable 6,000, RCM −2,000 carries.

Ledger expiry (filing 2026): origins 2022–2025 usable (2022 offsets in 2023–2026); origin 2021 → config error "expired".

Coefficients (S1-verified, DF 27/2025 disp. adic. 1ª, for disposals in **2026**) — the rows the shipped table needs, full list in `coefficients.rs`:

| Acq. year | 1994 y ant. | 1995 | 2000 | 2010 | 2020 | 2021 | 2022 | 2023 | 2024 | 2025 | 2026 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Coefficient | 2.030 | 2.156 | 1.866 | 1.402 | 1.249 | **1.212** | 1.121 | 1.082 | **1.050** | 1.020 | 1.000 |

Note the seam: `1995 (2.156) > 1994-y-anteriores (2.030)`, so the table is **not** monotonic and a naive "older ⇒ bigger coefficient" assertion would fail. Plus the statutory special case: an asset acquired **exactly on 31-12-1994** takes the **1995** coefficient. Disposal years 2025 (DF 61/2024) and 2024 (DF 58/2023) ship their own full tables.

**`fifo` / `coefficients` fixture — pinned expectations** (fixture: buy 100 AAPL @ $100 on 2021-03-10, buy 100 @ $200 on 2024-06-10, sell 100 @ $250 on 2026-04-15, sell 100 @ $300 on 2026-09-15, all commission-free, converter fixed at 0.9 EUR/USD):

| | Sale 1 (2026-04-15) | Sale 2 (2026-09-15) |
|---|---|---|
| Proceeds EUR | 22,500 | 27,000 |
| FIFO lot consumed | 2021 lot, cost €9,000 | 2024 lot, cost €18,000 |
| Coefficient (Gipuzkoa 2026) | 1.212 | 1.050 |
| Actualized cost | 10,908 | 18,900 |
| **Gipuzkoa gain** | **11,592** | **8,100** |
| **Común gain** (coefficient 1) | **13,500** | **9,000** |

Totals: Gipuzkoa GyP **€19,692**, Común GyP **€22,500**. The delta is exactly the coefficient effect: `9,000×0.212 + 18,000×0.050 = 1,908 + 900 = 2,808`, and `22,500 − 19,692 = 2,808` ✓. This is the "same trades under both regimes differ exactly by the coefficient delta" assertion S8/S17 need.

Savings quota on the Gipuzkoa figure (no other income, 2026 scale): base 19,692 → `1,425 + 1,500 + 4,692×0.22 = 1,425 + 1,500 + 1,032.24 =` **`3,957.24`**; `average_rate = 3,957.24 / 19,692 ≈ 0.200957…` (full precision, no rounding inside the scale).

## Appendix B — Existing APIs to reuse (verified signatures)

- `StockSell::calculate(&Country, &Instrument, &[], &CurrencyConverter) -> SellDetails` and `SellDetails.fifo: Vec<FifoDetails>` — `src/broker_statement/trades.rs:342-419`; per-lot `conclusion_time`/`execution_date`, `quantity`, `multiplier`, `total_cost(currency, converter)`, `StockSourceDetails::{Trade, CorporateAction, Grant}`.
- `CurrencyConverter::for_jurisdiction(jurisdiction, database, quotes, strict_mode)` — `src/currency/converter.rs:57` (sole construction point; ECB backend, namespaced sqlite rate cache).
- `Jurisdiction::traits()` / `RateSourceKind` — `src/localities.rs:69-125`.
- German templates: `tax_statement/germany/processor.rs` (per-lot allocation `:256-340`, `creditable_foreign_tax :566`), `statement.rs` (`calculate_totals :476`), `tests.rs:22-121` (`FixedEurBackend`, `run_pipeline_full`), `csv_formatter.rs` (summary-block shape), `analysis/sell_simulation.rs:56-248` (`GermanTaxSimulation`).
- `taxes::rates::ProgressiveTaxRate` — `src/taxes/rates.rs:42` (analysis approximation only; stateful by design).
- Fee/dividend/interest sources: `BrokerStatement { dividends, idle_cash_interest, fees, foreign_cash_flows, ... }` — `src/broker_statement/mod.rs:64`.


## Closing report (S20)

### Gate

| Check | Result |
|---|---|
| `cargo check --lib --all-targets` | clean |
| `cargo test spain --lib` | **169 passed / 0 failed** |
| `cargo test --lib` | **635 passed / 33 failed** — all 33 are the pre-existing `broker_statement::*::parse_real` failures from the private, empty `testdata/` submodule. Verified: filtering the failure list for anything that is not a `parse_real` case yields zero rows, and the count is unchanged from the base commit |
| `cargo test --no-fail-fast` (all targets) | lib as above; `tests/generate.rs` 1 passed; binary target has no tests |
| `./check` (clippy, dev + release, `-Dwarnings`) | the same **4 pre-existing errors** in untouched files: `portfolio/rebalancing.rs`, `quotes/cbr/mod.rs`, `analysis/performance/statistics.rs`, `formats/xls/table.rs`. No new lint was introduced at any commit |

`cargo fmt --check` reports diffs across the repo including files this feature never touched
(`build.rs` among them); the project's gate is `./check`, which runs clippy only, so rustfmt is not
enforced here and was not run.

### Every `TODO(verify)` in the Spanish feature

Ordered by how much of a computed number rides on it.

**Affects a figure on the return**

1. `tax_statement/spain/wash_sale.rs:228` — **reintegration keying.** Both statutes release a
   deferred loss "a medida que se transmitan los activos que **permanezcan en el patrimonio** de la
   persona contribuyente" (NF 3/2014 art. 43 closing ¶ / LIRPF art. 33.5 closing ¶) — the securities
   *remaining in the estate*, which is broader than the repurchased lot the implementation attaches
   the deferral to. Under FIFO with a single instrument the two readings coincide in the common case;
   they diverge when untouched pre-existing lots are held alongside the repurchased ones. The same
   marker settles, by assumption, which shares of a partly-blocked acquisition date a sale consumes
   first: the blocked ones.
2. `tax_statement/spain/processor.rs:35` — **treaty rate.** Hard-coded at 15%, the dividend rate in
   the Spain-US and Spain-Germany treaties and most of Spain's network. Neither NF 3/2014 art. 91 nor
   LIRPF art. 80 mentions a treaty cap at all — the limit comes from the treaty itself, so a filer on
   a different treaty gets the wrong credit.
3. `tax_statement/spain/processor.rs:74` — **fecha de transmisión.** Whether a listed security's
   transfer date is the trade date or the settlement date was not resolvable. The conclusion date is
   used. Moves income between tax years for a trade concluded in late December.
4. `taxes/spain/compensation.rs:38` — **order of operations in the cross-offset.** Art. 49.1 caps it
   at "el 25 por ciento de dicho saldo positivo" without settling whether that positive is measured
   before or after prior-year balances are absorbed. Measured after, the conservative reading.
   Común only.
5. `tax_statement/spain/wash_sale.rs:33` — **window endpoints.** Treated as inclusive. Neither text
   says whether "dos meses anteriores" includes the day exactly two months back. Inclusive defers
   more, which understates the deduction rather than overstating it.
6. `tax_statement/spain/wash_sale.rs:54` — **homogeneity = same ISIN.** Art. 43 defers to the
   RD 1704/1999 / RIRPF definition, which reaches different issues of the same issuer with the same
   rights. Same-ISIN is the only test a Flex statement supports.
7. `tax_statement/spain/processor.rs:682` — **custody-fee keyword match.** Art. 26.1.a names the
   *service*, not the wording a broker uses. An unrecognised fee is reported but not deducted, which
   overstates tax rather than understating it. Común only — nothing is deductible under Gipuzkoa.
8. `tax_statement/spain/statement.rs:401` — **the credit's income base.** Art. 91.b applies the
   average rate to "la renta obtenida en el extranjero" and art. 80.1.b to "la parte de base
   liquidable gravada en el extranjero"; whether that is gross or net of attributable expenses is
   unsettled. Gross dividends are used.
9. `taxes/spain/scale.rs:135` — **average-rate rounding.** Both statutes say the rate is expressed to
   two decimals, but neither says whether the credit cap is computed from the rounded rate or the
   exact quotient. The exact quotient is used.
10. `tax_statement/eur.rs:16` — **rounding convention.** Two decimals, half away from zero, assumed
    for Modelo 109 / 100 per-line amounts. Neither the foral nor the state instructions state the
    mode explicitly. Sub-cent effect only.

**Reporting layer only**

11. `tax_statement/spain/csv_formatter.rs:22` — every Modelo 109 casilla. See below.

### The Modelo 109 casilla situation

This is the one place a human must not trust the tool's output as printed.

- Gipuzkoa **publishes no static numbered form**. The return is generated by the Zergabidea platform,
  so there is no PDF whose boxes can be cited for a filing year.
- The only official box map reachable during S1 is explicitly dated **AÑO 2019**
  (`egoitza.gipuzkoa.eus/documents/39465/25500467/5.FE-Errenta_maila_PFEZ-es.pdf`): RCM 06+16,
  RCM deductible expenses 17, RCM retenciones 07+22, ganancias patrimoniales 28, base imponible
  general 19, base imponible del ahorro 33, cuota líquida 64. The tool emits six of these.
- Two 2026 changes plausibly renumbered boxes: art. 76.1 gained its own cuota-íntegra column under
  NF 1/2025, and Anexo 3 gained a crypto section.
- The casilla for the **deducción por doble imposición internacional is not published anywhere
  reachable, for any year**. It is emitted as `MODELO_109_CASILLA_UNKNOWN` with the computed amount
  and no number.
- **Modelo 100** (Territorio Común) was never mapped: its rows carry labels and the marker
  `casilla unverified`.

Every one of these is preceded by a `# WARNING` banner in the CSV. The amounts are computed; the box
numbers are not evidence.

### What a human should check before filing

1. **Verify each casilla against your filing year's own form.** The numbers above are indicative.
2. **Confirm your treaty's dividend rate** if the source state is not the US or Germany, and change
   the hard-coded 15% if it differs.
3. **Extend the statement at least two months past year end** before filing, or accept the
   `WASH_SALE_WINDOW_OPEN` warning: a December loss whose repurchase window runs into February is
   deducted in full and may be overstated. Re-run when the statement covers the window.
4. **Copy both carry-forward blocks into next year's config** — `loss_carryforward` (per group, per
   origin year) and `deferred_losses` (per blocked lot). The console prints them paste-ready. A
   balance dropped for expiry is reported separately; it is gone, not carried.
5. **Check any instrument you hold in more than one homogeneous issue**, and any deferral that
   spanned a stock split — neither is modelled.
6. **Review the borrowed-balance FX figures** if any appear: they are excluded from the base pending
   a determination neither statute makes.
7. **Modelo 720** is a separate informational obligation this tool never computes.
8. **Joint holdings run a separate FIFO queue** under the Diputación Foral's own guidance. The tool
   models a single queue per symbol; a jointly-held position needs its own run.

### Known gaps, deliberately not implemented

- Art. 43.h / 33.5.g **unlisted securities** — one-year window, not two months.
- Art. 43.i **fungible crypto**.
- **Corporate-action sells are not replayed**, so a blocked lot is not carried across a stock split.
- The **general base** (employment income, including RSU vesting) is out of scope, as are wealth tax,
  partial-year residency and the Beckham regime.


## Feature status

All twenty tasks are complete. Branch `002-spain-tax`, 27 commits on top of `1a5c0e7b`.

**Correctness gaps that remain loud rather than silent** — each emits a `warn!`, a console warning
and a `# WARNING` line in the CSV, because each would otherwise produce a plausible wrong number:

- **Borrowed-balance FX** (`fx_borrowed_review`). Excluded from the savings base, because repaying a
  currency loan is not clearly a transfer of a patrimonial element and neither statute settles it.
- **Open valores-homogéneos window** (`wash_sale_window_gaps`). A loss whose +2-month repurchase
  window reaches past the statement's last date is deducted in full, which may **overstate** it.
- **Unpriced disposal years** (`wash_sale_unpriced_years`). Sales in a year with no shipped
  actualization table are replayed for lot consumption but never tested for deferral.

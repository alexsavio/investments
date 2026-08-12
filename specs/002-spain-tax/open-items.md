# Spanish Tax Feature — Open-Items Resolution Round

**Date**: 2026-08-11
**Branch**: `002-spain-tax` (continue; after review2 close-out)
**Inputs**: three web-research passes over the four remaining `TODO(verify)` items. Outcome: two items **settled by binding/high-authority sources**, one **substantially resolved** (official casilla layouts found for 2023–2025 including the previously unknown DDI box), one **classified per-type with two residual warn-cases**. This plan implements the findings, adds **runtime warnings wherever a computation actually turns on a residual open interpretation**, and adds a durable "Open interpretations" section to the docs. Work like the prior plans: phases in order, failing test first, per-task gate (`cargo check` → `cargo test spain` → `./check`), one Conventional Commit per task, statuses updated in-place. Pre-existing and untouchable: ~33 `parse_real` failures, 4 clippy errors in untouched files.

## Research verdicts (sources retrieved 2026-08-11)

### V1 — US-listed shares take the 2-MONTH wash-sale limb: CONFIRMED by binding DGT doctrine
**DGT CV V0778-25 (05-05-2025)** answers the exact question for NYSE/NASDAQ/CME; **V0951-25 (30-05-2025)** generalizes to "los mercados de valores de Estados Unidos". Holding: third-country markets covered by an **in-force Commission equivalence decision** under MiFID II art. 25(4)(a) fall inside art. 33.5.f (and 37.1.a valuation) "mientras dicha decisión de equivalencia no haya sido objeto de derogación". US venues are equivalent per **Commission Implementing Decision (EU) 2017/2320** (Annex names NYSE, all Nasdaq exchanges, NYSE Arca/MKT/National, Cboe, IEX…); Australia per 2017/2318; Hong Kong per 2017/2319. **Switzerland's decisions lapsed 30-06-2019 and were never renewed** → SIX-only listings fall to the 1-year limb (art. 33.5.g / NF art. 43.h); same for Canada/Japan/other venues with no decision. Following published DGT criteria also shields from penalties (art. 179.2.d LGT). Gipuzkoa: NF 3/2014 art. 43.g is a verbatim clone interpreting the same EU concept; no foral pronouncement — DGT criteria persuasive, not formally binding, on the Hacienda Foral (say so in docs). Bonus doctrine **V1872-25 (14-10-2025)**: dual-listed same-class lines in different currencies ARE homogeneous (supports the ISIN key); ADR↔ordinary homogeneity expressly left open.
Full consulta texts saved at `/private/tmp/claude-501/-Users-alexandre-projects-alexsavio-investments/864d554e-7895-4b99-9187-48e9e68b5156/scratchpad/doc_71913.html` (V0778-25), `doc_72086.html` (V0951-25), `doc_73007.html` (V1872-25). petete needs `curl -k`.

### V2 — Window endpoints INCLUSIVE: CONFIRMED by TS doctrine
CC art. 5.1 "de fecha a fecha" (supletory via LGT art. 7.2); **STS 552/2022 (10-05-2022, RC 1874/2021)** and precedents (STS 02-07-2020 RC 3780/2019; 02-04-2008 rec. 323/2004: publication 13-02 → one-month plazo expires 13-03, filing 15-03 late): the same-ordinal terminal day is the last day OF the period; **STS 287/2009 (Sala 1ª)** line applies de-fecha-a-fecha with natural days to substantive periods. Month-end fallback: period ends on the last day of the month when no equivalent ordinal exists (CC 5.1; Ley 39/2015 art. 30.4). No tax-specific authority computes art. 33.5.f boundaries with concrete dates (practitioner example, OnTax Legal: sale 10-02-2024 → window 10-12-2023 through 10-04-2024, matches the implementation). Residual ambiguity → warn-cases: (i) a repurchase EXACTLY on a boundary ordinal whose inclusion flips deferral; (ii) clamped boundaries (backward underflow, e.g. sale 30-04 → anterior boundary Feb-end; forward, e.g. sale 31-12 → posterior boundary Feb-end).

### V3 — Fee classification: three buckets per DGT doctrine, two warn-types
Doctrine: **V2117-19** (custody/administration by a commercializer deductible; the covered service is "static" safekeeping, dividend collection, corporate events); **V2629-13** (buy/sell commissions are NOT art. 26 expenses — they adjust acquisition/transmission values, which the shared trade engine already does); **V1047-16** (performance/success fees = management, excluded); consulta 03-04-1998 (only costs directly required for the deposit function; bank-account maintenance out). AEAT Manual cap. 5 (2024/2025) restates. Foreign-broker charger: no on-point consulta; statutory wording not territorially limited and AEAT practice accepts it (deduct, note medium confidence).
Buckets (Común only; Gipuzkoa deducts nothing regardless):
- **A deduct vs RCM**: `custod`, `safekeep`, `deposit fee`, `depósito`, `administraci` (accented + unaccented); medium-confidence: `dividend fee`, `corporate action`, `cobro de dividendos`, cupón handling.
- **B value-adjustment (never an art. 26 expense; already inside SellDetails when per-trade)**: commissions, exchange/SEC/FINRA/canon/stamp/FTT pass-throughs.
- **C non-deductible**: `market data`/`datos de mercado`/research/quote/snapshot; wire/withdrawal/transfer/SEPA; margin interest; `management`/`advisory`/`performance`/`gestión`/`éxito`.
- **WARN (unresolved, default non-deductible)**: inactivity / minimum-activity / maintenance / connectivity fees; standalone FX-conversion fees; securities transfer-out (`traspaso`) fees.

### V4 — Modelo 109 casillas: official Hoja layouts FOUND for 2023, 2024 and 2025
Source: Hacienda Foral de Gipuzkoa "Propuesta de autoliquidación" specimen PDFs (per-ejercicio pages `gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/<year>/propuesta-autoliquidacion`; files saved in the session scratchpad as `propuesta-devolver-2023/2024/2025.pdf`). Verified Hoja de liquidación numbers:
- Stable 2019→2025: 26/27 RCM ahorro components · **28 rendimiento neto del capital mobiliario (ahorro)** · 29 compensación RCM negativos anteriores · **30 ganancias patrimoniales (transmisiones)** · 31 compensación pérdidas anteriores · **33 BASE LIQUIDABLE AHORRO** · **38 cuota íntegra ahorro** · 41 cuota íntegra total.
- Ejercicios 2023/2024: **DDI = casilla 60**, total deducciones 63, **cuota líquida 64**, pagos a cuenta RCM 67, total pagos 72.
- Ejercicio 2025 (NF 1/2025 renumber): **DDI = casilla 70**, total deducciones 73, **cuota líquida 74**, pagos a cuenta RCM 77, total pagos 81.
- 2026: unpublished (filed 2027) — emit the 2025 layout labeled as such, keep a warning.
- The old AÑO-2019 NISAE map's `06+16 / 17 / 07+22` rows are **Anexo 3 internals**, a DIFFERENT numbering space from the Hoja (collision: Hoja 28 = RCM ahorro vs Anexo-4 28 = ganancias). Anexo internals are confirmed only through ejercicio 2024 and demonstrably renumbered in 2025 (Anexo 6 comparison) — keep them for ≤2024 with sheet-labeled keys, warn for ≥2025.

## Tasks

- **O1 — Venue-aware wash-sale disclosure.** Status: ✅ Done (`272c1fbd`)
  (a) Upgrade the marker + docs from "unresolved" to CONFIRMED-2-months for equivalence-decision venues, citing V0778-25/V0951-25, Decision 2017/2320 (+2318/2319), the in-force condition, and the foral persuasive-not-binding note. The engine's 2-month window is now the DGT's own criterion — behavior unchanged.
  (b) NEW runtime warning: using the instrument's listing-exchange metadata where the Flex statement provides it (SecurityInfo listing exchange; fall back to "unknown"), classify venues against a hardcoded equivalent-set table (US 2017/2320 list, EEA venues, ASX, SEHK; sourced comment). For every loss whose homogeneous repurchases fall ONLY in the (2-months, 1-year] zone AND whose venue is non-equivalent or unknown-non-US, emit a warning (log + console + CSV `WASH_SALE_VENUE_REVIEW` row) quantifying the loss that the 1-year limb would defer. Equivalent-venue instruments in that zone need no warning (settled). Do NOT change deferral behavior — warn-only, documented as such.
  (c) ADR note in docs (V1872-25: dual-listed same-class homogeneous; ADR↔ordinary open; ISIN matching aligns with the consulta).
  Tests: fixture or unit tests with a fabricated non-equivalent-venue instrument (repurchase at 3 months → warning; at 1 month → normal deferral, no warning) and an NYSE instrument (no warning either way).
  Commit: `feat(spain-tax): warn when the wash-sale window turns on venue equivalence`
- **O2 — Endpoint citations + boundary warnings.** Status: ✅ Done (`36012b77`)
  (a) Replace the `wash_sale.rs:33` marker with the settled basis (CC 5.1; STS 552/2022 RC 1874/2021; STS 287/2009; Ley 39/2015 art. 30.4; OnTax worked dates). Behavior unchanged (inclusive, last-day clamp).
  (b) NEW warnings when an outcome actually turns on residual arithmetic ambiguity: a repurchase landing EXACTLY on the anterior/posterior boundary ordinal that causes a deferral (or is the nearest miss just outside), and any window whose boundary was month-end CLAMPED where a repurchase falls within the clamp-affected span. Emit log + console + CSV note on the affected sale row; message names the two dates and the euro amount at stake.
  Tests: rstest boundary cases (repurchase on D+2-months exactly → deferred + warned; sale 31-12 with repurchase 28-02 → clamp warning; mid-window repurchase → no warning).
  Commit: `feat(spain-tax): warn when a wash-sale deferral turns on window-boundary arithmetic`
- **O3 — Doctrine-based fee classifier.** Status: ✅ Done (`90ed30c6`)
  Replace the keyword heuristic with the three-bucket + WARN classifier from V3 (Común path; Gipuzkoa untouched — everything stays informational there). Bucket A deducts; C is non-deductible with the citation in the row note; WARN types are non-deductible AND emit a warning naming the open interpretation ("no DGT doctrine on <type>; not deducted — consult a gestor if material") with log + console + CSV note. Replace the `processor.rs` fee marker with the citations (V2117-19, V2629-13, V1047-16, 03-04-1998, AEAT Manual cap. 5); docs get the per-type verdict table incl. the foreign-broker medium-confidence note.
  Tests: unit tests per bucket keyword; fixture with a custody fee (deducted, Común), a market-data fee (denied, note), an inactivity fee (denied + warning).
  Commit: `feat(spain-tax): classify broker fees per DGT doctrine and warn on unresolved types`
- **O4 — Per-ejercicio Modelo 109 casillas.** Status: ✅ Done (`b889f575`)
  Replace the single 2019 map with per-ejercicio Hoja tables (V4): savings-relevant rows keyed to Hoja numbers (28, 29, 30, 31, 33, 38, and DDI 60/70, cuota líquida 64/74 by year); the DDI row LOSES `CASILLA_UNKNOWN` for 2023–2025 (real numbers) and 2026 emits the 2025 layout with a "2025-layout, 2026 form unpublished" warning. Keep the Anexo-3 rows (`06+16`, `17`) for ejercicios ≤2024 only, with keys renamed to say the sheet (`MODELO_109_ANEXO3_...` vs `MODELO_109_HOJA_...`); for ≥2025 emit the Hoja rows only plus a warning that Anexo-level internals are unverified post-reform. Cite the specimen-PDF sources + retrieval date in the code comment and contract. Update the CSV contract (key shapes, per-year mapping table) and `docs/spain-taxes.md`.
  Tests: formatter tests pinning the 2024 vs 2025 vs 2026 mappings (DDI 60 vs 70 vs 70+warning).
  Commit: `feat(spain-tax): map Modelo 109 boxes per ejercicio from the official specimens`
- **O5 — "Open interpretations" documentation section.** Status: ✅ Done (`efb7b0d1`)
  New section in `docs/spain-taxes.md` — one entry per item, each with: status (SETTLED / SETTLED-WITH-EDGE-CASES / OPEN), the authority (consulta/sentencia/specimen with URL + retrieval date), the tool's implemented reading, the exact warning text the tool emits when the case arises, and what the filer should do on seeing it. Entries: venue equivalence (incl. Switzerland lapse, ADR question), window endpoints (incl. the two boundary warn-cases), fee types (per-type table, two warn-types, foreign-broker note), Modelo 109 casillas (Hoja verified 2023–2025, Anexo ≥2025 unverified, 2026 unpublished), and the carried items with their existing warnings (IIC distributions inside the €1,500 exemption; borrowed-balance FX; fecha de transmisión trade-vs-settlement; deferred-loss quantities across splits; Modelo 100 numbers from the consultation draft). Cross-link every runtime warning to its entry. Update `review2.md`/`remediation.md` marker inventories (fee + endpoint + casilla markers now resolved; venue marker resolved for equivalent venues, residual warn-case documented).
  Commit: `docs(spain-tax): add the open-interpretations register`
- **O6 — Gate, smoke, close-out.** Status: ✅ Done — see the close-out at the end of this file.
  Full gate; re-run the real-statement smoke test (`--config /private/tmp/claude-501/-Users-alexandre-projects-alexsavio-investments/864d554e-7895-4b99-9187-48e9e68b5156/scratchpad/es-smoke`, `ibkr-miren` 2025): totals must stay €17.32 / €3.46; expected diffs are ONLY the Modelo rows (Hoja numbering: cuota líquida 74, DDI 70, sheet-labeled keys, Anexo-3 warning) — explain anything else. NVDA/GOOG/STRC are NYSE/NASDAQ → no venue warnings expected. Append a close-out section here (what settled, what warns, final numbers).
  Commit: `docs(spain-tax): close the open-items round`

## Accepted / explicitly out of scope this round
- No behavior change to the deferral engine from venue classification (warn-only; a config- or metadata-driven 1-year mode can be a future feature if a non-equivalent-venue holding ever appears in the user's statements).
- ADR↔ordinary homogeneity stays open (V1872-25 expressly declined it); docs only.
- Anexo 3 post-2024 numbers unverifiable until a 2025 return is generated in Zergabidea (~April 2026 campaign artifacts); warning stands.

## Close-out (O6)

**Date**: 2026-08-11 · Branch `002-spain-tax`, five tasks on top of `55cf7791`.

### Commits

| Task | Commit |
|---|---|
| O1 venue-aware wash-sale disclosure | `272c1fbd` |
| O2 endpoint citations + boundary warnings | `36012b77` |
| O3 doctrine-based fee classifier | `90ed30c6` |
| O4 per-ejercicio Modelo 109 casillas | `b889f575` |
| O5 open-interpretations register | `efb7b0d1` |

Plus two `docs(plan)` status commits and this one.

### Gate

| Check | Result |
|---|---|
| `cargo check --lib --all-targets` | clean |
| `cargo test spain --lib` | **267 passed / 0 failed** (226 at the start of this round, +41) |
| `cargo test --lib` | **733 passed / 33 failed** — every failure is a pre-existing `broker_statement::*::parse_real` case from the private, empty `testdata/` submodule; the count is unchanged and filtering for anything that is not a `parse_real` case yields 0 rows |
| `cargo test --no-fail-fast` (all targets) | lib as above; `tests/generate.rs` 1 passed; binary target has no tests |
| `./check` (clippy, dev + release, `-Dwarnings`) | the same **4 pre-existing errors** in untouched files: `portfolio/rebalancing.rs`, `quotes/cbr/mod.rs`, `analysis/performance/statistics.rs`, `formats/xls/table.rs`. No new lint at any commit |

### Real-statement smoke test

`cargo run -- --config <scratch>/es-smoke tax-statement ibkr-miren 2025 <scratch>/es-openitems.csv`,
Gipuzkoa 2025. Every filed figure is **unchanged**: base liquidable €17.32, cuota íntegra €3.46,
credit €0.00, cuota líquida €3.46, ganancias y pérdidas €16.18.

The CSV diff against the previous round is confined to the Modelo block, exactly as predicted:

- the AÑO-2019 provenance banner is replaced by the ejercicio-2025 specimen banner plus the Anexo-3
  post-2024 warning;
- `MODELO_109_CASILLA_06+16` / `_17` are gone (Anexo 3, unverified from 2025) and the Hoja rows are
  keyed `MODELO_109_HOJA_<n>`: 28 rendimiento neto 1.14, 29 and 31 compensation 0.00, 30 ganancias
  16.18, 33 base 17.32, 38 cuota íntegra 3.46, **70 DDI 0.00**, **74 cuota líquida 3.46**;
- the withholding warning names casilla **77 (hoja)** instead of Anexo 3's `07+22`, since that sheet
  is not emitted for 2025;
- `MODELO_109_CASILLA_UNKNOWN` and its "not published anywhere reachable" warning are gone: the DDI
  casilla is a real number now.

No other line moved. No venue review (NVDA/GOOG/STRC are NASDAQ-listed, settled by V0778-25), no
boundary review (no repurchase lands on a window edge), no fee review (Gipuzkoa deducts nothing, and
the statement carries no fee rows anyway). The three pre-existing warnings — open NVDA window, the
€82.23 exemption payer list, the €-17.18 borrowed-balance FX — print unchanged.

### What is settled now

- **Venue equivalence**: two months is the DGT's own criterion for equivalence-decision venues
  (V0778-25, V0951-25; Decisions (EU) 2017/2320, 2017/2318, 2017/2319). Behaviour unchanged; a
  warn-only `WASH_SALE_VENUE_REVIEW` fires when a repurchase falls in the (2 months, 1 year] zone on
  a venue with no decision in force, or one the statement does not name.
- **Window endpoints**: CC art. 5.1 de fecha a fecha with STS 552/2022 and its line; month-end clamp
  per Ley 39/2015 art. 30.4. Behaviour unchanged; a warning fires when a deferral is decided by the
  terminal day itself, by the day just outside it, or by a clamped edge.
- **Fee types**: per-type DGT doctrine (V2117-19, V2629-13, V1047-16, consulta 03-04-1998, AEAT
  Manual cap. 5). Común behaviour changed: market-data, cash-movement and management fees now carry
  their citation, dividend-collection fees are deducted, and three types with no doctrine are flagged
  rather than silently denied. Gipuzkoa is bit-identical — nothing is deductible, every row
  informational, no warning.
- **Modelo 109 casillas**: Hoja numbering verified from the official per-ejercicio specimens, with the
  NF 1/2025 renumbering of the deduction block. No casilla is guessed any more.

### Left open

- The three flagged fee types (inactivity/maintenance, standalone FX conversion, traspaso), Anexo 3
  internals from ejercicio 2025, the 2026 form, the ADR↔ordinary homogeneity question, and the
  carried items (IIC distributions, borrowed-balance FX, split-adjusted carried-in deferrals, Modelo
  100 draft numbers). All catalogued in the open-interpretations register in `docs/spain-taxes.md`,
  each with the warning text that surfaces it.
- No `TODO(verify)` marker remains anywhere in the Spanish feature. The two left in `src/` are
  German (`tax_statement/germany/csv_formatter.rs`, `tax_statement/germany/statement.rs`) and out of
  scope for this branch. They were closed in a later round — see `specs/001-germany-tax/open-items.md`.
- The pre-existing 33 `parse_real` failures and 4 clippy errors in untouched files.

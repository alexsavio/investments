# Spanish tax report: printable A4 HTML for Spain (común), Gipuzkoa and Navarra

## Context

PR #4 (`feat/germany-tax-html-report`, targets `dev`) added a printable A4-landscape HTML report
for the German tax statement (`investments tax-statement <portfolio> <year> report.html`), with a
processor that records broker-level detail (`statement.report: ReportDetails`) next to the tax
entries and a renderer that is a pure function of the statement. The user now wants the same kind
of report for the Spanish jurisdiction, covering its three regimes: Spain common regime (`comun`),
Gipuzkoa and Navarra. This plan is written for a fresh session and must be executable without the
current conversation.

Ground rules carried over from the German work (see memory `base-work-on-dev-branch`):
- Base the work on `dev` after PR #4 is merged (or on `feat/germany-tax-html-report` if not yet).
- Never run `cargo fmt` repo-wide on `dev`: its Spain and network files are not rustfmt-clean.
  Format only touched files with `rustfmt <file>`.
- Cargo's target dir is `~/.cache/cargo-target`; `./target` in the repo is stale.
- Every EUR figure in a report goes through `tax_statement::eur::format_eur`, then German/Spanish
  punctuation (`1.234,56` in both languages).

## Facts gathered (2026-09-08, branch `feat/germany-tax-html-report` = dev + PR #4)

### What the German report already provides (reuse, do not rewrite)
- Neutral HTML infrastructure in `src/tax_statement/germany/html/`: `builder.rs` (escape, `Cell`/`Row`/`RowKind`/`Column`/`col`, `table`, `section_start/end`, `h3/h4/p/ul/note/b`), `style.rs::CSS`, `format.rs` (`germanize` → works for es-ES too, `eur`, `dec2`, `rate`, `qty`, `pct`, `pct_observed`, `days` + tests), `ReportMeta` and `kpi` (`html/mod.rs:30`, `:298`), the `Section`/`SectionRenderer` registry + `render` shell (`:56-167`; German bits only `lang="de"`, `<title>`, "Inhalt"), `sum_row`/`security_heading`/`id_cell` (`detail.rs:24-45`), `ActivityRow`/`SecurityResult`/`add_signed` (`summary.rs:385-660`).
- Neutral report data in `src/tax_statement/germany/report_details.rs`: `ReportDetails` container, `TradeRow`, `SaleLotRow` (German only `pre_2009`), `SaleWorksheet`, `OpenLotRow`, `BookingRow`, `WithholdingRow`, `FxRow` (German only `treatment: FxTreatment`), `SecurityRow` (German only `teilfreistellung_rate`), each with a German `category: AssetCategory`; helpers `security_identity`, `isin_country`, `ecb_rate` (`:273-324`); collectors `collect_buys` (`processor.rs:548`), `collect_open_lots` (`:599`), `collect_securities` (`:669`, reads German-only `vorabpauschale`/`stock_grants`), `collect_fx_rows` (`:1253`, German treatment mapping + §20 rounding).
- Shared already: `src/tax_statement/eur.rs::{format_eur, round_eur}`, `src/tax_statement/fx_fifo.rs` (`CurrencyFxResult.ledger: Vec<FxLedgerRow>`, `FxLedgerKind::{Acquisition, Disposal, Repayment}`; Spain never reads `ledger` yet), `write_atomically` and `GermanOutputFormat` in `src/tax_statement/mod.rs:640-690` (German in name only).
- Needs parameters for Spain: language strings (`country_name`, `side_label`, `lot_source_label`, `booking_kind_label`, `activity_label`, section ids/titles, prose), date format (`format::date`/`datetime` hardcode `%d.%m.%Y`; Spain: `dd/mm/yyyy`), asset-category type, FX-treatment enum.

### Spanish tax statement (all on `dev`)
- Entry `generate_spanish_tax_statement` (`src/tax_statement/mod.rs:152`): `BrokerStatement::load`, ECB converter via `CurrencyConverter::for_jurisdiction`, `spain::compute_tax_year(broker_statement, year, converter, tax_config) -> (SpanishTaxStatement, bool)` (`spain/processor.rs:133`); output block `:190-201` is a bare `File::create` + `spain::CsvFormatter::write` (no format dispatch, no atomic write); console summary `:203-400` prints regime, GyP/RCM, savings base/quota, credit, the next-year `taxes.spain` config block (loss_carryforward rcm/gyp, deferred_losses), expiry warnings, valores homogéneos figures and reviews, the five `*_message()` sentences, unpriced years, stock vests. `margin_interest_message()` is only logged (`processor.rs:152-168`).
- `SpanishTaxStatement` (`spain/statement.rs:236-397`): `regime: SpanishTaxRegime`, `scale: SavingsScale`; entries `capital_gains: Vec<CapitalGainEntry>` (`:35`, with `lots: Vec<SpanishLotDetail>` `:20` = acquisition_date, quantity, cost_eur, coefficient, actualized_cost_eur, proceeds_eur, gain_eur; plus deferred_loss, integrable_amount, notes), `wash_sale_reintegrations` (`:64`), `dividends` (`:144`: gross_eur, withheld_eur, treaty_capped_credit, exemption_eligible), `interest` (`:164`), `fees` (`:219`: deductible, review), `fx_gains` / `fx_borrowed_review` (`FxGainEntry` `:178`: date, currency, acquisition_date, amount_eur, activity_code), `stock_grants`, `corporate_actions`, `short_positions`, `abatement_lots`, `wash_sale_unpriced_years`, `deferred_losses_next`, `wash_sale_window_gaps`/`venue_reviews`/`boundary_reviews` (each with `.message()`); totals incl. regime-only ones (`total_dividend_exemption` Gipuzkoa; `custody_fee_cap`/`total_capped_fees`/`small_disposals_*` Navarra); `rcm_net`/`gyp_net`/`rcm_taxable`/`gyp_taxable`; ledgers `rcm/gyp_ledger_prior|next: LossLedger` (`balances()` by vintage), `rcm/gyp_applied: LedgerApplication`, `cross_offset_*`/`prior_cross_offset_*` (zero under Gipuzkoa), `rcm/gyp_expired`; `foreign_gross_income`, `foreign_taxable_income`, `total_foreign_tax_credit`, `savings_base`, `savings_quota`, `average_savings_rate`, `net_tax_due`. Private regime params (`dividend_exemption_limit`, `cross_offset`, `treaty_rate`, `custody_fee_cap_fraction`, `small_disposals_exemption_applies`) need `pub(super)` accessors for a sibling `html` module. Accessors: `rcm_own_group_losses_applied()` `:865`, `gyp_own_group_losses_applied()` `:870`, `prior_cross_offset()` `:875`, `declarable_rcm_income()` `:1258`; messages `abatement_message` `:665`, `small_disposals_message` `:758`, `custody_fee_cap_message` `:821`, `margin_interest_message` `:844`, `carried_cross_offset_message` `:888`.
- CSV (`spain/csv_formatter.rs`): `write` `:166-215`; 19-column detail rows `:219-421` (Capital Gain + inline Lot rows "cost × coefficient = actualized_cost", Wash Sale Reintegration, Dividend, Interest, FX Gain/Loss, FX Borrowed (review), Fee, Stock Grant, Corporate Action); `write_summary_rows` `:436` (~30 `SUMMARY_*` rows; six labels switch wording per regime `:525-618`); `write_carryforward` `:740`; `write_deferred_losses` `:787`; `write_modelo_boxes` `:836` with private modules `modelo_109` `:35` (Gipuzkoa; `Layout` per ejercicio `:53-88`: 2024 casillas 60/64/67 + Anexo 3 06+16/17; 2025+ 70/74/77; hoja 28/29/30/31/33/38), `modelo_100` `:96` (Común, draft Orden HAC/277/2026: 0027, 0029, 0037, 0326-0340, 0460, 0588; warning 0597), `modelo_f93` `:113` (Navarra, Orden Foral 24/2026, verified 2025: 031, 037, 047, 050, 706, 8808, 809, 8815, 8809, 8810, 8825, 8805, 8840, 8841, 815, 829, 572; conditional 1658-1672, 8816, 8850, 818, 8875); `write_warnings` `:1329`; `format_date` is ISO (`:1503`), `format_coefficient` 3 dp (`:1513`).
- Rules `src/taxes/spain/`: `SpanishTaxRegime {Gipuzkoa, Comun, Navarra}` + `description()` (`mod.rs:39`), `scale.rs` (`SavingsScale::for_year/brackets/tax/average_rate`, years 2024-2026), `carryforward.rs` (`LossLedger`, `LedgerApplication`, `CARRYFORWARD_YEARS = 4`), `compensation.rs` (`CrossOffset {None, AeatTwoPhase, NavarraOrdered}`), `credit.rs`, `exemption.rs` (Navarra €3,000), `coefficients.rs` (Gipuzkoa). All regime decisions in `SpanishTaxParams::resolve` (`processor.rs:66-111`); treaty rate hard-coded 0.15 (`:71`).
- Processor (`spain/processor.rs`): `process_trades` `:399` → `price_sale` `:560` (`StockSell::calculate` `:575`; locals `details.local_revenue`, `details.local_commission`, lots with `StockSourceDetails`, original-currency `Cash`; runs for every year, only filing-year sales become entries) → `apply_wash_sale_rule` `:686`; `process_dividends` `:970`; `process_interest` `:1093`; `process_fees` `:1304`; `process_fx_gains` `:1386` (`compute_fx_fifo(&foreign_cash_flows, &[], …)`, held → `fx_gains`, borrowed → `fx_borrowed_review`); `process_stock_grants` `:236`; `process_corporate_actions` `:298`; `convert_to_eur` `:951`. Nothing broker-level (original currency, ECB rate, trade ids, open lots, securities) is stored.
- Tests: `spain/tests.rs` private harness (`FixedEurBackend` 0.9 EUR/USD, `converter` `:68`, `read_fixture` `:86`, `spain_config(regime)` `:98`, `run_pipeline_with_config` `:111`, `run_pipeline(fixture, year, regime)` `:124`); `golden_tests.rs` byte-exact CSV goldens in `testdata/golden/*.csv` (`CORPUS` `:213`, cases `:134-166` cover fifo×3 regimes, income×3, wash_sale_multi_lot×Gipuzkoa, cross_offset×{Común, Navarra}, small_disposal×{Navarra, Común}, small_disposal_mixed×Navarra; regenerate with `UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests`); `property_tests.rs`, `continuity_tests.rs`; 37 fixture dirs under `spain/testdata/`.
- Docs: `docs/spain-taxes.md` (Usage `:79`, form mapping `:729`, "CSV output" `:826`, "Sell simulation" `:834`, "Open interpretations" `:889`), README `:14` and `:84-88`. Template to mirror: `docs/germany-taxes.md:373-433`.

## Decisions

1. **Two PRs, both targeting `dev`**, in order:
   - **PR A `refactor(tax-statement): share the HTML report infrastructure`** — pure refactor. German HTML output stays byte-identical (pinned by new HTML goldens before any move). Neutral code moves out of `germany/html` and `germany/report_details.rs`; `GermanOutputFormat` becomes `OutputFormat`; the Spanish output block adopts `OutputFormat` + `write_atomically` (CSV arm only, HTML arm returns a clear error).
   - **PR B `feat(spain-tax): add printable A4 HTML report`** — the Spanish report in Spanish (`lang="es"`, prose es-ES, dates `dd/mm/yyyy`, numbers `1.234,56`), same look and depth as the German one, plus Spain-only content: actualization coefficients per lot (Gipuzkoa), valores homogéneos deferrals/reintegrations, compensation ledgers with vintages and expiry, Modelo 109 / Modelo 100 / Modelo F-93 boxes, per-regime warnings, and the next-year `taxes.spain` YAML block.
2. **Form-box mappings are never duplicated**: `modelo_109`/`modelo_100`/`modelo_f93` move out of `csv_formatter.rs` into `spain/forms.rs`, producing a `FormMapping` both the CSV (byte-identical goldens) and the HTML consume. Same for the six regime-switching summary labels and the seventh (FX) label.
3. **Report rows come from the same locals that feed the tax entries** (as in Germany): `SpanishTaxStatement.report: ReportDetails` filled in `price_sale`, `process_dividends`, `process_interest`, `process_fees`, `process_fx_gains` (via `result.ledger`), plus shared collectors for buys, open lots and securities.
4. **Parameterisation = two type parameters, no `Locale` struct**: `ReportDetails<C, T>` (asset category `C`, FX treatment `T`) with per-jurisdiction type aliases so every existing German import path keeps compiling. Labels, dates and prose stay in each jurisdiction's own `html/format.rs`; the only shared text is the document shell (`Document { lang, title, toc_id, toc_title }`).
5. Spain's asset category is `AssetClass { Stock, Fund }` from the existing `taxes.etf_classification` map (unclassified = share, exactly the processor's assumption). Spain's FX treatment is `FxTreatment { HeldBalance /* → GyP */, BorrowedBalance /* → revisión manual */ }`.
6. The five `*_message()` sentences, `WashSale*Review::message()`, `FeeEntry.review` and entry `notes` are English and shared with CSV/console by decision 2; v1 shows them verbatim under "Avisos del cálculo (texto literal)". Spanish variants are a follow-up.

## PR A — shared infrastructure (German output byte-identical)

Layout after the refactor:

```
src/tax_statement/html/mod.rs        ReportMeta, Section<S>, SectionRenderer<S>, Document, render<S>(), kpi()
src/tax_statement/html/builder.rs    germany/html/builder.rs moved verbatim (pub(crate) items)
src/tax_statement/html/format.rs     punctuate (ex-germanize), eur, dec2, rate, qty, pct, pct_observed, days + their tests
src/tax_statement/html/style.rs      germany/html/style.rs moved verbatim (pub(crate) const CSS)
src/tax_statement/report/mod.rs      pub mod details; pub mod collect;
src/tax_statement/report/details.rs  ReportDetails<C, T> (manual Default, derive Debug+Clone), TradeRow, SaleLotRow (no pre_2009),
                                     SaleWorksheet<C>, OpenLotRow<C>, BookingRow<C>, WithholdingRow<C>, FxRow<T>, SecurityRow<C>
                                     (no teilfreistellung_rate), TradeSide, LotSource, BookingKind, security_identity, isin_country,
                                     ecb_rate, ReportDetails::security_names()
src/tax_statement/report/collect.rs  collect_buys, collect_open_lots, collect_securities, collect_fx_rows (generic)
```

Shared signatures:

```rust
pub type SectionRenderer<S> = fn(&mut String, &S, &ReportMeta) -> bool;
pub struct Section<S> { pub id: &'static str, pub title: &'static str, pub render: SectionRenderer<S> }
pub struct Document { pub lang: &'static str, pub title: String, pub toc_id: &'static str, pub toc_title: &'static str }
pub fn render<S>(doc: &Document, sections: &[Section<S>], statement: &S, meta: &ReportMeta,
                 title_block: impl FnOnce(&mut String)) -> String;   // body of germany/html/mod.rs:135-167
pub fn collect_buys<C, T>(report: &mut ReportDetails<C, T>, bs: &BrokerStatement, year: i32, converter: &CurrencyConverter) -> GenericResult<()>;
pub fn collect_open_lots<C, T>(report, bs, year, converter, classify: &dyn Fn(&str) -> C,
                               grant_cost_eur: &dyn Fn(&str, Date, Decimal) -> GenericResult<Decimal>) -> GenericResult<()>;
pub fn collect_securities<C, T>(report, bs, extra_symbols: &[&str], classify: &dyn Fn(&str) -> C);
pub fn collect_fx_rows<C, T>(report, results: &[CurrencyFxResult], year: i32,
                             treatment: &dyn Fn(&FxLedgerRow) -> Option<(T, Decimal)>);  // closure returns None for Acquisition
```

Steps (one commit each, `cargo test --lib germany` green after each):

**Status: done, PR #6 (`refactor/share-html-report-infrastructure` → `dev`).**

- [x] 1. Pin first — 6 goldens under `germany/testdata/golden/` (`fifo`, `dividend_withholding`,
  `fx_ledger`, `vorabpauschale`, plus `derivative` and `short_position` added on review).
  `income_edge` is a negative-path fixture the broker-statement reader rejects, so it cannot be
  pinned. The `UPDATE_GOLDEN` mechanism was extracted from `spain/golden_tests.rs` into
  `tax_statement::golden::GoldenCorpus` instead of copied, so both jurisdictions share one
  comparison, one orphan check and one regeneration story.
- [x] 2. `html/` module — `builder.rs` and `style.rs` moved, `format.rs` carved out
  (`germanize` → `punctuate`), `mod.rs` holds `ReportMeta`, `Section<S>`, `Document`, `render<S>`
  and `kpi`. `germany/html/format.rs` re-exports the shared numerics.
- [x] 3. `report/` module — `ReportDetails<C, T>` with a hand-written `Default`; `collect_buys`,
  `collect_open_lots`, `collect_securities` (now taking `extra_symbols`) and `collect_fx_rows`
  (now taking a `treatment_of` closure). Germany keeps `AssetCategory`, `FxTreatment`, `classify`,
  the aliases and a thin `collect_fx_rows` wrapper so its two unit tests stayed unchanged.
- [x] 4. `OutputFormat` — renamed, Spanish output through it and `write_atomically`. The `.html`
  arm rejects **before** opening anything, so the user does not read a failure to write a temp file.
- [x] 5. Verify — 898 lib tests pass (34 known `parse_real` failures), clippy clean in every touched
  file, `rustfmt --edition 2024` per file, both jurisdictions run end to end on real IBKR data.

Landed beyond the plan, from review:

- [x] `write_atomically` carries an existing file's mode over the rename. A tax CSV chmod'ed to
  `0600` came back `0644`; Germany already had this on `dev` and routing Spain through the helper
  would have spread it. Reproduced and fixed, with a test.
- [x] `.gitattributes` marks both golden corpora binary, so a checkout under `core.autocrlf=true`
  cannot rewrite them.
- [x] Tests for `extra_symbols` (replacing it with `&[]` had left every test green), the pre-2009
  Altbestand branch, the `AssetCategory` ⇄ `TeilfreistellungRate` round trip, and
  `OutputFormat::from_path`.

Left for later, deliberately:

- `by_security` (`germany/html/summary.rs`) allocates a three-`String` map key per row where the
  sibling sections borrow. Pre-existing; touching it would put byte-identity at risk for no gain.
- `collect_open_lots`'s grant-cost closure and the `Grant` / `CorporateAction` lot sources have no
  German fixture, so they are unexercised (already true on `dev`).
- `statement.vorabpauschale_missing_nav` symbols still do not reach `extra_symbols`: a missing-NAV
  fund with no trades shows in the Vorabpauschale section and not in the Wertpapierübersicht. True
  on `dev` too; fixing it changes the rendered output, so it does not belong in a byte-identical PR.
- Only `Section20Taxable` FX rows reach a golden; the §23 and loan-repayment treatments are pinned
  at row level by `processor.rs::fx_rows_follow_the_ledger_with_treatments` but never rendered.

Original step detail:

1. **Pin first.** In `germany/html/tests.rs` add `fixed_meta(year)` (constant `generated_at`, e.g. 2026-01-15 12:00) and an `#[rstest]` golden test over fixtures `fifo`, `dividend_withholding`, `fx_ledger`, `vorabpauschale` (with the Equity classification of `tests.rs:227-232`) against `src/tax_statement/germany/testdata/golden/<name>.html`, using the `UPDATE_GOLDEN=1` mechanism copied from `spain/golden_tests.rs:233-250` plus an orphan check like `:193-213`. Generate with `UPDATE_GOLDEN=1 cargo test --lib germany::html::tests`, commit the goldens.
2. **`html/` module.** `git mv` builder.rs and style.rs; carve `format.rs` (rename `germanize` → `punctuate`). `germany/html/mod.rs`: `pub use crate::tax_statement::html::ReportMeta;`, `SECTIONS: &[Section<GermanTaxStatement>]`, `render()` builds `Document { lang: "de", title: format!("Informativer Steuerbericht {}", year), toc_id: "inhalt", toc_title: "Inhalt" }` and calls the shared `render`; delete the local `kpi`; add `pub(super) use crate::tax_statement::html::{builder, style};` so `detail.rs`/`summary.rs` imports stay. `germany/html/format.rs` keeps its German labels/dates and re-exports the shared numerics; update `tests.rs` imports. Add `pub(crate) mod html; pub(crate) mod report;` to `src/tax_statement/mod.rs:1-9`.
3. **`report/` module.** Shrink `germany/report_details.rs` to `AssetCategory` (+ `impl From<AssetCategory> for TeilfreistellungRate`), `FxTreatment`, `classify`, the type aliases (`pub type ReportDetails = report::ReportDetails<AssetCategory, FxTreatment>;` etc.) and re-exports. Renderer edits: `detail.rs::lot_notes` computes Altbestand as `lot.open_date < 2009-01-01`; the securities table uses `TeilfreistellungRate::from(security.category)`; `security_names` delegates to `statement.report.security_names()`. Processor edits: drop `pre_2009`/`teilfreistellung_rate` field inits; replace `collect_buys`/`collect_open_lots`/`collect_securities` (`processor.rs:548-723`) and `collect_fx_rows` (`:1253-1332`) with the shared calls, passing `&|isin| AssetCategory::from(classify(tax_config, isin))`, `&|symbol, date, qty| grant_lot_cost_basis_eur(broker_statement, symbol, date, qty, converter)`, `extra` = symbols of `vorabpauschale` + `stock_grants`, and a treatment closure that does the `ForeignCurrencyTaxation` match and the §20 cent rounding.
4. **`OutputFormat`.** Rename `GermanOutputFormat` → `OutputFormat` (`src/tax_statement/mod.rs:640-663`, call sites `:500-503`). Replace the Spanish output block `:190-201` with `OutputFormat::from_path` + `write_atomically`, `Csv => spain::CsvFormatter::write(...)`, `Html => Err!("The printable HTML report is not available for Spain yet; write a .csv path")`, and the green "Spanish tax {description} written to …" line. Hand-format (file is rustfmt-ignored). Console summary untouched.
5. **Verify**: `cargo test --lib germany`, `cargo test --lib spain`, `cargo test --lib` (34 known `parse_real` failures), clippy warnings only in touched files, `rustfmt --edition 2024 <touched files>` (never `cargo fmt`).

## PR B — Spanish report

**Status: done, PR pending (`feat/spain-html-report` → `dev`), three commits.**

- [x] **B1 `spain/forms.rs`** — the three modelo modules, the six compensation labels and the FX one
  moved behind a `FormMapping` the CSV and the report both consume. `FormBox` keeps `concept` and
  `sheet` apart rather than storing one exact CSV label, so the CSV rebuilds its
  `(hoja, casilla 28)` suffix byte for byte while a table with its own Casilla column shows the
  concept alone. `declarable_rcm_income` moved onto `SpanishTaxStatement`. All 12 CSV goldens
  unchanged. `form_name(regime)` was added on top of the plan: the title block and the methodology
  both name the filer's own form, and a second literal would drift.
- [x] **B2 report types, statement field, processor pushes** — `spain/report.rs` with
  `AssetClass`/`FxTreatment` and the aliases; `SpanishTaxStatement.report`; `PricedSale.report`
  built in `price_sale` under the filing-year filter and moved onto the statement in
  `process_trades`, so `report.sales[i]` ↔ `capital_gains[i]`; bookings and the withholding row from
  `process_dividends`/`process_interest`/`process_fees`; `collect_fx_rows` over the whole ledger;
  the three shared collectors in `process_broker_statement`. `grant_lot_cost_basis_eur` reshaped to
  the German signature. The `small_disposals_exemption_applies` accessor the plan asked for was not
  added: the section it was for keys off `small_disposals_exemption`/`_unmeasurable`, so it would
  have been dead code.
- [x] **B3 `spain/html/`** — the 14 sections as tabled below, `lang="es"`, dates `dd/mm/yyyy`,
  numbers `1.234,56`, ISO confined to the config block. The YAML block is a `<pre>` with an inline
  style: the stylesheet is Germany's too, and adding a `pre` rule there would have rewritten all six
  German goldens for a page they do not have.
- [x] **B4 output arm, docs, tests** — the `.html` arm in `src/tax_statement/mod.rs`; the new
  `## HTML report` section in `docs/spain-taxes.md` plus Usage, README and `.gitignore`; 16 tests in
  `spain/html/tests.rs`; 4 HTML goldens in `spain/golden_tests.rs` with the orphan check over both
  extensions.
- [x] **Verify** — 418 `spain` tests and 102 `germany` tests pass (`cargo test --lib`: 924 pass, 34
  known `parse_real` failures from the private submodule); clippy clean in every touched file;
  `rustfmt --edition 2024` per new file, never repo-wide. Run end to end on the real IBKR statement
  under all three regimes and read back as PDF (21–23 pages each): the FIFO worksheets, the currency
  ledger, the page breaks and the column widths all hold.

Left for later, deliberately:

- **`collect_open_lots` ignores stock splits** (`src/tax_statement/report/collect.rs:90`). It pushes
  `buy.get_unsold()` raw, while `BrokerStatement::open_positions`
  (`src/broker_statement/mod.rs:706-710`) multiplies the same quantity by
  `stock_splits.get_multiplier(...)`. An instrument that split after purchase and is still held
  therefore shows its pre-split share count in "Posiciones abiertas". `cost_eur` and `price` stay
  consistent with each other, so only the count contradicts the account. Found by review of this
  PR, but the code and the bug are shared: Germany's "Offene Positionen" prints the same, and fixing
  it rewrites six German goldens. It belongs in its own PR, where that diff is the change under
  review rather than noise beside a new report.
- **The Spanish processor never reads `broker_statement.cash_grants`.** Germany's does, and reports
  a cash award as sonstige Einkünfte. Spain drops the field silently, so a statement carrying one
  produces a return that never mentions it — while a stock vest, equally outside the savings base,
  *is* reported. Fixing it needs a statement field, a processor loop, a CSV row (the CSV contract
  changes), a console line and a report row, so it is its own PR. Documented meanwhile in
  `docs/spain-taxes.md` under "Important notes".
- **`WithholdingRow.currency` is the dividend's, not the withholding's** (`processor.rs`). The two
  are the same in every statement the tool has seen, and Germany does the same; a broker that
  withheld in a different currency would label the `Retenido` cell wrongly. The EUR columns and the
  rate are unaffected.

Original step detail:

### B1. `src/tax_statement/spain/forms.rs` (CSV goldens are the safety net)

```rust
pub struct FormBox { pub key: String /* "MODELO_109_HOJA_28" */, pub form: &'static str /* "Modelo 109" */,
                     pub casilla: String /* "28", "06+16", "0326_0340", "815 (= 524)" */, pub label: String /* exact CSV label */, pub value: Decimal }
pub struct FormMapping { pub title: &'static str, pub header_lines: Vec<String> /* every "# …" line after the title, in order */,
                         pub boxes: Vec<FormBox>, pub footer_lines: Vec<String> /* Navarra H3 note */, pub withholding_casilla: String }
pub fn form_mapping(statement: &SpanishTaxStatement) -> FormMapping;
pub struct CompensationLabels { pub own_rcm, own_gyp, cross_rcm, cross_gyp, prior_rcm, prior_gyp: &'static str }
pub fn compensation_labels(regime: SpanishTaxRegime) -> CompensationLabels;   // csv_formatter.rs:579-624
pub fn fx_summary_label(regime: SpanishTaxRegime) -> &'static str;            // csv_formatter.rs:534-543
```

Move `modelo_109`/`modelo_100`/`modelo_f93` (`csv_formatter.rs:20-159`) unchanged; move `declarable_rcm_income` (`:1266-1274`) onto `impl SpanishTaxStatement`. `write_modelo_boxes` (`:836-1264`) becomes: blank line, `# {title}`, `# {line}` per header line, `{key},{label},{value}` per box, `# {line}` per footer line, then the withholding warning with `mapping.withholding_casilla`. Store lines exactly as the CSV breaks them (one string per `# …` line) so goldens stay byte-identical; the HTML joins them with spaces. `write_summary_rows` uses the two label functions. Run `cargo test --lib spain::golden_tests` after each of the three moves. Add a unit test for a Gipuzkoa **2024** statement asserting keys `MODELO_109_ANEXO3_06+16`, `MODELO_109_ANEXO3_17`, `MODELO_109_HOJA_60`, `MODELO_109_HOJA_64` (no golden pins the Anexo 3 branch: all 12 corpus cases file 2026).

### B2. Report types, statement field, processor pushes

`src/tax_statement/spain/report.rs` (new): `AssetClass { Stock, Fund }`, `FxTreatment { HeldBalance, BorrowedBalance }`, `classify(tax_config, isin) -> AssetClass` (Fund when `TaxConfig::get_etf_classification(isin) != None`), `pub type ReportDetails = report::ReportDetails<AssetClass, FxTreatment>;` + per-row aliases and re-exports.

`spain/statement.rs`: add `pub report: ReportDetails` to `SpanishTaxStatement` (`:236-397`, derive `Clone` stays valid because the shared struct derives Clone) and `report: ReportDetails::default()` in `new` (`:412-481`); add `pub(super)` accessors `treaty_rate()`, `cross_offset()`, `dividend_exemption_limit()`, `custody_fee_cap_fraction()`, `small_disposals_exemption_applies()` next to `:865-877`. `spain/mod.rs:20-26`: `mod forms; mod html; mod report; pub use self::html::HtmlReport;`.

`spain/processor.rs`:
- `SpanishTaxParams` (`:34-64`) gains `tax_config: &'a TaxConfig` (set in `resolve` `:67`).
- `PricedSale` (`:360-379`) gains `report: Option<(TradeRow, SaleWorksheet)>`, built in `price_sale` **only when the sale year is the filing year** (the replay prices every year; report rows must follow the same filter as entries). In `price_sale`: after `:575` destructure `trade.type_` (`StockSellType::Trade { price, volume, commission }`; the other arm is unreachable because of the filter at `:416`); `eur_per_unit = ecb_rate(converter, trade.execution_date, price.currency)?`; `security_identity` for the name; in the lot loop (`:592-638`) push a `SaleLotRow` 1:1 with the `SpanishLotDetail` pushed at `:629` (`open_date = lot.conclusion_time.date`, `open_trade_id = lot.trade_id.clone()`, `source` from `StockSourceDetails`, `quantity`, `price`, `cost_eur = lot cost`, `proceeds_eur = lot proceeds`, `gain_loss_eur = proceeds − actualized cost`, `holding_days`); after `:642` build the sell `TradeRow` (`gross: volume.amount`, `commission: -commission.amount`, `net`, `amount_eur: net_proceeds_eur`) and the `SaleWorksheet` (`category: classify(params.tax_config, &isin)`, `gross`, `commission`, `proceeds_eur`, `cost_basis_eur: cost_eur`, `gain_loss_eur: fiscal_gain_loss`, `lots`).
- `process_trades` filing-year loop (`:467-514`): `if let Some((trade_row, mut worksheet)) = sale.report.take() { worksheet.notes = joined notes; report.trades.push(trade_row); report.sales.push(worksheet); }` before `capital_gains.push`, so `report.sales[i]` ↔ `capital_gains[i]` and lot indexes match.
- `process_dividends` (`~:1016`): `BookingRow` (Dividend; `currency`/`amount` from `dividend.amount`, `eur_per_unit` via `ecb_rate`, `amount_eur: gross_eur`); when `!dividend.tax_withheld.is_zero()` also the WithholdingTax booking (`description: format!("Retención en origen: {}", …)`, negative amounts) and a `WithholdingRow` (`country_code: isin_country(&isin)`, `withholding_rate = withheld_eur / gross_eur` (0 when gross is zero), `creditable_eur: reclaim_floor` i.e. the per-row treaty-capped credit).
- `process_interest` (`~:1122`): Interest `BookingRow` with the entry's description.
- `process_fees` (`~:1367`): hoist `fee_cash = fee.amount.withholding()`; Fee `BookingRow` (`amount: -fee_cash.amount`, `amount_eur: -amount_eur`).
- `process_fx_gains` (`~:1404`): `collect_fx_rows(&mut statement.report, &results, params.year, &|row| match row.kind { Acquisition => None, Disposal { amount, .. } => Some((FxTreatment::HeldBalance, amount)), Repayment { amount, .. } => Some((FxTreatment::BorrowedBalance, amount)) })` (full precision, matching `FxGainEntry.amount_eur`).
- `grant_lot_cost_basis_eur` (`:911-948`) reshaped to `(broker_statement, original_symbol, vest_date, quantity, converter)`; `lot_cost_basis_eur` (`:898`) passes `lot.original_symbol, lot.conclusion_time.date, lot.quantity`.
- `process_broker_statement` (`:184-229`), after `:199`: `collect_buys`, sort `report.trades` by `(symbol, date, settle_date)`, `collect_open_lots` (classify closure + grant-cost closure), `collect_securities` with `extra` = symbols of `stock_grants`, `wash_sale_reintegrations`, `deferred_losses_next`.

### B3. `src/tax_statement/spain/html/{mod.rs, format.rs, summary.rs, detail.rs, tests.rs}`

`mod.rs`: `pub struct HtmlReport; impl HtmlReport { pub fn write<W: Write>(statement: &SpanishTaxStatement, meta: &ReportMeta, writer: &mut W) -> EmptyResult }`; `Document { lang: "es", title: format!("Informe fiscal informativo {year}"), toc_id: "indice", toc_title: "Índice" }`; title block: eyebrow `Ejercicio {year} · Base del ahorro · {regime.description()}`, chips (broker, `Cartera:`, `Cuenta:`, `Periodo: dd/mm/yyyy – dd/mm/yyyy`, `Régimen:`), KPIs (`Base liquidable del ahorro`, `Cuota íntegra del ahorro`, `Deducción por doble imposición`, `Cuota líquida del ahorro`), Spanish disclaimer mirroring `germany/html/mod.rs:231-293` (names the regime's form, FIFO, ECB rate, "no constituye asesoramiento fiscal").

`format.rs`: `date` (`%d/%m/%Y`), `iso_date` (YAML block only), `datetime` (`%d/%m/%Y %H:%M`), `coefficient` (3 dp half away from zero, punctuated, same rounding as `csv_formatter.rs:1513`), re-export of the shared numerics, and label tables: `side_label` (Compra/Venta), `lot_source_label` (Compra / Adjudicación (vesting) / Operación societaria), `booking_kind_label` (Dividendos y distribuciones / Retenciones en origen / Intereses / Comisiones y gastos / Otros ingresos), `asset_class_label` (Acciones / Fondos e IIC), `treatment_label` (Ganancia/pérdida patrimonial (saldo propio) / Revisión manual (saldo prestado)), `activity_label` (BUY Compra, SELL Venta, DIV Dividendo, PIL Dividendo sustitutivo, WHTAX/FRTAX Retención en origen, FOREX Cambio de divisa, CINT Intereses cobrados, DINT/BINT Intereses pagados, OFEE/FEE Comisión, DEP Ingreso, WITH Retirada, ADJ Ajuste, OPENING Saldo inicial), `country_name` (same ISO list as the German one, Spanish names, `""` → Desconocido), `group_label` (RCM → Rendimientos del capital mobiliario, GyP → Ganancias y pérdidas patrimoniales).

Sections (`SECTIONS: &[Section<SpanishTaxStatement>]`):

| # | id / title | data | content | regime differences |
|---|---|---|---|---|
| 1 | `formularios` / Resumen para los formularios | `forms::form_mapping` | intro with `title` + joined `header_lines`; table Formulario · Casilla · Concepto · Importe (EUR); `footer_lines`; warn note about foreign withholding and `withholding_casilla` when `total_foreign_withholding ≠ 0` | title/boxes per regime; Gipuzkoa Anexo 3 ≤ 2024; Navarra conditional boxes + H3 note |
| 2 | `calculo` / Cálculo del impuesto | totals, own-group/cross/prior-cross fields, `rcm/gyp_taxable`, `savings_base`, `scale.brackets()`, `savings_quota`, `average_savings_rate`, credit fields, `net_tax_due` | 2-col tables RCM, GyP, Compensación (labels from `forms::compensation_labels`), Base y escala (bracket table Tramo desde · Tipo · Base en el tramo · Cuota, slices summing to `savings_quota`), Crédito; info rows `total_informational_fees`, `total_fx_borrowed_review` | exemption row only if `dividend_exemption_limit() > 0`; fee-cap rows only if `custody_fee_cap.is_some()`; small-disposals row only if `> 0` or `small_disposals_unmeasurable`; FX label from `forms::fx_summary_label`; cross rows hidden when `cross_offset() == CrossOffset::None` |
| 3 | `actividad` / Resumen por actividad y categoría | `capital_gains` (integrable_amount, deferred_loss), `dividends`, `interest`, `fees`, `fx_gains`, `report.securities` | Categoría · Actividad · Ganancias · Pérdidas · Neto · Diferido · Grupo (reuse the `ActivityRow` shape) | none |
| 4 | `valores` / Ganancias y pérdidas por valor | `capital_gains`, `wash_sale_reintegrations`, `dividends`, `report.security_names()` | grouped by `AssetClass`: Valor · ISIN · Dividendos brutos · Retenido · Ganancias · Pérdidas · Diferido · Neto integrable | none |
| 5 | `movimientos` / Movimientos de efectivo | `report.bookings` | Divisa · Fecha · Categoría · ISIN · Valor · Concepto · Importe · Tipo BCE · Importe EUR; totals reconcile with `total_dividend_income`, `-total_foreign_withholding`, `total_interest_income − total_paid_interest`, fees | none |
| 6 | `retenciones` / Retenciones en origen | `report.withholding`, `treaty_rate()`, `total_foreign_tax_credit` | per country: Fecha · Valor · ISIN · Divisa · Bruto · Bruto EUR · Retenido · Retenido EUR · Tipo · Crédito con límite del convenio (15 %); note that the year credit is `total_foreign_tax_credit` | Gipuzkoa: exempt dividends carry no credit |
| 7 | `transmisiones` / Ganancias y pérdidas patrimoniales (FIFO) | zip(`report.sales`, `capital_gains`), zip(lots, `entry.lots`) | per security: sale row (Fecha · ID · Cantidad · Precio · Bruto · Comisión · Tipo BCE · Ingreso EUR · Coste EUR · [Coeficiente · Coste actualizado] · Resultado · Diferido · Integrable), lot rows (Fecha adq. · ID · Origen · Cantidad · Precio · Coste EUR · [Coef. · Coste act.] · Ingreso EUR · Resultado · Días); totals reconcile with `total_capital_gains`, `total_deferred_loss` | coefficient columns only for Gipuzkoa; abatement flag on lots with `open_date < 1994-12-31` (Navarra prose) |
| 8 | `valores-homogeneos` / Valores homogéneos | deferred entries, `wash_sale_reintegrations`, window gaps, boundary reviews (`.message()`), venue reviews, `deferred_losses_next`, `wash_sale_unpriced_years` | four tables + warn notes; `false` when all empty | article cited per regime |
| 9 | `operaciones` / Operaciones con valores | `report.trades` | as German trades section | none |
| 10 | `divisas` / Diferencias de cambio | `report.fx_rows`, `total_fx_result`, `total_fx_borrowed_review` | per-currency ledger (Fecha · ID · Actividad · Unidades · Tipo · Importe EUR · Adq. · Tipo adq. · Valor adq. · Resultado · Días · Saldo · Tratamiento), totals by treatment | Navarra: `forms::fx_summary_label` as a note |
| 11 | `posiciones-abiertas` / Posiciones abiertas a 31/12 | `report.open_lots`, `open_lots_as_of` | as German | none |
| 12 | `compensacion` / Compensación y saldos pendientes | ledgers prior/`applied.used_by_year`/next, `rcm/gyp_expired`, `deferred_losses_next` | table Grupo · Año de origen · Saldo inicial · Aplicado · Pendiente · Caduca en (origin + `CARRYFORWARD_YEARS`); then `<pre>` with the exact YAML the console prints (`src/tax_statement/mod.rs:249-289`, ISO dates) | none |
| 13 | `relacion-valores` / Relación de valores | `report.securities` | Símbolo · ISIN · Nombre · País · Divisa · Categoría | none |
| 14 | `avisos` / Avisos y observaciones | period coverage; the five `*_message()`; `total_dividend_exemption` caveat with the list of Fund payers; `fees[].review`; `total_fx_borrowed_review`; `short_positions`; `stock_grants` table (Valor EUR or "—"); `corporate_actions`; per-entry notes; methodology bullets; pointer to the open-interpretations register | English calculation messages shown verbatim under "Avisos del cálculo (texto literal)" |

### B4. Output arm, docs, tests, verification

- `src/tax_statement/mod.rs`: replace the PR-A `Err!` arm with the `ReportMeta` construction (as in the German block) and `spain::HtmlReport::write(&statement, &meta, writer)`. Console summary unchanged.
- Docs: `docs/spain-taxes.md` new `## HTML report (printable, A4 landscape)` between "CSV output" (`:826`) and "Sell simulation" (`:834`), mirroring `docs/germany-taxes.md:373-428` (usage, the 14 sections, notation, Chrome/Playwright/Gotenberg recipes, caveats: English calculation messages, `taxes.etf_classification` drives "Fondos e IIC", treaty 15 %, open lots as-of, no opening FX balance for Spain); update `## Usage` (`:79-95`); README `:14` and `:84-88`; `.gitignore` add `/spanish-tax-*.csv` and `/spanish-tax-*.html`.
- `spain/html/tests.rs` (harness `super::super::tests::{run_pipeline, run_pipeline_with_config}` made `pub(super)`): skeleton + section order per regime (`fifo` × 3), Spanish notation, escaping, statement with only a prior ledger renders `formularios`/`calculo`/`compensacion`/`avisos`, worksheet reconciliation (`report.sales.len() == capital_gains.len()`, lot counts equal, Σ `lot.cost_eur == entry.cost_eur`, Σ `lot.gain_loss_eur == entry.fiscal_gain_loss`), withholding rows ↔ `dividends[i].withheld_eur`/`treaty_capped_credit` (`income`), FX rows ↔ `fx_gains`/`fx_borrowed_review` totals (`fx_gain`, `fx_borrowed` with the revaluing converter), coefficient column present for Gipuzkoa / absent for Común, Navarra conditional F-93 boxes (`cross_offset` + `rcm_saldo_2000`), every `FormBox.casilla` appears in the HTML.
- Golden HTML: extend `spain/golden_tests.rs` with an HTML `#[rstest]` (`fifo_gipuzkoa_2026.html`, `income_comun_2026.html`, `cross_offset_navarra_2026_saldo.html`, `wash_sale_multi_lot_gipuzkoa_2026.html`), `fixed_meta()` with constant `generated_at`, `assert_golden(name, ext, emitted)`, `CORPUS` entries with extension, orphan check accepting both extensions; regenerate with `UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests`.
- Verification: `cargo test --lib spain`, `cargo test --lib germany` (PR-A goldens must still pass), `cargo test --lib`; clippy warnings only in touched files; `rustfmt --edition 2024` on touched files except the rustfmt-ignored `src/tax_statement/mod.rs` and `src/taxes/mod.rs`. Real run per regime: copy `~/.investments/config.yaml` to a scratch dir, set `taxes.jurisdiction: spain` and `taxes.spain.regime` (gipuzkoa, comun, navarra), keep `db_path`, then `cargo run -- -c <scratch-dir> tax-statement <portfolio> 2025 <scratch>/spanish-tax-2025-<regime>.html` (scales exist for 2024-2026 only), render with `"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --print-to-pdf=<pdf> --no-pdf-header-footer <html>` and read the PDF pages to check breaks and column widths. The `ibkr-miren` statements folder also holds the tool's own CSV; run from a scratch statements dir holding only `BubbleTaxReport.xml`.

## Risks and open points

1. Gipuzkoa Anexo 3 (≤ 2024) has no golden; the `forms.rs` unit test is the only pin. Add a 2024 fixture if one exists with a 2024 sale.
2. F-93 is verified for 2025 only; header warning lines for other years travel verbatim through `FormMapping.header_lines`.
3. Treaty rate hard-coded 15 % (`processor.rs:71`): the withholding section names it from `treaty_rate()` and repeats the UK/REIT caveat.
4. Spain passes `&[]` opening lots to the FX FIFO: a truncated history shows a borrowed balance; the section prose must say the ledger starts at zero.
5. Wash-sale carry-out snapshots before the first post-year-end sale: label the carry-out "a 31/12" and mention the two-month window workflow.
6. `StockGrantEntry.value_eur` is optional: render "—", never compute.
7. Only the YAML `<pre>` block may use ISO dates.
8. Keep CSV goldens byte-identical while extracting `forms.rs`: preserve per-line strings and ordering; run the goldens after each move.
9. Report pushes also run inside `simulate-sell` (`sell_simulation.rs:401` calls `spain::compute_tax_year`); worksheets are built for filing-year sales only, bookings/open lots still look up ECB rates. Same cost profile as Germany; gate behind a flag later if it hurts.
10. Report names prefer configured `instrument_names`, then the broker description, then the symbol; the CSV keeps `get_name()`. Document as for Germany.
11. `AssetClass` reads `taxes.etf_classification`, documented today as a German key; document it as optional for Spain.
12. English calculation messages inside a Spanish report (decision 6). Follow-up: Spanish variants as statement methods.
13. `rustfmt.toml` / `rust-toolchain.toml` are untracked; do not commit them.

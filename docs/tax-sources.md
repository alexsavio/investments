# Tax Sources

Every number this tool prints traces to a published document. Those documents are re-issued each
year and the tool has no way to notice: a form gets renumbered, a scale is amended, a coefficient
table is published, and the code goes on emitting last year's answer with full confidence.

This file is the manifest of what the tool depends on, when each source changes, how to fetch it
again, and what happens when it has not been refreshed. The reasoning behind each reading stays in
[`germany-taxes.md`](./germany-taxes.md) and [`spain-taxes.md`](./spain-taxes.md); this is the
maintenance view.

## What breaks first

The tool fails loudly where it can and warns where it cannot. This is the order things stop working
as filing years advance past the shipped data.

| Filing year | What happens |
|---|---|
| Spain 2027 | **Hard error.** `SavingsScale::for_year` ships 2024–2026 only and refuses to extrapolate: a savings scale is set by statute each year, and guessing one files real money at an invented rate. Gipuzkoa also loses its actualization table, so disposals cannot be priced. |
| Germany 2026 | **Hard error** on any fund held at year end: `german_basiszins` ships 2023–2025. Everything else still computes. |
| Either, any year past the last published form | **Warning, not an error.** The form block prints the last published numbering and says so. A form for a filing year is published during the following year's campaign, so this state is normal for most of the year. |

Both hard errors name the config key that overrides them (`taxes.spain.coefficients.<year>`,
`taxes.basiszins.<year>`), so a new publication can be used before the next release.

## Germany

| What the tool takes | Source | Published | Lands in |
|---|---|---|---|
| Anlage KAP line numbers (19, 20, 22, 23, 41) | *Anlage KAP* of the filing year, Bundesfinanzverwaltung | ~September–October of the filing year | `src/tax_statement/germany/statement.rs` |
| Anlage KAP-INV line numbers (4/5/8, 9/10/13, 14/17/26) | *Anlage KAP-INV* of the filing year | ~September of the filing year | `src/tax_statement/germany/mod.rs::kap_inv_zeilen` |
| Basiszins for the Vorabpauschale | BMF-Schreiben | **each January, for the year just ended** | `src/taxes/mod.rs::german_basiszins` |
| Sparer-Pauschbetrag (€1,000 since 2023) | §20 Abs. 9 EStG | on amendment | `src/taxes/mod.rs::german_sparer_pauschbetrag` |
| §23 Freigrenze (€1,000 since 2024) | §23 Abs. 3 EStG | on amendment | `src/tax_statement/germany/statement.rs::Section23::freigrenze` |
| Abgeltungsteuer, Soli, Kirchensteuer rates | §32d EStG, SolZG, Landeskirchensteuergesetze | on amendment | `src/taxes/germany/rates.rs` |

**Fetching the forms.** `formulare-bfinv.de` is a session-gated web application, so the PDFs cannot
be linked directly. The mapping was verified against mirrors that carry the official print id and
the ELSTER barcode number verbatim — check that the print id in the PDF matches the one quoted in
the code (`2025AnlKAP051NET`, `2025AnlKAP-INV361NET`) before trusting a mirror.

**What to watch.** Both shipped years carry identical numbering for every line the tool fills. The
2025 Anlage KAP voids the Termingeschäfte lines — 21, 24 and 25 print "frei" — **without**
renumbering 22, 23 or 41. A future form that renumbers instead of voiding is the failure mode to
look for.

## Spain

| What the tool takes | Source | Published | Lands in |
|---|---|---|---|
| Gipuzkoa savings scale | NF 3/2014 art. 76.1, as amended (NF 1/2025 for 2026) | on amendment | `src/taxes/spain/scale.rs` |
| Común savings scale | LIRPF arts. 66/76, as amended | with each PGE or amending law | `src/taxes/spain/scale.rs` |
| Navarra savings scale | TRLFIRPF art. 60 | on amendment | `src/taxes/spain/scale.rs` |
| Gipuzkoa actualization coefficients | Decreto Foral, keyed by **disposal** year | **late December, for the following year**, in the BOG | `src/taxes/spain/coefficients.rs` |
| Gipuzkoa €1,500 dividend exemption | NF 3/2014 art. 9.24 | on amendment | `src/tax_statement/spain/processor.rs` |
| Navarra €3,000 small-disposals limit | TRLFIRPF art. 39.5.d | on amendment | `src/taxes/spain/exemption.rs` |
| Four-year carry-forward window | LIRPF art. 49.1 and the foral equivalents | on amendment | `src/taxes/spain/carryforward.rs` |
| Modelo 109 casillas (Gipuzkoa) | see below — **no published form exists** | — | `src/tax_statement/spain/forms.rs` |
| Modelo 100 casillas (Común) | Orden HAC of the campaign, Anexo I | spring of the year **after** the filing year | `src/tax_statement/spain/forms.rs` |
| Modelo F-93 casillas (Navarra) | Orden Foral of the campaign, Anexo I, in the BON | ~April of the year after the filing year | `src/tax_statement/spain/forms.rs` |

### Fetching each Spanish source

**Gipuzkoa coefficients and campaign orders (BOG).** The Boletín Oficial de Gipuzkoa serves PDFs at
`egoitza.gipuzkoa.eus/gao-bog/castell/bog/<YYYY>/<MM>/<DD>/c<NNNNNNN>.pdf`. The campaign's Orden
Foral is linked from the year's Modelo 109 page,
`gipuzkoa.eus/es/web/ogasuna/impuestos/modelo/109/<ejercicio>`. The same page carries *Principales
aspectos IRPF <year>*, a one-page summary that states the year's savings scale — the quickest check
that `scale.rs` is still right.

**Navarra F-93 (BON).** The Orden Foral is at `bon.navarra.es/es/anuncio/-/texto/<year>/<bon-no>/<n>`
and links its Anexo I. That link points at `DescargarFichero/default.aspx`, which is a JavaScript
shim, not the file: replace `default.aspx` with **`DownloadFile.aspx`** in the URL to get the PDF.

**Reading a form PDF.** `pdftotext -layout` is what makes a form legible — the box numbers sit in a
right-hand column and only `-layout` keeps them aligned with their labels. Without it the numbers
arrive detached from what they name, which is how a mapping gets read wrong.

**Modelo 100 (AEAT).** The AEAT's box numbers currently in the code come from the **Orden
HAC/277/2026 consultation draft**, because no enacted order exists for the filing year yet. The
per-box help pages under `sede.agenciatributaria.gob.es` are the practical cross-check.

## Verification status

Honest state of each form mapping, because the three differ and the difference matters.

| Mapping | Status | Last checked |
|---|---|---|
| Anlage KAP / KAP-INV | **Verified** against the official forms for 2024 and 2025 | 2026-08-11 |
| Modelo F-93 (Navarra) | **Verified** — all 23 casillas read off Anexo I of Orden Foral 24/2026 (BON nº 66, 07-04-2026), including the form's own arithmetic (`8809 = 8808 − 809 − 810 − 8815`, `8840 = 8810 − 8825 − 8835 − 8805`, `8841 = 8809 + 8840`) | 2026-09-09 |
| Modelo 100 (Común) | **Draft source.** Numbers from a consultation draft, not an enacted order. Casillas 0588 and 0597 cross-checked against AEAT help pages | 2026-09-09 |
| Modelo 109 (Gipuzkoa) | **Unverifiable by construction.** See below | 2026-09-09 |

### Why Modelo 109 cannot be verified

Gipuzkoa publishes no blank numbered form. The campaign's Orden Foral (112/2026 for ejercicio 2025)
attaches only Anexo 6 and Anexo II, and says the return is *"el que se identifique en la plataforma
Zergabidea como modelo 109 de dicho periodo impositivo"* — the form is whatever the Zergabidea
platform renders. The numbers in the code were read off "Propuesta de autoliquidación" specimens.

This is not a gap in the research; it is how Gipuzkoa publishes. The report and the CSV both say so
on every page that prints those numbers, and tell the filer to check each casilla against their own
proposal. **Do not** replace that warning with a claim of verification.

## Refreshing a source

1. Fetch the new document and record **what** it is, **when** it was retrieved, and the identifier
   that pins it (print id, BON number, Decreto Foral number). A URL alone rots.
2. Update the constant or table, and the citation beside it. The citation lives next to the value,
   not only here — a value whose source you have to go looking for is a value nobody re-checks.
3. Update the **Last checked** column above, and the retrieval date in the jurisdiction's own doc.
4. Regenerate the goldens and **read the diff line by line**:

   ```bash
   export CARGO_TARGET_DIR=~/.cache/cargo-target
   UPDATE_GOLDEN=1 cargo test --lib germany::html::tests
   UPDATE_GOLDEN=1 cargo test --lib spain::golden_tests
   git diff src/tax_statement/*/testdata/golden/
   ```

   That diff is the whole point of the corpora. A refresh that changes nothing is a refresh worth
   doubting; a refresh that changes more than you expected is the corpus doing its job.
5. If the new source contradicts a reading rather than a number, the change belongs in the
   jurisdiction's "Open interpretations" register, not only here.

## What is not sourced from a document

Three values are the tool's own choice, not a reading of a published table, and each is warned about
where it is used:

- **The 15% treaty rate** for the Spanish double-taxation credit. It is the portfolio-dividend rate
  in most of Spain's treaty network, but the UK caps portfolio dividends at 10% and a US REIT paid
  to a >10% holder gets no treaty benefit at all. There is no per-country table.
- **Security names** in the reports prefer the configured `instrument_names`, then the broker's
  description, then the symbol.
- **Asset classification** reads `taxes.etf_classification` by ISIN. It drives the German
  Teilfreistellung — a real tax figure — but in Spain only groups the report.

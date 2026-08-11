# German Tax Feature — Open-Items Resolution Round

**Date**: 2026-08-11
**Branch**: `002-spain-tax` (the branch carrying both jurisdictions; German feature only)
**Inputs**: the two `TODO(verify)` markers left open by the T6 Anlage KAP work
(`statement.rs`, `csv_formatter.rs`), plus a sweep of every German runtime warning and documented
limitation. Outcome: **both markers closed against the official forms for both filing years**, no
computed figure changed, and the German docs gain the durable "Open interpretations" register the
Spanish docs already carry.

## Research verdicts (sources retrieved 2026-08-11)

### V1 — Anlage KAP line numbers: CONFIRMED for 2024 and 2025, no renumbering

Official forms of the Bundesfinanzverwaltung: *Anlage KAP 2024* (print id `2024AnlKAP051NET`,
"- September 2024 -") and *Anlage KAP 2025* (`2025AnlKAP051NET`, "- Oktober 2025 -"). All five lines
the tool emits carry the same number in both years, with the labels verbatim:

| Zeile | Label (both years) | Emitted as |
|---|---|---|
| 19 | Ausländische Kapitalerträge | `KAP_ZEILE_19` |
| 20 | In den Zeilen 18 und 19 enthaltene Gewinne aus Aktienveräußerungen i. S. d. § 20 Abs. 2 Satz 1 Nr. 1 EStG | `KAP_ZEILE_20` |
| 22 | In den Zeilen 18 und 19 enthaltene Verluste ohne Verluste aus der Veräußerung von Aktien | `KAP_ZEILE_22` |
| 23 | In den Zeilen 18 und 19 enthaltene Verluste aus der Veräußerung von Aktien i. S. d. § 20 Abs. 2 Satz 1 Nr. 1 EStG | `KAP_ZEILE_23` |
| 41 | Anrechenbare noch nicht angerechnete ausländische Steuern | `KAP_ZEILE_41` |

The only 2024→2025 change in this block is that the Termingeschäfte lines are voided — 2025 prints
21, 24 and 25 as "frei" — **without** renumbering 22, 23 or 41. So no per-year mapping is needed.

### V2 — Anlage KAP-INV line numbers: CONFIRMED for 2024 and 2025; the inferred line 26 is right

Official forms *Anlage KAP-INV 2024* (`2024AnlKAP-INV361NET`, "- September 2024 -") and
*Anlage KAP-INV 2025* (`2025AnlKAP-INV361NET`, "- September 2025 -"), identical in both years:

| Fund type | Ausschüttungen § 2 Abs. 11 InvStG | Vorabpauschalen § 18 InvStG | Veräußerung |
|---|---|---|---|
| Aktienfonds (equity) | 4 | 9 | 14 |
| Mischfonds (mixed) | 5 | 10 | 17 |
| Immobilienfonds | 6 | 11 | 20 |
| Auslands-Immobilienfonds | 7 | 12 | 23 |
| sonstige Investmentfonds (bond/other) | **8** | **13** | **26** |

The tool's `sonstige` Veräußerung line — previously inferred from the three-lines-per-fund-type
layout — is **26**, confirmed. Line 27 is the Altbestand line contained in it, exactly as the marker
guessed. Verified twice from independently hosted copies of the 2024 form.

### Sources

The Formular-Management-System (`formulare-bfinv.de`) is a session-gated web application whose PDFs
are not directly fetchable, so the forms were read from a mirror carrying the official print ids,
print dates and ELSTER barcode numbers verbatim:

- KAP 2024 — `https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2024/Anlage_KAP_steuern.de_01.pdf`
- KAP 2025 — `https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2025/Anlage_KAP_2025_steuern-de.pdf`
- KAP-INV 2024 — `https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2024/Anlage_KAP_INV_Steuern.de_01.pdf`
  (cross-checked against `https://geldanlage-schweiz.de/assets/Anlage-KAP-INV_2024.pdf`)
- KAP-INV 2025 — `https://www.steuern.de/fileadmin/user_upload/Steuerformulare_2025/Anlage_KAP_INV_2025_steuern-de.pdf`

Anlage SO needed no verification: the §23 block names no line numbers, only the section and the
year's Freigrenze.

## What changed

- **`fix(germany-tax)` (`bf1b6bbb`)** — both `TODO(verify)` markers replaced by citations naming the
  form, print id, source URL and retrieval date. The CSV banners over the KAP and KAP-INV blocks now
  say the Zeilen are verified for 2024 and 2025 instead of asking the filer to re-check them. Two new
  formatter tests pin the mappings (`kap_inv_zeilen_match_the_official_form`,
  `kap_summary_rows_carry_the_official_zeilen`). **No computed figure moved** — every number was
  already correct, so no test needed a failing-first pass.
- **`docs(germany-tax)` (this round)** — `docs/germany-taxes.md` gains an "Open interpretations"
  register of ten entries mirroring the Spanish one, each with its authority, the tool's reading, the
  exact text the tool emits, and what the filer should do. The two stock-grant €0-cost-basis warnings
  were reworded to name the consequence (the whole disposal proceeds taxed as gain) and point at the
  register. No computation was touched.

## What remains open

Catalogued in the register in `docs/germany-taxes.md`, each with the warning that surfaces it:
multi-account statements (rejected, not interpreted), short positions (report-only), the three
Vorabpauschale inputs (Basiszins, year-boundary NAVs, month of acquisition), the uniform 15% treaty
cap on the foreign-tax credit and the zero credit on fund distributions, the FX ledger requirement
and the declared opening balance, the §23 Anlage SO path with its Freigrenze cliff and borrowed-
balance review, Altbestand conditions the statement cannot show, the grant €0 cost-basis fallback,
and the settlement-date FX convention on trades.

Form line numbers are open only for a filing year whose form is not published yet; the CSV says so
above the rows.

**No `TODO(verify)` marker remains anywhere in `src/`.** The remaining occurrences of that string in
the tree are prose inside the spec documents describing earlier rounds, not live markers.

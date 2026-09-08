//! Inline stylesheet: screen preview plus A4-landscape print rules for Chrome's print engine.
//!
//! The look is a calm slate palette with a teal accent: light table headers, hairline rows,
//! accent-bar section headings and key-figure cards on the title page. Backgrounds are forced to
//! print (`print-color-adjust`) so the PDF matches the screen.

pub(crate) const CSS: &str = r#"
@page { size: A4 landscape; margin: 12mm; }

:root {
  --ink: #0f172a;
  --ink-2: #334155;
  --muted: #64748b;
  --line: #e2e8f0;
  --line-2: #cbd5e1;
  --surface: #ffffff;
  --surface-2: #f8fafc;
  --surface-3: #f1f5f9;
  --primary: #14305c;
  --accent: #0f766e;
  --accent-soft: #e6f4f2;
  --neg: #c2410c;
  --danger-line: #dc2626;
  --danger-bg: #fef2f2;
  --warn-line: #f59e0b;
  --warn-bg: #fffbeb;
  --info-line: #0ea5e9;
  --info-bg: #f0f9ff;
}

* {
  box-sizing: border-box;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}

body {
  margin: 0;
  color: var(--ink);
  background: var(--surface);
  font: 9.5pt/1.45 -apple-system, BlinkMacSystemFont, "SF Pro Text", Inter, "Segoe UI", Roboto,
    "Helvetica Neue", Arial, sans-serif;
  font-feature-settings: "tnum" 1, "cv11" 1;
}

@media screen {
  body { max-width: 277mm; margin: 0 auto; padding: 14mm 12mm; }
  section { border-top: 1px solid var(--line); margin-top: 14mm; padding-top: 8mm; }
}

h1 {
  color: var(--primary);
  font-size: 26pt;
  font-weight: 800;
  letter-spacing: -0.02em;
  line-height: 1.1;
  margin: 0 0 8pt;
}
h2 {
  color: var(--primary);
  font-size: 16pt;
  font-weight: 700;
  letter-spacing: -0.01em;
  line-height: 1.2;
  margin: 0 0 8pt;
  padding-left: 10pt;
  border-left: 4px solid var(--accent);
}
h3 { color: var(--ink); font-size: 11.5pt; font-weight: 650; margin: 14pt 0 4pt; }
h4 { color: var(--ink); font-size: 10.5pt; font-weight: 600; margin: 12pt 0 2pt; }
p { margin: 0 0 6pt; color: var(--ink-2); }
ul, ol { margin: 0 0 6pt; padding-left: 18pt; color: var(--ink-2); }
li { margin: 0 0 2pt; }
li::marker { color: var(--accent); }
b { color: var(--ink); }
code { font-size: 0.9em; background: var(--surface-3); padding: 0 3pt; border-radius: 3px; }

.eyebrow {
  color: var(--accent);
  font-size: 8.5pt;
  font-weight: 700;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  margin: 0 0 4pt;
}
.chips { display: flex; flex-wrap: wrap; gap: 6pt; list-style: none; margin: 0 0 12pt; padding: 0; }
.chip {
  color: var(--ink-2);
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: 999px;
  font-size: 8.5pt;
  padding: 2pt 9pt;
  margin: 0;
}
.kpis { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8pt; margin: 0 0 12pt; }
.kpi {
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 9pt 11pt;
  background: linear-gradient(180deg, var(--surface) 0%, var(--surface-2) 100%);
}
.kpi .label {
  color: var(--muted);
  font-size: 7.5pt;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  margin-bottom: 3pt;
}
.kpi .value { color: var(--primary); font-size: 15pt; font-weight: 700; font-variant-numeric: tabular-nums; }
.kpi .value.neg { color: var(--neg); }
.kpi .hint { color: var(--muted); font-size: 7.5pt; margin-top: 2pt; }
.meta { color: var(--muted); font-size: 8.5pt; }
.security-id { color: var(--muted); font-size: 8.5pt; margin: 0 0 4pt; }
.fine { color: var(--muted); font-size: 8pt; }

.disclaimer {
  border-left: 4px solid var(--danger-line);
  background: var(--danger-bg);
  border-radius: 0 8px 8px 0;
  padding: 8pt 12pt;
  margin: 8pt 0 10pt;
}
.disclaimer h3 { color: #991b1b; font-size: 10.5pt; margin: 0 0 4pt; }
.disclaimer p { font-size: 8.5pt; }

.note { border: 1px solid var(--line); border-left-width: 4px; border-radius: 0 8px 8px 0; padding: 7pt 10pt; margin: 6pt 0; color: var(--ink-2); }
.note.info { background: var(--info-bg); border-color: #bae6fd; border-left-color: var(--info-line); }
.note.warn { background: var(--warn-bg); border-color: #fde68a; border-left-color: var(--warn-line); }

nav .toc { columns: 2; column-gap: 24pt; padding-left: 20pt; }
nav .toc li { break-inside: avoid; margin: 0 0 3pt; }
nav .toc li::marker { color: var(--accent); font-weight: 700; }
nav .toc a { color: var(--ink); text-decoration: none; }

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 8pt;
  margin: 4pt 0 12pt;
}
thead { display: table-header-group; }
thead th {
  background: var(--surface-3);
  color: var(--ink-2);
  font-size: 7.25pt;
  font-weight: 650;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  text-align: left;
  padding: 4pt 5pt;
  vertical-align: bottom;
  border-bottom: 1.5px solid var(--primary);
}
tbody td { padding: 3pt 5pt; vertical-align: top; border-bottom: 1px solid var(--line); color: var(--ink); }
tbody tr.data:nth-child(even) td { background: #fbfcfe; }
tbody tr.lot td { color: var(--muted); font-size: 7.5pt; }
tbody tr.subtotal td { font-weight: 600; background: var(--surface-2); border-top: 1px solid var(--line-2); }
tbody tr.total td { font-weight: 700; background: var(--accent-soft); border-top: 2px solid var(--primary); border-bottom: none; }
.num { text-align: right; white-space: nowrap; font-variant-numeric: tabular-nums; }
td.neg { color: var(--neg); }

section { break-before: page; page-break-before: always; }
tr, .note, .disclaimer, .kpi { break-inside: avoid; page-break-inside: avoid; }
h2, h3, h4 { break-after: avoid; page-break-after: avoid; }
table { orphans: 3; widows: 3; }

@media print {
  a { color: inherit; }
  .no-print { display: none; }
}
"#;

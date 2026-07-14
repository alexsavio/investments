# Germany Tax Statement Generation

## Overview

Investments can generate German tax statements (Steuererklärung) for capital gains, dividends, and interest income from foreign brokerage accounts. The output is a CSV file that can be used for self-declaration or imported into tax preparation software.

## Configuration

Add the following to your `config.yaml`:

```yaml
taxes:
  # Set jurisdiction to germany for German tax calculations
  jurisdiction: germany

  # Optional: Church tax rate (Kirchensteuer)
  # Set to your applicable rate: 8 (Bavaria, Baden-Württemberg) or 9 (other states)
  # Accepted forms: 0, 8, 9 (percent) or 0.08 / 0.09 (fraction). Default: 0 (no church tax)
  church_tax_rate: 9

  # Optional: Festgestellter Verlustvortrag (loss carryforward) as of Dec 31 of the prior year.
  # §20(6) EStG keeps two separate pots: share-sale (Aktien) losses offset only future share-sale
  # gains; all other losses form the general pot. Each is a single EUR amount.
  loss_carryforward_stock: 1500.00   # Verlustverrechnungstopf Aktien
  loss_carryforward_other: 2000.00   # general pot (funds, FX, dividends, interest)

  # Optional: Sparer-Pauschbetrag (saver's allowance). Defaults to 1000 (2023+) / 801 (before).
  # Set to 0 if the allowance is already used via a Freistellungsauftrag at a German bank.
  sparer_pauschbetrag: 1000

  # Optional: ETF classification for Teilfreistellung (partial tax exemption)
  # Map ISIN to classification: equity (30% exempt), mixed (15% exempt), bond (0% exempt)
  etf_classification:
    IE00BK5BQT80: equity    # Vanguard FTSE All-World UCITS ETF (VWCE.DE)
    IE00B4L5Y983: equity    # iShares Core MSCI World UCITS ETF
    IE00BDBRDM35: mixed     # iShares Core Global Aggregate Bond
    LU0274211480: bond      # Xtrackers II Eurozone Government Bond
```

### Portfolio Configuration

Your portfolio configuration remains the same as for other jurisdictions:

```yaml
portfolios:
  - name: ib
    broker: interactive-brokers
    statements: ~/Brokerage/Interactive Brokers/Statements
    currency: USD
    # ... other settings
```

## Usage

Generate a German tax statement with:

```bash
investments tax-statement <portfolio_name> <year> <output.csv>
```

Example:

```bash
investments tax-statement ib 2024 german-tax-2024.csv
```

The command will:

1. Read all broker statements for the specified portfolio
2. Calculate capital gains using FIFO method
3. Apply German tax rates (Abgeltungssteuer + Solidaritätszuschlag + Kirchensteuer)
4. Account for foreign tax credits
5. Generate a detailed CSV report

## German Tax Rates

### Abgeltungssteuer (Flat Tax)

Since 2009, Germany applies a flat tax on investment income:

| Tax Component | Rate | Applied To |
|---------------|------|------------|
| Abgeltungssteuer | 25% | Capital gains, dividends, interest |
| Solidaritätszuschlag | 5.5% of Abgeltungssteuer | All taxable income |
| Kirchensteuer | 8-9% of Abgeltungssteuer | If applicable |

### Effective Tax Rates

Under §32d(1) EStG, church tax is deductible **inside** the flat-rate formula, so it does not
simply stack on top of 25%. For taxable capital income `e` and church-tax fraction `k` (0, 0.08,
or 0.09), with creditable foreign tax `q`:

- `Abgeltungsteuer = max(0, e − 4·q) / (4 + k)`
- `Solidaritätszuschlag = 5.5% × Abgeltungsteuer`
- `Kirchensteuer = k × Abgeltungsteuer`

| Church tax | Total tax on €1,000 (q = 0) | Effective rate |
|------------|-----------------------------|----------------|
| None (k = 0) | 263.75 | 26.375% |
| 8% (k = 0.08) | 278.19 | ≈27.82% |
| 9% (k = 0.09) | 279.95 | ≈27.99% |

## Teilfreistellung (Partial Exemption)

For ETFs, partial tax exemptions apply based on the fund composition:

| Fund Type | Equity Ratio | Exemption Rate | Taxable Portion |
|-----------|--------------|----------------|-----------------|
| Equity ETF | >51% | 30% | 70% |
| Mixed ETF | 25-51% | 15% | 85% |
| Bond ETF | <25% | 0% | 100% |
| Regular Stocks | N/A | 0% | 100% |

**Note:** Configure ETF classifications in your config to apply correct exemption rates.

## Anlage KAP vs Anlage KAP-INV

Foreign capital income is declared on two different forms depending on whether it comes from an
investment fund:

- **Anlage KAP** — non-fund income: direct-share (Aktien) sale gains/losses, dividends from
  non-funds, interest, and FX results. The statement fills:
  - `Zeile 19`: net foreign capital income (all positives − contained losses).
  - `Zeile 20`: share-sale gains contained in Zeile 19 (needed to operate the §20(6) stock pot).
  - `Zeile 22`: contained losses **excluding** share-sale losses (FX / other §20 losses).
  - `Zeile 23`: contained share-sale losses.
  - `Zeile 41`: creditable foreign withholding tax.
- **Anlage KAP-INV** — investment-fund income (any Teilfreistellung classification, **including bond
  funds**): reported per fund type (Aktienfonds / Mischfonds / sonstige) as **gross** distributions,
  a net sale gain/loss, and the gross gain/loss split (informational). Enter the gross figures as-is;
  the Finanzamt applies the Teilfreistellung itself. The tool still applies Teilfreistellung in its
  own tax *estimate*, but the reported KAP-INV values stay pre-exemption. Each row carries its form
  Zeile:

  | Fund type | Distributions | Vorabpauschale | Sale gain/loss |
  |---|---|---|---|
  | Aktienfonds (equity) | Zeile 4 | Zeile 9 | Zeile 14 |
  | Mischfonds (mixed) | Zeile 5 | Zeile 10 | Zeile 17 |
  | sonstige (bond/other) | Zeile 8 | Zeile 13 | Zeile 26 |

Anlage KAP line numbers follow the 2024/2025 form; the KAP-INV Zeilen above have been stable since the
2018 InvStG reform. Both shift between years, so re-check them against the form for your filing year.

**Altbestand on KAP:** a pure pre-2009 (Altbestand) share sale has a positive gross gain but a
taxable amount of 0, so it contributes nothing to Zeile 19/20 — the tax-free gain is not declared as
income.

## Vorabpauschale (§18 InvStG)

Funds held at year end are taxed on an advance lump sum (Vorabpauschale) even without a distribution.
For each fund held on 31 December the tool computes:

```text
basisertrag    = nav_jan1 × basiszins × 0.7
vorabpauschale = max(0, min(basisertrag − distributions, max(0, nav_dec31 − nav_jan1)))
```

reduced by 1/12 per full month before the month of acquisition (Zwölftelung), then taxed after
Teilfreistellung. The lump sum is deemed received on the **first business day of the following year**
(§18 Abs. 3), so it is income of that year's return — it is reported in a dedicated CSV section with
its deemed-receipt date and is **not** folded into the current year's taxable base.

Because a foreign broker's statement carries no German year-boundary redemption prices, the NAVs are
supplied by hand in config:

- `taxes.basiszins.<year>` — the BMF Basiszins (fraction). The tool ships the statutory 2023 (2.55%),
  2024 (2.29%), and 2025 (2.53%) values; set this only to override or add a year.
- `taxes.fund_nav.<ISIN>.<year>` — `jan1` / `dec31` redemption price (EUR per unit) and an optional
  `acquired_month` (1–12) for the Zwölftelung. When omitted, the acquisition month is derived from the
  trade history (a fund first bought mid-year is prorated); set it only to override that.
- `taxes.vorabpauschale_carryforward.<ISIN>` — accumulated gross Vorabpauschale already taxed in
  prior years. On a **full** fund disposal it reduces the sale gain in full and before Teilfreistellung
  (§19 InvStG); the tool notes when to reset it. The CSV also prints the running accumulated figure
  to carry into next year's config.

A fund held at year end whose NAVs are not configured is reported in a `# WARNING` CSV block (and a
console warning) rather than silently omitted.

Holdings are taken as of 31 December of the tax year (reconstructed from the trade history when the
statement extends past year-end), not the statement's last-date snapshot.

**Simplifications:** a position of mixed vintage (partly held from the year's start, partly bought
mid-year) is approximated by a single acquisition month, and the sale-gain reduction is applied per
fund (not per FIFO lot) and only on a full disposal.

## Short Positions

Short (negative-quantity) holdings at year end — short stock, written options — are
Termin-/Stillhaltergeschäfte whose §20 EStG treatment the tool does not compute. Rather than dropping
them, it lists each one in a `# SHORT POSITIONS — MANUAL §20 EStG REVIEW REQUIRED` CSV block (and emits
a console warning), carrying the signed quantity. Classify and declare these by hand on Anlage KAP. They
are kept out of the cost-basis reconciliation, so they never affect the computed capital gains.

## Foreign Tax Credits

For dividends from foreign sources, you may have already paid withholding tax in the source country. German tax law allows crediting foreign taxes against German tax liability, subject to limits.

The program automatically:

- Tracks foreign tax withheld on dividends
- Calculates the creditable amount (limited to German tax on that income)
- Reports both foreign tax paid and the credit applied

### US Dividend Example

With the US-Germany tax treaty (15% withholding):

- US dividend: $100
- US withholding tax: $15 (15%)
- German tax on $85 (net): ~€22.42 (at 26.375%)
- Foreign tax credit: min(€15, €22.42) = €15
- Net German tax due: €22.42 - €15 = €7.42

## Foreign Currency Gains (Fremdwährungsgewinne, §20 EStG)

Holding a foreign currency and later spending it is a taxable event. When you receive USD (a
dividend, sale proceeds, or a EUR→USD conversion) at one exchange rate and later use it (buy a
security, pay a fee, convert back to EUR) at a different rate, the euro-value difference is a
realized gain or loss.

The program replays each currency's cash movements through a per-currency **signed-inventory
FIFO**, in statement order, and splits the result into two books by the running balance:

- **Positive balance — Fremdwährungsguthaben.** Because Interactive Brokers pays interest on cash
  balances, the account is interest-bearing, so its gains and losses are §20 EStG capital income.
  Gains raise `Zeile 19`; losses join the general pot in `Zeile 22`.
- **Negative balance — Fremdwährungskredit.** Buying a security in USD without USD cash borrows
  the currency. Repaying that loan is **not taxable** (Tilgung eines Fremdwährungskredits,
  BMF 19.05.2022 Rz. 131). The program reports this amount separately as *nicht steuerbar* and
  excludes it from the taxable base.

Real currency exchanges are valued at the actual execution rate from the broker transaction; every
other movement — including a security bought directly in USD, which produces a "verdeckter"
(hidden) gain with no euro changing hands — is valued at the ECB reference rate of the transaction
date.

The FX figures on a real 2025 IBKR statement reconcile with Interactive Brokers' own German tax
report (BubbleTax) within ECB rounding, and the non-taxable margin-loan split matches it exactly.

### Requirements and scope

The FIFO trusts the statement's own running balance and refuses to guess when it cannot. For the
figures to be correct, the statement must satisfy these boundaries — the program errors out rather
than emit a wrong number when they are not met:

- **Statement of Funds at `Currency` level of detail.** This is the source ledger. Without it the FX
  gain cannot be computed; the program warns when a statement shows foreign-currency activity but
  carries no such ledger.
- **A continuous history starting from a zero foreign balance.** The FIFO replays the movements and
  seeds every currency from empty, validating that they sum, in document order, to the broker's
  reported balance after each step. It therefore requires the ledger to begin where the foreign
  balance was zero (account opening). For a position held across a year boundary, drop **every
  year's** Flex file from account opening onward into the statements directory: they are merged in
  period order into one continuous ledger, so a lot acquired in 2023 and disposed in 2024 is
  correctly valued at its 2023 rate, and only the filing year's disposals are taxed. A single-year
  statement whose opening balance is non-zero, out-of-order rows, or dropped rows break the check and
  are rejected rather than mis-valued.
- **One account per query.** As with securities FIFO, a duplicate forex `transactionID` (multi-account
  or merged exports) makes the execution-rate pairing ambiguous and is rejected.
- **Tax treatment selected per account.** By default all foreign currency is treated as an
  interest-bearing Fremdwährungsguthaben (§20 EStG), which is correct for Interactive Brokers. A
  non-interest-bearing balance falls under §23 EStG instead; select it per portfolio with
  `foreign_currency_taxation: non_interest_bearing` (see below).
- **EUR-based account.** A conversion between two non-EUR currencies (e.g. USD↔GBP) has no EUR leg, so
  both sides are valued at the ECB reference rate rather than the actual execution rate.

Each realized §20 gain/loss is rounded to cents per row to reconcile against BubbleTax's per-line
worksheet; the non-taxable margin-loan total is summed at full precision and rounded once.

### Non-interest-bearing currency (§23 EStG, Anlage SO)

A foreign-currency balance that earns no interest is not §20 capital income; its disposals are
private Veräußerungsgeschäfte under §23 EStG. Set this per portfolio:

```yaml
portfolios:
  - name: my-account
    broker: interactive-brokers
    statements: "..."
    foreign_currency_taxation: non_interest_bearing  # default: interest_bearing (§20)
```

In this mode the same signed-inventory FIFO runs, but its realizations route to Anlage SO instead of
Anlage KAP, and **no tax is computed** — §23 income is taxed at the filer's personal income rate,
which the tool cannot know. It reports informational buckets, mirroring the Anlage N / §22 grant
handling:

- **One-year Spekulationsfrist.** A held-currency disposal more than one year after acquisition is
  tax-free (`long_term_tax_free`); the holding period is measured from the FIFO lot's acquisition
  date to the disposal, inclusive of the anniversary (a disposal exactly one year later is still
  taxable). Disposals within the year split into `short_term_gains` and `short_term_losses`.
- **Freigrenze cliff.** The statement prints the year's §23 Freigrenze (€600 through 2023, €1000 from
  2024). It is a cliff, not an allowance: if the filer's *total* private-sale gains for the year stay
  at or below it they are entirely tax-free, and one euro over makes the whole amount taxable. Because
  the tool sees only this account's currency gains — not the filer's other private sales — it cannot
  apply the Freigrenze itself and leaves the €0-tax decision to the filer.
- **Borrowed currency flagged for review.** The §20 Fremdwährungskredit exemption (BMF 19.05.2022
  Rz. 131) does not carry over to §23. A negative-balance repayment is surfaced as
  `borrowed_review` — its §23 treatment is a manual call, not silently excluded as it is under §20.

§23 losses offset only §23 gains (never the §20 pot or KAP), so the tool keeps these figures isolated
from the Anlage KAP totals.

## Altbestand (Pre-2009 Holdings)

Securities purchased before January 1, 2009 ("Altbestand") may be exempt from capital gains tax under grandfathering rules. The program:

- Detects purchases made before 2009
- Marks these as potentially tax-exempt
- Includes them in the CSV with a note

**Note:** Review pre-2009 holdings carefully with your tax advisor.

## CSV Output Format

The generated CSV contains:

### Transaction Rows

Each capital gain, dividend, and interest transaction includes:

- Date (transaction and settlement)
- Symbol and ISIN
- Description
- Amounts (cost basis, proceeds, gain/loss)
- Tax breakdown (Abgeltungssteuer, Soli, Kirchensteuer)
- Foreign tax and credits
- Teilfreistellung percentage
- Notes

### Summary Section

The final rows provide totals:

- Total capital gains/losses
- Total dividends
- Total interest
- Total taxable income
- Total German tax
- Total foreign tax credit
- Net tax due
- Loss carryforward (if applicable)

## Example Output

```csv
Type,Date,Settlement Date,Symbol,ISIN,Description,Quantity,Cost Basis (EUR),Proceeds (EUR),Gross Gain/Loss,Teilfreistellung %,Taxable Amount,Foreign Tax,Abgeltungssteuer,Solidaritätszuschlag,Kirchensteuer,Total Tax,Foreign Tax Credit,Notes
CAPITAL_GAIN,2024-03-15,2024-03-18,AAPL,US0378331005,Apple Inc.,10,1500.00,1800.00,300.00,0,300.00,0.00,75.00,4.13,6.75,85.88,0.00,
DIVIDEND,2024-06-15,,MSFT,US5949181045,Microsoft Corp.,,,,120.00,0,120.00,18.00,30.00,1.65,2.70,34.35,18.00,US withholding tax
...
SUMMARY,,,,,Total Capital Gains,,,,1500.00,,1500.00,,375.00,20.63,33.75,429.38,,
SUMMARY,,,,,Total Dividends,,,,850.00,,850.00,127.50,212.50,11.69,19.13,243.31,127.50,
SUMMARY,,,,,Net Tax Due,,,,,,,,,,,544.19,,
```

## Important Notes

1. **Currency Conversion:** All amounts are converted to EUR using ECB exchange rates on the transaction date.

2. **FIFO Method:** Cost basis is calculated using First-In-First-Out (FIFO) as required by German tax law.

3. **Self-Declaration:** The CSV is for your records and tax preparation. You must still file your tax return through official channels (ELSTER or paper forms).

4. **Tax Advisor:** Complex situations (derivatives, loss carryforwards across years, dual residency) should be reviewed with a qualified tax advisor.

5. **Broker Statements:** Ensure you have complete broker statements for the entire year and any previous years for accurate cost basis tracking.

6. **One account per Flex Query:** A multi-account Flex Query (several `FlexStatement` elements in one file) is rejected with an explicit error rather than processed. Export one account per query. FIFO (§20(4) S.7) is *per depot*, and the trade engine keys lots by symbol only, so pooling accounts would match sells against the wrong depot's lots and misstate gains or Altbestand status. Run each account as its own import.

## Troubleshooting

### Missing Exchange Rates

If ECB rates are unavailable for certain dates, the program will:

- Use the nearest available rate
- Log a warning about the fallback

### Unrecognized Instruments

For instruments without ISIN:

- The program uses the symbol as identifier
- Teilfreistellung defaults to 0%
- Review these entries manually

### Large Statements

For accounts with many transactions:

- Processing may take a few seconds
- The CSV will be sorted chronologically
- Summary rows appear at the end

## Additional Income Types

### Broker Fees (informational only)

Standalone broker fees are extracted and reported, but they do **not** reduce taxable income:

- Fees are automatically extracted from broker statements and converted to EUR using ECB rates
- They are reported in the CSV summary section for information only
- Under the Abgeltungsteuer, §20(9) EStG bars deducting actual expenses (Werbungskosten); the only
  deduction is the Sparer-Pauschbetrag. Trade commissions are already folded into the cost basis
  (Anschaffungsnebenkosten) at the point of sale.

### Loss offsetting and the Sparer-Pauschbetrag

- §20(6) EStG keeps two loss pots: share-sale (Aktien) losses offset only future share-sale gains;
  every other loss (fund/ETF sales, FX, etc.) forms the general pot, which also offsets dividends
  and interest. Neither pot offsets the other, and there is no carry-back.
- The Sparer-Pauschbetrag (€1,000 single since 2023, €801 before) is applied to the combined net
  positive result before tax.
- The summary tax is therefore computed once on the year's net taxable base — it is not the sum of
  the per-row tax columns, which are informational.

### Stock Grants / RSUs

Stock grants (Restricted Stock Units) have **two taxable events** in Germany:

1. **At Vesting (Employment Income):** The fair market value (FMV) at vest date is taxed as "geldwerter Vorteil" (benefit in kind) at your marginal income tax rate. This is employment income, not capital gains.

2. **At Sale (Capital Gains):** Only the gain above the vest-date FMV is subject to Abgeltungssteuer (25% flat tax).

The program:

- Reports vest-date FMV as employment income in a separate section
- Uses IBKR's provided FMV for accurate cost basis
- Warns if FMV is missing or zero (review manually)

**Important:** Employment income from RSUs must be declared on your Einkommensteuererklärung (income tax return), typically via Anlage N, not Anlage KAP.

### Cash Grants

Cash bonuses from brokers (sign-up bonuses, referral bonuses, etc.) are treated as "sonstige Einkünfte" (other income) under §22 EStG:

- Only taxable if total exceeds €256 per year (Freigrenze)
- Taxed at your marginal income tax rate, not the flat Abgeltungssteuer
- The program tracks these and issues a warning if the threshold is exceeded

**Important:** Cash grants should be declared on Anlage SO, not Anlage KAP.

### Corporate Actions

The program handles various corporate actions:

| Action Type | Tax Treatment |
|-------------|---------------|
| **Spinoffs** | Cost basis must be allocated between parent and spun-off shares based on market values |
| **Liquidations** | Treated as a sale event, may result in capital gain or loss |
| **Stock Splits** | No tax event - only quantity changes, cost basis per share adjusts accordingly |
| **Stock Dividends** | May be taxable as dividend income |
| **Delistings** | If shares become worthless, may claim capital loss |

The program:

- Extracts corporate actions from broker statements
- Reports them with tax impact in EUR
- Includes notes for manual review

**Note:** Corporate actions often require manual verification. Review these entries with your tax advisor.

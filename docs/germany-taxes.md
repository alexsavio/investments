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
  # Set to your applicable rate: 8% (Bavaria, Baden-Württemberg) or 9% (other states)
  # Default: 0 (no church tax)
  church_tax_rate: 9

  # Optional: Loss carryforward from previous years (Verlustvortrag)
  # Specify amounts by year
  loss_carryforward:
    2023: 1500.00
    2024: 2000.00

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

**Without Kirchensteuer:**

- 25% + (25% × 5.5%) = **26.375%**

**With Kirchensteuer (9%):**

- 25% + (25% × 5.5%) + (25% × 9%) = **28.625%**

## Teilfreistellung (Partial Exemption)

For ETFs, partial tax exemptions apply based on the fund composition:

| Fund Type | Equity Ratio | Exemption Rate | Taxable Portion |
|-----------|--------------|----------------|-----------------|
| Equity ETF | >51% | 30% | 70% |
| Mixed ETF | 25-51% | 15% | 85% |
| Bond ETF | <25% | 0% | 100% |
| Regular Stocks | N/A | 0% | 100% |

**Note:** Configure ETF classifications in your config to apply correct exemption rates.

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

### Broker Fees (Werbungskosten)

Broker fees and commissions are tracked separately and reported as deductible expenses:

- Fees are automatically extracted from broker statements
- All fees are converted to EUR using ECB rates
- The total is reported in the CSV summary section as deductible expenses
- Note: German tax law allows deducting fees from capital gains (Werbungskosten)

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

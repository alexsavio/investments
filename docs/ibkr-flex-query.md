# IBKR Flex Query Setup for German Tax Reporting

This guide explains how to configure an Interactive Brokers (IBKR) Flex Query report that provides all the data needed for German tax statements (Anlage KAP).

## Overview

A Flex Query is a customizable report format from IBKR that exports your account data in XML format. For German tax purposes, we need specific sections to calculate:

- **Capital gains/losses** from stock sales (KAP Zeile 23 for losses)
- **Dividends** with foreign withholding tax (KAP Zeile 19, 41)
- **Interest income** from cash balances (KAP Zeile 19)
- **FX gains/losses** from currency conversions (KAP Zeile 22 for losses)

## Creating a Flex Query

### Step 1: Access Flex Queries

1. Log in to IBKR Account Management (Client Portal)
2. Navigate to **Reports** → **Flex Queries**
3. Click **Create** or **+** to create a new Flex Query

### Step 2: Configure Basic Settings

| Setting | Value |
|---------|-------|
| Query Name | `German Tax Report` (or your preference) |
| Format | **XML** |
| Period | **Year to Date** or **Custom Date Range** |
| Date Format | `yyyyMMdd` |
| Time Format | `HHmmss` |
| Date/Time Separator | `;` (semicolon) |

### Step 3: Select Required Sections

Enable the following sections in your Flex Query:

#### Required Sections

| Section | Purpose | Key Fields |
|---------|---------|------------|
| **Trades** (Execution) | Stock purchases and sales | Symbol, DateTime, Quantity, TradePrice, Proceeds, Cost, Commission, AssetCategory |
| **Cash Transactions** | Dividends, interest, fees | Type, DateTime, Amount, Description, Symbol |
| **Statement of Funds** (Currency Breakout, Base Currency Summary) | FX conversions, transfers | ActivityDescription, Amount, Balance |
| **Forex P/L Details** (Transactions) | Accurate FX P&L | RealizedPL, ActivityDescription, DateTime |
| **Financial Instrument Information** | ISIN codes for stocks | Symbol, ISIN, Description |

#### Optional but Recommended

| Section | Purpose |
|---------|---------|
| **Cash Report** (Currency Breakout) | Account balances by currency |
| **Corporate Actions** (Detail) | Stock splits, mergers, spin-offs |
| **Open Positions** (Summary) | Year-end stock holdings |
| **Transfers** (Transfer) | Account transfers |
| **Grant Activity** | Stock grants (RSUs, options) |

### Step 4: Configure Each Section

> **Tip:** If you're unsure which fields to select, you can simply enable **all fields** for each section. The parser will extract what it needs and ignore the rest. This is the easiest approach and ensures you won't miss any required data.

#### Trades Section

Enable these fields (or select all):

- `AccountId`
- `Symbol`
- `DateTime`
- `Quantity`
- `TradePrice`
- `TradeMoney`
- `Proceeds`
- `Cost` (or `CostBasis`)
- `Commission` (or `IBCommission`)
- `Currency`
- `FxRateToBase`
- `AssetCategory`
- `Description`
- `ISIN`
- `SettleDateTarget`
- `BuySell`

#### CashTransactions Section

Enable these fields:

- `DateTime`
- `Type`
- `Amount`
- `Currency`
- `FxRateToBase`
- `Description`
- `Symbol`
- `ISIN`
- `ActionDescription`

Important transaction types:

- `Dividends` - Dividend payments
- `Withholding Tax` - Foreign tax withheld
- `Broker Interest Paid` / `Broker Interest Received` - Interest on cash
- `Payment In Lieu Of Dividends` - Dividend substitutes (fully taxable)

#### StmtFunds (Statement of Funds) Section

Enable these fields:

- `ActivityDescription`
- `TradeDate`
- `Amount`
- `Balance`
- `Currency`

Look for entries containing:

- `FOREX` - Currency conversion entries
- `Dividend` - Dividend-related entries
- `Interest` - Interest payments

#### FxTransactions Section

This section provides **accurate realized P&L** for FX conversions. Enable:

- `FunctionalCurrency`
- `FxCurrency`
- `DateTime`
- `ReportDate`
- `ActivityDescription`
- `Quantity`
- `RealizedPL`
- `Code`

The `RealizedPL` field contains the actual gain/loss for tax purposes, which is more accurate than calculating from position changes.

### Step 5: Save and Run

1. Click **Save** to store the Flex Query
2. Click **Run** to generate the report
3. Download the XML file

## Running the Report

### From Client Portal

1. Go to **Reports** → **Flex Queries**
2. Find your saved query
3. Click the **Run** (▶) button
4. Select the date range (full tax year: Jan 1 - Dec 31)
5. Click **Run**
6. Download the generated XML file

### Scheduling Automatic Reports

You can schedule Flex Queries to run automatically:

1. Edit your Flex Query
2. Enable **Delivery Configuration**
3. Set frequency (e.g., monthly, yearly)
4. Configure email or FTP delivery

## Configuration in investments

Add your IBKR portfolio to the configuration file:

```yaml
portfolios:
  - name: ibkr-germany
    broker: interactive-brokers
    statements: ~/Documents/IBKR/Statements
    currency: EUR  # Your base currency

    # How this account's foreign-currency balances are taxed.
    # `interest_bearing` (default) = §20 EStG, correct for IBKR.
    foreign_currency_taxation: interest_bearing

# The jurisdiction is account-wide, so it belongs in the top-level `taxes:` block,
# not inside a portfolio.
taxes:
  jurisdiction: germany       # required — omitting it applies Russian rules
  church_tax_rate: 0          # Kirchensteuer: 0, 8 or 9 (percent). Default 0
  sparer_pauschbetrag: 1000   # saver's allowance; default 1000 (2023+) / 801 (before)
```

See [`config-example.yaml`](config-example.yaml) for the remaining German options
(`loss_carryforward_stock`, `loss_carryforward_other`, `etf_classification`, `basiszins`,
`fund_nav`, `opening_foreign_currency`).

> `jurisdiction` is the only setting whose absence is not an error. A misspelled value fails
> to deserialize, and an unknown key is rejected outright by `deny_unknown_fields` — but an
> omitted `taxes:` block falls back to Russia. The tool warns when that happens; heed it.

Place your downloaded Flex Query XML files in the `statements` directory.

## Running the Tax Statement

Generate the German tax statement:

```bash
investments tax-statement ibkr-germany 2025 german-tax-2025.csv
```

This produces a CSV file with:

- All taxable transactions
- Calculated German taxes (Abgeltungssteuer, Soli, Kirchensteuer)
- Anlage KAP line values (Zeile 19, 22, 23, 41)
- Loss carryforward tracking

## Understanding the Output

### Anlage KAP Line Mapping

| Line | German Name | Content |
|------|-------------|---------|
| **Zeile 19** | Ausländische Kapitalerträge | Total foreign capital income (dividends + interest + gains) |
| **Zeile 22** | Sonstige Verluste | Non-stock losses (FX losses) |
| **Zeile 23** | Verluste aus Aktienveräußerungen | Stock sale losses (restricted offsetting) |
| **Zeile 41** | Anrechenbare ausländische Steuer | Creditable foreign withholding tax |

### FX Gain/Loss Categorization

The tool categorizes FX transactions for German tax purposes:

| Category | Tax Treatment | Example |
|----------|---------------|---------|
| **Taxable FX** | §20 EStG capital income | FX from dividend conversion, interest |
| **Non-taxable FX** | Tilgung Fremdwährungskredit | FX from margin loan repayment, stock trades |

IBKR accounts are interest-bearing, so most FX gains/losses are taxable under German law. However, FX from margin loan operations (large cash conversions for stock purchases) are treated as debt repayment and not taxable.

## Troubleshooting

### Missing ISIN Codes

If ISIN codes are missing:

1. Ensure **SecuritiesInfo** section is enabled
2. Check that `ISIN` field is selected in both Trades and SecuritiesInfo

### Incorrect FX Values

If FX gains/losses seem wrong:

1. Enable the **FxTransactions** section (provides accurate `RealizedPL`)
2. The tool prefers FxTransactions over StmtFunds for FX calculations

### Missing Dividends

Ensure CashTransactions includes:

- `Dividends` type transactions
- `Withholding Tax` entries (for foreign tax credit)
- `Payment In Lieu Of Dividends` (PILs)

### Date Range Issues

- Use **Year to Date** for current year
- Use **Custom Date Range** for previous years (Jan 1 - Dec 31)
- Ensure your account was active during the period

## Sample Flex Query XML Structure

A properly configured Flex Query produces XML like:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<FlexQueryResponse queryName="German Tax Report" type="AF">
  <FlexStatements count="1">
    <FlexStatement accountId="U1234567" toDate="20251231">
      <Trades>
        <Trade symbol="AAPL" dateTime="20251015;143022" quantity="10"
               proceeds="1500.00" cost="1200.00" commission="1.00" .../>
      </Trades>
      <CashTransactions>
        <CashTransaction type="Dividends" dateTime="20251201;080000"
                        amount="50.00" symbol="AAPL" .../>
        <CashTransaction type="Withholding Tax" amount="-7.50" .../>
      </CashTransactions>
      <StmtFunds>
        <StatementOfFundsLine activityDescription="FOREX" amount="100.00" .../>
      </StmtFunds>
      <FxTransactions>
        <FxTransaction realizedPL="1.50" activityDescription="CASH: -100 EUR.USD" .../>
      </FxTransactions>
      <SecuritiesInfo>
        <SecurityInfo symbol="AAPL" isin="US0378331005" description="APPLE INC"/>
      </SecuritiesInfo>
    </FlexStatement>
  </FlexStatements>
</FlexQueryResponse>
```

## References

- [IBKR Flex Query Documentation](https://www.interactivebrokers.com/en/software/reportguide/reportguide.htm)
- [German Anlage KAP Instructions](https://www.bundesfinanzministerium.de)
- [§20 EStG - Capital Income Taxation](https://www.gesetze-im-internet.de/estg/__20.html)

# Data Model: Germany Tax Reports

**Feature**: Germany Tax Reports
**Date**: 2026-01-24
**Input**: spec.md requirements + research.md findings

## Overview

This document defines the data entities and their relationships for German tax report generation. The model follows existing patterns from the Russia tax implementation while adding Germany-specific structures.

## Core Entities

### 1. GermanTaxStatement

Represents a complete tax declaration for a single tax year.

**Attributes**:

- `year: i32` - Tax year (e.g., 2025)
- `jurisdiction: Jurisdiction::Germany` - Fixed jurisdiction
- `capital_gains: Vec<CapitalGainEntry>` - All stock/ETF sales
- `dividends: Vec<DividendEntry>` - All dividend income
- `interest: Vec<InterestEntry>` - Interest income (if any)
- `loss_carryforward_used: Decimal` - Previous year losses applied
- `loss_carryforward_remaining: Decimal` - Losses to carry forward to next year
- `total_taxable_income: Decimal` - Sum of all taxable amounts
- `total_foreign_tax: Decimal` - Sum of foreign withholding tax
- `total_abgeltungssteuer: Decimal` - 25% capital gains tax
- `total_solidaritaetszuschlag: Decimal` - 5.5% solidarity surcharge
- `total_kirchensteuer: Decimal` - Church tax (if applicable)
- `total_german_tax: Decimal` - Sum of all German taxes
- `net_tax_due: Decimal` - German tax minus foreign credits

**Relationships**:

- Contains many `CapitalGainEntry`
- Contains many `DividendEntry`
- Contains many `InterestEntry`

**Validation Rules**:

- Year must be >= 2009 (Abgeltungssteuer introduction)
- All amounts in EUR (Decimal type)
- All amounts rounded to €0.01 precision
- Foreign tax credit cannot exceed German tax on that income

### 2. CapitalGainEntry

Represents a single stock or ETF sale transaction.

**Attributes**:

- `transaction_date: Date` - Trade conclusion date
- `settle_date: Date` - Settlement date (used for tax purposes per FR-014)
- `symbol: String` - Ticker symbol (e.g., "AAPL", "VWCE.DE")
- `isin: String` - International Securities Identification Number
- `description: String` - Human-readable description
- `quantity: Decimal` - Number of shares/units sold
- `cost_basis_eur: Decimal` - Purchase cost in EUR (via FIFO matching)
- `proceeds_eur: Decimal` - Sale proceeds in EUR
- `gross_gain_loss: Decimal` - proceeds - cost_basis
- `teilfreistellung_rate: Decimal` - Exemption rate (0, 0.15, or 0.30)
- `taxable_amount: Decimal` - After Teilfreistellung exemption
- `foreign_tax: Decimal` - Foreign withholding tax (usually 0 for stock sales)
- `abgeltungssteuer: Decimal` - 25% of taxable_amount
- `solidaritaetszuschlag: Decimal` - 5.5% of Abgeltungssteuer
- `kirchensteuer: Decimal` - Church tax on Abgeltungssteuer
- `total_tax: Decimal` - Sum of German taxes
- `pre_2009_holding: bool` - Altbestand flag (tax-exempt)

**Relationships**:

- Belongs to `GermanTaxStatement`
- References `Instrument` for ETF classification
- Matched against `FifoLot` entries for cost basis

**Validation Rules**:

- settle_date >= transaction_date
- quantity > 0
- All amounts use Decimal type
- teilfreistellung_rate in [0, 0.15, 0.30]
- If pre_2009_holding = true, taxable_amount = 0

**Calculation**:

```text
gross_gain_loss = proceeds_eur - cost_basis_eur
taxable_amount = gross_gain_loss * (1 - teilfreistellung_rate)
abgeltungssteuer = max(0, taxable_amount * 0.25)
solidaritaetszuschlag = abgeltungssteuer * 0.055
kirchensteuer = abgeltungssteuer * church_tax_rate
total_tax = abgeltungssteuer + solidaritaetszuschlag + kirchensteuer
```

### 3. DividendEntry

Represents a single dividend payment.

**Attributes**:

- `payment_date: Date` - Dividend payment date
- `symbol: String` - Ticker symbol
- `isin: String` - International Securities Identification Number
- `description: String` - Description (e.g., "AAPL Cash Dividend")
- `quantity: Decimal` - Number of shares held
- `dividend_per_share: Decimal` - Gross dividend per share (local currency)
- `gross_amount_eur: Decimal` - Total gross dividend in EUR
- `foreign_withholding_tax: Decimal` - Tax withheld by foreign broker (in EUR)
- `teilfreistellung_rate: Decimal` - Exemption rate for ETFs (0, 0.15, or 0.30)
- `taxable_amount: Decimal` - After Teilfreistellung exemption
- `abgeltungssteuer: Decimal` - 25% of taxable_amount
- `solidaritaetszuschlag: Decimal` - 5.5% of Abgeltungssteuer
- `kirchensteuer: Decimal` - Church tax on Abgeltungssteuer
- `foreign_tax_credit: Decimal` - min(foreign_tax, German tax)
- `total_tax: Decimal` - German taxes
- `net_tax: Decimal` - total_tax - foreign_tax_credit

**Relationships**:

- Belongs to `GermanTaxStatement`
- References `Instrument` for ETF classification

**Validation Rules**:

- payment_date must be in tax year
- quantity > 0
- dividend_per_share >= 0
- foreign_withholding_tax <= gross_amount_eur
- foreign_tax_credit <= min(foreign_withholding_tax, total_tax)
- teilfreistellung_rate in [0, 0.15, 0.30]

**Calculation**:

```text
gross_amount_eur = (from broker statement, converted via ECB rate)
taxable_amount = gross_amount_eur * (1 - teilfreistellung_rate)
abgeltungssteuer = taxable_amount * 0.25
solidaritaetszuschlag = abgeltungssteuer * 0.055
kirchensteuer = abgeltungssteuer * church_tax_rate
total_tax = abgeltungssteuer + solidaritaetszuschlag + kirchensteuer
foreign_tax_credit = min(foreign_withholding_tax, total_tax)
net_tax = total_tax - foreign_tax_credit
```

### 4. InterestEntry

Represents interest income (e.g., from idle cash in brokerage account).

**Attributes**:

- `payment_date: Date` - Interest payment date
- `description: String` - Description
- `gross_amount_eur: Decimal` - Total interest in EUR
- `foreign_withholding_tax: Decimal` - Foreign tax withheld (if any)
- `taxable_amount: Decimal` - Same as gross (no Teilfreistellung for interest)
- `abgeltungssteuer: Decimal` - 25% of taxable_amount
- `solidaritaetszuschlag: Decimal` - 5.5% of Abgeltungssteuer
- `kirchensteuer: Decimal` - Church tax on Abgeltungssteuer
- `foreign_tax_credit: Decimal` - min(foreign_tax, German tax)
- `total_tax: Decimal` - German taxes
- `net_tax: Decimal` - total_tax - foreign_tax_credit

**Relationships**:

- Belongs to `GermanTaxStatement`

**Validation Rules**:

- payment_date must be in tax year
- gross_amount_eur >= 0
- No Teilfreistellung (always 0%)

### 5. FifoLot

Internal structure for cost basis tracking (not in final output).

**Attributes**:

- `symbol: String` - Ticker symbol
- `purchase_date: Date` - Original purchase date
- `settle_date: Date` - Purchase settlement date
- `quantity: Decimal` - Remaining shares in this lot
- `cost_per_share_eur: Decimal` - Cost basis per share in EUR
- `total_cost_eur: Decimal` - quantity * cost_per_share_eur
- `pre_2009: bool` - Whether purchased before 2009-01-01

**Relationships**:

- Part of `FifoQueue` for a specific symbol
- Consumed by `CapitalGainEntry` calculations

**Validation Rules**:

- quantity > 0
- cost_per_share_eur >= 0
- settle_date >= purchase_date

**Operations**:

- `deduct(quantity)` - Remove shares from lot when selling
- `split()` - Partial consumption of lot

### 6. FifoQueue

Collection of lots for FIFO cost basis tracking.

**Attributes**:

- `symbol: String` - Ticker symbol
- `lots: VecDeque<FifoLot>` - Queue of purchase lots (oldest first)

**Operations**:

- `add_purchase(date, quantity, cost)` - Add new lot to end of queue
- `sell(quantity) -> Vec<Match>` - Match sale against oldest lots, return matches
- `remaining_quantity() -> Decimal` - Total shares in queue

**Validation Rules**:

- Lots ordered by purchase_date (oldest first)
- Cannot sell more than available quantity

### 7. LossCarryforward

Tracks capital losses from previous years.

**Attributes**:

- `year: i32` - Year the loss originated
- `amount_eur: Decimal` - Loss amount (positive number)
- `utilized_eur: Decimal` - Amount used in current year
- `remaining_eur: Decimal` - amount - utilized

**Relationships**:

- Loaded from configuration
- Applied to `GermanTaxStatement` calculations

**Validation Rules**:

- amount > 0
- utilized >= 0
- utilized <= amount
- remaining = amount - utilized

### 8. EtfClassification

Metadata for ETF Teilfreistellung calculation.

**Attributes**:

- `symbol: String` - ETF ticker symbol
- `isin: String` - ISIN
- `classification: EtfType` - Equity, Mixed, or Bond
- `teilfreistellung_rate: Decimal` - 0.30, 0.15, or 0.00

**Enum: EtfType**:

- `Equity` - ≥51% stocks (30% exemption)
- `Mixed` - ≥25% stocks (15% exemption)
- `Bond` - <25% stocks (0% exemption)

**Relationships**:

- Extends `Instrument` struct
- Referenced by `CapitalGainEntry` and `DividendEntry`

**Validation Rules**:

- classification determines teilfreistellung_rate
- Cannot change mid-year (simplification)

### 9. GermanTaxConfig

Configuration specific to German tax calculation.

**Attributes**:

- `church_tax_rate: Decimal` - 0.00, 0.08, or 0.09
- `loss_carryforward: BTreeMap<i32, Decimal>` - Year -> amount
- `exchange_rate_fallback: FallbackMode` - Strict or Lenient
- `etf_classifications: HashMap<String, EtfType>` - Symbol -> classification

**Enum: FallbackMode**:

- `Strict` - Error if exact ECB rate not found
- `Lenient` - Use nearest previous business day with warning

**Relationships**:

- Part of main `TaxConfig` struct
- Loaded from `~/.investments/config.yaml`

**Validation Rules**:

- church_tax_rate in [0, 0.08, 0.09]
- loss_carryforward amounts > 0
- loss_carryforward years < current tax year

## Entity Relationships Diagram

```text
GermanTaxStatement (1)
    |
    +-- (many) CapitalGainEntry
    |       |
    |       +-- references --> Instrument (ETF classification)
    |       +-- matched via --> FifoQueue --> FifoLot (many)
    |
    +-- (many) DividendEntry
    |       |
    |       +-- references --> Instrument (ETF classification)
    |
    +-- (many) InterestEntry
    |
    +-- uses --> LossCarryforward (many, from previous years)
    |
    +-- configured by --> GermanTaxConfig
```

## Data Flow

1. **Input**: Broker statement parsed by existing parsers
   - Trades → extract purchases and sales
   - Dividends → extract payment details
   - Interest → extract payment details

2. **FIFO Matching**: Build FifoQueue per symbol
   - Add purchases to queue (chronological order)
   - Match sales against oldest lots
   - Generate CapitalGainEntry for each sale

3. **Currency Conversion**: Convert all amounts to EUR
   - Use ECB rates for transaction settle_date
   - Cache rates in SQLite database
   - Apply fallback strategy if rate missing

4. **Tax Calculation**:
   - Apply Teilfreistellung based on ETF classification
   - Calculate Abgeltungssteuer (25%)
   - Calculate Solidaritätszuschlag (5.5%)
   - Calculate Kirchensteuer (0-9%)
   - Apply foreign tax credits

5. **Loss Carryforward**:
   - Load previous years' losses from config
   - Apply against current year gains (oldest first)
   - Calculate new loss carryforward if applicable

6. **Output**: GermanTaxStatement
   - Aggregate all entries
   - Generate summary totals
   - Export to CSV

## Database Schema

No new database tables required. Reuse existing:

**currency_rates** (existing, in `~/.investments/db.sqlite`):

- `id: INTEGER PRIMARY KEY`
- `date: TEXT` - Rate date (YYYY-MM-DD)
- `base: TEXT` - Base currency (always "EUR" for ECB)
- `quote: TEXT` - Quote currency (e.g., "USD")
- `rate: REAL` - Exchange rate
- Used for ECB rate caching

## Configuration Schema

Addition to `~/.investments/config.yaml`:

```yaml
# Main config section
tax_jurisdiction: germany  # or "russia", "usa"

# Germany-specific configuration
germany_tax:
  church_tax_rate: 0.08  # 0, 0.08, or 0.09

  exchange_rate_fallback: lenient  # or "strict"

  # Loss carryforwards from previous years
  loss_carryforward:
    2024: 5000.00
    2023: 1200.50

  # ETF classifications for Teilfreistellung
  etf_classifications:
    VWCE.DE: equity    # or "mixed", "bond"
    EUNL.DE: equity
    AGGH.DE: bond
```

## Type Definitions (Rust)

Key struct signatures:

```rust
pub struct GermanTaxStatement {
    pub year: i32,
    pub capital_gains: Vec<CapitalGainEntry>,
    pub dividends: Vec<DividendEntry>,
    pub interest: Vec<InterestEntry>,
    pub loss_carryforward_used: Decimal,
    pub loss_carryforward_remaining: Decimal,
    // ... summary fields
}

pub struct CapitalGainEntry {
    pub transaction_date: Date,
    pub settle_date: Date,
    pub symbol: String,
    pub isin: String,
    pub quantity: Decimal,
    pub cost_basis_eur: Decimal,
    pub proceeds_eur: Decimal,
    pub taxable_amount: Decimal,
    pub pre_2009_holding: bool,
    // ... tax fields
}

pub struct FifoQueue {
    symbol: String,
    lots: VecDeque<FifoLot>,
}

pub struct FifoLot {
    pub purchase_date: Date,
    pub quantity: Decimal,
    pub cost_per_share_eur: Decimal,
    pub pre_2009: bool,
}

pub enum EtfType {
    Equity,   // 30% Teilfreistellung
    Mixed,    // 15% Teilfreistellung
    Bond,     // 0% Teilfreistellung
}
```

## Validation Summary

All entities must satisfy:

- **Type Safety**: Decimal for all currency amounts (never float)
- **Precision**: EUR amounts rounded to €0.01 (2 decimal places)
- **Date Validity**: settle_date >= transaction_date
- **Non-Negative**: Quantities, amounts always >= 0
- **Range Checks**: Tax rates, exemption rates within valid ranges
- **FIFO Integrity**: Cannot sell more shares than owned
- **Tax Year**: All transactions within specified tax year

## Migration from Existing Code

No breaking changes:

- Extends `Jurisdiction` enum with `Germany`
- Extends `TaxConfig` with `germany_tax` section
- Follows same patterns as Russia implementation
- Reuses existing `currency_rates` database table
- Reuses existing broker statement parsers

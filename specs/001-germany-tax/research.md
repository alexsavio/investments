# Research: Germany Tax Reports

**Feature**: Germany Tax Reports
**Date**: 2026-01-24
**Status**: Complete

## Overview

This document captures research findings for implementing German tax report generation. The research resolves all technical unknowns identified in the Technical Context and clarifies best practices for German tax calculations.

## 1. ECB Exchange Rate API Integration

**Decision**: Use ECB's Statistical Data Warehouse (SDW) XML API with local caching

**Rationale**:

- ECB provides official daily reference exchange rates used across EU
- Free, stable API with historical data back to 1999
- XML format easily parseable with existing Rust XML libraries
- Rates published around 16:00 CET daily (after market close)
- German tax authorities accept ECB rates as official source

**API Details**:

- Endpoint: `https://sdw-wsrest.ecb.europa.eu/service/data/EXR/D..EUR.SP00.A`
- Format: XML (SDMX-ML)
- Rate limit: None specified for statistical data
- Historical data: Full history available

**Implementation Approach**:

- Extend `src/forex.rs` with new `EcbRateProvider` struct
- Implement existing `QuoteProvider` trait pattern
- Cache rates in SQLite database (reuse `currency_rates` table)
- Fallback strategy: configurable strict/lenient mode (FR-017)
  - Strict: error if exact date not found
  - Lenient: use nearest previous business day with warning

**Alternative Considered**: Bundesbank API - rejected because ECB is more widely used and has cleaner API

## 2. German Tax Rate Structure (By Year)

**Decision**: Implement year-specific tax rate maps similar to Russia implementation

**Key Tax Rates History**:

| Year Range | Abgeltungssteuer | Solidaritätszuschlag | Notes |
|------------|------------------|---------------------|-------|
| 2009-2020  | 25%             | 5.5% of Abgeltungssteuer | Introduced January 1, 2009 |
| 2021+      | 25%             | 5.5% of Abgeltungssteuer | Solidarity surcharge exemption for low incomes (not implemented initially per A-008) |

**Kirchensteuer (Church Tax)**:

- Bavaria, Baden-Württemberg: 8%
- Other states: 9%
- Applied to Abgeltungssteuer
- User-configurable (0%, 8%, or 9%)

**Implementation Pattern**:

```rust
// In src/localities.rs - following russia() pattern
pub fn germany(config: &TaxConfig) -> Country {
    let jurisdiction = Jurisdiction::Germany;
    let tax_precision = 2; // Euro cents

    // Abgeltungssteuer introduced in 2009
    let rates_2009 = btreemap! {
        dec!(0) => dec!(0.25),  // Flat 25% rate
    };

    let tax_calculators = btreemap! {
        2009 => Box::new(GermanTaxRate::new(
            dec!(0.25),          // Abgeltungssteuer
            dec!(0.055),         // Solidaritätszuschlag
            config.church_tax_rate, // 0.00, 0.08, or 0.09
            tax_precision
        )) as Box<dyn TaxRate>,
    };

    Country::new(Jurisdiction::Germany, tax_calculators, tax_calculators.clone(), None)
}
```

**Rationale**: German capital gains tax is flat-rate (not progressive like Russia), simplifying calculations

## 3. FIFO Cost Basis Tracking

**Decision**: Implement FIFO queue per security symbol with purchase date tracking

**German Tax Law Requirement**:

- Capital gains taxed on FIFO (First-In-First-Out) basis
- Pre-2009 holdings (Altbestand) exempt from Abgeltungssteuer
- Purchase date determines tax treatment

**Implementation Approach**:

- Create `FifoQueue<T>` struct in `src/taxes/germany/capital_gains.rs`
- Track `(purchase_date, quantity, cost_basis_per_share)` tuples
- When selling, match against oldest purchases first
- Separate tracking for pre-2009 vs post-2009 holdings

**Data Structure**:

```rust
struct FifoQueue {
    symbol: String,
    lots: VecDeque<Lot>,
}

struct Lot {
    purchase_date: Date,
    settle_date: Date,
    quantity: Decimal,
    cost_basis_eur: Decimal,  // Total cost in EUR
    pre_2009: bool,            // Altbestand flag
}
```

**Rationale**: VecDeque provides efficient push_back/pop_front for FIFO operations

## 4. Teilfreistellung (Partial Exemption) for ETFs

**Decision**: ETF classification metadata in instrument configuration, applied at calculation time

**Exemption Rates** (unchanged since 2009):

- Equity ETFs (≥51% stocks): 30% of income exempt
- Mixed ETFs (≥25% stocks): 15% of income exempt
- Bond ETFs (<25% stocks): 0% (no exemption)
- Real estate ETFs: 60% or 80% depending on type (not implemented initially)

**Implementation Approach**:

- Extend `Instrument` struct in `src/instruments.rs` with `etf_classification` field
- Add to config.yaml:

  ```yaml
  instruments:
    VWCE.DE:
      type: equity_etf  # or mixed_etf, bond_etf
  ```

- Apply exemption in both dividend and capital gain calculations
- Missing classification = 0% exemption (conservative default)

**Calculation Pattern**:

```rust
fn apply_teilfreistellung(gross_income: Decimal, classification: EtfType) -> Decimal {
    let exemption_rate = match classification {
        EtfType::Equity => dec!(0.30),
        EtfType::Mixed => dec!(0.15),
        EtfType::Bond => dec!(0),
    };
    gross_income * (dec!(1) - exemption_rate)
}
```

**Rationale**: Manual configuration initially (automatic detection can be added later per A-003)

## 5. Foreign Tax Credit Calculation

**Decision**: Implement double taxation treaty limits per source country

**Treaty Rates** (major jurisdictions):

- USA: 15% withholding on dividends (treaty limit)
- UK: 15% withholding
- Ireland: 15% withholding
- Most EU countries: 15% or lower

**Maximum Credit**: Cannot exceed German tax on that income

**Implementation Approach**:

- Read withholding tax from broker statement (existing parsers)
- Convert to EUR using ECB rates
- Calculate German tax: `taxable_amount * 0.25`
- Credit = `min(foreign_tax_eur, german_tax)`
- Remaining foreign tax is lost (no carryforward for excess)

**Special Case - US Tax Increase**:

- Pre-August 2024: 10% US withholding (treaty)
- Post-August 2024: up to 30% possible
- Credit only up to German rate (25%), excess lost
- Handled by existing `us_dividend_tax_rate()` function pattern

**Rationale**: Follows existing Russia tax credit implementation pattern

## 6. Loss Carryforward Mechanics

**Decision**: User-configured previous year losses in config, calculated current year losses in output

**German Tax Law**:

- Capital losses offset capital gains in same year
- Net losses carry forward indefinitely to future years
- No carryback to previous years
- Separate tracking for different income types (not applicable here - all capital gains)

**Configuration Format**:

```yaml
germany_tax:
  loss_carryforward:
    2024: 5000.00  # €5,000 loss from 2024
    2023: 1200.50  # €1,200.50 loss from 2023
```

**Calculation Logic**:

1. Sum all capital gains for year
2. Sum all capital losses for year
3. Net gain/loss = gains - losses
4. If net gain > 0: apply loss carryforward from previous years (oldest first)
5. Calculate tax on remaining gain
6. If net loss: report as new loss carryforward for next year

**Output**: CSV includes utilized carryforward amounts and remaining balance

**Rationale**: Simple user-managed approach, no automatic multi-year state tracking

## 7. CSV Output Format Design

**Decision**: Comprehensive single-file CSV with all transaction details

**Column Structure**:

1. `transaction_type` - "Capital Gain", "Dividend", "Interest"
2. `transaction_date` - Trade conclusion date
3. `settle_date` - Settlement date (used for tax purposes)
4. `symbol` - Ticker symbol
5. `isin` - International Securities Identification Number
6. `description` - Human-readable transaction description
7. `quantity` - Shares/units
8. `cost_basis_eur` - Purchase cost in EUR
9. `proceeds_eur` - Sale proceeds in EUR
10. `gross_gain_loss_eur` - Before exemptions
11. `teilfreistellung_pct` - Exemption percentage (0%, 15%, 30%)
12. `taxable_amount_eur` - After exemptions
13. `foreign_tax_eur` - Withholding tax paid
14. `abgeltungssteuer_eur` - 25% capital gains tax
15. `solidaritaetszuschlag_eur` - 5.5% solidarity surcharge
16. `kirchensteuer_eur` - Church tax (if applicable)
17. `total_german_tax_eur` - Sum of German taxes
18. `net_tax_eur` - German tax minus foreign credit

**Summary Rows** (at end of CSV):

- Total taxable income
- Total foreign tax paid
- Total German tax due
- Loss carryforward utilized
- New loss carryforward

**Rationale**: Provides complete audit trail, matches Russia tax-statement pattern

## 8. Pre-2009 Holdings (Altbestand) Handling

**Decision**: Detect and warn, minimal initial support

**Tax Treatment**:

- Stocks purchased before 2009: capital gains tax-free
- Held as "Altbestand" (legacy holdings)
- Must track separate cost basis
- Special rules for partial sales

**Implementation**:

- Check purchase_date < 2009-01-01
- Flag as `pre_2009: true` in Lot struct
- When selling: emit warning if Altbestand detected
- Exclude from taxable gains
- Include in CSV with note "Altbestand - exempt"

**Warning Message**: "Pre-2009 holdings detected. Complex Altbestand rules may apply - consult tax advisor."

**Rationale**: Rare case, complex rules, initial warning approach per A-006

## 9. Derivative Instrument Detection

**Decision**: Detect by instrument type, emit warning, exclude from calculations

**Detection Strategy**:

- Check symbol patterns: options often end in "O" or have strike/expiry codes
- Check instrument type in broker statement metadata
- Futures often have "/F" or month codes
- Warrants often have "W" suffix

**Action on Detection**:

1. Emit clear warning: "Derivative instrument {symbol} detected - not supported"
2. Exclude from tax calculations
3. Log at WARN level
4. Continue processing other transactions

**Instrument Types to Exclude**:

- Futures
- Options (calls/puts)
- Warrants
- Structured products (certificates)

**Rationale**: Per FR-016 and out-of-scope section, derivatives require specialized tax treatment

## 10. Debug Logging Strategy

**Decision**: Structured debug logging for all calculation steps

**Log Levels**:

- ERROR: Missing required data, validation failures
- WARN: Derivatives detected, missing ETF classification, fallback exchange rates
- INFO: High-level progress (processing statement, generating CSV)
- DEBUG: Each calculation step (FIFO matching, tax computation, currency conversion)
- TRACE: Individual transaction details

**Debug Log Content Examples**:

```text
DEBUG: [Capital Gain] AAPL: Selling 100 shares
DEBUG: [FIFO] Matched against lot from 2020-03-15: 100 shares @ €95.50
DEBUG: [Currency] Convert $12,345.67 to EUR using ECB rate 0.8512 = €10,507.89
DEBUG: [Tax] Gross gain: €5,250.00, Taxable: €5,250.00, Tax: €1,312.50
DEBUG: [Tax] Abgeltungssteuer: €1,312.50, Soli: €72.19, Church (8%): €104.98
```

**Implementation**: Use existing `log` crate macros, configured via `RUST_LOG` environment variable

**Rationale**: Per FR-015, enables verification and troubleshooting of complex calculations

## Technology Stack Summary

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| Language | Rust 2024 | Project standard, type safety for financial calculations |
| Currency Arithmetic | rust_decimal | Precise decimal arithmetic, no float errors |
| Date Handling | chrono | Project standard, handles timezones |
| XML Parsing | quick-xml or serde-xml-rs | For ECB API responses |
| CSV Output | csv crate | Project standard, used in existing cash-flow command |
| Database | SQLite (diesel ORM) | Existing infrastructure for forex caching |
| Testing | cargo test + testdata fixtures | Project standard |
| Logging | log crate | Project standard |

## Open Questions: None

All technical unknowns have been resolved through this research phase.

## References

- [ECB Statistical Data Warehouse](https://sdw.ecb.europa.eu/)
- [German Tax Law (EStG §20)](https://www.gesetze-im-internet.de/estg/__20.html) - Investment income taxation
- [Abgeltungssteuer Overview](https://en.wikipedia.org/wiki/Capital_gains_tax#Germany)
- Existing codebase patterns:
  - `src/localities.rs` - russia() function
  - `src/taxes/rates.rs` - TaxRate trait
  - `src/tax_statement/statement/` - Russia implementation
  - `src/forex.rs` - Currency conversion

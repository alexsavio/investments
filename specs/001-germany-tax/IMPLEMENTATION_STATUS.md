# Germany Tax Reports - Implementation Status

**Feature**: Germany Tax Reports
**Branch**: `001-germany-tax`
**Last Updated**: 24 January 2026

---

## Summary

The Germany tax statement generation feature is now functional at the MVP level. Users can generate German tax statements with capital gains, dividends, and interest calculations including all German tax components (Abgeltungssteuer, Solidaritätszuschlag, Kirchensteuer).

---

## Phase Completion Status

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Setup | ✅ Complete |
| Phase 2 | Foundational (Blocking) | ✅ Complete |
| Phase 3 | User Story 1 - MVP | 🟡 ~90% Complete |
| Phase 4 | User Story 2 - Loss Carryforward | ⬜ Not Started |
| Phase 5 | User Story 3 - Teilfreistellung | ⬜ Not Started |
| Phase 6 | Polish & Documentation | ⬜ Not Started |

---

## Test Results

```text
cargo test germany: 17 passed ✓
cargo test --lib --skip broker_statement: 222 passed ✓
```

### Germany-Specific Tests (17 total)

**Tax Rate Calculation** (`src/taxes/germany/rates.rs`):

- ✅ test_default_rates
- ✅ test_effective_rate_no_church_tax
- ✅ test_effective_rate_with_church_tax
- ✅ test_tax_calculation
- ✅ test_tax_calculation_with_church_tax

**Capital Gains** (`src/taxes/germany/capital_gains.rs`):

- ✅ test_fifo_basic
- ✅ test_fifo_pre_2009
- ✅ test_calculate_capital_gain
- ✅ test_calculate_capital_gain_with_loss

**Dividends** (`src/taxes/germany/dividends.rs`):

- ✅ test_dividend_tax_no_exemption
- ✅ test_dividend_tax_with_equity_etf
- ✅ test_foreign_tax_credit_limit
- ✅ test_teilfreistellung_rates

**Processor** (`src/tax_statement/germany/processor.rs`):

- ✅ test_teilfreistellung_application
- ✅ test_equity_etf_teilfreistellung
- ✅ test_pre_2009_altbestand_handling
- ✅ test_end_to_end_csv_generation

---

## Completed Tasks (Phase 3 MVP)

### Data Model ✅

- [x] T030-T035: All entry structs created (FifoLot, FifoQueue, CapitalGainEntry, DividendEntry, InterestEntry, GermanTaxStatement)

### FIFO Capital Gains ✅

- [x] T036-T039: FIFO queue implementation with Altbestand (pre-2009) detection

### Dividend Calculation ✅

- [x] T041-T043: Dividend income with Teilfreistellung and foreign tax credits

### Tax Rate Calculation ✅

- [x] T045-T047: GermanTaxRates with Abgeltungssteuer + Soli + Kirchensteuer

### Tax Statement Aggregation ✅

- [x] T049-T054: GermanTaxStatement with add methods and totals calculation

### CSV Output ✅

- [x] T055-T061: Complete CSV formatter with all column types and summary rows

### CLI Integration ✅

- [x] T062-T063: Jurisdiction routing and tax statement generation wired into CLI

### Test Verification ✅

- [x] T066: All Germany tests pass (GREEN phase achieved)

---

## Remaining MVP Tasks

| Task | Description | Status | Priority |
|------|-------------|--------|----------|
| T064 | Error handling for missing ECB rates | ⬜ | Medium |
| T065 | Warning for detected derivatives (FR-016) | ⬜ | Low |
| T020 | Create `tests/germany_tax_tests.rs` integration test file | ⬜ | Medium |
| T026-T028 | Create test fixtures in `testdata/germany_tax/` | ⬜ | Medium |
| T029 | Verify tests fail initially (RED phase for integration) | ⬜ | Medium |
| T067-T069 | Integration testing and refinement | ⬜ | Medium |

---

## How to Use

### 1. Configure Jurisdiction

Add to your `config.yaml`:

```yaml
taxes:
  jurisdiction: germany
  church_tax_rate: 9        # Optional: 0-9%, default 0
  loss_carryforward:        # Optional: carryforward amounts by year
    2023: 1500
    2024: 2000
```

### 2. Generate Tax Statement

```bash
investments tax-statement <portfolio_name> 2024 german-tax-2024.csv
```

### 3. Output

The CSV contains:

- **Capital Gains**: Transaction date, symbol, ISIN, quantity, cost basis, proceeds, gain/loss, Teilfreistellung, taxes
- **Dividends**: Payment date, symbol, gross amount, withholding, Teilfreistellung, taxes
- **Interest**: Date, source, amount, taxes
- **Summary**: 10 rows with totals for taxable income, each tax component, foreign tax credits, net tax due

---

## Architecture

### File Structure

```text
src/
├── taxes/
│   └── germany/
│       ├── mod.rs              # Module exports
│       ├── rates.rs            # GermanTaxRates, calculate_german_taxes()
│       ├── capital_gains.rs    # FifoLot, FifoQueue, calculate_capital_gain()
│       └── dividends.rs        # Teilfreistellung, calculate_dividend_tax()
│
├── tax_statement/
│   ├── mod.rs                  # Main entry point with Germany routing
│   └── germany/
│       ├── mod.rs              # Module exports
│       ├── statement.rs        # GermanTaxStatement, entry types
│       ├── processor.rs        # process_broker_statement()
│       └── csv_formatter.rs    # GermanCsvFormatter
│
└── config.rs                   # TaxConfig with jurisdiction field
```

### Key Functions

1. **`generate_tax_statement()`** in `src/tax_statement/mod.rs`
   - Routes to `generate_german_tax_statement()` when jurisdiction is Germany

2. **`generate_german_tax_statement()`** in `src/tax_statement/mod.rs`
   - Creates GermanTaxStatement
   - Calls processor to populate entries
   - Writes CSV output

3. **`process_broker_statement()`** in `src/tax_statement/germany/processor.rs`
   - Iterates broker statement trades, dividends, interest
   - Converts to EUR using CurrencyConverter
   - Calculates taxes using German tax rates
   - Populates GermanTaxStatement

4. **`GermanCsvFormatter::write()`** in `src/tax_statement/germany/csv_formatter.rs`
   - Formats statement as CSV per contracts/csv-output.md

---

## German Tax Rates

| Component | Rate | Applied To |
|-----------|------|------------|
| Abgeltungssteuer | 25% | Capital gains, dividends, interest |
| Solidaritätszuschlag | 5.5% of Abgeltungssteuer | All taxable income |
| Kirchensteuer | 0-9% of Abgeltungssteuer | Optional, configurable |

### Effective Rates (No Church Tax)

- Total: 25% + (25% × 5.5%) = **26.375%**

### Effective Rates (9% Church Tax)

- Total: 25% + (25% × 5.5%) + (25% × 9%) = **28.625%**

---

## Teilfreistellung (Partial Exemption)

| Fund Type | Exemption Rate | Taxable Portion |
|-----------|----------------|-----------------|
| Regular Stocks | 0% | 100% |
| Equity ETF (>51% stocks) | 30% | 70% |
| Mixed ETF (25-51% stocks) | 15% | 85% |
| Bond ETF (<25% stocks) | 0% | 100% |

---

## Known Limitations

1. **ECB Exchange Rates**: The ECB rate provider is implemented but not yet integrated with error handling for missing rates
2. **Derivatives Warning**: FR-016 requires warning when derivatives are detected - not yet implemented
3. **Integration Tests**: Test fixtures for end-to-end testing not yet created
4. **Loss Carryforward**: Config fields exist but full tracking logic is in Phase 4

---

## Next Steps

1. **Complete T064**: Add graceful error handling for missing ECB rates
2. **Complete T065**: Add warning when derivatives detected in broker statement
3. **Create test fixtures**: Add sample broker statements and expected outputs
4. **Phase 4**: Implement loss carryforward tracking across years
5. **Phase 5**: Enhance ETF Teilfreistellung with automatic classification
6. **Phase 6**: Documentation, performance testing, cleanup

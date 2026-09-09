# Quickstart: Germany Tax Reports

**Audience**: Developers implementing Germany tax report functionality
**Prerequisites**: Familiarity with Rust, existing investments codebase
**Last Updated**: 2026-01-24

## Overview

This guide helps developers implement German tax report generation by following the existing Russia tax statement pattern. The feature adds Germany as a third jurisdiction (alongside Russia and USA) with support for Abgeltungssteuer, Solidaritätszuschlag, foreign tax credits, and ETF Teilfreistellung.

## Implementation Order

Follow this sequence to implement the feature incrementally:

### Phase 0: Foundation (Blocking Prerequisites)

**Goal**: Set up Germany jurisdiction and basic infrastructure

1. **Add Germany to Jurisdiction enum** (`src/localities.rs`)

   ```rust
   pub enum Jurisdiction {
       Russia,
       Usa,
       Germany,  // NEW
   }
   ```

2. **Implement `germany()` function** (`src/localities.rs`)
   - Follow `russia()` pattern
   - Year-specific tax rates (2009+)
   - Abgeltungssteuer 25%, Solidaritätszuschlag 5.5%

3. **Add ECB rate provider** (`src/forex.rs`)
   - Implement `EcbRateProvider` struct
   - XML API integration
   - Cache in existing `currency_rates` table

4. **Extend configuration** (`src/config.rs`)
   - Add `GermanyTaxConfig` struct
   - Church tax rate, loss carryforward, ETF classifications
   - Fallback mode (strict/lenient)

**Checkpoint**: Germany jurisdiction exists, ECB rates fetch, config loads

### Phase 1: User Story 1 - Generate German Tax Declaration (P1 - MVP)

**Goal**: Basic tax statement generation with capital gains and dividends

#### 1.1: Capital Gains Calculation

**Files**: `src/taxes/germany/capital_gains.rs`

1. Implement `FifoQueue` and `FifoLot` structs
2. Build FIFO queue from broker trades
3. Match sales against purchases (oldest first)
4. Calculate gain/loss per sale
5. Apply Teilfreistellung for ETFs
6. Detect pre-2009 holdings (Altbestand)

**Test**: Unit test with 3 purchases, 2 sales, verify FIFO matching

#### 1.2: Dividend Income Calculation

**Files**: `src/taxes/germany/dividends.rs`

1. Extract dividend payments from broker statement
2. Convert to EUR using ECB rates
3. Apply Teilfreistellung based on ETF classification
4. Calculate foreign tax credit

**Test**: Unit test with US dividend (15% withholding), ETF dividend (30% exemption)

#### 1.3: Tax Rate Calculation

**Files**: `src/taxes/germany/rates.rs`

1. Implement `GermanTaxRate` struct (implements `TaxRate` trait)
2. Calculate Abgeltungssteuer (25%)
3. Calculate Solidaritätszuschlag (5.5% of Abgeltungssteuer)
4. Calculate Kirchensteuer (0-9% of Abgeltungssteuer, config-driven)

**Test**: Unit test with known taxable amount, verify all tax components

#### 1.4: Statement Generation

**Files**: `src/tax_statement/germany/statement.rs`

1. Create `GermanTaxStatement` struct
2. Aggregate capital gains, dividends, interest
3. Calculate summary totals
4. Generate `CapitalGainEntry`, `DividendEntry`, `InterestEntry`

**Test**: Integration test with sample broker statement

#### 1.5: CSV Output

**Files**: `src/tax_statement/germany/csv_formatter.rs`

1. Implement CSV formatter per contract specification
2. Transaction rows with all required columns
3. Summary rows at end
4. Number formatting (2 decimals, thousands separator)

**Test**: Verify CSV matches contract, import into Excel

#### 1.6: CLI Integration

**Files**: `src/bin/investments/tax_statement.rs` (extend existing)

1. Accept `germany` as jurisdiction parameter
2. Wire up Germany tax statement generation
3. Output to specified file

**Test**: End-to-end test with real broker data

**Checkpoint**: `investments tax-statement ib 2025 output.csv --jurisdiction germany` works

### Phase 2: User Story 2 - Loss Carryforward (P2)

**Goal**: Support multi-year loss tracking

**Files**: `src/taxes/germany/loss_carryforward.rs`

1. Load loss carryforward from config
2. Apply against current year gains (oldest losses first)
3. Calculate remaining carryforward
4. Include in CSV summary section

**Test**: Unit test with €5,000 carryforward, €10,000 gain, verify €5,000 taxable

**Checkpoint**: Loss carryforward applied correctly, reported in output

### Phase 3: User Story 3 - Teilfreistellung for ETFs (P3)

**Goal**: Automatic ETF exemption application

**Files**: `src/instruments.rs` (extend), `src/taxes/germany/dividends.rs`, `src/taxes/germany/capital_gains.rs`

1. Extend `Instrument` with `etf_classification` field
2. Load classifications from config
3. Apply exemption in both dividend and capital gain calculations
4. Default to 0% if classification missing (conservative)

**Test**: Unit test equity ETF (30%), mixed ETF (15%), verify exemptions

**Checkpoint**: ETF exemptions apply automatically, logged when missing

## Key Implementation Patterns

### Error Handling

Use `GenericResult<T>` throughout:

```rust
pub fn calculate_capital_gain(sale: &Trade, fifo_queue: &mut FifoQueue) -> GenericResult<CapitalGainEntry> {
    if sale.quantity <= dec!(0) {
        return Err!("Invalid sale quantity: {}", sale.quantity);
    }
    // ... calculation
    Ok(entry)
}
```

### Currency Conversion

Always use ECB rates, never manual rates:

```rust
let proceeds_eur = converter.convert_to(
    settle_date,
    Cash::new(original_currency, amount),
    "EUR"
)?;
```

### FIFO Matching

Pop from front of queue (oldest lots first):

```rust
while remaining_quantity > dec!(0) {
    let lot = fifo_queue.lots.front_mut().ok_or("Insufficient shares")?;
    let matched_qty = min(remaining_quantity, lot.quantity);
    // ... process match
    remaining_quantity -= matched_qty;
    if lot.quantity == dec!(0) {
        fifo_queue.lots.pop_front();
    }
}
```

### Tax Calculation

Apply exemptions before tax:

```rust
let gross_gain = proceeds_eur - cost_basis_eur;
let exemption_rate = etf_classification.teilfreistellung_rate();
let taxable_amount = gross_gain * (dec!(1) - exemption_rate);
let abgeltungssteuer = taxable_amount * dec!(0.25);
let solidaritaetszuschlag = abgeltungssteuer * dec!(0.055);
let kirchensteuer = abgeltungssteuer * church_tax_rate;
```

### Debug Logging

Log all calculation steps:

```rust
debug!("[Capital Gain] {}: Selling {} shares", symbol, quantity);
debug!("[FIFO] Matched against lot from {}: {} shares @ €{}",
    lot.purchase_date, matched_qty, lot.cost_per_share_eur);
debug!("[Tax] Gross gain: €{}, Taxable: €{}, Tax: €{}",
    gross_gain, taxable_amount, total_tax);
```

## Testing Strategy

### Unit Tests

**Location**: `#[cfg(test)]` modules within each `.rs` file

**Coverage**:

- Tax rate calculations with known inputs
- FIFO matching with various lot scenarios
- Teilfreistellung application
- Foreign tax credit limits
- Pre-2009 holdings detection

**Example**:

```rust
#[test]
fn test_capital_gain_with_etf_exemption() {
    let mut fifo = FifoQueue::new("VWCE.DE");
    fifo.add_purchase(date!(2020, 1, 1), dec!(100), dec!(80));

    let sale = /* ... */;
    let entry = calculate_capital_gain(&sale, &mut fifo, EtfType::Equity).unwrap();

    assert_eq!(entry.teilfreistellung_rate, dec!(0.30));
    assert_eq!(entry.taxable_amount, dec!(700)); // €1000 gain * 0.70
}
```

### Integration Tests

**Location**: `tests/germany_tax_tests.rs`

**Coverage**:

- End-to-end statement generation
- Multiple transactions across year
- Loss carryforward application
- CSV output format validation

**Example**:

```rust
#[test]
fn test_generate_statement_with_sample_data() {
    let statement = /* load from testdata */;
    let config = /* test config */;

    let tax_statement = generate_germany_tax_statement(statement, config, 2025).unwrap();

    assert_eq!(tax_statement.capital_gains.len(), 5);
    assert_eq!(tax_statement.total_abgeltungssteuer, dec!(1234.56));
}
```

### Regression Tests

**Location**: `testdata/germany_tax/`

**Content**:

- Real broker statements (anonymized)
- Expected CSV outputs
- Test script in `./tests/run`

**Execution**:

```bash
./tests/run germany_tax_ib_2025
# Compares actual CSV output against expected
```

## Common Pitfalls

❌ **Don't**: Use `f64` for currency amounts
✅ **Do**: Always use `Decimal` type

❌ **Don't**: Unwrap Results in production code
✅ **Do**: Propagate errors with `?` operator

❌ **Don't**: Hard-code tax rates
✅ **Do**: Use year-specific rate maps

❌ **Don't**: Ignore settle vs trade date
✅ **Do**: Always use settle_date for tax calculations (FR-014)

❌ **Don't**: Apply Teilfreistellung to bonds
✅ **Do**: Check ETF classification, default to 0%

❌ **Don't**: Credit unlimited foreign tax
✅ **Do**: Cap credit at German tax on that income

## Development Checklist

Before submitting PR:

- [ ] All unit tests pass (`cargo test`)
- [ ] Integration tests pass (`./tests/run`)
- [ ] Clippy clean (`./check`)
- [ ] Debug logging present for all calculations
- [ ] CSV output matches contract specification
- [ ] Documentation updated (`docs/germany_tax.md`)
- [ ] Configuration example added to `docs/config-example.yaml`
- [ ] Testdata includes sample statement and expected output
- [ ] Altbestand warning emits when pre-2009 holdings detected
- [ ] Derivative warning emits when futures/options detected
- [ ] ECB rate fallback works (test with missing date)

## Reference Implementation

Study these existing files for patterns:

- `src/localities.rs`: `russia()` function
- `src/taxes/rates.rs`: Tax rate calculation patterns
- `src/tax_statement/statement/mod.rs`: Russia tax statement
- `src/forex.rs`: Currency conversion patterns

## Next Steps After Implementation

1. Add user documentation to `docs/germany_tax.md`
2. Update `README.md` with Germany support
3. Add configuration example to `docs/config-example.yaml`
4. Create GitHub issue for German tax software integration (future enhancement)
5. Consider adding Altbestand full support (future enhancement)

## Getting Help

- Review constitution: `.specify/memory/constitution.md`
- Check spec: `specs/001-germany-tax/spec.md`
- See data model: `specs/001-germany-tax/data-model.md`
- Review contracts: `specs/001-germany-tax/contracts/`

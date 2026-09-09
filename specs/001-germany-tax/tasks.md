# Tasks: Germany Tax Reports

**Feature**: Germany Tax Reports
**Branch**: `001-germany-tax`
**Input**: Design documents from `/specs/001-germany-tax/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/csv-output.md, quickstart.md

**Test Strategy**: Per Constitution Principle II (Test-First Development), ALL tests MUST be written FIRST, verified to FAIL (RED phase), then implementation makes them pass (GREEN phase), then refactored.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `- [ ] [ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure

- [X] T001 Checkout branch `001-germany-tax` and ensure clean working directory
- [X] T002 [P] Review constitution.md and ensure all 6 principles are understood
- [X] T003 [P] Review plan.md, spec.md, and data-model.md to understand scope

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### 2.1: Jurisdiction Setup

- [X] T004 Add `Germany` variant to `Jurisdiction` enum in src/localities.rs
- [X] T005 Implement `germany()` function in src/localities.rs following russia() pattern with year-specific tax rates (2009+: 25% Abgeltungssteuer, 5.5% Solidaritätszuschlag)

### 2.2: ECB Exchange Rate Provider

- [X] T006 Create `src/quotes/ecb.rs` module for ECB Statistical Data Warehouse integration
- [X] T007 Implement `EcbRateProvider` struct with XML API client in src/quotes/ecb.rs
- [X] T008 Add ECB rate caching to existing database infrastructure (reuse currency_rates table)
- [X] T009 [P] Register ECB provider in src/quotes/mod.rs
- [X] T010 [P] Add unit tests for ECB rate fetching and caching in src/quotes/ecb.rs (#[cfg(test)])

### 2.3: Configuration Extension

- [X] T011 Create `GermanTaxConfig` struct in src/config.rs with fields: jurisdiction (Germany), church_tax_rate, loss_carryforward_balance, ecb_fallback_mode
- [X] T012 [P] Add ETF classification support to config (isin → classification map)
- [X] T013 [P] Update config example in docs/config-example.yaml with Germany section

### 2.4: Core Germany Tax Module Structure

- [X] T014 Create directory src/taxes/germany/
- [X] T015 Create src/taxes/germany/mod.rs with public exports
- [X] T016 Create directory src/tax_statement/germany/
- [X] T017 Create src/tax_statement/germany/mod.rs with public exports
- [X] T018 Export germany module from src/taxes/mod.rs
- [X] T019 Export germany module from src/tax_statement/mod.rs

**Checkpoint**: Foundation ready - Germany jurisdiction exists, ECB rates work, config supports Germany, module structure in place

---

## Phase 3: User Story 1 - Generate German Tax Declaration (Priority: P1) 🎯 MVP

**Goal**: Generate complete German tax statement with capital gains, dividends, interest, foreign tax credits, and CSV output

**Independent Test**: Run `cargo run -- tax-statement ib 2025 german-tax-2025.csv` with German jurisdiction configured, producing complete tax report that can be verified against broker statements

### 3.1: Tests for User Story 1 (REQUIRED per Constitution) ✓

> **CRITICAL: Write these tests FIRST, ensure they FAIL, then implement**

#### Unit Tests

- [ ] T020 [P] [US1] Create tests/germany_tax_tests.rs for integration tests
- [X] T021 [P] [US1] Write FIFO lot matching unit tests in src/taxes/germany/capital_gains.rs (#[cfg(test)] - test buy 100, sell 50, verify lot remains)
- [X] T022 [P] [US1] Write capital gains calculation unit tests in src/taxes/germany/capital_gains.rs (#[cfg(test)] - test cost basis, proceeds, gain calculation)
- [X] T023 [P] [US1] Write Teilfreistellung unit tests in src/taxes/germany/dividends.rs (#[cfg(test)] - test 0%, 15%, 30% exemption)
- [X] T024 [P] [US1] Write foreign tax credit unit tests in src/taxes/germany/dividends.rs (#[cfg(test)] - test US 15% treaty)
- [X] T025 [P] [US1] Write tax rate calculation unit tests in src/taxes/germany/rates.rs (#[cfg(test)] - test 25% + 5.5% + church tax)

#### Integration Tests

- [ ] T026 [US1] Create testdata/germany_tax/sample_ib_statement.xml with test transactions (100 AAPL buy, 50 AAPL sell, dividends)
- [ ] T027 [US1] Write integration test in tests/germany_tax_tests.rs: parse statement → generate tax report → verify totals
- [ ] T028 [US1] Create testdata/germany_tax/expected_output_us1.csv with expected CSV output for integration test

#### Test Verification

- [ ] T029 [US1] Run `cargo test germany` and verify ALL tests FAIL (RED phase confirmed)

### 3.2: Data Model Implementation

- [X] T030 [P] [US1] Create `FifoLot` struct in src/taxes/germany/capital_gains.rs
- [X] T031 [P] [US1] Create `FifoQueue` struct in src/taxes/germany/capital_gains.rs with VecDeque<FifoLot>
- [X] T032 [P] [US1] Create `CapitalGainEntry` struct in src/tax_statement/germany/statement.rs
- [X] T033 [P] [US1] Create `DividendEntry` struct in src/tax_statement/germany/statement.rs
- [X] T034 [P] [US1] Create `InterestEntry` struct in src/tax_statement/germany/statement.rs
- [X] T035 [US1] Create `GermanTaxStatement` struct in src/tax_statement/germany/statement.rs with all entry vectors and summary fields

### 3.3: FIFO Capital Gains Calculation

- [X] T036 [US1] Implement `FifoQueue::add_purchase()` in src/taxes/germany/capital_gains.rs
- [X] T037 [US1] Implement `FifoQueue::match_sale()` returning matched lots and cost basis in src/taxes/germany/capital_gains.rs
- [X] T038 [US1] Implement `calculate_capital_gain()` function in src/taxes/germany/capital_gains.rs
- [X] T039 [US1] Add Altbestand (pre-2009) detection and tax exemption logic in src/taxes/germany/capital_gains.rs
- [X] T040 [US1] Add debug logging for FIFO matching steps in src/taxes/germany/capital_gains.rs

### 3.4: Dividend Income Calculation

- [X] T041 [P] [US1] Implement `calculate_dividend_income()` function in src/taxes/germany/dividends.rs
- [X] T042 [P] [US1] Implement Teilfreistellung lookup (0% stocks, 30% equity ETF, 15% mixed ETF) in src/taxes/germany/dividends.rs
- [X] T043 [US1] Implement foreign tax credit calculation (min of foreign tax, German tax) in src/taxes/germany/dividends.rs
- [X] T044 [US1] Add debug logging for dividend calculations in src/taxes/germany/dividends.rs

### 3.5: Tax Rate Calculation

- [X] T045 [US1] Create src/taxes/germany/rates.rs module
- [X] T046 [US1] Implement `GermanTaxRates` struct with abgeltungssteuer_rate, soli_rate, church_tax_rate in src/taxes/germany/rates.rs
- [X] T047 [US1] Implement `calculate_taxes()` function (Abgeltungssteuer + Soli + Kirchensteuer) in src/taxes/germany/rates.rs
- [X] T048 [US1] Add debug logging for tax calculations in src/taxes/germany/rates.rs

### 3.6: Tax Statement Aggregation

- [X] T049 [US1] Implement `GermanTaxStatement::new()` in src/tax_statement/germany/statement.rs
- [X] T050 [US1] Implement `add_capital_gain()` method in src/tax_statement/germany/statement.rs
- [X] T051 [US1] Implement `add_dividend()` method in src/tax_statement/germany/statement.rs
- [X] T052 [US1] Implement `add_interest()` method in src/tax_statement/germany/statement.rs
- [X] T053 [US1] Implement `calculate_totals()` method summing all taxes in src/tax_statement/germany/statement.rs
- [X] T054 [US1] Add validation ensuring all amounts use Decimal type with €0.01 precision in src/tax_statement/germany/statement.rs

### 3.7: CSV Output Formatting

- [X] T055 [US1] Create src/tax_statement/germany/csv_formatter.rs module
- [X] T056 [US1] Implement CSV header generation per contracts/csv-output.md (19 columns) in src/tax_statement/germany/csv_formatter.rs
- [X] T057 [US1] Implement `format_capital_gain_row()` in src/tax_statement/germany/csv_formatter.rs
- [X] T058 [US1] Implement `format_dividend_row()` in src/tax_statement/germany/csv_formatter.rs
- [X] T059 [US1] Implement `format_interest_row()` in src/tax_statement/germany/csv_formatter.rs
- [X] T060 [US1] Implement summary row generation (10 summary rows) in src/tax_statement/germany/csv_formatter.rs
- [X] T061 [US1] Add CSV quoting for fields with commas/newlines in src/tax_statement/germany/csv_formatter.rs

### 3.8: CLI Integration

- [X] T062 [US1] Extend `tax-statement` subcommand in src/bin/investments/main.rs to support Germany jurisdiction
- [X] T063 [US1] Wire GermanTaxStatement generation into tax_statement CLI flow in src/bin/investments/tax_statement.rs (create if not exists)
- [X] T064 [US1] Add error handling for missing ECB rates (respect strict/lenient fallback mode) in src/bin/investments/tax_statement.rs
- [X] T065 [US1] Add warning messages for detected derivatives (FR-016) in src/bin/investments/tax_statement.rs

### 3.9: Test Validation & Refinement

- [X] T066 [US1] Run `cargo test germany` and verify ALL tests PASS (GREEN phase achieved)
- [ ] T067 [US1] Run integration test with testdata and verify CSV matches expected output
- [ ] T068 [US1] Test with real Interactive Brokers statement (if available) and verify calculations
- [X] T069 [US1] Refactor code for clarity while keeping tests green (REFACTOR phase)

**Checkpoint**: User Story 1 complete - Can generate German tax statement with capital gains, dividends, interest, and CSV output

---

## Phase 4: User Story 2 - Multi-Year Loss Carryforward (Priority: P2)

**Goal**: Track and apply capital loss carryforwards from previous years to reduce current year tax liability

**Independent Test**: Configure loss carryforward in config, generate tax statement, verify losses applied to gains and remaining balance reported

### 4.1: Tests for User Story 2 (REQUIRED per Constitution) ✓

- [X] T070 [P] [US2] Write loss carryforward application unit tests in src/taxes/germany/loss_carryforward.rs (#[cfg(test)] - test €5,000 CF against €10,000 gain = €5,000 taxable)
- [X] T071 [P] [US2] Write loss carryforward calculation unit tests in src/taxes/germany/loss_carryforward.rs (#[cfg(test)] - test current year loss > gain = new CF balance)
- [ ] T072 [US2] Create testdata/germany_tax/expected_output_us2_with_cf.csv with loss carryforward scenario
- [ ] T073 [US2] Write integration test in tests/germany_tax_tests.rs for loss carryforward workflow
- [X] T074 [US2] Verify all US2 tests FAIL (RED phase)

### 4.2: Loss Carryforward Implementation

- [X] T075 [P] [US2] Create `LossCarryforward` struct in src/tax_statement/germany/statement.rs
- [X] T076 [US2] Create src/taxes/germany/loss_carryforward.rs module
- [X] T077 [US2] Implement `apply_loss_carryforward()` function in src/taxes/germany/loss_carryforward.rs
- [X] T078 [US2] Implement `calculate_new_carryforward()` function in src/taxes/germany/loss_carryforward.rs
- [X] T079 [US2] Extend `GermanTaxStatement` with loss_carryforward_used and loss_carryforward_remaining fields in src/tax_statement/germany/statement.rs
- [X] T080 [US2] Load loss carryforward from config in src/tax_statement/germany/statement.rs
- [X] T081 [US2] Update CSV summary rows to include loss carryforward lines in src/tax_statement/germany/csv_formatter.rs
- [X] T082 [US2] Add debug logging for loss carryforward calculations in src/taxes/germany/loss_carryforward.rs

### 4.3: Test Validation

- [X] T083 [US2] Run `cargo test germany` and verify US2 tests PASS (GREEN phase)
- [ ] T084 [US2] Test multi-year scenario: 2024 loss → 2025 gain → verify CF applied
- [X] T085 [US2] Refactor loss carryforward code (REFACTOR phase)

**Checkpoint**: User Story 2 complete - Loss carryforwards work correctly across tax years

---

## Phase 5: User Story 3 - Teilfreistellung for ETFs (Priority: P3)

**Goal**: Automatically apply partial exemptions (30% equity, 15% mixed) for ETF dividends and capital gains

**Independent Test**: Configure ETFs with classifications, generate tax statement, verify exemptions correctly reduce taxable amounts

### 5.1: Tests for User Story 3 (REQUIRED per Constitution) ✓

- [X] T086 [P] [US3] Write equity ETF exemption unit tests in src/taxes/germany/dividends.rs (#[cfg(test)] - test €1,000 dividend × 30% = €300 exempt)
- [X] T087 [P] [US3] Write mixed ETF exemption unit tests in src/taxes/germany/capital_gains.rs (#[cfg(test)] - test €1,000 gain × 15% = €150 exempt)
- [ ] T088 [US3] Create testdata/germany_tax/expected_output_us3_etf.csv with ETF Teilfreistellung scenarios
- [ ] T089 [US3] Write integration test in tests/germany_tax_tests.rs for ETF exemptions
- [X] T090 [US3] Verify all US3 tests FAIL (RED phase)

### 5.2: ETF Classification Implementation

- [X] T091 [P] [US3] Create `EtfClassification` enum (Equity, Mixed, Bond) in src/instruments.rs
- [X] T092 [P] [US3] Create `InstrumentMetadata` struct with etf_classification field in src/instruments.rs
- [X] T093 [US3] Implement ETF classification lookup by ISIN from config in src/instruments.rs
- [X] T094 [US3] Add default classification (0% exemption) for unknown ISINs in src/instruments.rs

### 5.3: Teilfreistellung Application

- [X] T095 [US3] Implement `get_teilfreistellung_rate()` function (30%/15%/0%) in src/taxes/germany/dividends.rs
- [X] T096 [US3] Update `calculate_dividend_income()` to apply Teilfreistellung in src/taxes/germany/dividends.rs
- [X] T097 [US3] Update `calculate_capital_gain()` to apply Teilfreistellung for ETFs in src/taxes/germany/capital_gains.rs
- [X] T098 [US3] Update CSV output to show teilfreistellung_pct column in src/tax_statement/germany/csv_formatter.rs
- [X] T099 [US3] Add debug logging for Teilfreistellung application in src/taxes/germany/dividends.rs

### 5.4: Test Validation

- [X] T100 [US3] Run `cargo test germany` and verify US3 tests PASS (GREEN phase)
- [ ] T101 [US3] Test with real ETF transactions (VWCE.DE equity, mixed bond ETFs)
- [X] T102 [US3] Refactor ETF classification code (REFACTOR phase)

**Checkpoint**: User Story 3 complete - ETF Teilfreistellung works for both dividends and capital gains

---

## Phase 5b: User Story 4 - Additional Income Types (Priority: P4)

**Goal**: Support fees, stock grants (RSUs), cash grants, and corporate actions in Germany tax reporting

**Independent Test**: Generate tax statement with fees, grants, and corporate actions, verify correct tax treatment

### 5b.1: Broker Fees Support

Broker fees are deductible from capital gains (Anschaffungsnebenkosten / Werbungskosten).

- [X] T117 [P] [US4] Write broker fees unit tests in src/tax_statement/germany/processor.rs (#[cfg(test)] - test fee deduction reduces taxable gain)
- [X] T118 [US4] Implement `process_fees()` function in src/tax_statement/germany/processor.rs
- [X] T119 [US4] Create `FeeEntry` struct in src/tax_statement/germany/statement.rs with date, amount_eur, description
- [X] T120 [US4] Add `fees` vector to `GermanTaxStatement` struct in src/tax_statement/germany/statement.rs
- [X] T121 [US4] Update CSV formatter to output fee rows in src/tax_statement/germany/csv_formatter.rs
- [X] T122 [US4] Update summary calculations to include fee deductions in src/tax_statement/germany/statement.rs
- [X] T123 [US4] Run `cargo test germany` and verify fee tests PASS

### 5b.2: Stock Grants (RSUs) Support

Stock grants in Germany have TWO taxable events:

1. At vesting: Taxed as employment income (geldwerter Vorteil) at marginal income tax rate
2. At sale: Capital gains tax only on gain above vest-date FMV

The current code converts grants to zero-cost StockBuys which is INCORRECT for Germany.
For Germany, the cost basis should be the FMV at vest date (which IBKR provides).

- [X] T124 [P] [US4] Write stock grant unit tests in src/tax_statement/germany/processor.rs (#[cfg(test)] - test vest value as income, FMV as cost basis)
- [X] T125 [US4] Create `StockGrantEntry` struct in src/tax_statement/germany/statement.rs with vest_date, symbol, quantity, fmv_per_share, fmv_total_eur
- [X] T126 [US4] Implement `process_stock_grants()` function in src/tax_statement/germany/processor.rs
- [X] T127 [US4] Add `stock_grants` vector to `GermanTaxStatement` in src/tax_statement/germany/statement.rs
- [X] T128 [US4] Report vest-date FMV as employment income (separate from Abgeltungssteuer income) in summary
- [X] T129 [US4] Ensure stock grant sales use FMV at vest as cost basis (not zero) - verify IBKR cost basis
- [X] T130 [US4] Update CSV formatter to output stock grant rows (employment income section) in src/tax_statement/germany/csv_formatter.rs
- [X] T131 [US4] Add warning if grant FMV is missing/zero in src/tax_statement/germany/processor.rs
- [X] T132 [US4] Run `cargo test germany` and verify stock grant tests PASS

### 5b.3: Cash Grants Support

Cash bonuses from brokers are "sonstige Einkünfte" (other income) if >€256/year, taxed at marginal rate.

- [X] T133 [P] [US4] Write cash grant unit tests in src/tax_statement/germany/processor.rs (#[cfg(test)] - test cash grant as other income)
- [X] T134 [US4] Create `CashGrantEntry` struct in src/tax_statement/germany/statement.rs with date, amount_eur, description
- [X] T135 [US4] Implement `process_cash_grants()` function in src/tax_statement/germany/processor.rs
- [X] T136 [US4] Add `cash_grants` vector to `GermanTaxStatement` in src/tax_statement/germany/statement.rs
- [X] T137 [US4] Update CSV formatter with cash grant rows (other income section) in src/tax_statement/germany/csv_formatter.rs
- [X] T138 [US4] Add note in output that cash grants >€256/year are taxable as sonstige Einkünfte
- [X] T139 [US4] Run `cargo test germany` and verify cash grant tests PASS

### 5b.4: Corporate Actions Support

Corporate actions (spinoffs, liquidations, mergers) may trigger tax events or require cost basis allocation.

- [X] T140 [P] [US4] Write corporate action unit tests in src/tax_statement/germany/processor.rs (#[cfg(test)] - test spinoff cost basis allocation, liquidation gain/loss)
- [X] T141 [US4] Create `CorporateActionEntry` struct in src/tax_statement/germany/statement.rs with date, action_type, symbol, details, tax_impact_eur
- [X] T142 [US4] Implement `process_corporate_actions()` function in src/tax_statement/germany/processor.rs
- [X] T143 [US4] Handle spinoffs: Allocate cost basis based on market values at spinoff date
- [X] T144 [US4] Handle liquidations: Report as capital gain/loss event
- [X] T145 [US4] Handle stock splits: No tax event, already handled by StockSplitController (verify)
- [X] T146 [US4] Add `corporate_actions` vector to `GermanTaxStatement` in src/tax_statement/germany/statement.rs
- [X] T147 [US4] Update CSV formatter with corporate action rows in src/tax_statement/germany/csv_formatter.rs
- [X] T148 [US4] Add warning for unsupported corporate action types
- [X] T149 [US4] Run `cargo test germany` and verify corporate action tests PASS

### 5b.5: Integration & Validation

- [X] T150 [US4] Wire all new processors into `process_broker_statement()` in src/tax_statement/germany/processor.rs
- [X] T151 [US4] Update `GermanTaxStatement::calculate_totals()` to include all new entry types
- [X] T152 [US4] Run full `cargo test` and verify all tests pass
- [X] T153 [US4] Run `cargo clippy -- -Dwarnings` and fix any warnings
- [X] T154 [US4] Update docs/germany_tax.md with information about fees, grants, and corporate actions

**Checkpoint**: User Story 4 complete - Fees, stock grants, cash grants, and corporate actions properly handled for German taxes

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories and final quality checks

- [X] T103 [P] Create docs/germany_tax.md user documentation with setup instructions and examples
- [X] T104 [P] Update README.md to mention Germany tax support
- [X] T105 [P] Add example Germany configuration to docs/config-example.yaml
- [X] T106 Run full test suite: `cargo test`
- [X] T107 Run clippy with -Dwarnings: `cargo clippy -- -Dwarnings`
- [X] T108 Run formatting check: `cargo fmt -- --check`
- [X] T109 Performance test: Verify <10s for 1000 transaction statement per SC-001
- [X] T110 Accuracy test: Verify ±€0.01 precision across all calculations per SC-002
- [X] T111 [P] Review all debug logging for completeness per FR-015
- [X] T112 [P] Review error messages for clarity (ECB rate failures, missing config, etc.)
- [ ] T113 Create regression test suite in testdata/germany_tax/ with various broker formats
- [X] T114 Run quickstart.md validation (verify implementation matches guide)
- [X] T115 Code cleanup: Remove any debug prints, TODOs, or temporary code
- [X] T116 Final constitution compliance check (all 6 principles)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - start immediately
- **Foundational (Phase 2)**: Depends on Setup - **BLOCKS all user stories**
- **User Story 1 (Phase 3)**: Depends on Foundational completion
- **User Story 2 (Phase 4)**: Depends on Foundational completion (can start in parallel with US1 if multiple developers)
- **User Story 3 (Phase 5)**: Depends on Foundational completion (can start in parallel with US1/US2 if multiple developers)
- **Polish (Phase 6)**: Depends on desired user stories being complete

### User Story Dependencies

**User Story 1 (P1 - MVP)**:

- **Depends on**: Phase 2 (Foundational) complete
- **Blocks**: Nothing - other stories can proceed in parallel
- **Must complete before deployment**: Yes (this is the MVP)

**User Story 2 (P2 - Loss Carryforward)**:

- **Depends on**: Phase 2 (Foundational) complete
- **Blocks**: Nothing
- **Can integrate with**: US1 (reads same capital gains, modifies taxable amounts)
- **Must complete before deployment**: Optional (nice-to-have, not MVP)

**User Story 3 (P3 - Teilfreistellung)**:

- **Depends on**: Phase 2 (Foundational) complete
- **Blocks**: Nothing
- **Can integrate with**: US1 (modifies dividend and capital gain calculations)
- **Must complete before deployment**: Optional (nice-to-have for ETF investors)

### Critical Path (MVP)

To deliver minimum viable product (User Story 1 only):

1. Phase 1: Setup (T001-T003) - ~1 hour
2. Phase 2: Foundational (T004-T019) - **MUST COMPLETE** - ~1 day
3. Phase 3: User Story 1 (T020-T069) - ~2-3 days
4. Phase 6: Polish (T103-T116) - ~0.5 day

**Total MVP Estimate**: 4-5 days

### Parallel Opportunities

**Within Phase 2 (Foundational)**:

- T009 (register ECB), T010 (ECB tests) can run in parallel after T007-T008
- T012 (ETF config), T013 (config example) can run in parallel after T011

**Within Phase 3.1 (US1 Tests)**:

- T020-T025 (all unit test files) can be written in parallel
- T026, T028 (test fixtures) can be created in parallel

**Within Phase 3.2 (US1 Data Models)**:

- T030-T034 (all entity structs) can be created in parallel

**Within Phase 3.7 (US1 CSV Output)**:

- T057-T059 (all format row functions) can be written in parallel

**Within Phase 6 (Polish)**:

- T103-T105 (documentation) can be written in parallel
- T111-T112 (code review tasks) can be done in parallel

**Cross-User Stories** (if multiple developers available):

- After Phase 2 completes: US1 (Phase 3), US2 (Phase 4), US3 (Phase 5) can all start in parallel
- Each user story is independently testable and deliverable

---

## Parallel Example: Phase 3 (User Story 1)

```bash
# Step 1: Write all unit tests in parallel (T020-T025)
Developer A: Write FIFO tests in capital_gains.rs
Developer B: Write Teilfreistellung tests in dividends.rs
Developer C: Write tax rate tests in rates.rs

# Step 2: Create all data model structs in parallel (T030-T034)
Developer A: Create FifoLot, FifoQueue
Developer B: Create CapitalGainEntry, DividendEntry
Developer C: Create InterestEntry, GermanTaxStatement

# Step 3: Implement calculations (sequential within area, parallel across areas)
Developer A: FIFO implementation (T036-T040)
Developer B: Dividend calculation (T041-T044)
Developer C: Tax rates (T045-T048)
```

---

## Implementation Strategy

### MVP First (Recommended)

**Goal**: Deliver working Germany tax statement as fast as possible

1. ✅ Complete Phase 1: Setup
2. ✅ Complete Phase 2: Foundational (CRITICAL - blocks everything)
3. ✅ Complete Phase 3: User Story 1 (MVP)
4. 🎯 **STOP and VALIDATE**:
   - Run integration tests
   - Test with real broker statement
   - Verify CSV output matches expectations
5. ✅ Complete Phase 6: Polish (documentation, performance, cleanup)
6. 🚀 **Deploy/Demo MVP** - Users can generate German tax statements!
7. Optional: Add Phase 4 (Loss CF) and Phase 5 (ETFs) later

### Incremental Delivery

**Goal**: Deliver value with each user story

1. Foundation (Phase 1 + 2) → Test infrastructure works
2. + User Story 1 (Phase 3) → **MVP RELEASE** - Basic tax statements
3. + User Story 2 (Phase 4) → **V1.1 RELEASE** - Loss carryforward support
4. + User Story 3 (Phase 5) → **V1.2 RELEASE** - Full ETF exemptions
5. + Polish (Phase 6) → **V1.3 RELEASE** - Production-ready

### Parallel Team Strategy

**Goal**: Maximize throughput with multiple developers

**Week 1**:

- All developers: Phase 1 + 2 together (foundation)

**Week 2** (after foundation complete):

- Developer A: User Story 1 (capital gains, dividends, CSV)
- Developer B: User Story 2 (loss carryforward)
- Developer C: User Story 3 (ETF Teilfreistellung)

**Week 3**:

- All developers: Integration testing, polish, documentation
- Stories merge independently, tested independently

---

## Success Metrics

After completing all tasks, verify these outcomes from spec.md:

- **SC-001**: Performance < 10s for complete tax statement → Run performance test T109
- **SC-002**: Calculations accurate within €0.01 → Run accuracy test T110
- **SC-003**: Foreign tax credits correct for major treaties → Verify in integration tests T027
- **SC-004**: CSV contains all required fields → Validate against contracts/csv-output.md in T067
- **SC-005**: Loss carryforward 100% accurate → Verify in US2 integration test T084

---

## Notes

- **Test-First**: Constitution Principle II is NON-NEGOTIABLE - write tests first, see them fail, then implement
- **[P] marker**: Indicates parallelizable tasks (different files, no blocking dependencies)
- **[Story] label**: Maps task to user story for traceability (US1 = MVP, US2 = Loss CF, US3 = ETFs)
- **File paths**: All tasks include exact file paths for clarity
- **Decimal precision**: All currency amounts MUST use `Decimal` type (never float) per Principle I
- **Checkpoints**: Stop at each checkpoint to validate before proceeding
- **MVP scope**: Phase 3 (User Story 1) is minimum viable product
- **Constitution**: Review all 6 principles before starting (T002)
- **clippy enforcement**: Must pass `cargo clippy -- -Dwarnings` (T107)

**Total Tasks**: 154
**Critical Path Tasks** (MVP): ~50 tasks (Setup + Foundational + US1 + Core Polish)
**Estimated MVP Duration**: 4-5 days with test-first development
**Estimated Full Feature**: 8-10 days including all user stories (US1-US4)

# Implementation Plan: Germany Tax Reports

**Branch**: `001-germany-tax` | **Date**: 2026-01-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-germany-tax/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Add comprehensive German tax report generation for investors with foreign brokerage accounts. The system will calculate Abgeltungssteuer (25% capital gains tax), Solidaritätszuschlag (5.5% solidarity surcharge), apply foreign tax credits under double taxation treaties, handle loss carryforwards, and support Teilfreistellung (partial exemptions) for ETFs. Output will be comprehensive CSV format with all transaction details and tax calculations using ECB exchange rates, following the existing pattern established for Russia tax statements.

## Technical Context

**Language/Version**: Rust 2024 edition (as configured in Cargo.toml)
**Primary Dependencies**:

- `rust_decimal` for precise currency calculations
- `chrono` for date handling
- `diesel` for SQLite database (forex rate caching)
- Existing `src/forex.rs` for currency conversion infrastructure
- Existing `src/taxes/` module patterns for tax calculations

**Storage**: SQLite database (`~/.investments/db.sqlite`) for ECB exchange rate caching
**Testing**: `cargo test` for unit tests, `./tests/run` for regression tests with testdata fixtures
**Target Platform**: Linux, macOS, Windows (cross-platform Rust CLI tool)
**Project Type**: Single project (CLI tool with library modules)
**Performance Goals**: < 10 seconds for complete tax statement generation (matching Russia tax statement performance)
**Constraints**:

- Tax calculations must use `Decimal` type (never float) for precision
- All amounts rounded to Euro cents (€0.01 precision)
- FIFO cost basis tracking required by German tax law
- Year-specific tax rates support (Abgeltungssteuer 25%, Solidaritätszuschlag 5.5% may change over time)

**Scale/Scope**: Support portfolios with up to 10,000 transactions per tax year

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

This feature MUST comply with all constitutional principles:

- [x] **Code Quality & Type Safety**: Plan includes proper error handling strategy using `Result<T, E>` types, `Decimal` for all currency amounts, no unwrapping in production paths
- [x] **Test-First Development**: Test strategy defined - unit tests for tax calculation logic, integration tests for end-to-end tax statement generation, regression tests with real broker data in testdata/
- [x] **User Experience Consistency**: CLI interface follows existing `tax-statement` command pattern, CSV output format specified with comprehensive columns
- [x] **Performance & Efficiency**: Performance target < 10 seconds specified (matches existing Russia tax statement), ECB rate caching infrastructure planned
- [x] **Data Integrity & Validation**: Input validation at parser boundaries (FR-009, FR-016 for derivatives), EUR precision to €0.01, FIFO cost basis tracking
- [x] **Multi-Broker Support**: Extends existing `src/localities.rs` pattern, reuses existing broker statement parsers, adds Germany jurisdiction following Russia/USA model

**Justification for any principle modifications**: None - this feature fully adheres to all constitutional principles and extends existing patterns.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── localities.rs              # Add Jurisdiction::Germany with tax rates by year
├── forex.rs                   # ECB rate provider integration (extend existing)
├── taxes/
│   ├── rates.rs              # Reuse existing TaxRate trait infrastructure
│   ├── germany/              # NEW: Germany-specific tax calculation logic
│   │   ├── mod.rs
│   │   ├── rates.rs          # Abgeltungssteuer + Solidarit\u00e4tszuschlag calculators
│   │   ├── capital_gains.rs  # Capital gains calculation with FIFO
│   │   ├── dividends.rs      # Dividend income with Teilfreistellung
│   │   ├── loss_carryforward.rs
│   │   └── foreign_tax_credit.rs
│   └── mod.rs                # Export germany module
├── tax_statement/
│   ├── germany/              # NEW: Germany tax statement generation
│   │   ├── mod.rs
│   │   ├── statement.rs      # Main GermanTaxStatement struct
│   │   ├── csv_formatter.rs  # CSV output with comprehensive columns
│   │   └── config.rs         # Germany-specific config (church tax %, etc.)
│   └── mod.rs                # Export germany module
├── instruments.rs            # Extend with ETF classification metadata
└── config.rs                 # Extend with Germany tax config section

tests/
├── germany_tax_tests.rs      # Integration tests for tax calculations
└── testdata/
    └── germany_tax/          # NEW: Test fixtures
        ├── sample_statements/
        └── expected_outputs/

docs/
└── germany_tax.md            # NEW: User documentation for German tax reports
```

**Structure Decision**: Single project structure (Option 1 from template). This feature extends the existing Rust CLI tool by adding Germany as a new jurisdiction alongside Russia and USA. The implementation follows established patterns: jurisdiction definition in `localities.rs`, tax calculation logic in `src/taxes/germany/`, and tax statement generation in `src/tax_statement/germany/`.

## Complexity Tracking

> No constitutional violations - this section intentionally left empty. All complexity additions are justified by German tax law requirements and follow existing patterns.

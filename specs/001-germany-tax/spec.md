# Feature Specification: Germany Tax Reports

**Feature Branch**: `001-germany-tax`
**Created**: 2026-01-24
**Status**: Draft
**Input**: User description: "Add Germany tax reports to this project"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Generate German Tax Declaration (Priority: P1)

German residents with foreign brokerage accounts must file annual tax declarations reporting capital gains, dividend income, and interest. Users need to generate compliant tax documentation with accurate calculations of taxable income, withholding tax credits, and final tax liability according to German tax law.

**Why this priority**: Core functionality that enables the tool to support German tax residents, completing the multi-jurisdiction support alongside existing Russia and USA support. Without this, German investors cannot use the tool for tax compliance.

**Independent Test**: Can be fully tested by running `investments tax-statement [broker] 2025 [output-file]` with German jurisdiction configured, producing a complete tax report with capital gains, dividends, and tax calculations that can be verified against broker statements.

**Acceptance Scenarios**:

1. **Given** a German tax resident has brokerage accounts with transactions in the tax year, **When** they run `investments tax-statement ib 2025 german-tax-2025.csv`, **Then** the system generates a CSV report with all capital gains (Kursgewinne), dividend income (Dividenden), interest income (Zinsen), and foreign withholding tax credits (Quellensteuer).

2. **Given** the user has dividend income from US stocks with 15% withholding tax, **When** the tax statement is generated, **Then** the system correctly calculates the withholding tax credit available under the Germany-USA double taxation treaty.

3. **Given** the user sold stocks with both gains and losses, **When** the tax statement is generated, **Then** the system correctly nets gains and losses and calculates the Abgeltungssteuer (25% capital gains tax) plus Solidaritätszuschlag (5.5% solidarity surcharge).

---

### User Story 2 - Multi-Year Loss Carryforward (Priority: P2)

German tax law allows investors to carry forward capital losses from previous years to offset against current year gains. Users need the system to track and apply loss carryforwards across tax years.

**Why this priority**: Important for accurate tax calculations but can be implemented after basic tax statement generation. Many users will have loss carryforwards from previous years that significantly reduce their tax liability.

**Independent Test**: Can be tested by configuring loss carryforwards in the configuration file and verifying they are properly applied to current year gains in the generated tax statement.

**Acceptance Scenarios**:

1. **Given** a user has €5,000 in capital loss carryforward from 2024, **When** they generate a 2025 tax statement with €10,000 in capital gains, **Then** the system calculates tax on only €5,000 of gains and reports the utilized loss carryforward.

2. **Given** a user has losses in the current year exceeding gains, **When** the tax statement is generated, **Then** the system reports the net loss available for carryforward to future years.

---

### User Story 3 - Teilfreistellung for ETFs (Priority: P3)

German tax law provides partial exemption (Teilfreistellung) for certain ETF types - equity ETFs get 30% exemption, mixed ETFs get 15% exemption. Users need the system to automatically apply these exemptions based on ETF classification.

**Why this priority**: Important for ETF investors but requires additional instrument metadata. Can be implemented after core tax calculations are working. Provides tax savings but not required for basic compliance.

**Independent Test**: Can be tested by configuring ETFs with their classification (equity/mixed) and verifying the partial exemption is correctly applied in dividend and capital gain calculations.

**Acceptance Scenarios**:

1. **Given** a user holds equity ETFs and receives €1,000 in dividends, **When** the tax statement is generated, **Then** the system applies 30% Teilfreistellung, taxing only €700 of the dividend income.

2. **Given** a user sells a mixed ETF with €1,000 capital gain, **When** the tax statement is generated, **Then** the system applies 15% Teilfreistellung, taxing only €850 of the gain.

---

### Out of Scope

- **Derivative instruments**: Futures, options, warrants, and other Termingeschäfte are excluded from initial implementation due to specialized tax treatment. System will emit warnings if such instruments are detected in broker statements.
- **Crypto assets**: Cryptocurrency holdings and transactions are not supported (different tax treatment under private sale rules).
- **Real estate and precious metals**: Physical assets outside of securities scope.

---

### Edge Cases

- What happens when broker statement contains transactions in multiple currencies requiring EUR conversion at ECB reference rates?
- How does the system handle stock splits and corporate actions in the cost basis calculation for German tax purposes?
- What validation errors occur when dividend income lacks sufficient data to determine the foreign withholding tax amount?
- How should the system behave when the ECB exchange rate API is unavailable during tax calculation (use cached rates) or when a transaction date has no cached rate (use configurable fallback: strict mode fails, lenient mode uses nearest previous business day)?
- What happens when a user has both long-term holdings (pre-2009 with different tax treatment) and post-2009 holdings of the same security?
- How does the system handle partial Teilfreistellung for ETFs with changing classifications over time?
- What happens when loss carryforward data is missing or inconsistent with previous year's tax statement?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST add Germany as a supported jurisdiction in `src/localities.rs` with EUR currency and appropriate tax rates
- **FR-002**: System MUST calculate Abgeltungssteuer (25% capital gains tax) on all capital gains from stock and ETF sales
- **FR-003**: System MUST calculate Solidaritätszuschlag (5.5% solidarity surcharge on Abgeltungssteuer) for applicable income levels
- **FR-004**: System MUST calculate and report dividend income with applicable foreign withholding tax credits based on double taxation treaties
- **FR-005**: System MUST convert all income amounts to EUR using ECB (European Central Bank) reference rates for the transaction date, cached locally similar to existing quote providers
- **FR-006**: System MUST track and calculate cost basis using FIFO (First-In-First-Out) method as required by German tax law
- **FR-007**: System MUST apply loss carryforward from previous years against current year capital gains when configured
- **FR-008**: System MUST generate tax statement output in CSV format with comprehensive columns: transaction date, settlement date, symbol, ISIN, quantity, cost basis (EUR), proceeds (EUR), gain/loss (EUR), foreign withholding tax (EUR), Teilfreistellung percentage, taxable amount (EUR), Abgeltungssteuer (EUR), Solidaritätszuschlag (EUR), Kirchensteuer (EUR if applicable), total German tax (EUR), and transaction description
- **FR-009**: System MUST validate that all transactions have complete data (dates, amounts, symbols) required for German tax calculations
- **FR-010**: Users MUST be able to configure their tax jurisdiction as Germany in the configuration file
- **FR-011**: System MUST apply Teilfreistellung (partial exemption) for ETFs when ETF classification metadata is available
- **FR-012**: System MUST handle Kirchensteuer (church tax) as an optional percentage applied to Abgeltungssteuer when configured by user
- **FR-013**: System MUST distinguish between pre-2009 holdings (exempt from Abgeltungssteuer under Altbestand rules) and post-2009 holdings
- **FR-014**: System MUST use transaction settle date (not trade date) for all tax calculations
- **FR-015**: System MUST provide debug-level logging for all tax calculations including cost basis determination, currency conversions, FIFO matching, and final tax amounts to enable verification and troubleshooting
- **FR-016**: System MUST detect derivative instruments (futures, options, warrants) in broker statements and emit clear warnings that these are not supported in the current implementation
- **FR-017**: System MUST support configurable exchange rate fallback strategy when transaction date has no cached ECB rate (strict mode: error and require manual input; lenient mode: use nearest previous business day rate with warning)

### Key Entities

- **GermanTaxStatement**: Represents a complete tax declaration for a tax year, containing capital gains, dividend income, interest income, foreign tax credits, and calculated German tax liability
- **CapitalGain**: Represents a taxable stock/ETF sale with purchase date, sale date, cost basis, proceeds, gain/loss, and applicable Teilfreistellung
- **DividendIncome**: Represents dividend payment with gross amount, foreign withholding tax, applicable Teilfreistellung, and net taxable amount in EUR
- **LossCarryforward**: Represents capital losses from previous years available to offset current year gains, with year of origin and remaining balance
- **ETFClassification**: Metadata for ETFs indicating type (equity/mixed/bond) for determining applicable Teilfreistellung percentage

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: German tax residents can generate complete tax statements for all their foreign brokerage accounts in under 10 seconds (matching performance of existing Russia tax statement generation)
- **SC-002**: Tax calculations match manual calculations within €0.01 for 100% of test cases using real broker statement data
- **SC-003**: System correctly applies double taxation treaty withholding tax credits for major jurisdictions (USA 15%, other common rates)
- **SC-004**: Generated tax statements contain all required data fields (including ISIN, dates, cost basis, proceeds, tax calculations) for German tax filing without requiring manual cross-reference to broker statements
- **SC-005**: Loss carryforward tracking maintains consistency across tax years with 100% accuracy (no losses lost or double-counted)

## Clarifications

### Session 2026-01-24

- Q: What is the data source for EUR exchange rates? → A: Use ECB (European Central Bank) reference rates with local caching (similar to existing quote providers)
- Q: What level of observability is needed for tax calculations? → A: Standard logging with debug trace for calculations (cost basis, conversions, tax amounts)
- Q: How should derivative instruments (futures/options) be handled? → A: Exclude derivatives from initial implementation (scope boundary, emit warning if detected)
- Q: What columns should the CSV tax statement contain? → A: Comprehensive columns matching tax form requirements (date, symbol, ISIN, quantity, cost basis, proceeds, gain/loss, foreign tax, taxable amount, German tax, description)
- Q: How to handle missing exchange rates for transaction dates? → A: Allow configuring fallback strategy per user preference (strict vs lenient mode)

## Assumptions

- **A-001**: Users will configure their tax jurisdiction as Germany in the configuration file before running tax statement generation
- **A-002**: ECB (European Central Bank) provides daily reference exchange rates via free XML/CSV API that will be cached locally using the existing forex rate cache infrastructure (similar to other currency providers)
- **A-003**: ETF classification metadata will be manually configured in the configuration file or instrument mapping initially; automatic detection can be added later
- **A-004**: Church tax (Kirchensteuer) percentage varies by region and religious affiliation; users will manually configure their applicable rate (0%, 8%, or 9%)
- **A-005**: The Abgeltungssteuer rate of 25% is stable; any future rate changes will require constitution updates
- **A-006**: Pre-2009 holdings (Altbestand) are rare for most users; initial implementation will include warnings if detected but may not fully support complex Altbestand scenarios
- **A-007**: Output format will be CSV initially (similar to cash-flow command); German tax software integration (like .deX format for Russia) is a future enhancement
- **A-008**: Solidarity surcharge applies at the standard 5.5% rate; exemption thresholds for low incomes will not be implemented initially
- **A-009**: Default exchange rate fallback mode will be lenient (use nearest previous business day with warning) to minimize friction; users requiring strict validation can enable strict mode in configuration

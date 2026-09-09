# Task Generation Report: Germany Tax Reports

**Generated**: 2026-01-24
**Feature**: Germany Tax Reports (001-germany-tax)
**Command**: `/speckit.tasks`

## Summary

✅ **Task breakdown complete** - 116 tasks generated and organized by user story

**Output File**: [specs/001-germany-tax/tasks.md](tasks.md)

## Task Breakdown

### By Phase

| Phase | Purpose | Tasks | Key Deliverables |
|-------|---------|-------|------------------|
| **Phase 1: Setup** | Project initialization | 3 | Branch checkout, constitution review |
| **Phase 2: Foundational** | Blocking prerequisites | 16 | Germany jurisdiction, ECB provider, config, module structure |
| **Phase 3: User Story 1** (P1 MVP) | Generate German tax declaration | 50 | Capital gains, dividends, tax calculations, CSV output, CLI |
| **Phase 4: User Story 2** (P2) | Multi-year loss carryforward | 16 | Loss tracking, carryforward application |
| **Phase 5: User Story 3** (P3) | Teilfreistellung for ETFs | 17 | ETF classification, exemption application |
| **Phase 6: Polish** | Cross-cutting improvements | 14 | Documentation, performance, validation |
| **TOTAL** | | **116** | Full Germany tax support |

### By User Story

| User Story | Priority | Tasks | Independent Test Criteria |
|------------|----------|-------|---------------------------|
| **US1**: Generate Tax Declaration | P1 (MVP) | 50 | Run `investments tax-statement ib 2025 german-tax-2025.csv` with German jurisdiction |
| **US2**: Loss Carryforward | P2 | 16 | Configure loss CF, verify applied to gains, remaining balance reported |
| **US3**: ETF Teilfreistellung | P3 | 17 | Configure ETF classifications, verify 30%/15% exemptions applied |
| **Foundation** | N/A | 16 | Germany jurisdiction exists, ECB rates work, module structure ready |
| **Setup & Polish** | N/A | 17 | Project ready, documentation complete, tests pass |

## Parallel Execution Opportunities

**Parallelizable Tasks**: 31 tasks marked with [P]

**Key Parallel Opportunities**:

1. **Phase 2 (Foundational)**: ECB provider registration + tests can run in parallel (T009, T010)
2. **Phase 3.1 (US1 Tests)**: All 6 unit test files can be written in parallel (T020-T025)
3. **Phase 3.2 (US1 Data Models)**: All 5 entity structs can be created in parallel (T030-T034)
4. **Phase 3.7 (US1 CSV)**: All 3 format functions can be written in parallel (T057-T059)
5. **User Stories**: US1, US2, US3 can all start in parallel after Phase 2 completes (with multiple developers)

## Test-First Development (Constitution Principle II)

**Critical**: All test tasks MUST be completed BEFORE implementation tasks

**Test Tasks by Story**:

- **User Story 1**: T020-T029 (10 test tasks) → MUST FAIL (RED) → Then implement T030-T069 → Tests PASS (GREEN)
- **User Story 2**: T070-T074 (5 test tasks) → RED → Implement T075-T082 → GREEN
- **User Story 3**: T086-T090 (5 test tasks) → RED → Implement T091-T099 → GREEN

**Verification Checkpoints**:

- T029: Verify US1 tests FAIL (RED phase confirmed)
- T074: Verify US2 tests FAIL (RED phase confirmed)
- T090: Verify US3 tests FAIL (RED phase confirmed)

## MVP Path (Minimum Viable Product)

**Goal**: Deliver basic Germany tax statement generation

**Required Phases**:

1. Phase 1: Setup (T001-T003)
2. Phase 2: Foundational (T004-T019) - **CRITICAL BLOCKER**
3. Phase 3: User Story 1 (T020-T069)
4. Phase 6: Core Polish (T103-T110, T115-T116)

**MVP Task Count**: ~50 critical path tasks
**Estimated Duration**: 4-5 days with test-first development

**What MVP Delivers**:

- ✅ Generate German tax statements for foreign brokerage accounts
- ✅ Calculate capital gains with FIFO cost basis
- ✅ Calculate dividend income with foreign tax credits
- ✅ Calculate Abgeltungssteuer (25%) + Solidaritätszuschlag (5.5%) + Kirchensteuer
- ✅ Output comprehensive CSV with 19 columns + summary
- ✅ ECB exchange rate integration with caching
- ✅ CLI integration via `investments tax-statement` command

**Deferred to Post-MVP**:

- ⏳ Loss carryforward (User Story 2) - Nice to have
- ⏳ ETF Teilfreistellung (User Story 3) - Nice to have for ETF investors

## Incremental Delivery Strategy

**Recommended Approach**: Deliver value with each user story

1. **Foundation** (Phase 1+2) → Germany jurisdiction and infrastructure ready
2. **+ User Story 1** → 🚀 **MVP RELEASE** - Basic German tax statements working
3. **+ User Story 2** → 📦 **V1.1 RELEASE** - Loss carryforward support
4. **+ User Story 3** → 📦 **V1.2 RELEASE** - Full ETF exemptions
5. **+ Polish** → 📦 **V1.3 RELEASE** - Production-ready

Each release is independently testable and delivers incremental value.

## Dependencies

### Critical Blocking Dependencies

**Phase 2 (Foundational) BLOCKS everything**:

- No user story can begin until Germany jurisdiction exists (T004-T005)
- No user story can begin until ECB provider works (T006-T010)
- No user story can begin until module structure is ready (T014-T019)

### User Story Independence

After Phase 2 completes, all user stories are **independently implementable**:

- **US1** (P1 MVP): No dependencies on US2 or US3
- **US2** (P2 Loss CF): No dependencies on US3, minimal integration with US1 (reads capital gains)
- **US3** (P3 ETFs): No dependencies on US2, integrates with US1 calculations

**Parallel Team Strategy**: After Phase 2, assign different developers to US1, US2, US3 simultaneously.

## Format Validation

✅ **All tasks follow required format**: `- [ ] [ID] [P?] [Story?] Description with file path`

**Format Components Verified**:

- ✅ Checkbox: All 116 tasks start with `- [ ]`
- ✅ Task IDs: Sequential T001-T116 in execution order
- ✅ [P] markers: 31 tasks marked as parallelizable
- ✅ [Story] labels: 50 US1, 16 US2, 17 US3 tasks properly labeled
- ✅ File paths: All implementation tasks include exact file paths
- ✅ Descriptions: Clear, actionable task descriptions

**Sample Tasks**:

- ✅ `- [ ] T001 Checkout branch 001-germany-tax and ensure clean working directory`
- ✅ `- [ ] T010 [P] Add unit tests for ECB rate fetching and caching in src/quotes/ecb.rs (#[cfg(test)])`
- ✅ `- [ ] T032 [P] [US1] Create CapitalGainEntry struct in src/tax_statement/germany/statement.rs`

## Constitution Compliance

**All 6 Principles Addressed in Tasks**:

1. ✅ **Code Quality & Type Safety**: T054 validates Decimal types, T107 enforces clippy -Dwarnings
2. ✅ **Test-First Development**: T020-T029, T070-T074, T086-T090 create tests BEFORE implementation
3. ✅ **User Experience Consistency**: T062-T065 integrate with existing CLI patterns
4. ✅ **Performance & Efficiency**: T109 validates <10s performance target
5. ✅ **Data Integrity & Validation**: T054 ensures €0.01 precision, T064-T065 handle errors/warnings
6. ✅ **Multi-Jurisdiction Support**: T004-T005 extend existing locality pattern

## Success Criteria Mapping

Tasks generated to satisfy all 5 success criteria from spec.md:

| Success Criterion | Verification Task | Target |
|-------------------|------------------|--------|
| SC-001: Performance < 10s | T109 | <10 seconds for 1000 transactions |
| SC-002: Calculation accuracy | T110 | ±€0.01 for all calculations |
| SC-003: Foreign tax credits | T027, T043 | Correct treaty rates (US 15%) |
| SC-004: Complete CSV output | T067, T056-T060 | All 19 columns + summary |
| SC-005: Loss CF accuracy | T084 | 100% accuracy across years |

## Entity Coverage

All 9 entities from data-model.md have corresponding implementation tasks:

1. ✅ **GermanTaxStatement**: T035, T049-T053
2. ✅ **CapitalGainEntry**: T032, T038
3. ✅ **DividendEntry**: T033, T041-T044
4. ✅ **InterestEntry**: T034, T052
5. ✅ **FifoLot**: T030
6. ✅ **FifoQueue**: T031, T036-T037
7. ✅ **LossCarryforward**: T075-T078
8. ✅ **EtfClassification**: T091-T094
9. ✅ **GermanTaxConfig**: T011-T012

## CSV Contract Implementation

All 19 columns from contracts/csv-output.md covered:

- ✅ Header generation: T056
- ✅ Transaction rows: T057-T059 (capital gains, dividends, interest)
- ✅ Summary rows: T060 (10 summary rows)
- ✅ Validation: T067 (verify against contract)

## Files to be Created/Modified

**New Files** (~16 files):

- src/quotes/ecb.rs (T006-T007)
- src/taxes/germany/mod.rs (T015)
- src/taxes/germany/rates.rs (T045-T048)
- src/taxes/germany/capital_gains.rs (T030-T040)
- src/taxes/germany/dividends.rs (T041-T044, T095-T097)
- src/taxes/germany/loss_carryforward.rs (T076-T082)
- src/tax_statement/germany/mod.rs (T017)
- src/tax_statement/germany/statement.rs (T032-T035, T049-T054)
- src/tax_statement/germany/csv_formatter.rs (T055-T061)
- tests/germany_tax_tests.rs (T020, T027, T073, T089)
- testdata/germany_tax/* (T026, T028, T072, T088, T113)
- docs/germany_tax.md (T103)

**Modified Files** (~7 files):

- src/localities.rs (T004-T005)
- src/config.rs (T011-T012)
- src/instruments.rs (T091-T094)
- src/quotes/mod.rs (T009)
- src/taxes/mod.rs (T018)
- src/tax_statement/mod.rs (T019)
- src/bin/investments/main.rs or tax_statement.rs (T062-T065)
- README.md (T104)
- docs/config-example.yaml (T013, T105)

## Next Steps

1. ✅ **Tasks generated** - This file (tasks.md)
2. 🔄 **Begin implementation** - Start with Phase 1 (Setup)
3. ⏳ **Phase 2 critical** - Complete Foundational phase before any user story work
4. ⏳ **MVP focus** - Prioritize User Story 1 for first release
5. ⏳ **Test-first** - Write tests, see them fail, then implement
6. ⏳ **Checkpoints** - Stop at each phase checkpoint to validate
7. ⏳ **Deploy MVP** - Release User Story 1 before starting US2/US3

## Developer Instructions

**To start implementation**:

```bash
# 1. Checkout feature branch
git checkout 001-germany-tax

# 2. Review design documents
cat specs/001-germany-tax/spec.md
cat specs/001-germany-tax/plan.md
cat specs/001-germany-tax/data-model.md
cat specs/001-germany-tax/quickstart.md

# 3. Start with Phase 1 (Setup)
# Complete T001-T003

# 4. Move to Phase 2 (Foundational) - CRITICAL
# Complete T004-T019
# Verify: cargo build succeeds, Germany jurisdiction exists

# 5. Start User Story 1 (MVP)
# Write tests first: T020-T029
# Verify tests fail: cargo test germany
# Implement: T030-T069
# Verify tests pass: cargo test germany

# 6. Polish and release
# Complete T103-T116
# Run full validation: cargo test && cargo clippy -- -Dwarnings
```

**For parallel development** (multiple team members):

After Phase 2 completes, assign:

- Developer A → User Story 1 (T020-T069)
- Developer B → User Story 2 (T070-T085)
- Developer C → User Story 3 (T086-T102)

All user stories can merge and test independently.

---

**Task Generation**: ✅ COMPLETE
**Ready for Implementation**: ✅ YES
**MVP Path Defined**: ✅ YES
**Test Strategy**: ✅ TEST-FIRST REQUIRED
**Constitution Compliant**: ✅ YES

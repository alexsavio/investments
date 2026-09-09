# Germany Tax Implementation Plan - COMPLETE

**Branch**: `001-germany-tax`
**Spec**: [spec.md](spec.md)
**Plan**: [plan.md](plan.md)
**Status**: ✅ Planning phase complete

## Deliverables Created

### Phase 0: Research ✓

- **[research.md](research.md)** - 10 sections covering all technical unknowns
  - ECB API integration
  - Year-specific German tax rates (2009+ Abgeltungssteuer)
  - FIFO cost basis methodology
  - Teilfreistellung exemption logic
  - Foreign tax credit calculations
  - Loss carryforward mechanics
  - CSV format design
  - Pre-2009 Altbestand handling
  - Derivative detection strategy
  - Debug logging approach

### Phase 1: Design ✓

- **[data-model.md](data-model.md)** - 9 core entities with validation rules
  - GermanTaxStatement, CapitalGainEntry, DividendEntry, InterestEntry
  - FifoLot, FifoQueue for cost basis tracking
  - LossCarryforward for multi-year losses
  - EtfClassification for Teilfreistellung
  - GermanTaxConfig for user settings

- **[contracts/csv-output.md](contracts/csv-output.md)** - Complete CSV specification
  - 19 transaction columns
  - 10 summary rows
  - Validation rules
  - Example output

- **[quickstart.md](quickstart.md)** - Developer implementation guide
  - 3-phase implementation roadmap
  - Code patterns and examples
  - Testing strategy
  - Common pitfalls

- **[.github/agents/copilot-instructions.md](../../.github/agents/copilot-instructions.md)** - Updated agent context

### Phase 2: Planning ✓

- **[plan.md](plan.md)** - Implementation plan
  - Summary and technical context
  - Constitution compliance check (all 6 principles ✓)
  - Source code structure
  - Complexity tracking
  - Risk assessment

## Implementation Roadmap

```text
Foundation (Phase 0)
├── Add Jurisdiction::Germany to localities.rs
├── Implement ECB rate provider in forex.rs
├── Extend Config with GermanTaxConfig
└── Setup test infrastructure

User Story 1 - Generate Tax Declaration (P1 MVP)
├── FIFO capital gains calculation
├── Dividend income with foreign tax credits
├── Tax rates: Abgeltungssteuer (25%) + Soli (5.5%) + Kirchensteuer (0-9%)
├── Tax statement aggregation
├── CSV output (19 columns + summary)
└── CLI integration

User Story 2 - Loss Carryforward (P2)
├── Load carryforward from config
├── Apply to current year gains
└── Report remaining balance

User Story 3 - Teilfreistellung for ETFs (P3)
├── ETF classification metadata
├── Apply exemptions to dividends (30%/15%/0%)
└── Apply exemptions to capital gains
```

## Key Technical Decisions

| Area | Decision | Rationale |
|------|----------|-----------|
| **Exchange Rates** | ECB Statistical Data Warehouse | Wider EU usage, cleaner API, tax authority accepted |
| **Currency Arithmetic** | `rust_decimal::Decimal` | ±€0.01 precision, no float errors |
| **FIFO Tracking** | `VecDeque<FifoLot>` | Efficient queue operations, matches accounting standard |
| **Tax Rates** | `BTreeMap<i32, GermanTaxRates>` | Year-specific rates following existing russia() pattern |
| **Teilfreistellung** | Lookup table by InstrumentType | ETF 30%, Mixed 15%, Bond 0% |
| **Fallback Mode** | Configurable strict/lenient | Weekend/holiday exchange rate gaps |
| **Pre-2009 Holdings** | Warning-only (defer complexity) | Rare edge case, needs user clarification |
| **Derivatives** | Exclude with detection warning | Out of scope for MVP |

## Constitution Compliance

✅ **Principle I**: Type Safety - All currency amounts use `Decimal`, proper error handling
✅ **Principle II**: Test-First - Test suite planned, unit tests for all calculations
✅ **Principle III**: UX Consistency - CSV output follows cash-flow pattern
✅ **Principle IV**: Performance - Target <10s for 1000 transactions
✅ **Principle V**: Data Integrity - Validation rules defined, FIFO queue integrity
✅ **Principle VI**: Multi-Jurisdiction - Extends existing locality pattern

## Estimated Effort

- **Lines of Code**: ~2,500 new lines (based on Russia module)
- **New Files**: ~12 Rust source files
- **Modified Files**: ~5 existing files
- **Test Files**: ~3 integration + unit tests
- **Duration**: 3-5 days for MVP (User Story 1)

## Next Steps

1. ✅ Constitution ratified
2. ✅ Feature specification complete
3. ✅ Clarifications resolved
4. ✅ Technical research complete
5. ✅ Design artifacts created
6. ✅ Implementation plan finalized
7. **🔄 NEXT: Run `/speckit.tasks` to generate task breakdown**
8. ⏳ Begin implementation (test-first development)
9. ⏳ Testing and validation
10. ⏳ Documentation and review

## Files Generated

```text
specs/001-germany-tax/
├── spec.md                    (Feature specification with clarifications)
├── plan.md                    (Implementation plan - this document)
├── research.md                (Technical research findings)
├── data-model.md              (Entity definitions)
├── quickstart.md              (Developer guide)
├── PLAN_COMPLETE.md           (This summary)
├── contracts/
│   └── csv-output.md          (CSV format contract)
└── checklists/
    └── requirements.md        (Quality validation checklist)
```

**Planning Phase Status**: ✅ COMPLETE
**Ready for Task Breakdown**: YES
**Command**: `/speckit.tasks`

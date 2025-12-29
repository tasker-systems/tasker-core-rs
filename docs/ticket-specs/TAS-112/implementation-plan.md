# TAS-112 Implementation Plan: Cross-Language Handler Harmonization

**Created**: 2025-12-29
**Status**: Ready for Review
**Branch**: `jcoletaylor/tas-112-cross-language-step-handler-ergonomics-analysis`
**Prerequisites**: Research Phases 1-9 Complete

---

## Executive Summary

The 9-phase research effort identified clear gaps and a path to cross-language consistency. This implementation plan organizes the work into **parallel streams** where possible and **sequential gates** where dependencies exist.

**Project Context**: Pre-alpha greenfield open source project. Breaking changes are encouraged to achieve correct architecture. No backward compatibility concerns.

**Guiding Principles**:
1. **Ruby is reference implementation** — When in doubt, follow Ruby's patterns
2. **Composition over inheritance** — Batchable already uses mixins; all patterns should follow
3. **FFI boundaries require exact alignment** — Structures crossing languages need explicit types
4. **Breaking changes together** — Batch all breaking changes into a single coordinated release
5. **Determine specifics during implementation** — This plan defines *what* and *sequence*, not exact code

---

## Work Stream Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    PHASE 1: FOUNDATION (Parallel)                   │
├─────────────────┬─────────────────┬─────────────────┬───────────────┤
│ Stream A        │ Stream B        │ Stream C        │ Stream D      │
│ TypeScript      │ Python          │ FFI Boundary    │ Examples &    │
│ Domain Events   │ Enhancements    │ Types           │ Tests         │
│ (CRITICAL)      │                 │                 │               │
├─────────────────┴─────────────────┴─────────────────┴───────────────┤
│                         ↓ VALIDATION GATE 1 ↓                       │
│           All languages can publish/subscribe to events             │
│           FFI boundary types have explicit classes                  │
├─────────────────────────────────────────────────────────────────────┤
│                 PHASE 2: RUST ERGONOMICS (Parallel)                 │
├─────────────────────────────────────────────────────────────────────┤
│ • Rust Handler Traits (API, Decision, Batchable)                    │
│ • Rust examples using new traits                                    │
│ Can proceed independently — no dependency on Phase 1                │
├─────────────────────────────────────────────────────────────────────┤
│                         ↓ VALIDATION GATE 2 ↓                       │
│           All additive changes complete and tested                  │
│           Ready to introduce breaking changes                       │
├─────────────────────────────────────────────────────────────────────┤
│                  PHASE 3: BREAKING CHANGES (Sequential)             │
│           Composition Pattern → Ruby Result → Ruby Cursors          │
│           (Must coordinate — single release)                        │
├─────────────────────────────────────────────────────────────────────┤
│                         ↓ VALIDATION GATE 3 ↓                       │
│           All handlers use composition pattern                      │
│           All breaking changes complete                             │
├─────────────────────────────────────────────────────────────────────┤
│                     PHASE 4: DOCUMENTATION                          │
│           5 new docs + updated worker crate docs                    │
│           Cross-language comparison tables                          │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Foundation (Parallel Streams)

All four streams can start immediately and run in parallel. No dependencies between streams.

### Stream A: TypeScript Domain Events (CRITICAL)

**Priority**: CRITICAL
**Why Critical**: TypeScript workers cannot publish custom events or subscribe to fast events. This is the only language completely missing a major feature.

**Ticket Mapping**: Updates TAS-119 (Domain Events Integration)

#### Tasks

| Task | Description | Validation |
|------|-------------|------------|
| Create `domain-events.ts` | BasePublisher, BaseSubscriber, StepEventContext, DomainEvent interfaces | Types compile |
| Implement publisher lifecycle hooks | before_publish, after_publish, on_publish_error, additional_metadata | Hooks called in correct order |
| Implement subscriber lifecycle hooks | before_handle, after_handle, on_handle_error | Hooks called in correct order |
| Create InProcessDomainEventPoller | FFI integration with Rust broadcast channel | Can receive events from Rust |
| Add PublisherRegistry | Centralized publisher management | Publishers registered and retrieved |
| Add SubscriberRegistry | Centralized subscriber management with start/stop | Subscribers start/stop correctly |
| Create payment publisher example | Custom payload transformation | Example compiles and runs |
| Create logging subscriber example | Pattern-based subscription | Receives matching events |
| Create metrics subscriber example | Counter aggregation | Counts events correctly |
| Integration tests | Full publish/subscribe cycle | Events flow through system |

#### Deliverables
- `workers/typescript/src/handler/domain-events.ts`
- `workers/typescript/src/examples/domain_events/` (publishers + subscribers)
- Integration tests in `workers/typescript/tests/`

---

### Stream B: Python Enhancements

**Priority**: HIGH
**Why**: Python is missing lifecycle hooks and some helper methods that Ruby has.

**Ticket Mapping**: Updates TAS-119 (Domain Events) and TAS-117 (Batchable)

#### Tasks

| Task | Description | Validation |
|------|-------------|------------|
| Add BasePublisher lifecycle hooks | before_publish, after_publish, on_publish_error | Hooks called correctly |
| Add BasePublisher.additional_metadata() | Custom metadata injection | Metadata appears in events |
| Add BaseSubscriber lifecycle hooks | before_handle, after_handle, on_handle_error | Hooks called correctly |
| Add PublisherRegistry class | Centralized publisher management | Matches Ruby pattern |
| Add SubscriberRegistry class | Centralized subscriber management | Matches Ruby pattern |
| Add handle_no_op_worker() | Batchable mixin helper | Consistency with Ruby/TS |
| Add create_cursor_configs() | Worker-count based API (currently only batch-size) | Both APIs available |
| Add checkpoint write API | update_checkpoint() or update_step_results() | Can persist checkpoint data |
| Add aggregation_fn parameter | Custom aggregation support in aggregate_worker_results() | Custom aggregation works |

#### Deliverables
- Updated `workers/python/python/tasker_core/domain_events.py`
- Updated `workers/python/python/tasker_core/batch_processing/batchable.py`
- Updated examples and tests

---

### Stream C: FFI Boundary Types

**Priority**: HIGH
**Why**: Python/TypeScript use inline dicts/objects for FFI structures, causing type safety issues and harder debugging.

**Ticket Mapping**: Updates TAS-118 (Batch Processing Lifecycle)

#### Tasks

| Task | Description | Validation |
|------|-------------|------------|
| Python: Create BatchProcessingOutcome | Pydantic model matching Rust enum exactly | Serializes identically to Rust |
| Python: Enhance CursorConfig | Change cursor types from `int` to `Any`, add `batch_id` | Flexible cursor types work |
| Python: Align BatchWorkerContext | Match Rust BatchWorkerInputs structure exactly | Field names and types match |
| TypeScript: Create BatchProcessingOutcome | Interface/type matching Rust enum | Type-safe construction |
| TypeScript: Enhance CursorConfig | Change cursor types from `number` to `unknown`, add `batch_id` | Flexible cursor types work |
| TypeScript: Add BatchAggregationScenario | Helper class for aggregation detection | Scenario detection works |
| Document FFI boundary types | Create reference documentation | Clear serialization guidance |

#### Deliverables
- Updated `workers/python/python/tasker_core/types.py`
- Updated `workers/typescript/src/types/batch.ts`
- New `docs/reference/ffi-boundary-types.md` (per TAS-99 structure)

---

### Stream D: Examples & Tests

**Priority**: HIGH
**Why**: Python/TypeScript lack complete conditional workflow examples with convergence patterns.

**Ticket Mapping**: Updates TAS-120 (Conditional Workflows)

#### Tasks

| Task | Description | Validation |
|------|-------------|------------|
| Python: Create conditional_approval example | Complete workflow: validate → decision → branches → converge | E2E test passes |
| Python: Create YAML template | Task template for conditional approval | Loads and validates |
| Python: Add E2E tests | Small, medium, large amount routing scenarios | All scenarios pass |
| TypeScript: Create conditional_approval example | Complete workflow matching Python | E2E test passes |
| TypeScript: Create YAML template | Task template matching Python | Loads and validates |
| TypeScript: Add E2E tests | Small, medium, large amount routing scenarios | All scenarios pass |
| Document convergence patterns | How intersection semantics work | Clear explanation |

#### Deliverables
- `workers/python/examples/conditional_approval/`
- `workers/typescript/src/examples/conditional_approval/`
- YAML templates in `tests/fixtures/task_templates/`
- E2E tests for both languages

---

## Phase 2: Rust Ergonomics

**Priority**: HIGH
**Parallel with Phase 1**: Can proceed independently — no dependencies.

**Ticket Mapping**: Updates TAS-114 (Base Handler), TAS-115 (API Handler), TAS-116 (Decision Handler), TAS-117 (Batchable)

### Tasks

| Task | Description | Validation |
|------|-------------|------------|
| Enhance StepHandler trait | Add version(), capabilities(), config_schema() methods | Methods available on trait |
| Create APICapable trait | api_success(), api_failure(), classify_status_code() | Helpers construct correct results |
| Create DecisionCapable trait | decision_success(), skip_branches(), decision_failure() | Helpers construct correct outcomes |
| Create BatchableCapable trait | batch_analyzer_success(), batch_worker_success(), cursor helpers | Helpers construct correct outcomes |
| Add BatchWorkerOutcome type | Add to tasker-shared (currently missing) | Type available for use |
| Create cursor helper methods | create_cursor_configs(), create_cursor_ranges() | Both APIs work correctly |
| Create example handlers | Handlers demonstrating each trait | Examples compile and run |
| Update Rust worker docs | Complete examples with new traits | Documentation accurate |

### Trait Sketches (Implementation Details TBD)

```rust
// APICapable trait
pub trait APICapable {
    fn api_success(&self, data: Value, status: u16, headers: Option<Value>) -> StepExecutionResult;
    fn api_failure(&self, message: &str, status: u16, error_type: &str) -> StepExecutionResult;
    fn classify_status_code(&self, status: u16) -> ErrorClassification;
}

// DecisionCapable trait
pub trait DecisionCapable {
    fn decision_success(&self, step_names: Vec<String>, routing_context: Option<Value>) -> StepExecutionResult;
    fn skip_branches(&self, reason: &str, routing_context: Option<Value>) -> StepExecutionResult;
    fn decision_failure(&self, message: &str, error_type: &str) -> StepExecutionResult;
}

// BatchableCapable trait
pub trait BatchableCapable {
    fn create_cursor_configs(&self, total_items: u64, worker_count: u32) -> Vec<CursorConfig>;
    fn create_cursor_ranges(&self, total_items: u64, batch_size: u64, max_batches: Option<u32>) -> Vec<CursorConfig>;
    fn batch_analyzer_success(&self, worker_template: &str, configs: Vec<CursorConfig>, total: u64) -> StepExecutionResult;
    fn batch_worker_success(&self, processed: u64, succeeded: u64, failed: u64, skipped: u64) -> StepExecutionResult;
    fn no_batches_outcome(&self, reason: &str) -> StepExecutionResult;
}
```

### Deliverables
- New traits in `workers/rust/src/step_handlers/`
- BatchWorkerOutcome type in `tasker-shared/src/messaging/execution_types.rs`
- Example handlers in `workers/rust/src/examples/`
- Updated `docs/workes/rust.md`

---

## Validation Gate 1: Pre-Breaking Changes Checkpoint

Before proceeding to Phase 3, all of the following must be true:

### Technical Validation
- [ ] TypeScript domain events module complete and tested
- [ ] TypeScript can publish custom events via BasePublisher
- [ ] TypeScript can subscribe to fast events via BaseSubscriber
- [ ] Python lifecycle hooks added to BasePublisher and BaseSubscriber
- [ ] Python has PublisherRegistry and SubscriberRegistry
- [ ] Python BatchProcessingOutcome is explicit Pydantic model
- [ ] TypeScript BatchProcessingOutcome is explicit interface
- [ ] Python/TypeScript CursorConfig supports flexible cursor types
- [ ] Python/TypeScript conditional workflow examples complete
- [ ] All examples pass E2E tests
- [ ] Rust handler traits complete with examples

### Quality Validation
- [ ] No serialization mismatches at FFI boundaries
- [ ] All new code has test coverage
- [ ] Documentation updated for new features

**Milestone**: All languages have equivalent capabilities (additive changes only).

---

## Phase 3: Breaking Changes (Sequential)

**Priority**: CRITICAL
**Must be Sequential**: These changes affect each other and should be released together.

**Ticket Mapping**: Updates TAS-114 (Base Handler), TAS-117 (Batchable)

### Step 3.1: Composition Pattern Migration

**Affects**: Ruby, Python, TypeScript (all handler types)

#### Current State (Inheritance)
```ruby
# Ruby
class MyHandler < TaskerCore::StepHandler::API
```
```python
# Python
class MyHandler(APIHandler):
```
```typescript
// TypeScript
class MyHandler extends APIHandler
```

#### Target State (Composition)
```ruby
# Ruby
class MyHandler < TaskerCore::StepHandler::Base
  include TaskerCore::StepHandler::Mixins::API
```
```python
# Python
class MyHandler(StepHandler, APIMixin):
```
```typescript
// TypeScript
class MyHandler extends StepHandler implements APICapable {
  // Mixin binding or use applyMixin()
}
```

#### Migration Approach
1. **Create mixin modules** alongside existing subclasses (non-breaking)
2. **Add deprecation warnings** to subclasses (non-breaking)
3. **Migrate all examples** to mixin pattern
4. **Update tests** to use new pattern
5. **Remove subclasses** (breaking — coordinate with other breaking changes)

#### Tasks

| Language | Task | Validation |
|----------|------|------------|
| Ruby | Create Mixins::API module | Module includes all API helpers |
| Ruby | Create Mixins::Decision module | Module includes all decision helpers |
| Ruby | Add deprecation to API, Decision subclasses | Warning printed on instantiation |
| Ruby | Migrate all examples | Examples use include pattern |
| Python | Create APIMixin class | Mixin includes all API helpers |
| Python | Create DecisionMixin class | Mixin includes all decision helpers |
| Python | Migrate all examples | Examples use multiple inheritance |
| TypeScript | Create API mixin/interface | Mixin includes all API helpers |
| TypeScript | Create Decision mixin/interface | Mixin includes all decision helpers |
| TypeScript | Migrate all examples | Examples use mixin pattern |

---

### Step 3.2: Ruby Result Unification

**Affects**: Ruby only

#### Current State (Split Classes)
```ruby
StepHandlerResult::Success.new(result: data)
StepHandlerResult::Error.new(message: "error")

# Type checking
result.is_a?(StepHandlerResult::Success)
```

#### Target State (Unified Class)
```ruby
StepHandlerResult.success(result: data)
StepHandlerResult.failure(message: "error")

# Type checking
result.success?
```

#### Migration Approach
1. **Create unified StepHandlerResult class** with factory methods
2. **Make Success/Error inherit from unified** (backward compatible initially)
3. **Add deprecation warnings** to Success/Error direct usage
4. **Migrate all examples** to factory methods
5. **Remove Success/Error subclasses** (breaking)

#### Tasks

| Task | Validation |
|------|------------|
| Create unified StepHandlerResult class | Factory methods work |
| Add success?, failure? predicate methods | Predicates return correct values |
| Add deprecation to Success/Error | Warnings printed |
| Migrate all examples | Examples use factory methods |
| Update all type checks | Use predicates not is_a? |

---

### Step 3.3: Ruby Cursor Indexing Fix

**Affects**: Ruby batch processing only

#### Current State (1-indexed)
```ruby
# Ruby cursors start at 1
configs = create_cursor_configs(1000, 5)
# => [{start_cursor: 1, end_cursor: 201}, ...]
```

#### Target State (0-indexed)
```ruby
# Ruby cursors start at 0 (matching Python/TypeScript/Rust)
configs = create_cursor_configs(1000, 5)
# => [{start_cursor: 0, end_cursor: 200}, ...]
```

#### Tasks

| Task | Validation |
|------|------------|
| Update create_cursor_configs() logic | Cursors start at 0 |
| Update all batch processing examples | Examples use 0-indexed |
| Update batch processing tests | Tests expect 0-indexed |
| Document breaking change | Migration guide updated |

---

## Validation Gate 2: Breaking Changes Complete

Before proceeding to Phase 4, all of the following must be true:

### Technical Validation
- [ ] All Ruby handlers use `include Mixins::X` pattern
- [ ] All Python handlers use multiple inheritance with mixins
- [ ] All TypeScript handlers use mixin pattern
- [ ] No handler subclasses API, Decision, or Batchable
- [ ] Ruby StepHandlerResult unified (no Success/Error subclasses)
- [ ] Ruby cursor indexing is 0-based
- [ ] All examples updated and passing
- [ ] All tests updated and passing

### Documentation Validation
- [ ] Migration guide written for composition pattern
- [ ] Migration guide written for Ruby result unification
- [ ] Migration guide written for Ruby cursor indexing

**Milestone**: Architecture is consistent across all languages.

---

## Phase 4: Documentation Overhaul

**Priority**: HIGH
**After**: All implementation complete

### Documentation Structure (TAS-99 Alignment)

Per the TAS-99 documentation restructuring, docs are organized by cognitive function:

| Directory | Purpose | TAS-112 Relevant Docs |
|-----------|---------|----------------------|
| `docs/principles/` | The "why" | `composition-over-inheritance.md`, `cross-language-consistency.md` |
| `docs/architecture/` | The "what" | `domain-events.md` |
| `docs/guides/` | The "how" | `batch-processing.md`, `conditional-workflows.md` |
| `docs/workers/` | Language-specific | `{ruby,python,typescript,rust}.md`, `api-convergence-matrix.md` |
| `docs/reference/` | Technical specs | FFI boundary types (new) |

### New Documentation

| Document | Location | Content |
|----------|----------|---------|
| FFI Boundary Types | `docs/reference/ffi-boundary-types.md` | FFI structures, serialization formats, Rust ownership model |

### Updated Documentation

Many documents already exist and need updates rather than creation:

| Document | Updates Needed |
|----------|----------------|
| `docs/principles/composition-over-inheritance.md` | Add migration examples, Ruby/Python/TypeScript specifics |
| `docs/architecture/domain-events.md` | Add publishing flow clarification, custom publisher role |
| `docs/guides/batch-processing.md` | Add checkpoint read/write patterns, cross-language examples |
| `docs/guides/conditional-workflows.md` | Add convergence patterns, intersection semantics detail |
| `docs/workers/api-convergence-matrix.md` | Update with new APIs, lifecycle hooks, FFI types |
| `docs/workers/rust.md` | Handler traits, composition via traits, complete examples |
| `docs/workers/ruby.md` | Mixin pattern, unified results, 0-indexed cursors |
| `docs/workers/python.md` | Lifecycle hooks, mixin pattern, checkpoint APIs |
| `docs/workers/typescript.md` | Domain events module, mixin pattern, complete examples |

### CLAUDE-GUIDE.md Updates

Add trigger mappings for new patterns:

| If conversation involves... | First consult... |
|-----------------------------|------------------|
| Handler composition patterns | `docs/principles/composition-over-inheritance.md` |
| FFI boundary types, serialization | `docs/reference/ffi-boundary-types.md` |
| Cross-language API consistency | `docs/workers/api-convergence-matrix.md` |

### Documentation Quality Standards

All new/updated docs must include:
- Cross-language examples where applicable
- Complete code examples (no pseudo-code)
- Explanation of "why" not just "how"
- Common pitfalls and how to avoid them
- Links to related documentation per CLAUDE-GUIDE.md patterns

---

## Validation Gate 3: Final Validation

### Technical Criteria
- [ ] All four languages have equivalent handler APIs
- [ ] FFI boundary types explicitly defined and documented
- [ ] All handlers use composition pattern
- [ ] TypeScript has complete domain events module
- [ ] Python/TypeScript have checkpoint write APIs
- [ ] Ruby uses unified result class
- [ ] Rust has ergonomic handler traits

### Documentation Criteria
- [ ] `docs/reference/ffi-boundary-types.md` created
- [ ] `docs/principles/composition-over-inheritance.md` updated with migration examples
- [ ] `docs/workers/{ruby,python,typescript,rust}.md` updated with complete examples
- [ ] `docs/workers/api-convergence-matrix.md` reflects new APIs
- [ ] `docs/guides/batch-processing.md` includes checkpoint patterns
- [ ] `docs/architecture/domain-events.md` clarifies publishing flow
- [ ] `docs/CLAUDE-GUIDE.md` updated with new trigger mappings

### Quality Criteria
- [ ] All examples passing E2E tests
- [ ] Zero serialization bugs at FFI boundaries
- [ ] "One obvious way" to implement each pattern

**Milestone**: TAS-112 complete. Cross-language ergonomics harmonized.

---

## Parallelization Summary

| Work | Dependencies | Can Parallel With |
|------|--------------|-------------------|
| Stream A (TS Domain Events) | None | B, C, D, Rust Traits |
| Stream B (Python Enhancements) | None | A, C, D, Rust Traits |
| Stream C (FFI Types) | None | A, B, D, Rust Traits |
| Stream D (Examples) | Partial on A, B, C | Can start immediately, finish after |
| Rust Handler Traits | None | All Phase 1 streams |
| Composition Migration | Gate 1 passed | Sequential within Phase 3 |
| Ruby Result Unification | Gate 1 passed | Sequential within Phase 3 |
| Ruby Cursor Fix | Gate 1 passed | Sequential within Phase 3 |
| Documentation | Gate 2 passed | N/A (final phase) |

---

## Ticket Mapping

The existing research phase tickets (TAS-113–TAS-121) map to implementation work as follows:

| Ticket | Research Phase | Implementation Work |
|--------|----------------|---------------------|
| TAS-113 | Phase 1: Documentation Analysis | N/A (research complete) |
| TAS-114 | Phase 2: Base Handler API | Rust traits, composition migration |
| TAS-115 | Phase 3: API Handler Pattern | API mixin creation, Rust APICapable |
| TAS-116 | Phase 4: Decision Handler Pattern | Decision mixin creation, Rust DecisionCapable |
| TAS-117 | Phase 5: Batchable Handler Pattern | Batchable enhancements, Rust BatchableCapable |
| TAS-118 | Phase 6: Batch Processing Lifecycle | FFI boundary types, checkpoint APIs |
| TAS-119 | Phase 7: Domain Events Integration | TypeScript domain events, Python lifecycle hooks |
| TAS-120 | Phase 8: Conditional Workflows | Python/TypeScript examples and tests |
| TAS-121 | Phase 9: Validation & Documentation | Final validation, documentation overhaul |

---

## Success Criteria

This implementation effort is successful when:

1. **Equivalent APIs**: All four languages have equivalent handler APIs for each pattern
2. **FFI Alignment**: FFI boundary types are explicitly defined with zero serialization bugs
3. **Composition Pattern**: All handlers use composition (mixins/traits), not inheritance
4. **TypeScript Complete**: TypeScript has complete domain events module
5. **Checkpoint APIs**: Python/TypeScript have checkpoint write APIs
6. **Ruby Unified**: Ruby uses unified result class and 0-indexed cursors
7. **Rust Ergonomic**: Rust has ergonomic handler traits matching other languages
8. **Documentation**: Per TAS-99 structure — 1 new reference doc, 8+ existing docs updated
9. **Quality**: All examples pass E2E tests, "one obvious way" to implement each pattern

---

## Appendix: Research Artifacts

All 9 phase documents are in `docs/ticket-specs/TAS-112/`:

| Document | Content |
|----------|---------|
| `research-plan.md` | Phase 1: Core Documentation Analysis |
| `base.md` | Phase 2: Base Handler Pattern |
| `api.md` | Phase 3: API Handler Pattern |
| `decision.md` | Phase 4: Decision Handler Pattern |
| `batchable.md` | Phase 5: Batchable Handler Pattern |
| `batch-processing.md` | Phase 6: Batch Processing Lifecycle |
| `domain-events.md` | Phase 7: Domain Events Integration |
| `conditional-workflows.md` | Phase 8: Conditional Workflows |
| `recommendations.md` | Phase 9: Synthesis & Recommendations |

**Total Research**: ~12,000 lines of analysis across 9 documents.

---

## Appendix: Related Documentation

### Existing Docs (TAS-99 Structure)

| Document | Relevance to TAS-112 |
|----------|---------------------|
| `docs/decisions/TAS-112-composition-pattern.md` | ADR for composition over inheritance decision |
| `docs/principles/composition-over-inheritance.md` | Canonical composition pattern guidance |
| `docs/principles/cross-language-consistency.md` | Multi-language API philosophy |
| `docs/workers/api-convergence-matrix.md` | Cross-language API reference |
| `docs/architecture/domain-events.md` | Domain event architecture |
| `docs/guides/batch-processing.md` | Batch processing guide |
| `docs/guides/conditional-workflows.md` | Conditional workflow guide |

### Navigation Guide

See `docs/CLAUDE-GUIDE.md` for efficient documentation navigation, including:
- Trigger → Document mapping
- Investigation patterns
- Anti-patterns (what not to search for)

---

## Metadata

**Document Version**: 1.0
**Created**: 2025-12-29
**Author**: Claude (with Pete Taylor)
**Status**: Ready for Review

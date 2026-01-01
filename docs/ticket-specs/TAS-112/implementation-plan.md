# TAS-112 Implementation Plan: Cross-Language Handler Harmonization

**Created**: 2025-12-29
**Updated**: 2025-12-31 (Phases 1 & 2 Complete)
**Status**: Phases 1 & 2 COMPLETE - Ready for Phase 3 (Breaking Changes)
**Checkpoint API**: Deferred to TAS-125 "Batchable Handler Checkpoint"
**Branch**: `jcoletaylor/tas-112-cross-language-step-handler-ergonomics-analysis`
**Prerequisites**: Research Phases 1-9 Complete

> **Key Research Reference**: See [`domain-events-research-analysis.md`](./domain-events-research-analysis.md) for detailed cross-language domain event implementation analysis, type safety findings, and prioritized recommendations.

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

**Status**: Core infrastructure complete. FFI type safety improvements pending.

> **Research Finding**: The TypeScript FFI bridge uses manual JSON construction for domain events (`bridge.rs:409-423`), creating type drift risk. See [`domain-events-research-analysis.md`](./domain-events-research-analysis.md#2-type-safety-analysis) for details.

#### Tasks

| Task | Description | Validation | Status |
|------|-------------|------------|--------|
| Create `domain-events.ts` | BasePublisher, BaseSubscriber, StepEventContext, DomainEvent interfaces | Types compile | :white_check_mark: Complete |
| Implement publisher lifecycle hooks | before_publish, after_publish, on_publish_error, additional_metadata | Hooks called in correct order | :white_check_mark: Complete |
| Implement subscriber lifecycle hooks | before_handle, after_handle, on_handle_error | Hooks called in correct order | :white_check_mark: Complete |
| Create InProcessDomainEventPoller | FFI integration with Rust broadcast channel | Can receive events from Rust | :white_check_mark: Complete |
| Add PublisherRegistry | Centralized publisher management | Publishers registered and retrieved | :white_check_mark: Complete |
| Add SubscriberRegistry | Centralized subscriber management with start/stop | Subscribers start/stop correctly | :white_check_mark: Complete |
| Wire FFI poll adapter | Connect `createFfiPollAdapter()` to runtime | Events flow through FFI | :white_check_mark: Complete |
| **Create FfiDomainEventDto** | DTO struct with ts-rs generation (type safety fix) | Types auto-generated to `ffi/generated/` | :white_check_mark: Complete |
| **Create FfiDomainEventMetadataDto** | Metadata DTO with ts-rs generation | Types auto-generated | :white_check_mark: Complete |
| **Configure ts-rs export path** | `.cargo/config.toml` with TS_RS_EXPORT_DIR | Exports to `src/ffi/generated/` | :white_check_mark: Complete |
| **Add subscriber lifecycle tests** | 17 tests for BaseSubscriber and SubscriberRegistry | Tests pass | :white_check_mark: Complete |
| **Add BatchAggregationScenario** | Aggregation scenario detection for convergence steps | Matches Ruby/Python API | :white_check_mark: Complete |
| **Add aggregation helper methods** | detectAggregationScenario(), aggregateBatchWorkerResults() | Cross-language parity | :white_check_mark: Complete |
| Create payment publisher example | Custom payload transformation | Example compiles and runs | :white_check_mark: Complete |
| Create logging subscriber example | Pattern-based subscription | Receives matching events | :white_check_mark: Complete |
| Create metrics subscriber example | Counter aggregation | Counts events correctly | :white_check_mark: Complete |
| Integration tests | Full publish/subscribe cycle | Events flow through system | :white_check_mark: Complete (36 tests) |

#### Deliverables
- `workers/typescript/src/handler/domain-events.ts` :white_check_mark:
- `workers/typescript/src-rust/dto.rs` (FfiDomainEventDto, FfiDomainEventMetadataDto) :white_check_mark:
- `workers/typescript/src/ffi/generated/` (ts-rs auto-generated types) :white_check_mark:
- `workers/typescript/.cargo/config.toml` (ts-rs export configuration) :white_check_mark:
- `workers/typescript/src/handler/batchable.ts` (BatchAggregationScenario, aggregation helpers) :white_check_mark:
- Batchable unit tests (44 tests) :white_check_mark:
- `workers/typescript/src/examples/domain-events/` (publishers + subscribers) :white_check_mark:
  - `payment-publisher.ts` - PaymentEventPublisher, RefundEventPublisher
  - `logging-subscriber.ts` - AuditLoggingSubscriber, PaymentLoggingSubscriber
  - `metrics-subscriber.ts` - MetricsSubscriber, PaymentMetricsSubscriber
- Integration tests in `workers/typescript/tests/integration/domain-events-flow.test.ts` (36 tests) :white_check_mark:

---

### Stream B: Python Enhancements

**Priority**: HIGH
**Why**: Python is missing lifecycle hooks, type definitions, and structured registries that Ruby and TypeScript have.

**Ticket Mapping**: Updates TAS-119 (Domain Events) and TAS-117 (Batchable)

**Status**: HIGH priority items complete. Batchable helpers complete. Checkpoint API requires orchestration layer.

> **Research Finding**: Python has core domain event functionality but is missing lifecycle hooks, `EventDeclaration`/`StepResult`/`PublishContext` types, and `SubscriberRegistry`. See [`domain-events-research-analysis.md`](./domain-events-research-analysis.md#42-python-gaps) for complete gap analysis.

#### Tasks

| Task | Description | Validation | Priority | Status |
|------|-------------|------------|----------|--------|
| Add BasePublisher lifecycle hooks | `before_publish()`, `after_publish()`, `on_publish_error()` | Hooks called correctly | **HIGH** | :white_check_mark: Complete |
| Add BasePublisher.additional_metadata() | Custom metadata injection | Metadata appears in events | **HIGH** | :white_check_mark: Complete |
| Add BaseSubscriber lifecycle hooks | `before_handle()`, `after_handle()`, `on_handle_error()` | Hooks called correctly | **HIGH** | :white_check_mark: Complete |
| Add `EventDeclaration` type | Pydantic model matching TypeScript interface | Type available for publishers | **HIGH** | :white_check_mark: Complete |
| Add `StepResult` type | Pydantic model with success, result, metadata | Type available for context | **HIGH** | :white_check_mark: Complete |
| Add `PublishContext` type | Unified context for `publish()` method | Matches TypeScript API | **MEDIUM** | :white_check_mark: Complete |
| Create SubscriberRegistry class | Singleton with start_all/stop_all lifecycle | Matches Ruby/TypeScript pattern | **MEDIUM** | :white_check_mark: Complete |
| Update `InProcessDomainEvent` | Add `execution_result` field | Complete type parity | **MEDIUM** | :white_check_mark: Complete |
| **Add ExecutionResult type** | Pydantic model for step execution results | Type available for events | **MEDIUM** | :white_check_mark: Complete |
| Add handle_no_op_worker() | Batchable mixin helper | Consistency with Ruby/TS | MEDIUM | :white_check_mark: Complete |
| Add create_cursor_configs() | Worker-count based API (worker_count param) | Both APIs available | MEDIUM | :white_check_mark: Complete |
| Add checkpoint write API | update_checkpoint() or update_step_results() | Can persist checkpoint data | MEDIUM | :arrow_right: Deferred to TAS-125 |
| **Add BatchAggregationScenario** | Aggregation scenario detection for convergence steps | Matches Ruby API | **MEDIUM** | :white_check_mark: Complete |
| **Add aggregation helper methods** | detect_aggregation_scenario(), aggregate_batch_worker_results() | Cross-language parity | **MEDIUM** | :white_check_mark: Complete |

#### Deliverables
- `workers/python/python/tasker_core/domain_events.py` (lifecycle hooks, types) :white_check_mark:
- `workers/python/python/tasker_core/types.py` (InProcessDomainEvent, ExecutionResult) :white_check_mark:
- `workers/python/python/tasker_core/__init__.py` (exports) :white_check_mark:
- `workers/python/tests/test_domain_events.py` (unit tests) :white_check_mark:
- `workers/python/python/tasker_core/batch_processing/batchable.py` :white_check_mark:
  - `handle_no_op_worker()`, `create_cursor_configs()`, `get_batch_worker_inputs()`
  - `BatchAggregationScenario`, `detect_aggregation_scenario()`, `aggregate_batch_worker_results()`
- Batchable unit tests (27 tests) :white_check_mark:

---

### Stream C: FFI Boundary Types

**Priority**: HIGH
**Why**: Python/TypeScript use inline dicts/objects for FFI structures, causing type safety issues and harder debugging.

**Ticket Mapping**: Updates TAS-118 (Batch Processing Lifecycle)

**Status**: Complete. All FFI boundary types implemented with flexible cursor support.

#### Tasks

| Task | Description | Validation | Status |
|------|-------------|------------|--------|
| Python: Create BatchProcessingOutcome | Pydantic model matching Rust enum exactly | Serializes identically to Rust | :white_check_mark: Complete |
| Python: Enhance CursorConfig | Change cursor types from `int` to `Any`, add `batch_id` | Flexible cursor types work | :white_check_mark: Complete |
| Python: Align BatchWorkerContext | Match Rust BatchWorkerInputs structure exactly | Field names and types match | :white_check_mark: Complete |
| TypeScript: Create BatchProcessingOutcome | Interface/type matching Rust enum | Type-safe construction | :white_check_mark: Complete |
| TypeScript: Enhance CursorConfig | Change cursor types from `number` to `unknown`, add `batch_id` | Flexible cursor types work | :white_check_mark: Complete |
| TypeScript: Add BatchAggregationResult | Aggregation result type and helper function | Aggregation works | :white_check_mark: Complete |
| Document FFI boundary types | Create reference documentation | Clear serialization guidance | :white_check_mark: Complete |

#### Deliverables
- `workers/python/python/tasker_core/types.py` :white_check_mark:
  - `RustCursorConfig`, `BatchMetadata`, `RustBatchWorkerInputs`
  - `NoBatchesOutcome`, `CreateBatchesOutcome`, `BatchProcessingOutcome`
  - `BatchAggregationResult`, `aggregate_batch_results()`
- `workers/typescript/src/types/batch.ts` :white_check_mark:
  - `RustCursorConfig`, `BatchMetadata`, `RustBatchWorkerInputs`
  - `NoBatchesOutcome`, `CreateBatchesOutcome`, `BatchProcessingOutcome`
  - `BatchAggregationResult`, `aggregateBatchResults()`
- `docs/reference/ffi-boundary-types.md` :white_check_mark:

---

### Stream D: Examples & Tests

**Priority**: HIGH
**Why**: Python/TypeScript lack complete conditional workflow examples with convergence patterns.

**Ticket Mapping**: Updates TAS-120 (Conditional Workflows)

**Status**: Complete. Conditional approval workflows implemented for all languages with comprehensive E2E tests.

#### Tasks

| Task | Description | Validation | Status |
|------|-------------|------------|--------|
| Python: Create conditional_approval example | Complete workflow: validate → decision → branches → converge | E2E test passes | :white_check_mark: Complete |
| Python: Create YAML template | Task template for conditional approval | Loads and validates | :white_check_mark: Complete |
| Python: Add E2E tests | Small, medium, large amount routing scenarios + boundaries | All scenarios pass (6 tests) | :white_check_mark: Complete |
| TypeScript: Create conditional_approval example | Complete workflow matching Python | E2E test passes | :white_check_mark: Complete |
| TypeScript: Create YAML template | Task template matching Python | Loads and validates | :white_check_mark: Complete |
| TypeScript: Add E2E tests | Small, medium, large amount routing scenarios + boundaries | All scenarios pass (5 tests) | :white_check_mark: Complete |
| Document convergence patterns | How intersection semantics work | Clear explanation | :white_check_mark: Complete (in YAML templates) |

#### Deliverables
- `workers/python/tests/handlers/examples/conditional_approval_handlers.py` (314 lines, 6 handlers) :white_check_mark:
- `workers/typescript/tests/handlers/examples/conditional_approval/` (290 lines, 6 handlers) :white_check_mark:
- `tests/fixtures/task_templates/python/conditional_approval_handler_py.yaml` :white_check_mark:
- `tests/fixtures/task_templates/typescript/conditional_approval_handler_ts.yaml` :white_check_mark:
- `tests/e2e/python/conditional_approval_test.rs` (6 E2E tests) :white_check_mark:
- `tests/e2e/typescript/conditional_approval_test.rs` (5 E2E tests) :white_check_mark:

---

## Validation Summary (2025-12-31)

On New Year's Eve 2025, a comprehensive validation was performed against all claimed implementation progress. All validation agents confirmed the implementation status.

### Test Suite Results ✅

| Language | Claimed | Validated | Status |
|----------|---------|-----------|--------|
| TypeScript | 578 tests | 578 tests | ✅ All passing (updated 2025-12-31 PM with integration tests) |
| Python | 351 tests | 351 tests | ✅ All passing |

### Stream A: TypeScript Domain Events - VALIDATED COMPLETE ✅

| Component | Lines | Status |
|-----------|-------|--------|
| `domain-events.ts` | 1,521 | ✅ Complete |
| - BasePublisher with lifecycle hooks | ✓ | beforePublish, afterPublish, onPublishError, additionalMetadata |
| - BaseSubscriber with lifecycle hooks | ✓ | beforeHandle, afterHandle, onHandleError |
| - PublisherRegistry | ✓ | Singleton with freeze/validation |
| - SubscriberRegistry | ✓ | Singleton with start/stop lifecycle |
| - InProcessDomainEventPoller | ✓ | FFI integration with setPollFunction() |
| `dto.rs` FfiDomainEventDto | ✓ | ts-rs generation configured |
| `dto.rs` FfiDomainEventMetadataDto | ✓ | ts-rs generation configured |
| `.cargo/config.toml` ts-rs export | ✓ | TS_RS_EXPORT_DIR configured |
| `src/ffi/generated/` | 12 files | Auto-generated TypeScript types |
| `batchable.ts` BatchAggregationScenario | ✓ | Cross-language standard |
| `batchable.ts` aggregation helpers | ✓ | detectAggregationScenario, aggregateBatchWorkerResults |
| Test coverage | 1,005 lines | domain-events.test.ts comprehensive |

### Stream B: Python Enhancements - VALIDATED COMPLETE ✅

| Component | Lines | Status |
|-----------|-------|--------|
| `domain_events.py` | ~1,350 | ✅ Complete |
| - BasePublisher with lifecycle hooks | ✓ | before_publish, after_publish, on_publish_error, additional_metadata |
| - BaseSubscriber with lifecycle hooks | ✓ | before_handle, after_handle, on_handle_error |
| - SubscriberRegistry | ✓ | Thread-safe singleton with double-checked locking |
| `types.py` EventDeclaration | ✓ | Pydantic model |
| `types.py` StepResult | ✓ | Pydantic model |
| `types.py` PublishContext | ✓ | Pydantic model |
| `types.py` ExecutionResult | ✓ | Pydantic model |
| `types.py` InProcessDomainEvent.execution_result | ✓ | TAS-112 field added |
| Test coverage | 1,147 lines | test_domain_events.py comprehensive |

### Stream C: FFI Boundary Types - VALIDATED COMPLETE ✅

| Component | TypeScript | Python | Status |
|-----------|------------|--------|--------|
| RustCursorConfig (flexible cursors) | `unknown` type | `Any` type | ✅ Complete |
| BatchProcessingOutcome (discriminated union) | ✓ | ✓ | ✅ Complete |
| NoBatchesOutcome | ✓ | ✓ | ✅ Complete |
| CreateBatchesOutcome | ✓ | ✓ | ✅ Complete |
| BatchAggregationResult | ✓ | ✓ | ✅ Complete |
| aggregateBatchResults() | ✓ | ✓ | ✅ Complete |
| Factory functions (noBatches, createBatches) | ✓ | ✓ | ✅ Complete |
| Type guards (isNoBatches, isCreateBatches) | ✓ | - | ✅ Complete |

### Batchable Aggregation Methods - VALIDATED COMPLETE ✅

| Method | TypeScript | Python | Status |
|--------|------------|--------|--------|
| BatchAggregationScenario | Interface | Dataclass | ✅ Complete |
| detectAggregationScenario() | ✓ | detect() classmethod | ✅ Complete |
| noBatchesAggregationResult() | ✓ | ✓ | ✅ Complete |
| aggregateBatchWorkerResults() | ✓ | ✓ | ✅ Complete |
| Batchable tests (TAS-112 specific) | 12 tests | 23 tests | ✅ Complete |

### Remaining for Validation Gate 1

| Item | Status |
|------|--------|
| TypeScript domain event examples (payment publisher) | :white_check_mark: Complete |
| TypeScript domain event examples (logging subscriber) | :white_check_mark: Complete |
| TypeScript domain event examples (metrics subscriber) | :white_check_mark: Complete |
| TypeScript domain event integration tests | :white_check_mark: Complete (36 tests) |
| Stream D: Conditional workflow examples (Python/TypeScript) | :white_check_mark: Complete (11 E2E tests) |

### 🎉 VALIDATION GATE 1 PASSED (2025-12-31)

All Phase 1 streams are complete:
- **Stream A**: TypeScript Domain Events - 1,521 lines, full lifecycle hooks, FFI integration
- **Stream B**: Python Enhancements - ~1,350 lines, lifecycle hooks, type definitions
- **Stream C**: FFI Boundary Types - Flexible cursors, discriminated unions, factory functions
- **Stream D**: Conditional Workflows - 11 E2E tests covering all routing scenarios

### Validation Methodology

The validation was performed using 5 parallel exploration agents:
1. TypeScript Domain Events agent - verified all components in domain-events.ts
2. Python Domain Events agent - verified all components in domain_events.py
3. TypeScript Batchable agent - verified BatchAggregationScenario and FFI types
4. Python Batchable agent - verified BatchAggregationScenario and FFI types
5. Test Runner agent - executed both test suites to confirm test counts

All agents reported comprehensive implementation matching the claimed status in session summaries.

---

## Phase 2: Rust Ergonomics

**Priority**: HIGH
**Parallel with Phase 1**: Can proceed independently — no dependencies.

**Ticket Mapping**: Updates TAS-114 (Base Handler), TAS-115 (API Handler), TAS-116 (Decision Handler), TAS-117 (Batchable)

**Status**: ✅ COMPLETE (validated 2025-12-31)

### Tasks

| Task | Description | Validation | Status |
|------|-------------|------------|--------|
| Enhance StepHandler trait | Add version(), capabilities(), config_schema() methods | Methods available on trait | :white_check_mark: Complete |
| Create APICapable trait | api_success(), api_failure(), classify_status_code() | Helpers construct correct results | :white_check_mark: Complete |
| Create DecisionCapable trait | decision_success(), skip_branches(), decision_failure() | Helpers construct correct outcomes | :white_check_mark: Complete |
| Create BatchableCapable trait | batch_analyzer_success(), batch_worker_success(), cursor helpers | Helpers construct correct outcomes | :white_check_mark: Complete |
| Add BatchWorkerOutcome type | Add to tasker-shared (currently missing) | Type available for use | :white_check_mark: Complete (BatchProcessingOutcome) |
| Create cursor helper methods | create_cursor_configs(), create_cursor_ranges() | Both APIs work correctly | :white_check_mark: Complete |
| Create example handlers | Handlers demonstrating each trait | Examples compile and run | :white_check_mark: Complete (755 lines) |
| Update Rust worker docs | Complete examples with new traits | Documentation accurate | :construction: Pending (Phase 4) |

### Implementation Summary

All capability traits are implemented in `tasker-worker/src/handler_capabilities.rs` (958 lines):

**APICapable** (lines 125-218):
- `api_success(step_uuid, data, status, headers, execution_time_ms)`
- `api_failure(step_uuid, message, status, error_type, execution_time_ms)`
- `classify_status_code(status)` → `ErrorClassification` enum

**DecisionCapable** (lines 256-344):
- `decision_success(step_uuid, step_names, routing_context, execution_time_ms)`
- `skip_branches(step_uuid, reason, routing_context, execution_time_ms)`
- `decision_failure(step_uuid, message, error_type, execution_time_ms)`

**BatchableCapable** (lines 391-614):
- `create_cursor_configs(total_items, worker_count)` → `Vec<CursorConfig>`
- `create_cursor_ranges(total_items, batch_size, max_batches)` → `Vec<CursorConfig>`
- `batch_analyzer_success(step_uuid, worker_template, configs, total_items, execution_time_ms)`
- `batch_worker_success(step_uuid, processed, succeeded, failed, skipped, execution_time_ms)`
- `no_batches_outcome(step_uuid, reason, execution_time_ms)`
- `batch_failure(step_uuid, message, error_type, retryable, execution_time_ms)`

### Deliverables
- `tasker-worker/src/handler_capabilities.rs` (958 lines) :white_check_mark:
  - `APICapable`, `DecisionCapable`, `BatchableCapable` traits
  - `ErrorClassification` enum for HTTP status classification
  - `HandlerCapabilities` unified trait
- `tasker-shared/src/messaging/execution_types.rs` :white_check_mark:
  - `StepExecutionResult` with factory methods
  - `DecisionPointOutcome` (NoBranches, CreateSteps)
  - `BatchProcessingOutcome` (NoBatches, CreateBatches)
  - `CursorConfig` for batch worker initialization
- `tasker-shared/src/models/core/batch_worker.rs` :white_check_mark:
  - `BatchWorkerInputs` for worker context
  - `CheckpointProgress` for resumability
- `workers/rust/src/step_handlers/capability_examples.rs` (755 lines) :white_check_mark:
  - `ExampleApiHandler` demonstrating APICapable
  - `ExampleDecisionHandler` demonstrating DecisionCapable
  - `ExampleBatchAnalyzer` and `ExampleBatchWorker` demonstrating BatchableCapable
  - `CompositeHandler` showing trait composition

---

## Validation Gate 1: Pre-Breaking Changes Checkpoint

Before proceeding to Phase 3, all of the following must be true:

### Technical Validation

#### TypeScript Domain Events
- [x] TypeScript domain events module created (`domain-events.ts`)
- [x] TypeScript BasePublisher with lifecycle hooks (before/after/error)
- [x] TypeScript BaseSubscriber with lifecycle hooks (before/after/error)
- [x] TypeScript PublisherRegistry and SubscriberRegistry
- [x] TypeScript InProcessDomainEventPoller with FFI integration
- [x] FFI poll adapter wired (`createFfiPollAdapter()`)
- [x] **FfiDomainEventDto created with ts-rs generation** (type safety fix)
- [x] **FfiDomainEventMetadataDto created with ts-rs generation** (type safety fix)
- [x] **ts-rs export path configured** (`.cargo/config.toml` with TS_RS_EXPORT_DIR)
- [x] **bridge.rs updated to use DTO serialization** (FfiDomainEventDto::from + to_json_string)
- [x] TypeScript domain event examples (publisher + subscribers) - payment-publisher.ts, logging-subscriber.ts, metrics-subscriber.ts
- [x] TypeScript domain event integration tests - domain-events-flow.test.ts (36 tests)

#### Python Enhancements
- [x] Python BasePublisher lifecycle hooks (`before_publish`, `after_publish`, `on_publish_error`)
- [x] Python BasePublisher `additional_metadata()` method
- [x] Python BaseSubscriber lifecycle hooks (`before_handle`, `after_handle`, `on_handle_error`)
- [x] Python `EventDeclaration`, `StepResult`, `PublishContext` types added (as Pydantic models)
- [x] Python SubscriberRegistry created (singleton pattern)
- [x] Python `InProcessDomainEvent` updated with `execution_result` field
- [x] Python `ExecutionResult` type added

#### FFI Boundary Types
- [x] Python BatchProcessingOutcome is explicit Pydantic model (`NoBatchesOutcome`, `CreateBatchesOutcome`)
- [x] TypeScript BatchProcessingOutcome is explicit interface (discriminated union)
- [x] Python/TypeScript CursorConfig supports flexible cursor types (`RustCursorConfig` with `Any`/`unknown`)

#### Unit Tests (Complete)
- [x] Python domain events unit tests (test_domain_events.py - 1147 lines)
- [x] TypeScript domain events unit tests (domain-events.test.ts - 1005 lines)
- [x] Python decision handler unit tests (test_decision_handler.py - 163 lines)
- [x] TypeScript decision handler unit tests (decision.test.ts - 427 lines)

#### Examples & Integration Tests
- [x] Python/TypeScript domain event example workflows - TypeScript examples complete
- [x] Python/TypeScript conditional workflow examples - 11 E2E tests passing
- [x] All examples pass E2E tests - verified via `tests/e2e/{python,typescript}/`
- [x] Rust handler traits complete with examples (Phase 2) - 755 lines in capability_examples.rs

### Quality Validation
- [x] All new code has test coverage (351 Python tests, 578 TypeScript tests passing)
- [x] No serialization mismatches at FFI boundaries (domain events use DTO serialization)
- [ ] Documentation updated for new features

**Milestone**: All languages have equivalent capabilities (additive changes only).

> **Reference**: See [`domain-events-research-analysis.md`](./domain-events-research-analysis.md#6-action-items) for detailed action items and priority levels.

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

All research documents are in `docs/ticket-specs/TAS-112/`:

### Phase Research Documents

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

### Implementation Research Documents

| Document | Content | Created |
|----------|---------|---------|
| [`domain-events-research-analysis.md`](./domain-events-research-analysis.md) | Deep-dive cross-language domain event analysis with type safety findings, FFI bridge comparison, and prioritized action items | 2025-12-30 |

This document provides:
- Cross-language implementation matrix (Ruby/TypeScript/Python/Rust)
- Type safety analysis (manual JSON construction risks)
- FFI bridge architecture comparison
- Gap analysis by language with specific file references
- Priority recommendations with code examples
- Data flow diagrams for durable and fast event paths

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

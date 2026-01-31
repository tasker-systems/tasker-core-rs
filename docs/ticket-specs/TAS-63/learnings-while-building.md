# TAS-63: Learnings While Building Code Coverage

## Key Decision: Integration Tests Are In Scope for Coverage

We established that **dependent-service-level integration tests are in scope** for expanding code coverage. Specifically:

- **PostgreSQL** is expected to be running for all coverage measurement. The database is a foundational dependency — there is no meaningful way to test most of tasker-shared's model layer, SQL functions, state machine persistence, or scope system without it.
- **The configured messaging backend** (PGMQ or RabbitMQ) is expected to be available when using `--features test-messaging` or `--all-features`.

This means coverage targets should be measured with `--all-features` and real infrastructure, not with mocked-out databases. The existing factory system (`TaskFactory`, `WorkflowStepFactory`, `ComplexWorkflowFactory`, etc.) and `#[sqlx::test]` annotation provide excellent test isolation — each test gets its own database transaction that rolls back automatically.

---

## Critical Finding: Orphaned Integration Test Modules

### The Problem

Rust's integration test system compiles each `.rs` file directly inside `tests/` as its own test binary. Subdirectories with `mod.rs` files are **not** automatically compiled — they must be explicitly referenced by a root `.rs` file.

In `tasker-shared`, we discovered that **5 entire module trees** in `tests/` were orphaned:

| Directory | Contents | Status |
|-----------|----------|--------|
| `tests/models/` | 13 test files across `core/`, `insights/`, `orchestration/` | Never compiled |
| `tests/state_machine/` | 8 test files (actions, events, guards, persistence, states, step/task state machines) | Never compiled |
| `tests/sql_functions/` | 5 test files (production validation, step readiness, system health, task execution, SQL executor) | Never compiled |
| `tests/database/` | 2 test files (pool tests, SQL function tests) | Never compiled |
| `tests/circuit_breaker/` | 1 test file | Never compiled; also referenced wrong crate |

Each directory had a `mod.rs` that correctly declared its submodules, but **no root `.rs` file in `tests/` referenced any of these directories**. The result: Rust silently ignored them. They appeared in the file tree, showed up in IDE navigation, but were never part of any build or CI run.

### How It Manifested

These tests had accumulated significant code rot:

- **Schema drift**: Fields like `skippable` were removed from `WorkflowStep`, `in_backoff_steps` removed from `SystemHealthCounts`, `processor_uuid` added to `NewTaskTransition` — none of these were caught because the tests never compiled.
- **API changes**: Methods renamed (`get_by_task` → `get_for_task`, `health_score` → `success_rate`), string fields replaced with enums (`execution_status: String` → `ExecutionStatus` enum), function signatures changed — all invisible.
- **State name changes**: Task states renamed (`in_progress` → `steps_in_process`), which would have been caught immediately by database CHECK constraints if the tests had ever run.
- **Privacy changes**: Structs made `pub(crate)` that tests still imported as `pub`.
- **Wrong crate references**: The `circuit_breaker/` tests imported `tasker_orchestration`, which is not a dependency of `tasker-shared`.

Despite all this, `cargo check --tests`, `cargo test`, and `cargo clippy` all passed cleanly. The orphaned files were invisible to every quality gate.

### The Fix

Rust does not support the `tests/foo.rs` + `tests/foo/` directory convention for implicit module resolution in integration tests (unlike in `src/`). The fix is to use explicit `#[path]` attributes in root test files:

```rust
// tests/models.rs — creates the "models" integration test binary
#[path = "models/core/mod.rs"]
pub mod core;
#[path = "models/insights/mod.rs"]
pub mod insights;
```

We created 4 root files (`models.rs`, `state_machine.rs`, `sql_functions.rs`, `database.rs`) and deleted the corresponding `mod.rs` files from the subdirectories (Rust disallows both `tests/foo.rs` and `tests/foo/mod.rs` simultaneously).

The circuit_breaker tests were removed entirely because they referenced `tasker_orchestration` — a different crate that `tasker-shared` does not depend on. Those tests, if still needed, belong in `tasker-orchestration`.

### Impact

| Metric | Before Fix | After Fix |
|--------|-----------|-----------|
| Tests discovered | 1,481 | 1,713 |
| Tests unlocked | — | +232 |
| Line coverage | 69.0% | 75.82% |
| Lines covered | 25,868 | 28,426 |

The targeted model files went from 6-12% coverage to 71-99% coverage. The tests expressed meaningful intent — by fixing their compilation and runtime errors, we gained genuine confidence in the system's behavior, not just a number.

---

## Guidance for Remaining Crates

### Audit Results (January 2026)

We audited all remaining Rust crates for the same orphan problem:

#### tasker-orchestration — HAS ISSUES

**Non-standard pattern**: Uses `tests/mod.rs` as a root integration test file. This works (Rust compiles it as a test binary named `mod`) but is unusual and causes a subtle problem:

- `tests/mod.rs` declares `pub mod state_manager;` and `pub mod task_initialization_cycle_detection_test;`
- But `tests/state_manager.rs` and `tests/task_initialization_cycle_detection_test.rs` also exist as standalone root files
- Result: **7 tests run twice** — once in the `mod` binary and once in the standalone binaries

The `services/`, `messaging/`, `web/`, and `web/auth/` subdirectories are all referenced through `mod.rs` and ARE being compiled (122 tests). But the duplication and non-standard pattern should be cleaned up.

**Recommended fix**: Rename `tests/mod.rs` to something like `tests/orchestration_tests.rs`, remove the duplicate `pub mod state_manager` and `pub mod task_initialization_cycle_detection_test` declarations (let those remain standalone), and use `#[path]` attributes for the subdirectories.

#### tasker-worker — HAS ORPHANED TESTS

`tests/integration/worker_system_test.rs` exists with 3 test functions but is **never compiled**. The `tests/integration/` directory has no `mod.rs` and no root file references it.

**Recommended fix**: Either create a root file with `#[path = "integration/worker_system_test.rs"]` or, if the tests are outdated, remove them. Check if they compile first — they may have the same code rot problem.

#### tasker-pgmq — CLEAN

Follows the standard pattern: root `.rs` files in `tests/` with a shared `common.rs` helper module. No subdirectories, no orphan risk.

#### tasker-client — CLEAN

Single test file, no subdirectories.

### Checklist for Future Coverage Work on Any Crate

1. **Audit `tests/` directory structure first**. List all subdirectories with `mod.rs` files. For each, verify a root `.rs` file references it. If not, those tests are orphaned.

2. **Check for `tests/mod.rs`**. This is a legal but non-standard pattern. It creates a test binary named `mod`. Verify it doesn't duplicate tests that also exist as standalone root `.rs` files.

3. **Count tests before and after**. Use `cargo nextest list --package <crate> --all-features 2>&1 | grep -c "::"` to count integration tests. Any new root file wiring should increase this count.

4. **Expect compilation failures in orphaned tests**. Files that were never compiled will have accumulated schema drift, API changes, renamed types, and removed fields. Budget time to triage: fix if the intent is sound, remove if the test references wrong crates or tests removed functionality.

5. **Expect runtime failures too**. Even after compilation fixes, tests may fail due to database CHECK constraints rejecting old state names, changed assertion conditions, or restructured return types. These are real signals — each fix improves confidence.

6. **Use `#[path]` attributes for subdirectory modules**. The pattern:
   ```rust
   // tests/my_tests.rs
   #[path = "my_tests/some_module.rs"]
   pub mod some_module;
   ```
   This is the only reliable way to reference subdirectory modules from integration test root files. Do NOT rely on implicit `tests/foo.rs` + `tests/foo/` resolution — it does not work for integration tests.

7. **Run coverage with `--all-features`**. The coverage task (`CRATE_NAME=<crate> cargo make coverage-crate`) already does this. Verify the test count in coverage output matches `cargo nextest list` — a mismatch means tests are being excluded.

### The Broader Lesson

Tests that exist on disk but aren't wired into the build system are worse than no tests at all. They create a false sense of coverage, accumulate code rot silently, and when finally discovered, require significant effort to rehabilitate. The Rust compiler's module system makes this particularly easy to get wrong with integration tests — subdirectories in `tests/` look like they should work but don't without explicit wiring.

**Prevention**: Any PR that adds a new `tests/` subdirectory should be verified with `cargo nextest list` to confirm the new tests actually appear in the test count.

---

## tasker-orchestration Coverage Improvement (January 30, 2026)

### Starting Point

The tasker-orchestration crate was at 31.60% line coverage (28.56% function) with 424 tests. The original analysis (`analysis-tasker-orchestration.md`) identified a 23.4 percentage point gap to the 55% target, with gaps concentrated in orchestration core infrastructure, result processing, gRPC, and the actor system.

### Structural Fix: tests/mod.rs Cleanup

Applied the recommended fix from the tasker-shared orphan audit: renamed `tests/mod.rs` to `tests/orchestration_tests.rs` with `#[path]` attributes for subdirectory modules. Also removed the obsolete `tests/triggers/` directory that tested deprecated PostgreSQL LISTEN/NOTIFY infrastructure (replaced by PGMQ in TAS-41). These trigger tests referenced columns and schemas that no longer exist. Test count went from 431 to 424 (7 duplicate tests eliminated, 7 obsolete tests removed).

### Phase 1: Inline Unit Tests (424 → 607 tests)

Added `#[cfg(test)]` unit test modules to source files that had zero or minimal test coverage. These tests require no database — they validate error types, type conversions, data structures, and pure functions.

| Source File | Tests Added | What Was Tested |
|-------------|-------------|-----------------|
| `orchestration/errors.rs` | 10 | From trait implementations (FinalizationError, BackoffError → OrchestrationError) |
| `orchestration/backoff_calculator.rs` | 18 | BackoffConfig defaults, BackoffContext builder, retry-after extraction, BackoffResult construction |
| `orchestration/error_handling_service.rs` | 9 | ErrorHandlingConfig, ErrorHandlingResult, ErrorHandlingAction variants, serialization |
| `orchestration/state_manager.rs` | 20 | StateTransitionRequest construction, TransitionOutcome variants, health summary |
| `grpc/conversions.rs` | 28 | Proto-to-domain type conversions (task states, step states, UUID parsing, JSON helpers) |
| `orchestration/hydration/finalization_hydrator.rs` | 14 | UUID extraction from PgmqMessage and QueuedMessage (valid, missing, invalid, null, numeric, empty) |
| `orchestration/hydration/task_request_hydrator.rs` | 12 | TaskRequestMessage parsing from queue messages (valid, invalid format, context/metadata preservation) |
| `web/middleware/operational_state.rs` | 10 | `is_health_or_metrics_endpoint` path matching (health, metrics, ready, live, API paths) |
| `web/middleware/request_id.rs` | 5 | RequestId struct (creation, as_str, clone, debug) |
| `orchestration/commands/types.rs` | 24 | All result types (TaskInitializeResult, StepProcessResult, TaskReadinessResult, TaskFinalizationResult, OrchestrationProcessingStats, SystemHealth) |
| `services/task_service.rs` | 13 | Error Display messages, is_client_error classification |
| `services/step_service.rs` | 10 | Error Display messages, From<StepQueryError> conversion |
| `api_common/operational_status.rs` | 6 | DatabasePoolUsageStats, OrchestrationStatus construction/clone/debug |
| `orchestration/channels.rs` | 14 (expanded from 3) | Notification/command channel send/recv/capacity/close, From conversions, ChannelFactory |

**Coverage after Phase 1**: 35.51% line / 32.46% function (+3.91 pp line / +3.90 pp function)

### Phase 2: Database Integration Tests (607 → 648 tests)

Added `#[sqlx::test]` integration tests in `tests/services/` that exercise query services and analytics against real PostgreSQL. Each test gets an isolated database transaction that rolls back automatically.

| Test File | Tests Added | Service Tested | Lines in Source |
|-----------|-------------|----------------|-----------------|
| `task_query_service_tests.rs` | 11 | `TaskQueryService` — get_task_with_context, list_tasks_with_context, to_task_response | 278 (was 0 tests) |
| `step_query_service_tests.rs` | 15 | `StepQueryService` — list_steps_for_task, get_step_with_readiness, audit history, ownership checks, to_step_response | 213 (was 0 tests) |
| `template_query_service_tests.rs` | 11 | `TemplateQueryService` — list_templates, get_template, template_exists, get_namespace | 342 (was 3 error-only tests) |
| `analytics_service_tests.rs` | 8 | `AnalyticsQueryService` + `AnalyticsService` — performance metrics, bottleneck analysis, cache-aside delegation | 243+250 (was unit-only) |

**Integration test setup pattern** (consistent across all service tests):

```rust
async fn setup_services(pool: PgPool) -> Result<(ServiceUnderTest, TaskInitializer)> {
    let registry = TaskHandlerRegistry::new(pool.clone());
    registry.discover_and_register_templates(&fixture_path()).await?;
    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
    let step_enqueuer = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let task_initializer = TaskInitializer::new(system_context, step_enqueuer);
    let service = ServiceUnderTest::new(pool);
    Ok((service, task_initializer))
}
```

**Coverage after Phase 2**: 35.70% line / 32.59% function (+0.19 pp line / +0.13 pp function)

### Coverage Progression Summary

| Phase | Tests | Line Coverage | Function Coverage | Delta (Line) |
|-------|-------|---------------|-------------------|--------------|
| Baseline | 424 | 31.60% | 28.56% | — |
| After structural fix | 424 | 31.60% | 28.56% | 0.00 pp |
| After unit tests | 607 | 35.51% | 32.46% | +3.91 pp |
| After integration tests | 648 | 35.70% | 32.59% | +0.19 pp |
| **Total improvement** | **+224** | **+4.10 pp** | **+4.03 pp** | — |

### Key Observations

**Unit tests provided 95% of the coverage lift.** The 183 unit tests added +3.91 pp while the 41 integration tests added only +0.19 pp. This is because:

1. **Unit tests cover code directly.** An inline `#[cfg(test)]` module in `backoff_calculator.rs` exercises the same file's functions directly. Coverage tools attribute every executed line to its source file.

2. **Integration tests have diminishing returns for already-tested code paths.** The query services (TaskQueryService, StepQueryService) were already partially exercised by the existing TaskService and StepService integration tests that delegate to them. Adding direct tests validates the query service API contract but doesn't illuminate many new source lines.

3. **Integration tests are valuable for correctness, not just coverage.** The 41 `#[sqlx::test]` tests validate real SQL function execution, ownership checking, pagination, and error classification against PostgreSQL — none of which unit tests can verify. They caught a real issue where `to_step_response` fallback behavior with `readiness: None` needed validation.

**Remaining coverage gap is dominated by infrastructure-heavy modules.** The analysis shows the largest untested files are:
- `command_processor_actor.rs` (574 coverable lines, 18.6%) — requires full actor system
- `orchestration_event_system.rs` (370 lines, 0%) — requires messaging infrastructure
- `bootstrap.rs` (444 lines, 9.7%) — wires entire system together
- `viable_step_discovery.rs` (342 lines, 7.3%) — SQL-driven, needs database fixtures
- `state_manager.rs` (442 lines, 17.4%) — coordinates SQL functions with state machines

These modules require either: (a) decomposing large functions into testable units, or (b) complex integration test infrastructure with messaging backends. The next phase of work should focus on **refactoring large files to extract testable logic** before writing more tests.

### Lessons for Future Coverage Work

1. **Start with unit tests.** They provide the best coverage-per-effort ratio. Look for files with `0%` coverage that contain enums, error types, type conversions, builder patterns, or pure functions.

2. **Integration tests complement, not replace, unit tests.** Write integration tests to validate database interactions, ownership checks, and SQL function correctness — not to inflate coverage numbers.

3. **The `#[sqlx::test]` pattern is excellent.** Automatic migration, transaction isolation, and pool injection make database tests as easy to write as unit tests. The `fixture_path()` + `TaskHandlerRegistry::discover_and_register_templates()` pattern provides repeatable template setup.

4. **Coverage ceilings exist for un-refactored code.** Files like `command_processor_actor.rs` at 1001 lines have complex control flow that resists testing from the outside. Refactoring to extract testable units (strategy pattern, pure functions, smaller methods) will unlock more coverage than adding more integration tests.

---

## Orchestration Refactoring Phase (January 30, 2026)

### What Was Accomplished

The refactoring phase addressed the coverage ceiling identified in lesson #4 above. Four large files (totaling 4,240 lines) were decomposed. All work was behavior-preserving — 506 library tests passing at each checkpoint.

**Dead code removal:**
- `state_manager.rs` (1,297 lines) — deleted. 11 of 14 methods were dead code. 3 used methods (`mark_task_in_progress`, `mark_step_enqueued`, `get_or_create_step_state_machine`) were inlined into their callers (`StepEnqueuer`, `StateInitializer`).
- Unused `current_queue_sizes: HashMap<String, i64>` removed from stats struct (never populated in production).

**Duplication reduction (~650 lines eliminated):**
- `orchestration_event_system.rs`: 1,359 → ~700 lines. Extracted `send_command_and_await()` helper (removed ~250 lines of triplicated send-and-wait), fire-and-forget notification helper, and deployment mode setup helper (removed ~125 lines of duplicated `start()` branches).
- `command_processor_actor.rs`: 1,001 → 366 lines. Business logic extracted into `CommandProcessingService`, leaving only thin routing + stats tracking.
- `viable_step_discovery.rs`: 583 → ~450 lines. Pure functions extracted for step request building and dependency filtering.

**New files created (testable surface):**
- `commands/service.rs` — `CommandProcessingService` with three explicit lifecycle flows
- `commands/pgmq_message_resolver.rs` — PGMQ signal-only resolution logic
- `event_systems/command_outcome.rs` — `CommandOutcome` enum with pure `from_*` classifiers
- `health_check_evaluator.rs` — Pure function for health status evaluation
- `event_systems/orchestration_statistics.rs` — Extracted statistics tracking

**Concurrency fix:**
- `OrchestrationProcessingStats` converted from `Arc<RwLock<struct>>` to `AtomicProcessingStats` with lock-free `AtomicU64` counters, consistent with the newer stats types in the codebase.

### The Three-Flow Lifecycle Model

The most significant design outcome was the explicit lifecycle flow model for command processing:

```
Flow 1 - Direct:      DomainObject ──────────────────────────────→ Actor
Flow 2 - FromMessage: QueuedMessage → Hydrator → DomainObject ──→ Actor → Ack
Flow 3 - FromEvent:   MessageEvent → PgmqResolver → QueuedMessage → (Flow 2)
```

This model makes the provider-agnostic vs PGMQ-specific boundary explicit:
- **Flow 1** has no messaging dependency at all — just actor delegation
- **Flow 2** uses `MessageClient` only for the final `ack_message()` — provider-agnostic
- **Flow 3** is the only PGMQ-specific code path, isolated in `PgmqMessageResolver`

### InMemoryMessagingService Enables Unit Testing of Service Layer

The key testability insight: `commands/service.rs` can be tested with `InMemoryMessagingService` as the messaging provider, eliminating the need for PGMQ or RabbitMQ infrastructure for most test scenarios.

**What works with InMemoryMessagingService:**

| Scenario | Why It Works |
|----------|-------------|
| Flow 2 (FromMessage) — full hydrate-process-ack cycle | `InMemoryMessagingService` implements `ack_message()` via receipt handle. Hydrators parse JSON from `QueuedMessage<Value>`. |
| Flow 3 error paths — provider rejection | `InMemoryMessagingService.supports_fetch_by_message_id()` returns `false`, so Flow 3 methods return the expected `MessagingError`. |
| Health check evaluation | Pure function on `HealthStatusCaches` — no messaging needed. |

**What still needs real infrastructure:**

| Scenario | Why |
|----------|-----|
| Flow 1 (Direct) | Needs `ActorRegistry` with real or mock actors (not messaging-related). |
| Flow 3 success path | Needs `PgmqClient.read_specific_message()` — PGMQ-specific. |

**Construction pattern for tests:**

```rust
let provider = Arc::new(MessagingProvider::new_in_memory());
let router = MessageRouterKind::default();
let message_client = Arc::new(MessageClient::new(provider, router));
let service = CommandProcessingService::new(context, actors, message_client, health_caches);
```

This pattern means the service layer can have unit tests that:
1. Construct `QueuedMessage` values directly (no queue infrastructure needed)
2. Pass them through `*_from_message()` methods
3. Verify hydration, processing delegation, and message acknowledgment
4. Verify Flow 3 error handling when the provider doesn't support fetch-by-ID

### Lessons from the Refactoring

5. **Extract along provider boundaries, not just size boundaries.** The initial plan targeted file size reduction. The more valuable outcome was separating PGMQ-specific logic (`PgmqMessageResolver`) from provider-agnostic logic (the rest of the service). This boundary-based extraction directly enables testing with alternative providers.

6. **Dead code in the denominator is worse than dead code in a file.** The `state_manager.rs` removal eliminated 442 coverable lines from the denominator. This has a double benefit: it removes the false impression that those lines need tests, and it raises the coverage percentage for the same number of covered lines. The 20 tests that tested dead code paths were also removed — they validated code that never executed in production.

7. **The actor/service separation pattern (TAS-46) pays off at testing time.** Every actor in the codebase delegates to a service struct. The `CommandProcessingService` extraction followed this established pattern. The service can be constructed with test doubles (in-memory messaging, mock actors) while the actor remains a thin routing layer. This is the same pattern used by `TaskInitializer`, `StepEnqueuerService`, `TaskFinalizer`, etc.

8. **Lifecycle flow organization is self-documenting.** Grouping methods by flow (Direct → FromMessage → FromEvent) rather than by entity (task init → step result → finalization) makes the provider boundary visible in the source code. A reader immediately sees which methods are provider-agnostic and which require PGMQ.

# TAS-63: Orchestration System Refactor for Testability

## Goal

Decompose large, tightly-coupled source files in `tasker-orchestration` into smaller, testable units while removing dead code. All active functionality must be preserved. The refactoring unlocks coverage by making pure logic and repeating patterns independently testable without requiring full infrastructure (messaging backends, actor systems, database).

## Current State

| File | Lines | Coverable | Coverage | Tests |
|------|-------|-----------|----------|-------|
| `orchestration_event_system.rs` | 1,359 | 370 | **0.0%** | 0 |
| `command_processor_actor.rs` | 1,001 | 574 | **18.6%** | 0 inline |
| `viable_step_discovery.rs` | 583 | 342 | **7.3%** | 0 inline |
| `state_manager.rs` | 1,297 | 442 | **17.4%** | 20 inline |
| **Total** | **4,240** | **1,728** | — | **20** |

Coverage target: 55% crate-wide (currently 35.70%).

**Note**: `bootstrap.rs` (923 lines, 9.7% coverage) was dropped from scope. It was recently refactored into well-decomposed named steps, and the testable surface area there doesn't add or remove confidence in the system that doesn't already happen every time we build and launch the service.

---

## Priority Order

Ordered by expected testability gain per effort:

1. **orchestration_event_system.rs** — 0% coverage, massive code duplication, extractable pure logic
2. **command_processor_actor.rs** — Three repeated handler patterns, extractable health check, SWMR stats fix
3. **viable_step_discovery.rs** — Pure filtering logic buried inside DB-dependent methods
4. **state_manager.rs** — **Removal candidate**: 11 of 14 methods are dead code; 3 used methods can be migrated to callers

---

## File 1: `orchestration_event_system.rs` (1,359 lines → ~700 + extractables)

### Problem

Three distinct sources of duplication and tight coupling:

**A. `process_event()` (lines 636–1011)** — 375 lines with three nearly identical branches for `StepResult`, `TaskRequest`, `TaskFinalization`. Each branch does:
1. Clone message ID
2. Create oneshot channel
3. Build `OrchestrationCommand` variant
4. Send command through sender with channel monitoring
5. Handle send error with stats tracking
6. Wait for oneshot response
7. Match success/failure/skipped variants with per-variant logging and stats

The only difference between branches is: the command variant constructed, the result type matched, and the log messages.

**B. `process_orchestration_notification()` (lines 1138–1359)** — 221 lines with fire-and-forget notification routing. Four top-level notification variants, with the `Event` variant containing three sub-variants that each duplicate the same send-command pattern (create oneshot, send, log, update stats).

**C. `start()` method (lines 370–582)** — The `Hybrid` and `EventDrivenOnly` branches share ~50 lines of identical queue listener + event processing loop setup. Only difference: Hybrid also creates a fallback poller.

### Refactoring Plan

#### Extract 1: `OrchestrationStatistics` → `orchestration_statistics.rs`

Move `OrchestrationStatistics`, its `Clone` impl, `EventSystemStatistics` impl, `with_component_aggregation`, and `OrchestrationComponentStatistics` into a new file `event_systems/orchestration_statistics.rs`.

**Why**: 127 lines of standalone logic (no dependency on the event system itself). All methods are pure computation over atomic counters and latency deques. Fully unit-testable.

**Tests unlocked**: `processing_rate()` with various latency distributions, `average_latency_ms()`, `deployment_mode_score()` edge cases (zero events, all failures, high latency), `with_component_aggregation()` with mock stats, `Clone` correctness with concurrent state.

#### Extract 2: Command dispatch helper with `CommandOutcome` enum

Add a private helper method to eliminate the triplicated send-and-wait pattern in `process_event()`. Use an **enum dispatch** approach since we have an intentionally exhaustive list of command results:

```rust
/// Classifies any command result into a uniform outcome for stats and logging.
pub(crate) enum CommandOutcome {
    Success,
    Failed(String),
    Skipped(String),
}

impl CommandOutcome {
    pub fn from_step_result(result: &StepProcessResult) -> Self { ... }
    pub fn from_task_initialize_result(result: &TaskInitializeResult) -> Self { ... }
    pub fn from_task_finalization_result(result: &TaskFinalizationResult) -> Self { ... }

    pub fn is_success(&self) -> bool { ... }
    pub fn is_failure(&self) -> bool { ... }
    pub fn display_label(&self) -> &str { ... }
}
```

The dispatch helper then becomes:

```rust
/// Send a command and await its result, tracking statistics throughout.
async fn send_command_and_await(
    &self,
    command: OrchestrationCommand,
    event_label: &str,
    msg_id: &str,
    classify: impl FnOnce(&/* result type */) -> CommandOutcome,
) -> Result<(), DeploymentModeError>
```

**Why**: Eliminates ~250 lines of duplicated code across three `process_event()` branches. The three event type branches become ~10 lines each: construct command, call helper with the appropriate `CommandOutcome::from_*` classifier, done.

**Design decision**: We use an enum instead of a trait because the result type set is intentionally finite and exhaustive. An enum avoids trait generics, keeps the code concrete, and the match arms serve as documentation of exactly which result types exist.

**Tests unlocked**: The `CommandOutcome` enum and its `from_*` constructors are pure functions — fully unit-testable. The helper method itself can be tested with a mock command sender. The result classification logic (success/failure/skip) becomes independently verifiable.

#### Extract 3: Notification dispatch helper

Add a similar fire-and-forget helper for `process_orchestration_notification()`:

```rust
/// Send a fire-and-forget command through the command channel.
async fn fire_and_forget_command(
    command: OrchestrationCommand,
    command_sender: &OrchestrationCommandSender,
    command_channel_monitor: &ChannelMonitor,
    statistics: &Arc<OrchestrationStatistics>,
    label: &str,
)
```

**Why**: The five send-and-forget blocks in `process_orchestration_notification()` share identical channel-send + monitor + stats logic. Extracting this reduces the method from ~221 lines to ~80 lines.

#### Extract 4: Listener setup helper

Extract the shared queue listener + event processing loop setup from `start()`:

```rust
async fn create_and_start_queue_listener(
    &mut self,
) -> Result<(), DeploymentModeError>
```

**Why**: `Hybrid` and `EventDrivenOnly` branches share ~50 identical lines for listener config, channel creation, monitor initialization, listener creation, and event loop spawning.

### Expected Result

- `orchestration_event_system.rs`: ~700 lines (from 1,359)
- New `orchestration_statistics.rs`: ~140 lines (all unit-testable)
- New `CommandOutcome` enum: ~40 lines (all unit-testable)
- Net new testable surface: ~230 lines of pure/near-pure logic
- Reduced duplication: ~400 lines eliminated

---

## File 2: `command_processor_actor.rs` (1,001 lines → ~600 + extractables)

### Problem

The `CommandHandler` has three groups of three handlers each, with significant structural duplication:

**Group 1: Message Event handlers** (PGMQ signal-only) — `handle_task_initialize_from_message_event`, `handle_step_result_from_message_event`, `handle_task_finalize_from_message_event`. Each does:
1. Provider guard clause (identical ~12 lines)
2. Parse message ID from string to i64 (identical ~5 lines)
3. Read specific message from PGMQ (identical ~15 lines, different error messages)
4. Wrap as `QueuedMessage` (identical 1 line)
5. Delegate to corresponding message handler

**Group 2: Message handlers** (provider-agnostic) — `handle_task_initialize_from_message`, `handle_step_result_from_message`, `handle_task_finalize_from_message`. Each does:
1. Hydrate domain object from queued message (different hydrator)
2. Delegate to direct handler (different handler)
3. On success: ack message (identical ~8 lines)
4. On failure: log error, keep in queue (identical ~8 lines)

**Group 3: Direct handlers** — Already clean and minimal. No duplication.

**Additionally**: `OrchestrationProcessingStats` uses `Arc<std::sync::RwLock<OrchestrationProcessingStats>>` with 5 `u64` counter fields and a `HashMap<String, i64>` for `current_queue_sizes`. This is a pessimistic locking strategy — every counter increment acquires a write lock on the entire struct. The newer stats types in the codebase (`OrchestrationPollerStats`, `OrchestrationListenerStats`) correctly use `AtomicU64` for lock-free counter increments.

### Refactoring Plan

#### Extract 1: `health_check_evaluator.rs` (new file)

Extract the `handle_health_check()` method (lines 959–1000) into a standalone file at `orchestration/health_check_evaluator.rs` with a pure function:

```rust
pub fn evaluate_health_status(
    db_status: &DatabaseHealthStatus,
    channel_status: &ChannelHealthStatus,
    queue_status: &QueueHealthStatus,
    backpressure: &BackpressureStatus,
    actor_count: u32,
) -> SystemHealth
```

**Why**: 42 lines of pure logic that determines system health from cached status values. Currently untestable because it's buried inside `CommandHandler` which requires `Arc<ActorRegistry>`, `Arc<MessageClient>`, etc. As a free function in its own file, it can be tested with constructed input structs and serves as a clear module boundary.

**Tests unlocked**: All health status combinations — unknown (no evaluation), unhealthy (DB down, queue critical, backpressure), degraded (saturated channels, queue warning), healthy. Boundary conditions for each flag.

#### Extract 2: PGMQ message event pipeline

Extract the shared PGMQ guard + parse + fetch + wrap pattern into a helper:

```rust
/// Fetch a full message from PGMQ given a signal-only MessageEvent.
/// Returns the provider-agnostic QueuedMessage ready for processing.
async fn fetch_message_from_event(
    &self,
    message_event: &MessageEvent,
) -> TaskerResult<QueuedMessage<serde_json::Value>>
```

**Why**: The three `handle_*_from_message_event` methods share ~35 lines of identical boilerplate. After extraction, each becomes a 3-line method: `let msg = self.fetch_message_from_event(&event).await?; self.handle_*_from_message(msg).await`.

**Tests unlocked**: The fetch pipeline can be tested with mock provider capabilities. Guard clause behavior (non-PGMQ provider rejection), message ID parsing errors, and missing message scenarios become directly testable.

#### Extract 3: Message-then-ack pipeline

Extract the shared hydrate + delegate + ack/nack pattern into a generic helper:

```rust
/// Execute a message handler with automatic acknowledgment on success.
async fn process_message_with_ack<T, F, Fut>(
    &self,
    message: QueuedMessage<serde_json::Value>,
    handler_name: &str,
    hydrate_and_handle: F,
) -> TaskerResult<T>
where
    F: FnOnce(QueuedMessage<serde_json::Value>) -> Fut,
    Fut: Future<Output = TaskerResult<T>>,
```

**Why**: Each message handler follows the same 3-phase pattern: hydrate → handle → ack-on-success. The ack/nack logic with its logging is duplicated three times. This helper captures the pattern while allowing different hydration and handling logic via the closure.

#### Fix 4: SWMR conversion for `OrchestrationProcessingStats`

Convert `OrchestrationProcessingStats` from `Arc<std::sync::RwLock<OrchestrationProcessingStats>>` to a lock-free SWMR design:

**Current (problematic)**:
```rust
pub struct OrchestrationProcessingStats {
    pub tasks_initialized: u64,
    pub steps_processed: u64,
    pub tasks_finalized: u64,
    pub tasks_failed: u64,
    pub steps_failed: u64,
    pub current_queue_sizes: HashMap<String, i64>,  // NEVER POPULATED
}
// Used as: Arc<std::sync::RwLock<OrchestrationProcessingStats>>
```

**Target (lock-free)**:
```rust
pub struct OrchestrationProcessingStats {
    pub tasks_initialized: AtomicU64,
    pub steps_processed: AtomicU64,
    pub tasks_finalized: AtomicU64,
    pub tasks_failed: AtomicU64,
    pub steps_failed: AtomicU64,
}
// Used as: Arc<OrchestrationProcessingStats> — no lock needed
```

Changes:
1. **Remove `current_queue_sizes: HashMap<String, i64>`** — investigation confirmed it is initialized as `HashMap::new()` and never written to in production. Dead weight that forces the struct behind a lock.
2. **Convert 5 `u64` fields to `AtomicU64`** with `Ordering::Relaxed` for counter increments (consistent with `OrchestrationPollerStats` and `OrchestrationListenerStats`).
3. **Remove `RwLock` wrapper** — `Arc<OrchestrationProcessingStats>` is sufficient when all fields are atomic.
4. **Update all call sites** from `stats.write().unwrap().field += 1` to `stats.field.fetch_add(1, Ordering::Relaxed)`.
5. **Update snapshot/reporting** to use `stats.field.load(Ordering::Relaxed)`.

**Why**: The RwLock acquires a write lock on every single counter increment in the hot path — every task initialized, every step processed, every finalization. This is the exact anti-pattern that `AtomicU64` solves. The newer stats types in the codebase already follow the correct pattern.

**Tests unlocked**: Construction, increment, snapshot consistency (all pure, no async needed).

### Expected Result

- `command_processor_actor.rs`: ~600 lines (from 1,001)
- New `health_check_evaluator.rs`: ~50 lines (all unit-testable)
- `OrchestrationProcessingStats`: lock-free, fully testable
- ~150 lines of duplication eliminated
- Three handler groups reduced from ~180 lines each to ~30 lines each

---

## File 3: `viable_step_discovery.rs` (583 lines → ~450 + extractables)

### Problem

The file is structurally cleaner than the first two, but has two testability issues:

1. **`build_step_execution_requests()`** (lines 251–419, ~170 lines) — A sequential pipeline that mixes database fetches with data transformation. The data transformation logic (filtering by dependency satisfaction, building `StepExecutionRequest` from step template + task context) is pure but unreachable without a real database because it's interleaved with DB calls.

2. **`get_task_template_from_database()`** (lines 440–552, ~110 lines) — Database fetch + JSON deserialization + validation. The validation logic (empty steps check, deserialization error formatting) is testable but buried in the DB method.

3. **`TaskReadinessSummary`** (lines 556–582, ~27 lines) — Already clean with `is_complete()`, `is_blocked()`, `has_failures()` methods. Just needs tests.

### Refactoring Plan

#### Extract 1: Step execution request builder (pure function)

Extract the per-step transformation logic from `build_step_execution_requests()` into a standalone function:

```rust
/// Build a StepExecutionRequest from a viable step and its template configuration.
/// Pure function — no database access.
pub(crate) fn build_single_step_request(
    step: &ViableStep,
    task_uuid: Uuid,
    task_context: &serde_json::Value,
    step_template: &StepTemplate,
    previous_results: HashMap<String, StepExecutionResult>,
) -> StepExecutionRequest
```

**Why**: The per-step construction logic (handler class extraction, handler config mapping, timeout calculation, metadata assembly) is pure data transformation. Currently untestable because it's inside the `for step in viable_steps_filtered` loop that requires prior DB calls.

**Tests unlocked**: Handler config mapping, timeout fallback chain (`timeout_ms` → `timeout_seconds` → default 30s), metadata construction with various attempt counts, previous results inclusion.

#### Extract 2: Dependency filter (pure function)

Extract the dependency satisfaction filter into a standalone function:

```rust
/// Filter viable steps to only those with satisfied dependencies.
/// Returns the filtered steps and a count of how many were removed.
pub(crate) fn filter_by_dependency_satisfaction(
    steps: &[ViableStep],
) -> (Vec<&ViableStep>, usize)
```

**Why**: Small but testable — the warning logic for filtered steps and the empty-after-filter early return are currently interleaved with the DB-dependent method.

#### Add unit tests for `TaskReadinessSummary`

The three methods (`is_complete`, `is_blocked`, `has_failures`) are already pure. Just need test coverage for edge cases: zero total steps, all complete, all failed, mixed states.

### Expected Result

- `viable_step_discovery.rs`: ~450 lines (from 583)
- ~60 lines of extracted pure functions
- `TaskReadinessSummary` fully covered
- Timeout fallback chain and dependency filtering independently verified

---

## File 4: `state_manager.rs` (1,297 lines → **removed**)

### Problem

Investigation revealed that `StateManager` is a largely unused abstraction layer. Of its 14 methods, **only 3 are called anywhere in production code**:

| Method | Caller | Location |
|--------|--------|----------|
| `mark_task_in_progress` | `StepEnqueuer` | `step_enqueuer.rs:328` |
| `mark_step_enqueued` | `StepEnqueuer` | `step_enqueuer.rs:522` |
| `get_or_create_step_state_machine` | `StateInitializer` | `state_initializer.rs:157` |

The remaining **11 methods are dead code**:

| Method | Status | Notes |
|--------|--------|-------|
| `evaluate_task_state` | Unused | SQL functions handle evaluation in production |
| `evaluate_step_state` | Unused | SQL functions handle evaluation in production |
| `calculate_recommended_task_state` | Unused | Recommendations are determined by SQL functions |
| `calculate_recommended_step_state` | Unused | Recommendations are determined by SQL functions |
| `transition_steps_to_completed` | Unused | Bulk transitions not called |
| `process_bulk_transitions` | Unused | Bulk transitions not called |
| `get_state_health_summary` | Unused | Health monitoring uses different path |
| `fail_step_with_error` | Unused | Error handling uses different path |
| `complete_step_with_results` | Deprecated | Ruby workers handle via MessageManager |
| `handle_step_failure_with_retry` | Deprecated | Ruby workers handle via MessageManager |
| `new` | Only by 2 callers | Instantiated in StepEnqueuer and StateInitializer |

The existing 20 unit tests test the unused recommendation and health summary functions — they validate code that never executes in production.

The user's assessment: the state machines (`TaskStateMachine`, `StepStateMachine`) are already robust and usable. StateManager adds an unnecessary wrapper-abstraction that isn't providing value. The "recommendation" actions in particular are speculative — SQL functions determine recommendations in the actual system.

### Removal Plan

#### Step 1: Migrate 3 used methods to their callers

**`mark_task_in_progress`** — Called by `StepEnqueuer` to transition a task to `InProgress` state. This is a thin wrapper around `TaskStateMachine::transition()`. Inline the state machine call directly in `StepEnqueuer`.

**`mark_step_enqueued`** — Called by `StepEnqueuer` to mark a step as `Enqueued`. Same pattern — inline the state machine call.

**`get_or_create_step_state_machine`** — Called by `StateInitializer` to ensure a step has a state machine record. This may involve a database upsert. Move to `StateInitializer` as a private method, or expose as a function on the state machine itself if it's a natural fit.

#### Step 2: Remove `StateManager` and its 20 tests

Once the 3 used methods are migrated, delete:
- `orchestration/state_manager.rs` (1,297 lines)
- The `StateManager` re-export from `orchestration/mod.rs`
- Any remaining imports/references across the crate

#### Step 3: Verify no regressions

Run the full test suite (`cargo test --all-features`) plus the integration test suite to confirm the migrated methods work identically.

### Expected Result

- **1,297 lines of code removed** (net, after migrating ~30 lines of used logic)
- **442 coverable lines removed** from the denominator — directly improves coverage percentage
- **20 tests removed** that tested dead code paths
- Callers become self-contained, depending directly on state machines rather than an intermediary
- The crate's architecture becomes more honest about what code actually executes

### Risk and Rollback

This is the highest-risk change in the plan. Mitigation:
- Grep the entire workspace for `StateManager` before removing to catch any references not found in the initial investigation
- Run the full CI suite after migration
- If any caller turns out to need the evaluation/recommendation logic, it can be reintroduced as focused, tested functions on the state machine types themselves rather than resurrecting the full `StateManager`

---

## Cross-Cutting: SWMR Stats Consistency

The investigation revealed inconsistent concurrency models for statistics tracking across the crate:

| Stats Type | Concurrency Model | Hot Path Lock? |
|------------|-------------------|----------------|
| `OrchestrationProcessingStats` | `Arc<RwLock<struct>>` | **Yes** — write lock on every increment |
| `OrchestrationStatistics` | `AtomicU64` + `Mutex<VecDeque>` | Partial — atomics for counters, lock for latency deque |
| `OrchestrationPollerStats` | `AtomicU64` + async `Mutex` for timestamps | **No** — atomics for hot path |
| `OrchestrationListenerStats` | `AtomicU64` + async `Mutex` for timestamps | **No** — atomics for hot path |

The `OrchestrationProcessingStats` conversion (File 2, Fix 4) brings it in line with the newer, correct pattern. The `OrchestrationStatistics` latency deque (File 1) uses a `Mutex<VecDeque<Duration>>` bounded to 1,000 entries — this is acceptable since latency tracking requires ordered insertion, and the deque is bounded. No change needed there.

---

## Execution Order and Dependencies

```
Phase 1: Independent extractions and cleanups (no cross-file dependencies)
├── [1A] Extract OrchestrationStatistics → orchestration_statistics.rs
├── [2A] Extract health_check_evaluator → health_check_evaluator.rs (new file)
├── [2D] SWMR conversion for OrchestrationProcessingStats
├── [3C] Add TaskReadinessSummary unit tests
└── [4]  StateManager removal (migrate 3 methods, delete file)

Phase 2: Duplication reduction (depends on Phase 1 patterns)
├── [1B] Add CommandOutcome enum + command dispatch helper
├── [1C] Extract notification dispatch helper
├── [1D] Extract listener setup helper
├── [2B] Extract PGMQ message event pipeline
├── [2C] Extract message-then-ack pipeline
├── [3A] Extract step execution request builder
└── [3B] Extract dependency filter function

Phase 3: Write tests for all extracted code
├── Test all Phase 1 extractions + removals
└── Test all Phase 2 extractions
```

Phase 1 items are independent and can be done in parallel. Phase 2 items build on Phase 1 patterns. Phase 3 is test writing for the newly testable surface.

---

## Constraints

- **All refactoring must be behavior-preserving.** No functional changes to active code paths.
- **Dead code removal is in scope.** Unused methods, deprecated code, and never-populated fields should be removed.
- **Public API changes require verification.** If `StateManager` is re-exported, removing it is an API change — verify no external crate depends on it.
- **Internal visibility changes are acceptable.** `pub(crate)` or `pub(super)` for extracted helpers.
- **No new dependencies.** Refactoring uses existing language features (enums, free functions, modules, atomics).
- **Tests run with `--all-features` and real PostgreSQL.** Extracted pure functions get standard `#[test]` tests; integration tests use `#[sqlx::test]`.

---

## Success Criteria

After all phases:
- `state_manager.rs` is removed (1,297 lines of dead/near-dead code eliminated)
- No remaining file exceeds 800 lines (currently two exceed 1,000)
- Each extracted function/module has unit tests
- `OrchestrationProcessingStats` uses lock-free atomics (consistent with crate conventions)
- `cargo test --all-features` passes with no regressions
- Coverage increases toward 40%+ (from 35.70%) through newly testable surface + reduced coverable denominator
- `cargo clippy --all-targets --all-features` passes clean

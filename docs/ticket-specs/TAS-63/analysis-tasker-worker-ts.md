# Coverage Analysis: tasker-worker-ts (TypeScript Worker)

**Current Coverage**: 66.55% line, 70.25% function
**Target**: 60% (PASSING)
**Headroom**: 6.55 percentage points above threshold (line), 10.25 points (function)

---

## Summary

The TypeScript worker is the only crate currently passing its 60% coverage threshold. Coverage is distributed unevenly across 48 files: 17 files have 100% line coverage, while 7 files sit below 50% line coverage. The largest coverage gaps are in the handler system (file I/O-heavy handler discovery), event system (orchestration coordinator), FFI layer (native library loading), and batch types (complex FFI boundary types). These low-coverage files are the most architecturally significant -- they represent the core infrastructure that ties TypeScript handlers to the Rust FFI runtime. Improving coverage in these areas would increase confidence in the worker's reliability without requiring FFI integration in the test environment.

**Key observations:**
- Most high-coverage files are pure data types, resolvers, and utility functions -- easy to unit test.
- Low-coverage files are dominated by I/O, FFI dependencies, or lifecycle orchestration -- harder to test without mocks.
- The largest single gap is `handler-system.ts` at 7.46% (310 uncovered lines of 335), which is the handler discovery/loading system.
- 0 functions covered in `event-system.ts` despite 18.49% line coverage (only type/config definitions counted as lines).

---

## Uncovered Files (0% Coverage)

No files have exactly 0% coverage. However, `src/events/event-system.ts` has **0% function coverage** (0 of 6 functions), meaning none of its runtime methods (`start()`, `stop()`, `isRunning()`, `getEmitter()`, `getStats()`, or the constructor) are exercised by tests. The 18.49% line coverage comes from type/interface definitions and module-level logger initialization code that executes on import.

---

## Lowest Coverage Files

| File | Lines | Line % | Functions | Func % | Category |
|------|-------|--------|-----------|--------|----------|
| `src/handler/handler-system.ts` | 25/335 | 7.46% | 2/28 | 7.14% | Handler discovery |
| `src/events/event-system.ts` | 27/146 | 18.49% | 0/6 | 0.0% | Event orchestration |
| `src/ffi/ffi-layer.ts` | 22/98 | 22.45% | 2/10 | 20.0% | FFI lifecycle |
| `src/types/batch.ts` | 31/100 | 31.0% | 4/13 | 30.77% | Batch types/helpers |
| `src/server/worker-server.ts` | 120/323 | 37.15% | 11/17 | 64.71% | Server lifecycle |
| `src/types/step-context.ts` | 60/146 | 41.10% | 7/16 | 43.75% | Step context |
| `src/bootstrap/bootstrap.ts` | 53/109 | 48.62% | 9/9 | 100.0% | Bootstrap API |
| `src/ffi/runtime.ts` | 44/80 | 55.0% | 7/7 | 100.0% | Runtime detection |
| `src/handler/registry.ts` | 104/176 | 59.09% | 15/21 | 71.43% | Handler registry |

---

## Gap Analysis by Priority

### High Priority

These files contain core infrastructure logic that, if buggy, could cause silent failures in production. Testing them improves reliability of the entire worker.

#### 1. `src/handler/handler-system.ts` -- 7.46% line coverage (310 lines uncovered)

**What it contains**: `HandlerSystem` class that owns handler registration and discovery. Includes directory scanning, index file importing, handler validation, and recursive file discovery. This is the primary entry point for loading user-defined handlers.

**Why it matters**: If handler loading fails silently, steps dispatch to missing handlers and error out. The discovery logic has multiple code paths (index file vs. directory scan, ALL_EXAMPLE_HANDLERS array vs. module exports, error handling for bad imports).

**Uncovered areas**:
- `loadFromPath()` -- directory existence check, index file import, fallback scanning
- `loadFromEnv()` -- environment variable reading
- `tryImportIndexFile()` -- iterating index file candidates
- `registerHandlersFromModule()` -- ALL_EXAMPLE_HANDLERS array detection
- `registerFromHandlerArray()` / `registerFromModuleExports()` -- iteration and validation
- `scanAndImportHandlers()` -- recursive directory traversal
- `processDirectoryEntry()` -- file/directory discrimination
- `isHandlerFile()` -- filename pattern matching
- `importHandlerFile()` -- dynamic import and export scanning
- `isValidHandlerClass()` -- handler class validation

**Tests needed**:
- Unit tests with a temporary directory containing handler files
- Test `loadFromPath` with: non-existent path, empty directory, directory with index.ts, directory without index.ts
- Test `isHandlerFile` with various filenames (.ts, .js, .d.ts, .test.ts, underscore-prefixed)
- Test `isValidHandlerClass` with valid handler class, plain function, object, null
- Test `registerHandlersFromModule` with ALL_EXAMPLE_HANDLERS array and with plain exports
- Test `loadFromEnv` with and without TYPESCRIPT_HANDLER_PATH set

#### 2. `src/events/event-system.ts` -- 18.49% line / 0% function coverage

**What it contains**: `EventSystem` class that coordinates the event emitter, event poller, and step execution subscriber. It is the unified lifecycle manager for event processing -- the component that ties the TypeScript worker to Rust's FFI event dispatch.

**Why it matters**: The start/stop lifecycle is critical for the worker to process steps at all. Incorrect startup ordering (subscriber before poller) or incomplete shutdown could cause missed events or resource leaks.

**Uncovered areas**:
- Constructor -- creates emitter, poller, subscriber with injected dependencies
- `start()` -- registers debug listener, starts subscriber then poller
- `stop()` -- stops poller, waits for in-flight handlers, stops subscriber
- `isRunning()`, `getEmitter()`, `getStats()` -- state accessors

**Tests needed**:
- Unit tests with mock `TaskerRuntime` and mock `HandlerRegistryInterface`
- Test construction: verify emitter, poller, subscriber are created with correct config
- Test `start()`: verify subscriber starts before poller, idempotency (calling start twice)
- Test `stop()`: verify poller stops before subscriber, idempotency when not running
- Test `getStats()`: verify it aggregates subscriber and poller stats

#### 3. `src/ffi/ffi-layer.ts` -- 22.45% line coverage (76 lines uncovered)

**What it contains**: `FfiLayer` class that manages FFI runtime loading and lifecycle. Handles runtime detection (Bun/Node/Deno), library path discovery from environment variable, and runtime creation/destruction.

**Why it matters**: This is the gateway between TypeScript and Rust. If loading fails, the entire worker is non-functional. The error messages guide developers during setup.

**Uncovered areas**:
- `load()` -- already-loaded check, path resolution, runtime creation
- `unload()` -- cleanup logic
- `getRuntime()` -- guard for unloaded state
- `getLibraryPath()`, `getRuntimeType()` -- accessors
- `findLibraryPath()` (static) -- environment variable reading, file existence check
- `createRuntime()` -- runtime type switch (bun/node/deno/unsupported)

**Tests needed**:
- Test `FfiLayer.findLibraryPath()` with: env var set to existing file, env var set to non-existent file, env var not set
- Test constructor with explicit runtimeType and libraryPath config
- Test `isLoaded()` returns false before load
- Test `getRuntime()` throws when not loaded
- Test `load()` idempotency (returns early when already loaded)
- Test `unload()` clears state, safe to call when not loaded

### Medium Priority

These files have partial coverage but contain important logic that warrants more thorough testing.

#### 4. `src/types/batch.ts` -- 31.0% line coverage (69 lines uncovered)

**What it contains**: Batch processing types and factory functions that cross the Rust-TypeScript FFI boundary. Includes `createBatchWorkerContext()`, `createBatches()`, `noBatches()`, type guards (`isNoBatches`, `isCreateBatches`), and `aggregateBatchResults()`.

**Why it matters**: These types are serialized by Rust and deserialized by TypeScript. Incorrect parsing of cursor configs or batch metadata could cause batch workers to process wrong data ranges.

**Uncovered areas**:
- `createBatchWorkerContext()` -- nested vs flat cursor_config format, checkpoint extraction
- `createBatches()` -- validation that cursorConfigs.length === workerCount
- `noBatches()`, `isNoBatches()`, `isCreateBatches()` -- factory and type guard functions
- `aggregateBatchResults()` -- null/undefined filtering, error collection, success rate calculation

**Tests needed**:
- Test `createBatchWorkerContext` with nested cursor_config format and flat format
- Test `createBatchWorkerContext` with and without checkpoint data
- Test checkpoint accessor properties (checkpointCursor, accumulatedResults, checkpointItemsProcessed, hasCheckpoint)
- Test `createBatches` validation error when lengths mismatch
- Test `aggregateBatchResults` with: empty array, null entries, mixed success/failure results, error collection truncation at maxErrors

#### 5. `src/server/worker-server.ts` -- 37.15% line coverage (203 lines uncovered)

**What it contains**: `WorkerServer` class orchestrating the complete worker lifecycle: FFI loading, handler registration, Rust worker bootstrapping, event processing, and graceful shutdown. This is the top-level entry point for running a TypeScript worker.

**Why it matters**: The 3-phase startup and 3-phase shutdown sequences are the most complex lifecycle management in the worker. Partial failures during startup (e.g., FFI loads but bootstrap fails) need proper cleanup.

**Uncovered areas**:
- `start()` -- state machine guards, 3-phase startup, error recovery
- `shutdown()` -- 3-phase shutdown, shutdown handler execution
- `healthCheck()` -- FFI health delegation, worker status check
- `status()` -- stats aggregation
- `initializePhase()`, `bootstrapPhase()`, `startEventProcessingPhase()` -- individual phases
- `cleanupOnError()` -- partial initialization cleanup

**Tests needed**:
- Test state machine: start from INITIALIZED succeeds, start from RUNNING throws, start from STARTING throws
- Test shutdown: from RUNNING completes, from SHUTTING_DOWN returns early, from non-RUNNING returns early
- Test `healthCheck()` when not running, when FFI unhealthy, when worker not running, when healthy
- Test `onShutdown()` handlers are called during shutdown
- Test `cleanupOnError()` handles partial initialization (event system exists but worker not running)

#### 6. `src/types/step-context.ts` -- 41.10% line coverage (86 lines uncovered)

**What it contains**: `StepContext` class that provides context to step handlers during execution. Includes `fromFfiEvent()` factory, dependency result extraction, checkpoint accessors, and batch-related convenience methods.

**Why it matters**: Every handler receives a StepContext. Incorrect extraction of input data, dependency results, or checkpoint data would cause handlers to operate on wrong data.

**Uncovered areas**:
- `fromFfiEvent()` -- task context extraction, dependency results, step config, retry info
- `getDependencyResult()` -- unwrapping nested {result: value} format
- `getInput()`, `getConfig()` -- typed accessor methods
- `isRetry()`, `isLastRetry()` -- retry state checks
- `getInputOr()` -- default value accessor
- `getDependencyField()` -- nested field extraction
- Checkpoint accessors: `checkpoint`, `checkpointCursor`, `checkpointItemsProcessed`, `accumulatedResults`, `hasCheckpoint()`
- `getDependencyResultKeys()`, `getAllDependencyResults()` -- batch result collection

**Tests needed**:
- Test `fromFfiEvent()` with a complete FfiStepEvent mock: verify all fields extracted correctly
- Test `getDependencyResult()` with: wrapped {result: value}, unwrapped primitive, missing key
- Test `getDependencyField()` with: single-level path, multi-level path, non-object intermediate, missing key
- Test `getInputOr()` with present value and missing value
- Test retry methods: isRetry() when retryCount is 0 and > 0, isLastRetry() at boundary
- Test checkpoint accessors with and without checkpoint data in workflow_step
- Test `getAllDependencyResults()` with matching and non-matching prefixes

#### 7. `src/handler/batchable.ts` -- 61.49% line coverage (176 lines uncovered)

**What it contains**: `BatchableMixin` class and `BatchableStepHandler` abstract class providing batch processing capabilities. Includes cursor config creation, batch outcome builders, aggregation scenario detection, checkpoint yielding, and the `applyBatchable()` helper.

**Why it matters**: Batch processing is a cross-language standard feature (TAS-112). The cursor math, aggregation logic, and checkpoint yielding need thorough testing to ensure consistent behavior across Rust/Python/Ruby/TypeScript.

**Uncovered areas**:
- `createCursorConfigs()` -- Ruby-style worker-count-based division
- `getBatchContext()` -- stepConfig/inputData/stepInputs fallback chain
- `getBatchWorkerInputs()` -- stepInputs extraction
- `handleNoOpWorker()` -- no-op placeholder detection
- `checkpointYield()` -- checkpoint result construction
- `detectAggregationScenario()` (instance method) -- delegation
- `noBatchesAggregationResult()` -- zero metrics result
- `aggregateBatchWorkerResults()` -- scenario-based aggregation
- `BatchableStepHandler.batchSuccess()` -- cursor config conversion, outcome creation
- `BatchableStepHandler.noBatchesResult()` -- no-batches outcome with reason
- `applyBatchable()` -- method binding to target

**Tests needed**:
- Test `createCursorConfigs()` with: exact division, uneven division, more workers than items, zero items
- Test `handleNoOpWorker()` with: no-op inputs, normal inputs, empty inputs
- Test `checkpointYield()` with: numeric cursor, string cursor, object cursor, with and without accumulatedResults
- Test `detectAggregationScenario()` instance method delegates correctly
- Test `aggregateBatchWorkerResults()` with: NoBatches scenario, WithBatches with custom aggregation fn, WithBatches with default aggregation
- Test `batchSuccess()` end-to-end: configs converted to RustCursorConfig format
- Test `noBatchesResult()` with and without reason
- Test `applyBatchable()` binds all methods correctly

### Lower Priority

These files are closer to the threshold or have limited uncovered surface area.

#### 8. `src/handler/registry.ts` -- 59.09% line coverage (72 lines uncovered)

**What it contains**: `HandlerRegistry` class with resolver chain support (TAS-93). Manages handler registration, lazy initialization, and flexible resolution via explicit mapping and class lookup resolvers.

**Uncovered areas**:
- `ensureInitialized()` -- concurrent initialization locking
- `resolveSync()` -- backwards compatibility sync resolution
- `getHandlerClass()` -- deprecated method
- `clear()` -- handler cleanup
- `addResolver()`, `getResolver()`, `listResolvers()`, `getResolverChain()` -- resolver chain accessors
- `debugInfo()` -- debug state export

**Tests needed**:
- Test concurrent `ensureInitialized()` calls resolve correctly
- Test `resolveSync()` behavior (best-effort sync resolution)
- Test `clear()` removes all handlers
- Test `addResolver()` / `getResolver()` / `listResolvers()` chain operations

#### 9. `src/events/event-poller.ts` -- 63.26% line coverage (97 lines uncovered)

**What it contains**: `EventPoller` class that runs a polling loop retrieving step events from FFI and dispatching them via the event emitter. Includes periodic starvation checks, cleanup, and metrics emission.

**Uncovered areas**:
- Private `poll()` method internals -- event loop, starvation check, cleanup, metrics emission
- `handleStepEvent()` -- emitter emission with error handling, callback invocation
- `checkStarvation()` -- starvation detection
- `emitMetrics()` -- metrics emission
- `handleError()` -- error event emission

**Tests needed**:
- Test `start()` / `stop()` lifecycle with mock runtime
- Test `poll()` processing multiple events per cycle
- Test starvation detection at configured interval
- Test error handling when runtime throws

#### 10. `src/bootstrap/bootstrap.ts` -- 48.62% line coverage (56 lines uncovered), 100% function coverage

**What it contains**: High-level TypeScript API for Rust worker lifecycle management. Wraps FFI calls with type-safe interfaces. All 9 functions are covered but many error branches are not exercised.

**Uncovered areas**:
- Error/edge branches in `bootstrapWorker()`, `stopWorker()`, `getWorkerStatus()`, `transitionToGracefulShutdown()`
- Fallback returns when runtime is not loaded

**Tests needed**:
- Test each function with: runtime not loaded, runtime throws during call
- Test `bootstrapWorker()` with undefined config

#### 11. `src/handler/domain-events.ts` -- 70.61% line coverage (169 lines uncovered)

**What it contains**: Domain events infrastructure including publishers, subscribers, event bus, and polling. A substantial file (575 lines) with publisher/subscriber lifecycle management.

**Uncovered areas**:
- Various publisher and subscriber lifecycle methods
- Event bus polling and delivery logic
- Error handling paths

**Tests needed**:
- Test publisher registration and event transformation
- Test subscriber event delivery
- Test event bus start/stop lifecycle

#### 12. `src/logging/index.ts` -- 60.47% line coverage (34 lines uncovered)

**What it contains**: Structured logging API that forwards to Rust tracing via FFI, with console fallback.

**Uncovered areas**:
- `logWarn()`, `logTrace()` -- individual log level functions
- Error handling in log functions (catch blocks)
- `clearLoggingRuntime()` -- test utility

**Tests needed**:
- Test each log level with and without installed runtime
- Test fallback to console when no runtime
- Test error recovery when runtime.logX() throws

---

## Recommended Test Plan

### Phase 1: Pure Unit Tests (no FFI required) -- Estimated +8-10% line coverage

1. **`src/types/batch.ts`** -- Test all factory functions, type guards, and `aggregateBatchResults()`
2. **`src/types/step-context.ts`** -- Test `fromFfiEvent()`, dependency extraction, checkpoint accessors
3. **`src/handler/handler-system.ts`** -- Test handler discovery with temp directories and mock modules
4. **`src/handler/batchable.ts`** -- Test cursor math, aggregation scenarios, checkpoint yielding

### Phase 2: Mock-Based Integration Tests -- Estimated +5-7% line coverage

5. **`src/events/event-system.ts`** -- Test lifecycle with mock runtime and mock registry
6. **`src/ffi/ffi-layer.ts`** -- Test path discovery, state management, error messages
7. **`src/server/worker-server.ts`** -- Test state machine transitions with mock dependencies
8. **`src/handler/registry.ts`** -- Test resolver chain operations, concurrent initialization

### Phase 3: Edge Cases and Error Paths -- Estimated +3-5% line coverage

9. **`src/bootstrap/bootstrap.ts`** -- Test error branches for all lifecycle functions
10. **`src/events/event-poller.ts`** -- Test poll cycle, starvation detection, metrics emission
11. **`src/logging/index.ts`** -- Test all log levels, fallback, error recovery
12. **`src/handler/domain-events.ts`** -- Test publisher/subscriber lifecycle

---

## Estimated Impact

| Phase | Files Targeted | Estimated Lines Added | Coverage Gain |
|-------|---------------|----------------------|---------------|
| Phase 1 | 4 files | ~300-400 test lines | +8-10% to ~75-77% |
| Phase 2 | 4 files | ~250-350 test lines | +5-7% to ~80-84% |
| Phase 3 | 4 files | ~150-200 test lines | +3-5% to ~83-89% |
| **Total** | **12 files** | **~700-950 test lines** | **~83-89% line coverage** |

Phase 1 alone would bring coverage to approximately 75-77%, providing a comfortable 15+ point margin above the 60% threshold. The most impactful single file to test is `handler-system.ts` (310 uncovered lines), followed by `worker-server.ts` (203 uncovered lines) and `batchable.ts` (176 uncovered lines).

**Files with 100% coverage (no action needed)**: `bootstrap/types.ts`, `events/event-emitter.ts`, `events/event-names.ts`, `examples/domain-events/index.ts`, `handler/base.ts`, `handler/decision.ts`, `registry/index.ts`, `registry/registry-resolver.ts`, `registry/resolvers/class-lookup.ts`, `registry/resolvers/explicit-mapping.ts`, `registry/resolvers/index.ts`, `server/shutdown-controller.ts`, `server/types.ts`, `types/error-type.ts`, `types/step-handler-result.ts`.

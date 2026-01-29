# Coverage Analysis: tasker-worker

**Current Coverage**: 16.54% line (6,103/36,899), 13.91% function (743/5,341)
**Target**: 55%
**Gap**: 38.46 percentage points (need ~14,200 additional lines covered)

---

## Summary

The `tasker-worker` crate is a large crate (70 source files, ~36,900 lines) with significant coverage gaps. Out of 70 files, **26 files have 0% coverage** and only **7 files meet or exceed the 55% target**. The crate contains critical infrastructure: the FFI dispatch channel bridging Rust to Ruby/Python/TypeScript (531 lines at 28.25%), the WorkerCore lifecycle manager (530 lines at 0%), and the entire web + gRPC API surface (both at 0%).

The coverage gap breaks into three categories:
1. **Zero-coverage runtime/infrastructure** (core.rs, bootstrap.rs, event_driven_processor.rs, step_claim.rs) -- these are integration-heavy but contain testable logic
2. **Zero-coverage API surface** (all web handlers, middleware, routes, gRPC services/server) -- these need API integration tests
3. **Low-coverage services** (health, metrics, template_query, step_execution, dispatch_service) -- these have some tests but need deeper coverage

---

## Uncovered Files (0% Coverage)

| File | Lines | Functions | Description |
|------|-------|-----------|-------------|
| `worker/core.rs` | 530 | 61 | **WorkerCore lifecycle** - actor startup, shutdown, health, event publishing |
| `worker/step_claim.rs` | 251 | 18 | **Step claiming** - state machine transitions, DB queries, metrics |
| `worker/event_driven_processor.rs` | 145 | 11 | Legacy bridge to WorkerEventSystem |
| `worker/worker_queues/fallback_poller.rs` | 160 | 15 | Queue polling safety net |
| `worker/services/shared.rs` | 108 | 10 | SharedWorkerServices construction |
| `testing/environment.rs` | 143 | 17 | Test environment validation |
| `web/handlers/templates.rs` | 122 | 11 | Template CRUD HTTP handlers |
| `web/middleware/auth.rs` | 96 | 5 | Authentication middleware |
| `web/routes.rs` | 47 | 5 | Route definitions |
| `web/handlers/health.rs` | 30 | 8 | Health check HTTP handlers |
| `web/handlers/config.rs` | 28 | 4 | Config HTTP handler |
| `web/handlers/metrics.rs` | 16 | 6 | Metrics HTTP handlers |
| `web/middleware/mod.rs` | 30 | 5 | Middleware setup |
| `web/middleware/request_id.rs` | 17 | 3 | Request ID middleware |
| `web/mod.rs` | 28 | 1 | Web app creation |
| `grpc/server.rs` | 101 | 13 | gRPC server setup and lifecycle |
| `grpc/interceptors/auth.rs` | 57 | 14 | gRPC auth interceptor |
| `grpc/services/templates.rs` | 39 | 6 | gRPC template service |
| `grpc/services/config.rs` | 29 | 5 | gRPC config service |
| `grpc/services/health.rs` | 11 | 5 | gRPC health service |
| `grpc/state.rs` | 24 | 7 | gRPC state |
| `bin/server.rs` | 71 | 7 | Server binary entrypoint |
| `bin/generate_openapi.rs` | 6 | 1 | OpenAPI generation binary |
| `worker/actors/messages.rs` | 16 | 1 | Actor message types |
| `worker/handlers/domain_event_callback.rs` | 14 | 5 | Domain event callback |
| `worker/traits.rs` | 2 | 1 | Worker traits |

**Total uncovered lines in 0% files: 2,091** (5.66% of total crate)

---

## Lowest Coverage Files (Non-Zero, Below 55%)

| File | Coverage | Lines Covered/Total | Description |
|------|----------|---------------------|-------------|
| `worker/services/step_execution/service.rs` | 5.57% | 19/341 | Step executor service |
| `web/state.rs` | 6.86% | 12/175 | Web state (SharedWorkerServices wrapper) |
| `worker/services/health/service.rs` | 7.65% | 28/366 | Health check service |
| `bootstrap.rs` | 11.95% | 49/410 | Worker bootstrap system |
| `worker/services/config_query/service.rs` | 12.20% | 10/82 | Config query service |
| `worker/services/metrics/service.rs` | 12.87% | 26/202 | Metrics service |
| `worker/services/template_query/service.rs` | 13.16% | 20/152 | Template query service |
| `worker/orchestration_result_sender.rs` | 17.24% | 10/58 | Result sender |
| `worker/handlers/completion_processor.rs` | 17.65% | 9/51 | Completion processor |
| `worker/actor_command_processor.rs` | 19.71% | 54/274 | Actor command routing |
| `worker/handlers/dispatch_service.rs` | 22.00% | 88/400 | Handler dispatch service |
| `worker/services/ffi_completion/service.rs` | 22.67% | 17/75 | FFI completion service |
| `worker/task_template_manager.rs` | 22.75% | 119/523 | Template cache management |
| `worker/step_event_publisher_registry.rs` | 23.81% | 60/252 | Event publisher registry |
| `worker/event_systems/worker_event_system.rs` | 27.24% | 88/323 | Worker event system |
| `worker/handlers/ffi_dispatch_channel.rs` | 28.25% | 150/531 | **FFI dispatch bridge** |
| `worker/step_event_publisher.rs` | 31.61% | 55/174 | Step event publisher |
| `worker/worker_queues/listener.rs` | 33.56% | 100/298 | Queue listener |
| `worker/actors/step_executor_actor.rs` | 34.92% | 66/189 | Step executor actor |
| `worker/actors/domain_event_actor.rs` | 42.55% | 20/47 | Domain event actor |
| `worker/actors/ffi_completion_actor.rs` | 46.88% | 15/32 | FFI completion actor |
| `worker/services/worker_status/service.rs` | 47.06% | 48/102 | Worker status service |
| `worker/channels.rs` | 47.32% | 53/112 | Channel management |

---

## Gap Analysis by Priority

### Critical Priority

These files contain core worker logic where bugs directly affect step execution correctness.

#### 1. `worker/core.rs` (0% -- 530 lines, 61 functions)

**What it contains**: WorkerCore is the central lifecycle manager for the entire worker. It manages:
- Worker initialization (`new()`, `new_with_event_system()`) -- creates all subsystems including ActorCommandProcessor, EventDrivenMessageProcessor, TaskTemplateManager, orchestration client, event router, domain event system, and in-process event bus
- Lifecycle management (`start()`, `stop()`) -- spawns background tasks for command processing, domain events, and completion processing, with graceful shutdown and JoinHandle awaiting
- Health reporting (`get_health()`) -- aggregates health from event-driven processor and template manager
- Step event publishing (`publish_step_events()`) -- invokes publisher registry for post-execution domain events
- Publisher registry management -- register/get custom step event publishers

**Why critical**: Every worker depends on WorkerCore. If initialization or shutdown has bugs, all step processing fails. The `start()` method spawns three concurrent background tasks; the `stop()` method must drain them all within timeouts or risk resource leaks.

**Recommended tests**:
- Unit test `WorkerCoreStatus` Display impl and state transitions
- Unit test `DispatchHandles` Debug impl
- Unit test `WorkerCore` Debug impl (requires mocking subsystems)
- Integration test: Create WorkerCore with mocked SystemContext, verify initialization succeeds
- Integration test: Start/stop lifecycle with verification of background task behavior
- Integration test: Publisher registry (register, get, get_or_default)
- Integration test: Health reporting aggregation

#### 2. `worker/step_claim.rs` (0% -- 251 lines, 18 functions)

**What it contains**: Step claiming logic that bridges message processing to step execution. Contains:
- `StepClaim::new()` constructor
- `get_task_sequence_step_by_uuid()` -- hydrates step context from database (fetches Task, WorkflowStep, step definition, transitive dependencies)
- `get_task_sequence_step_from_step_message()` -- same hydration but from a StepMessage
- `try_claim_step()` -- uses StepStateMachine to transition step to InProgress, increments attempts counter, records OpenTelemetry metrics

**Why critical**: Step claiming is the foundation of work distribution. Race conditions in claiming would cause duplicate execution or missed steps. The state machine transition and attempt counter update must be atomic.

**Recommended tests**:
- Integration test: `get_task_sequence_step_by_uuid()` with seeded database
- Integration test: `get_task_sequence_step_from_step_message()` with seeded database
- Integration test: `try_claim_step()` with step in Pending, Enqueued, and already-InProgress states
- Integration test: Concurrent claim attempts verify only one succeeds
- Unit test: Error paths (task not found, step not found, template not found)

#### 3. `worker/handlers/ffi_dispatch_channel.rs` (28.25% -- 150/531 lines)

**What it contains**: The FFI dispatch bridge (1,027 lines total with tests) that allows Ruby/Python/TypeScript to poll for work and submit completions. Contains:
- `FfiDispatchChannel::new()`, `with_circuit_breaker()`, `with_checkpoint_support()` -- construction
- `poll()` / `poll_async()` -- non-blocking event retrieval for FFI callers
- `complete()` / `complete_async()` -- completion submission with circuit breaker, post-handler callbacks, and backpressure handling
- `checkpoint_yield()` -- TAS-125 batch processing checkpoint with re-dispatch
- `metrics()` -- starvation detection and pending event tracking
- `cleanup_timeouts()` -- timeout detection and failure result generation
- `check_starvation_warnings()` -- aging event detection

**Why critical**: This is the only interface for non-Rust workers (Ruby, Python, TypeScript). Any bug here blocks all FFI handler execution. The `complete()` method has complex logic: circuit breaker check, try_send with retry loop, fire-and-forget callback spawning.

**Current test coverage**: Config builder tests, channel creation, empty poll, checkpoint-not-configured. Missing: actual poll/complete flow, circuit breaker integration, timeout cleanup, starvation detection, metrics, checkpoint yield success path.

**Recommended tests**:
- Unit test: `poll()` with messages in dispatch channel -- verify FfiStepEvent fields
- Unit test: `complete()` happy path -- verify completion_sender receives result and callback fires
- Unit test: `complete()` with circuit breaker open -- verify fast failure
- Unit test: `complete()` with full completion channel -- verify retry and timeout behavior
- Unit test: `complete_async()` happy path
- Unit test: `cleanup_timeouts()` -- seed pending events, verify timed-out events generate failure results
- Unit test: `metrics()` -- seed pending events with varying ages, verify starvation detection
- Unit test: `check_starvation_warnings()` with aged events
- Unit test: `checkpoint_yield()` with configured checkpoint service -- mock persistence and re-dispatch

#### 4. `worker/services/step_execution/service.rs` (5.57% -- 19/341 lines)

**What it contains**: The step executor service extracted from the command processor (TAS-69). Handles step claiming via state machine, state verification with exponential backoff, PGMQ message deletion after claim, FFI handler invocation via event publisher, domain event dispatch, and metrics recording.

**Why critical**: Contains the core step execution pipeline. Steps that fail to claim or execute correctly break the entire workflow.

**Recommended tests**:
- Integration test: Execute step from message -- verify claim, execution, result
- Integration test: State verification with backoff (mock database to return stale states)
- Unit test: Error paths -- handler error, handler panic, handler timeout
- Unit test: Domain event dispatch after execution

### High Priority

These files affect reliability, health monitoring, and API functionality.

#### 5. `bootstrap.rs` (11.95% -- 49/410 lines)

**What it contains**: Unified worker bootstrap system. Handles:
- `WorkerBootstrap::bootstrap()` -- auto-detect configuration, initialize context, create worker core
- `WorkerSystemHandle` -- lifecycle management (is_running, stop, status, take_dispatch_handles)
- `WorkerBootstrapConfig` -- configuration with From<&TaskerConfig> conversion
- Feature-gated steps: create shared services, web state, web server, gRPC server, shutdown handler

**Why high priority**: Bootstrap failures prevent the worker from starting at all. The multi-step initialization with conditional feature gates is complex.

**Recommended tests**:
- Unit test: `WorkerBootstrapConfig::default()` values (already exists)
- Unit test: `From<&TaskerConfig>` conversion with various configurations
- Unit test: `WorkerSystemHandle::is_running()`, `has_dispatch_handles()`, `take_dispatch_handles()`
- Integration test: Full bootstrap with test configuration
- Integration test: Bootstrap with web API disabled
- Integration test: Stop and verify shutdown signal propagation

#### 6. `worker/handlers/dispatch_service.rs` (22.0% -- 88/400 lines)

**What it contains**: Non-blocking handler dispatch with bounded parallelism. Contains:
- `HandlerDispatchService` -- receives dispatch messages, invokes handlers with semaphore-bounded concurrency
- `CapacityChecker` -- load shedding based on semaphore utilization
- `PostHandlerCallback` trait -- extensible post-execution hooks
- `NoOpCallback` -- default implementation
- Error handling: catches handler errors, panics, and timeouts

**Recommended tests**:
- Unit test: `CapacityChecker` -- test capacity checking, warning thresholds, load shedding
- Unit test: `NoOpCallback` implementation
- Integration test: Dispatch with mock handler registry -- verify concurrent execution bounded by semaphore
- Integration test: Handler panic recovery -- verify failure result sent
- Integration test: Handler timeout -- verify timeout result sent

#### 7. `worker/services/health/service.rs` (7.65% -- 28/366 lines)

**What it contains**: Health check service with detailed, basic, readiness, and liveness checks. Includes database connectivity, PGMQ health, circuit breaker status, distributed cache info, worker core status, template manager stats, and pool utilization.

**Recommended tests**:
- Unit test: Basic health check with mocked dependencies
- Unit test: Readiness check -- database connected, templates loaded
- Unit test: Liveness check -- lightweight probe
- Integration test: Detailed health with database pools, circuit breakers

#### 8. Web API Layer (0% -- 414 lines total)

Files: `web/handlers/templates.rs` (122), `web/middleware/auth.rs` (96), `web/routes.rs` (47), `web/handlers/health.rs` (30), `web/handlers/config.rs` (28), `web/middleware/mod.rs` (30), `web/middleware/request_id.rs` (17), `web/mod.rs` (28), `web/handlers/metrics.rs` (16)

**What they contain**: The entire REST API surface: route definitions, health/metrics/template/config handlers, authentication middleware, request ID middleware.

**Recommended tests**:
- Integration test: Mount axum app with test state, hit health endpoints
- Integration test: Template list/get/validate endpoints with mock services
- Integration test: Auth middleware with valid/invalid/missing tokens
- Unit test: Error response formatting in template handlers

### Medium Priority

These files affect operational observability, event processing, and template management.

#### 9. `worker/services/metrics/service.rs` (12.87% -- 26/202 lines)

**What it contains**: Metrics collection including queue depths, handler counts, event stats, template cache stats, and Prometheus formatting.

**Recommended tests**:
- Unit test: Metrics construction with mock dependencies
- Unit test: Prometheus format output
- Integration test: Worker metrics with database and queue state

#### 10. `worker/task_template_manager.rs` (22.75% -- 119/523 lines)

**What it contains**: Template cache management including discovery from handler registry, database synchronization, cache invalidation, namespace management, and handler metadata lookup.

**Recommended tests**:
- Unit test: Template discovery and namespace extraction
- Unit test: Cache stats reporting
- Integration test: Template synchronization with database
- Integration test: Cache invalidation and refresh

#### 11. `worker/actor_command_processor.rs` (19.71% -- 54/274 lines)

**What it contains**: Pure routing command processor that delegates WorkerCommand variants to appropriate actors. Creates ActorRegistry, DispatchChannels, and event subscriber integration.

**Recommended tests**:
- Unit test: Command routing for each WorkerCommand variant
- Integration test: Full command processing pipeline with mock actors
- Unit test: Event subscriber enablement

#### 12. `worker/event_systems/worker_event_system.rs` (27.24% -- 88/323 lines)

**What it contains**: Worker event system managing listener, fallback poller, and deployment mode switching (PollingOnly, EventDrivenOnly, Hybrid).

**Recommended tests**:
- Unit test: Configuration mapping for each deployment mode
- Integration test: Start/stop lifecycle
- Unit test: Statistics reporting

#### 13. `worker/step_event_publisher_registry.rs` (23.81% -- 60/252 lines)

**What it contains**: Registry for custom step event publishers with name-based lookup and default fallback.

**Recommended tests**:
- Unit test: Register, get, get_or_default, list names
- Unit test: Default publisher behavior
- Integration test: Publisher invocation with mock context

#### 14. gRPC API Layer (0% -- 261 lines total)

Files: `grpc/server.rs` (101), `grpc/interceptors/auth.rs` (57), `grpc/services/templates.rs` (39), `grpc/services/config.rs` (29), `grpc/services/health.rs` (11), `grpc/state.rs` (24)

**What they contain**: Worker gRPC server, auth interceptor, health/config/template services.

**Recommended tests**:
- Integration test: gRPC health service
- Integration test: gRPC template list/get
- Unit test: Auth interceptor with valid/invalid credentials
- Unit test: GrpcServer construction and handle management

### Lower Priority

These files are less critical or have limited testable logic.

#### 15. `worker/event_driven_processor.rs` (0% -- 145 lines)

Legacy bridge that delegates entirely to WorkerEventSystem. Contains config mapping and API compatibility.

**Recommended tests**:
- Unit test: `EventDrivenConfig::default()` values
- Unit test: `map_config_to_new_architecture()` output verification
- Integration test: Start/stop delegation to WorkerEventSystem

#### 16. `worker/worker_queues/fallback_poller.rs` (0% -- 160 lines)

Safety net polling for missed messages. Contains polling loop, namespace queue iteration, and statistics.

**Recommended tests**:
- Unit test: `WorkerPollerConfig::default()` values
- Unit test: `WorkerPollerStats` atomics
- Integration test: Poll namespace queue with mock messaging provider

#### 17. `worker/services/shared.rs` (0% -- 108 lines)

SharedWorkerServices construction from WorkerCore. Requires full integration context.

**Recommended tests**:
- Integration test: `from_worker_core()` with test WorkerCore
- Unit test: `is_auth_enabled()`, `uptime_seconds()`, `supported_namespaces()`

#### 18. `web/state.rs` (6.86% -- 12/175 lines)

Web state wrapping SharedWorkerServices with accessor delegation.

**Recommended tests**:
- Unit test: All accessor methods delegate correctly
- Unit test: `domain_event_stats()` aggregation
- Unit test: `circuit_breakers_health()` with/without provider

#### 19. `testing/environment.rs` (0% -- 143 lines)

Test environment validation utilities. These are test helpers themselves.

**Recommended tests**:
- Unit test: Environment validation logic
- Unit test: Safety check enforcement

#### 20. Binary entrypoints (0% -- 77 lines)

`bin/server.rs` (71 lines), `bin/generate_openapi.rs` (6 lines). These are thin wrappers and are difficult to unit test.

**Recommended tests**: Typically excluded from coverage targets as they are integration-tested via E2E tests.

---

## Recommended Test Plan

### Phase 1: Critical Coverage Gains (Target: +15 percentage points)

Focus on highest-line-count files with testable logic that does not require full infrastructure.

| File | Target Coverage | Lines to Cover | Test Type |
|------|----------------|----------------|-----------|
| `worker/core.rs` | 40% | ~212 | Integration (mock SystemContext) |
| `worker/handlers/ffi_dispatch_channel.rs` | 60% | ~168 | Unit (channel-based, no DB) |
| `worker/step_claim.rs` | 40% | ~100 | Integration (DB-backed) |
| `bootstrap.rs` | 35% | ~95 | Unit + Integration |
| `worker/handlers/dispatch_service.rs` | 50% | ~112 | Unit + Integration |

**Estimated lines covered**: ~687
**Estimated impact**: +1.86 percentage points

### Phase 2: Service Layer Coverage (Target: +10 percentage points)

| File | Target Coverage | Lines to Cover | Test Type |
|------|----------------|----------------|-----------|
| `worker/services/step_execution/service.rs` | 40% | ~117 | Integration |
| `worker/services/health/service.rs` | 40% | ~118 | Unit + Integration |
| `worker/task_template_manager.rs` | 45% | ~116 | Integration |
| `worker/actor_command_processor.rs` | 45% | ~69 | Integration |
| `worker/services/metrics/service.rs` | 40% | ~55 | Unit |
| `worker/services/template_query/service.rs` | 40% | ~41 | Unit |
| `worker/event_systems/worker_event_system.rs` | 50% | ~73 | Integration |
| `worker/step_event_publisher_registry.rs` | 50% | ~66 | Unit |

**Estimated lines covered**: ~655
**Estimated impact**: +1.78 percentage points

### Phase 3: API Surface Coverage (Target: +10 percentage points)

| File | Target Coverage | Lines to Cover | Test Type |
|------|----------------|----------------|-----------|
| Web handlers (all) | 60% | ~145 | Integration (axum test) |
| Web middleware (all) | 50% | ~72 | Integration |
| Web state/routes/mod | 50% | ~51 | Unit + Integration |
| gRPC services (all) | 50% | ~48 | Integration |
| gRPC server/auth | 40% | ~63 | Integration |

**Estimated lines covered**: ~379
**Estimated impact**: +1.03 percentage points

### Phase 4: Remaining Gaps (Target: +3 percentage points)

| File | Target Coverage | Lines to Cover | Test Type |
|------|----------------|----------------|-----------|
| `worker/event_driven_processor.rs` | 40% | ~58 | Unit + Integration |
| `worker/worker_queues/fallback_poller.rs` | 35% | ~56 | Integration |
| `worker/services/shared.rs` | 40% | ~43 | Integration |
| `web/state.rs` | 40% | ~58 | Unit |
| Various remaining gaps | 50%+ | ~200+ | Mixed |

**Estimated lines covered**: ~415
**Estimated impact**: +1.12 percentage points

---

## Estimated Impact

### Conservative Estimate

Achieving the recommended test plan across all phases:

| Phase | Lines Added | Cumulative Coverage |
|-------|-------------|---------------------|
| Current | 6,103/36,899 | 16.54% |
| Phase 1 | +687 | 18.40% |
| Phase 2 | +655 | 20.18% |
| Phase 3 | +379 | 21.20% |
| Phase 4 | +415 | 22.33% |

**Note**: The conservative estimate accounts only for directly tested lines. In practice, integration tests that exercise WorkerCore, bootstrap, and the dispatch pipeline will cover significantly more lines than the direct targets -- often 3-5x more lines through transitive execution.

### Realistic Estimate with Integration Test Coverage Multiplier

Integration tests for WorkerCore lifecycle, bootstrap, and dispatch service will transitively exercise:
- Channel management (`channels.rs` -- 112 lines)
- Actor registry (`registry.rs` -- 142 lines)
- Event subscriber (`event_subscriber.rs` -- 342 lines)
- Event router (`event_router.rs` -- 488 lines)
- Listener (`listener.rs` -- 298 lines)
- Domain event system (`domain_event_system.rs` -- 465 lines)

With integration test multiplier (estimated 3x coverage of direct targets):

| Phase | Direct Lines | Transitive Lines | Cumulative Coverage |
|-------|-------------|------------------|---------------------|
| Current | 6,103 | -- | 16.54% |
| Phase 1 | +687 | +1,400 | 22.19% |
| Phase 2 | +655 | +1,200 | 27.21% |
| Phase 3 | +379 | +600 | 29.87% |
| Phase 4 | +415 | +500 | 32.35% |

### Key Insight

Reaching 55% coverage for `tasker-worker` is achievable but requires a significant testing effort focused on integration tests. The crate's architecture -- where WorkerCore orchestrates many subsystems -- means that well-designed integration tests will have outsized coverage impact compared to isolated unit tests. The single most impactful test would be a WorkerCore lifecycle test that bootstraps, processes a step through the dispatch channel, and shuts down, as it would exercise ~60% of the crate's code paths transitively.

### Priority Files for Maximum Impact (lines at 0% that are most testable)

1. **`worker/core.rs`** (530 lines) -- Highest line count, central to all worker operations
2. **`worker/step_claim.rs`** (251 lines) -- Step claiming is independently testable with DB
3. **`worker/worker_queues/fallback_poller.rs`** (160 lines) -- Testable with mock messaging provider
4. **`worker/event_driven_processor.rs`** (145 lines) -- Thin delegation layer, easy to test
5. **`testing/environment.rs`** (143 lines) -- Pure logic, no dependencies needed
6. **`web/handlers/templates.rs`** (122 lines) -- Testable with axum test harness
7. **`worker/services/shared.rs`** (108 lines) -- Integration test with WorkerCore
8. **`grpc/server.rs`** (101 lines) -- Testable with tonic test harness

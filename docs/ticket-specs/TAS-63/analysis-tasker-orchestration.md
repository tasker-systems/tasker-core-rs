# Coverage Analysis: tasker-orchestration

**Current Coverage**: 31.60% line (11,790 / 37,306), 28.57% function (1,509 / 5,282)
**Target**: 55%
**Gap**: 23.4 percentage points (approximately 8,728 additional lines need coverage)

---

## Summary

The `tasker-orchestration` crate is the core orchestration engine containing actors, lifecycle management, result processing, web handlers, and gRPC services. At 31.6% line coverage it is substantially below the 55% target. The largest gaps are concentrated in four areas: (1) the orchestration core infrastructure (event systems, bootstrap, state management) at 0-10% coverage comprising 2,261 uncovered lines, (2) the result processing pipeline at 8-21% coverage with 589 uncovered lines, (3) the entire gRPC service layer at 0% coverage with 741 uncovered lines, and (4) the actor system including the critical command processor at 18-51% coverage with 556 uncovered lines. Closing these gaps through targeted integration and unit tests would add approximately 18.5 percentage points of coverage, bringing the crate to roughly 50%, with the remaining gap addressable through medium-priority modules.

## Uncovered Files (0% Coverage)

These 23 files have zero test coverage. Together they represent 1,771 coverable lines.

| File | Lines (coverable) | Description |
|------|-------------------|-------------|
| `orchestration/event_systems/orchestration_event_system.rs` | 370 | Queue-level event system coordinating PGMQ listener and fallback poller with EventDrivenSystem interface. Manages deployment mode transitions, event routing, and statistics. |
| `orchestration/error_handling_service.rs` | 217 | Bridges error classification with state transitions and backoff logic. Implements retry scheduling, permanent failure marking, and error-to-state mapping. |
| `orchestration/orchestration_queues/fallback_poller.rs` | 208 | Safety-net polling for orchestration queues. Provider-agnostic message pickup for missed events in production. |
| `grpc/services/dlq.rs` | 166 | gRPC DLQ service: investigation tracking, queue listing, staleness monitoring via protobuf. |
| `grpc/server.rs` | 121 | gRPC server bootstrap: service registration, reflection, health checking, lifecycle management. |
| `grpc/services/analytics.rs` | 88 | gRPC analytics service: performance metrics and bottleneck analysis endpoints. |
| `grpc/conversions.rs` | 87 | Proto-to-domain type conversions: task/step state mappings, UUID parsing, JSON-to-struct helpers. |
| `grpc/services/tasks.rs` | 82 | gRPC task service: CRUD operations, task creation from requests, streaming. |
| `grpc/interceptors/auth.rs` | 57 | gRPC authentication interceptor: API key and JWT validation for tonic requests. |
| `orchestration/errors.rs` | 54 | Error type conversions from FinalizationError to OrchestrationError. |
| `web/extractors.rs` | 47 | Custom Axum extractors: database connection pool selection, worker claims, request ID. |
| `grpc/services/templates.rs` | 45 | gRPC template service: template CRUD operations. |
| `grpc/services/config.rs` | 39 | gRPC config service: runtime configuration endpoints. |
| `grpc/services/steps.rs` | 33 | gRPC step service: step query and management endpoints. |
| `web/middleware/operational_state.rs` | 28 | Middleware for operational state checks (maintenance mode, degraded mode). |
| `web/middleware/mod.rs` | 22 | Middleware module re-exports and composition. |
| `orchestration/orchestration_queues/events.rs` | 14 | Queue event types for orchestration queue notifications. |
| `grpc/state.rs` | 12 | gRPC shared state container holding service references. |
| `grpc/services/health.rs` | 11 | gRPC health check service (liveness/readiness). |
| `bin/server.rs` | 58 | Server binary entry point: CLI args, config loading, server startup. |
| `bin/generate_openapi.rs` | 6 | OpenAPI spec generation binary. |
| `orchestration/lifecycle/batch_processing/mod.rs` | 3 | Module re-export for batch processing. |
| `orchestration/lifecycle/decision_point/mod.rs` | 3 | Module re-export for decision point. |

## Lowest Coverage Files (Above 0% but Below 55%)

| File | Lines (coverable) | Covered | Coverage % | Description |
|------|-------------------|---------|-----------|-------------|
| `orchestration/hydration/step_result_hydrator.rs` | 149 | 3 | 2.0% | Hydrates full StepExecutionResult from lightweight PGMQ messages via database lookup. |
| `orchestration/viable_step_discovery.rs` | 342 | 25 | 7.3% | SQL-driven step readiness discovery with state machine verification and dependency analysis. |
| `orchestration/lifecycle/result_processing/message_handler.rs` | 271 | 22 | 8.1% | Routes step result messages to appropriate handlers (state transitions, decision points, batch processing). |
| `orchestration/lifecycle/batch_processing/service.rs` | 203 | 18 | 8.9% | Dynamic batch worker instance creation: dataset analysis, cursor generation, transactional step creation. |
| `orchestration/bootstrap.rs` | 444 | 43 | 9.7% | Unified orchestration system bootstrap across all deployment modes with lifecycle management. |
| `orchestration/hydration/finalization_hydrator.rs` | 63 | 7 | 11.1% | Extracts and validates task_uuid from PGMQ finalization messages. |
| `orchestration/lifecycle/result_processing/metadata_processor.rs` | 80 | 11 | 13.8% | Processes orchestration metadata from step execution results. |
| `orchestration/hydration/task_request_hydrator.rs` | 50 | 7 | 14.0% | Hydrates task request data from queue messages. |
| `orchestration/lifecycle/result_processing/state_transition_handler.rs` | 162 | 23 | 14.2% | Handles EnqueuedForOrchestration state transitions to fix race conditions. |
| `orchestration/lifecycle/decision_point/service.rs` | 255 | 41 | 16.1% | Decision point processing: dynamic workflow step creation from decision outcomes. |
| `orchestration/state_manager.rs` | 442 | 77 | 17.4% | High-level state management coordinating SQL functions with state machine transitions. |
| `actors/command_processor_actor.rs` | 574 | 107 | 18.6% | Unified actor receiving commands via MPSC channels, routing to task/result/finalizer/enqueuer actors. |
| `orchestration/staleness_detector.rs` | 274 | 58 | 21.2% | Background service detecting and transitioning stale tasks via SQL function calls. |
| `orchestration/lifecycle/result_processing/task_coordinator.rs` | 108 | 23 | 21.3% | Coordinates task-level finalization when steps complete. |
| `orchestration/core.rs` | 221 | 54 | 24.4% | OrchestrationCore: command-pattern bootstrap, channel setup, health monitoring. |
| `orchestration/lifecycle/task_finalization/state_handlers.rs` | 216 | 56 | 25.9% | Task execution state handlers during finalization (all-complete, has-errors, in-progress). |
| `health/db_status.rs` | 90 | 24 | 26.7% | Database health status checks. |
| `orchestration/event_systems/task_readiness_event_system.rs` | 86 | 26 | 30.2% | Task readiness event system using LISTEN/NOTIFY for event-driven step discovery. |
| `orchestration/lifecycle/step_result_processor.rs` | 69 | 22 | 31.9% | Delegates step result processing to the result processing module. |
| `web/state.rs` | 145 | 49 | 33.8% | Web application state: database pools, services, circuit breaker references. |
| `orchestration/backoff_calculator.rs` | 213 | 72 | 33.8% | Exponential backoff calculation with jitter for retry scheduling. |
| `orchestration/orchestration_queues/listener.rs` | 330 | 112 | 33.9% | PGMQ queue listener: LISTEN/NOTIFY integration, message dispatch, statistics tracking. |
| `orchestration/event_systems/unified_event_coordinator.rs` | 300 | 106 | 35.3% | Unified coordinator managing multiple event systems across deployment modes. |
| `orchestration/lifecycle/task_request_processor.rs` | 119 | 43 | 36.1% | Task request processing: validation, template loading, initialization delegation. |
| `orchestration/lifecycle/task_finalization/event_publisher.rs` | 143 | 54 | 37.8% | Publishes task completion/error events to messaging queues. |
| `orchestration/lifecycle/step_enqueuer_services/state_handlers.rs` | 164 | 66 | 40.2% | Step state handling during enqueueing (pending, ready, blocked). |
| `orchestration/task_readiness/fallback_poller.rs` | 129 | 53 | 41.1% | Fallback poller for task readiness with circuit breaker integration. |
| `orchestration/lifecycle/task_finalization/completion_handler.rs` | 331 | 147 | 44.4% | Task completion logic: all-steps-complete evaluation, error aggregation, final state transitions. |
| `orchestration/error_classifier.rs` | 531 | 261 | 49.2% | Error classification engine: transient/permanent/retryable categorization with pattern matching. |
| `services/template_query_service.rs` | 171 | 85 | 49.7% | Template query service: listing, filtering, caching for task templates. |

---

## Gap Analysis by Priority

### Critical Priority -- Production Correctness Risk

These modules form the core orchestration pipeline. Bugs here cause task corruption, stuck workflows, or data loss.

**1. Result Processing Pipeline (589 uncovered lines, 8-21% coverage)**

| Module | Coverage | Uncovered Lines | Risk |
|--------|----------|-----------------|------|
| `message_handler.rs` | 8.1% | 249 | Routes all step results; unverified routing logic can misroute completions |
| `state_transition_handler.rs` | 14.2% | 139 | Handles EnqueuedForOrchestration race condition fix (TAS-41); untested means race could regress |
| `task_coordinator.rs` | 21.3% | 85 | Finalization coordination; incorrect idempotency logic risks duplicate finalization |
| `metadata_processor.rs` | 13.8% | 69 | Orchestration metadata extraction; silent failures cause missing execution data |
| `step_result_processor.rs` | 31.9% | 47 | Delegation layer to result processing; thin but critical path |

**Test approach**: Integration tests with database fixtures. Create tasks with multiple steps in various states, submit step results via the message handler, and verify correct state transitions and finalization triggering. Mock the decision point and batch processing actors. The state transition handler needs specific tests for EnqueuedForOrchestration steps to verify the TAS-41 race condition fix.

**Estimated coverage impact**: +1.6 percentage points (approximately 589 lines / 37,306 total)

**2. State Management (442 uncovered lines at 17.4% coverage)**

The `state_manager.rs` coordinates SQL function intelligence with state machine transitions. It handles bulk state transitions, event publishing, and error recovery. Untested state management risks corrupted workflow state.

**Test approach**: Integration tests exercising each state transition path. Test bulk operations with mixed success/failure results. Verify event publishing for each transition type. Test error recovery scenarios where state machine and database disagree.

**Estimated coverage impact**: +1.0 percentage points

**3. Viable Step Discovery (342 lines at 7.3% coverage)**

SQL-driven step readiness discovery is the engine for determining which workflow steps can execute. At 7.3% coverage, the dependency resolution, circuit breaker integration, and state machine verification are essentially untested.

**Test approach**: Integration tests with multi-step workflow fixtures at various dependency levels. Test circuit breaker bypass logic, dependency level calculation, and state machine verification after SQL function returns.

**Estimated coverage impact**: +0.9 percentage points

**4. Error Handling Service (217 lines at 0% coverage)**

Bridges error classification with actual step state transitions and backoff scheduling. This module determines whether a failed step retries, permanently fails, or enters waiting state. Zero coverage means the retry/failure decision logic is completely unverified.

**Test approach**: Unit tests with mock state machine and error classifier. Test each ErrorHandlingAction path: permanent failure, retry with backoff, retry limit exceeded, and no-action-needed cases. Verify backoff timing calculations.

**Estimated coverage impact**: +0.6 percentage points

**5. Command Processor Actor (574 lines at 18.6% coverage)**

The unified command processor (TAS-148) is the central routing hub for all orchestration commands. It receives messages via MPSC channels and delegates to task request, result processor, finalizer, and step enqueuer actors. At 18.6% coverage, command routing, statistics tracking, and message acknowledgment are largely untested.

**Test approach**: Integration tests sending each command type through the actor and verifying delegation. Test the `execute_with_stats` helper with success and error cases. Test message acknowledgment flows. Use mock actors for isolated command routing tests.

**Estimated coverage impact**: +1.3 percentage points

### High Priority -- Quality and Reliability Risk

**6. Bootstrap System (444 lines at 9.7% coverage)**

The unified orchestration bootstrap initializes the entire system for all deployment modes (standalone, Docker, test). At 9.7% coverage, most initialization paths, lifecycle management (start/stop), and error handling during startup are untested.

**Test approach**: Integration tests for each deployment mode initialization. Test graceful shutdown sequencing. Test error handling when dependent services are unavailable. The bootstrap is complex because it conditionally initializes web, gRPC, and event systems based on feature flags.

**Estimated coverage impact**: +1.1 percentage points

**7. Orchestration Event System (370 lines at 0% coverage)**

The queue-level event system implementation coordinates PGMQ listener and fallback poller with the EventDrivenSystem interface. It manages deployment mode transitions (polling-only, event-driven, hybrid) and event routing. Zero coverage means deployment mode switching and event coordination are entirely unverified.

**Test approach**: Integration tests requiring messaging infrastructure. Test each deployment mode startup and event routing. Test mode transitions (e.g., hybrid fallback to polling-only). Test statistics collection. This is one of the harder modules to test due to infrastructure dependencies.

**Estimated coverage impact**: +1.0 percentage points

**8. Fallback Poller (208 lines at 0% coverage)**

The orchestration queue fallback poller is the safety net ensuring no messages are missed. Zero coverage means the reliability mechanism itself is unverified.

**Test approach**: Integration tests with messaging infrastructure. Seed queue with messages, verify poller picks them up. Test age threshold filtering, batch size limits, and circuit breaker interaction.

**Estimated coverage impact**: +0.6 percentage points

**9. Staleness Detector (274 lines at 21.2% coverage)**

Background service that detects and transitions stale tasks. Uses SQL functions and integrates with OpenTelemetry metrics. The detection logic, batch worker checkpoint health checks (TAS-59), and dry-run mode need verification.

**Test approach**: Integration tests creating tasks in stale states. Verify detection and state transition via SQL function. Test batch worker checkpoint filtering. Test dry-run mode produces correct output without side effects.

**Estimated coverage impact**: +0.6 percentage points

**10. gRPC Service Layer (741 lines across 13 files, all 0% coverage)**

The entire gRPC layer has zero coverage. This includes 7 service implementations (tasks, steps, templates, analytics, DLQ, config, health), auth interceptors, type conversions, server setup, and shared state.

| gRPC Module | Uncovered Lines |
|-------------|-----------------|
| `services/dlq.rs` | 166 |
| `server.rs` | 121 |
| `services/analytics.rs` | 88 |
| `conversions.rs` | 87 |
| `services/tasks.rs` | 82 |
| `interceptors/auth.rs` | 57 |
| `services/templates.rs` | 45 |
| `services/config.rs` | 39 |
| `services/steps.rs` | 33 |
| `state.rs` | 12 |
| `services/health.rs` | 11 |

**Test approach**: The gRPC services delegate to the same shared `services/` layer that the REST handlers use (which are already at 65-96% coverage). There are two viable strategies:

- *Conversion tests*: Unit tests for `conversions.rs` (proto-to-domain and domain-to-proto) and auth interceptor logic. These are pure functions and can be tested without infrastructure.
- *Integration tests*: Spin up a tonic test server, send gRPC requests, verify responses match expectations. Existing REST/gRPC parity tests (`cargo make test-grpc-parity`) could be extended.

**Estimated coverage impact**: +2.0 percentage points

**11. Decision Point Service (255 lines at 16.1% coverage)**

Dynamic workflow step creation from decision outcomes. Creates new steps, edges, and manages cycle detection. Low coverage means the DAG modification logic is largely unverified.

**Test approach**: Integration tests with template fixtures containing decision points. Test each outcome type: step creation, edge creation, cycle detection rejection. Verify transactional safety (rollback on failure).

**Estimated coverage impact**: +0.6 percentage points

**12. Batch Processing Service (203 lines at 8.9% coverage)**

Dynamic batch worker instance creation (TAS-59). Analyzes datasets, generates cursor configurations, creates worker instances. Similar architecture to decision point but for batch parallelism.

**Test approach**: Integration tests with batch-configured templates. Test NoBatches and CreateBatches paths. Verify convergence step creation when dependencies intersect. Test cursor configuration generation.

**Estimated coverage impact**: +0.5 percentage points

### Medium Priority -- Infrastructure and Observability

**13. Orchestration Queues Listener (330 lines at 33.9% coverage)**

PGMQ queue listener with LISTEN/NOTIFY integration. The message dispatch and statistics tracking have partial coverage. The inline test module provides some baseline.

**Test approach**: Extend existing inline tests to cover message dispatch for each queue type. Test statistics tracking (messages received, errors, latency). Test reconnection logic on listener failures.

**Estimated coverage impact**: +0.6 percentage points

**14. Unified Event Coordinator (300 lines at 35.3% coverage)**

Manages multiple event systems across deployment modes. Partially covered but the mode coordination and startup sequencing need more tests.

**Test approach**: Integration tests for coordinator lifecycle: startup, shutdown, mode transitions. Test error handling when individual event systems fail.

**Estimated coverage impact**: +0.5 percentage points

**15. Task Finalization Pipeline (completion_handler + state_handlers + event_publisher)**

| Module | Coverage | Uncovered Lines |
|--------|----------|-----------------|
| `completion_handler.rs` | 44.4% | 184 |
| `state_handlers.rs` | 25.9% | 160 |
| `event_publisher.rs` | 37.8% | 89 |

These modules handle the final phase of task processing. The completion handler evaluates whether all steps are done; state handlers manage per-state finalization logic; event publisher emits completion events.

**Test approach**: Integration tests exercising each finalization path: all-steps-complete (success), partial failure (some steps errored), in-progress (more steps to enqueue). Test event publishing for each outcome type.

**Estimated coverage impact**: +1.2 percentage points

**16. Backoff Calculator (213 lines at 33.8% coverage)**

Exponential backoff with jitter. Has inline tests but many edge cases untested. The `BackoffContext` integration and configuration-driven behavior need more coverage.

**Test approach**: Unit tests for edge cases: max retry exceeded, zero delay, very large retry counts, configuration boundary values. The pure-function nature makes this straightforward.

**Estimated coverage impact**: +0.4 percentage points

**17. Error Classifier (531 lines at 49.2% coverage)**

Error classification is half-covered. The uncovered portions likely include less common error patterns and edge cases in the pattern matching logic.

**Test approach**: Add unit tests for underrepresented error patterns. Test classification of rare error types. The existing inline test infrastructure should make this easy to extend.

**Estimated coverage impact**: +0.7 percentage points

**18. Hydration Layer (step_result + finalization + task_request hydrators)**

| Module | Coverage | Uncovered Lines |
|--------|----------|-----------------|
| `step_result_hydrator.rs` | 2.0% | 146 |
| `finalization_hydrator.rs` | 11.1% | 56 |
| `task_request_hydrator.rs` | 14.0% | 43 |

These modules convert lightweight queue messages into rich domain objects via database lookups. All three are very low coverage.

**Test approach**: Integration tests with seeded database records. Create workflow steps with stored results, hydrate from mock PGMQ messages, verify correct output. Test error paths: missing step, invalid JSON, null results column.

**Estimated coverage impact**: +0.7 percentage points

### Lower Priority -- Completeness Items

**19. Web Infrastructure (extractors, middleware, state)**

| Module | Coverage | Uncovered Lines |
|--------|----------|-----------------|
| `web/extractors.rs` | 0% | 47 |
| `web/middleware/operational_state.rs` | 0% | 28 |
| `web/middleware/mod.rs` | 0% | 22 |
| `web/state.rs` | 33.8% | 96 |

The web extractors and operational state middleware are at 0% but are exercised implicitly by web handler tests (which are at 65-96%). Adding direct tests would improve coverage metrics but provides lower marginal value since the paths are already exercised.

**Test approach**: Unit tests for extractors (pool selection logic). Test operational state middleware responses during maintenance/degraded mode. These are relatively small modules.

**Estimated coverage impact**: +0.5 percentage points

**20. Actor Wrappers (batch_processing, result_processor, task_finalizer, step_enqueuer, task_request)**

The actor wrapper modules are at 43-52% coverage. They are thin wrappers that implement the `Handler` trait and delegate to lifecycle services. The uncovered portions are primarily the `handle` method implementations which require the full actor system to be running.

**Test approach**: These are best tested through integration tests that exercise the full actor pipeline. Individual actor tests would require significant mocking infrastructure for limited value.

**Estimated coverage impact**: +0.3 percentage points

**21. Binary Entry Points (server.rs, generate_openapi.rs)**

These are CLI entry points that parse arguments and start the server. Testing them directly requires starting the full server, which is better covered by E2E tests.

**Estimated coverage impact**: +0.2 percentage points

---

## Recommended Test Plan

### Phase 1: Critical Path (Target: +7.0 pp, reaching approximately 38.6%)

Focus on production correctness. All modules here handle task/step state and affect data integrity.

| Action | Files | Test Type | Est. Lines | Est. pp |
|--------|-------|-----------|-----------|---------|
| Result processing pipeline tests | message_handler, state_transition_handler, task_coordinator, metadata_processor | Integration (DB fixtures) | 542 | +1.5 |
| State manager integration tests | state_manager.rs | Integration (DB) | 365 | +1.0 |
| Viable step discovery tests | viable_step_discovery.rs | Integration (DB) | 317 | +0.9 |
| Error handling service unit tests | error_handling_service.rs | Unit (mock state machine) | 217 | +0.6 |
| Command processor actor tests | command_processor_actor.rs | Integration (channels) | 467 | +1.3 |
| Error type conversion tests | errors.rs | Unit | 54 | +0.1 |
| Core orchestration tests | core.rs | Integration | 167 | +0.4 |
| Decision point service tests | decision_point/service.rs | Integration (DB) | 214 | +0.6 |
| Batch processing service tests | batch_processing/service.rs | Integration (DB) | 185 | +0.5 |

### Phase 2: Reliability and API (Target: +6.5 pp, reaching approximately 45.1%)

Focus on infrastructure reliability and API coverage.

| Action | Files | Test Type | Est. Lines | Est. pp |
|--------|-------|-----------|-----------|---------|
| Bootstrap lifecycle tests | bootstrap.rs | Integration | 401 | +1.1 |
| gRPC conversion unit tests | grpc/conversions.rs | Unit | 87 | +0.2 |
| gRPC auth interceptor tests | grpc/interceptors/auth.rs | Unit | 57 | +0.2 |
| gRPC service integration tests | grpc/services/*.rs | Integration (tonic) | 464 | +1.2 |
| gRPC server/state tests | grpc/server.rs, grpc/state.rs | Integration | 133 | +0.4 |
| Staleness detector tests | staleness_detector.rs | Integration (DB) | 216 | +0.6 |
| Task finalization pipeline tests | completion_handler, state_handlers, event_publisher | Integration (DB) | 433 | +1.2 |
| Event system + fallback poller | orchestration_event_system, fallback_poller | Integration (messaging) | 578 | +1.5 |

### Phase 3: Depth and Completeness (Target: +4.5 pp, reaching approximately 49.6%)

Extend existing coverage and close remaining gaps.

| Action | Files | Test Type | Est. Lines | Est. pp |
|--------|-------|-----------|-----------|---------|
| Hydration layer tests | step_result_hydrator, finalization_hydrator, task_request_hydrator | Integration (DB) | 245 | +0.7 |
| Backoff calculator edge cases | backoff_calculator.rs | Unit | 141 | +0.4 |
| Error classifier extensions | error_classifier.rs | Unit | 270 | +0.7 |
| Queue listener extensions | orchestration_queues/listener.rs | Integration | 218 | +0.6 |
| Unified event coordinator | unified_event_coordinator.rs | Integration | 194 | +0.5 |
| Task request processor | task_request_processor.rs | Integration | 76 | +0.2 |
| Step enqueuer services | batch_processor, state_handlers | Integration | 164 | +0.4 |
| Fallback poller (task readiness) | task_readiness/fallback_poller.rs | Integration | 76 | +0.2 |
| Web infrastructure | extractors, operational_state, state | Unit/Integration | 193 | +0.5 |
| Health modules | db_status, status_evaluator | Unit/Integration | 123 | +0.3 |

### Phase 4: Final Push to 55% (Target: +5.4 pp)

Extend coverage on already-partially-covered modules, targeting the 55-85% range files to bring them to 90%+ and picking up remaining gaps.

| Action | Est. pp |
|--------|---------|
| Deepen web handler tests (dlq, tasks) | +1.0 |
| Extend step enqueuer tests | +0.8 |
| Actor wrapper handle() method tests | +0.5 |
| Template query service extensions | +0.4 |
| System events module extensions | +0.5 |
| Task initialization module extensions | +0.3 |
| Health service edge cases | +0.4 |
| Miscellaneous small modules | +0.5 |
| Binary entry point smoke tests | +0.2 |
| Event system statistics coverage | +0.4 |
| Remaining health/channel modules | +0.4 |

---

## Estimated Impact

| Phase | Focus | Estimated Lines Added | Coverage Delta | Cumulative Coverage |
|-------|-------|-----------------------|----------------|---------------------|
| Current | -- | -- | -- | 31.6% |
| Phase 1 | Critical path correctness | ~2,528 | +7.0 pp | ~38.6% |
| Phase 2 | Reliability and API layer | ~2,369 | +6.5 pp | ~45.1% |
| Phase 3 | Depth and completeness | ~1,700 | +4.5 pp | ~49.6% |
| Phase 4 | Final push to target | ~2,131 | +5.4 pp | ~55.0% |

**Key constraints**:
- Many critical modules (state_manager, viable_step_discovery, hydration, result processing) require a live database connection for meaningful integration tests, since they use `sqlx::query!` macros and SQL function calls.
- The orchestration event system and fallback poller require messaging infrastructure (PGMQ or RabbitMQ) for integration tests.
- gRPC tests can be split: pure conversion tests (no infrastructure) and service tests (tonic test server).
- The bootstrap module is particularly challenging to test in isolation because it wires together the entire system; partial mocking or feature-flag-based test configurations may be needed.
- Many modules already have `#[cfg(test)]` blocks with inline tests. Extending these is often more efficient than adding new test files, since the test infrastructure (imports, helper functions) is already in place.

**Files with existing inline tests (69 files)**: The presence of `#[cfg(test)]` in 69 of 133 source files indicates a strong inline testing culture. Gaps are concentrated in the orchestration core infrastructure, result processing pipeline, gRPC layer, and the newer TAS-41/TAS-53/TAS-59 features.

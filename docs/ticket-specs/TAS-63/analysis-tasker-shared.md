# Coverage Analysis: tasker-shared

**Current Coverage**: 55.94% line (17,527/31,333), 49.86% function (2,196/4,404)
**Target**: 70%
**Gap**: 14.06 percentage points (need ~4,406 additional covered lines)
**Files Analyzed**: 143 total, 113 with some coverage, 30 with 0% coverage

---

## Summary

The `tasker-shared` crate is the foundational shared library for the tasker-core workspace, providing configuration types, database models, state machine definitions, messaging abstractions, event systems, metrics/observability, registry/resolver patterns, caching, and security/authentication types. At 55.94% line coverage, it sits 14 percentage points below the 70% target. The largest gaps are concentrated in three areas: (1) the entire metrics/observability subsystem (833 lines at 0%), (2) orchestration model types used by SQL views/functions (838 lines at 0%), and (3) the security/auth stack (types, services) with 461 lines uncovered. Many 0%-covered files have corresponding integration test files that require database connectivity (`#[sqlx::test]`), meaning coverage only registers when running with `--features test-services` against a live database. Closing the gap requires a mix of pure unit tests for data types and error conversions, plus integration tests for database-dependent modules.

## Uncovered Files (0% Coverage)

30 files totaling 2,706 lines and 426 functions have zero test coverage.

### Configuration (6 files, 212 lines)

| File | Lines | Functions | Contents |
|------|-------|-----------|----------|
| `config/orchestration/event_systems.rs` | 35 | 8 | `OrchestrationEventSystemConfig` with `from_config_manager()`, `from_tasker_config()`, and Duration convenience methods |
| `config/orchestration/mod.rs` | 39 | 5 | `OrchestrationConfig` and `OrchestrationSystemConfig` structs with defaults and `from_config_manager()` |
| `config/orchestration/step_enqueuer.rs` | 26 | 3 | `StepEnqueuerConfig` with Default and `From<&TaskerConfig>` / `From<Arc<TaskerConfig>>` |
| `config/orchestration/step_result_processor.rs` | 35 | 3 | `StepResultProcessorConfig` with Default and `From<&TaskerConfig>` / `From<Arc<TaskerConfig>>` |
| `config/orchestration/task_claim_step_enqueuer.rs` | 33 | 3 | `TaskClaimStepEnqueuerConfig` composing step enqueuer + result processor configs |
| `config/web.rs` | 44 | 2 | `WebAuthConfig` with Default impl and `From<AuthConfig>` conversion |

### Metrics/Observability (6 files, 833 lines)

| File | Lines | Functions | Contents |
|------|-------|-----------|----------|
| `metrics/mod.rs` | 90 | 16 | `MetricsConfig`, `init_metrics()` (Prometheus exporter), `prometheus_exporter()`, `shutdown_metrics()` |
| `metrics/channels.rs` | 214 | 31 | Channel MPSC metrics: counters, gauges, `ChannelMetricsRecorder`, `ChannelMetricsRegistry` |
| `metrics/orchestration.rs` | 240 | 35 | Task lifecycle metrics, stale task discovery, decision points, DLQ, API backpressure |
| `metrics/health.rs` | 142 | 16 | Health-related metrics: system health gauges, component health checks |
| `metrics/messaging.rs` | 89 | 14 | Message queue latency metrics, queue depth gauges |
| `metrics/database.rs` | 58 | 10 | SQL query duration histograms, connection pool metrics |

### Database Models (8 files, 838 lines)

| File | Lines | Functions | Contents |
|------|-------|-----------|----------|
| `models/core/named_tasks_named_step.rs` | 153 | 22 | Junction table model with CRUD operations (requires DB) |
| `models/insights/analytics_metrics.rs` | 32 | 10 | SQL function return type for `get_analytics_metrics()` |
| `models/insights/slowest_steps.rs` | 86 | 17 | SQL function return type for `get_slowest_steps()` |
| `models/insights/slowest_tasks.rs` | 112 | 21 | SQL function return type for `get_slowest_tasks()` |
| `models/insights/system_health_counts.rs` | 131 | 18 | SQL function return type for `get_system_health_counts()` |
| `models/orchestration/step_dag_relationship.rs` | 115 | 24 | Computed view model for DAG analysis |
| `models/orchestration/step_readiness_status.rs` | 112 | 26 | Computed view model for step readiness |
| `models/orchestration/step_transitive_dependencies.rs` | 97 | 22 | Computed view model for transitive dependency resolution |

### Security & Auth (1 file, 186 lines)

| File | Lines | Functions | Contents |
|------|-------|-----------|----------|
| `services/security_service.rs` | 186 | 18 | Unified `SecurityService` combining JWT (static + JWKS) and API key auth |

### State Machine (2 files, 172 lines)

| File | Lines | Functions | Contents |
|------|-------|-----------|----------|
| `state_machine/errors.rs` | 38 | 10 | `StateMachineError`, `GuardError`, `ActionError`, `PersistenceError` + conversions |
| `state_machine/events.rs` | 134 | 27 | `TaskEvent` and `StepEvent` enums with helper methods |

### Types (4 files, 271 lines)

| File | Lines | Functions | Contents |
|------|-------|-----------|----------|
| `types/auth.rs` | 175 | 19 | `JwtAuthenticator`, `TokenClaims`, RSA key parsing, token generation/validation |
| `types/api/health.rs` | 30 | 1 | `PoolUtilizationInfo`, `PoolDetail` structs and `build_pool_utilization()` |
| `types/api/worker.rs` | 59 | 8 | Worker API response types: health checks, templates, circuit breakers |
| `types/openapi_security.rs` | 7 | 1 | `SecurityAddon` for OpenAPI schema modification |

### Other (2 files, 109 lines)

| File | Lines | Functions | Contents |
|------|-------|-----------|----------|
| `errors.rs` | 97 | 17 | `TaskerError`, `OrchestrationError`, `StepExecutionError`, `RegistryError`, `EventError`, `StateError`, `DiscoveryError`, `ExecutionError` + From conversions |
| `sql_functions.rs` | 12 | 12 | Documentation-only module with empty function bodies documenting SQL functions |
| `database/migrator.rs` | 85 | 7 | Database migration runner with split-database support (requires live DB) |

## Lowest Coverage Files (1-30%)

18 files with partial but critically low coverage, totaling 5,823 lines with 4,886 uncovered.

| File | Coverage | Lines | Uncovered | Contents |
|------|----------|-------|-----------|----------|
| `metrics/worker.rs` | 5.8% | 172 | 162 | Worker step execution metrics |
| `models/core/task_transition.rs` | 6.3% | 459 | 430 | Task transition model with DB queries |
| `models/core/workflow_step.rs` | 8.5% | 750 | 686 | Core workflow step model with DB CRUD |
| `models/core/workflow_step_transition.rs` | 9.0% | 544 | 495 | Step transition model with DB queries |
| `models/core/workflow_step_edge.rs` | 9.3% | 107 | 97 | Step dependency edge model |
| `models/core/named_step.rs` | 9.7% | 145 | 131 | Named step definition model |
| `database/sql_functions.rs` | 10.6% | 517 | 462 | SQL function wrappers (requires DB) |
| `models/core/task_namespace.rs` | 12.4% | 161 | 141 | Namespace model with DB queries |
| `models/core/named_task.rs` | 12.7% | 189 | 165 | Named task definition model |
| `types/base.rs` | 17.8% | 303 | 249 | Base types: `HandlerMetadata`, `CacheStats`, API response types |
| `state_machine/guards.rs` | 18.1% | 94 | 77 | State machine guard conditions (requires DB) |
| `logging.rs` | 18.3% | 312 | 255 | Tracing/logging initialization with telemetry |
| `messaging/service/providers/rabbitmq.rs` | 22.8% | 470 | 363 | RabbitMQ messaging provider |
| `state_machine/actions.rs` | 23.9% | 460 | 350 | State machine action handlers (requires DB) |
| `state_machine/persistence.rs` | 26.2% | 103 | 76 | State persistence layer (requires DB) |
| `messaging/errors.rs` | 27.6% | 254 | 184 | Messaging error types and conversions |
| `proto/conversions.rs` | 27.9% | 619 | 446 | gRPC protobuf conversion functions |
| `database/pools.rs` | 28.7% | 164 | 117 | Database connection pool management |

---

## Gap Analysis by Priority

### Critical Priority

These files contain core business logic where lack of coverage represents production risk.

**1. Error Types (`errors.rs`) -- 0% coverage, 97 lines**

The central error hierarchy defines `TaskerError`, `OrchestrationError`, and 6 other error enums with 12+ `From` conversion implementations. These conversions are used throughout the entire codebase for error propagation. A bug in any conversion could cause incorrect error handling, lost error context, or panics.

- **Risk**: Error conversion bugs silently lose diagnostic information or cause unexpected error types
- **Test type**: Unit tests -- construct each error variant, verify Display output, test all From conversions
- **Estimated effort**: Low (pure data types, no dependencies)

**2. State Machine Events (`state_machine/events.rs`) -- 0% coverage, 134 lines**

`TaskEvent` (22 variants) and `StepEvent` (12 variants) drive the entire state machine. Helper methods like `is_terminal()`, `requires_ownership()`, `error_message()`, `results()` directly affect orchestration behavior. Note: integration tests exist in `tests/state_machine/events.rs` but are not registering in coverage.

- **Risk**: Incorrect event classification could cause tasks to stall or terminate prematurely
- **Test type**: Unit tests -- test all event variants' helper methods, serialization round-trips
- **Estimated effort**: Low (pure enum logic)

**3. State Machine Errors (`state_machine/errors.rs`) -- 0% coverage, 38 lines**

Error types and conversions for the state machine subsystem. Integration tests exist in `tests/state_machine/errors.rs` but are not registering in coverage.

- **Risk**: State machine error propagation failures
- **Test type**: Unit tests -- verify error conversions, helper functions
- **Estimated effort**: Very low

**4. Security Service (`services/security_service.rs`) -- 0% coverage, 186 lines**

The unified `SecurityService` handles JWT (static key + JWKS) and API key authentication. Contains critical security logic: algorithm allowlist enforcement, key resolution from files, JWKS validation, permission checking with strict mode.

- **Risk**: Authentication bypass, algorithm confusion attacks, permission escalation
- **Test type**: Integration tests with mock keys and JWKS endpoints; unit tests for algorithm parsing and permission validation
- **Estimated effort**: Medium (needs RSA key fixtures, async test setup)

**5. Auth Types (`types/auth.rs`) -- 0% coverage, 175 lines**

`JwtAuthenticator` with RSA key parsing (PKCS#1 and PKCS#8), token generation/validation, bearer token extraction, and namespace/permission checks.

- **Risk**: Key parsing failures in production, token validation bypasses
- **Test type**: Unit tests with test RSA key pairs, integration tests for full token round-trips
- **Estimated effort**: Medium (needs RSA key fixtures)

### High Priority

Files where lack of coverage creates quality risk for core functionality.

**6. Database Models -- multiple files at 0-13% coverage (~2,700+ lines)**

Core models (`task_transition.rs`, `workflow_step.rs`, `workflow_step_transition.rs`, `named_step.rs`, `named_task.rs`, `task_namespace.rs`, `workflow_step_edge.rs`, `named_tasks_named_step.rs`) contain CRUD operations and business logic. These models are the data backbone of the system.

- **Risk**: Data integrity issues, query bugs, incorrect default values
- **Test type**: Integration tests with `#[sqlx::test]` (most already exist but may need test-services feature); unit tests for Default impls, serialization, and non-DB methods
- **Estimated effort**: Medium-high (DB-dependent tests need running services)
- **Note**: Tests exist in `tests/models/core/` but require `--features test-services` to execute

**7. Database SQL Functions (`database/sql_functions.rs`) -- 10.6% coverage, 517 lines**

Wrappers around PostgreSQL functions for task execution contexts, step readiness, analytics. Critical for orchestration decisions.

- **Risk**: SQL function call errors, incorrect result parsing
- **Test type**: Integration tests with `#[sqlx::test]` (some exist in `tests/sql_functions/`)
- **Estimated effort**: Medium (DB-dependent)

**8. Proto Conversions (`proto/conversions.rs`) -- 27.9% coverage, 619 lines**

gRPC protobuf conversion functions for the TAS-177 transport layer. 446 uncovered lines of conversion logic.

- **Risk**: Data loss or corruption during REST/gRPC transport translation
- **Test type**: Unit tests for bidirectional conversion round-trips (no infrastructure needed)
- **Estimated effort**: Medium (many conversion functions to test)

**9. Config Orchestration (6 files at 0%, 212 lines)**

Configuration types with `Default` impls and `From<TaskerConfig>` conversions. Used to configure all orchestration actors.

- **Risk**: Incorrect default values or config mapping could misconfigure production actors
- **Test type**: Unit tests -- verify Default values, test From conversions with known TaskerConfig
- **Estimated effort**: Low (pure data types)

### Medium Priority

Technical debt items that should be addressed but pose less immediate risk.

**10. Metrics Subsystem (6 files at 0%, 833 lines)**

The entire OpenTelemetry metrics module has zero coverage. Contains `init_metrics()`, metric definitions (counters, gauges, histograms), `ChannelMetricsRecorder`, and `ChannelMetricsRegistry`.

- **Risk**: Metrics initialization failures in production, incorrect metric labeling
- **Test type**: Unit tests for metric initialization, recorder snapshot exports, registry operations. Requires OpenTelemetry test provider setup.
- **Estimated effort**: Medium (OpenTelemetry test harness needed)
- **Note**: Metrics are largely declarative (OnceLock + builder patterns), so functional risk is lower than line count suggests

**11. Messaging Providers (RabbitMQ at 22.8%, 470 lines)**

RabbitMQ provider implementation with 363 uncovered lines. The PGMQ provider is better covered at 46.5%.

- **Risk**: RabbitMQ integration failures in production
- **Test type**: Integration tests requiring RabbitMQ service (`docker-compose.test.yml`)
- **Estimated effort**: Medium-high (requires RabbitMQ infrastructure)

**12. Logging (`logging.rs`) -- 18.3% coverage, 312 lines**

Tracing subscriber initialization with OpenTelemetry, environment detection, and format configuration.

- **Risk**: Observability gaps if initialization fails silently
- **Test type**: Unit tests for configuration parsing; integration tests with captured output
- **Estimated effort**: Medium

**13. Orchestration View Models (3 files at 0%, 324 lines)**

`StepDagRelationship`, `StepReadinessStatus`, `StepTransitiveDependencies` -- computed view models that map SQL view results. These are read-only data structures.

- **Risk**: Deserialization failures from SQL view results
- **Test type**: Unit tests for struct construction and serialization; integration tests via SQL views
- **Estimated effort**: Low for unit tests, medium for integration tests

**14. Insights Models (4 files at 0%, 361 lines)**

`AnalyticsMetrics`, `SlowestSteps`, `SlowestTasks`, `SystemHealthCounts` -- SQL function return types. Tests exist in `tests/models/insights/` but require database.

- **Risk**: Analytics query result parsing failures
- **Test type**: Unit tests for struct defaults and serialization; existing integration tests need feature flags
- **Estimated effort**: Low

### Lower Priority

Nice-to-have improvements with lower risk profiles.

**15. SQL Functions Documentation (`sql_functions.rs`) -- 0% coverage, 12 lines**

Documentation-only module with empty function bodies. The 12 "lines" are empty `fn` declarations used purely for documentation purposes.

- **Risk**: None (no executable logic)
- **Test type**: Not needed -- exclude from coverage or add trivial tests
- **Estimated effort**: Negligible

**16. OpenAPI Security (`types/openapi_security.rs`) -- 0% coverage, 7 lines**

`SecurityAddon` struct implementing `utoipa::Modify` for OpenAPI spec generation.

- **Risk**: Very low (compile-time schema generation)
- **Test type**: Unit test verifying SecurityAddon modifies OpenAPI spec correctly
- **Estimated effort**: Very low

**17. Types/API Health and Worker (2 files at 0%, 89 lines)**

Response types for health check and worker API endpoints. Mostly data structs with `Serialize`/`Deserialize`.

- **Risk**: Low (data structures validated by serde derives)
- **Test type**: Unit tests for helper methods (`all_healthy()`, `is_healthy()`, `build_pool_utilization()`)
- **Estimated effort**: Low

**18. Database Migrator (`database/migrator.rs`) -- 0% coverage, 85 lines**

Split-database migration runner. Requires live database to test meaningfully.

- **Risk**: Migration routing in split-database production deployments
- **Test type**: Integration tests with `#[sqlx::test]`
- **Estimated effort**: Medium (requires DB, split-database setup is complex)

**19. Config Web (`config/web.rs`) -- 0% coverage, 44 lines**

`WebAuthConfig` struct with Default impl and `From<AuthConfig>` conversion. Type aliases for backward compatibility.

- **Risk**: Low (pure data conversion)
- **Test type**: Unit tests for Default values and From conversion
- **Estimated effort**: Very low

---

## Recommended Test Plan

### Phase 1: Quick Wins -- Pure Unit Tests (estimated +5-7% coverage)

These require no infrastructure and can be written immediately.

| Module | Test File | Tests to Add | Est. Lines Covered |
|--------|-----------|--------------|-------------------|
| `errors.rs` | inline `#[cfg(test)]` | All error variants Display, all 12 From conversions, StepExecutionError::error_class() | ~80 lines |
| `state_machine/events.rs` | inline `#[cfg(test)]` or ensure existing `tests/state_machine/events.rs` runs | All TaskEvent + StepEvent helper methods, serde round-trips | ~120 lines |
| `state_machine/errors.rs` | inline `#[cfg(test)]` | Error conversions, helper functions `internal_error()`, `dependencies_not_met()`, `business_rule_violation()` | ~35 lines |
| `config/orchestration/*.rs` (5 files) | inline `#[cfg(test)]` | Default values, From conversions, Duration helper methods | ~180 lines |
| `config/web.rs` | inline `#[cfg(test)]` | WebAuthConfig Default, From<AuthConfig> conversion | ~40 lines |
| `proto/conversions.rs` | inline `#[cfg(test)]` | Round-trip conversion tests for all protobuf types | ~300 lines |
| `types/base.rs` | inline `#[cfg(test)]` | Helper methods on HandlerMetadata, CacheStats, response types | ~100 lines |
| `types/api/worker.rs` | inline `#[cfg(test)]` | `WorkerReadinessChecks::all_healthy()`, `HealthCheck::healthy()/degraded()/unhealthy()` | ~40 lines |
| `messaging/errors.rs` | inline `#[cfg(test)]` | Error variant construction, Display output, From conversions | ~80 lines |
| `models/orchestration/*.rs` (3 files) | inline `#[cfg(test)]` | Struct construction, serialization, Default impls | ~150 lines |
| `models/insights/*.rs` (4 files) | inline `#[cfg(test)]` | Struct construction, serialization | ~100 lines |

**Subtotal**: ~1,225 lines newly covered

### Phase 2: Security Tests (estimated +2-3% coverage)

| Module | Test File | Tests to Add | Est. Lines Covered |
|--------|-----------|--------------|-------------------|
| `types/auth.rs` | inline `#[cfg(test)]` | RSA key parsing (PKCS#1 + PKCS#8), token generate/validate round-trip, bearer extraction, permission/namespace checks, disabled-auth path | ~140 lines |
| `services/security_service.rs` | inline `#[cfg(test)]` | `from_config()` disabled path, API key auth, algorithm parsing, permission validation with strict/lenient modes | ~120 lines |
| `types/jwks.rs` | extend existing tests | JWKS key store initialization, key retrieval, stale key handling | ~40 lines |

**Subtotal**: ~300 lines newly covered (with test RSA key fixtures)

### Phase 3: Integration Tests with Database (estimated +4-6% coverage)

These require `--features test-services` with a running PostgreSQL instance.

| Module | Test File | Tests to Add | Est. Lines Covered |
|--------|-----------|--------------|-------------------|
| `models/core/task_transition.rs` | `tests/models/core/task_transition.rs` (extend) | CRUD operations, state query methods | ~200 lines |
| `models/core/workflow_step.rs` | `tests/models/core/workflow_step.rs` (extend) | CRUD, status methods, batch operations | ~300 lines |
| `models/core/workflow_step_transition.rs` | `tests/models/core/workflow_step_transition.rs` (extend) | Transition recording, query methods | ~200 lines |
| `models/core/named_tasks_named_step.rs` | `tests/models/core/named_tasks_named_step.rs` (exists) | Ensure tests run with proper features | ~100 lines |
| `database/sql_functions.rs` | `tests/sql_functions/` (extend) | Additional SQL function wrapper tests | ~200 lines |
| `state_machine/actions.rs` | `tests/state_machine/actions.rs` (exists) | Additional action execution paths | ~150 lines |
| `state_machine/guards.rs` | `tests/state_machine/guards.rs` (exists) | Additional guard condition paths | ~40 lines |
| `state_machine/persistence.rs` | `tests/state_machine/persistence.rs` (exists) | Additional persistence operations | ~40 lines |

**Subtotal**: ~1,230 lines newly covered

### Phase 4: Infrastructure Tests (estimated +2-3% coverage)

| Module | Test File | Tests to Add | Est. Lines Covered |
|--------|-----------|--------------|-------------------|
| `metrics/*.rs` | new `metrics` test module | `init_metrics()`, metric recording, `ChannelMetricsRecorder` snapshot export, registry operations | ~300 lines |
| `logging.rs` | inline `#[cfg(test)]` | Config detection, subscriber initialization, format selection | ~100 lines |
| `messaging/service/providers/rabbitmq.rs` | integration test (requires RabbitMQ) | Provider lifecycle, publish/consume, error handling | ~150 lines |
| `database/pools.rs` | integration test | Pool creation, utilization stats, split-database detection | ~60 lines |
| `database/migrator.rs` | integration test | Migration run, split-database routing, idempotency | ~50 lines |

**Subtotal**: ~660 lines newly covered

---

## Estimated Impact

| Phase | Effort | New Lines Covered | Cumulative Coverage |
|-------|--------|-------------------|-------------------|
| Current baseline | -- | 17,527 / 31,333 | **55.94%** |
| Phase 1: Unit tests | 2-3 days | +1,225 | **59.85%** |
| Phase 2: Security tests | 1-2 days | +300 | **60.81%** |
| Phase 3: DB integration | 3-4 days | +1,230 | **64.73%** |
| Phase 4: Infrastructure | 2-3 days | +660 | **66.84%** |
| **All phases combined** | **8-12 days** | **+3,415** | **~66.8%** |

To reach the full 70% target (21,933 lines), approximately 991 additional lines beyond Phase 4 would need coverage. This could come from:
- Deeper coverage of partially-tested modules (e.g., pushing `messaging/service/provider.rs` from 44.5% to 80%)
- Additional edge-case tests for `config/tasker.rs` (currently 88.3%, 66 uncovered lines)
- Expanding `models/core/task.rs` tests (currently 30.4%, 682 uncovered lines)
- More thorough `models/core/batch_worker.rs` tests (currently 48.6%, 179 uncovered lines)

**Recommended priority**: Phases 1 and 2 deliver the highest ROI, covering critical error handling, state machine events, security, and configuration correctness with minimal infrastructure requirements. Phase 3 is essential for reaching 65%+ but requires database test infrastructure. Phase 4 addresses technical debt in observability and messaging.

### Key Observation: Integration Test Coverage Attribution

Several modules showing 0% coverage (state_machine/events, state_machine/errors, models/core/named_tasks_named_step, models/insights/*) have corresponding test files in `tests/`. These tests likely execute under `--features test-services` or `--features test-messaging` and may already contribute to coverage when run with the full test suite. The coverage report should be regenerated with `--features test-services` against a live database to capture the true baseline before writing new tests. This alone could improve reported coverage by 2-5%.

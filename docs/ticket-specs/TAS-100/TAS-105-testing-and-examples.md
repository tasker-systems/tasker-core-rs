# TAS-105: TypeScript Testing and Examples

**Parent**: [TAS-100](./README.md)
**Linear**: [TAS-105](https://linear.app/tasker-systems/issue/TAS-105)
**Branch**: `jcoletaylor/tas-105-testing-and-examples`
**Priority**: Medium
**Status**: Implementation Ready
**Depends On**: TAS-101, TAS-102, TAS-103, TAS-104

---

## Objective

Comprehensive test suite covering unit tests, integration tests, and E2E tests demonstrating all workflow patterns. The TypeScript worker must achieve feature parity with Python and Ruby workers for core testing scenarios.

**Key Goals**:
- Achieve >80% code coverage for unit tests
- Implement all E2E test types present in both Python AND Ruby workers
- Follow established patterns for FFI integration (no duplicate HTTP server)
- Provide production-ready example handlers

---

## Architecture Correction: HTTP Server

### Problem Identified

Initial implementation added a `Bun.serve()` HTTP server in `bin/server.ts`. This is **incorrect**.

### Correct Pattern (Ruby/Python)

Ruby and Python workers do NOT implement their own HTTP servers. Instead:

1. **Rust worker starts HTTP server** on port 8081 via `bootstrap_worker()` FFI call
2. **FFI workers call FFI functions** that delegate to Rust services
3. **All observability flows through Rust HTTP API**

### Rust Worker HTTP Endpoints (Available to All FFI Workers)

From `tasker-worker/src/web/routes.rs`:

```
Health:
  GET /health           - Basic health check
  GET /health/ready     - Kubernetes readiness probe
  GET /health/live      - Kubernetes liveness probe
  GET /health/detailed  - Comprehensive health report

Metrics:
  GET /metrics          - Prometheus format metrics
  GET /metrics/worker   - Worker metrics JSON
  GET /metrics/events   - Domain event statistics (used by E2E tests)

Templates:
  GET /templates                           - List all templates
  GET /templates/{ns}/{name}/{ver}         - Get specific template
  POST /templates/{ns}/{name}/{ver}/validate - Validate template

Config:
  GET /config           - Runtime configuration
```

### Required Changes

1. **Remove** `Bun.serve()` from `bin/server.ts` (lines 145-251)
2. **Add** FFI health/metrics calls to TypeScript FFI module
3. **Update** health check loop to use FFI calls instead of in-process checks

---

## E2E Test Requirements

### Intersection Analysis: Python + Ruby

Tests present in BOTH Python and Ruby workers must be implemented for TypeScript:

| Test Type | Python File | Ruby File | TypeScript Status |
|-----------|-------------|-----------|-------------------|
| batch_processing | `batch_processing_test.rs` | `batch_processing_csv_test.rs` | **Required** |
| conditional_approval | `conditional_approval_test.rs` | `conditional_approval_test.rs` | **Required** |
| domain_event_publishing | `domain_event_publishing_test.rs` | `domain_event_publishing_test.rs` | **Required** |
| error_scenarios | `error_scenarios_test.rs` | `error_scenarios_test.rs` | **Required** |
| linear_workflow | `linear_workflow_test.rs` | (sequential in ecommerce) | **Required** |
| diamond_workflow | `diamond_workflow_test.rs` | (parallel in data_pipeline) | **Required** |

### Out of Scope (TAS-91)

Blog post examples are covered by TAS-91 (separate backlog item):
- `ecommerce_order_test.rs` - Blog Post 01
- `data_pipeline_test.rs` - Blog Post 02
- `microservices_coordination_test.rs` - Blog Post 03
- `namespace_isolation_test.rs` - Blog Post 04

---

## Implementation Phases

### Phase 1: Architecture Fixes

**1.1 Remove Incorrect HTTP Server**

File: `workers/typescript/bin/server.ts`

Remove:
- `startHttpServer()` function
- `stopHttpServer()` function
- `HttpServerState` interface
- `getHttpPort()`, `isWebEnabled()` functions
- All `Bun.serve()` related code

**1.2 Add FFI Health/Metrics Support**

File: `workers/typescript/src/ffi/observability.ts` (new)

```typescript
/**
 * FFI bindings for health and metrics.
 * Delegates to Rust HealthService and MetricsService via FFI.
 */
export interface HealthCheckResult {
  healthy: boolean;
  status: string;
  error?: string;
  components?: Record<string, ComponentHealth>;
}

export interface ComponentHealth {
  healthy: boolean;
  message?: string;
}

export interface DomainEventStats {
  total_events: number;
  durable_events: number;
  fast_events: number;
  ffi_dispatches: number;
}

// FFI function signatures (implemented in Rust FFI layer)
export declare function ffiHealthCheck(): HealthCheckResult;
export declare function ffiHealthDetailed(): HealthCheckResult;
export declare function ffiMetricsEvents(): DomainEventStats;
```

**1.3 Update Server Health Loop**

File: `workers/typescript/bin/server.ts`

Replace in-process health check with FFI call:

```typescript
// Before (wrong):
const healthStatus = server.healthCheck();

// After (correct):
const healthStatus = await ffiHealthCheck();
```

---

### Phase 2: E2E Test Infrastructure

**2.1 Directory Structure**

```
tests/e2e/typescript/
├── mod.rs                          # Module registration
├── README.md                       # Test documentation
├── batch_processing_test.rs        # CSV batch processing
├── conditional_approval_test.rs    # Decision point workflows
├── diamond_workflow_test.rs        # Parallel with convergence
├── domain_event_publishing_test.rs # Event publishing verification
├── error_scenarios_test.rs         # Success/permanent/retryable
└── linear_workflow_test.rs         # Sequential workflows
```

**2.2 Task Templates**

```
tests/fixtures/task_templates/typescript/
├── batch_processing/
│   └── csv_product_inventory_analyzer_ts.yaml
├── conditional_approval/
│   └── approval_routing_ts.yaml
├── diamond_workflow/
│   └── parallel_computation_ts.yaml
├── domain_events/
│   └── domain_event_publishing_ts.yaml
├── error_scenarios/
│   ├── success_only_ts.yaml
│   ├── permanent_error_only_ts.yaml
│   └── retryable_error_only_ts.yaml
└── linear_workflow/
    └── mathematical_sequence_ts.yaml
```

**2.3 Example Handlers**

```
workers/typescript/tests/handlers/examples/
├── batch_processing/
│   └── step_handlers/
│       ├── csv-analyzer-handler.ts      # Batchable pattern
│       ├── csv-batch-processor-handler.ts
│       └── csv-aggregator-handler.ts    # Deferred convergence
├── conditional_approval/
│   └── step_handlers/
│       ├── validate-request-handler.ts
│       ├── routing-decision-handler.ts  # Decision point
│       ├── auto-approve-handler.ts
│       ├── manager-approval-handler.ts
│       └── finalize-approval-handler.ts
├── diamond_workflow/
│   └── step_handlers/
│       ├── diamond-start-handler.ts
│       ├── diamond-branch-b-handler.ts
│       ├── diamond-branch-c-handler.ts
│       └── diamond-end-handler.ts       # Convergence
├── domain_events/
│   └── step_handlers/
│       ├── validate-order-handler.ts
│       ├── process-payment-handler.ts   # Publishes durable event
│       ├── update-inventory-handler.ts
│       └── send-notification-handler.ts
├── error_scenarios/
│   └── step_handlers/
│       ├── success-handler.ts
│       ├── permanent-error-handler.ts
│       └── retryable-error-handler.ts
└── linear_workflow/
    └── step_handlers/
        └── linear-step-handlers.ts      # Already created
```

---

### Phase 3: E2E Test Implementation

#### 3.1 Batch Processing Test

**File**: `tests/e2e/typescript/batch_processing_test.rs`

**Template**: `csv_processing_ts/csv_product_inventory_analyzer_ts`

**Test Functions**:
- `test_typescript_csv_batch_processing()` - 1000 rows, 5 workers
- `test_typescript_no_batches_scenario()` - Empty CSV handling

**Key Assertions**:
- Exactly 5 batch workers created for 1000 rows (batch size 200)
- `analyze_csv_ts` step completes (batchable pattern)
- `aggregate_csv_results_ts` step collects all worker results (deferred convergence)
- Final aggregation: `total_processed=1000`, `worker_count=5`

**Fixture**: Uses `TASKER_FIXTURE_PATH` + `/products.csv`

#### 3.2 Conditional Approval Test

**File**: `tests/e2e/typescript/conditional_approval_test.rs`

**Template**: `conditional_approval_ts/approval_routing_ts`

**Test Functions**:
- `test_typescript_small_amount_auto_approval()` - $500 → auto-approve (4 steps)
- `test_typescript_medium_amount_manager_approval()` - $2,500 → manager (4 steps)
- `test_typescript_large_amount_dual_approval()` - $10,000 → dual (5 steps)
- `test_typescript_boundary_small_threshold()` - Exactly $1,000
- `test_typescript_boundary_large_threshold()` - Exactly $5,000

**Key Assertions**:
- Correct steps created based on amount thresholds
- Decision point step completes
- Conditional paths NOT created when outside threshold
- All created steps reach COMPLETE state

#### 3.3 Diamond Workflow Test

**File**: `tests/e2e/typescript/diamond_workflow_test.rs`

**Template**: `diamond_workflow_ts/parallel_computation_ts`

**Test Functions**:
- `test_typescript_diamond_workflow_standard()` - Input 6: parallel branches converge
- `test_typescript_diamond_workflow_parallel_branches()` - Verify parallel execution
- `test_typescript_diamond_workflow_different_input()` - Input 4

**Key Assertions**:
- 4 steps: start → branch_b, branch_c (parallel) → end
- All steps reach COMPLETE state
- Convergence step waits for both branches

#### 3.4 Domain Event Publishing Test

**File**: `tests/e2e/typescript/domain_event_publishing_test.rs`

**Template**: `domain_events_ts/domain_event_publishing_ts`

**Test Functions**:
- `test_typescript_domain_event_publishing_success()` - Event stats verification
- `test_typescript_domain_event_publishing_concurrent()` - 3 concurrent tasks
- `test_typescript_domain_event_metrics_availability()` - Worker health + metrics

**Key Assertions**:
- All 4 steps complete in < 10s (events are non-blocking)
- At least 4 events published (via `/metrics/events` endpoint)
- At least 1 durable event (PGMQ)
- At least 3 fast events (in-process bus)
- Worker health check passes

**Worker Client**: Uses `IntegrationTestManager::setup_typescript_worker()`

#### 3.5 Error Scenarios Test

**File**: `tests/e2e/typescript/error_scenarios_test.rs`

**Templates**:
- `test_scenarios_ts/success_only_ts`
- `test_scenarios_ts/permanent_error_only_ts`
- `test_scenarios_ts/retryable_error_only_ts`

**Test Functions**:
- `test_typescript_success_scenario()` - Happy path (< 3s)
- `test_typescript_permanent_failure_scenario()` - No retries (< 5s)
- `test_typescript_retryable_failure_scenario()` - Backoff exhaustion (100ms - 10s)

**Key Assertions**:
- **Success**: Task completes, step COMPLETE
- **Permanent**: Fails quickly (no retry delays), step ERROR
- **Retryable**: Measurable delays (backoff), eventually ERROR

#### 3.6 Linear Workflow Test

**File**: `tests/e2e/typescript/linear_workflow_test.rs` (update existing)

**Template**: `linear_workflow/mathematical_sequence_ts`

**Test Functions** (already created, verify):
- `test_typescript_linear_workflow_standard()` - Input 6
- `test_typescript_linear_workflow_different_input()` - Input 10
- `test_typescript_linear_workflow_ordering()` - Input 8

---

### Phase 4: Unit Test Coverage

**Target**: >80% coverage on:
- `src/handler/` - Handler base classes, registry, API handler
- `src/types/` - Type definitions and result classes
- `src/server/` - Worker server, shutdown controller
- `src/events/` - Event poller, subscriber

**Existing Tests** (verify coverage):
- `tests/unit/handler/` - registry, base, api, decision, batchable
- `tests/unit/types/` - step context, step handler result, error type
- `tests/unit/server/` - worker-server, shutdown-controller
- `tests/unit/events/` - event-poller, step-execution-subscriber

---

## Testing Utilities

### IntegrationTestManager

```rust
// Setup with TypeScript worker
let manager = IntegrationTestManager::setup_typescript_worker().await?;

// Create task
let task_request = create_task_request(
    "namespace_ts",
    "template_ts",
    json!({ "input": "value" }),
);

// Execute and verify
let response = manager.orchestration_client.create_task(task_request).await?;
wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

// Verify domain events (via Rust HTTP API)
let stats = manager.worker_client.get_domain_event_stats().await?;
assert!(stats.total_events >= 4);
```

### Environment Variables

```bash
# Required for E2E tests
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test
TASKER_ENV=test
TASKER_TEMPLATE_PATH=/app/tests/fixtures/task_templates/typescript
TASKER_FIXTURE_PATH=/app/tests/fixtures

# TypeScript worker port (mapped by Docker)
PORT=8081  # Internal, mapped to 8084 external
```

### Docker Compose Service

```yaml
# In docker/docker-compose.test.yml
typescript-worker:
  build:
    context: ..
    dockerfile: docker/build/typescript-worker.test.Dockerfile
  environment:
    DATABASE_URL: postgresql://tasker:tasker@postgres:5432/tasker_rust_test
    TASKER_ENV: test
    TASKER_TEMPLATE_PATH: /app/tests/fixtures/task_templates/typescript
    TASKER_FIXTURE_PATH: /app/tests/fixtures
    PORT: 8081
  ports:
    - "8084:8081"  # TypeScript worker external port
  healthcheck:
    test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
```

---

## Success Criteria

### Required

- [ ] HTTP server code removed from `bin/server.ts`
- [ ] FFI health/metrics functions exposed to TypeScript
- [ ] All 6 E2E test files created and passing:
  - [ ] batch_processing_test.rs (2 tests)
  - [ ] conditional_approval_test.rs (5 tests)
  - [ ] diamond_workflow_test.rs (3 tests)
  - [ ] domain_event_publishing_test.rs (3 tests)
  - [ ] error_scenarios_test.rs (3 tests)
  - [ ] linear_workflow_test.rs (3 tests)
- [ ] Task templates created for each test type (10+ templates)
- [ ] Example handlers created for each workflow pattern
- [ ] Unit test coverage >80%

### Test Counts

| Test File | Test Count | Assertions |
|-----------|------------|------------|
| batch_processing | 2 | 15+ |
| conditional_approval | 5 | 25+ |
| diamond_workflow | 3 | 12+ |
| domain_event_publishing | 3 | 20+ |
| error_scenarios | 3 | 15+ |
| linear_workflow | 3 | 12+ |
| **Total** | **19 E2E tests** | **99+ assertions** |

---

## Execution Order

1. **Phase 1**: Remove HTTP server, add FFI observability (Day 1)
2. **Phase 2**: Create directory structure, templates, handlers (Day 1-2)
3. **Phase 3**: Implement E2E tests (Day 2-3)
4. **Phase 4**: Verify unit test coverage, fill gaps (Day 3)

---

## Related Tickets

- **TAS-91**: Blog post examples (Python, Rust, TypeScript) - separate scope
- **TAS-104**: Server and Bootstrap - prerequisite, completed
- **TAS-103**: Handler System - prerequisite, completed
- **TAS-102**: Event System - prerequisite, completed

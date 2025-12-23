# TypeScript Worker E2E Tests

End-to-end tests for TypeScript handler implementations through the orchestration API.

## Overview

These tests validate that TypeScript handlers work correctly in the distributed system by calling the orchestration API and verifying task/step states. Tests are **language-agnostic** - they test the API contract, not TypeScript-specific implementation details.

## Test Coverage

| Test File                      | Tests | Description                                    |
|--------------------------------|-------|------------------------------------------------|
| `batch_processing_test.rs`     | 2     | CSV batch processing with parallel workers     |
| `conditional_approval_test.rs` | 5     | Decision point routing based on amount         |
| `diamond_workflow_test.rs`     | 3     | Parallel branches with convergence             |
| `domain_event_publishing_test.rs` | 3  | Domain event fire-and-forget verification      |
| `error_scenarios_test.rs`      | 3     | Success, permanent, and retryable errors       |
| `linear_workflow_test.rs`      | 3     | Sequential step execution                      |
| **Total**                      | **19**| **TAS-105 E2E Test Requirements**              |

### Test Details

#### Batch Processing (`batch_processing_test.rs`)
- `test_typescript_csv_batch_processing` - 1000 rows, 5 workers, verify aggregation
- `test_typescript_no_batches_scenario` - Empty CSV handling

#### Conditional Approval (`conditional_approval_test.rs`)
- `test_typescript_small_amount_auto_approval` - $500 → auto-approve (4 steps)
- `test_typescript_medium_amount_manager_approval` - $2,500 → manager (4 steps)
- `test_typescript_large_amount_dual_approval` - $10,000 → dual (5 steps)
- `test_typescript_boundary_small_threshold` - Exactly $1,000
- `test_typescript_boundary_large_threshold` - Exactly $5,000

#### Diamond Workflow (`diamond_workflow_test.rs`)
- `test_typescript_diamond_workflow_standard` - Input 6: parallel branches converge
- `test_typescript_diamond_workflow_parallel_branches` - Verify parallel execution
- `test_typescript_diamond_workflow_different_input` - Input 4

#### Domain Event Publishing (`domain_event_publishing_test.rs`)
- `test_typescript_domain_event_publishing_success` - Event stats verification
- `test_typescript_domain_event_publishing_concurrent` - 3 concurrent tasks
- `test_typescript_domain_event_metrics_availability` - Worker health + metrics

#### Error Scenarios (`error_scenarios_test.rs`)
- `test_typescript_success_scenario` - Happy path (< 3s)
- `test_typescript_permanent_failure_scenario` - No retries (< 5s)
- `test_typescript_retryable_failure_scenario` - Backoff exhaustion (100ms - 10s)

#### Linear Workflow (`linear_workflow_test.rs`)
- `test_typescript_linear_workflow_standard` - Input 6
- `test_typescript_linear_workflow_different_input` - Input 10
- `test_typescript_linear_workflow_ordering` - Input 8

## Prerequisites

### 1. Start Docker Services

```bash
# From tasker-core directory
docker compose -f docker/docker-compose.test.yml up -d --build

# Wait for services to be healthy
docker compose -f docker/docker-compose.test.yml ps
```

### 2. Verify Services

```bash
# Check orchestration service
curl http://localhost:8080/health

# Check TypeScript worker service (Rust HTTP API)
curl http://localhost:8084/health

# Check metrics endpoint
curl http://localhost:8084/metrics/events
```

## Running Tests

### Run All TypeScript E2E Tests

```bash
# Using cargo test
DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test" \
cargo test --all-features e2e::typescript

# Using cargo-nextest (faster)
DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test" \
cargo nextest run --all-features -E 'test(typescript)'
```

### Run Specific Test Module

```bash
# Batch processing tests
DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test" \
cargo test --all-features batch_processing_test -- --nocapture

# Error scenarios tests
DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test" \
cargo test --all-features error_scenarios_test -- --nocapture
```

## Test Handlers

Test handlers are located in `workers/typescript/tests/handlers/examples/`:

```
examples/
├── batch_processing/       # CSV batch processing handlers
│   └── step_handlers/
│       └── batch-handlers.ts
├── conditional_approval/   # Approval routing handlers
│   └── step_handlers/
│       └── approval-handlers.ts
├── diamond_workflow/       # Diamond pattern handlers
│   └── step_handlers/
│       └── diamond-handlers.ts
├── domain_events/          # Domain event handlers
│   └── step_handlers/
│       └── event-handlers.ts
├── linear_workflow/        # Linear sequence handlers
│   └── step_handlers/
│       └── linear-step-handlers.ts
├── test_errors/            # Error testing handlers
│   └── step_handlers/
│       └── error-handlers.ts
└── test_scenarios/         # Success testing handlers
    └── step_handlers/
        └── success-step-handler.ts
```

## Task Templates

Templates are in `tests/fixtures/task_templates/typescript/`:

- `batch_processing_ts.yaml` - CSV batch processing workflow
- `conditional_approval_handler_ts.yaml` - Approval routing workflow
- `diamond_workflow_handler_ts.yaml` - Diamond pattern workflow
- `domain_event_publishing_ts.yaml` - Domain events workflow
- `error_testing_handler_ts.yaml` - Error testing workflow
- `linear_workflow_handler_ts.yaml` - Linear sequence workflow
- `permanent_error_only_ts.yaml` - Permanent error testing
- `retryable_error_only_ts.yaml` - Retryable error testing
- `success_only_ts.yaml` - Success scenario testing

## Troubleshooting

### Services not healthy

```bash
# Check service logs
docker compose -f docker/docker-compose.test.yml logs typescript-worker
docker compose -f docker/docker-compose.test.yml logs orchestration

# Restart services
docker compose -f docker/docker-compose.test.yml restart
```

### Handlers not found

```bash
# Verify volume mount
docker compose -f docker/docker-compose.test.yml exec typescript-worker \
  ls -la /app/tests/fixtures/task_templates/typescript/

# Check template path
docker compose -f docker/docker-compose.test.yml exec typescript-worker \
  env | grep TASKER_TEMPLATE_PATH
```

### Tests timeout

```bash
# Check task status manually
curl http://localhost:8080/v1/tasks/{task_uuid}

# Check worker metrics
curl http://localhost:8084/metrics/events
```

## Architecture

### Test Flow

```
Rust E2E Test
    ↓
IntegrationTestManager.setup()
    ↓
create_task_request(...)
    ↓
orchestration_client.create_task(...)
    ↓
[Orchestration Service] → [PostgreSQL PGMQ] → [TypeScript Worker]
    ↓                                              ↓
wait_for_task_completion(...)     TypeScript Handler Execution
    ↓                                              ↓
orchestration_client.get_task(...)         Result via PGMQ
    ↓
Assertions (task status, step states, etc.)
```

### Why Rust Tests for TypeScript Handlers?

1. **API Contract Testing** - Tests verify orchestration API behavior, not TypeScript internals
2. **Language Agnostic** - Same test framework for Rust, Ruby, Python, TypeScript workers
3. **Single Source of Truth** - One test infrastructure, consistent assertions
4. **Reduced Duplication** - No need for separate Jest integration harness

TypeScript unit tests (`workers/typescript/tests/`) focus on framework concerns:
- FFI layer integration
- Type wrappers and conversions
- Event system
- Handler registry and discovery

## Related Tickets

- **TAS-100**: TypeScript Worker implementation (parent)
- **TAS-105**: Testing and Examples (this work)
- **TAS-91**: Blog post examples (out of scope)
- **TAS-109**: Domain events parity (future work)

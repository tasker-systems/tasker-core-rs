# Ruby Worker E2E Tests

End-to-end tests for Ruby handler implementations through the orchestration API.

## Overview

These tests validate that Ruby handlers work correctly in the distributed system by calling the orchestration API and verifying task/step states. Tests are **language-agnostic** - they test the API contract, not Ruby-specific implementation details.

## Test Coverage

### Error Scenarios (`error_scenarios_test.rs`)

Validates core error handling patterns:

1. **Success Scenario** - Happy path execution
   - Task completes successfully
   - No retries needed
   - Fast execution (< 3s)

2. **Permanent Failure** - Non-retryable errors
   - Task fails immediately
   - No retry attempts (< 2s)
   - Step marked as non-retryable

3. **Retryable Failure** - Retry with backoff exhaustion
   - Retries with exponential backoff
   - Respects retry limit (2 retries)
   - Eventually fails after exhaustion
   - Observable retry delays (>100ms, <3s)

4. **Mixed Workflow** - Parallel execution with mixed outcomes
   - Some steps succeed, others fail
   - Orchestration handles mixed results
   - Overall task marked as failed

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

# Check Ruby worker service
curl http://localhost:8082/health

# Verify handler discovery (should show test_errors namespace)
curl http://localhost:8082/handlers | jq
```

## Running Tests

### Run All Ruby E2E Tests

```bash
cargo test --test error_scenarios_test -- --ignored
```

### Run Specific Test

```bash
# Success scenario only
cargo test --test error_scenarios_test test_success_scenario -- --ignored --nocapture

# Permanent failure scenario
cargo test --test error_scenarios_test test_permanent_failure_scenario -- --ignored --nocapture

# Retryable failure scenario
cargo test --test error_scenarios_test test_retryable_failure_scenario -- --ignored --nocapture

# Mixed workflow scenario
cargo test --test error_scenarios_test test_mixed_workflow_scenario -- --ignored --nocapture
```

### Run with cargo-nextest (Faster)

```bash
cargo nextest run --test error_scenarios_test
```

## Test Handlers

Test handlers are located in `workers/ruby/spec/handlers/examples/error_scenarios/`:

- `success_handler.rb` - Always succeeds
- `permanent_error_handler.rb` - Raises PermanentError (no retries)
- `retryable_error_handler.rb` - Raises RetryableError (exhausts retries)

Templates are in `workers/ruby/spec/fixtures/templates/error_testing_handler.yaml`.

## Configuration

### Handler Discovery

Ruby worker discovers handlers via `TASKER_TEMPLATE_PATH` environment variable:

```yaml
# docker-compose.test.yml
environment:
  TASKER_TEMPLATE_PATH: /app/workers/ruby/spec/fixtures/templates

volumes:
  - ../workers/ruby/spec:/app/workers/ruby/spec:ro
```

Handlers are discovered relative to template path:
```
/app/workers/ruby/spec/fixtures/templates  (templates)
/app/workers/ruby/spec/handlers/examples   (handlers - discovered relative to templates)
```

### Test Environment Overrides

Fast execution for testing (configured in template):

```yaml
environments:
  test:
    steps:
      - name: retryable_error_step
        retry:
          limit: 2
          backoff_base_ms: 50    # Fast for testing (vs 100ms default)
          max_backoff_ms: 200    # Fast for testing (vs 500ms default)
```

## Troubleshooting

### Services not healthy

```bash
# Check service logs
docker compose -f docker/docker-compose.test.yml logs ruby-worker
docker compose -f docker/docker-compose.test.yml logs orchestration

# Restart services
docker compose -f docker/docker-compose.test.yml restart
```

### Handlers not found

```bash
# Verify volume mount
docker compose -f docker/docker-compose.test.yml exec ruby-worker ls -la /app/workers/ruby/spec/

# Check template path
docker compose -f docker/docker-compose.test.yml exec ruby-worker env | grep TASKER_TEMPLATE_PATH

# Check handler discovery in logs
docker compose -f docker/docker-compose.test.yml logs ruby-worker | grep -i "handler\|template"
```

### Tests timeout

```bash
# Check task status manually
curl http://localhost:8080/v1/tasks/{task_uuid}

# Check worker is processing
curl http://localhost:8082/status
```

## Architecture

### Test Flow

```
Rust E2E Test
    ↓
IntegrationTestManager.setup()
    ↓
create_task_request_with_bypass(...)
    ↓
orchestration_client.create_task(...)
    ↓
[Orchestration Service] → [PostgreSQL PGMQ] → [Ruby Worker]
    ↓                                              ↓
wait_for_task_completion/failure(...)     Ruby Handler Execution
    ↓                                              ↓
orchestration_client.get_task(...)         Result via PGMQ
    ↓
Assertions (task status, step states, etc.)
```

### Why Rust Tests for Ruby Handlers?

1. **API Contract Testing** - Tests verify orchestration API behavior, not Ruby internals
2. **Language Agnostic** - Same test framework for Rust, Ruby, Python workers
3. **Single Source of Truth** - One test infrastructure, consistent assertions
4. **Reduced Duplication** - No need for separate RSpec integration harness

Ruby specs (`workers/ruby/spec/`) focus on framework concerns:
- FFI layer integration
- Type wrappers and conversions
- Event system (dry-events)
- Handler registry and discovery

## Next Steps

Future test additions:
- Blog example workflows (e-commerce, data pipeline)
- Additional workflow patterns (linear, diamond, tree, mixed DAG)
- Concurrent task execution
- Worker restart scenarios

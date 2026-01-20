# TAS-73: Domain Event Testing in Multi-Instance Mode

## Problem Statement

Domain event tests verify that events are published correctly by checking worker metrics endpoints (`/metrics/events`) before and after task execution. This approach is **inferential** - we infer success by observing metric count changes rather than directly verifying event delivery.

**Challenge in cluster mode**: When multiple worker instances are running, a task may be processed by any worker. The test only checks ONE worker's metrics endpoint, but the work could have been done by a different instance. This makes the tests non-deterministic in multi-instance mode.

### Current Test Pattern

```rust
// 1. Capture event stats BEFORE task execution
let stats_before = worker_client.get_domain_event_stats().await?;

// 2. Execute task (processed by unknown worker instance)
let task = orchestration_client.create_task(request).await?;
wait_for_task_completion(&task).await?;

// 3. Capture event stats AFTER and compare
let stats_after = worker_client.get_domain_event_stats().await?;
assert!(stats_after.router.total_routed > stats_before.router.total_routed);
```

**Problem**: `worker_client` points to ONE worker (e.g., `worker-rust-1` at port 8100), but `worker-rust-2` at port 8101 may have processed the task.

## Options Analysis

### Option A: Feature Flag to Skip in Cluster Mode

Add a `test-cluster` feature check to skip domain event metric verification tests in cluster mode.

**Implementation**:
```rust
#[tokio::test]
#[cfg_attr(feature = "test-cluster", ignore)]
async fn test_rust_domain_event_publishing_success() -> Result<()> {
    // ... existing test
}
```

**Pros**:
- Simple, low effort
- Clear intent - these tests are known to be non-deterministic in cluster mode
- Domain events are already well-tested at unit and message-queue levels
- Matches existing pattern (`test-services`, `test-messaging`, `test-cluster`)

**Cons**:
- Reduced test coverage in cluster mode
- Doesn't validate domain events work correctly across multiple instances

**Effort**: Low (1-2 hours)

### Option B: Check All Worker Metrics Endpoints

Extend test helpers to detect cluster mode and check ALL configured worker endpoints. If ANY worker shows an increase in event counts, treat it as success.

**Implementation**:
```rust
async fn verify_domain_events_published(
    worker_urls: &[String],
    stats_before: HashMap<String, EventStats>,
) -> Result<bool> {
    for url in worker_urls {
        let client = WorkerClient::new(url);
        let stats_after = client.get_domain_event_stats().await?;
        if let Some(before) = stats_before.get(url) {
            if stats_after.router.total_routed > before.router.total_routed {
                return Ok(true); // Found the worker that processed it
            }
        }
    }
    Ok(false)
}
```

**Pros**:
- Maintains test coverage in cluster mode
- Validates events work across instances

**Cons**:
- More complex test infrastructure
- **Signal degradation**: With parallel test execution, multiple tasks may be running simultaneously, causing metric increases from OTHER tests. This makes the "did MY task's events get published?" question harder to answer.
- Requires environment variable coordination (`TASKER_TEST_WORKER_RUST_URLS`, etc.)

**Effort**: Medium (4-8 hours)

### Option C: Use External Event Queue Verification

Force domain event tests to use "publish external" pathway, then read from the configured domain event queue (PGMQ or RabbitMQ) to verify events appear.

**Implementation**:
```rust
// Configure test handlers to publish externally
// Read from pgmq.domain_events or rabbitmq domain event exchange
// Filter by correlation ID to find our task's events
```

**Pros**:
- Precise verification - actually reads the published events
- Works reliably in both single and cluster mode

**Cons**:
- **High complexity**: Must handle both PGMQ and RabbitMQ backends
- Different queue schemas and read patterns per backend
- Need correlation ID tracking through events
- Significant test infrastructure changes

**Effort**: High (2-3 days)

## Recommendation: Option A (Feature Flag)

**Recommended approach**: Use feature flags to skip domain event metric verification tests in cluster mode.

### Rationale

1. **Domain events are well-tested elsewhere**:
   - Unit tests in `tasker-worker/src/domain_events/`
   - Message-queue integration tests
   - Event router tests
   - Publisher integration tests

2. **Metrics endpoints are ephemeral**:
   - In production, OpenTelemetry handles observability
   - Worker `/metrics/events` endpoints are for development debugging
   - They're not designed for aggregation across instances

3. **Signal quality vs. effort tradeoff**:
   - Option B's signal degrades with parallel test execution
   - Option C's effort is high for relatively low concern
   - Option A is pragmatic - the core functionality is tested elsewhere

4. **TAS-73's focus is different**:
   - Multi-instance validation should focus on race conditions, claim semantics, and state machine atomicity
   - Domain event publishing is inherently fire-and-forget; if the step completes, the event was attempted
   - What matters is task/step lifecycle correctness, which other e2e tests validate

### Implementation Plan

1. Add `#[cfg_attr(feature = "test-cluster", ignore)]` to domain event metric verification tests
2. Keep the tests running in single-instance mode (CI's default)
3. Document that cluster mode skips these tests intentionally

### Affected Tests

```
tests/e2e/rust/domain_event_publishing_test.rs
- test_rust_domain_event_publishing_success
- test_rust_domain_event_step_results
- test_rust_domain_event_publishing_concurrent
- test_rust_domain_event_metrics_availability

tests/e2e/ruby/domain_event_publishing_test.rs
- test_domain_event_publishing_success
- test_domain_event_publishing_concurrent
- test_ruby_domain_event_metrics_availability

tests/e2e/python/domain_event_publishing_test.rs
- test_python_domain_event_publishing_success
- test_python_domain_event_publishing_concurrent
- test_python_domain_event_metrics_availability
```

## Future Considerations

If domain event verification becomes critical in cluster mode, Option B could be revisited with these mitigations:
- Run domain event tests in isolation (not parallel with other tests)
- Use test-specific correlation IDs and filter metric checks
- Accept some non-determinism as acceptable for integration testing

Option C remains available if precise verification becomes a hard requirement, but the effort is likely not justified given the existing test coverage.

---

**Status**: Recommendation documented. Implementation deferred to focus on TAS-73 priority: validating workflow lifecycle correctness in multi-instance mode (failure patterns 2-4 from test results).

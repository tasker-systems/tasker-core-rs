# TAS-65 Phase 2: Domain Event Subscribers and Enhanced Publishing

**Created**: 2025-11-28
**Updated**: 2025-11-28
**Status**: Implemented ✅
**Depends on**: TAS-65 Phase 1 (Domain Event Publication Architecture)

## Implementation Status

| Part | Description | Status |
|------|-------------|--------|
| Part 1 | Extended publication conditions (retryable_failure, permanent_failure) | ✅ Complete |
| Part 2 | Broadcast delivery mode | ✅ Complete |
| Part 3 | Test manager event observability extensions | ✅ Complete |
| Part 4 | Failure path integration tests | ✅ Complete |
| Part 5 | Example event subscribers (Rust + Ruby) | ✅ Complete |
| Part 6 | Fast event test helper (integration tests) | ✅ Complete |
| Part 7 | Documentation updates | ✅ Complete |

### Key Implementation Files

| File | Purpose |
|------|---------|
| `tasker-shared/src/models/core/task_template/event_declaration.rs` | Extended `PublicationCondition` enum with `RetryableFailure`, `PermanentFailure`; Added `Broadcast` to `EventDeliveryMode` |
| `tasker-worker/src/worker/event_router.rs` | Added `Broadcast` variant to `EventRouteOutcome`, `route_broadcast()` method, stats tracking |
| `tests/common/domain_event_test_helpers.rs` | **NEW** - `DurableEventCapture` for PGMQ event inspection with cleanup |
| `tests/common/fast_event_test_helper.rs` | **NEW** - `FastEventCapture` for in-memory event capture |
| `tests/integration/domain_event_workflow/failure_path.rs` | **NEW** - 5 failure path test scenarios |
| `workers/rust/src/event_subscribers/logging_subscriber.rs` | **NEW** - Example logging subscriber |
| `workers/rust/src/event_subscribers/metrics_subscriber.rs` | **NEW** - `EventMetricsCollector` for fast events |
| `workers/ruby/spec/handlers/examples/domain_events/subscribers/logging_subscriber.rb` | **NEW** - Ruby logging subscriber examples |
| `workers/ruby/spec/handlers/examples/domain_events/subscribers/metrics_subscriber.rb` | **NEW** - Ruby `MetricsSubscriber` for fast events |

## Overview

This specification covers the second phase of domain event implementation, focusing on:

1. **Extended Publication Conditions** - Distinguishing retryable vs permanent failures
2. **Broadcast Delivery Mode** - Dual-path publishing (durable + fast)
3. **Test Observability** - Verifying events were actually published
4. **Example Event Subscribers** - Demonstrating fast event consumption
5. **Test Infrastructure** - Helpers for both durable and fast event testing

## Motivation

Phase 1 established the domain event publication architecture with durable (PGMQ) and fast (in-process) delivery modes. However, several gaps remain:

- **No failure type distinction**: Cannot differentiate retryable errors (timeouts) from permanent errors (validation failures) in YAML declarations
- **No dual-path publishing**: Cannot send the same event to both external consumers AND internal metrics
- **No test observability**: Tests prove workflow completion but not event firing
- **No subscriber examples**: Fast events have no example consumers to demonstrate the pattern
- **Limited test infrastructure**: No helpers for verifying PGMQ queue contents or capturing in-process events

## Architecture

### Event Flow with All Delivery Modes

```
Step Execution Complete
         │
         ▼
  StepEventPublisher.publish(ctx)
         │
         ├─────────────────────────────────────────────┐
         │                                             │
         ▼                                             ▼
  Check Condition                              Custom Publisher
  (success/failure/                            (implements own
   retryable_failure/                           condition logic)
   permanent_failure/always)
         │
         ▼
  EventRouter.route_event(delivery_mode, ...)
         │
    ┌────┴────┬────────────┐
    │         │            │
    ▼         ▼            ▼
 Durable    Fast       Broadcast
    │         │            │
    ▼         ▼            ├──► Fast (first, real-time)
  PGMQ   InProcess        │
  Queue     Bus           └──► Durable (second, reliable)
    │         │
    ▼         ▼
 External  Internal
Consumers Subscribers
```

### Retryability Source of Truth

Retryability MUST come from the database, not the execution result:

```
┌─────────────────────────────────────────────────────────┐
│ get_step_readiness_status(pool, step_uuid)              │
│                                                          │
│   Returns StepReadinessStatus with:                     │
│   - current_attempts                                     │
│   - max_attempts                                        │
│   - retryable (from step definition)                    │
│   - backoff_request_seconds                             │
│                                                          │
│   is_retryable() = retryable && attempts < max_attempts │
└─────────────────────────────────────────────────────────┘
```

This ensures consistency with orchestration decisions and respects retry exhaustion.

## Part 1: Extended Publication Conditions

### Current State

```yaml
publishes_events:
  - name: payment.failed
    condition: failure  # Any failure - cannot distinguish type
```

### Target State

```yaml
publishes_events:
  - name: payment.failed.retryable
    condition: retryable_failure   # Only retryable failures (e.g., timeouts)
    delivery_mode: fast            # Internal alerting

  - name: payment.failed.permanent
    condition: permanent_failure   # Only permanent failures (e.g., validation)
    delivery_mode: durable         # External DLQ consumers
```

### Implementation

**File: `tasker-shared/src/models/core/task_template/event_declaration.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PublicationCondition {
    #[default]
    Success,
    Failure,           // Any failure (backward compatible)
    RetryableFailure,  // Step failed but is retryable
    PermanentFailure,  // Step failed permanently
    Always,
}

impl PublicationCondition {
    /// Determine if event should be published given step outcome
    ///
    /// # Arguments
    /// * `step_succeeded` - Whether step completed successfully
    /// * `is_retryable` - From DB: whether step can be retried (None if success)
    pub fn should_publish(&self, step_succeeded: bool, is_retryable: Option<bool>) -> bool {
        match self {
            Self::Success => step_succeeded,
            Self::Failure => !step_succeeded,
            Self::RetryableFailure => !step_succeeded && is_retryable == Some(true),
            Self::PermanentFailure => !step_succeeded && is_retryable == Some(false),
            Self::Always => true,
        }
    }
}
```

**File: `tasker-worker/src/worker/step_event_publisher.rs`**

```rust
// In DefaultDomainEventPublisher::publish()
let step_succeeded = ctx.step_succeeded();

// Get authoritative retryability from database
let is_retryable = if !step_succeeded {
    let readiness = get_step_readiness_status(&self.pool, ctx.step_uuid()).await?;
    readiness.map(|s| s.is_retryable())
} else {
    None  // Not applicable for success
};

for event_decl in declared_events {
    if !event_decl.condition.should_publish(step_succeeded, is_retryable) {
        result.skipped.push(event_decl.name.clone());
        continue;
    }
    // ... publish event
}
```

## Part 2: Broadcast Delivery Mode

### Use Case

Some events need both external visibility (audit, cross-service) AND internal processing (metrics, alerts):

```yaml
publishes_events:
  - name: order.completed
    condition: success
    delivery_mode: broadcast  # Both PGMQ AND in-process
```

### Implementation

**File: `tasker-shared/src/models/core/task_template/event_declaration.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EventDeliveryMode {
    #[default]
    Durable,    // PGMQ only (external/public boundary)
    Fast,       // In-process only (internal subscribers)
    Broadcast,  // Both PGMQ AND in-process
}
```

**File: `tasker-worker/src/worker/event_router.rs`**

```rust
pub async fn route_event(
    &self,
    delivery_mode: EventDeliveryMode,
    event_name: &str,
    payload: DomainEventPayload,
    metadata: EventMetadata,
) -> RouteResult {
    match delivery_mode {
        EventDeliveryMode::Durable => self.route_durable(...).await,
        EventDeliveryMode::Fast => self.route_fast(...).await,
        EventDeliveryMode::Broadcast => self.route_broadcast(...).await,
    }
}

async fn route_broadcast(&self, ...) -> RouteResult {
    // Fast FIRST for real-time responsiveness
    let fast_result = self.route_fast(event_name, payload.clone(), metadata.clone()).await;

    // Then durable for reliable persistence
    let durable_result = self.route_durable(event_name, payload, metadata).await;

    // Track metrics (fast failures don't constitute business failures)
    {
        let mut stats = self.stats.lock().unwrap();
        stats.broadcast_routed += 1;
        if fast_result.is_err() {
            stats.fast_delivery_errors += 1;
        }
    }

    // Return durable result as primary success indicator
    durable_result.map(|outcome| EventRouteOutcome::Broadcast {
        event_id: outcome.event_id(),
        queue_name: /* from durable result */,
    })
}
```

### Security Consideration

**Warning**: Broadcast mode sends data to both the public PGMQ boundary AND internal subscribers. Ensure sensitive data is not inadvertently exposed:

- Use `fast` for internal-only events with sensitive data
- Use `durable` for external consumers
- Use `broadcast` only when the same payload is appropriate for both audiences

## Part 3: Test Observability

### Goal

Tests should verify events were actually published, not just that workflows completed.

### Durable Event Verification (PGMQ)

**File: `tests/common/lifecycle_test_manager.rs`**

```rust
impl LifecycleTestManager {
    /// Get domain events from namespace queue
    /// Direct SQL bypasses VTT (visibility timeout) for test inspection
    pub async fn get_domain_events(
        &self,
        namespace: &str,
        limit: i64,
    ) -> Result<Vec<DomainEventMessage>> {
        let queue_name = format!("{}_domain_events", namespace);
        sqlx::query_as(
            &format!("SELECT msg_id, message FROM pgmq.q_{} ORDER BY msg_id LIMIT $1", queue_name)
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    /// Count events in namespace queue
    pub async fn count_domain_events(&self, namespace: &str) -> Result<i64> { ... }

    /// Assert specific event exists
    pub async fn assert_event_published(
        &self,
        namespace: &str,
        event_name: &str,
    ) -> Result<DomainEventMessage> { ... }

    /// Wait for event with timeout
    pub async fn wait_for_domain_event(
        &self,
        namespace: &str,
        event_name: &str,
        timeout_secs: u64,
    ) -> Result<DomainEventMessage> { ... }

    /// Clean up specific messages by msg_id at teardown
    /// Only deletes messages we've tracked during the test (not all namespace messages)
    /// This prevents test pollution when multiple tests run against the same namespace
    pub async fn cleanup_tracked_events(
        &self,
        namespace: &str,
        msg_ids: &[i64],
    ) -> Result<CleanupResult> {
        let queue_name = format!("{}_domain_events", namespace);
        let pgmq_client = PgmqClient::new_with_pool(self.pool.clone()).await;

        let mut deleted = 0;
        let mut failed = 0;

        for msg_id in msg_ids {
            // Best effort deletion - don't fail if message already consumed
            match pgmq_client.delete_message(&queue_name, *msg_id).await {
                Ok(()) => deleted += 1,
                Err(e) => {
                    debug!("Failed to delete msg_id {}: {} (may already be consumed)", msg_id, e);
                    failed += 1;
                }
            }
        }

        Ok(CleanupResult { deleted, failed })
    }
}

#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub deleted: usize,
    pub failed: usize,
}
```

### Fast Event Capture (Integration Tests Only)

**Important**: Fast event capture only works in integration tests (same memory space). E2E tests run workers in Docker containers - fast events are not observable from the test harness.

**File: `tests/common/fast_event_test_helper.rs`**

```rust
/// Test helper for capturing fast/in-process events
/// NOTE: Only works in integration tests (same memory space)
pub struct FastEventCapture {
    pub bus: Arc<RwLock<InProcessEventBus>>,
    captured_events: Arc<RwLock<Vec<DomainEvent>>>,
    event_count: Arc<AtomicUsize>,
}

impl FastEventCapture {
    pub fn new() -> Self { ... }
    pub fn event_count(&self) -> usize { ... }
    pub async fn get_captured_events(&self) -> Vec<DomainEvent> { ... }
    pub async fn assert_event_captured(&self, event_name: &str) -> Option<DomainEvent> { ... }
    pub async fn wait_for_events(&self, count: usize, timeout_ms: u64) -> bool { ... }
}
```

### Test Strategy by Test Type

| Test Type | Durable Events | Fast Events |
|-----------|----------------|-------------|
| Unit tests | Mock publisher | FastEventCapture |
| Integration tests | PGMQ queries | FastEventCapture |
| E2E tests (Docker) | PGMQ queries | Not observable* |

*Fast events in E2E tests are validated indirectly through unit/integration tests.

## Part 4: Example Event Subscribers

### Purpose

Demonstrate how to consume fast/in-process events for common use cases like logging and metrics.

### Rust Examples

**File: `workers/rust/src/event_subscribers/logging_subscriber.rs`**

```rust
//! Example logging subscriber for fast/in-process events

use std::sync::Arc;
use tasker_shared::events::registry::EventHandler;
use tracing::info;

/// Create a logging subscriber that logs all events matching a pattern
pub fn create_logging_subscriber(prefix: &str) -> EventHandler {
    let prefix = prefix.to_string();
    Arc::new(move |event| {
        let prefix = prefix.clone();
        Box::pin(async move {
            info!(
                prefix = %prefix,
                event_name = %event.event_name,
                event_id = %event.event_id,
                task_uuid = %event.metadata.task_uuid,
                "Domain event received"
            );
            Ok(())
        })
    })
}
```

**File: `workers/rust/src/event_subscribers/metrics_subscriber.rs`**

```rust
//! Example metrics subscriber for fast/in-process events

pub struct EventMetricsCollector {
    pub events_received: AtomicU64,
    pub success_events: AtomicU64,
    pub failure_events: AtomicU64,
}

impl EventMetricsCollector {
    pub fn create_handler(self: &Arc<Self>) -> EventHandler {
        let collector = self.clone();
        Arc::new(move |event| {
            let collector = collector.clone();
            Box::pin(async move {
                collector.events_received.fetch_add(1, Ordering::Relaxed);
                if event.payload.execution_result.success {
                    collector.success_events.fetch_add(1, Ordering::Relaxed);
                } else {
                    collector.failure_events.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            })
        })
    }
}
```

### Ruby Examples

**File: `workers/ruby/spec/handlers/examples/domain_events/subscribers/logging_subscriber.rb`**

```ruby
module DomainEvents
  module Subscribers
    class LoggingSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      subscribes_to '*'  # Subscribe to all events

      def handle(event)
        logger.info "[LoggingSubscriber] Event: #{event[:event_name]}"
        logger.info "  Task: #{event.dig(:metadata, :task_uuid)}"
        logger.info "  Step: #{event.dig(:metadata, :step_name)}"
      end
    end
  end
end
```

**File: `workers/ruby/spec/handlers/examples/domain_events/subscribers/metrics_subscriber.rb`**

```ruby
module DomainEvents
  module Subscribers
    class MetricsSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      subscribes_to 'payment.*', 'order.*'

      class << self
        attr_accessor :events_received, :success_count, :failure_count
      end

      self.reset_counters!

      def handle(event)
        self.class.events_received += 1
        if event.dig(:execution_result, :success)
          self.class.success_count += 1
        else
          self.class.failure_count += 1
        end
      end
    end
  end
end
```

### Registration Pattern

**Rust:**
```rust
let mut bus = InProcessEventBus::new(config);
bus.subscribe("payment.*", create_logging_subscriber("payments"))?;
bus.subscribe("*", metrics_collector.create_handler())?;
```

**Ruby:**
```ruby
registry = TaskerCore::DomainEvents::SubscriberRegistry.instance
registry.register(DomainEvents::Subscribers::LoggingSubscriber)
registry.register(DomainEvents::Subscribers::MetricsSubscriber)
registry.start_all!
```

## Part 5: Failure Path Integration Tests

### Goal

Verify event publishers are called for both success and failure scenarios.

**File: `tests/integration/domain_event_workflow/failure_paths.rs`**

```rust
/// Test event publishing on retryable failure
#[sqlx::test(migrator = "...")]
async fn test_domain_event_on_retryable_failure(pool: PgPool) -> Result<()> {
    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "domain_events",
        "domain_event_publishing",
        json!({ "simulate_failure": true, "failure_type": "retryable" }),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Simulate retryable step failure
    manager.fail_step_retryable(
        init_result.task_uuid,
        "domain_events_process_payment",
        "Payment gateway timeout",
    ).await?;

    // Verify retryable_failure event was published
    let event = manager.assert_event_published(
        "domain_events",
        "payment.failed.retryable"
    ).await?;

    // Cleanup
    manager.drain_domain_events("domain_events").await?;

    Ok(())
}

/// Test event publishing on permanent failure
#[sqlx::test(migrator = "...")]
async fn test_domain_event_on_permanent_failure(pool: PgPool) -> Result<()> {
    // Similar but with permanent failure (exhausted retries or non-retryable)
}

/// Test 'always' condition publishes regardless of outcome
#[sqlx::test(migrator = "...")]
async fn test_domain_event_always_condition(pool: PgPool) -> Result<()> {
    // Verify inventory.updated publishes on both success and failure
}
```

## Updated YAML Template

**File: `tests/fixtures/task_templates/*/domain_event_publishing.yaml`**

```yaml
steps:
  - name: domain_events_validate_order
    publishes_events:
      - name: order.validated
        condition: success
        delivery_mode: fast  # Internal only

  - name: domain_events_process_payment
    publishes_events:
      - name: payment.processed
        condition: success
        delivery_mode: broadcast  # Both external AND internal
        publisher: DomainEvents::Publishers::PaymentEventPublisher
      - name: payment.failed.retryable
        condition: retryable_failure
        delivery_mode: fast  # Internal alerting only
      - name: payment.failed.permanent
        condition: permanent_failure
        delivery_mode: durable  # External DLQ

  - name: domain_events_update_inventory
    publishes_events:
      - name: inventory.updated
        condition: always  # Audit trail regardless of outcome
        delivery_mode: durable

  - name: domain_events_send_notification
    publishes_events:
      - name: notification.sent
        condition: success
        delivery_mode: broadcast  # Real-time internal + external audit
        publisher: DomainEvents::Publishers::NotificationEventPublisher
```

## Test Coverage Matrix

| Scenario | Test Type | File | Verifies |
|----------|-----------|------|----------|
| Success condition | Integration | `happy_path.rs` | `condition: success` |
| Retryable failure | Integration | `failure_paths.rs` | `condition: retryable_failure` |
| Permanent failure | Integration | `failure_paths.rs` | `condition: permanent_failure` |
| Always condition | Integration | `failure_paths.rs` | Publishes on both outcomes |
| Durable delivery | Integration | `happy_path.rs` | PGMQ queue has message |
| Fast delivery | Integration | `fast_event_test.rs` | FastEventCapture receives |
| Broadcast delivery | Integration | `broadcast_test.rs` | Both paths receive |
| Subscriber patterns | Unit | `in_process_event_bus.rs` | Wildcard matching |
| Message cleanup | Integration | All tests | `cleanup_tracked_events(msg_ids)` |

## Implementation Order

1. **Part 1**: Extended conditions (foundation)
2. **Part 2**: Broadcast delivery mode
3. **Part 3**: Test manager extensions
4. **Part 4**: Example subscribers
5. **Part 5**: Failure path tests

## Files to Modify

| File | Changes |
|------|---------|
| `tasker-shared/src/models/core/task_template/event_declaration.rs` | Add conditions + broadcast |
| `tasker-worker/src/worker/event_router.rs` | Broadcast routing, stats |
| `tasker-worker/src/worker/step_event_publisher.rs` | DB-based retryability |
| `tests/common/lifecycle_test_manager.rs` | Event inspection methods |
| `tests/common/fast_event_test_helper.rs` | NEW - FastEventCapture |
| `tests/integration/domain_event_workflow/failure_paths.rs` | NEW |
| `workers/rust/src/event_subscribers/` | NEW directory |
| `workers/ruby/spec/handlers/examples/domain_events/subscribers/` | NEW directory |
| `tests/fixtures/task_templates/*/domain_event_publishing.yaml` | Update with new conditions |

## Success Criteria

- [x] All 5 publication conditions work correctly (`Success`, `Failure`, `RetryableFailure`, `PermanentFailure`, `Always`)
- [x] Broadcast mode delivers to both paths (fast first, then durable)
- [x] Integration tests verify durable events via PGMQ queries (`DurableEventCapture`)
- [x] Integration tests verify fast events via FastEventCapture
- [x] Example subscribers demonstrate logging and metrics patterns (Rust + Ruby)
- [x] Failure path tests cover retryable vs permanent failures (5 test scenarios)
- [x] All tests clean up queues via tracked `msg_id` cleanup

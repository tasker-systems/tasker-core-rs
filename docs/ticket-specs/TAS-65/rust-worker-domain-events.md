# TAS-65 Rust Worker Domain Events

**Status**: Implementation Ready
**Date**: 2025-11-24
**Phase**: 2.3 → 2.4 Bridge (Pre-Ruby FFI)

## Overview

This document specifies the integration of domain event publishing into native Rust worker handlers. This serves as a validation step before implementing Ruby FFI bindings, allowing us to identify and resolve architectural issues in pure Rust first.

## Motivation

Following the established pattern of validating architecture in Rust before adding FFI complexity:
1. **Validate Architecture**: Prove the design works in pure Rust without FFI overhead
2. **Identify Gotchas**: Surface integration issues early in a simpler environment
3. **Establish Patterns**: Create reusable patterns for Ruby FFI implementation
4. **Enable Testing**: Build integration tests that validate end-to-end flow

## Architecture

### Data Flow

```
Rust Handler → DomainEventPublisher → PGMQ Queue → EventConsumer → EventRegistry → Subscribers
```

### Component Integration

```
RustStepHandlerRegistry
  ├─ Handlers (with StepHandlerConfig)
  │    └─ event_publisher: Option<Arc<DomainEventPublisher>>
  │
  └─ Two-Phase Initialization:
       1. Create registry (handlers with empty config)
       2. Inject publisher after worker bootstrap
```

### Key Design Decisions

#### 1. Event Publishing Never Fails the Step

**Decision**: Hard distinction between business logic failures and event publishing failures.

**Implementation**:
- Event publish errors are logged with `warn!`
- Step execution continues regardless of publish outcome
- Observability over reliability for event system

**Rationale**: Events are for observability and downstream processing. A handler that successfully processes a payment should not fail if event publishing fails.

#### 2. Injected Publisher (Not Global)

**Decision**: Pass `DomainEventPublisher` through `StepHandlerConfig`.

**Advantages**:
- Testable: Easy to mock or omit in tests
- Explicit: Handlers know if they can publish events
- Flexible: Different handlers can have different publishers

**Alternatives Considered**:
- Global publisher: Hard to test, implicit dependencies
- TaskSequenceStep field: Requires changes to shared types

#### 3. Two-Phase Initialization

**Decision**: Create registry first, inject publisher after worker bootstrap.

**Implementation**:
```rust
// Phase 1: Create registry (at construction)
let registry = RustStepHandlerRegistry::new();

// Phase 2: Inject publisher (after worker bootstrap)
registry.set_event_publisher(event_publisher);
```

**Rationale**: Avoids circular dependency between registry creation and worker bootstrap.

## Implementation Specification

### 1. Extend StepHandlerConfig

**File**: `workers/rust/src/step_handlers/mod.rs`

```rust
#[derive(Debug, Clone)]
pub struct StepHandlerConfig {
    /// YAML initialization data
    pub data: HashMap<String, Value>,

    /// Optional domain event publisher (injected after creation)
    pub event_publisher: Option<Arc<DomainEventPublisher>>,
}

impl StepHandlerConfig {
    pub fn new(data: HashMap<String, Value>) -> Self {
        Self {
            data,
            event_publisher: None,
        }
    }

    pub fn with_event_publisher(mut self, publisher: Arc<DomainEventPublisher>) -> Self {
        self.event_publisher = Some(publisher);
        self
    }
}
```

### 2. Create DomainEventPublishable Trait

**File**: `workers/rust/src/step_handlers/mod.rs`

```rust
use tasker_shared::events::domain_events::{DomainEventPublisher, EventMetadata};
use chrono::Utc;

/// Trait providing convenient domain event publishing for step handlers
///
/// Automatically extracts metadata from TaskSequenceStep and handles errors gracefully.
/// Event publishing failures are logged but do NOT fail the step.
pub trait DomainEventPublishable {
    /// Access to the handler's configuration
    fn config(&self) -> &StepHandlerConfig;

    /// The handler's name for metadata (defaults to RustStepHandler::name())
    fn handler_name(&self) -> &str;

    /// Publish a domain event with automatic metadata extraction
    ///
    /// # Arguments
    /// - `step_data`: Current step execution context (provides all metadata)
    /// - `event_name`: Event name in dot notation (e.g., "payment.processed")
    /// - `payload`: Event payload as JSON value
    ///
    /// # Error Handling
    /// - If publisher not configured: silently skip (no-op)
    /// - If publish fails: log warning, continue (don't fail step)
    ///
    /// # Example
    /// ```rust,ignore
    /// self.publish_domain_event(
    ///     step_data,
    ///     "order.fulfilled",
    ///     json!({"order_id": 123, "total": 99.99})
    /// ).await?;
    /// ```
    async fn publish_domain_event(
        &self,
        step_data: &TaskSequenceStep,
        event_name: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        // Skip if publisher not configured (e.g., in tests)
        let publisher = match self.config().event_publisher.as_ref() {
            Some(p) => p,
            None => {
                debug!("Skipping event '{}' - no publisher configured", event_name);
                return Ok(());
            }
        };

        // Extract all metadata from step_data
        let metadata = EventMetadata {
            task_uuid: step_data.task.task.task_uuid,
            step_uuid: Some(step_data.workflow_step.workflow_step_uuid),
            step_name: Some(step_data.workflow_step.name.clone()),
            namespace: step_data.task.namespace_name.clone(),
            correlation_id: step_data.task.task.correlation_id,
            fired_at: Utc::now(),
            fired_by: self.handler_name().to_string(),
        };

        // Publish event with graceful error handling
        if let Err(e) = publisher.publish_event(event_name, payload, metadata).await {
            warn!(
                handler = self.handler_name(),
                event_name = event_name,
                error = %e,
                "Failed to publish domain event - step will continue"
            );
        } else {
            debug!(
                handler = self.handler_name(),
                event_name = event_name,
                "Domain event published successfully"
            );
        }

        Ok(())
    }
}
```

### 3. Update RustStepHandlerRegistry

**File**: `workers/rust/src/step_handlers/registry.rs`

Add method to inject publisher into all handlers:

```rust
impl RustStepHandlerRegistry {
    /// Inject domain event publisher into all handlers (phase 2 initialization)
    ///
    /// This method updates all handler configs with the publisher after the registry
    /// has been created. This enables two-phase initialization:
    /// 1. Create registry during construction
    /// 2. Inject publisher after worker bootstrap (when message_client available)
    pub fn set_event_publisher(&mut self, publisher: Arc<DomainEventPublisher>) {
        // Re-create all handlers with updated config including publisher
        for (name, handler) in &mut self.handlers {
            // Get current handler's config data
            let config_data = handler.config().data.clone();

            // Create new config with publisher
            let new_config = StepHandlerConfig::new(config_data)
                .with_event_publisher(publisher.clone());

            // Re-create handler with new config
            *handler = Self::create_handler_instance(name, new_config);
        }

        info!(
            "Injected domain event publisher into {} handlers",
            self.handlers.len()
        );
    }

    /// Helper to create handler instance by name with given config
    fn create_handler_instance(
        name: &str,
        config: StepHandlerConfig,
    ) -> Arc<dyn RustStepHandler> {
        // Match on handler name and instantiate
        // (existing logic from register_all_handlers, extracted)
        match name {
            "linear_step_1" => Arc::new(LinearStep1Handler::new(config)),
            // ... all other handlers ...
            _ => panic!("Unknown handler: {}", name),
        }
    }
}
```

**Note**: This requires extracting handler creation logic into a helper method to avoid duplication.

### 4. Update Bootstrap Integration

**File**: `workers/rust/src/bootstrap.rs`

```rust
pub async fn bootstrap() -> Result<(WorkerSystemHandle, RustEventHandler)> {
    // Phase 1: Create registry without publisher
    info!("📋 Creating native Rust step handler registry...");
    let mut registry = Arc::new(RustStepHandlerRegistry::new());

    // Bootstrap worker
    let event_system = get_global_event_system();
    let event_handler = RustEventHandler::new(
        registry.clone(),
        event_system.clone(),
        "rust-worker-demo-001".to_string(),
    );

    let worker_handle = WorkerBootstrap::bootstrap_with_event_system(
        Some(event_system)
    ).await?;

    // Phase 2: Create and inject domain event publisher
    info!("🔔 Setting up domain event publisher...");
    let message_client = worker_handle.get_message_client(); // Need to add this accessor
    let event_publisher = Arc::new(DomainEventPublisher::new(message_client));

    Arc::get_mut(&mut registry)
        .expect("Registry should have unique ownership")
        .set_event_publisher(event_publisher);

    info!("✅ Domain event publisher injected into {} handlers", registry.handler_count());

    Ok((worker_handle, event_handler))
}
```

**Required Change**: Add `get_message_client()` method to `WorkerSystemHandle` to expose message client.

### 5. Example Handler with Event Publishing

**File**: `workers/rust/src/step_handlers/payment_example.rs`

```rust
use super::{DomainEventPublishable, RustStepHandler, StepHandlerConfig, success_result};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::info;

/// Example handler demonstrating domain event publishing
#[derive(Debug)]
pub struct ProcessPaymentHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessPaymentHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract payment data from task context
        let amount: f64 = step_data.get_context_field("amount")?;
        let currency: String = step_data.get_context_field("currency")?;

        // Business logic: process payment
        info!("Processing payment: {} {}", amount, currency);
        let transaction_id = format!("TXN-{}", uuid::Uuid::new_v4());

        // Publish domain event
        self.publish_domain_event(
            step_data,
            "payment.processed",
            json!({
                "transaction_id": transaction_id,
                "amount": amount,
                "currency": currency,
                "status": "completed"
            })
        ).await?;

        Ok(success_result(
            step_uuid,
            json!({
                "transaction_id": transaction_id,
                "status": "success"
            }),
            start_time.elapsed().as_millis() as i64,
            None,
        ))
    }

    fn name(&self) -> &str {
        "process_payment"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// Implement DomainEventPublishable
impl DomainEventPublishable for ProcessPaymentHandler {
    fn config(&self) -> &StepHandlerConfig {
        &self.config
    }

    fn handler_name(&self) -> &str {
        self.name()
    }
}
```

## Testing Strategy

### Unit Tests

**File**: `workers/rust/src/step_handlers/payment_example.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_payment_handler_without_publisher() {
        // Handler works without publisher (no-op for events)
        let config = StepHandlerConfig::empty();
        let handler = ProcessPaymentHandler::new(config);

        // Create test step_data
        let step_data = create_test_step_data();

        // Should succeed without publisher
        let result = handler.call(&step_data).await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests

**File**: `workers/rust/tests/domain_events_integration_test.rs`

```rust
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_handler_publishes_domain_event(pool: PgPool) -> Result<()> {
    // Setup
    let context = SystemContext::with_pool(pool.clone()).await?;
    let namespace = "payments";

    // Initialize domain event queues
    context.initialize_domain_event_queues(&[namespace]).await?;

    // Create publisher
    let publisher = Arc::new(DomainEventPublisher::new(context.message_client()));

    // Create handler with publisher
    let config = StepHandlerConfig::empty().with_event_publisher(publisher);
    let handler = ProcessPaymentHandler::new(config);

    // Execute handler
    let step_data = create_test_step_data(namespace);
    let result = handler.call(&step_data).await?;
    assert!(result.is_success());

    // Verify event was published to queue
    let queue_name = format!("{}_domain_events", namespace);
    let message_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM pgmq.q_{}",
        queue_name
    ))
    .fetch_one(&pool)
    .await?;

    assert_eq!(message_count, 1, "Should have published one event");

    // Read and verify event content
    let event_json: serde_json::Value = sqlx::query_scalar(&format!(
        "SELECT message FROM pgmq.q_{} LIMIT 1",
        queue_name
    ))
    .fetch_one(&pool)
    .await?;

    assert_eq!(event_json["event_name"], "payment.processed");
    assert_eq!(event_json["metadata"]["namespace"], namespace);

    Ok(())
}
```

## Success Criteria

1. ✅ `StepHandlerConfig` extended with optional `event_publisher`
2. ✅ `DomainEventPublishable` trait provides one-line event publishing
3. ✅ `RustStepHandlerRegistry` supports two-phase initialization
4. ✅ Example handler demonstrates event publishing pattern
5. ✅ Unit tests verify handler works with/without publisher
6. ✅ Integration test validates end-to-end event flow
7. ✅ Event publishing failures logged but don't fail steps
8. ✅ Zero breaking changes to existing handlers

## Future Work (Post-Implementation)

1. **Ruby FFI Integration** (Phase 2.4): Replicate this pattern for Ruby handlers
2. **Event Schema Validation**: Integrate with `EventSchemaValidator` from Phase 2.2
3. **Metrics**: Add OpenTelemetry metrics for event publication success/failure rates
4. **Automatic Event Registration**: Register event handlers based on YAML declarations

## Appendix: Data Access Patterns

### Confirmed Available Data in TaskSequenceStep

```rust
// Correlation ID (from Phase 1.5 worker instrumentation)
step_data.task.task.correlation_id: Uuid

// Namespace (for queue routing)
step_data.task.namespace_name: String

// Task UUID
step_data.task.task.task_uuid: Uuid

// Step UUID
step_data.workflow_step.workflow_step_uuid: Uuid

// Step Name
step_data.workflow_step.name: String
```

All required metadata for `EventMetadata` is directly accessible without additional lookups.

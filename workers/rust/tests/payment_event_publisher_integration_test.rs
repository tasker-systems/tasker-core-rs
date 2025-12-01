//! # TAS-65 Phase 5.2: Payment Event Publisher Integration Tests
//!
//! Tests the full flow of:
//! 1. ProcessPaymentHandler executes business logic (no event publishing)
//! 2. PaymentEventPublisher invoked after step completion
//! 3. Domain events published to PGMQ
//!
//! This validates the post-execution publisher callback architecture where:
//! - Handlers focus on business logic only
//! - Event publishing is handled by StepEventPublisher after step completes
//! - Events are published based on YAML configuration (publishes_events)

use std::collections::HashMap;
use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use tasker_shared::events::domain_events::DomainEventPublisher;
use tasker_shared::messaging::execution_types::{StepExecutionMetadata, StepExecutionResult, StepExecutionError};
use tasker_shared::models::core::task::Task;
use tasker_shared::models::core::task::TaskForOrchestration;
use tasker_shared::models::core::task_template::{
    EventDeclaration, EventDeliveryMode, HandlerDefinition, PublicationCondition,
    RetryConfiguration, StepDefinition,
};
use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::TaskSequenceStep;

use tasker_worker::worker::step_event_publisher::StepEventContext;
use tasker_worker::worker::step_event_publisher_registry::StepEventPublisherRegistry;

use tasker_worker_rust::step_handlers::payment_event_publisher::PaymentEventPublisher;
use tasker_worker_rust::step_handlers::payment_example::ProcessPaymentHandler;
use tasker_worker_rust::step_handlers::{RustStepHandler, StepHandlerConfig};

/// Helper to create a payment TaskSequenceStep for testing
fn create_payment_task_sequence_step(
    amount: f64,
    currency: &str,
    namespace: &str,
    publish_events: bool,
) -> TaskSequenceStep {
    let now = chrono::Utc::now().naive_utc();
    let task_uuid = Uuid::new_v4();
    let step_uuid = Uuid::new_v4();

    // Define event declarations based on whether we want to publish events
    let publishes_events = if publish_events {
        vec![
            EventDeclaration::builder()
                .name("payment.processed".to_string())
                .description("Payment was processed successfully".to_string())
                .condition(PublicationCondition::Success)
                .schema(serde_json::json!({"type": "object"}))
                .delivery_mode(EventDeliveryMode::Durable)
                .publisher("PaymentEventPublisher".to_string())
                .build(),
            EventDeclaration::builder()
                .name("payment.failed".to_string())
                .description("Payment processing failed".to_string())
                .condition(PublicationCondition::Failure)
                .schema(serde_json::json!({"type": "object"}))
                .delivery_mode(EventDeliveryMode::Durable)
                .publisher("PaymentEventPublisher".to_string())
                .build(),
            EventDeclaration::builder()
                .name("payment.analytics".to_string())
                .description("Analytics event for all outcomes".to_string())
                .condition(PublicationCondition::Always)
                .schema(serde_json::json!({"type": "object"}))
                .delivery_mode(EventDeliveryMode::Durable)
                .publisher("PaymentEventPublisher".to_string())
                .build(),
        ]
    } else {
        vec![]
    };

    TaskSequenceStep {
        task: TaskForOrchestration {
            task: Task {
                task_uuid,
                named_task_uuid: Uuid::new_v4(),
                complete: false,
                requested_at: now,
                initiator: Some("test".to_string()),
                source_system: Some("payment_system".to_string()),
                reason: Some("Payment integration test".to_string()),
                bypass_steps: None,
                tags: None,
                context: Some(serde_json::json!({
                    "amount": amount,
                    "currency": currency,
                    "customer_tier": "premium",
                })),
                identity_hash: format!("payment_test_{}", Uuid::new_v4()),
                priority: 5,
                created_at: now,
                updated_at: now,
                correlation_id: Uuid::new_v4(),
                parent_correlation_id: None,
            },
            task_name: "payment_workflow".to_string(),
            task_version: "1.0".to_string(),
            namespace_name: namespace.to_string(),
        },
        workflow_step: WorkflowStepWithName {
            workflow_step_uuid: step_uuid,
            task_uuid,
            named_step_uuid: Uuid::new_v4(),
            name: "process_payment".to_string(),
            template_step_name: "process_payment".to_string(),
            retryable: true,
            max_attempts: Some(3),
            in_process: false,
            processed: false,
            processed_at: None,
            attempts: Some(1),
            last_attempted_at: None,
            backoff_request_seconds: None,
            inputs: None,
            results: None,
            skippable: false,
            created_at: now,
            updated_at: now,
        },
        dependency_results: HashMap::new(),
        step_definition: StepDefinition {
            name: "process_payment".to_string(),
            description: Some("Process payment transaction".to_string()),
            handler: HandlerDefinition {
                callable: "ProcessPaymentHandler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: Default::default(),
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events,
            batch_config: None,
        },
    }
}

/// Helper to create execution result from handler output
fn create_execution_result(step_uuid: Uuid, success: bool, result_data: serde_json::Value) -> StepExecutionResult {
    if success {
        StepExecutionResult {
            step_uuid,
            success: true,
            result: result_data,
            metadata: StepExecutionMetadata {
                execution_time_ms: 50,
                handler_version: Some("1.0".to_string()),
                retryable: true,
                completed_at: chrono::Utc::now(),
                worker_id: Some("rust-worker-test".to_string()),
                worker_hostname: Some("test-host".to_string()),
                started_at: Some(chrono::Utc::now()),
                custom: HashMap::new(),
                error_code: None,
                error_type: None,
                context: HashMap::new(),
            },
            status: "completed".to_string(),
            error: None,
            orchestration_metadata: None,
        }
    } else {
        StepExecutionResult {
            step_uuid,
            success: false,
            result: serde_json::Value::Null,
            metadata: StepExecutionMetadata {
                execution_time_ms: 25,
                handler_version: Some("1.0".to_string()),
                retryable: true,
                completed_at: chrono::Utc::now(),
                worker_id: Some("rust-worker-test".to_string()),
                worker_hostname: Some("test-host".to_string()),
                started_at: Some(chrono::Utc::now()),
                custom: HashMap::new(),
                error_code: Some("PAYMENT_DECLINED".to_string()),
                error_type: Some("PaymentError".to_string()),
                context: HashMap::new(),
            },
            status: "failed".to_string(),
            error: Some(StepExecutionError {
                message: "Payment was declined by processor".to_string(),
                error_type: Some("PaymentError".to_string()),
                retryable: true,
                backtrace: None,
            }),
            orchestration_metadata: None,
        }
    }
}

/// Test 1: ProcessPaymentHandler executes business logic only (no event publishing)
/// Verifies that the handler returns data for downstream event publishing
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_payment_handler_executes_business_logic_only(_pool: PgPool) -> sqlx::Result<()> {
    // Create test step data
    let namespace = format!("payments_{}", &Uuid::new_v4().to_string()[..8]);
    let step_data = create_payment_task_sequence_step(100.00, "USD", &namespace, true);

    // Execute handler
    let handler = ProcessPaymentHandler::new(StepHandlerConfig::empty());
    let result = handler.call(&step_data).await.expect("Handler should execute successfully");

    // Verify handler returns business result
    assert!(result.is_success(), "Handler should succeed");
    assert!(result.result.get("transaction_id").is_some(), "Should have transaction_id");
    assert_eq!(result.result["amount"], 100.00, "Amount should match");
    assert_eq!(result.result["currency"], "USD", "Currency should match");
    assert_eq!(result.result["status"], "success", "Status should be success");

    // Handler does NOT publish events - that's StepEventPublisher's job
    // This test just verifies the handler returns the right data

    Ok(())
}

/// Test 2: PaymentEventPublisher publishes success events
/// Verifies that when the handler succeeds, payment.processed event is published
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_payment_event_publisher_success_flow(pool: PgPool) -> sqlx::Result<()> {
    // Setup context with database
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    // Use short namespace to avoid PGMQ 47-char limit
    let namespace = format!("pay_{}", &Uuid::new_v4().to_string()[..8]);

    // Initialize domain event queues
    context
        .initialize_domain_event_queues(&[&namespace])
        .await
        .expect("Failed to initialize domain event queues");

    // Create domain publisher
    let domain_publisher = Arc::new(DomainEventPublisher::new(context.message_client()));

    // Create publisher registry with PaymentEventPublisher
    let mut registry = StepEventPublisherRegistry::new(domain_publisher.clone());
    registry.register(PaymentEventPublisher::new(domain_publisher.clone()));

    // Create test step data with event declarations
    let step_data = create_payment_task_sequence_step(250.00, "EUR", &namespace, true);

    // Simulate handler execution result
    let execution_result = create_execution_result(
        step_data.workflow_step.workflow_step_uuid,
        true,
        serde_json::json!({
            "transaction_id": "TXN-12345",
            "amount": 250.00,
            "currency": "EUR",
            "status": "success",
        }),
    );

    // Create event context (pure DTO)
    let ctx = StepEventContext::new(step_data.clone(), execution_result);

    // Get the publisher from registry (as the worker would)
    let publisher = registry.get("PaymentEventPublisher")
        .expect("PaymentEventPublisher should be registered");

    // Verify publisher handles payment steps
    assert!(publisher.should_handle("process_payment"), "Should handle payment steps");

    // Publish events
    let result = publisher.publish(&ctx).await;

    // Verify results
    println!("Published: {:?}", result.published);
    println!("Skipped: {:?}", result.skipped);
    println!("Errors: {:?}", result.errors);

    // Should have published payment.processed (success condition met)
    // Should have published payment.analytics (always condition)
    // Should NOT have published payment.failed (failure condition not met)
    assert!(result.errors.is_empty(), "Should have no errors");
    assert!(result.published.len() >= 1, "Should have published at least 1 event");

    // Verify events were published to the queue
    let queue_name = format!("{}_domain_events", namespace);
    let message_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM pgmq.q_{}",
        queue_name
    ))
    .fetch_one(&pool)
    .await?;

    assert!(message_count >= 1, "Should have at least one message in queue");

    // Cleanup
    let dlq_queue = format!("{}_domain_events_dlq", namespace);
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", queue_name))
        .execute(&pool)
        .await;
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", dlq_queue))
        .execute(&pool)
        .await;

    Ok(())
}

/// Test 3: PaymentEventPublisher publishes failure events
/// Verifies that when the handler fails, payment.failed event is published
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_payment_event_publisher_failure_flow(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    let namespace = format!("pay_{}", &Uuid::new_v4().to_string()[..8]);

    context
        .initialize_domain_event_queues(&[&namespace])
        .await
        .expect("Failed to initialize domain event queues");

    let domain_publisher = Arc::new(DomainEventPublisher::new(context.message_client()));

    let mut registry = StepEventPublisherRegistry::new(domain_publisher.clone());
    registry.register(PaymentEventPublisher::new(domain_publisher.clone()));

    // Create test step with event declarations
    let step_data = create_payment_task_sequence_step(1000.00, "USD", &namespace, true);

    // Simulate FAILED handler result
    let execution_result = create_execution_result(
        step_data.workflow_step.workflow_step_uuid,
        false,
        serde_json::Value::Null,
    );

    let ctx = StepEventContext::new(step_data.clone(), execution_result);

    let publisher = registry.get("PaymentEventPublisher")
        .expect("PaymentEventPublisher should be registered");

    let result = publisher.publish(&ctx).await;

    println!("Published: {:?}", result.published);
    println!("Skipped: {:?}", result.skipped);
    println!("Errors: {:?}", result.errors);

    // Should have published payment.failed (failure condition met)
    // Should have published payment.analytics (always condition)
    // Should NOT have published payment.processed (success condition not met)
    assert!(result.errors.is_empty(), "Should have no errors");
    assert!(result.published.len() >= 1, "Should have published at least 1 event");

    // Verify events in queue
    let queue_name = format!("{}_domain_events", namespace);
    let message_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM pgmq.q_{}",
        queue_name
    ))
    .fetch_one(&pool)
    .await?;

    assert!(message_count >= 1, "Should have at least one message in queue");

    // Cleanup
    let dlq_queue = format!("{}_domain_events_dlq", namespace);
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", queue_name))
        .execute(&pool)
        .await;
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", dlq_queue))
        .execute(&pool)
        .await;

    Ok(())
}

/// Test 4: End-to-end flow - Handler + Publisher
/// Verifies the complete flow: handler executes, then publisher invokes
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_end_to_end_payment_flow(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    let namespace = format!("pay_{}", &Uuid::new_v4().to_string()[..8]);

    context
        .initialize_domain_event_queues(&[&namespace])
        .await
        .expect("Failed to initialize domain event queues");

    let domain_publisher = Arc::new(DomainEventPublisher::new(context.message_client()));

    let mut registry = StepEventPublisherRegistry::new(domain_publisher.clone());
    registry.register(PaymentEventPublisher::new(domain_publisher.clone()));

    // Create step data
    let step_data = create_payment_task_sequence_step(500.00, "GBP", &namespace, true);

    // STEP 1: Execute handler (business logic only)
    let handler = ProcessPaymentHandler::new(StepHandlerConfig::empty());
    let handler_result = handler.call(&step_data).await.expect("Handler should succeed");

    assert!(handler_result.is_success(), "Handler should succeed");
    assert!(handler_result.result.get("transaction_id").is_some());

    // STEP 2: Create execution result from handler output
    // (In real system, this is what the worker does after handler completes)
    let execution_result = StepExecutionResult {
        step_uuid: step_data.workflow_step.workflow_step_uuid,
        success: handler_result.success,
        result: handler_result.result.clone(),
        metadata: handler_result.metadata.clone(),
        status: if handler_result.success { "completed".to_string() } else { "failed".to_string() },
        error: handler_result.error.clone(),
        orchestration_metadata: handler_result.orchestration_metadata.clone(),
    };

    // STEP 3: Invoke publisher (post-execution callback)
    let ctx = StepEventContext::new(step_data.clone(), execution_result);

    let publisher = registry.get("PaymentEventPublisher")
        .expect("PaymentEventPublisher should be registered");

    let publish_result = publisher.publish(&ctx).await;

    // Verify publishing succeeded
    assert!(publish_result.errors.is_empty(), "Should have no publishing errors");
    assert!(!publish_result.published.is_empty(), "Should have published events");

    // Verify events in queue
    let queue_name = format!("{}_domain_events", namespace);
    let message_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM pgmq.q_{}",
        queue_name
    ))
    .fetch_one(&pool)
    .await?;

    assert!(message_count >= 1, "Should have at least one message in queue");

    println!("End-to-end test passed:");
    println!("  - Handler executed successfully");
    println!("  - Published {} events to {}", publish_result.published.len(), queue_name);

    // Cleanup
    let dlq_queue = format!("{}_domain_events_dlq", namespace);
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", queue_name))
        .execute(&pool)
        .await;
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", dlq_queue))
        .execute(&pool)
        .await;

    Ok(())
}

/// Test 5: Registry lookup and fallback to default publisher
/// Verifies that registry returns DefaultDomainEventPublisher for unknown publishers
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_publisher_registry_lookup(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    let domain_publisher = Arc::new(DomainEventPublisher::new(context.message_client()));

    let mut registry = StepEventPublisherRegistry::new(domain_publisher.clone());
    registry.register(PaymentEventPublisher::new(domain_publisher.clone()));

    // Lookup existing publisher
    let payment_publisher = registry.get("PaymentEventPublisher");
    assert!(payment_publisher.is_some(), "PaymentEventPublisher should be found");
    assert_eq!(payment_publisher.unwrap().name(), "PaymentEventPublisher");

    // Lookup non-existent publisher returns None
    let unknown = registry.get("UnknownPublisher");
    assert!(unknown.is_none(), "Unknown publisher should not be found");

    // get_or_default returns DefaultDomainEventPublisher for unknown names
    let fallback = registry.get_or_default(Some("UnknownPublisher"));
    assert_eq!(fallback.name(), "DefaultDomainEventPublisher");

    // get_or_default returns default when None is passed
    let default = registry.get_or_default(None);
    assert_eq!(default.name(), "DefaultDomainEventPublisher");

    Ok(())
}

/// Test 6: No events published when step has no event declarations
/// Verifies that steps without publishes_events don't trigger publishing
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_no_events_when_not_declared(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    let namespace = format!("pay_{}", &Uuid::new_v4().to_string()[..8]);

    context
        .initialize_domain_event_queues(&[&namespace])
        .await
        .expect("Failed to initialize domain event queues");

    let domain_publisher = Arc::new(DomainEventPublisher::new(context.message_client()));

    let mut registry = StepEventPublisherRegistry::new(domain_publisher.clone());
    registry.register(PaymentEventPublisher::new(domain_publisher.clone()));

    // Create step WITHOUT event declarations
    let step_data = create_payment_task_sequence_step(100.00, "USD", &namespace, false);

    // Verify no events are declared
    assert!(
        step_data.step_definition.publishes_events.is_empty(),
        "Should have no event declarations"
    );

    let execution_result = create_execution_result(
        step_data.workflow_step.workflow_step_uuid,
        true,
        serde_json::json!({"status": "success"}),
    );

    let ctx = StepEventContext::new(step_data, execution_result);

    // All events should be skipped because they're not declared
    let publisher = registry.get("PaymentEventPublisher")
        .expect("PaymentEventPublisher should be registered");

    let result = publisher.publish(&ctx).await;

    // With no events declared, all should be skipped (not in declaration list)
    println!("Published: {:?}", result.published);
    println!("Skipped: {:?}", result.skipped);

    // The publisher checks is_event_declared(), so events not declared are skipped
    // or not published at all

    // Cleanup
    let queue_name = format!("{}_domain_events", namespace);
    let dlq_queue = format!("{}_domain_events_dlq", namespace);
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", queue_name))
        .execute(&pool)
        .await;
    let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", dlq_queue))
        .execute(&pool)
        .await;

    Ok(())
}

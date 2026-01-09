use sqlx::PgPool;
use std::collections::HashMap;
use tasker_shared::events::{DomainEvent, DomainEventPayload, DomainEventPublisher, EventMetadata};
use tasker_shared::messaging::execution_types::{StepExecutionMetadata, StepExecutionResult};
use tasker_shared::models::core::task::Task;
use tasker_shared::models::core::task::TaskForOrchestration;
use tasker_shared::models::core::task_template::{
    HandlerDefinition, RetryConfiguration, StepDefinition,
};
use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::TaskSequenceStep;
use uuid::Uuid;

/// Helper to create test metadata
fn create_test_metadata(namespace: &str) -> EventMetadata {
    EventMetadata {
        task_uuid: Uuid::new_v4(),
        step_uuid: Some(Uuid::new_v4()),
        step_name: Some("test_step".to_string()),
        namespace: namespace.to_string(),
        correlation_id: Uuid::new_v4(),
        fired_at: chrono::Utc::now(),
        fired_by: "TestHandler".to_string(),
    }
}

/// Helper to create a minimal TaskSequenceStep for testing
fn create_test_task_sequence_step() -> TaskSequenceStep {
    let now = chrono::Utc::now().naive_utc();
    TaskSequenceStep {
        task: TaskForOrchestration {
            task: Task {
                task_uuid: Uuid::new_v4(),
                named_task_uuid: Uuid::new_v4(),
                complete: false,
                requested_at: now,
                initiator: Some("test".to_string()),
                source_system: None,
                reason: None,
                bypass_steps: None,
                tags: None,
                context: Some(serde_json::json!({})),
                identity_hash: "test_hash".to_string(),
                priority: 5,
                created_at: now,
                updated_at: now,
                correlation_id: Uuid::new_v4(),
                parent_correlation_id: None,
            },
            task_name: "test_task".to_string(),
            task_version: "1.0".to_string(),
            namespace_name: "test".to_string(),
        },
        workflow_step: WorkflowStepWithName {
            workflow_step_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            named_step_uuid: Uuid::new_v4(),
            name: "test_step".to_string(),
            template_step_name: "test_step".to_string(),
            retryable: true,
            max_attempts: Some(3),
            in_process: false,
            processed: false,
            processed_at: None,
            attempts: Some(0),
            last_attempted_at: None,
            backoff_request_seconds: None,
            inputs: None,
            results: None,
            skippable: false,
            created_at: now,
            updated_at: now,
            checkpoint: None,
        },
        dependency_results: HashMap::new(),
        step_definition: StepDefinition {
            name: "test_step".to_string(),
            description: Some("Test step".to_string()),
            handler: HandlerDefinition {
                callable: "TestHandler".to_string(),
                method: None,
                resolver: None,
                initialization: HashMap::new(),
            },
            step_type: Default::default(),
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
            batch_config: None,
        },
    }
}

/// Helper to create a minimal StepExecutionResult for testing
fn create_test_execution_result(
    step_uuid: Uuid,
    payload: serde_json::Value,
) -> StepExecutionResult {
    StepExecutionResult {
        step_uuid,
        success: true,
        result: payload,
        metadata: StepExecutionMetadata {
            execution_time_ms: 100,
            handler_version: None,
            retryable: true,
            completed_at: chrono::Utc::now(),
            worker_id: None,
            worker_hostname: None,
            started_at: None,
            custom: HashMap::new(),
            error_code: None,
            error_type: None,
            context: HashMap::new(),
        },
        status: "completed".to_string(),
        error: None,
        orchestration_metadata: None,
    }
}

/// Helper to create a test DomainEventPayload
fn create_test_payload(business_payload: serde_json::Value) -> DomainEventPayload {
    let tss = create_test_task_sequence_step();
    let execution_result = create_test_execution_result(
        tss.workflow_step.workflow_step_uuid,
        business_payload.clone(),
    );
    DomainEventPayload {
        task_sequence_step: tss,
        execution_result,
        payload: business_payload,
    }
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_domain_event_publisher_creation(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool)
        .await
        .expect("Failed to create system context");

    let publisher = DomainEventPublisher::new(context.message_client());

    // Test Debug formatting
    let debug_str = format!("{:?}", publisher);
    assert!(debug_str.contains("DomainEventPublisher"));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_publish_event(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    // Use short namespace to avoid PGMQ 47-char queue name limit
    let namespace = format!("test_{}", &Uuid::new_v4().to_string()[..8]);

    // Initialize domain event queues (SystemContext responsibility)
    context
        .initialize_domain_event_queues(&[&namespace])
        .await
        .expect("Failed to initialize domain event queues");

    // Create publisher with message_client (not PgPool)
    let publisher = DomainEventPublisher::new(context.message_client());

    // Publish event
    let metadata = create_test_metadata(&namespace);
    let business_payload = serde_json::json!({
        "order_id": 123,
        "amount": 99.99,
        "status": "processed"
    });
    let payload = create_test_payload(business_payload);

    let event_id = publisher
        .publish_event("order.processed", payload, metadata.clone())
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to publish event: {}", e)))?;

    assert!(!event_id.is_nil(), "Event ID should not be nil");

    // Verify event was published to queue
    let queue_name = format!("{}_domain_events", namespace);
    let message_count: i64 =
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM pgmq.q_{}", queue_name))
            .fetch_one(&pool)
            .await?;

    assert_eq!(message_count, 1, "Should have exactly one message in queue");

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

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_publish_event_with_correlation(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    // Use short namespace to avoid PGMQ 47-char queue name limit
    let namespace = format!("test_{}", &Uuid::new_v4().to_string()[..8]);

    // Initialize domain event queues
    context
        .initialize_domain_event_queues(&[&namespace])
        .await
        .expect("Failed to initialize domain event queues");

    // Create publisher
    let publisher = DomainEventPublisher::new(context.message_client());

    // Publish event with specific correlation_id (from Phase 1.5)
    let correlation_id = Uuid::new_v4();
    let metadata = EventMetadata {
        task_uuid: Uuid::new_v4(),
        step_uuid: Some(Uuid::new_v4()),
        step_name: Some("order_processor".to_string()),
        namespace: namespace.clone(),
        correlation_id,
        fired_at: chrono::Utc::now(),
        fired_by: "OrderProcessor".to_string(),
    };

    let business_payload = serde_json::json!({"order_id": 456});
    let payload = create_test_payload(business_payload);

    publisher
        .publish_event("order.created", payload, metadata)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("Failed to publish event: {}", e)))?;

    // Read message from queue and verify correlation_id
    let queue_name = format!("{}_domain_events", namespace);
    let message: Option<(serde_json::Value,)> = sqlx::query_as(&format!(
        "SELECT message FROM pgmq.q_{} LIMIT 1",
        queue_name
    ))
    .fetch_optional(&pool)
    .await?;

    assert!(message.is_some(), "Message should exist in queue");

    let (message_json,) = message.unwrap();
    let event: DomainEvent =
        serde_json::from_value(message_json).expect("Failed to deserialize event");

    assert_eq!(
        event.metadata.correlation_id, correlation_id,
        "Correlation ID should match"
    );

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

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_multiple_namespaces(pool: PgPool) -> sqlx::Result<()> {
    let context = SystemContext::with_pool(pool.clone())
        .await
        .expect("Failed to create system context");

    // Create two namespaces
    let namespace1 = format!("test_{}", &Uuid::new_v4().to_string()[..8]);
    let namespace2 = format!("test_{}", &Uuid::new_v4().to_string()[..8]);

    // Initialize domain event queues for both namespaces
    context
        .initialize_domain_event_queues(&[&namespace1, &namespace2])
        .await
        .expect("Failed to initialize domain event queues");

    // Verify all queues exist
    let queues = vec![
        format!("{}_domain_events", namespace1),
        format!("{}_domain_events_dlq", namespace1),
        format!("{}_domain_events", namespace2),
        format!("{}_domain_events_dlq", namespace2),
    ];

    for queue_name in &queues {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pgmq.meta WHERE queue_name = $1)")
                .bind(queue_name)
                .fetch_one(&pool)
                .await?;

        assert!(exists, "Queue {} should exist", queue_name);
    }

    // Cleanup
    for queue_name in &queues {
        let _ = sqlx::query(&format!("SELECT pgmq.drop_queue('{}')", queue_name))
            .execute(&pool)
            .await;
    }

    Ok(())
}

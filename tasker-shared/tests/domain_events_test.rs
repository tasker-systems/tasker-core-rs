use sqlx::PgPool;
use tasker_shared::events::{DomainEvent, DomainEventPublisher, EventMetadata};
use tasker_shared::system_context::SystemContext;
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
    let payload = serde_json::json!({
        "order_id": 123,
        "amount": 99.99,
        "status": "processed"
    });

    let event_id = publisher
        .publish_event("order.processed", payload.clone(), metadata.clone())
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

    let payload = serde_json::json!({"order_id": 456});

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

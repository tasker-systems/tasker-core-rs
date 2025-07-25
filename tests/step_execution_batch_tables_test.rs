use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn test_step_execution_batch_tables_creation(pool: PgPool) {
    // Test that all our new tables exist and can be queried

    // 1. Test tasker_step_execution_batches table
    let batch_count = sqlx::query_scalar!("SELECT COUNT(*) FROM tasker_step_execution_batches")
        .fetch_one(&pool)
        .await
        .expect("Should be able to query tasker_step_execution_batches table");

    assert_eq!(batch_count, Some(0)); // Should be empty initially

    // 2. Test tasker_step_execution_batch_steps table
    let batch_steps_count =
        sqlx::query_scalar!("SELECT COUNT(*) FROM tasker_step_execution_batch_steps")
            .fetch_one(&pool)
            .await
            .expect("Should be able to query tasker_step_execution_batch_steps table");

    assert_eq!(batch_steps_count, Some(0)); // Should be empty initially

    // 3. Test tasker_step_execution_batch_received_results table
    let results_count =
        sqlx::query_scalar!("SELECT COUNT(*) FROM tasker_step_execution_batch_received_results")
            .fetch_one(&pool)
            .await
            .expect("Should be able to query tasker_step_execution_batch_received_results table");

    assert_eq!(results_count, Some(0)); // Should be empty initially

    // 4. Test tasker_step_execution_batch_transitions table
    let transitions_count =
        sqlx::query_scalar!("SELECT COUNT(*) FROM tasker_step_execution_batch_transitions")
            .fetch_one(&pool)
            .await
            .expect("Should be able to query tasker_step_execution_batch_transitions table");

    assert_eq!(transitions_count, Some(0)); // Should be empty initially
}

#[sqlx::test]
async fn test_step_execution_batch_basic_crud(pool: PgPool) {
    // Test basic CRUD operations on the new tables

    // First, we need a task to reference
    let task_id = sqlx::query_scalar!(
        r#"
        INSERT INTO tasker_tasks (named_task_id, context, complete, created_at, updated_at)
        VALUES (1001, '{"test": true}'::jsonb, false, NOW(), NOW())
        RETURNING task_id
        "#
    )
    .fetch_one(&pool)
    .await
    .expect("Should create test task");

    // Create a batch
    let batch_uuid = Uuid::new_v4().to_string();
    let batch_id = sqlx::query_scalar!(
        r#"
        INSERT INTO tasker_step_execution_batches 
        (task_id, handler_class, batch_uuid, batch_size, created_at, updated_at)
        VALUES ($1, 'TestHandler', $2, 2, NOW(), NOW())
        RETURNING batch_id
        "#,
        task_id,
        batch_uuid
    )
    .fetch_one(&pool)
    .await
    .expect("Should create batch");

    // Verify batch was created
    let stored_batch = sqlx::query!(
        "SELECT batch_uuid, handler_class, batch_size FROM tasker_step_execution_batches WHERE batch_id = $1",
        batch_id
    )
    .fetch_one(&pool)
    .await
    .expect("Should fetch created batch");

    assert_eq!(stored_batch.batch_uuid, batch_uuid);
    assert_eq!(stored_batch.handler_class, "TestHandler");
    assert_eq!(stored_batch.batch_size, 2);
}

#[sqlx::test]
async fn test_step_execution_batch_constraints(pool: PgPool) {
    // Test unique constraints and foreign key relationships

    let batch_uuid = Uuid::new_v4().to_string();

    // First, we need a task to reference
    let task_id = sqlx::query_scalar!(
        r#"
        INSERT INTO tasker_tasks (named_task_id, context, complete, created_at, updated_at)
        VALUES (1002, '{"test": true}'::jsonb, false, NOW(), NOW())
        RETURNING task_id
        "#
    )
    .fetch_one(&pool)
    .await
    .expect("Should create test task");

    // Create first batch with UUID
    let batch_id1 = sqlx::query_scalar!(
        r#"
        INSERT INTO tasker_step_execution_batches 
        (task_id, handler_class, batch_uuid, batch_size, created_at, updated_at)
        VALUES ($1, 'TestHandler', $2, 1, NOW(), NOW())
        RETURNING batch_id
        "#,
        task_id,
        batch_uuid
    )
    .fetch_one(&pool)
    .await
    .expect("Should create first batch");

    // Try to create second batch with same UUID - should fail due to unique constraint
    let duplicate_uuid_result = sqlx::query_scalar!(
        r#"
        INSERT INTO tasker_step_execution_batches 
        (task_id, handler_class, batch_uuid, batch_size, created_at, updated_at)
        VALUES ($1, 'TestHandler', $2, 1, NOW(), NOW())
        RETURNING batch_id
        "#,
        task_id,
        batch_uuid
    )
    .fetch_one(&pool)
    .await;

    // Should fail due to unique constraint on batch_uuid
    assert!(duplicate_uuid_result.is_err());

    // Verify the first batch still exists
    let batch_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM tasker_step_execution_batches WHERE batch_id = $1",
        batch_id1
    )
    .fetch_one(&pool)
    .await
    .expect("Should be able to count batches");

    assert_eq!(batch_count, Some(1));
}

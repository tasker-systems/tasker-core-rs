//! # TaskQueryService Tests (TAS-63)
//!
//! Integration tests for TaskQueryService validating:
//! - Single task retrieval with full execution context
//! - Task listing with pagination and batch context
//! - Response transformation (to_task_response)
//! - Error handling (not found, metadata errors)
//! - Default execution context fallback

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use tasker_orchestration::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use tasker_orchestration::orchestration::lifecycle::task_initialization::TaskInitializer;
use tasker_orchestration::services::TaskQueryService;
use tasker_shared::models::core::task::TaskListQuery;
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::system_context::SystemContext;

/// Get the path to task template fixtures in the workspace root
fn fixture_path() -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("tests/fixtures/task_templates/rust")
        .to_string_lossy()
        .to_string()
}

/// Helper to set up TaskQueryService and TaskInitializer
async fn setup_services(pool: PgPool) -> Result<(TaskQueryService, TaskInitializer)> {
    let registry = TaskHandlerRegistry::new(pool.clone());
    registry
        .discover_and_register_templates(&fixture_path())
        .await?;

    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
    let step_enqueuer = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let task_initializer = TaskInitializer::new(system_context, step_enqueuer);
    let query_service = TaskQueryService::new(pool);

    Ok((query_service, task_initializer))
}

/// Helper to create a task and return its UUID
async fn create_test_task(initializer: &TaskInitializer) -> Result<Uuid> {
    let request = TaskRequest {
        name: "boundary_max_attempts_one".to_string(),
        namespace: "test_sql".to_string(),
        version: "1.0.0".to_string(),
        context: serde_json::json!({
            "_test_run_id": Uuid::now_v7().to_string(),
            "test_value": 42
        }),
        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
        initiator: "query_service_test".to_string(),
        source_system: "integration_test".to_string(),
        reason: "Testing TaskQueryService".to_string(),
        tags: vec!["test".to_string(), "query_service".to_string()],
        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
        idempotency_key: None,
    };

    let result = initializer.create_task_from_request(request).await?;
    Ok(result.task_uuid)
}

// =============================================================================
// get_task_with_context tests
// =============================================================================

/// Test: get_task_with_context returns full task with execution context
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_task_with_context_success(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let twc = query_service.get_task_with_context(task_uuid).await?;

    assert_eq!(twc.task.task_uuid, task_uuid);
    assert_eq!(twc.task_name, "boundary_max_attempts_one");
    assert_eq!(twc.namespace, "test_sql");
    assert_eq!(twc.version, "1.0.0");
    assert!(!twc.status.is_empty(), "Status should be populated");
    assert!(
        twc.execution_context.total_steps > 0,
        "Should have execution context with steps"
    );

    Ok(())
}

/// Test: get_task_with_context returns step readiness data
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_task_with_context_has_steps(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let twc = query_service.get_task_with_context(task_uuid).await?;

    assert!(
        !twc.steps.is_empty(),
        "Should include step readiness statuses"
    );

    // Verify step data is coherent
    for step in &twc.steps {
        assert_eq!(step.task_uuid, task_uuid, "Step should belong to task");
        assert!(!step.name.is_empty(), "Step should have a name");
    }

    Ok(())
}

/// Test: get_task_with_context returns NotFound for unknown UUID
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_task_with_context_not_found(pool: PgPool) -> Result<()> {
    let (query_service, _) = setup_services(pool).await?;

    let unknown_uuid = Uuid::now_v7();
    let result = query_service.get_task_with_context(unknown_uuid).await;

    assert!(result.is_err(), "Should return error for unknown UUID");
    let err = result.unwrap_err();
    assert!(
        matches!(
            err,
            tasker_orchestration::services::TaskQueryError::NotFound(_)
        ),
        "Error should be NotFound variant"
    );

    Ok(())
}

// =============================================================================
// list_tasks_with_context tests
// =============================================================================

/// Test: list_tasks_with_context returns paginated results
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_tasks_with_context_pagination(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;

    // Create multiple tasks
    for _ in 0..4 {
        create_test_task(&task_initializer).await?;
    }

    let query = TaskListQuery {
        page: 1,
        per_page: 2,
        namespace: Some("test_sql".to_string()),
        status: None,
        initiator: None,
        source_system: None,
    };

    let result = query_service.list_tasks_with_context(&query).await?;

    assert_eq!(result.tasks.len(), 2, "Should return requested page size");
    assert!(
        result.pagination.total_count >= 4,
        "Should have at least 4 total tasks"
    );
    assert!(
        result.pagination.total_pages >= 2,
        "Should have at least 2 pages"
    );

    Ok(())
}

/// Test: list_tasks_with_context includes execution context for each task
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_tasks_with_context_has_execution_context(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;

    create_test_task(&task_initializer).await?;
    create_test_task(&task_initializer).await?;

    let query = TaskListQuery {
        page: 1,
        per_page: 10,
        namespace: Some("test_sql".to_string()),
        status: None,
        initiator: None,
        source_system: None,
    };

    let result = query_service.list_tasks_with_context(&query).await?;

    for twc in &result.tasks {
        assert!(
            twc.execution_context.total_steps > 0,
            "Each task should have execution context"
        );
        assert!(!twc.task_name.is_empty(), "Each task should have a name");
        assert!(
            !twc.namespace.is_empty(),
            "Each task should have a namespace"
        );
    }

    Ok(())
}

/// Test: list_tasks_with_context returns empty for non-matching namespace
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_tasks_with_context_empty_namespace(pool: PgPool) -> Result<()> {
    let (query_service, _) = setup_services(pool).await?;

    let query = TaskListQuery {
        page: 1,
        per_page: 10,
        namespace: Some("nonexistent_namespace".to_string()),
        status: None,
        initiator: None,
        source_system: None,
    };

    let result = query_service.list_tasks_with_context(&query).await?;

    assert!(
        result.tasks.is_empty(),
        "Should return empty for non-matching namespace"
    );
    assert_eq!(result.pagination.total_count, 0);

    Ok(())
}

// =============================================================================
// to_task_response conversion tests
// =============================================================================

/// Test: to_task_response converts all fields correctly
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_to_task_response_conversion(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let twc = query_service.get_task_with_context(task_uuid).await?;
    let response = TaskQueryService::to_task_response(&twc);

    // Verify identity fields
    assert_eq!(response.task_uuid, task_uuid.to_string());
    assert_eq!(response.name, "boundary_max_attempts_one");
    assert_eq!(response.namespace, "test_sql");
    assert_eq!(response.version, "1.0.0");

    // Verify execution context fields are populated
    assert!(response.total_steps > 0);
    assert!(!response.execution_status.is_empty());
    assert!(!response.health_status.is_empty());

    // Verify metadata fields
    assert_eq!(response.initiator, "query_service_test");
    assert_eq!(response.source_system, "integration_test");
    assert_eq!(response.priority, Some(5));

    // Verify tags conversion
    assert!(response.tags.is_some());
    let tags = response.tags.unwrap();
    assert!(tags.contains(&"test".to_string()));
    assert!(tags.contains(&"query_service".to_string()));

    Ok(())
}

/// Test: to_task_response includes step readiness data
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_to_task_response_includes_steps(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let twc = query_service.get_task_with_context(task_uuid).await?;
    let response = TaskQueryService::to_task_response(&twc);

    assert!(
        !response.steps.is_empty(),
        "Response should include step readiness data"
    );

    Ok(())
}

/// Test: to_task_response handles completion percentage conversion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_to_task_response_completion_percentage(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let twc = query_service.get_task_with_context(task_uuid).await?;
    let response = TaskQueryService::to_task_response(&twc);

    // Completion percentage should be a valid f64 between 0 and 100
    assert!(
        response.completion_percentage >= 0.0,
        "Completion percentage should be >= 0"
    );
    assert!(
        response.completion_percentage <= 100.0,
        "Completion percentage should be <= 100"
    );

    Ok(())
}

// =============================================================================
// TaskWithContext Clone/Debug tests
// =============================================================================

/// Test: TaskWithContext supports Clone and Debug
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_with_context_clone_and_debug(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let twc = query_service.get_task_with_context(task_uuid).await?;

    // Test Clone
    let cloned = twc.clone();
    assert_eq!(cloned.task.task_uuid, twc.task.task_uuid);
    assert_eq!(cloned.task_name, twc.task_name);

    // Test Debug
    let debug_str = format!("{:?}", twc);
    assert!(debug_str.contains("TaskWithContext"));

    Ok(())
}

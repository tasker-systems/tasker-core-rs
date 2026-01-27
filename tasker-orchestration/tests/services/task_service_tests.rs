//! # TaskService Tests (TAS-76)
//!
//! Integration tests for TaskService validating:
//! - Task creation with templates
//! - Task retrieval with execution context
//! - Task listing with pagination
//! - Task cancellation
//! - Error classification (client vs server errors)

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use tasker_orchestration::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use tasker_orchestration::orchestration::lifecycle::task_initialization::TaskInitializer;
use tasker_orchestration::services::TaskService;
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

/// Helper to set up TaskService with real infrastructure
async fn setup_task_service(pool: PgPool) -> Result<TaskService> {
    // Register templates from workspace root fixtures
    let registry = TaskHandlerRegistry::new(pool.clone());
    registry
        .discover_and_register_templates(&fixture_path())
        .await?;

    // Create system context
    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

    // Create step enqueuer and task initializer
    let step_enqueuer = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let task_initializer = Arc::new(TaskInitializer::new(system_context.clone(), step_enqueuer));

    // Create TaskService with SystemContext (not OrchestrationCore)
    let task_service = TaskService::new(
        pool.clone(), // read pool
        pool.clone(), // write pool
        task_initializer,
        system_context,
    );

    Ok(task_service)
}

/// Helper to create a task request for a template
fn create_task_request(template_name: &str, namespace: &str) -> TaskRequest {
    TaskRequest {
        name: template_name.to_string(),
        namespace: namespace.to_string(),
        version: "1.0.0".to_string(),
        context: serde_json::json!({
            "_test_run_id": Uuid::now_v7().to_string(),
            "test_value": 42
        }),
        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
        initiator: "service_test".to_string(),
        source_system: "integration_test".to_string(),
        reason: "Testing TaskService".to_string(),
        tags: vec!["test".to_string()],
        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
        idempotency_key: None,
    }
}

/// Test: TaskService.create_task() successfully creates a task
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_create_task_success(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.create_task() success");

    let task_service = setup_task_service(pool).await?;

    let request = create_task_request("boundary_max_attempts_one", "test_sql");

    let response = task_service.create_task(request).await?;

    assert!(!response.task_uuid.is_empty(), "Task UUID should be set");
    // TaskService.create_task now returns full TaskResponse (same shape as GET)
    assert!(!response.status.is_empty(), "Status should be set");
    assert!(response.total_steps > 0, "Should have at least one step");

    tracing::info!(
        task_uuid = %response.task_uuid,
        total_steps = response.total_steps,
        status = %response.status,
        "✅ Task created successfully via TaskService"
    );

    Ok(())
}

/// Test: TaskService.create_task() validates empty name
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_create_task_validates_empty_name(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.create_task() validates empty name");

    let task_service = setup_task_service(pool).await?;

    let mut request = create_task_request("boundary_max_attempts_one", "test_sql");
    request.name = "".to_string();

    let result = task_service.create_task(request).await;

    assert!(result.is_err(), "Should reject empty name");
    let err = result.unwrap_err();
    assert!(
        err.is_client_error(),
        "Should be classified as client error"
    );

    tracing::info!("✅ Correctly rejected empty task name");
    Ok(())
}

/// Test: TaskService.create_task() returns TemplateNotFound for unknown template
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_create_task_template_not_found(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.create_task() template not found");

    let task_service = setup_task_service(pool).await?;

    let request = create_task_request("nonexistent_template", "test_sql");

    let result = task_service.create_task(request).await;

    assert!(result.is_err(), "Should fail for unknown template");
    let err = result.unwrap_err();
    assert!(
        err.is_client_error(),
        "Template not found should be client error"
    );

    tracing::info!("✅ Correctly returned error for unknown template");
    Ok(())
}

/// Test: TaskService.get_task() returns task with execution context
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_task_with_context(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.get_task() with execution context");

    let task_service = setup_task_service(pool).await?;

    // Create a task first
    let request = create_task_request("boundary_max_attempts_one", "test_sql");
    let create_response = task_service.create_task(request).await?;
    let task_uuid = Uuid::parse_str(&create_response.task_uuid)?;

    // Retrieve the task
    let task_response = task_service.get_task(task_uuid).await?;

    assert_eq!(task_response.task_uuid, create_response.task_uuid);
    assert_eq!(task_response.name, "boundary_max_attempts_one");
    assert_eq!(task_response.namespace, "test_sql");
    assert!(
        task_response.total_steps > 0,
        "Should have execution context"
    );

    tracing::info!(
        task_uuid = %task_response.task_uuid,
        total_steps = task_response.total_steps,
        "✅ Task retrieved with execution context"
    );

    Ok(())
}

/// Test: TaskService.get_task() returns NotFound for unknown UUID
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_task_not_found(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.get_task() not found");

    let task_service = setup_task_service(pool).await?;

    let unknown_uuid = Uuid::now_v7();
    let result = task_service.get_task(unknown_uuid).await;

    assert!(result.is_err(), "Should return error for unknown UUID");

    tracing::info!("✅ Correctly returned NotFound for unknown UUID");
    Ok(())
}

/// Test: TaskService.list_tasks() returns paginated results
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_tasks_pagination(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.list_tasks() pagination");

    let task_service = setup_task_service(pool).await?;

    // Create multiple tasks
    for i in 0..5 {
        let mut request = create_task_request("boundary_max_attempts_one", "test_sql");
        request.context = serde_json::json!({
            "_test_run_id": Uuid::now_v7().to_string(),
            "iteration": i
        });
        task_service.create_task(request).await?;
    }

    // List with pagination
    let query = TaskListQuery {
        page: 1,
        per_page: 3,
        namespace: Some("test_sql".to_string()),
        status: None,
        initiator: None,
        source_system: None,
    };

    let response = task_service.list_tasks(query).await?;

    assert_eq!(response.tasks.len(), 3, "Should return 3 tasks per page");
    assert!(
        response.pagination.total_count >= 5,
        "Should have at least 5 total"
    );

    tracing::info!(
        page_count = response.tasks.len(),
        total = response.pagination.total_count,
        "✅ Pagination working correctly"
    );

    Ok(())
}

/// Test: TaskService.list_tasks() validates pagination parameters
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_tasks_validates_pagination(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.list_tasks() validates pagination");

    let task_service = setup_task_service(pool).await?;

    // Test invalid per_page
    let query = TaskListQuery {
        page: 1,
        per_page: 0, // Invalid
        namespace: None,
        status: None,
        initiator: None,
        source_system: None,
    };

    let result = task_service.list_tasks(query).await;
    assert!(result.is_err(), "Should reject per_page=0");

    // Test per_page > 100
    let query = TaskListQuery {
        page: 1,
        per_page: 101, // Invalid
        namespace: None,
        status: None,
        initiator: None,
        source_system: None,
    };

    let result = task_service.list_tasks(query).await;
    assert!(result.is_err(), "Should reject per_page > 100");

    // Test page=0
    let query = TaskListQuery {
        page: 0, // Invalid
        per_page: 10,
        namespace: None,
        status: None,
        initiator: None,
        source_system: None,
    };

    let result = task_service.list_tasks(query).await;
    assert!(result.is_err(), "Should reject page=0");

    tracing::info!("✅ Pagination validation working correctly");
    Ok(())
}

/// Test: TaskService.cancel_task() cancels a pending task
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_sql_task_success(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.cancel_task() success");

    let task_service = setup_task_service(pool).await?;

    // Create a task
    let request = create_task_request("boundary_max_attempts_one", "test_sql");
    let create_response = task_service.create_task(request).await?;
    let task_uuid = Uuid::parse_str(&create_response.task_uuid)?;

    // Cancel it
    let cancel_response = task_service.cancel_task(task_uuid).await?;

    assert_eq!(cancel_response.task_uuid, create_response.task_uuid);

    tracing::info!(
        task_uuid = %cancel_response.task_uuid,
        status = %cancel_response.status,
        "✅ Task cancelled successfully"
    );

    Ok(())
}

/// Test: TaskService.cancel_task() returns NotFound for unknown UUID
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_sql_task_not_found(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: TaskService.cancel_task() not found");

    let task_service = setup_task_service(pool).await?;

    let unknown_uuid = Uuid::now_v7();
    let result = task_service.cancel_task(unknown_uuid).await;

    assert!(result.is_err(), "Should return error for unknown UUID");

    tracing::info!("✅ Correctly returned NotFound for unknown UUID");
    Ok(())
}

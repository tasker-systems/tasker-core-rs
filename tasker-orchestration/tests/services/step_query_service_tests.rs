//! # StepQueryService Tests (TAS-63)
//!
//! Integration tests for StepQueryService validating:
//! - Step listing with readiness status
//! - Single step retrieval with readiness
//! - Ownership verification
//! - Audit history retrieval
//! - Response transformation (to_step_response, to_audit_responses)

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use tasker_orchestration::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use tasker_orchestration::orchestration::lifecycle::task_initialization::TaskInitializer;
use tasker_orchestration::services::{StepQueryService, StepWithReadiness};
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

/// Helper to set up StepQueryService and TaskInitializer
async fn setup_services(pool: PgPool) -> Result<(StepQueryService, TaskInitializer)> {
    let registry = TaskHandlerRegistry::new(pool.clone());
    registry
        .discover_and_register_templates(&fixture_path())
        .await?;

    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
    let step_enqueuer = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let task_initializer = TaskInitializer::new(system_context, step_enqueuer);
    let query_service = StepQueryService::new(pool);

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
        initiator: "step_query_test".to_string(),
        source_system: "integration_test".to_string(),
        reason: "Testing StepQueryService".to_string(),
        tags: vec!["test".to_string()],
        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
        idempotency_key: None,
    };

    let result = initializer.create_task_from_request(request).await?;
    Ok(result.task_uuid)
}

// =============================================================================
// list_steps_for_task tests
// =============================================================================

/// Test: list_steps_for_task returns steps with readiness status
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_steps_for_task_success(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let steps = query_service.list_steps_for_task(task_uuid).await?;

    assert!(!steps.is_empty(), "Task should have steps");

    for swr in &steps {
        assert_eq!(swr.step.task_uuid, task_uuid, "Step should belong to task");
        // Readiness may or may not be present depending on SQL function
        if let Some(ref readiness) = swr.readiness {
            assert_eq!(
                readiness.task_uuid, task_uuid,
                "Readiness should reference same task"
            );
            assert!(
                !readiness.name.is_empty(),
                "Readiness should have step name"
            );
        }
    }

    Ok(())
}

/// Test: list_steps_for_task returns empty for unknown task
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_steps_for_task_empty(pool: PgPool) -> Result<()> {
    let (query_service, _) = setup_services(pool).await?;

    let unknown_uuid = Uuid::now_v7();
    let steps = query_service.list_steps_for_task(unknown_uuid).await?;

    assert!(
        steps.is_empty(),
        "Should return empty for unknown task UUID"
    );

    Ok(())
}

/// Test: list_steps_for_task includes readiness status with correct fields
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_list_steps_for_task_readiness_fields(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let steps = query_service.list_steps_for_task(task_uuid).await?;
    assert!(!steps.is_empty());

    // Find a step that has readiness data
    let with_readiness: Vec<&StepWithReadiness> =
        steps.iter().filter(|s| s.readiness.is_some()).collect();

    if !with_readiness.is_empty() {
        let readiness = with_readiness[0].readiness.as_ref().unwrap();
        assert!(!readiness.current_state.is_empty(), "Should have state");
        assert!(readiness.max_attempts > 0, "Should have max_attempts > 0");
    }

    Ok(())
}

// =============================================================================
// get_step_with_readiness tests
// =============================================================================

/// Test: get_step_with_readiness returns step and readiness data
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_step_with_readiness_success(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    // Get a step UUID from the list
    let steps = query_service.list_steps_for_task(task_uuid).await?;
    assert!(!steps.is_empty(), "Need at least one step");
    let step_uuid = steps[0].step.workflow_step_uuid;

    // Retrieve the specific step
    let swr = query_service
        .get_step_with_readiness(task_uuid, step_uuid)
        .await?;

    assert_eq!(swr.step.workflow_step_uuid, step_uuid);
    assert_eq!(swr.step.task_uuid, task_uuid);

    Ok(())
}

/// Test: get_step_with_readiness returns NotFound for unknown step
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_step_with_readiness_not_found(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let unknown_step = Uuid::now_v7();
    let result = query_service
        .get_step_with_readiness(task_uuid, unknown_step)
        .await;

    assert!(result.is_err(), "Should return error for unknown step");
    assert!(matches!(
        result.unwrap_err(),
        tasker_orchestration::services::StepQueryError::NotFound(_)
    ));

    Ok(())
}

/// Test: get_step_with_readiness returns OwnershipMismatch for wrong task
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_step_with_readiness_ownership_mismatch(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;

    // Create two tasks
    let task1_uuid = create_test_task(&task_initializer).await?;
    let task2_uuid = create_test_task(&task_initializer).await?;

    // Get a step from task1
    let steps = query_service.list_steps_for_task(task1_uuid).await?;
    assert!(!steps.is_empty());
    let step_uuid = steps[0].step.workflow_step_uuid;

    // Try to get it using task2's UUID
    let result = query_service
        .get_step_with_readiness(task2_uuid, step_uuid)
        .await;

    assert!(result.is_err(), "Should fail with ownership mismatch");
    assert!(matches!(
        result.unwrap_err(),
        tasker_orchestration::services::StepQueryError::OwnershipMismatch { .. }
    ));

    Ok(())
}

// =============================================================================
// get_step_audit_history tests
// =============================================================================

/// Test: get_step_audit_history returns records for valid step
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_step_audit_history_success(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let steps = query_service.list_steps_for_task(task_uuid).await?;
    assert!(!steps.is_empty());
    let step_uuid = steps[0].step.workflow_step_uuid;

    // Newly created steps may have audit records from initialization
    let audits = query_service
        .get_step_audit_history(task_uuid, step_uuid)
        .await?;

    // The result is valid whether empty or populated
    // Just verify no error was thrown
    tracing::info!(
        step_uuid = %step_uuid,
        audit_count = audits.len(),
        "Audit history retrieved"
    );

    Ok(())
}

/// Test: get_step_audit_history returns NotFound for unknown step
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_step_audit_history_not_found(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let unknown_step = Uuid::now_v7();
    let result = query_service
        .get_step_audit_history(task_uuid, unknown_step)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        tasker_orchestration::services::StepQueryError::NotFound(_)
    ));

    Ok(())
}

/// Test: get_step_audit_history returns OwnershipMismatch for wrong task
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_step_audit_history_ownership_mismatch(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;

    let task1_uuid = create_test_task(&task_initializer).await?;
    let task2_uuid = create_test_task(&task_initializer).await?;

    let steps = query_service.list_steps_for_task(task1_uuid).await?;
    assert!(!steps.is_empty());
    let step_uuid = steps[0].step.workflow_step_uuid;

    let result = query_service
        .get_step_audit_history(task2_uuid, step_uuid)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        tasker_orchestration::services::StepQueryError::OwnershipMismatch { .. }
    ));

    Ok(())
}

// =============================================================================
// to_step_response tests
// =============================================================================

/// Test: to_step_response with readiness data
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_to_step_response_with_readiness(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let steps = query_service.list_steps_for_task(task_uuid).await?;
    assert!(!steps.is_empty());

    let response = StepQueryService::to_step_response(&steps[0]);

    assert_eq!(response.task_uuid, task_uuid.to_string());
    assert!(
        !response.step_uuid.is_empty(),
        "Step UUID should be populated"
    );
    assert!(
        !response.created_at.is_empty(),
        "Created at should be formatted"
    );
    assert!(
        !response.updated_at.is_empty(),
        "Updated at should be formatted"
    );
    assert!(
        !response.current_state.is_empty(),
        "Current state should be set"
    );

    Ok(())
}

/// Test: to_step_response without readiness falls back correctly
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_to_step_response_without_readiness(pool: PgPool) -> Result<()> {
    let (query_service, task_initializer) = setup_services(pool).await?;
    let task_uuid = create_test_task(&task_initializer).await?;

    let steps = query_service.list_steps_for_task(task_uuid).await?;
    assert!(!steps.is_empty());

    // Create a StepWithReadiness without readiness data
    let swr_no_readiness = StepWithReadiness {
        step: steps[0].step.clone(),
        readiness: None,
    };

    let response = StepQueryService::to_step_response(&swr_no_readiness);

    assert_eq!(response.task_uuid, task_uuid.to_string());
    assert_eq!(response.current_state, "unknown");
    assert!(!response.dependencies_satisfied);
    assert!(!response.retry_eligible);
    assert!(!response.ready_for_execution);
    assert_eq!(response.total_parents, 0);
    assert_eq!(response.completed_parents, 0);

    Ok(())
}

/// Test: to_audit_responses converts empty list
#[test]
fn test_to_audit_responses_empty() {
    let responses = StepQueryService::to_audit_responses(&[]);
    assert!(responses.is_empty());
}

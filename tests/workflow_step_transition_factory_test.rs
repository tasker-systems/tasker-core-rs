//! Tests for WorkflowStepTransitionFactory
//!
//! Verifies that the factory creates proper state transitions following Rails patterns

mod factories;

use factories::base::SqlxFactory;
use factories::core::WorkflowStepFactory;
use factories::states::WorkflowStepTransitionFactory;
use serde_json::json;
use sqlx::PgPool;
use std::f64::consts::PI;
use tasker_core::models::WorkflowStepTransition;

#[sqlx::test]
async fn test_basic_transition_creation(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Create a workflow step first
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("test_step")
        .create(&pool)
        .await?;

    // Create a simple transition
    let transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .to_state("complete")
        .create(&pool)
        .await?;

    assert_eq!(
        transition.workflow_step_uuid,
        workflow_step.workflow_step_uuid
    );
    assert_eq!(transition.to_state, "complete");
    assert!(transition.most_recent);
    assert!(transition.metadata.is_some());

    // Verify metadata contains factory information
    let metadata = transition.metadata.unwrap();
    assert!(metadata.get("factory_created").unwrap().as_bool().unwrap());

    Ok(())
}

#[sqlx::test]
async fn test_transition_with_error_metadata(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("failing_step")
        .create(&pool)
        .await?;

    let error_transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .with_error("Network timeout")
        .create(&pool)
        .await?;

    assert_eq!(error_transition.to_state, "error");

    let metadata = error_transition.metadata.unwrap();
    assert_eq!(
        metadata.get("error_message").unwrap().as_str().unwrap(),
        "Network timeout"
    );

    Ok(())
}

#[sqlx::test]
async fn test_transition_with_execution_duration(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("timed_step")
        .create(&pool)
        .await?;

    let complete_transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .to_complete_with_duration(PI)
        .create(&pool)
        .await?;

    assert_eq!(complete_transition.to_state, "complete");

    let metadata = complete_transition.metadata.unwrap();
    assert_eq!(
        metadata
            .get("execution_duration")
            .unwrap()
            .as_f64()
            .unwrap(),
        PI
    );

    Ok(())
}

#[sqlx::test]
async fn test_retry_transition(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("retry_step")
        .create(&pool)
        .await?;

    let retry_transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .with_retry_attempt(2)
        .create(&pool)
        .await?;

    assert_eq!(retry_transition.from_state, Some("error".to_string()));
    assert_eq!(retry_transition.to_state, "pending");

    let metadata = retry_transition.metadata.unwrap();
    assert!(metadata.get("retry_attempt").unwrap().as_bool().unwrap());
    assert_eq!(metadata.get("attempt_number").unwrap().as_i64().unwrap(), 2);

    Ok(())
}

#[sqlx::test]
async fn test_manual_resolution_transition(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("manual_step")
        .create(&pool)
        .await?;

    let resolved_transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .resolved_by("admin@company.com")
        .create(&pool)
        .await?;

    assert_eq!(resolved_transition.to_state, "resolved_manually");

    let metadata = resolved_transition.metadata.unwrap();
    assert_eq!(
        metadata.get("resolved_by").unwrap().as_str().unwrap(),
        "admin@company.com"
    );

    Ok(())
}

#[sqlx::test]
async fn test_transition_most_recent_flag(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("sequenced_step")
        .create(&pool)
        .await?;

    // Create first transition
    let first_transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .to_state("pending")
        .create(&pool)
        .await?;

    assert!(first_transition.most_recent);

    // Create second transition
    let second_transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .to_in_progress()
        .create(&pool)
        .await?;

    assert!(second_transition.most_recent);

    // Verify first transition is no longer most recent
    let updated_first = WorkflowStepTransition::find_by_uuid(&pool, first_transition.workflow_step_transition_uuid)
        .await?
        .unwrap();
    assert!(!updated_first.most_recent);

    Ok(())
}

#[sqlx::test]
async fn test_complete_lifecycle_factory(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("lifecycle_step")
        .create(&pool)
        .await?;

    let transitions = WorkflowStepTransitionFactory::create_complete_lifecycle(
        workflow_step.workflow_step_uuid,
        &pool,
    )
    .await?;

    assert_eq!(transitions.len(), 3);

    // Verify sequence: pending -> in_progress -> complete
    assert_eq!(transitions[0].to_state, "pending");
    assert_eq!(transitions[1].to_state, "in_progress");
    assert_eq!(transitions[2].to_state, "complete");

    // Refetch from database to get updated most_recent flags
    let all_transitions =
        WorkflowStepTransition::list_by_workflow_step(&pool, workflow_step.workflow_step_uuid)
            .await?;

    // Only the last should be most recent
    assert!(!all_transitions[0].most_recent);
    assert!(!all_transitions[1].most_recent);
    assert!(all_transitions[2].most_recent);

    // Verify completion metadata has execution duration
    let complete_metadata = transitions[2].metadata.as_ref().unwrap();
    assert!(complete_metadata.get("execution_duration").is_some());

    Ok(())
}

#[sqlx::test]
async fn test_failed_lifecycle_factory(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("failed_step")
        .create(&pool)
        .await?;

    let transitions = WorkflowStepTransitionFactory::create_failed_lifecycle(
        workflow_step.workflow_step_uuid,
        "Database connection failed",
        &pool,
    )
    .await?;

    assert_eq!(transitions.len(), 3);

    // Verify sequence: pending -> in_progress -> error
    assert_eq!(transitions[0].to_state, "pending");
    assert_eq!(transitions[1].to_state, "in_progress");
    assert_eq!(transitions[2].to_state, "error");

    // Verify error metadata
    let error_metadata = transitions[2].metadata.as_ref().unwrap();
    assert_eq!(
        error_metadata
            .get("error_message")
            .unwrap()
            .as_str()
            .unwrap(),
        "Database connection failed"
    );

    Ok(())
}

#[sqlx::test]
async fn test_retry_lifecycle_factory(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("retry_step")
        .create(&pool)
        .await?;

    let transitions = WorkflowStepTransitionFactory::create_retry_lifecycle(
        workflow_step.workflow_step_uuid,
        3, // Third attempt
        &pool,
    )
    .await?;

    assert_eq!(transitions.len(), 3);

    // Verify sequence: pending (retry) -> in_progress -> complete
    assert_eq!(transitions[0].to_state, "pending");
    assert_eq!(transitions[1].to_state, "in_progress");
    assert_eq!(transitions[2].to_state, "complete");

    // Verify retry metadata
    let retry_metadata = transitions[0].metadata.as_ref().unwrap();
    assert!(retry_metadata
        .get("retry_attempt")
        .unwrap()
        .as_bool()
        .unwrap());
    assert_eq!(
        retry_metadata
            .get("attempt_number")
            .unwrap()
            .as_i64()
            .unwrap(),
        3
    );

    Ok(())
}

#[sqlx::test]
async fn test_custom_metadata_preservation(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("custom_metadata_step")
        .create(&pool)
        .await?;

    let custom_metadata = json!({
        "user_provided": "custom_value",
        "step_type": "validation",
        "complexity_score": 8
    });

    let transition = WorkflowStepTransitionFactory::new()
        .for_workflow_step(workflow_step.workflow_step_uuid)
        .with_metadata(custom_metadata.clone())
        .to_complete()
        .create(&pool)
        .await?;

    let result_metadata = transition.metadata.unwrap();

    // Verify custom metadata is preserved
    assert_eq!(
        result_metadata
            .get("user_provided")
            .unwrap()
            .as_str()
            .unwrap(),
        "custom_value"
    );
    assert_eq!(
        result_metadata
            .get("complexity_score")
            .unwrap()
            .as_i64()
            .unwrap(),
        8
    );

    // Verify factory metadata is added
    assert!(result_metadata
        .get("factory_created")
        .unwrap()
        .as_bool()
        .unwrap());

    Ok(())
}

#[sqlx::test]
async fn test_invalid_workflow_step_uuid_error(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = WorkflowStepTransitionFactory::new()
        .to_state("complete")
        .create(&pool)
        .await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("workflow_step_uuid is required"));

    Ok(())
}

#[sqlx::test]
async fn test_database_integration_with_scopes(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_step = WorkflowStepFactory::new()
        .with_named_step("scoped_step")
        .create(&pool)
        .await?;

    // Create some transitions
    WorkflowStepTransitionFactory::create_complete_lifecycle(
        workflow_step.workflow_step_uuid,
        &pool,
    )
    .await?;

    // Use scopes from our implementation to verify transitions were created
    let current_transition =
        WorkflowStepTransition::get_current(&pool, workflow_step.workflow_step_uuid).await?;

    assert!(current_transition.is_some());
    let current = current_transition.unwrap();
    assert_eq!(current.to_state, "complete");
    assert!(current.most_recent);

    // Get all transitions for the step
    let all_transitions =
        WorkflowStepTransition::list_by_workflow_step(&pool, workflow_step.workflow_step_uuid)
            .await?;

    assert_eq!(all_transitions.len(), 3);

    Ok(())
}

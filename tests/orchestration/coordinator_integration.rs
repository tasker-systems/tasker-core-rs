//! Orchestration Coordinator Integration Tests
//!
//! Tests for the OrchestrationCoordinator with state machine integration.

use crate::factories::base::SqlxFactory;
use sqlx::PgPool;
use tasker_core::error::OrchestrationResult;
use tasker_core::events::publisher::EventPublisher;
use tasker_core::orchestration::coordinator::{
    OrchestrationCoordinator, StepExecutionDelegate, StepExecutionResult, StepExecutionStatus,
    ViableStep,
};

/// Mock delegation for testing
#[allow(dead_code)]
pub struct MockStepDelegate {
    pub framework_name: String,
    pub should_succeed: bool,
}

#[async_trait::async_trait]
impl StepExecutionDelegate for MockStepDelegate {
    async fn execute_steps(
        &self,
        _task_id: i64,
        steps: &[ViableStep],
    ) -> OrchestrationResult<Vec<StepExecutionResult>> {
        let results = steps
            .iter()
            .map(|step| StepExecutionResult {
                step_id: step.workflow_step_id,
                status: if self.should_succeed {
                    StepExecutionStatus::Success
                } else {
                    StepExecutionStatus::Error
                },
                output: if self.should_succeed {
                    Some(serde_json::json!({"step_id": step.workflow_step_id, "success": true}))
                } else {
                    None
                },
                error: if self.should_succeed {
                    None
                } else {
                    Some("Mock execution error".to_string())
                },
                execution_time_ms: 100,
            })
            .collect();

        Ok(results)
    }

    fn framework_name(&self) -> &'static str {
        // We need to return a static string, so we'll just return a constant for the mock
        "MockFramework"
    }
}

#[sqlx::test]
async fn test_orchestration_coordinator_creation(pool: PgPool) -> sqlx::Result<()> {
    // Create a simple task using the factory pattern
    let task = crate::factories::core::TaskFactory::default()
        .with_initiator("test_orchestration")
        .create(&pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;
    let event_publisher = EventPublisher::default();

    // Create orchestration coordinator
    let coordinator = OrchestrationCoordinator::new(task.clone(), pool.clone(), event_publisher);

    // Verify basic properties
    assert_eq!(coordinator.task_id(), task.task_id);

    Ok(())
}

#[sqlx::test]
async fn test_orchestration_coordinator_task_state_management(pool: PgPool) -> sqlx::Result<()> {
    // Create a task and coordinator
    let task = crate::factories::core::TaskFactory::default()
        .with_initiator("test_orchestration")
        .create(&pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;
    let event_publisher = EventPublisher::default();
    let coordinator = OrchestrationCoordinator::new(task.clone(), pool.clone(), event_publisher);

    // Check initial state - should be pending by default
    let initial_state = coordinator.current_task_state().await.unwrap();
    // The state machine returns TaskState enum, so we check for Pending
    assert_eq!(
        initial_state,
        tasker_core::state_machine::states::TaskState::Pending
    );

    Ok(())
}

#[sqlx::test]
async fn test_orchestration_with_mock_delegate(pool: PgPool) -> sqlx::Result<()> {
    // Create a task and coordinator
    let task = crate::factories::core::TaskFactory::default()
        .with_initiator("test_orchestration")
        .create(&pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;
    let event_publisher = EventPublisher::default();
    let mut coordinator =
        OrchestrationCoordinator::new(task.clone(), pool.clone(), event_publisher);

    // Create mock delegate that succeeds
    let delegate = MockStepDelegate {
        framework_name: "TestFramework".to_string(),
        should_succeed: true,
    };

    // Run orchestration - this will handle no viable steps gracefully
    let result = coordinator.orchestrate_task(&delegate).await;

    // Should succeed even with no viable steps
    assert!(
        result.is_ok(),
        "Orchestration should handle no viable steps: {:?}",
        result.err()
    );

    Ok(())
}

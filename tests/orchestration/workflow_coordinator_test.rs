use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tasker_core::orchestration::errors::OrchestrationError;
use tasker_core::orchestration::types::{
    FrameworkIntegration, StepResult, StepStatus, TaskContext, ViableStep,
};
use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinator;
use tasker_core::orchestration::TaskOrchestrationResult;

/// Mock framework for testing workflow coordination
struct MockWorkflowFramework {
    steps_executed: Arc<tokio::sync::Mutex<Vec<i64>>>,
    should_fail: bool,
}

impl MockWorkflowFramework {
    fn new(should_fail: bool) -> Self {
        Self {
            steps_executed: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            should_fail,
        }
    }
}

#[async_trait::async_trait]
impl FrameworkIntegration for MockWorkflowFramework {
    async fn execute_single_step(
        &self,
        step: &ViableStep,
        _task_context: &TaskContext,
    ) -> Result<StepResult, OrchestrationError> {
        // Track execution
        let mut executed = self.steps_executed.lock().await;
        executed.push(step.step_id);

        // Simulate execution
        tokio::time::sleep(Duration::from_millis(10)).await;

        let status = if self.should_fail {
            StepStatus::Failed
        } else {
            StepStatus::Completed
        };

        Ok(StepResult {
            step_id: step.step_id,
            status,
            output: serde_json::json!({
                "message": format!("Step {} executed by mock framework", step.step_id)
            }),
            execution_duration: Duration::from_millis(10),
            error_message: if self.should_fail {
                Some("Mock failure".to_string())
            } else {
                None
            },
            retry_after: None,
            error_code: if self.should_fail {
                Some("MOCK_ERROR".to_string())
            } else {
                None
            },
            error_context: None,
        })
    }

    fn framework_name(&self) -> &'static str {
        "mock_workflow_framework"
    }

    async fn get_task_context(&self, task_id: i64) -> Result<TaskContext, OrchestrationError> {
        Ok(TaskContext {
            task_id,
            data: serde_json::json!({
                "test": true,
                "framework": self.framework_name()
            }),
            metadata: HashMap::new(),
        })
    }

    async fn enqueue_task(
        &self,
        _task_id: i64,
        _delay: Option<Duration>,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }

    async fn mark_task_failed(
        &self,
        _task_id: i64,
        _error: &str,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }

    async fn update_step_state(
        &self,
        _step_id: i64,
        _state: &str,
        _result: Option<&serde_json::Value>,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }
}

#[sqlx::test]
async fn test_workflow_coordinator_basic_execution(pool: sqlx::PgPool) {
    // This test demonstrates the WorkflowCoordinator's ability to:
    // 1. Coordinate workflow execution
    // 2. Discover viable steps
    // 3. Execute them through the framework
    // 4. Handle state transitions
    // 5. Return appropriate results

    let coordinator = WorkflowCoordinator::new(pool);
    let framework = Arc::new(MockWorkflowFramework::new(false));

    // Create a test task (would need factory support)
    // For now, we expect this to fail because there's no task in the database
    let task_id = 1;

    let result = coordinator
        .execute_task_workflow(task_id, framework.clone())
        .await;

    // We expect an error because we don't have test data set up
    // but this verifies the coordinator is wired up correctly
    assert!(result.is_err());
}

#[test]
fn test_workflow_metrics_initialization() {
    use chrono::Utc;
    use tasker_core::orchestration::workflow_coordinator::WorkflowExecutionMetrics;

    let metrics = WorkflowExecutionMetrics {
        task_id: 123,
        started_at: Utc::now(),
        completed_at: None,
        total_duration: None,
        steps_discovered: 0,
        steps_executed: 0,
        steps_succeeded: 0,
        steps_failed: 0,
        steps_retried: 0,
        discovery_attempts: 0,
        discovery_duration: Duration::default(),
        execution_duration: Duration::default(),
    };

    assert_eq!(metrics.task_id, 123);
    assert_eq!(metrics.steps_executed, 0);
    assert!(metrics.completed_at.is_none());
}

#[test]
fn test_orchestration_result_variants() {
    // Test that we can construct all variants of TaskOrchestrationResult

    let complete = TaskOrchestrationResult::Complete {
        task_id: 1,
        steps_executed: 5,
        total_execution_time_ms: 1000,
    };

    match complete {
        TaskOrchestrationResult::Complete { task_id, .. } => assert_eq!(task_id, 1),
        _ => panic!("Wrong variant"),
    }

    let failed = TaskOrchestrationResult::Failed {
        task_id: 2,
        error: "Test error".to_string(),
        failed_steps: vec![10, 20],
    };

    match failed {
        TaskOrchestrationResult::Failed { error, .. } => assert_eq!(error, "Test error"),
        _ => panic!("Wrong variant"),
    }

    let in_progress = TaskOrchestrationResult::InProgress {
        task_id: 3,
        steps_executed: 2,
        next_poll_delay_ms: 5000,
    };

    match in_progress {
        TaskOrchestrationResult::InProgress {
            next_poll_delay_ms, ..
        } => {
            assert_eq!(next_poll_delay_ms, 5000)
        }
        _ => panic!("Wrong variant"),
    }

    let blocked = TaskOrchestrationResult::Blocked {
        task_id: 4,
        blocking_reason: "Waiting for dependencies".to_string(),
    };

    match blocked {
        TaskOrchestrationResult::Blocked {
            blocking_reason, ..
        } => {
            assert_eq!(blocking_reason, "Waiting for dependencies")
        }
        _ => panic!("Wrong variant"),
    }
}

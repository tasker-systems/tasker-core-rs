//! Mock Framework Implementation for Testing
//!
//! Provides a mock implementation of the FrameworkIntegration trait
//! for testing the orchestration core without requiring a real framework.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tasker_shared::orchestration::{
    errors::OrchestrationError,
    types::{
        FrameworkIntegration, OrchestrationEvent, StepResult, StepStatus, TaskContext, TaskResult,
        ViableStep,
    },
};

/// Mock framework state for tracking calls and simulating behavior
#[derive(Debug, Default, Clone)]
pub struct MockFrameworkState {
    /// Track executed steps
    pub executed_steps: Vec<(i64, String)>,
    /// Track published events
    pub published_events: Vec<OrchestrationEvent>,
    /// Track enqueued tasks
    pub enqueued_tasks: Vec<(i64, Option<Duration>)>,
    /// Track failed tasks
    pub failed_tasks: Vec<(i64, String)>,
    /// Track step state updates
    pub step_state_updates: Vec<(i64, String, Option<serde_json::Value>)>,
    /// Simulated task contexts
    pub task_contexts: HashMap<i64, TaskContext>,
    /// Simulated step results
    pub step_results: HashMap<i64, StepResult>,
    /// Health check status
    pub is_healthy: bool,
    /// Framework configuration
    pub config: HashMap<String, serde_json::Value>,
}

/// Mock framework implementation for testing
pub struct MockFramework {
    name: String,
    state: Arc<Mutex<MockFrameworkState>>,
    /// Simulate step execution delay
    execution_delay: Option<Duration>,
    /// Simulate random failures
    failure_rate: f32,
}

impl MockFramework {
    /// Create a new mock framework
    pub fn new(name: impl Into<String>) -> Self {
        let state = MockFrameworkState {
            is_healthy: true,
            config: HashMap::from([
                ("max_retries".to_string(), json!(3)),
                ("timeout_seconds".to_string(), json!(300)),
            ]),
            ..Default::default()
        };

        Self {
            name: name.into(),
            state: Arc::new(Mutex::new(state)),
            execution_delay: None,
            failure_rate: 0.0,
        }
    }

    /// Set execution delay for simulating slow operations
    pub fn with_execution_delay(mut self, delay: Duration) -> Self {
        self.execution_delay = Some(delay);
        self
    }

    /// Set failure rate for simulating random failures (0.0-1.0)
    pub fn with_failure_rate(mut self, rate: f32) -> Self {
        self.failure_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Add a task context for testing
    pub fn add_task_context(&self, task_uuid: Uuid, data: serde_json::Value) {
        let mut state = self.state.lock().unwrap();
        state.task_contexts.insert(
            task_uuid,
            TaskContext {
                task_uuid,
                data,
                metadata: HashMap::new(),
            },
        );
    }

    /// Configure a specific step result
    #[expect(dead_code, reason = "Test helper for configuring mock step results in tests")]
    pub fn configure_step_result(&self, step_uuid: Uuid, result: StepResult) {
        let mut state = self.state.lock().unwrap();
        state.step_results.insert(step_uuid, result);
    }

    /// Get the current state for assertions
    pub fn get_state(&self) -> MockFrameworkState {
        self.state.lock().unwrap().clone()
    }

    /// Reset the framework state
    #[expect(dead_code, reason = "Test helper for resetting mock framework state between tests")]
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.executed_steps.clear();
        state.published_events.clear();
        state.enqueued_tasks.clear();
        state.failed_tasks.clear();
        state.step_state_updates.clear();
        // Keep task contexts and step results for reuse
    }

    /// Set health status
    pub fn set_healthy(&self, healthy: bool) {
        let mut state = self.state.lock().unwrap();
        state.is_healthy = healthy;
    }
}

#[async_trait]
impl FrameworkIntegration for MockFramework {
    fn framework_name(&self) -> &'static str {
        // Note: This leaks memory but is fine for tests
        Box::leak(self.name.clone().into_boxed_str())
    }

    async fn get_task_context(&self, task_uuid: Uuid) -> Result<TaskContext, OrchestrationError> {
        let state = self.state.lock().unwrap();

        state.task_contexts.get(&task_uuid).cloned().ok_or_else(|| {
            OrchestrationError::TaskExecutionFailed {
                task_uuid,
                reason: "Task context not found".to_string(),
                error_code: Some("TASK_NOT_FOUND".to_string()),
            }
        })
    }

    async fn enqueue_task(
        &self,
        task_uuid: Uuid,
        delay: Option<Duration>,
    ) -> Result<(), OrchestrationError> {
        let mut state = self.state.lock().unwrap();
        state.enqueued_tasks.push((task_uuid, delay));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_framework_basic_execution() {
        let framework = MockFramework::new("test_framework");

        // Add a task context
        framework.add_task_context(
            123,
            json!({
                "order_id": "ORD-456",
                "amount": 99.99
            }),
        );

        // Create a viable step
        let step = ViableStep {
            step_uuid: 1,
            task_uuid: 123,
            name: "process_payment".to_string(),
            named_step_uuid: 100,
            current_state: "pending".to_string(),
            dependencies_satisfied: true,
            retry_eligible: true,
            attempts: 0,
            max_attempts: 3,
            last_failure_at: None,
            next_retry_at: None,
        };

        // Get task context
        let context = framework.get_task_context(123).await.unwrap();

        // Execute the step
        let result = framework
            .execute_single_step(&step, &context)
            .await
            .unwrap();

        // Verify the result
        assert!(result.is_success());
        assert_eq!(result.step_uuid, 1);

        // Check tracked state
        let state = framework.get_state();
        assert_eq!(state.executed_steps.len(), 1);
        assert_eq!(state.executed_steps[0], (1, "process_payment".to_string()));
    }

    #[tokio::test]
    async fn test_mock_framework_with_failure() {
        let framework = MockFramework::new("test_framework").with_failure_rate(1.0); // Always fail

        framework.add_task_context(123, json!({}));

        let step = ViableStep {
            step_uuid: 1,
            task_uuid: 123,
            name: "failing_step".to_string(),
            named_step_uuid: 100,
            current_state: "pending".to_string(),
            dependencies_satisfied: true,
            retry_eligible: true,
            attempts: 0,
            max_attempts: 3,
            last_failure_at: None,
            next_retry_at: None,
        };

        let context = framework.get_task_context(123).await.unwrap();
        let result = framework
            .execute_single_step(&step, &context)
            .await
            .unwrap();

        assert!(result.is_failure());
        assert_eq!(result.error_code, Some("MOCK_RANDOM_FAILURE".to_string()));
    }
}

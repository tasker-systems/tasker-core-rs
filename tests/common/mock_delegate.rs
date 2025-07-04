use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Mock delegate for testing delegation patterns
/// This simulates how the Rails framework would receive and process delegated tasks
#[derive(Debug, Clone)]
pub struct MockDelegate {
    /// Track step executions for testing
    pub executed_steps: Vec<MockStepExecution>,
    /// Mock results to return for step executions
    pub mock_results: HashMap<String, MockStepResult>,
}

#[derive(Debug, Clone)]
pub struct MockStepExecution {
    pub step_id: Uuid,
    pub handler_class: String,
    pub context: Value,
    pub execution_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct MockStepResult {
    pub success: bool,
    pub result_context: Option<Value>,
    pub error_message: Option<String>,
    pub execution_duration_ms: u64,
}

impl MockDelegate {
    pub fn new() -> Self {
        Self {
            executed_steps: Vec::new(),
            mock_results: HashMap::new(),
        }
    }

    /// Configure a mock result for a specific handler class
    pub fn mock_step_result(&mut self, handler_class: &str, result: MockStepResult) {
        self.mock_results.insert(handler_class.to_string(), result);
    }

    /// Simulate step execution (what Rails would do when receiving delegation)
    pub fn execute_step(
        &mut self,
        step_id: Uuid,
        handler_class: &str,
        context: Value,
    ) -> MockStepResult {
        // Record the execution
        self.executed_steps.push(MockStepExecution {
            step_id,
            handler_class: handler_class.to_string(),
            context: context.clone(),
            execution_time: chrono::Utc::now(),
        });

        // Return configured result or default success
        self.mock_results
            .get(handler_class)
            .cloned()
            .unwrap_or(MockStepResult {
                success: true,
                result_context: Some(serde_json::json!({"executed": true})),
                error_message: None,
                execution_duration_ms: 100,
            })
    }

    /// Check if a specific step was executed
    pub fn was_step_executed(&self, step_id: Uuid) -> bool {
        self.executed_steps
            .iter()
            .any(|exec| exec.step_id == step_id)
    }

    /// Get the number of times a handler class was executed
    pub fn execution_count_for_handler(&self, handler_class: &str) -> usize {
        self.executed_steps
            .iter()
            .filter(|exec| exec.handler_class == handler_class)
            .count()
    }

    /// Get all executions for a specific handler class
    pub fn executions_for_handler(&self, handler_class: &str) -> Vec<&MockStepExecution> {
        self.executed_steps
            .iter()
            .filter(|exec| exec.handler_class == handler_class)
            .collect()
    }

    /// Clear execution history
    pub fn clear_history(&mut self) {
        self.executed_steps.clear();
    }

    /// Create a successful mock result
    pub fn success_result(context: Option<Value>) -> MockStepResult {
        MockStepResult {
            success: true,
            result_context: context,
            error_message: None,
            execution_duration_ms: 150,
        }
    }

    /// Create a failed mock result
    pub fn error_result(error_message: &str) -> MockStepResult {
        MockStepResult {
            success: false,
            result_context: None,
            error_message: Some(error_message.to_string()),
            execution_duration_ms: 50,
        }
    }

    /// Create a slow execution result (for testing timeouts)
    pub fn slow_result(duration_ms: u64, context: Option<Value>) -> MockStepResult {
        MockStepResult {
            success: true,
            result_context: context,
            error_message: None,
            execution_duration_ms: duration_ms,
        }
    }
}

impl Default for MockDelegate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mock_delegate_execution_tracking() {
        let mut delegate = MockDelegate::new();
        let step_id = Uuid::new_v4();

        // Configure a mock result
        delegate.mock_step_result(
            "TestHandler",
            MockDelegate::success_result(Some(json!({"result": "test_data"}))),
        );

        // Execute a step
        let result = delegate.execute_step(step_id, "TestHandler", json!({"input": "test_input"}));

        // Verify execution was tracked
        assert!(delegate.was_step_executed(step_id));
        assert_eq!(delegate.execution_count_for_handler("TestHandler"), 1);
        assert!(result.success);
        assert_eq!(result.result_context, Some(json!({"result": "test_data"})));
    }

    #[test]
    fn test_multiple_executions() {
        let mut delegate = MockDelegate::new();
        let step1_id = Uuid::new_v4();
        let step2_id = Uuid::new_v4();

        // Execute multiple steps
        delegate.execute_step(step1_id, "Handler1", json!({}));
        delegate.execute_step(step2_id, "Handler1", json!({}));
        delegate.execute_step(step1_id, "Handler2", json!({}));

        // Verify tracking
        assert_eq!(delegate.execution_count_for_handler("Handler1"), 2);
        assert_eq!(delegate.execution_count_for_handler("Handler2"), 1);
        assert_eq!(delegate.executed_steps.len(), 3);
    }

    #[test]
    fn test_error_result() {
        let mut delegate = MockDelegate::new();
        delegate.mock_step_result(
            "FailingHandler",
            MockDelegate::error_result("Something went wrong"),
        );

        let result = delegate.execute_step(Uuid::new_v4(), "FailingHandler", json!({}));

        assert!(!result.success);
        assert_eq!(
            result.error_message,
            Some("Something went wrong".to_string())
        );
    }
}

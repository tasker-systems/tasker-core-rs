//! # TAS-112: Example Handlers Demonstrating Capability Traits
//!
//! This module provides example handlers showing how to use the ergonomic
//! capability traits: `APICapable`, `DecisionCapable`, and `BatchableCapable`.
//!
//! These examples mirror the patterns from Ruby, Python, and TypeScript workers
//! to ensure cross-language consistency.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Instant;

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;

use super::capabilities::{APICapable, BatchableCapable, DecisionCapable, HandlerCapabilities};
use super::{RustStepHandler, StepHandlerConfig};

// ============================================================================
// Context Access Helpers
// ============================================================================

/// Helper trait for accessing task context with optional defaults
trait ContextAccess {
    fn get_context_opt<T: serde::de::DeserializeOwned>(&self, field: &str) -> Option<T>;
}

impl ContextAccess for TaskSequenceStep {
    fn get_context_opt<T: serde::de::DeserializeOwned>(&self, field: &str) -> Option<T> {
        self.get_context_field(field).ok()
    }
}

// ============================================================================
// API Handler Example
// ============================================================================

/// Example API handler demonstrating the `APICapable` trait
///
/// Shows how to:
/// - Make HTTP API calls (simulated)
/// - Use `api_success()` for successful responses
/// - Use `api_failure()` for error responses
/// - Leverage automatic retry classification
#[derive(Debug)]
pub struct ExampleApiHandler {
    config: StepHandlerConfig,
}

impl APICapable for ExampleApiHandler {}

#[async_trait]
impl RustStepHandler for ExampleApiHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start = Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get URL from task context or config
        let url = step_data
            .get_context_opt::<String>("api_url")
            .or_else(|| self.config.get_string("default_url"))
            .unwrap_or_else(|| "https://api.example.com/data".to_string());

        // Simulate API call (in real handler, use reqwest)
        let simulated_response = self.simulate_api_call(&url).await;
        let execution_time_ms = start.elapsed().as_millis() as i64;

        match simulated_response {
            Ok((status, data)) => {
                if (200..300).contains(&status) {
                    Ok(self.api_success(
                        step_uuid,
                        data,
                        status,
                        Some(json!({"content-type": "application/json"})),
                        execution_time_ms,
                    ))
                } else {
                    Ok(self.api_failure(
                        step_uuid,
                        &format!("API returned error status: {status}"),
                        status,
                        "api_error",
                        execution_time_ms,
                    ))
                }
            }
            Err(e) => Ok(self.api_failure(
                step_uuid,
                &format!("API request failed: {e}"),
                0,
                "network_error",
                execution_time_ms,
            )),
        }
    }

    fn name(&self) -> &str {
        "example_api_handler"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExampleApiHandler {
    /// Simulate an API call for demonstration
    async fn simulate_api_call(&self, _url: &str) -> Result<(u16, Value), String> {
        // Simulate successful response
        Ok((
            200,
            json!({
                "user_id": 12345,
                "name": "Example User",
                "status": "active"
            }),
        ))
    }

    /// Get handler capabilities for introspection
    pub fn capabilities() -> HandlerCapabilities {
        HandlerCapabilities::new().with_api()
    }
}

// ============================================================================
// Decision Handler Example
// ============================================================================

/// Example decision handler demonstrating the `DecisionCapable` trait
///
/// Shows how to:
/// - Evaluate conditions from task context
/// - Use `decision_success()` to create downstream steps
/// - Use `skip_branches()` when no action needed
/// - Use `decision_failure()` for error conditions
#[derive(Debug)]
pub struct ExampleDecisionHandler {
    config: StepHandlerConfig,
}

impl DecisionCapable for ExampleDecisionHandler {}

#[async_trait]
impl RustStepHandler for ExampleDecisionHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start = Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get amount from task context
        let amount = step_data.get_context_opt::<f64>("amount").unwrap_or(0.0);

        let execution_time_ms = start.elapsed().as_millis() as i64;

        // Get thresholds from config or use defaults
        let small_threshold = self.config.get_f64("small_threshold").unwrap_or(1000.0);
        let large_threshold = self.config.get_f64("large_threshold").unwrap_or(5000.0);

        // Validate input
        if amount < 0.0 {
            return Ok(self.decision_failure(
                step_uuid,
                "Amount cannot be negative",
                "validation_error",
                execution_time_ms,
            ));
        }

        // Make routing decision based on amount thresholds
        let (steps_to_create, routing_path) = if amount == 0.0 {
            return Ok(self.skip_branches(
                step_uuid,
                "No amount to process",
                Some(json!({"amount": amount})),
                execution_time_ms,
            ));
        } else if amount < small_threshold {
            (vec!["auto_approve".to_string()], "auto_approve")
        } else if amount < large_threshold {
            (vec!["manager_approval".to_string()], "manager_approval")
        } else {
            (
                vec!["manager_approval".to_string(), "finance_review".to_string()],
                "dual_approval",
            )
        };

        Ok(self.decision_success(
            step_uuid,
            steps_to_create,
            Some(json!({
                "routing_path": routing_path,
                "amount": amount,
                "thresholds": {
                    "small": small_threshold,
                    "large": large_threshold,
                }
            })),
            execution_time_ms,
        ))
    }

    fn name(&self) -> &str {
        "example_decision_handler"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExampleDecisionHandler {
    /// Get handler capabilities for introspection
    pub fn capabilities() -> HandlerCapabilities {
        HandlerCapabilities::new().with_decision()
    }
}

// ============================================================================
// Batch Analyzer Example
// ============================================================================

/// Example batch analyzer demonstrating the `BatchableCapable` trait
///
/// Shows how to:
/// - Analyze data to determine batch parameters
/// - Use `create_cursor_configs()` for even distribution
/// - Use `create_cursor_ranges()` for fixed batch sizes
/// - Use `batch_analyzer_success()` to create worker steps
/// - Use `no_batches_outcome()` when no batching needed
#[derive(Debug)]
pub struct ExampleBatchAnalyzer {
    config: StepHandlerConfig,
}

impl BatchableCapable for ExampleBatchAnalyzer {}

#[async_trait]
impl RustStepHandler for ExampleBatchAnalyzer {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start = Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get total items from task context
        let total_items = step_data.get_context_opt::<u64>("total_items").unwrap_or(0);

        let execution_time_ms = start.elapsed().as_millis() as i64;

        // Check if there's anything to process
        if total_items == 0 {
            return Ok(self.no_batches_outcome(
                step_uuid,
                "No items to process",
                execution_time_ms,
            ));
        }

        // Get batch configuration
        let batch_size = self.config.get_u64("batch_size").unwrap_or(100);
        let max_workers = self.config.get_u64("max_workers").unwrap_or(10) as u32;
        let worker_template = self
            .config
            .get_string("worker_template")
            .unwrap_or_else(|| "batch_worker".to_string());

        // Determine batching strategy
        let use_worker_count = self.config.get_bool("use_worker_count").unwrap_or(false);

        let cursor_configs = if use_worker_count {
            // Distribute evenly across workers
            let worker_count = max_workers.min(total_items as u32);
            self.create_cursor_configs(total_items, worker_count)
        } else {
            // Use fixed batch size
            self.create_cursor_ranges(total_items, batch_size, Some(max_workers))
        };

        Ok(self.batch_analyzer_success(
            step_uuid,
            &worker_template,
            cursor_configs,
            total_items,
            execution_time_ms,
        ))
    }

    fn name(&self) -> &str {
        "example_batch_analyzer"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExampleBatchAnalyzer {
    /// Get handler capabilities for introspection
    pub fn capabilities() -> HandlerCapabilities {
        HandlerCapabilities::new().with_batch()
    }
}

// ============================================================================
// Batch Worker Example
// ============================================================================

/// Example batch worker demonstrating batch processing result reporting
///
/// Shows how to:
/// - Access cursor configuration from task context
/// - Process items within the batch range
/// - Use `batch_worker_success()` to report statistics
#[derive(Debug)]
pub struct ExampleBatchWorker {
    config: StepHandlerConfig,
}

impl BatchableCapable for ExampleBatchWorker {}

#[async_trait]
impl RustStepHandler for ExampleBatchWorker {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start = Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get cursor info from batch_worker_inputs in context
        #[derive(serde::Deserialize, Default)]
        struct CursorData {
            start_cursor: u64,
            end_cursor: u64,
        }

        #[derive(serde::Deserialize, Default)]
        struct BatchInputs {
            #[serde(default)]
            cursor: CursorData,
        }

        let batch_inputs: BatchInputs = step_data
            .get_context_opt("batch_worker_inputs")
            .unwrap_or_default();

        let start_cursor = batch_inputs.cursor.start_cursor;
        let end_cursor = batch_inputs.cursor.end_cursor;

        // Simulate processing items in range
        let items_to_process = end_cursor - start_cursor;
        let (succeeded, failed, skipped) = self.simulate_processing(items_to_process).await;

        let execution_time_ms = start.elapsed().as_millis() as i64;

        // Check if too many failures
        let failure_threshold = self.config.get_f64("failure_threshold").unwrap_or(0.5);
        let failure_rate = if items_to_process > 0 {
            failed as f64 / items_to_process as f64
        } else {
            0.0
        };

        if failure_rate > failure_threshold {
            return Ok(self.batch_failure(
                step_uuid,
                &format!(
                    "Batch failure rate {:.1}% exceeded threshold",
                    failure_rate * 100.0
                ),
                "high_failure_rate",
                true, // Retryable
                execution_time_ms,
            ));
        }

        Ok(self.batch_worker_success(
            step_uuid,
            items_to_process,
            succeeded,
            failed,
            skipped,
            execution_time_ms,
        ))
    }

    fn name(&self) -> &str {
        "example_batch_worker"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExampleBatchWorker {
    /// Simulate processing items and returning statistics
    async fn simulate_processing(&self, count: u64) -> (u64, u64, u64) {
        // Simulate: 95% success, 3% failure, 2% skipped
        let succeeded = (count as f64 * 0.95) as u64;
        let failed = (count as f64 * 0.03) as u64;
        let skipped = count - succeeded - failed;
        (succeeded, failed, skipped)
    }

    /// Get handler capabilities for introspection
    pub fn capabilities() -> HandlerCapabilities {
        HandlerCapabilities::new().with_batch()
    }
}

// ============================================================================
// Composite Handler Example
// ============================================================================

/// Example handler implementing multiple capability traits
///
/// Demonstrates that traits compose independently - a handler can implement
/// any combination of capabilities.
#[derive(Debug)]
pub struct CompositeHandler {
    #[expect(dead_code, reason = "Config available for future handler enhancements")]
    config: StepHandlerConfig,
}

// Implement multiple capability traits
impl APICapable for CompositeHandler {}
impl DecisionCapable for CompositeHandler {}
impl BatchableCapable for CompositeHandler {}

#[async_trait]
impl RustStepHandler for CompositeHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start = Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get operation type from context to decide which capability to use
        let operation = step_data
            .get_context_opt::<String>("operation")
            .unwrap_or_else(|| "default".to_string());

        let execution_time_ms = start.elapsed().as_millis() as i64;

        match operation.as_str() {
            "api_call" => {
                // Use API capability
                Ok(self.api_success(
                    step_uuid,
                    json!({"result": "api_data"}),
                    200,
                    None,
                    execution_time_ms,
                ))
            }
            "route_decision" => {
                // Use decision capability
                Ok(self.decision_success(
                    step_uuid,
                    vec!["next_step".to_string()],
                    None,
                    execution_time_ms,
                ))
            }
            "batch_setup" => {
                // Use batch capability
                let configs = self.create_cursor_configs(1000, 4);
                Ok(self.batch_analyzer_success(
                    step_uuid,
                    "worker_template",
                    configs,
                    1000,
                    execution_time_ms,
                ))
            }
            _ => {
                // Default success
                Ok(StepExecutionResult::success(
                    step_uuid,
                    json!({"operation": operation}),
                    execution_time_ms,
                    None,
                ))
            }
        }
    }

    fn name(&self) -> &str {
        "composite_handler"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

impl CompositeHandler {
    /// Get handler capabilities for introspection
    pub fn capabilities() -> HandlerCapabilities {
        HandlerCapabilities::new()
            .with_api()
            .with_decision()
            .with_batch()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tasker_shared::models::core::task::Task;
    use tasker_shared::models::core::task_template::HandlerDefinition;
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::models::task_template::StepDefinition;
    use uuid::Uuid;

    /// Helper to create test TaskSequenceStep with arbitrary context
    fn create_test_step_data(context: Value) -> TaskSequenceStep {
        let now = chrono::Utc::now().naive_utc();
        let task = Task {
            task_uuid: Uuid::new_v4(),
            named_task_uuid: Uuid::new_v4(),
            complete: false,
            requested_at: now,
            initiator: Some("test".to_string()),
            source_system: None,
            reason: None,

            tags: None,
            context: Some(context),
            identity_hash: "test".to_string(),
            priority: 0,
            created_at: now,
            updated_at: now,
            correlation_id: Uuid::new_v4(),
            parent_correlation_id: None,
        };

        let task_for_orch = tasker_shared::models::core::task::TaskForOrchestration {
            task,
            task_name: "test_workflow".to_string(),
            task_version: "1.0.0".to_string(),
            namespace_name: "test".to_string(),
        };

        let workflow_step = WorkflowStepWithName {
            workflow_step_uuid: Uuid::new_v4(),
            task_uuid: task_for_orch.task.task_uuid,
            named_step_uuid: Uuid::new_v4(),
            name: "test_step".to_string(),
            template_step_name: "test_step".to_string(),
            retryable: true,
            max_attempts: Some(3),
            in_process: false,
            processed: false,
            processed_at: None,
            attempts: Some(1),
            last_attempted_at: None,
            backoff_request_seconds: None,
            inputs: None,
            results: None,

            checkpoint: None,
            created_at: now,
            updated_at: now,
        };

        let step_definition = StepDefinition {
            name: "test_step".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "TestHandler".to_string(),
                method: None,
                resolver: None,
                initialization: HashMap::new(),
            },
            step_type: tasker_shared::models::task_template::StepType::Standard,
            system_dependency: None,
            dependencies: vec![],
            retry: tasker_shared::models::task_template::RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
            batch_config: None,
        };

        TaskSequenceStep {
            task: task_for_orch,
            workflow_step,
            dependency_results: HashMap::new(),
            step_definition,
        }
    }

    #[tokio::test]
    async fn test_api_handler_success() {
        let config = StepHandlerConfig::empty();
        let handler = ExampleApiHandler::new(config);

        let step_data = create_test_step_data(json!({
            "api_url": "https://api.example.com/test"
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert_eq!(result.result["status_code"], 200);
    }

    #[tokio::test]
    async fn test_decision_handler_auto_approve() {
        let config = StepHandlerConfig::empty();
        let handler = ExampleDecisionHandler::new(config);

        let step_data = create_test_step_data(json!({
            "amount": 500.0
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert_eq!(result.result["steps_created"][0], "auto_approve");
    }

    #[tokio::test]
    async fn test_decision_handler_dual_approval() {
        let config = StepHandlerConfig::empty();
        let handler = ExampleDecisionHandler::new(config);

        let step_data = create_test_step_data(json!({
            "amount": 10000.0
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        let steps: Vec<String> =
            serde_json::from_value(result.result["steps_created"].clone()).unwrap();
        assert!(steps.contains(&"manager_approval".to_string()));
        assert!(steps.contains(&"finance_review".to_string()));
    }

    #[tokio::test]
    async fn test_decision_handler_skip_zero_amount() {
        let config = StepHandlerConfig::empty();
        let handler = ExampleDecisionHandler::new(config);

        let step_data = create_test_step_data(json!({
            "amount": 0.0
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert!(result.result.get("skip_reason").is_some());
    }

    #[tokio::test]
    async fn test_batch_analyzer_creates_workers() {
        let mut config_data = HashMap::new();
        config_data.insert("batch_size".to_string(), json!(100));
        config_data.insert("max_workers".to_string(), json!(5));

        let config = StepHandlerConfig::new(config_data);
        let handler = ExampleBatchAnalyzer::new(config);

        let step_data = create_test_step_data(json!({
            "total_items": 350
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert_eq!(result.result["worker_count"], 4); // 350 / 100 = 4 batches
        assert_eq!(result.result["total_items"], 350);
    }

    #[tokio::test]
    async fn test_batch_analyzer_no_items() {
        let config = StepHandlerConfig::empty();
        let handler = ExampleBatchAnalyzer::new(config);

        let step_data = create_test_step_data(json!({
            "total_items": 0
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert_eq!(result.result["reason"], "No items to process");
    }

    #[tokio::test]
    async fn test_batch_worker_success() {
        let config = StepHandlerConfig::empty();
        let handler = ExampleBatchWorker::new(config);

        let step_data = create_test_step_data(json!({
            "batch_worker_inputs": {
                "cursor": {
                    "start_cursor": 0,
                    "end_cursor": 100
                }
            }
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert!(result.result.get("processed").is_some());
        assert!(result.result.get("succeeded").is_some());
    }

    #[tokio::test]
    async fn test_composite_handler_api_operation() {
        let config = StepHandlerConfig::empty();
        let handler = CompositeHandler::new(config);

        let step_data = create_test_step_data(json!({
            "operation": "api_call"
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert_eq!(result.result["status_code"], 200);
    }

    #[tokio::test]
    async fn test_composite_handler_decision_operation() {
        let config = StepHandlerConfig::empty();
        let handler = CompositeHandler::new(config);

        let step_data = create_test_step_data(json!({
            "operation": "route_decision"
        }));

        let result = handler.call(&step_data).await.unwrap();

        assert!(result.is_success());
        assert_eq!(result.result["steps_created"][0], "next_step");
    }

    #[test]
    fn test_handler_capabilities() {
        let api_caps = ExampleApiHandler::capabilities();
        assert!(api_caps.api_capable);
        assert!(!api_caps.decision_capable);

        let decision_caps = ExampleDecisionHandler::capabilities();
        assert!(decision_caps.decision_capable);
        assert!(!decision_caps.api_capable);

        let composite_caps = CompositeHandler::capabilities();
        assert!(composite_caps.api_capable);
        assert!(composite_caps.decision_capable);
        assert!(composite_caps.batchable);
    }
}

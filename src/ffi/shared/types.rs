//! # Shared FFI Types
//!
//! Common type definitions for language-agnostic FFI operations.
//! These types can be converted to/from language-specific types by
//! the language bindings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Task input for shared FFI operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInput {
    pub namespace: String,
    pub name: String,
    pub version: Option<String>,
    pub context: Option<serde_json::Value>,
    pub initiator: Option<String>,
}

/// Task output from shared FFI operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutput {
    pub task_id: i64,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub created_at: String,
}

/// Workflow step input for shared FFI operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepInput {
    pub task_id: i64,
    pub name: String,
    pub dependencies: Option<Vec<i64>>,
    pub handler_class: Option<String>,
    pub config: Option<serde_json::Value>,
}

/// Workflow step output from shared FFI operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutput {
    pub step_id: i64,
    pub task_id: i64,
    pub name: String,
    pub status: String,
    pub dependencies: Vec<i64>,
}

/// Handler metadata for shared FFI operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerMetadata {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub handler_class: String,
    pub config_schema: Option<serde_json::Value>,
}

/// Foundation creation input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundationInput {
    pub namespace: String,
    pub task_name: String,
}

/// Foundation creation output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundationOutput {
    pub namespace: HashMap<String, serde_json::Value>,
    pub named_task: HashMap<String, serde_json::Value>,
    pub named_step: HashMap<String, serde_json::Value>,
    pub foundation_id: String,
    pub status: String,
    pub components: Vec<String>,
}

// =============================================
// TESTING FACTORY TYPES
// =============================================

/// Test task creation input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateTestTaskInput {
    pub namespace: String,
    pub name: String,
    pub version: Option<String>,
    pub context: Option<serde_json::Value>,
    pub initiator: Option<String>,
}

/// Test task creation output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestTaskOutput {
    pub task_id: i64,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub context: serde_json::Value,
    pub created_at: String,
}

/// Test step creation input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateTestStepInput {
    pub task_id: i64,
    pub name: String,
    pub handler_class: Option<String>,
    pub dependencies: Option<Vec<i64>>,
    pub config: Option<serde_json::Value>,
}

/// Test step creation output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStepOutput {
    pub step_id: i64,
    pub task_id: i64,
    pub name: String,
    pub handler_class: String,
    pub status: String,
    pub dependencies: Vec<i64>,
    pub config: serde_json::Value,
}

/// Test foundation creation input
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateTestFoundationInput {
    pub namespace: String,
    pub task_name: String,
    pub step_name: String,
}

/// Test foundation creation output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFoundationOutput {
    pub foundation_id: String,
    pub namespace: serde_json::Value,
    pub named_task: serde_json::Value,
    pub named_step: serde_json::Value,
    pub status: String,
    pub components: Vec<String>,
}

/// Environment setup result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSetupResult {
    pub status: String,
    pub message: String,
    pub handle_id: String,
    pub pool_size: u32,
}

/// Environment cleanup result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentCleanupResult {
    pub status: String,
    pub message: String,
    pub handle_id: String,
    pub pool_size: u32,
}

// =============================================
// ANALYTICS TYPES
// =============================================

/// Task execution context for analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionContext {
    pub task_id: i64,
    pub total_steps: i64,
    pub completed_steps: i64,
    pub pending_steps: i64,
    pub error_steps: i64,
    pub ready_steps: i64,
    pub blocked_steps: i64,
    pub completion_percentage: f64,
    pub estimated_duration_seconds: Option<i64>,
    pub recommended_action: String,
    pub next_steps_to_execute: Vec<i64>,
    pub critical_path_steps: Vec<i64>,
    pub bottleneck_steps: Vec<i64>,
}

/// Analytics metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsMetrics {
    pub total_tasks: i64,
    pub completed_tasks: i64,
    pub failed_tasks: i64,
    pub pending_tasks: i64,
    pub average_completion_time_seconds: f64,
    pub success_rate_percentage: f64,
    pub most_common_failure_reason: String,
    pub peak_throughput_tasks_per_hour: i64,
    pub current_load_percentage: f64,
    pub resource_utilization: serde_json::Value,
}

/// Dependency analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAnalysis {
    pub task_id: i64,
    pub total_dependencies: i64,
    pub resolved_dependencies: i64,
    pub pending_dependencies: i64,
    pub circular_dependencies: Vec<i64>,
    pub critical_path: Vec<i64>,
    pub optimization_suggestions: Vec<String>,
    pub estimated_completion_time_seconds: i64,
}

/// Performance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub timeframe_hours: i64,
    pub total_operations: i64,
    pub successful_operations: i64,
    pub failed_operations: i64,
    pub average_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub p99_response_time_ms: f64,
    pub throughput_operations_per_second: f64,
    pub error_rate_percentage: f64,
    pub resource_usage: serde_json::Value,
    pub slowest_operations: Vec<SlowOperation>,
    pub recommendations: Vec<String>,
}

/// Slow operation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowOperation {
    pub operation_name: String,
    pub duration_ms: i64,
    pub count: i64,
}

// =============================================
// EVENT BRIDGE TYPES
// =============================================

/// Shared event for cross-language event forwarding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub metadata: serde_json::Value,
}

/// Structured event with typed fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredEvent {
    pub namespace: String,
    pub name: String,
    pub version: Option<String>,
    pub source: String,
    pub timestamp: String,
    pub context: serde_json::Value,
    pub data: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
}

/// Event statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventStatistics {
    pub total_events_published: i64,
    pub events_by_type: serde_json::Value,
    pub average_events_per_minute: f64,
    pub peak_events_per_minute: i64,
    pub callback_success_rate: f64,
    pub failed_callbacks: i64,
    pub active_language_bindings: Vec<String>,
}

/// Event bridge test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBridgeTestResult {
    pub success: bool,
    pub message: String,
    pub events_published: i64,
    pub callbacks_triggered: i64,
}

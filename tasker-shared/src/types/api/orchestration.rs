use crate::database::sql_functions::{StepReadinessStatus, SystemHealthCounts};
use crate::models::core::task::PaginationInfo;
use crate::models::orchestration::execution_status::ExecutionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

/// Response for successful task creation
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TaskCreationResponse {
    pub task_uuid: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub estimated_completion: Option<DateTime<Utc>>,
    /// Number of workflow steps created for this task
    pub step_count: usize,
    /// Mapping of step names to their workflow step UUIDs
    pub step_mapping: HashMap<String, String>,
    /// Handler configuration name used (if any)
    pub handler_config_name: Option<String>,
}

/// Task details response with execution context and step readiness information
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TaskResponse {
    pub task_uuid: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub context: serde_json::Value,
    pub initiator: String,
    pub source_system: String,
    pub reason: String,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,

    // Execution context fields from TaskExecutionContext
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub ready_steps: i64,
    pub execution_status: String,
    pub recommended_action: String,
    pub completion_percentage: f64,
    pub health_status: String,

    // Step readiness information
    pub steps: Vec<StepReadinessStatus>,
}

/// Task list response with pagination
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TaskListResponse {
    pub tasks: Vec<TaskResponse>,
    pub pagination: PaginationInfo,
}

/// Manual step resolution request
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ManualResolutionRequest {
    pub resolution_data: serde_json::Value,
    pub resolved_by: String,
    pub reason: String,
}

/// Step details response with readiness information
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct StepResponse {
    pub step_uuid: String,
    pub task_uuid: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub results: Option<serde_json::Value>,

    // StepReadinessStatus fields
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_failure_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub last_attempted_at: Option<String>,
}

/// Namespace information
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct NamespaceInfo {
    pub name: String,
    pub description: Option<String>,
    pub handler_count: u32,
}

/// Handler information
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HandlerInfo {
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub description: Option<String>,
    pub step_templates: Vec<String>,
}

/// Basic health check response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
}

/// Detailed health check response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct DetailedHealthResponse {
    pub status: String,
    pub timestamp: String,
    pub checks: HashMap<String, HealthCheck>,
    pub info: HealthInfo,
}

/// Individual health check result
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HealthCheck {
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: u64,
}

/// System information for detailed health
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HealthInfo {
    pub version: String,
    pub environment: String,
    pub operational_state: String,
    pub web_database_pool_size: u32,
    pub orchestration_database_pool_size: u32,
    pub circuit_breaker_state: String,
}

/// Query parameters for performance metrics
#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    /// Number of hours to look back (default: 24)
    pub hours: Option<u32>,
}

/// Query parameters for bottleneck analysis
#[derive(Debug, Deserialize)]
pub struct BottleneckQuery {
    /// Maximum number of slow steps to return (default: 10)
    pub limit: Option<i32>,
    /// Minimum number of executions for inclusion (default: 5)
    pub min_executions: Option<i32>,
}

/// Performance metrics response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct PerformanceMetrics {
    pub total_tasks: i64,
    pub active_tasks: i64,
    pub completed_tasks: i64,
    pub failed_tasks: i64,
    pub completion_rate: f64,
    pub error_rate: f64,
    pub average_task_duration_seconds: f64,
    pub average_step_duration_seconds: f64,
    pub tasks_per_hour: i64,
    pub steps_per_hour: i64,
    pub system_health_score: f64,
    pub analysis_period_start: String,
    pub calculated_at: String,
}

/// Bottleneck analysis response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct BottleneckAnalysis {
    pub slow_steps: Vec<SlowStepInfo>,
    pub slow_tasks: Vec<SlowTaskInfo>,
    pub resource_utilization: ResourceUtilization,
    pub recommendations: Vec<String>,
}

/// Information about slow-performing steps
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SlowStepInfo {
    pub step_name: String,
    pub average_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub execution_count: i32,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<String>,
}

/// Information about slow-performing tasks
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SlowTaskInfo {
    pub task_name: String,
    pub average_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub execution_count: i32,
    pub average_step_count: f64,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<String>,
}

/// Resource utilization metrics
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ResourceUtilization {
    pub database_pool_utilization: f64,
    pub system_health: SystemHealthCounts,
}

impl TaskResponse {
    /// Convert the execution_status string to ExecutionStatus enum for type-safe handling
    pub fn execution_status_typed(&self) -> ExecutionStatus {
        ExecutionStatus::from(self.execution_status.clone())
    }

    /// Check if the task has completed all steps (execution status indicates all complete)
    pub fn is_execution_complete(&self) -> bool {
        matches!(self.execution_status_typed(), ExecutionStatus::AllComplete)
    }

    /// Check if the task is blocked by failures that cannot be retried
    pub fn is_execution_blocked(&self) -> bool {
        matches!(
            self.execution_status_typed(),
            ExecutionStatus::BlockedByFailures
        )
    }

    /// Check if the task is currently processing (has steps in progress)
    pub fn is_execution_processing(&self) -> bool {
        matches!(self.execution_status_typed(), ExecutionStatus::Processing)
    }

    /// Check if the task has ready steps to execute
    pub fn has_execution_ready_steps(&self) -> bool {
        matches!(
            self.execution_status_typed(),
            ExecutionStatus::HasReadySteps
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_task_response_execution_status_conversion() {
        let task_response = TaskResponse {
            task_uuid: "test-uuid".to_string(),
            name: "test_task".to_string(),
            namespace: "test".to_string(),
            version: "1.0.0".to_string(),
            status: "pending".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            context: serde_json::json!({}),
            initiator: "test".to_string(),
            source_system: "test".to_string(),
            reason: "test".to_string(),
            priority: Some(1),
            tags: Some(vec!["test".to_string()]),
            total_steps: 3,
            pending_steps: 0,
            in_progress_steps: 0,
            completed_steps: 3,
            failed_steps: 0,
            ready_steps: 0,
            execution_status: "all_complete".to_string(),
            recommended_action: "finalize_task".to_string(),
            completion_percentage: 100.0,
            health_status: "healthy".to_string(),
            steps: vec![],
        };

        // Test conversion to ExecutionStatus enum
        assert_eq!(
            task_response.execution_status_typed(),
            ExecutionStatus::AllComplete
        );
        assert!(task_response.is_execution_complete());
        assert!(!task_response.is_execution_blocked());
        assert!(!task_response.is_execution_processing());
        assert!(!task_response.has_execution_ready_steps());
    }

    #[test]
    fn test_task_response_execution_status_variants() {
        let base_response = TaskResponse {
            task_uuid: "test-uuid".to_string(),
            name: "test_task".to_string(),
            namespace: "test".to_string(),
            version: "1.0.0".to_string(),
            status: "pending".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            context: serde_json::json!({}),
            initiator: "test".to_string(),
            source_system: "test".to_string(),
            reason: "test".to_string(),
            priority: Some(1),
            tags: Some(vec!["test".to_string()]),
            total_steps: 3,
            pending_steps: 2,
            in_progress_steps: 1,
            completed_steps: 0,
            failed_steps: 0,
            ready_steps: 0,
            execution_status: "processing".to_string(),
            recommended_action: "wait_for_completion".to_string(),
            completion_percentage: 33.3,
            health_status: "healthy".to_string(),
            steps: vec![],
        };

        // Test processing status
        assert_eq!(
            base_response.execution_status_typed(),
            ExecutionStatus::Processing
        );
        assert!(base_response.is_execution_processing());
        assert!(!base_response.is_execution_complete());

        // Test blocked by failures status
        let blocked_response = TaskResponse {
            execution_status: "blocked_by_failures".to_string(),
            ..base_response.clone()
        };
        assert_eq!(
            blocked_response.execution_status_typed(),
            ExecutionStatus::BlockedByFailures
        );
        assert!(blocked_response.is_execution_blocked());

        // Test has ready steps status
        let ready_response = TaskResponse {
            execution_status: "has_ready_steps".to_string(),
            ..base_response
        };
        assert_eq!(
            ready_response.execution_status_typed(),
            ExecutionStatus::HasReadySteps
        );
        assert!(ready_response.has_execution_ready_steps());
    }
}

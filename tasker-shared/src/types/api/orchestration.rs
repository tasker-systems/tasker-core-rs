use crate::database::sql_functions::{StepReadinessStatus, SystemHealthCounts};
use crate::models::core::task::PaginationInfo;
use crate::models::orchestration::execution_status::ExecutionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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

    /// TAS-154: Whether this request created a new task (true) or returned an existing one (false)
    ///
    /// When `false`, this indicates deduplication occurred based on the identity_hash.
    /// Check `deduplicated_from` for the original task creation timestamp.
    #[serde(default = "default_true")]
    pub created: bool,

    /// TAS-154: If deduplicated, the original task creation timestamp
    ///
    /// Only present when `created` is `false`, indicating when the original task was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicated_from: Option<DateTime<Utc>>,
}

fn default_true() -> bool {
    true
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
    pub correlation_id: Uuid,
    pub parent_correlation_id: Option<Uuid>,

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

/// Manual completion data for providing step execution results
///
/// This structure contains only the operator-provided business data.
/// The system will automatically set:
/// - `success: true` (manual completion implies success)
/// - `status: "completed"` (always completed for manual resolution)
/// - `error: None` (no errors for successful manual completion)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ManualCompletionData {
    /// The actual result data for this step
    ///
    /// This should contain the business data that dependent steps expect.
    /// The shape must match what downstream step handlers require.
    pub result: serde_json::Value,

    /// Optional metadata for observability and tracking
    ///
    /// Can include execution timing, resource usage, or other operational metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Manual step action request for PATCH /tasks/{uuid}/workflow_steps/{step_uuid}
///
/// Supports three distinct operator actions via tagged enum:
///
/// ## 1. Reset for Retry
/// Reset attempt counter and return step to `pending` for automatic retry.
/// Use when the issue was transient and natural retry should succeed.
///
/// ```json
/// {
///   "action_type": "reset_for_retry",
///   "reason": "Database connection restored, retry should succeed",
///   "reset_by": "operator@example.com"
/// }
/// ```
///
/// ## 2. Resolve Manually (Simple)
/// Mark step as `resolved_manually` without providing results.
/// Use when acknowledging failure and moving workflow forward without correction.
///
/// ```json
/// {
///   "action_type": "resolve_manually",
///   "reason": "Non-critical validation step, bypassing",
///   "resolved_by": "operator@example.com"
/// }
/// ```
///
/// ## 3. Complete Manually (with Results)
/// Transition step to `complete` with operator-provided execution results.
/// Use when providing corrected data for dependent steps to consume.
///
/// ```json
/// {
///   "action_type": "complete_manually",
///   "completion_data": {
///     "result": {"validated": true, "score": 95},
///     "metadata": {"manually_verified": true}
///   },
///   "reason": "Manual verification completed",
///   "completed_by": "operator@example.com"
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum StepManualAction {
    /// Reset attempt counter and return step to pending for automatic retry
    ///
    /// State transition: `error` → `pending`
    /// Attempt counter: Reset to 0
    /// Use case: Transient failures that should be retried naturally
    ResetForRetry {
        /// Human-readable reason for reset
        reason: String,
        /// Operator performing the reset
        reset_by: String,
    },

    /// Mark step as manually resolved without providing results
    ///
    /// State transition: Any state → `resolved_manually`
    /// Terminal state: Yes
    /// Use case: Acknowledge failure, move workflow forward without correction
    ResolveManually {
        /// Human-readable reason for manual resolution
        reason: String,
        /// Operator performing the resolution
        resolved_by: String,
    },

    /// Complete step manually with execution results
    ///
    /// State transition: Any state → `complete`
    /// Terminal state: Yes
    /// Use case: Provide corrected data for dependent steps to consume
    ///
    /// System enforces `success: true` and `status: "completed"`.
    /// Operator only provides `result` and optional `metadata`.
    CompleteManually {
        /// Execution results to persist
        ///
        /// Contains business data that dependent steps expect.
        /// System automatically wraps this in `StepExecutionResult` with:
        /// - `success: true`
        /// - `status: "completed"`
        /// - `error: null`
        completion_data: ManualCompletionData,
        /// Human-readable reason for manual completion
        reason: String,
        /// Operator performing the completion
        completed_by: String,
    },
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

/// Step audit record response for SOC2-compliant audit trails (TAS-62)
///
/// Provides attribution context (worker_uuid, correlation_id) and execution summary
/// with full results retrieved via JOIN to the transitions table.
///
/// ## Usage
///
/// ```json
/// GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}/audit
/// [
///   {
///     "audit_uuid": "01934567-89ab-cdef-0123-456789abcdef",
///     "workflow_step_uuid": "...",
///     "transition_uuid": "...",
///     "task_uuid": "...",
///     "recorded_at": "2025-12-03T10:30:00Z",
///     "worker_uuid": "worker-abc-123",
///     "correlation_id": "corr-xyz-789",
///     "success": true,
///     "execution_time_ms": 150,
///     "result": { ... },
///     "step_name": "validate_order",
///     "from_state": "in_progress",
///     "to_state": "enqueued_for_orchestration"
///   }
/// ]
/// ```
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct StepAuditResponse {
    /// Primary key of the audit record
    pub audit_uuid: String,

    /// The workflow step this audit record belongs to
    pub workflow_step_uuid: String,

    /// The specific transition that recorded the result
    pub transition_uuid: String,

    /// The parent task
    pub task_uuid: String,

    /// When the audit record was created (ISO 8601 format)
    pub recorded_at: String,

    // Attribution context (SOC2 compliance)
    /// UUID of the worker instance that processed this step
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_uuid: Option<String>,

    /// Correlation ID for distributed tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,

    // Result summary
    /// Whether the step execution succeeded
    pub success: bool,

    /// Execution time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<i64>,

    // Full result from transition metadata (via JOIN)
    /// Full execution result from the transition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    // Context
    /// Name of the step
    pub step_name: String,

    /// State before the transition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_state: Option<String>,

    /// State after the transition
    pub to_state: String,
}

impl StepAuditResponse {
    /// Create from database model with transition details
    pub fn from_audit_with_transition(
        audit: &crate::models::core::workflow_step_result_audit::StepAuditWithTransition,
    ) -> Self {
        // Extract result from transition metadata if available
        let result = audit.transition_metadata.as_ref().and_then(|meta| {
            meta.get("event")
                .and_then(|e| serde_json::from_str::<serde_json::Value>(e.as_str()?).ok())
        });

        Self {
            audit_uuid: audit.workflow_step_result_audit_uuid.to_string(),
            workflow_step_uuid: audit.workflow_step_uuid.to_string(),
            transition_uuid: audit.workflow_step_transition_uuid.to_string(),
            task_uuid: audit.task_uuid.to_string(),
            recorded_at: audit
                .recorded_at
                .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                .to_string(),
            worker_uuid: audit.worker_uuid.map(|u| u.to_string()),
            correlation_id: audit.correlation_id.map(|u| u.to_string()),
            success: audit.success,
            execution_time_ms: audit.execution_time_ms,
            result,
            step_name: audit.step_name.clone(),
            from_state: audit.from_state.clone(),
            to_state: audit.to_state.clone(),
        }
    }
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

// TAS-164: Pool utilization types are shared with worker health endpoints
pub use super::health::{PoolDetail, PoolUtilizationInfo};

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
    /// Connection pool utilization details (TAS-164)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_utilization: Option<PoolUtilizationInfo>,
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
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
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
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
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

// ============================================================================
// SAFE CONFIG RESPONSE TYPES (TAS-150: Whitelist-only config exposure)
// ============================================================================

/// Orchestration configuration response (whitelist-only).
///
/// Only exposes operational metadata that is safe for external consumption.
/// No secrets, credentials, keys, or database URLs are included.
/// Adding new fields requires a conscious decision about sensitivity.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct OrchestrationConfigResponse {
    pub metadata: ConfigMetadata,
    pub auth: SafeAuthConfig,
    pub circuit_breakers: SafeCircuitBreakerConfig,
    pub database_pools: SafeDatabasePoolConfig,
    pub deployment_mode: String,
    pub messaging: SafeMessagingConfig,
}

/// Worker configuration response (whitelist-only).
///
/// Same whitelist principle as orchestration — only safe operational fields.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct WorkerConfigResponse {
    pub metadata: ConfigMetadata,
    pub worker_id: String,
    pub worker_type: String,
    pub auth: SafeAuthConfig,
    pub messaging: SafeMessagingConfig,
}

/// Response metadata (non-sensitive system info).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ConfigMetadata {
    pub timestamp: DateTime<Utc>,
    pub environment: String,
    pub version: String,
}

/// Non-sensitive auth configuration summary.
///
/// Exposes whether auth is enabled and how it's configured,
/// without revealing keys, secrets, or credential values.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SafeAuthConfig {
    pub enabled: bool,
    /// Verification method: "public_key", "jwks", or "none"
    pub verification_method: String,
    pub jwt_issuer: String,
    pub jwt_audience: String,
    pub api_key_header: String,
    /// Number of configured API keys (not their values)
    pub api_key_count: usize,
    pub strict_validation: bool,
    pub allowed_algorithms: Vec<String>,
}

/// Non-sensitive circuit breaker configuration.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SafeCircuitBreakerConfig {
    pub enabled: bool,
    pub failure_threshold: u32,
    pub timeout_seconds: u32,
    pub success_threshold: u32,
}

/// Non-sensitive database pool configuration.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SafeDatabasePoolConfig {
    pub web_api_pool_size: u32,
    pub web_api_max_connections: u32,
}

/// Non-sensitive messaging configuration.
///
/// Exposes backend type and queue names — these are operational
/// metadata useful for debugging, not secrets.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SafeMessagingConfig {
    /// Backend type: "pgmq" or "rabbitmq"
    pub backend: String,
    /// Queue names owned by this component
    pub queues: Vec<String>,
}

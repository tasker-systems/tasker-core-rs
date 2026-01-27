//! Type conversions between Protocol Buffer types and domain types.
//!
//! This module provides `From` and `Into` implementations for converting between
//! the gRPC proto types and the domain types used throughout the application.
//! These are defined here in tasker-shared to satisfy Rust's orphan rules.
//!
//! Note: For conversions between external types (like `DateTime<Utc>` and `Timestamp`),
//! helper functions are provided since Rust's orphan rules prevent implementing
//! `From`/`Into` traits for types not defined in this crate.

use chrono::{DateTime, Utc};
use prost_types::Timestamp;

use crate::proto::v1 as proto;
use crate::state_machine::states::{TaskState, WorkflowStepState};
use crate::types::api::orchestration::{StepAuditResponse, StepResponse, TaskResponse};

// ============================================================================
// Timestamp Conversions (helper functions due to orphan rules)
// ============================================================================

/// Convert a `DateTime<Utc>` to a protobuf `Timestamp`.
pub fn datetime_to_timestamp(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Convert a protobuf `Timestamp` to a `DateTime<Utc>`.
pub fn timestamp_to_datetime(ts: Timestamp) -> DateTime<Utc> {
    DateTime::from_timestamp(ts.seconds, ts.nanos as u32).unwrap_or_default()
}

// ============================================================================
// Task State Conversions - Lossless 1:1 mapping
// ============================================================================

impl From<TaskState> for proto::TaskState {
    fn from(state: TaskState) -> Self {
        match state {
            TaskState::Pending => proto::TaskState::Pending,
            TaskState::Initializing => proto::TaskState::Initializing,
            TaskState::EnqueuingSteps => proto::TaskState::EnqueuingSteps,
            TaskState::StepsInProcess => proto::TaskState::StepsInProcess,
            TaskState::EvaluatingResults => proto::TaskState::EvaluatingResults,
            TaskState::WaitingForDependencies => proto::TaskState::WaitingForDependencies,
            TaskState::WaitingForRetry => proto::TaskState::WaitingForRetry,
            TaskState::BlockedByFailures => proto::TaskState::BlockedByFailures,
            TaskState::Complete => proto::TaskState::Complete,
            TaskState::Error => proto::TaskState::Error,
            TaskState::Cancelled => proto::TaskState::Cancelled,
            TaskState::ResolvedManually => proto::TaskState::ResolvedManually,
        }
    }
}

impl TryFrom<proto::TaskState> for TaskState {
    type Error = ();

    fn try_from(state: proto::TaskState) -> Result<Self, <Self as TryFrom<proto::TaskState>>::Error> {
        match state {
            proto::TaskState::Pending => Ok(TaskState::Pending),
            proto::TaskState::Initializing => Ok(TaskState::Initializing),
            proto::TaskState::EnqueuingSteps => Ok(TaskState::EnqueuingSteps),
            proto::TaskState::StepsInProcess => Ok(TaskState::StepsInProcess),
            proto::TaskState::EvaluatingResults => Ok(TaskState::EvaluatingResults),
            proto::TaskState::WaitingForDependencies => Ok(TaskState::WaitingForDependencies),
            proto::TaskState::WaitingForRetry => Ok(TaskState::WaitingForRetry),
            proto::TaskState::BlockedByFailures => Ok(TaskState::BlockedByFailures),
            proto::TaskState::Complete => Ok(TaskState::Complete),
            proto::TaskState::Error => Ok(TaskState::Error),
            proto::TaskState::Cancelled => Ok(TaskState::Cancelled),
            proto::TaskState::ResolvedManually => Ok(TaskState::ResolvedManually),
            proto::TaskState::Unspecified => Err(()),
        }
    }
}

// ============================================================================
// Step State Conversions - Lossless 1:1 mapping
// ============================================================================

impl From<WorkflowStepState> for proto::StepState {
    fn from(state: WorkflowStepState) -> Self {
        match state {
            WorkflowStepState::Pending => proto::StepState::Pending,
            WorkflowStepState::Enqueued => proto::StepState::Enqueued,
            WorkflowStepState::InProgress => proto::StepState::InProgress,
            WorkflowStepState::EnqueuedForOrchestration => proto::StepState::EnqueuedForOrchestration,
            WorkflowStepState::EnqueuedAsErrorForOrchestration => proto::StepState::EnqueuedAsErrorForOrchestration,
            WorkflowStepState::WaitingForRetry => proto::StepState::WaitingForRetry,
            WorkflowStepState::Complete => proto::StepState::Complete,
            WorkflowStepState::Error => proto::StepState::Error,
            WorkflowStepState::Cancelled => proto::StepState::Cancelled,
            WorkflowStepState::ResolvedManually => proto::StepState::ResolvedManually,
        }
    }
}

impl TryFrom<proto::StepState> for WorkflowStepState {
    type Error = ();

    fn try_from(state: proto::StepState) -> Result<Self, <Self as TryFrom<proto::StepState>>::Error> {
        match state {
            proto::StepState::Pending => Ok(WorkflowStepState::Pending),
            proto::StepState::Enqueued => Ok(WorkflowStepState::Enqueued),
            proto::StepState::InProgress => Ok(WorkflowStepState::InProgress),
            proto::StepState::EnqueuedForOrchestration => Ok(WorkflowStepState::EnqueuedForOrchestration),
            proto::StepState::EnqueuedAsErrorForOrchestration => Ok(WorkflowStepState::EnqueuedAsErrorForOrchestration),
            proto::StepState::WaitingForRetry => Ok(WorkflowStepState::WaitingForRetry),
            proto::StepState::Complete => Ok(WorkflowStepState::Complete),
            proto::StepState::Error => Ok(WorkflowStepState::Error),
            proto::StepState::Cancelled => Ok(WorkflowStepState::Cancelled),
            proto::StepState::ResolvedManually => Ok(WorkflowStepState::ResolvedManually),
            proto::StepState::Unspecified => Err(()),
        }
    }
}

// ============================================================================
// TaskResponse -> proto::Task Conversion
// ============================================================================

/// Convert serde_json::Value to prost_types::Struct
fn json_to_proto_struct(value: &serde_json::Value) -> Option<prost_types::Struct> {
    if value.is_null() {
        return None;
    }
    // Convert JSON object to proto Struct
    if let serde_json::Value::Object(map) = value {
        let fields = map
            .iter()
            .filter_map(|(k, v)| json_value_to_proto_value(v).map(|pv| (k.clone(), pv)))
            .collect();
        Some(prost_types::Struct { fields })
    } else {
        None
    }
}

/// Convert a serde_json::Value to a prost_types::Value
fn json_value_to_proto_value(value: &serde_json::Value) -> Option<prost_types::Value> {
    use prost_types::value::Kind;

    let kind = match value {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => {
            Kind::NumberValue(n.as_f64().unwrap_or(0.0))
        }
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => {
            let values: Vec<prost_types::Value> = arr
                .iter()
                .filter_map(json_value_to_proto_value)
                .collect();
            Kind::ListValue(prost_types::ListValue { values })
        }
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .filter_map(|(k, v)| json_value_to_proto_value(v).map(|pv| (k.clone(), pv)))
                .collect();
            Kind::StructValue(prost_types::Struct { fields })
        }
    };

    Some(prost_types::Value { kind: Some(kind) })
}

impl From<&TaskResponse> for proto::Task {
    fn from(response: &TaskResponse) -> Self {
        // Convert status string to proto TaskState
        let state = TaskState::try_from(response.status.as_str())
            .map(proto::TaskState::from)
            .unwrap_or(proto::TaskState::Unspecified);

        proto::Task {
            task_uuid: response.task_uuid.clone(),
            name: response.name.clone(),
            namespace: response.namespace.clone(),
            version: response.version.clone(),
            state: state as i32,
            created_at: Some(datetime_to_timestamp(response.created_at)),
            updated_at: Some(datetime_to_timestamp(response.updated_at)),
            completed_at: response.completed_at.map(datetime_to_timestamp),
            context: json_to_proto_struct(&response.context),
            initiator: response.initiator.clone(),
            source_system: response.source_system.clone(),
            reason: response.reason.clone(),
            priority: response.priority,
            tags: response.tags.clone().unwrap_or_default(),
            correlation_id: response.correlation_id.to_string(),
            parent_correlation_id: response.parent_correlation_id.map(|u| u.to_string()),
            total_steps: response.total_steps,
            pending_steps: response.pending_steps,
            in_progress_steps: response.in_progress_steps,
            completed_steps: response.completed_steps,
            failed_steps: response.failed_steps,
            ready_steps: response.ready_steps,
            execution_status: response.execution_status.clone(),
            recommended_action: response.recommended_action.clone(),
            completion_percentage: response.completion_percentage,
            health_status: response.health_status.clone(),
        }
    }
}

impl From<TaskResponse> for proto::Task {
    fn from(response: TaskResponse) -> Self {
        proto::Task::from(&response)
    }
}

// ============================================================================
// StepResponse -> proto::Step Conversion
// ============================================================================

/// Helper function to parse RFC3339 timestamp string
fn parse_rfc3339_timestamp(s: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| datetime_to_timestamp(dt.with_timezone(&Utc)))
}

impl From<&StepResponse> for proto::Step {
    fn from(response: &StepResponse) -> Self {
        // Convert current_state string to proto StepState
        let state = WorkflowStepState::try_from(response.current_state.as_str())
            .map(proto::StepState::from)
            .unwrap_or(proto::StepState::Unspecified);

        proto::Step {
            step_uuid: response.step_uuid.clone(),
            task_uuid: response.task_uuid.clone(),
            name: response.name.clone(),
            state: state as i32,
            created_at: parse_rfc3339_timestamp(&response.created_at),
            updated_at: parse_rfc3339_timestamp(&response.updated_at),
            completed_at: response
                .completed_at
                .as_ref()
                .and_then(|s| parse_rfc3339_timestamp(s)),
            results: response.results.as_ref().and_then(json_to_proto_struct),
            dependencies_satisfied: response.dependencies_satisfied,
            retry_eligible: response.retry_eligible,
            ready_for_execution: response.ready_for_execution,
            total_parents: response.total_parents,
            completed_parents: response.completed_parents,
            attempts: response.attempts,
            max_attempts: response.max_attempts,
            last_failure_at: response
                .last_failure_at
                .as_ref()
                .and_then(|s| parse_rfc3339_timestamp(s)),
            next_retry_at: response
                .next_retry_at
                .as_ref()
                .and_then(|s| parse_rfc3339_timestamp(s)),
            last_attempted_at: response
                .last_attempted_at
                .as_ref()
                .and_then(|s| parse_rfc3339_timestamp(s)),
            backoff_request_seconds: None, // Not present in StepResponse
        }
    }
}

impl From<StepResponse> for proto::Step {
    fn from(response: StepResponse) -> Self {
        proto::Step::from(&response)
    }
}

// ============================================================================
// StepAuditResponse -> proto::StepAuditRecord Conversion
// ============================================================================

impl From<&StepAuditResponse> for proto::StepAuditRecord {
    fn from(response: &StepAuditResponse) -> Self {
        // Parse from_state if present
        let from_state = response.from_state.as_ref().and_then(|s| {
            WorkflowStepState::try_from(s.as_str())
                .map(|state| proto::StepState::from(state) as i32)
                .ok()
        });

        // Parse to_state
        let to_state = WorkflowStepState::try_from(response.to_state.as_str())
            .map(proto::StepState::from)
            .unwrap_or(proto::StepState::Unspecified);

        proto::StepAuditRecord {
            audit_uuid: response.audit_uuid.clone(),
            step_uuid: response.workflow_step_uuid.clone(),
            transition_uuid: response.transition_uuid.clone(),
            task_uuid: response.task_uuid.clone(),
            recorded_at: parse_rfc3339_timestamp(&response.recorded_at),
            worker_uuid: response.worker_uuid.clone(),
            correlation_id: response.correlation_id.clone(),
            success: response.success,
            execution_time_ms: response.execution_time_ms,
            result: response.result.as_ref().and_then(json_to_proto_struct),
            step_name: response.step_name.clone(),
            from_state,
            to_state: to_state as i32,
        }
    }
}

impl From<StepAuditResponse> for proto::StepAuditRecord {
    fn from(response: StepAuditResponse) -> Self {
        proto::StepAuditRecord::from(&response)
    }
}

// ============================================================================
// Health Response Conversions (TAS-177)
// ============================================================================

use crate::types::api::orchestration::{
    DetailedHealthChecks, DetailedHealthResponse, HealthCheck, HealthInfo, HealthResponse,
    PoolUtilizationInfo, ReadinessChecks, ReadinessResponse,
};

impl From<&HealthResponse> for proto::HealthResponse {
    fn from(response: &HealthResponse) -> Self {
        proto::HealthResponse {
            status: response.status.clone(),
            timestamp: response.timestamp.clone(),
        }
    }
}

impl From<&ReadinessResponse> for proto::ReadinessResponse {
    fn from(response: &ReadinessResponse) -> Self {
        proto::ReadinessResponse {
            status: response.status.clone(),
            timestamp: response.timestamp.clone(),
            checks: Some(proto::ReadinessChecks::from(&response.checks)),
            info: Some(proto::HealthInfo::from(&response.info)),
        }
    }
}

impl From<&DetailedHealthResponse> for proto::DetailedHealthResponse {
    fn from(response: &DetailedHealthResponse) -> Self {
        proto::DetailedHealthResponse {
            status: response.status.clone(),
            timestamp: response.timestamp.clone(),
            checks: Some(proto::DetailedHealthChecks::from(&response.checks)),
            info: Some(proto::HealthInfo::from(&response.info)),
        }
    }
}

impl From<&ReadinessChecks> for proto::ReadinessChecks {
    fn from(checks: &ReadinessChecks) -> Self {
        proto::ReadinessChecks {
            web_database: Some(proto::HealthCheck::from(&checks.web_database)),
            orchestration_database: Some(proto::HealthCheck::from(&checks.orchestration_database)),
            circuit_breaker: Some(proto::HealthCheck::from(&checks.circuit_breaker)),
            orchestration_system: Some(proto::HealthCheck::from(&checks.orchestration_system)),
            command_processor: Some(proto::HealthCheck::from(&checks.command_processor)),
        }
    }
}

impl From<&DetailedHealthChecks> for proto::DetailedHealthChecks {
    fn from(checks: &DetailedHealthChecks) -> Self {
        proto::DetailedHealthChecks {
            web_database: Some(proto::HealthCheck::from(&checks.web_database)),
            orchestration_database: Some(proto::HealthCheck::from(&checks.orchestration_database)),
            circuit_breaker: Some(proto::HealthCheck::from(&checks.circuit_breaker)),
            orchestration_system: Some(proto::HealthCheck::from(&checks.orchestration_system)),
            command_processor: Some(proto::HealthCheck::from(&checks.command_processor)),
            pool_utilization: Some(proto::HealthCheck::from(&checks.pool_utilization)),
            queue_depth: Some(proto::HealthCheck::from(&checks.queue_depth)),
            channel_saturation: Some(proto::HealthCheck::from(&checks.channel_saturation)),
        }
    }
}

impl From<&HealthCheck> for proto::HealthCheck {
    fn from(check: &HealthCheck) -> Self {
        proto::HealthCheck {
            status: check.status.clone(),
            message: check.message.clone(),
            duration_ms: check.duration_ms,
        }
    }
}

impl From<&HealthInfo> for proto::HealthInfo {
    fn from(info: &HealthInfo) -> Self {
        proto::HealthInfo {
            version: info.version.clone(),
            environment: info.environment.clone(),
            operational_state: info.operational_state.clone(),
            web_database_pool_size: info.web_database_pool_size,
            orchestration_database_pool_size: info.orchestration_database_pool_size,
            circuit_breaker_state: info.circuit_breaker_state.clone(),
            pool_utilization: info.pool_utilization.as_ref().map(proto::PoolUtilizationInfo::from),
        }
    }
}

impl From<&PoolUtilizationInfo> for proto::PoolUtilizationInfo {
    fn from(info: &PoolUtilizationInfo) -> Self {
        // Convert domain pools (tasker_pool, pgmq_pool) to proto repeated pools
        let pools = vec![
            proto::PoolDetail {
                name: "tasker".to_string(),
                active: info.tasker_pool.active_connections,
                idle: info.tasker_pool.idle_connections,
                max: info.tasker_pool.max_connections,
                utilization: info.tasker_pool.utilization_percent / 100.0,
            },
            proto::PoolDetail {
                name: "pgmq".to_string(),
                active: info.pgmq_pool.active_connections,
                idle: info.pgmq_pool.idle_connections,
                max: info.pgmq_pool.max_connections,
                utilization: info.pgmq_pool.utilization_percent / 100.0,
            },
        ];

        // Determine overall health from pool utilizations
        let max_utilization = info.tasker_pool.utilization_percent.max(info.pgmq_pool.utilization_percent);
        let overall_health = if max_utilization >= 90.0 {
            "critical".to_string()
        } else if max_utilization >= 75.0 {
            "warning".to_string()
        } else {
            "healthy".to_string()
        };

        proto::PoolUtilizationInfo {
            pools,
            overall_health,
        }
    }
}

// ============================================================================
// Tests - Verify lossless roundtrip conversions
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify all TaskState variants roundtrip correctly through proto conversion
    #[test]
    fn test_task_state_roundtrip_all_variants() {
        let all_states = [
            TaskState::Pending,
            TaskState::Initializing,
            TaskState::EnqueuingSteps,
            TaskState::StepsInProcess,
            TaskState::EvaluatingResults,
            TaskState::WaitingForDependencies,
            TaskState::WaitingForRetry,
            TaskState::BlockedByFailures,
            TaskState::Complete,
            TaskState::Error,
            TaskState::Cancelled,
            TaskState::ResolvedManually,
        ];

        for original in all_states {
            let proto_state: proto::TaskState = original.into();
            let roundtrip: TaskState = proto_state
                .try_into()
                .unwrap_or_else(|()| panic!("Failed to convert proto state back for {original:?}"));

            assert_eq!(
                original, roundtrip,
                "TaskState roundtrip failed: {original:?} -> {proto_state:?} -> {roundtrip:?}"
            );
        }
    }

    /// Verify all WorkflowStepState variants roundtrip correctly through proto conversion
    #[test]
    fn test_step_state_roundtrip_all_variants() {
        let all_states = [
            WorkflowStepState::Pending,
            WorkflowStepState::Enqueued,
            WorkflowStepState::InProgress,
            WorkflowStepState::EnqueuedForOrchestration,
            WorkflowStepState::EnqueuedAsErrorForOrchestration,
            WorkflowStepState::WaitingForRetry,
            WorkflowStepState::Complete,
            WorkflowStepState::Error,
            WorkflowStepState::Cancelled,
            WorkflowStepState::ResolvedManually,
        ];

        for original in all_states {
            let proto_state: proto::StepState = original.into();
            let roundtrip: WorkflowStepState = proto_state
                .try_into()
                .unwrap_or_else(|()| panic!("Failed to convert proto state back for {original:?}"));

            assert_eq!(
                original, roundtrip,
                "WorkflowStepState roundtrip failed: {original:?} -> {proto_state:?} -> {roundtrip:?}"
            );
        }
    }

    /// Verify Unspecified proto states return Err
    #[test]
    fn test_unspecified_states_return_error() {
        let task_result: Result<TaskState, ()> = proto::TaskState::Unspecified.try_into();
        assert!(task_result.is_err(), "Unspecified TaskState should return Err");

        let step_result: Result<WorkflowStepState, ()> = proto::StepState::Unspecified.try_into();
        assert!(step_result.is_err(), "Unspecified StepState should return Err");
    }

    /// Verify timestamp roundtrip preserves precision
    #[test]
    fn test_timestamp_roundtrip() {
        let original = Utc::now();
        let proto_ts = datetime_to_timestamp(original);
        let roundtrip = timestamp_to_datetime(proto_ts);

        // Compare with millisecond precision (proto timestamps have nanosecond precision)
        assert_eq!(
            original.timestamp_millis(),
            roundtrip.timestamp_millis(),
            "Timestamp roundtrip should preserve millisecond precision"
        );
    }

    /// Verify each domain state maps to a distinct proto state (no collisions)
    #[test]
    fn test_task_state_no_collisions() {
        use std::collections::HashSet;

        let all_states = [
            TaskState::Pending,
            TaskState::Initializing,
            TaskState::EnqueuingSteps,
            TaskState::StepsInProcess,
            TaskState::EvaluatingResults,
            TaskState::WaitingForDependencies,
            TaskState::WaitingForRetry,
            TaskState::BlockedByFailures,
            TaskState::Complete,
            TaskState::Error,
            TaskState::Cancelled,
            TaskState::ResolvedManually,
        ];

        let proto_states: Vec<proto::TaskState> = all_states.iter().map(|&s| s.into()).collect();
        let unique: HashSet<i32> = proto_states.iter().map(|&s| s as i32).collect();

        assert_eq!(
            proto_states.len(),
            unique.len(),
            "Each TaskState should map to a unique proto::TaskState"
        );
    }

    /// Verify each domain step state maps to a distinct proto state (no collisions)
    #[test]
    fn test_step_state_no_collisions() {
        use std::collections::HashSet;

        let all_states = [
            WorkflowStepState::Pending,
            WorkflowStepState::Enqueued,
            WorkflowStepState::InProgress,
            WorkflowStepState::EnqueuedForOrchestration,
            WorkflowStepState::EnqueuedAsErrorForOrchestration,
            WorkflowStepState::WaitingForRetry,
            WorkflowStepState::Complete,
            WorkflowStepState::Error,
            WorkflowStepState::Cancelled,
            WorkflowStepState::ResolvedManually,
        ];

        let proto_states: Vec<proto::StepState> = all_states.iter().map(|&s| s.into()).collect();
        let unique: HashSet<i32> = proto_states.iter().map(|&s| s as i32).collect();

        assert_eq!(
            proto_states.len(),
            unique.len(),
            "Each WorkflowStepState should map to a unique proto::StepState"
        );
    }
}

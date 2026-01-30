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

    fn try_from(
        state: proto::TaskState,
    ) -> Result<Self, <Self as TryFrom<proto::TaskState>>::Error> {
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
            WorkflowStepState::EnqueuedForOrchestration => {
                proto::StepState::EnqueuedForOrchestration
            }
            WorkflowStepState::EnqueuedAsErrorForOrchestration => {
                proto::StepState::EnqueuedAsErrorForOrchestration
            }
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

    fn try_from(
        state: proto::StepState,
    ) -> Result<Self, <Self as TryFrom<proto::StepState>>::Error> {
        match state {
            proto::StepState::Pending => Ok(WorkflowStepState::Pending),
            proto::StepState::Enqueued => Ok(WorkflowStepState::Enqueued),
            proto::StepState::InProgress => Ok(WorkflowStepState::InProgress),
            proto::StepState::EnqueuedForOrchestration => {
                Ok(WorkflowStepState::EnqueuedForOrchestration)
            }
            proto::StepState::EnqueuedAsErrorForOrchestration => {
                Ok(WorkflowStepState::EnqueuedAsErrorForOrchestration)
            }
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
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => {
            let values: Vec<prost_types::Value> =
                arr.iter().filter_map(json_value_to_proto_value).collect();
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
    PoolDetail, PoolUtilizationInfo, ReadinessChecks, ReadinessResponse,
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
            pool_utilization: info
                .pool_utilization
                .as_ref()
                .map(proto::PoolUtilizationInfo::from),
        }
    }
}

impl From<&PoolUtilizationInfo> for proto::PoolUtilizationInfo {
    fn from(info: &PoolUtilizationInfo) -> Self {
        proto::PoolUtilizationInfo {
            tasker_pool: Some(proto::PoolDetail::from(&info.tasker_pool)),
            pgmq_pool: Some(proto::PoolDetail::from(&info.pgmq_pool)),
        }
    }
}

impl From<&PoolDetail> for proto::PoolDetail {
    fn from(pool: &PoolDetail) -> Self {
        proto::PoolDetail {
            active_connections: pool.active_connections,
            idle_connections: pool.idle_connections,
            max_connections: pool.max_connections,
            utilization_percent: pool.utilization_percent,
            total_acquires: pool.total_acquires,
            slow_acquires: pool.slow_acquires,
            acquire_errors: pool.acquire_errors,
            average_acquire_time_ms: pool.average_acquire_time_ms,
            max_acquire_time_ms: pool.max_acquire_time_ms,
        }
    }
}

// ============================================================================
// Worker Type Conversions (TAS-177)
// ============================================================================

use crate::types::api::health::PoolUtilizationInfo as DomainPoolUtilizationInfo;
use crate::types::api::orchestration::{
    ConfigMetadata, SafeAuthConfig, SafeMessagingConfig, WorkerConfigResponse,
};
use crate::types::api::worker::{
    BasicHealthResponse as WorkerBasicHealth, DetailedHealthResponse as WorkerDetailedHealth,
    DistributedCacheInfo, HealthCheck as WorkerDomainHealthCheck,
    ReadinessResponse as WorkerReadiness, TemplateListResponse as WorkerTemplateList,
    TemplateResponse as WorkerTemplate, WorkerDetailedChecks, WorkerReadinessChecks,
    WorkerSystemInfo,
};
use crate::types::base::{CacheStats, HandlerMetadata};

// Worker Config Response Conversions

impl From<&WorkerConfigResponse> for proto::WorkerGetConfigResponse {
    fn from(response: &WorkerConfigResponse) -> Self {
        proto::WorkerGetConfigResponse {
            metadata: Some(proto::ConfigMetadata::from(&response.metadata)),
            worker_id: response.worker_id.clone(),
            worker_type: response.worker_type.clone(),
            auth: Some(proto::SafeAuthConfig::from(&response.auth)),
            messaging: Some(proto::SafeMessagingConfig::from(&response.messaging)),
        }
    }
}

impl From<WorkerConfigResponse> for proto::WorkerGetConfigResponse {
    fn from(response: WorkerConfigResponse) -> Self {
        proto::WorkerGetConfigResponse::from(&response)
    }
}

impl From<&ConfigMetadata> for proto::ConfigMetadata {
    fn from(metadata: &ConfigMetadata) -> Self {
        proto::ConfigMetadata {
            timestamp: Some(datetime_to_timestamp(metadata.timestamp)),
            environment: metadata.environment.clone(),
            version: metadata.version.clone(),
        }
    }
}

impl From<&SafeAuthConfig> for proto::SafeAuthConfig {
    fn from(config: &SafeAuthConfig) -> Self {
        proto::SafeAuthConfig {
            enabled: config.enabled,
            verification_method: config.verification_method.clone(),
            jwt_issuer: config.jwt_issuer.clone(),
            jwt_audience: config.jwt_audience.clone(),
            api_key_header: config.api_key_header.clone(),
            api_key_count: config.api_key_count as i32,
            strict_validation: config.strict_validation,
            allowed_algorithms: config.allowed_algorithms.clone(),
        }
    }
}

impl From<&SafeMessagingConfig> for proto::SafeMessagingConfig {
    fn from(config: &SafeMessagingConfig) -> Self {
        proto::SafeMessagingConfig {
            backend: config.backend.clone(),
            queues: config.queues.clone(),
        }
    }
}

// Worker Health Response Conversions

impl From<&WorkerBasicHealth> for proto::WorkerBasicHealthResponse {
    fn from(response: &WorkerBasicHealth) -> Self {
        proto::WorkerBasicHealthResponse {
            status: response.status.clone(),
            timestamp: Some(datetime_to_timestamp(response.timestamp)),
            worker_id: response.worker_id.clone(),
        }
    }
}

impl From<WorkerBasicHealth> for proto::WorkerBasicHealthResponse {
    fn from(response: WorkerBasicHealth) -> Self {
        proto::WorkerBasicHealthResponse::from(&response)
    }
}

impl From<&WorkerReadiness> for proto::WorkerReadinessResponse {
    fn from(response: &WorkerReadiness) -> Self {
        proto::WorkerReadinessResponse {
            status: response.status.clone(),
            timestamp: Some(datetime_to_timestamp(response.timestamp)),
            worker_id: response.worker_id.clone(),
            checks: Some(proto::WorkerReadinessChecks::from(&response.checks)),
            system_info: Some(proto::WorkerSystemInfo::from(&response.system_info)),
        }
    }
}

impl From<WorkerReadiness> for proto::WorkerReadinessResponse {
    fn from(response: WorkerReadiness) -> Self {
        proto::WorkerReadinessResponse::from(&response)
    }
}

impl From<&WorkerDetailedHealth> for proto::WorkerDetailedHealthResponse {
    fn from(response: &WorkerDetailedHealth) -> Self {
        proto::WorkerDetailedHealthResponse {
            status: response.status.clone(),
            timestamp: Some(datetime_to_timestamp(response.timestamp)),
            worker_id: response.worker_id.clone(),
            checks: Some(proto::WorkerDetailedChecks::from(&response.checks)),
            system_info: Some(proto::WorkerSystemInfo::from(&response.system_info)),
            distributed_cache: response
                .distributed_cache
                .as_ref()
                .map(proto::DistributedCacheInfo::from),
        }
    }
}

impl From<WorkerDetailedHealth> for proto::WorkerDetailedHealthResponse {
    fn from(response: WorkerDetailedHealth) -> Self {
        proto::WorkerDetailedHealthResponse::from(&response)
    }
}

impl From<&WorkerReadinessChecks> for proto::WorkerReadinessChecks {
    fn from(checks: &WorkerReadinessChecks) -> Self {
        proto::WorkerReadinessChecks {
            database: Some(proto::WorkerHealthCheck::from(&checks.database)),
            command_processor: Some(proto::WorkerHealthCheck::from(&checks.command_processor)),
            queue_processing: Some(proto::WorkerHealthCheck::from(&checks.queue_processing)),
        }
    }
}

impl From<&WorkerDetailedChecks> for proto::WorkerDetailedChecks {
    fn from(checks: &WorkerDetailedChecks) -> Self {
        proto::WorkerDetailedChecks {
            database: Some(proto::WorkerHealthCheck::from(&checks.database)),
            command_processor: Some(proto::WorkerHealthCheck::from(&checks.command_processor)),
            queue_processing: Some(proto::WorkerHealthCheck::from(&checks.queue_processing)),
            event_system: Some(proto::WorkerHealthCheck::from(&checks.event_system)),
            step_processing: Some(proto::WorkerHealthCheck::from(&checks.step_processing)),
            circuit_breakers: Some(proto::WorkerHealthCheck::from(&checks.circuit_breakers)),
        }
    }
}

impl From<&WorkerDomainHealthCheck> for proto::WorkerHealthCheck {
    fn from(check: &WorkerDomainHealthCheck) -> Self {
        proto::WorkerHealthCheck {
            status: check.status.clone(),
            message: check.message.clone(),
            duration_ms: check.duration_ms,
            last_checked: Some(datetime_to_timestamp(check.last_checked)),
        }
    }
}

impl From<&WorkerSystemInfo> for proto::WorkerSystemInfo {
    fn from(info: &WorkerSystemInfo) -> Self {
        proto::WorkerSystemInfo {
            version: info.version.clone(),
            environment: info.environment.clone(),
            uptime_seconds: info.uptime_seconds,
            worker_type: info.worker_type.clone(),
            database_pool_size: info.database_pool_size,
            command_processor_active: info.command_processor_active,
            supported_namespaces: info.supported_namespaces.clone(),
            pool_utilization: info
                .pool_utilization
                .as_ref()
                .map(proto::WorkerPoolUtilizationInfo::from),
        }
    }
}

impl From<&DomainPoolUtilizationInfo> for proto::WorkerPoolUtilizationInfo {
    fn from(info: &DomainPoolUtilizationInfo) -> Self {
        proto::WorkerPoolUtilizationInfo {
            tasker_pool: Some(proto::WorkerPoolDetail {
                active_connections: info.tasker_pool.active_connections,
                idle_connections: info.tasker_pool.idle_connections,
                max_connections: info.tasker_pool.max_connections,
                utilization_percent: info.tasker_pool.utilization_percent,
                total_acquires: info.tasker_pool.total_acquires,
                slow_acquires: info.tasker_pool.slow_acquires,
                acquire_errors: info.tasker_pool.acquire_errors,
                average_acquire_time_ms: info.tasker_pool.average_acquire_time_ms,
                max_acquire_time_ms: info.tasker_pool.max_acquire_time_ms,
            }),
            pgmq_pool: Some(proto::WorkerPoolDetail {
                active_connections: info.pgmq_pool.active_connections,
                idle_connections: info.pgmq_pool.idle_connections,
                max_connections: info.pgmq_pool.max_connections,
                utilization_percent: info.pgmq_pool.utilization_percent,
                total_acquires: info.pgmq_pool.total_acquires,
                slow_acquires: info.pgmq_pool.slow_acquires,
                acquire_errors: info.pgmq_pool.acquire_errors,
                average_acquire_time_ms: info.pgmq_pool.average_acquire_time_ms,
                max_acquire_time_ms: info.pgmq_pool.max_acquire_time_ms,
            }),
        }
    }
}

impl From<&DistributedCacheInfo> for proto::DistributedCacheInfo {
    fn from(info: &DistributedCacheInfo) -> Self {
        proto::DistributedCacheInfo {
            enabled: info.enabled,
            provider: info.provider.clone(),
            healthy: info.healthy,
        }
    }
}

// Worker Template Response Conversions

impl From<&WorkerTemplateList> for proto::WorkerTemplateListResponse {
    fn from(response: &WorkerTemplateList) -> Self {
        proto::WorkerTemplateListResponse {
            supported_namespaces: response.supported_namespaces.clone(),
            template_count: response.template_count as i64,
            cache_stats: response.cache_stats.as_ref().map(proto::CacheStats::from),
            worker_capabilities: response.worker_capabilities.clone(),
        }
    }
}

impl From<WorkerTemplateList> for proto::WorkerTemplateListResponse {
    fn from(response: WorkerTemplateList) -> Self {
        proto::WorkerTemplateListResponse::from(&response)
    }
}

impl From<&CacheStats> for proto::CacheStats {
    fn from(stats: &CacheStats) -> Self {
        proto::CacheStats {
            total_cached: stats.total_cached as u64,
            cache_hits: stats.cache_hits,
            cache_misses: stats.cache_misses,
            cache_evictions: stats.cache_evictions,
            oldest_entry_age_seconds: stats.oldest_entry_age_seconds,
            average_access_count: stats.average_access_count,
            supported_namespaces: stats.supported_namespaces.clone(),
        }
    }
}

impl From<&WorkerTemplate> for proto::WorkerTemplateResponse {
    fn from(response: &WorkerTemplate) -> Self {
        let resolved = &response.template;
        let template = &resolved.template;

        proto::WorkerTemplateResponse {
            template: Some(proto::WorkerResolvedTemplate {
                name: template.name.clone(),
                namespace: template.namespace_name.clone(),
                version: template.version.clone(),
                description: template.description.clone(),
                steps: template
                    .steps
                    .iter()
                    .map(|step| proto::WorkerStepDefinition {
                        name: step.name.clone(),
                        description: step.description.clone(),
                        retryable: step.retry.retryable,
                        max_attempts: step.retry.max_attempts as i32,
                    })
                    .collect(),
            }),
            handler_metadata: Some(proto::WorkerHandlerMetadata::from(
                &response.handler_metadata,
            )),
            cached: response.cached,
            cache_age_seconds: response.cache_age_seconds,
            access_count: response.access_count,
        }
    }
}

impl From<WorkerTemplate> for proto::WorkerTemplateResponse {
    fn from(response: WorkerTemplate) -> Self {
        proto::WorkerTemplateResponse::from(&response)
    }
}

impl From<&HandlerMetadata> for proto::WorkerHandlerMetadata {
    fn from(metadata: &HandlerMetadata) -> Self {
        proto::WorkerHandlerMetadata {
            namespace: metadata.namespace.clone(),
            handler_name: metadata.name.clone(),
            version: metadata.version.clone(),
            description: None,  // HandlerMetadata doesn't have description
            step_names: vec![], // Would need to be populated from template
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
        assert!(
            task_result.is_err(),
            "Unspecified TaskState should return Err"
        );

        let step_result: Result<WorkflowStepState, ()> = proto::StepState::Unspecified.try_into();
        assert!(
            step_result.is_err(),
            "Unspecified StepState should return Err"
        );
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

    // ========================================================================
    // json_to_proto_struct tests
    // ========================================================================

    #[test]
    fn test_json_to_proto_struct_null_returns_none() {
        let result = json_to_proto_struct(&serde_json::Value::Null);
        assert!(result.is_none(), "Null JSON should return None");
    }

    #[test]
    fn test_json_to_proto_struct_empty_object() {
        let value = serde_json::json!({});
        let result = json_to_proto_struct(&value);
        assert!(result.is_some(), "Empty object should return Some");
        let s = result.unwrap();
        assert!(s.fields.is_empty(), "Empty object should have no fields");
    }

    #[test]
    fn test_json_to_proto_struct_nested_object() {
        let value = serde_json::json!({
            "name": "test",
            "count": 42,
            "active": true,
            "nested": { "inner": "value" }
        });
        let result = json_to_proto_struct(&value);
        assert!(result.is_some(), "Object should return Some");
        let s = result.unwrap();
        assert_eq!(s.fields.len(), 4, "Should have 4 fields");
        assert!(s.fields.contains_key("name"));
        assert!(s.fields.contains_key("count"));
        assert!(s.fields.contains_key("active"));
        assert!(s.fields.contains_key("nested"));
    }

    #[test]
    fn test_json_to_proto_struct_non_object_returns_none() {
        // Array is not an object
        let array = serde_json::json!([1, 2, 3]);
        assert!(
            json_to_proto_struct(&array).is_none(),
            "Array should return None"
        );

        // String is not an object
        let string = serde_json::json!("hello");
        assert!(
            json_to_proto_struct(&string).is_none(),
            "String should return None"
        );

        // Number is not an object
        let number = serde_json::json!(42);
        assert!(
            json_to_proto_struct(&number).is_none(),
            "Number should return None"
        );

        // Bool is not an object
        let boolean = serde_json::json!(true);
        assert!(
            json_to_proto_struct(&boolean).is_none(),
            "Bool should return None"
        );
    }

    // ========================================================================
    // json_value_to_proto_value tests
    // ========================================================================

    #[test]
    fn test_json_value_to_proto_value_null() {
        use prost_types::value::Kind;

        let result = json_value_to_proto_value(&serde_json::Value::Null);
        assert!(result.is_some());
        let v = result.unwrap();
        assert!(
            matches!(v.kind, Some(Kind::NullValue(0))),
            "Null JSON should produce NullValue"
        );
    }

    #[test]
    fn test_json_value_to_proto_value_bool() {
        use prost_types::value::Kind;

        let result = json_value_to_proto_value(&serde_json::json!(true));
        assert!(result.is_some());
        let v = result.unwrap();
        assert!(
            matches!(v.kind, Some(Kind::BoolValue(true))),
            "true should produce BoolValue(true)"
        );

        let result_false = json_value_to_proto_value(&serde_json::json!(false));
        let vf = result_false.unwrap();
        assert!(
            matches!(vf.kind, Some(Kind::BoolValue(false))),
            "false should produce BoolValue(false)"
        );
    }

    #[test]
    fn test_json_value_to_proto_value_number() {
        use prost_types::value::Kind;

        let result = json_value_to_proto_value(&serde_json::json!(2.72));
        assert!(result.is_some());
        let v = result.unwrap();
        match v.kind {
            Some(Kind::NumberValue(n)) => {
                assert!(
                    (n - 2.72).abs() < f64::EPSILON,
                    "Number should be close to 2.72"
                );
            }
            _ => panic!("Expected NumberValue"),
        }

        // Integer
        let result_int = json_value_to_proto_value(&serde_json::json!(42));
        let vi = result_int.unwrap();
        match vi.kind {
            Some(Kind::NumberValue(n)) => {
                assert!(
                    (n - 42.0).abs() < f64::EPSILON,
                    "Integer should convert to 42.0"
                );
            }
            _ => panic!("Expected NumberValue for integer"),
        }
    }

    #[test]
    fn test_json_value_to_proto_value_string() {
        use prost_types::value::Kind;

        let result = json_value_to_proto_value(&serde_json::json!("hello world"));
        assert!(result.is_some());
        let v = result.unwrap();
        match v.kind {
            Some(Kind::StringValue(ref s)) => {
                assert_eq!(s, "hello world");
            }
            _ => panic!("Expected StringValue"),
        }
    }

    #[test]
    fn test_json_value_to_proto_value_array() {
        use prost_types::value::Kind;

        let result = json_value_to_proto_value(&serde_json::json!([1, "two", true]));
        assert!(result.is_some());
        let v = result.unwrap();
        match v.kind {
            Some(Kind::ListValue(list)) => {
                assert_eq!(list.values.len(), 3, "Array should have 3 elements");
            }
            _ => panic!("Expected ListValue"),
        }
    }

    #[test]
    fn test_json_value_to_proto_value_nested_object() {
        use prost_types::value::Kind;

        let result = json_value_to_proto_value(&serde_json::json!({"key": "value"}));
        assert!(result.is_some());
        let v = result.unwrap();
        match v.kind {
            Some(Kind::StructValue(s)) => {
                assert_eq!(s.fields.len(), 1, "Object should have 1 field");
                assert!(s.fields.contains_key("key"));
            }
            _ => panic!("Expected StructValue"),
        }
    }

    // ========================================================================
    // parse_rfc3339_timestamp tests
    // ========================================================================

    #[test]
    fn test_parse_rfc3339_timestamp_valid() {
        let result = parse_rfc3339_timestamp("2024-01-15T10:30:00Z");
        assert!(
            result.is_some(),
            "Valid RFC3339 timestamp should parse successfully"
        );
        let ts = result.unwrap();
        assert!(ts.seconds > 0, "Seconds should be positive for a 2024 date");
    }

    #[test]
    fn test_parse_rfc3339_timestamp_invalid() {
        let result = parse_rfc3339_timestamp("not-a-timestamp");
        assert!(result.is_none(), "Invalid string should return None");

        let result_empty = parse_rfc3339_timestamp("");
        assert!(result_empty.is_none(), "Empty string should return None");
    }

    // ========================================================================
    // From<&TaskResponse> for proto::Task
    // ========================================================================

    #[test]
    fn test_task_response_to_proto_task() {
        use uuid::Uuid;

        let corr_id = Uuid::new_v4();
        let parent_corr_id = Uuid::new_v4();
        let now = Utc::now();

        let response = TaskResponse {
            task_uuid: "task-uuid-123".to_string(),
            name: "my_task".to_string(),
            namespace: "default".to_string(),
            version: "1.0.0".to_string(),
            status: "pending".to_string(),
            created_at: now,
            updated_at: now,
            completed_at: Some(now),
            context: serde_json::json!({"key": "value"}),
            initiator: "test-user".to_string(),
            source_system: "test-system".to_string(),
            reason: "testing".to_string(),
            priority: Some(2),
            tags: Some(vec!["tag1".to_string(), "tag2".to_string()]),
            correlation_id: corr_id,
            parent_correlation_id: Some(parent_corr_id),
            total_steps: 5,
            pending_steps: 2,
            in_progress_steps: 1,
            completed_steps: 2,
            failed_steps: 0,
            ready_steps: 1,
            execution_status: "processing".to_string(),
            recommended_action: "wait".to_string(),
            completion_percentage: 40.0,
            health_status: "healthy".to_string(),
            steps: vec![],
        };

        let proto_task = proto::Task::from(&response);

        assert_eq!(proto_task.task_uuid, "task-uuid-123");
        assert_eq!(proto_task.name, "my_task");
        assert_eq!(proto_task.namespace, "default");
        assert_eq!(proto_task.version, "1.0.0");
        assert_eq!(proto_task.state, proto::TaskState::Pending as i32);
        assert!(proto_task.created_at.is_some());
        assert!(proto_task.updated_at.is_some());
        assert!(proto_task.completed_at.is_some());
        assert!(proto_task.context.is_some());
        assert_eq!(proto_task.initiator, "test-user");
        assert_eq!(proto_task.source_system, "test-system");
        assert_eq!(proto_task.reason, "testing");
        assert_eq!(proto_task.priority, Some(2));
        assert_eq!(proto_task.tags, vec!["tag1", "tag2"]);
        assert_eq!(proto_task.correlation_id, corr_id.to_string());
        assert_eq!(
            proto_task.parent_correlation_id,
            Some(parent_corr_id.to_string())
        );
        assert_eq!(proto_task.total_steps, 5);
        assert_eq!(proto_task.pending_steps, 2);
        assert_eq!(proto_task.in_progress_steps, 1);
        assert_eq!(proto_task.completed_steps, 2);
        assert_eq!(proto_task.failed_steps, 0);
        assert_eq!(proto_task.ready_steps, 1);
        assert_eq!(proto_task.execution_status, "processing");
        assert_eq!(proto_task.recommended_action, "wait");
        assert!((proto_task.completion_percentage - 40.0).abs() < f64::EPSILON);
        assert_eq!(proto_task.health_status, "healthy");
    }

    // ========================================================================
    // From<&StepResponse> for proto::Step
    // ========================================================================

    #[test]
    fn test_step_response_to_proto_step() {
        let response = StepResponse {
            step_uuid: "step-uuid-456".to_string(),
            task_uuid: "task-uuid-123".to_string(),
            name: "validate_order".to_string(),
            created_at: "2024-01-15T10:30:00Z".to_string(),
            updated_at: "2024-01-15T10:31:00Z".to_string(),
            completed_at: Some("2024-01-15T10:32:00Z".to_string()),
            results: Some(serde_json::json!({"validated": true})),
            current_state: "complete".to_string(),
            dependencies_satisfied: true,
            retry_eligible: false,
            ready_for_execution: false,
            total_parents: 1,
            completed_parents: 1,
            attempts: 1,
            max_attempts: 3,
            last_failure_at: None,
            next_retry_at: None,
            last_attempted_at: Some("2024-01-15T10:30:30Z".to_string()),
        };

        let proto_step = proto::Step::from(&response);

        assert_eq!(proto_step.step_uuid, "step-uuid-456");
        assert_eq!(proto_step.task_uuid, "task-uuid-123");
        assert_eq!(proto_step.name, "validate_order");
        assert_eq!(proto_step.state, proto::StepState::Complete as i32);
        assert!(proto_step.created_at.is_some());
        assert!(proto_step.updated_at.is_some());
        assert!(proto_step.completed_at.is_some());
        assert!(proto_step.results.is_some());
        assert!(proto_step.dependencies_satisfied);
        assert!(!proto_step.retry_eligible);
        assert!(!proto_step.ready_for_execution);
        assert_eq!(proto_step.total_parents, 1);
        assert_eq!(proto_step.completed_parents, 1);
        assert_eq!(proto_step.attempts, 1);
        assert_eq!(proto_step.max_attempts, 3);
        assert!(proto_step.last_failure_at.is_none());
        assert!(proto_step.next_retry_at.is_none());
        assert!(proto_step.last_attempted_at.is_some());
    }

    // ========================================================================
    // From<&StepAuditResponse> for proto::StepAuditRecord
    // ========================================================================

    #[test]
    fn test_step_audit_response_to_proto() {
        let response = StepAuditResponse {
            audit_uuid: "audit-uuid-789".to_string(),
            workflow_step_uuid: "step-uuid-456".to_string(),
            transition_uuid: "transition-uuid-012".to_string(),
            task_uuid: "task-uuid-123".to_string(),
            recorded_at: "2024-01-15T10:30:00Z".to_string(),
            worker_uuid: Some("worker-abc".to_string()),
            correlation_id: Some("corr-xyz".to_string()),
            success: true,
            execution_time_ms: Some(150),
            result: Some(serde_json::json!({"output": "done"})),
            step_name: "validate_order".to_string(),
            from_state: Some("in_progress".to_string()),
            to_state: "enqueued_for_orchestration".to_string(),
        };

        let proto_audit = proto::StepAuditRecord::from(&response);

        assert_eq!(proto_audit.audit_uuid, "audit-uuid-789");
        assert_eq!(proto_audit.step_uuid, "step-uuid-456");
        assert_eq!(proto_audit.transition_uuid, "transition-uuid-012");
        assert_eq!(proto_audit.task_uuid, "task-uuid-123");
        assert!(proto_audit.recorded_at.is_some());
        assert_eq!(proto_audit.worker_uuid, Some("worker-abc".to_string()));
        assert_eq!(proto_audit.correlation_id, Some("corr-xyz".to_string()));
        assert!(proto_audit.success);
        assert_eq!(proto_audit.execution_time_ms, Some(150));
        assert!(proto_audit.result.is_some());
        assert_eq!(proto_audit.step_name, "validate_order");
        assert!(
            proto_audit.from_state.is_some(),
            "from_state should be parsed for valid state"
        );
        assert_eq!(
            proto_audit.to_state,
            proto::StepState::EnqueuedForOrchestration as i32
        );
    }

    // ========================================================================
    // From<&HealthResponse> for proto::HealthResponse
    // ========================================================================

    #[test]
    fn test_health_response_to_proto() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            timestamp: "2024-01-15T10:30:00Z".to_string(),
        };

        let proto_health = proto::HealthResponse::from(&response);
        assert_eq!(proto_health.status, "healthy");
        assert_eq!(proto_health.timestamp, "2024-01-15T10:30:00Z");
    }

    // ========================================================================
    // From<&ReadinessResponse> for proto::ReadinessResponse
    // ========================================================================

    #[test]
    fn test_readiness_response_to_proto() {
        let response = ReadinessResponse {
            status: "ready".to_string(),
            timestamp: "2024-01-15T10:30:00Z".to_string(),
            checks: ReadinessChecks {
                web_database: HealthCheck::healthy("db ok", 5),
                orchestration_database: HealthCheck::healthy("orch db ok", 3),
                circuit_breaker: HealthCheck::healthy("cb ok", 1),
                orchestration_system: HealthCheck::healthy("orch ok", 2),
                command_processor: HealthCheck::healthy("cmd ok", 1),
            },
            info: HealthInfo {
                version: "1.0.0".to_string(),
                environment: "test".to_string(),
                operational_state: "running".to_string(),
                web_database_pool_size: 10,
                orchestration_database_pool_size: 5,
                circuit_breaker_state: "closed".to_string(),
                pool_utilization: None,
            },
        };

        let proto_readiness = proto::ReadinessResponse::from(&response);
        assert_eq!(proto_readiness.status, "ready");
        assert_eq!(proto_readiness.timestamp, "2024-01-15T10:30:00Z");
        assert!(proto_readiness.checks.is_some());
        assert!(proto_readiness.info.is_some());

        let checks = proto_readiness.checks.unwrap();
        assert!(checks.web_database.is_some());
        assert!(checks.orchestration_database.is_some());
        assert!(checks.circuit_breaker.is_some());
        assert!(checks.orchestration_system.is_some());
        assert!(checks.command_processor.is_some());

        let info = proto_readiness.info.unwrap();
        assert_eq!(info.version, "1.0.0");
        assert_eq!(info.environment, "test");
        assert_eq!(info.operational_state, "running");
        assert_eq!(info.web_database_pool_size, 10);
        assert_eq!(info.orchestration_database_pool_size, 5);
        assert_eq!(info.circuit_breaker_state, "closed");
        assert!(
            info.pool_utilization.is_none(),
            "pool_utilization should be None when domain is None"
        );
    }

    // ========================================================================
    // From<&DetailedHealthResponse> for proto::DetailedHealthResponse
    // ========================================================================

    #[test]
    fn test_detailed_health_response_to_proto() {
        let pool_util = PoolUtilizationInfo {
            tasker_pool: PoolDetail {
                active_connections: 3,
                idle_connections: 7,
                max_connections: 10,
                utilization_percent: 30.0,
                total_acquires: 100,
                slow_acquires: 2,
                acquire_errors: 0,
                average_acquire_time_ms: 1.5,
                max_acquire_time_ms: 10.0,
            },
            pgmq_pool: PoolDetail {
                active_connections: 1,
                idle_connections: 4,
                max_connections: 5,
                utilization_percent: 20.0,
                total_acquires: 50,
                slow_acquires: 0,
                acquire_errors: 0,
                average_acquire_time_ms: 0.8,
                max_acquire_time_ms: 5.0,
            },
        };

        let response = DetailedHealthResponse {
            status: "healthy".to_string(),
            timestamp: "2024-01-15T10:30:00Z".to_string(),
            checks: DetailedHealthChecks {
                web_database: HealthCheck::healthy("db ok", 5),
                orchestration_database: HealthCheck::healthy("orch db ok", 3),
                circuit_breaker: HealthCheck::healthy("cb ok", 1),
                orchestration_system: HealthCheck::healthy("orch ok", 2),
                command_processor: HealthCheck::healthy("cmd ok", 1),
                pool_utilization: HealthCheck::healthy("pools ok", 1),
                queue_depth: HealthCheck::healthy("queues ok", 1),
                channel_saturation: HealthCheck::healthy("channels ok", 1),
            },
            info: HealthInfo {
                version: "1.0.0".to_string(),
                environment: "test".to_string(),
                operational_state: "running".to_string(),
                web_database_pool_size: 10,
                orchestration_database_pool_size: 5,
                circuit_breaker_state: "closed".to_string(),
                pool_utilization: Some(pool_util),
            },
        };

        let proto_detailed = proto::DetailedHealthResponse::from(&response);
        assert_eq!(proto_detailed.status, "healthy");
        assert!(proto_detailed.checks.is_some());
        assert!(proto_detailed.info.is_some());

        let checks = proto_detailed.checks.unwrap();
        assert!(checks.web_database.is_some());
        assert!(checks.orchestration_database.is_some());
        assert!(checks.circuit_breaker.is_some());
        assert!(checks.orchestration_system.is_some());
        assert!(checks.command_processor.is_some());
        assert!(checks.pool_utilization.is_some());
        assert!(checks.queue_depth.is_some());
        assert!(checks.channel_saturation.is_some());

        let info = proto_detailed.info.unwrap();
        assert!(
            info.pool_utilization.is_some(),
            "pool_utilization should be Some when domain has it"
        );
    }

    // ========================================================================
    // From<&HealthCheck> for proto::HealthCheck
    // ========================================================================

    #[test]
    fn test_health_check_to_proto() {
        let check = HealthCheck::healthy("all systems go", 42);
        let proto_check = proto::HealthCheck::from(&check);

        assert_eq!(proto_check.status, "healthy");
        assert_eq!(proto_check.message, Some("all systems go".to_string()));
        assert_eq!(proto_check.duration_ms, 42);
    }

    // ========================================================================
    // From<&HealthInfo> for proto::HealthInfo
    // ========================================================================

    #[test]
    fn test_health_info_to_proto_without_pool_utilization() {
        let info = HealthInfo {
            version: "2.0.0".to_string(),
            environment: "production".to_string(),
            operational_state: "active".to_string(),
            web_database_pool_size: 20,
            orchestration_database_pool_size: 10,
            circuit_breaker_state: "closed".to_string(),
            pool_utilization: None,
        };

        let proto_info = proto::HealthInfo::from(&info);
        assert_eq!(proto_info.version, "2.0.0");
        assert_eq!(proto_info.environment, "production");
        assert_eq!(proto_info.operational_state, "active");
        assert_eq!(proto_info.web_database_pool_size, 20);
        assert_eq!(proto_info.orchestration_database_pool_size, 10);
        assert_eq!(proto_info.circuit_breaker_state, "closed");
        assert!(proto_info.pool_utilization.is_none());
    }

    #[test]
    fn test_health_info_to_proto_with_pool_utilization() {
        let info = HealthInfo {
            version: "2.0.0".to_string(),
            environment: "production".to_string(),
            operational_state: "active".to_string(),
            web_database_pool_size: 20,
            orchestration_database_pool_size: 10,
            circuit_breaker_state: "closed".to_string(),
            pool_utilization: Some(PoolUtilizationInfo {
                tasker_pool: PoolDetail {
                    active_connections: 5,
                    idle_connections: 15,
                    max_connections: 20,
                    utilization_percent: 25.0,
                    total_acquires: 200,
                    slow_acquires: 1,
                    acquire_errors: 0,
                    average_acquire_time_ms: 2.0,
                    max_acquire_time_ms: 15.0,
                },
                pgmq_pool: PoolDetail {
                    active_connections: 2,
                    idle_connections: 8,
                    max_connections: 10,
                    utilization_percent: 20.0,
                    total_acquires: 80,
                    slow_acquires: 0,
                    acquire_errors: 0,
                    average_acquire_time_ms: 1.0,
                    max_acquire_time_ms: 8.0,
                },
            }),
        };

        let proto_info = proto::HealthInfo::from(&info);
        assert!(proto_info.pool_utilization.is_some());
    }

    // ========================================================================
    // From<&PoolUtilizationInfo> for proto::PoolUtilizationInfo
    // ========================================================================

    #[test]
    fn test_pool_utilization_info_to_proto() {
        let info = PoolUtilizationInfo {
            tasker_pool: PoolDetail {
                active_connections: 4,
                idle_connections: 6,
                max_connections: 10,
                utilization_percent: 40.0,
                total_acquires: 500,
                slow_acquires: 3,
                acquire_errors: 1,
                average_acquire_time_ms: 2.5,
                max_acquire_time_ms: 20.0,
            },
            pgmq_pool: PoolDetail {
                active_connections: 2,
                idle_connections: 3,
                max_connections: 5,
                utilization_percent: 40.0,
                total_acquires: 200,
                slow_acquires: 1,
                acquire_errors: 0,
                average_acquire_time_ms: 1.2,
                max_acquire_time_ms: 7.0,
            },
        };

        let proto_util = proto::PoolUtilizationInfo::from(&info);
        assert!(proto_util.tasker_pool.is_some());
        assert!(proto_util.pgmq_pool.is_some());
    }

    // ========================================================================
    // From<&PoolDetail> for proto::PoolDetail
    // ========================================================================

    #[test]
    fn test_pool_detail_to_proto() {
        let detail = PoolDetail {
            active_connections: 7,
            idle_connections: 13,
            max_connections: 20,
            utilization_percent: 35.0,
            total_acquires: 1000,
            slow_acquires: 5,
            acquire_errors: 2,
            average_acquire_time_ms: 3.0,
            max_acquire_time_ms: 25.0,
        };

        let proto_detail = proto::PoolDetail::from(&detail);
        assert_eq!(proto_detail.active_connections, 7);
        assert_eq!(proto_detail.idle_connections, 13);
        assert_eq!(proto_detail.max_connections, 20);
        assert!((proto_detail.utilization_percent - 35.0).abs() < f64::EPSILON);
        assert_eq!(proto_detail.total_acquires, 1000);
        assert_eq!(proto_detail.slow_acquires, 5);
        assert_eq!(proto_detail.acquire_errors, 2);
        assert!((proto_detail.average_acquire_time_ms - 3.0).abs() < f64::EPSILON);
        assert!((proto_detail.max_acquire_time_ms - 25.0).abs() < f64::EPSILON);
    }

    // ========================================================================
    // Worker health conversions
    // ========================================================================

    #[test]
    fn test_worker_basic_health_to_proto() {
        let now = Utc::now();
        let response = WorkerBasicHealth {
            status: "healthy".to_string(),
            timestamp: now,
            worker_id: "worker-001".to_string(),
        };

        let proto_health = proto::WorkerBasicHealthResponse::from(&response);
        assert_eq!(proto_health.status, "healthy");
        assert!(proto_health.timestamp.is_some());
        assert_eq!(proto_health.worker_id, "worker-001");
    }

    #[test]
    fn test_worker_readiness_to_proto() {
        let now = Utc::now();
        let response = WorkerReadiness {
            status: "ready".to_string(),
            timestamp: now,
            worker_id: "worker-002".to_string(),
            checks: WorkerReadinessChecks {
                database: WorkerDomainHealthCheck::healthy("db ok", 5),
                command_processor: WorkerDomainHealthCheck::healthy("cmd ok", 3),
                queue_processing: WorkerDomainHealthCheck::healthy("queue ok", 2),
            },
            system_info: WorkerSystemInfo {
                version: "1.0.0".to_string(),
                environment: "test".to_string(),
                uptime_seconds: 3600,
                worker_type: "rust".to_string(),
                database_pool_size: 10,
                command_processor_active: true,
                supported_namespaces: vec!["default".to_string()],
                pool_utilization: None,
            },
        };

        let proto_readiness = proto::WorkerReadinessResponse::from(&response);
        assert_eq!(proto_readiness.status, "ready");
        assert!(proto_readiness.timestamp.is_some());
        assert_eq!(proto_readiness.worker_id, "worker-002");
        assert!(proto_readiness.checks.is_some());
        assert!(proto_readiness.system_info.is_some());

        let checks = proto_readiness.checks.unwrap();
        assert!(checks.database.is_some());
        assert!(checks.command_processor.is_some());
        assert!(checks.queue_processing.is_some());
    }

    #[test]
    fn test_worker_detailed_health_to_proto() {
        let now = Utc::now();
        let response = WorkerDetailedHealth {
            status: "healthy".to_string(),
            timestamp: now,
            worker_id: "worker-003".to_string(),
            checks: WorkerDetailedChecks {
                database: WorkerDomainHealthCheck::healthy("db ok", 5),
                command_processor: WorkerDomainHealthCheck::healthy("cmd ok", 3),
                queue_processing: WorkerDomainHealthCheck::healthy("queue ok", 2),
                event_system: WorkerDomainHealthCheck::healthy("events ok", 1),
                step_processing: WorkerDomainHealthCheck::healthy("steps ok", 4),
                circuit_breakers: WorkerDomainHealthCheck::healthy("circuits ok", 1),
            },
            system_info: WorkerSystemInfo {
                version: "1.0.0".to_string(),
                environment: "test".to_string(),
                uptime_seconds: 7200,
                worker_type: "rust".to_string(),
                database_pool_size: 10,
                command_processor_active: true,
                supported_namespaces: vec!["default".to_string(), "custom".to_string()],
                pool_utilization: None,
            },
            distributed_cache: Some(DistributedCacheInfo {
                enabled: true,
                provider: "redis".to_string(),
                healthy: true,
            }),
        };

        let proto_detailed = proto::WorkerDetailedHealthResponse::from(&response);
        assert_eq!(proto_detailed.status, "healthy");
        assert!(proto_detailed.timestamp.is_some());
        assert_eq!(proto_detailed.worker_id, "worker-003");
        assert!(proto_detailed.checks.is_some());
        assert!(proto_detailed.system_info.is_some());
        assert!(proto_detailed.distributed_cache.is_some());

        let cache = proto_detailed.distributed_cache.unwrap();
        assert!(cache.enabled);
        assert_eq!(cache.provider, "redis");
        assert!(cache.healthy);
    }

    // ========================================================================
    // From<&WorkerReadinessChecks> for proto::WorkerReadinessChecks
    // ========================================================================

    #[test]
    fn test_worker_readiness_checks_to_proto() {
        let checks = WorkerReadinessChecks {
            database: WorkerDomainHealthCheck::healthy("db ok", 5),
            command_processor: WorkerDomainHealthCheck::healthy("cmd ok", 3),
            queue_processing: WorkerDomainHealthCheck::degraded("queue slow", 150),
        };

        let proto_checks = proto::WorkerReadinessChecks::from(&checks);
        assert!(proto_checks.database.is_some());
        assert!(proto_checks.command_processor.is_some());
        assert!(proto_checks.queue_processing.is_some());

        let queue_check = proto_checks.queue_processing.unwrap();
        assert_eq!(queue_check.status, "degraded");
    }

    // ========================================================================
    // From<&WorkerDetailedChecks> for proto::WorkerDetailedChecks
    // ========================================================================

    #[test]
    fn test_worker_detailed_checks_to_proto() {
        let checks = WorkerDetailedChecks {
            database: WorkerDomainHealthCheck::healthy("db ok", 5),
            command_processor: WorkerDomainHealthCheck::healthy("cmd ok", 3),
            queue_processing: WorkerDomainHealthCheck::healthy("queue ok", 2),
            event_system: WorkerDomainHealthCheck::unhealthy("events down", 5000),
            step_processing: WorkerDomainHealthCheck::healthy("steps ok", 4),
            circuit_breakers: WorkerDomainHealthCheck::healthy("circuits ok", 1),
        };

        let proto_checks = proto::WorkerDetailedChecks::from(&checks);
        assert!(proto_checks.database.is_some());
        assert!(proto_checks.command_processor.is_some());
        assert!(proto_checks.queue_processing.is_some());
        assert!(proto_checks.event_system.is_some());
        assert!(proto_checks.step_processing.is_some());
        assert!(proto_checks.circuit_breakers.is_some());

        let event_check = proto_checks.event_system.unwrap();
        assert_eq!(event_check.status, "unhealthy");
    }

    // ========================================================================
    // From<&WorkerDomainHealthCheck> for proto::WorkerHealthCheck
    // ========================================================================

    #[test]
    fn test_worker_domain_health_check_to_proto() {
        let check = WorkerDomainHealthCheck::healthy("service operational", 8);

        let proto_check = proto::WorkerHealthCheck::from(&check);
        assert_eq!(proto_check.status, "healthy");
        assert_eq!(proto_check.message, Some("service operational".to_string()));
        assert_eq!(proto_check.duration_ms, 8);
        assert!(
            proto_check.last_checked.is_some(),
            "last_checked should be set"
        );
    }

    // ========================================================================
    // From<&WorkerSystemInfo> for proto::WorkerSystemInfo
    // ========================================================================

    #[test]
    fn test_worker_system_info_to_proto_without_pool_utilization() {
        let info = WorkerSystemInfo {
            version: "1.2.3".to_string(),
            environment: "staging".to_string(),
            uptime_seconds: 86400,
            worker_type: "python".to_string(),
            database_pool_size: 15,
            command_processor_active: true,
            supported_namespaces: vec!["ns1".to_string(), "ns2".to_string()],
            pool_utilization: None,
        };

        let proto_info = proto::WorkerSystemInfo::from(&info);
        assert_eq!(proto_info.version, "1.2.3");
        assert_eq!(proto_info.environment, "staging");
        assert_eq!(proto_info.uptime_seconds, 86400);
        assert_eq!(proto_info.worker_type, "python");
        assert_eq!(proto_info.database_pool_size, 15);
        assert!(proto_info.command_processor_active);
        assert_eq!(proto_info.supported_namespaces, vec!["ns1", "ns2"]);
        assert!(proto_info.pool_utilization.is_none());
    }

    #[test]
    fn test_worker_system_info_to_proto_with_pool_utilization() {
        let info = WorkerSystemInfo {
            version: "1.2.3".to_string(),
            environment: "production".to_string(),
            uptime_seconds: 172800,
            worker_type: "rust".to_string(),
            database_pool_size: 20,
            command_processor_active: true,
            supported_namespaces: vec!["default".to_string()],
            pool_utilization: Some(DomainPoolUtilizationInfo {
                tasker_pool: crate::types::api::health::PoolDetail {
                    active_connections: 5,
                    idle_connections: 15,
                    max_connections: 20,
                    utilization_percent: 25.0,
                    total_acquires: 1000,
                    slow_acquires: 2,
                    acquire_errors: 0,
                    average_acquire_time_ms: 1.5,
                    max_acquire_time_ms: 12.0,
                },
                pgmq_pool: crate::types::api::health::PoolDetail {
                    active_connections: 3,
                    idle_connections: 7,
                    max_connections: 10,
                    utilization_percent: 30.0,
                    total_acquires: 500,
                    slow_acquires: 1,
                    acquire_errors: 0,
                    average_acquire_time_ms: 0.9,
                    max_acquire_time_ms: 6.0,
                },
            }),
        };

        let proto_info = proto::WorkerSystemInfo::from(&info);
        assert!(
            proto_info.pool_utilization.is_some(),
            "pool_utilization should be present"
        );
        let pool = proto_info.pool_utilization.unwrap();
        assert!(pool.tasker_pool.is_some());
        assert!(pool.pgmq_pool.is_some());

        let tasker = pool.tasker_pool.unwrap();
        assert_eq!(tasker.active_connections, 5);
        assert_eq!(tasker.idle_connections, 15);
        assert_eq!(tasker.max_connections, 20);
    }

    // ========================================================================
    // From<&WorkerConfigResponse> for proto::WorkerGetConfigResponse
    // ========================================================================

    #[test]
    fn test_worker_config_response_to_proto() {
        let now = Utc::now();
        let response = WorkerConfigResponse {
            metadata: ConfigMetadata {
                timestamp: now,
                environment: "test".to_string(),
                version: "1.0.0".to_string(),
            },
            worker_id: "worker-config-001".to_string(),
            worker_type: "rust".to_string(),
            auth: SafeAuthConfig {
                enabled: true,
                verification_method: "public_key".to_string(),
                jwt_issuer: "tasker".to_string(),
                jwt_audience: "api".to_string(),
                api_key_header: "X-API-Key".to_string(),
                api_key_count: 3,
                strict_validation: true,
                allowed_algorithms: vec!["RS256".to_string(), "ES256".to_string()],
            },
            messaging: SafeMessagingConfig {
                backend: "pgmq".to_string(),
                queues: vec!["worker_commands".to_string(), "worker_results".to_string()],
            },
        };

        let proto_config = proto::WorkerGetConfigResponse::from(&response);
        assert!(proto_config.metadata.is_some());
        assert_eq!(proto_config.worker_id, "worker-config-001");
        assert_eq!(proto_config.worker_type, "rust");
        assert!(proto_config.auth.is_some());
        assert!(proto_config.messaging.is_some());

        let metadata = proto_config.metadata.unwrap();
        assert!(metadata.timestamp.is_some());
        assert_eq!(metadata.environment, "test");
        assert_eq!(metadata.version, "1.0.0");

        let auth = proto_config.auth.unwrap();
        assert!(auth.enabled);
        assert_eq!(auth.verification_method, "public_key");
        assert_eq!(auth.jwt_issuer, "tasker");
        assert_eq!(auth.jwt_audience, "api");
        assert_eq!(auth.api_key_header, "X-API-Key");
        assert_eq!(auth.api_key_count, 3);
        assert!(auth.strict_validation);
        assert_eq!(auth.allowed_algorithms, vec!["RS256", "ES256"]);

        let messaging = proto_config.messaging.unwrap();
        assert_eq!(messaging.backend, "pgmq");
        assert_eq!(messaging.queues, vec!["worker_commands", "worker_results"]);
    }

    // ========================================================================
    // From<&WorkerTemplateList> for proto::WorkerTemplateListResponse
    // ========================================================================

    #[test]
    fn test_worker_template_list_to_proto_without_cache_stats() {
        let response = WorkerTemplateList {
            supported_namespaces: vec!["ns1".to_string(), "ns2".to_string()],
            template_count: 5,
            cache_stats: None,
            worker_capabilities: vec!["ruby".to_string(), "python".to_string()],
        };

        let proto_list = proto::WorkerTemplateListResponse::from(&response);
        assert_eq!(proto_list.supported_namespaces, vec!["ns1", "ns2"]);
        assert_eq!(proto_list.template_count, 5);
        assert!(proto_list.cache_stats.is_none());
        assert_eq!(proto_list.worker_capabilities, vec!["ruby", "python"]);
    }

    #[test]
    fn test_worker_template_list_to_proto_with_cache_stats() {
        let response = WorkerTemplateList {
            supported_namespaces: vec!["default".to_string()],
            template_count: 10,
            cache_stats: Some(CacheStats {
                total_cached: 10,
                cache_hits: 500,
                cache_misses: 50,
                cache_evictions: 5,
                oldest_entry_age_seconds: 3600,
                average_access_count: 45.5,
                supported_namespaces: vec!["default".to_string()],
            }),
            worker_capabilities: vec!["rust".to_string()],
        };

        let proto_list = proto::WorkerTemplateListResponse::from(&response);
        assert_eq!(proto_list.template_count, 10);
        assert!(proto_list.cache_stats.is_some());

        let stats = proto_list.cache_stats.unwrap();
        assert_eq!(stats.total_cached, 10);
        assert_eq!(stats.cache_hits, 500);
        assert_eq!(stats.cache_misses, 50);
        assert_eq!(stats.cache_evictions, 5);
        assert_eq!(stats.oldest_entry_age_seconds, 3600);
        assert!((stats.average_access_count - 45.5).abs() < f64::EPSILON);
        assert_eq!(stats.supported_namespaces, vec!["default"]);
    }

    // ========================================================================
    // From<&CacheStats> for proto::CacheStats
    // ========================================================================

    #[test]
    fn test_cache_stats_to_proto() {
        let stats = CacheStats {
            total_cached: 25,
            cache_hits: 1000,
            cache_misses: 100,
            cache_evictions: 10,
            oldest_entry_age_seconds: 7200,
            average_access_count: 38.2,
            supported_namespaces: vec!["ns_a".to_string(), "ns_b".to_string()],
        };

        let proto_stats = proto::CacheStats::from(&stats);
        assert_eq!(proto_stats.total_cached, 25);
        assert_eq!(proto_stats.cache_hits, 1000);
        assert_eq!(proto_stats.cache_misses, 100);
        assert_eq!(proto_stats.cache_evictions, 10);
        assert_eq!(proto_stats.oldest_entry_age_seconds, 7200);
        assert!((proto_stats.average_access_count - 38.2).abs() < f64::EPSILON);
        assert_eq!(proto_stats.supported_namespaces, vec!["ns_a", "ns_b"]);
    }

    // ========================================================================
    // From<&DistributedCacheInfo> for proto::DistributedCacheInfo
    // ========================================================================

    #[test]
    fn test_distributed_cache_info_to_proto() {
        let info = DistributedCacheInfo {
            enabled: true,
            provider: "redis".to_string(),
            healthy: true,
        };

        let proto_info = proto::DistributedCacheInfo::from(&info);
        assert!(proto_info.enabled);
        assert_eq!(proto_info.provider, "redis");
        assert!(proto_info.healthy);
    }

    #[test]
    fn test_distributed_cache_info_to_proto_disabled() {
        let info = DistributedCacheInfo {
            enabled: false,
            provider: "noop".to_string(),
            healthy: false,
        };

        let proto_info = proto::DistributedCacheInfo::from(&info);
        assert!(!proto_info.enabled);
        assert_eq!(proto_info.provider, "noop");
        assert!(!proto_info.healthy);
    }

    // ========================================================================
    // From<&HandlerMetadata> for proto::WorkerHandlerMetadata
    // ========================================================================

    #[test]
    fn test_handler_metadata_to_proto() {
        let metadata = HandlerMetadata {
            namespace: "default".to_string(),
            name: "my_handler".to_string(),
            version: "2.0.0".to_string(),
            handler_class: "MyHandler".to_string(),
            config_schema: Some(serde_json::json!({"type": "object"})),
            default_dependent_system: Some("external-api".to_string()),
            registered_at: Utc::now(),
        };

        let proto_metadata = proto::WorkerHandlerMetadata::from(&metadata);
        assert_eq!(proto_metadata.namespace, "default");
        assert_eq!(proto_metadata.handler_name, "my_handler");
        assert_eq!(proto_metadata.version, "2.0.0");
        assert!(
            proto_metadata.description.is_none(),
            "description should be None (HandlerMetadata has no description field)"
        );
        assert!(
            proto_metadata.step_names.is_empty(),
            "step_names should be empty (not populated from HandlerMetadata)"
        );
    }
}

//! Type conversions from Protocol Buffer types to domain types.
//!
//! This module provides conversion functions for transforming gRPC proto response
//! types back to the domain types used by the REST API. This is the inverse of
//! the conversions in `tasker_shared::proto::conversions`.
//!
//! # Design Philosophy
//!
//! The gRPC clients should return the exact same domain types as the REST clients.
//! This ensures consistent behavior regardless of transport protocol and allows
//! the client library to be transport-agnostic from the caller's perspective.
//!
//! We use helper functions rather than trait implementations due to Rust's orphan
//! rules - we cannot implement `From` for types defined in other crates.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use tasker_shared::proto::v1 as proto;

use crate::error::ClientError;

// ============================================================================
// Timestamp Conversions
// ============================================================================

/// Convert an optional protobuf Timestamp to DateTime<Utc>.
pub fn proto_timestamp_to_datetime(ts: Option<prost_types::Timestamp>) -> DateTime<Utc> {
    ts.and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos as u32))
        .unwrap_or_default()
}

/// Convert an optional protobuf Timestamp to an optional DateTime<Utc>.
pub fn proto_timestamp_to_datetime_opt(
    ts: Option<prost_types::Timestamp>,
) -> Option<DateTime<Utc>> {
    ts.and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos as u32))
}

/// Convert an optional protobuf Timestamp to an RFC3339 string.
pub fn proto_timestamp_to_string(ts: Option<prost_types::Timestamp>) -> String {
    proto_timestamp_to_datetime(ts).to_rfc3339()
}

/// Convert an optional protobuf Timestamp to an optional RFC3339 string.
pub fn proto_timestamp_to_string_opt(ts: Option<prost_types::Timestamp>) -> Option<String> {
    proto_timestamp_to_datetime_opt(ts).map(|dt| dt.to_rfc3339())
}

// ============================================================================
// JSON/Struct Conversions
// ============================================================================

/// Convert optional prost_types::Struct to serde_json::Value.
pub fn proto_struct_to_json_opt(s: Option<prost_types::Struct>) -> serde_json::Value {
    s.map(proto_struct_to_json)
        .unwrap_or(serde_json::Value::Null)
}

/// Convert prost_types::Struct to serde_json::Value.
pub fn proto_struct_to_json(s: prost_types::Struct) -> serde_json::Value {
    serde_json::Value::Object(
        s.fields
            .into_iter()
            .map(|(k, v)| (k, prost_value_to_json(v)))
            .collect(),
    )
}

fn prost_value_to_json(value: prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match value.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(b),
        Some(Kind::NumberValue(n)) => serde_json::Number::from_f64(n)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s),
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.into_iter().map(prost_value_to_json).collect())
        }
        Some(Kind::StructValue(s)) => proto_struct_to_json(s),
        None => serde_json::Value::Null,
    }
}

// ============================================================================
// State String Conversions
// ============================================================================

/// Convert proto TaskState enum to domain string representation.
pub fn proto_task_state_to_string(state: i32) -> String {
    use tasker_shared::state_machine::states::TaskState;

    match proto::TaskState::try_from(state) {
        Ok(proto::TaskState::Pending) => TaskState::Pending.as_str().to_string(),
        Ok(proto::TaskState::Initializing) => TaskState::Initializing.as_str().to_string(),
        Ok(proto::TaskState::EnqueuingSteps) => TaskState::EnqueuingSteps.as_str().to_string(),
        Ok(proto::TaskState::StepsInProcess) => TaskState::StepsInProcess.as_str().to_string(),
        Ok(proto::TaskState::EvaluatingResults) => {
            TaskState::EvaluatingResults.as_str().to_string()
        }
        Ok(proto::TaskState::WaitingForDependencies) => {
            TaskState::WaitingForDependencies.as_str().to_string()
        }
        Ok(proto::TaskState::WaitingForRetry) => TaskState::WaitingForRetry.as_str().to_string(),
        Ok(proto::TaskState::BlockedByFailures) => {
            TaskState::BlockedByFailures.as_str().to_string()
        }
        Ok(proto::TaskState::Complete) => TaskState::Complete.as_str().to_string(),
        Ok(proto::TaskState::Error) => TaskState::Error.as_str().to_string(),
        Ok(proto::TaskState::Cancelled) => TaskState::Cancelled.as_str().to_string(),
        Ok(proto::TaskState::ResolvedManually) => TaskState::ResolvedManually.as_str().to_string(),
        Ok(proto::TaskState::Unspecified) | Err(_) => "unspecified".to_string(),
    }
}

/// Convert proto StepState enum to domain string representation.
pub fn proto_step_state_to_string(state: i32) -> String {
    use tasker_shared::state_machine::states::WorkflowStepState;

    match proto::StepState::try_from(state) {
        Ok(proto::StepState::Pending) => WorkflowStepState::Pending.as_str().to_string(),
        Ok(proto::StepState::Enqueued) => WorkflowStepState::Enqueued.as_str().to_string(),
        Ok(proto::StepState::InProgress) => WorkflowStepState::InProgress.as_str().to_string(),
        Ok(proto::StepState::EnqueuedForOrchestration) => {
            WorkflowStepState::EnqueuedForOrchestration
                .as_str()
                .to_string()
        }
        Ok(proto::StepState::EnqueuedAsErrorForOrchestration) => {
            WorkflowStepState::EnqueuedAsErrorForOrchestration
                .as_str()
                .to_string()
        }
        Ok(proto::StepState::WaitingForRetry) => {
            WorkflowStepState::WaitingForRetry.as_str().to_string()
        }
        Ok(proto::StepState::Complete) => WorkflowStepState::Complete.as_str().to_string(),
        Ok(proto::StepState::Error) => WorkflowStepState::Error.as_str().to_string(),
        Ok(proto::StepState::Cancelled) => WorkflowStepState::Cancelled.as_str().to_string(),
        Ok(proto::StepState::ResolvedManually) => {
            WorkflowStepState::ResolvedManually.as_str().to_string()
        }
        Ok(proto::StepState::Unspecified) | Err(_) => "unspecified".to_string(),
    }
}

// ============================================================================
// Task Response Conversions
// ============================================================================

use tasker_shared::models::core::task::PaginationInfo;
use tasker_shared::types::api::orchestration::{
    StepAuditResponse, StepResponse, TaskListResponse, TaskResponse,
};

/// Convert proto Task to domain TaskResponse.
pub fn proto_task_to_domain(task: proto::Task) -> Result<TaskResponse, ClientError> {
    let correlation_id = uuid::Uuid::parse_str(&task.correlation_id)
        .map_err(|e| ClientError::Internal(format!("Invalid correlation_id: {e}")))?;

    let parent_correlation_id = task
        .parent_correlation_id
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| uuid::Uuid::parse_str(s))
        .transpose()
        .map_err(|e| ClientError::Internal(format!("Invalid parent_correlation_id: {e}")))?;

    Ok(TaskResponse {
        task_uuid: task.task_uuid,
        name: task.name,
        namespace: task.namespace,
        version: task.version,
        status: proto_task_state_to_string(task.state),
        created_at: proto_timestamp_to_datetime(task.created_at),
        updated_at: proto_timestamp_to_datetime(task.updated_at),
        completed_at: proto_timestamp_to_datetime_opt(task.completed_at),
        context: proto_struct_to_json_opt(task.context),
        initiator: task.initiator,
        source_system: task.source_system,
        reason: task.reason,
        priority: task.priority,
        tags: if task.tags.is_empty() {
            None
        } else {
            Some(task.tags)
        },
        correlation_id,
        parent_correlation_id,
        total_steps: task.total_steps,
        pending_steps: task.pending_steps,
        in_progress_steps: task.in_progress_steps,
        completed_steps: task.completed_steps,
        failed_steps: task.failed_steps,
        ready_steps: task.ready_steps,
        execution_status: task.execution_status,
        recommended_action: task.recommended_action,
        completion_percentage: task.completion_percentage,
        health_status: task.health_status,
        steps: vec![], // Steps are fetched separately via GetSteps
    })
}

/// Convert proto GetTaskResponse to domain TaskResponse.
pub fn proto_get_task_response_to_domain(
    response: proto::GetTaskResponse,
) -> Result<TaskResponse, ClientError> {
    response
        .task
        .ok_or_else(|| ClientError::Internal("Server returned empty task".to_string()))
        .and_then(proto_task_to_domain)
}

/// Convert proto CreateTaskResponse to domain TaskResponse.
pub fn proto_create_task_response_to_domain(
    response: proto::CreateTaskResponse,
) -> Result<TaskResponse, ClientError> {
    response
        .task
        .ok_or_else(|| {
            ClientError::Internal("Server returned empty task in create response".to_string())
        })
        .and_then(proto_task_to_domain)
}

/// Convert proto ListTasksResponse to domain TaskListResponse.
pub fn proto_list_tasks_response_to_domain(
    response: proto::ListTasksResponse,
) -> Result<TaskListResponse, ClientError> {
    let tasks: Result<Vec<TaskResponse>, ClientError> = response
        .tasks
        .into_iter()
        .map(proto_task_to_domain)
        .collect();

    let pagination = response.pagination.unwrap_or_default();
    let total = pagination.total as u64;
    let per_page = pagination.count.max(1) as u32;
    let offset = pagination.offset as u64;
    let page = if per_page > 0 {
        (offset / per_page as u64) as u32 + 1
    } else {
        1
    };
    let total_pages = if per_page > 0 {
        total.div_ceil(per_page as u64) as u32
    } else {
        1
    };

    Ok(TaskListResponse {
        tasks: tasks?,
        pagination: PaginationInfo {
            page,
            per_page,
            total_count: total,
            total_pages,
            has_next: pagination.has_more,
            has_previous: page > 1,
        },
    })
}

// ============================================================================
// Step Response Conversions
// ============================================================================

/// Convert proto Step to domain StepResponse.
pub fn proto_step_to_domain(step: proto::Step) -> Result<StepResponse, ClientError> {
    Ok(StepResponse {
        step_uuid: step.step_uuid,
        task_uuid: step.task_uuid,
        name: step.name,
        created_at: proto_timestamp_to_string(step.created_at),
        updated_at: proto_timestamp_to_string(step.updated_at),
        completed_at: proto_timestamp_to_string_opt(step.completed_at),
        results: step.results.map(proto_struct_to_json),
        current_state: proto_step_state_to_string(step.state),
        dependencies_satisfied: step.dependencies_satisfied,
        retry_eligible: step.retry_eligible,
        ready_for_execution: step.ready_for_execution,
        total_parents: step.total_parents,
        completed_parents: step.completed_parents,
        attempts: step.attempts,
        max_attempts: step.max_attempts,
        last_failure_at: proto_timestamp_to_string_opt(step.last_failure_at),
        next_retry_at: proto_timestamp_to_string_opt(step.next_retry_at),
        last_attempted_at: proto_timestamp_to_string_opt(step.last_attempted_at),
    })
}

/// Convert proto GetStepResponse to domain StepResponse.
pub fn proto_get_step_response_to_domain(
    response: proto::GetStepResponse,
) -> Result<StepResponse, ClientError> {
    response
        .step
        .ok_or_else(|| ClientError::Internal("Server returned empty step".to_string()))
        .and_then(proto_step_to_domain)
}

/// Convert proto StepAuditRecord to domain StepAuditResponse.
pub fn proto_audit_to_domain(
    record: proto::StepAuditRecord,
) -> Result<StepAuditResponse, ClientError> {
    Ok(StepAuditResponse {
        audit_uuid: record.audit_uuid,
        workflow_step_uuid: record.step_uuid,
        transition_uuid: record.transition_uuid,
        task_uuid: record.task_uuid,
        recorded_at: proto_timestamp_to_string(record.recorded_at),
        worker_uuid: record.worker_uuid,
        correlation_id: record.correlation_id,
        success: record.success,
        execution_time_ms: record.execution_time_ms,
        result: record.result.map(proto_struct_to_json),
        step_name: record.step_name,
        from_state: record.from_state.map(proto_step_state_to_string),
        to_state: proto_step_state_to_string(record.to_state),
    })
}

// ============================================================================
// Health Response Conversions (Orchestration)
// ============================================================================

use tasker_shared::types::api::health::{PoolDetail, PoolUtilizationInfo};
use tasker_shared::types::api::orchestration::{
    DetailedHealthChecks, DetailedHealthResponse, HealthCheck, HealthInfo, HealthResponse,
    ReadinessChecks, ReadinessResponse,
};

/// Convert proto HealthResponse to domain.
pub fn proto_health_response_to_domain(response: proto::HealthResponse) -> HealthResponse {
    HealthResponse {
        status: response.status,
        timestamp: response.timestamp,
    }
}

/// Convert proto ReadinessResponse to domain.
///
/// # Errors
/// Returns `InvalidResponse` if required fields (checks, info) are missing.
pub fn proto_readiness_response_to_domain(
    response: proto::ReadinessResponse,
) -> Result<ReadinessResponse, ClientError> {
    let checks = response.checks.ok_or_else(|| {
        ClientError::invalid_response(
            "ReadinessResponse.checks",
            "Readiness response missing required health checks",
        )
    })?;
    let checks = proto_readiness_checks_to_domain(checks)?;

    let info = response.info.ok_or_else(|| {
        ClientError::invalid_response(
            "ReadinessResponse.info",
            "Readiness response missing required health info",
        )
    })?;
    let info = proto_health_info_to_domain(info)?;

    Ok(ReadinessResponse {
        status: response.status,
        timestamp: response.timestamp,
        checks,
        info,
    })
}

/// Convert proto DetailedHealthResponse to domain.
///
/// # Errors
/// Returns `InvalidResponse` if required fields (checks, info) are missing.
pub fn proto_detailed_health_response_to_domain(
    response: proto::DetailedHealthResponse,
) -> Result<DetailedHealthResponse, ClientError> {
    let checks = response.checks.ok_or_else(|| {
        ClientError::invalid_response(
            "DetailedHealthResponse.checks",
            "Detailed health response missing required health checks",
        )
    })?;
    let checks = proto_detailed_checks_to_domain(checks)?;

    let info = response.info.ok_or_else(|| {
        ClientError::invalid_response(
            "DetailedHealthResponse.info",
            "Detailed health response missing required health info",
        )
    })?;
    let info = proto_health_info_to_domain(info)?;

    Ok(DetailedHealthResponse {
        status: response.status,
        timestamp: response.timestamp,
        checks,
        info,
    })
}

fn proto_readiness_checks_to_domain(
    checks: proto::ReadinessChecks,
) -> Result<ReadinessChecks, ClientError> {
    Ok(ReadinessChecks {
        web_database: checks
            .web_database
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.web_database", "missing"))?,
        orchestration_database: checks
            .orchestration_database
            .map(proto_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("checks.orchestration_database", "missing")
            })?,
        circuit_breaker: checks
            .circuit_breaker
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.circuit_breaker", "missing"))?,
        orchestration_system: checks
            .orchestration_system
            .map(proto_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("checks.orchestration_system", "missing")
            })?,
        command_processor: checks
            .command_processor
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.command_processor", "missing"))?,
    })
}

fn proto_detailed_checks_to_domain(
    checks: proto::DetailedHealthChecks,
) -> Result<DetailedHealthChecks, ClientError> {
    Ok(DetailedHealthChecks {
        web_database: checks
            .web_database
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.web_database", "missing"))?,
        orchestration_database: checks
            .orchestration_database
            .map(proto_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("checks.orchestration_database", "missing")
            })?,
        circuit_breaker: checks
            .circuit_breaker
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.circuit_breaker", "missing"))?,
        orchestration_system: checks
            .orchestration_system
            .map(proto_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("checks.orchestration_system", "missing")
            })?,
        command_processor: checks
            .command_processor
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.command_processor", "missing"))?,
        pool_utilization: checks
            .pool_utilization
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.pool_utilization", "missing"))?,
        queue_depth: checks
            .queue_depth
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.queue_depth", "missing"))?,
        channel_saturation: checks
            .channel_saturation
            .map(proto_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("checks.channel_saturation", "missing"))?,
    })
}

fn proto_health_check_to_domain(check: proto::HealthCheck) -> HealthCheck {
    HealthCheck {
        status: check.status,
        message: check.message,
        duration_ms: check.duration_ms,
    }
}

fn proto_health_info_to_domain(info: proto::HealthInfo) -> Result<HealthInfo, ClientError> {
    // pool_utilization is optional - only present when pool stats are available
    let pool_utilization = info
        .pool_utilization
        .map(proto_pool_utilization_to_domain)
        .transpose()?;

    Ok(HealthInfo {
        version: info.version,
        environment: info.environment,
        operational_state: info.operational_state,
        web_database_pool_size: info.web_database_pool_size,
        orchestration_database_pool_size: info.orchestration_database_pool_size,
        circuit_breaker_state: info.circuit_breaker_state,
        pool_utilization,
    })
}

fn proto_pool_utilization_to_domain(
    info: proto::PoolUtilizationInfo,
) -> Result<PoolUtilizationInfo, ClientError> {
    Ok(PoolUtilizationInfo {
        tasker_pool: info
            .tasker_pool
            .map(proto_pool_detail_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response(
                    "PoolUtilizationInfo.tasker_pool",
                    "Pool utilization missing tasker pool details",
                )
            })?,
        pgmq_pool: info
            .pgmq_pool
            .map(proto_pool_detail_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response(
                    "PoolUtilizationInfo.pgmq_pool",
                    "Pool utilization missing pgmq pool details",
                )
            })?,
    })
}

fn proto_pool_detail_to_domain(pool: proto::PoolDetail) -> PoolDetail {
    PoolDetail {
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

// ============================================================================
// Worker Health Response Conversions
// ============================================================================

use tasker_shared::types::api::worker::{
    BasicHealthResponse as WorkerBasicHealth, DetailedHealthResponse as WorkerDetailedHealth,
    DistributedCacheInfo, HealthCheck as WorkerHealthCheck, ReadinessResponse as WorkerReadiness,
    WorkerDetailedChecks, WorkerReadinessChecks, WorkerSystemInfo,
};

/// Convert proto WorkerBasicHealthResponse to domain.
pub fn proto_worker_basic_health_to_domain(
    response: proto::WorkerBasicHealthResponse,
) -> WorkerBasicHealth {
    WorkerBasicHealth {
        status: response.status,
        timestamp: proto_timestamp_to_datetime(response.timestamp),
        worker_id: response.worker_id,
    }
}

/// Convert proto WorkerReadinessResponse to domain.
///
/// # Errors
/// Returns `InvalidResponse` if required fields (checks, system_info) are missing.
pub fn proto_worker_readiness_to_domain(
    response: proto::WorkerReadinessResponse,
) -> Result<WorkerReadiness, ClientError> {
    let checks = response.checks.ok_or_else(|| {
        ClientError::invalid_response(
            "WorkerReadinessResponse.checks",
            "Worker readiness response missing required health checks",
        )
    })?;
    let checks = proto_worker_readiness_checks_to_domain(checks)?;

    let system_info = response.system_info.ok_or_else(|| {
        ClientError::invalid_response(
            "WorkerReadinessResponse.system_info",
            "Worker readiness response missing required system info",
        )
    })?;
    let system_info = proto_worker_system_info_to_domain(system_info)?;

    Ok(WorkerReadiness {
        status: response.status,
        timestamp: proto_timestamp_to_datetime(response.timestamp),
        worker_id: response.worker_id,
        checks,
        system_info,
    })
}

/// Convert proto WorkerDetailedHealthResponse to domain.
///
/// # Errors
/// Returns `InvalidResponse` if required fields (checks, system_info) are missing.
pub fn proto_worker_detailed_health_to_domain(
    response: proto::WorkerDetailedHealthResponse,
) -> Result<WorkerDetailedHealth, ClientError> {
    let checks = response.checks.ok_or_else(|| {
        ClientError::invalid_response(
            "WorkerDetailedHealthResponse.checks",
            "Worker detailed health response missing required health checks",
        )
    })?;
    let checks = proto_worker_detailed_checks_to_domain(checks)?;

    let system_info = response.system_info.ok_or_else(|| {
        ClientError::invalid_response(
            "WorkerDetailedHealthResponse.system_info",
            "Worker detailed health response missing required system info",
        )
    })?;
    let system_info = proto_worker_system_info_to_domain(system_info)?;

    Ok(WorkerDetailedHealth {
        status: response.status,
        timestamp: proto_timestamp_to_datetime(response.timestamp),
        worker_id: response.worker_id,
        checks,
        system_info,
        // distributed_cache is optional - may not be configured
        distributed_cache: response
            .distributed_cache
            .map(proto_distributed_cache_to_domain),
    })
}

fn proto_worker_readiness_checks_to_domain(
    checks: proto::WorkerReadinessChecks,
) -> Result<WorkerReadinessChecks, ClientError> {
    Ok(WorkerReadinessChecks {
        database: checks
            .database
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("worker_checks.database", "missing"))?,
        command_processor: checks
            .command_processor
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("worker_checks.command_processor", "missing")
            })?,
        queue_processing: checks
            .queue_processing
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("worker_checks.queue_processing", "missing")
            })?,
    })
}

fn proto_worker_detailed_checks_to_domain(
    checks: proto::WorkerDetailedChecks,
) -> Result<WorkerDetailedChecks, ClientError> {
    Ok(WorkerDetailedChecks {
        database: checks
            .database
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| ClientError::invalid_response("worker_checks.database", "missing"))?,
        command_processor: checks
            .command_processor
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("worker_checks.command_processor", "missing")
            })?,
        queue_processing: checks
            .queue_processing
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("worker_checks.queue_processing", "missing")
            })?,
        event_system: checks
            .event_system
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("worker_checks.event_system", "missing")
            })?,
        step_processing: checks
            .step_processing
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("worker_checks.step_processing", "missing")
            })?,
        circuit_breakers: checks
            .circuit_breakers
            .map(proto_worker_health_check_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response("worker_checks.circuit_breakers", "missing")
            })?,
    })
}

fn proto_worker_health_check_to_domain(check: proto::WorkerHealthCheck) -> WorkerHealthCheck {
    WorkerHealthCheck {
        status: check.status,
        message: check.message,
        duration_ms: check.duration_ms,
        last_checked: proto_timestamp_to_datetime(check.last_checked),
    }
}

fn proto_worker_system_info_to_domain(
    info: proto::WorkerSystemInfo,
) -> Result<WorkerSystemInfo, ClientError> {
    // pool_utilization is optional - only present when pool stats are available
    let pool_utilization = info
        .pool_utilization
        .map(proto_worker_pool_utilization_to_domain)
        .transpose()?;

    Ok(WorkerSystemInfo {
        version: info.version,
        environment: info.environment,
        uptime_seconds: info.uptime_seconds,
        worker_type: info.worker_type,
        database_pool_size: info.database_pool_size,
        command_processor_active: info.command_processor_active,
        supported_namespaces: info.supported_namespaces,
        pool_utilization,
    })
}

fn proto_worker_pool_utilization_to_domain(
    p: proto::WorkerPoolUtilizationInfo,
) -> Result<PoolUtilizationInfo, ClientError> {
    Ok(PoolUtilizationInfo {
        tasker_pool: p
            .tasker_pool
            .map(proto_worker_pool_detail_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response(
                    "WorkerPoolUtilizationInfo.tasker_pool",
                    "Worker pool utilization missing tasker pool details",
                )
            })?,
        pgmq_pool: p
            .pgmq_pool
            .map(proto_worker_pool_detail_to_domain)
            .ok_or_else(|| {
                ClientError::invalid_response(
                    "WorkerPoolUtilizationInfo.pgmq_pool",
                    "Worker pool utilization missing pgmq pool details",
                )
            })?,
    })
}

fn proto_worker_pool_detail_to_domain(pool: proto::WorkerPoolDetail) -> PoolDetail {
    PoolDetail {
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

fn proto_distributed_cache_to_domain(info: proto::DistributedCacheInfo) -> DistributedCacheInfo {
    DistributedCacheInfo {
        enabled: info.enabled,
        provider: info.provider,
        healthy: info.healthy,
    }
}

// ============================================================================
// Worker Template Response Conversions
// ============================================================================

use tasker_shared::models::core::task_template::{
    HandlerDefinition, ResolvedTaskTemplate, RetryConfiguration, StepDefinition,
    SystemDependencies, TaskTemplate,
};
use tasker_shared::types::api::worker::{
    TemplateListResponse as WorkerTemplateList, TemplateResponse as WorkerTemplateResponse,
};
use tasker_shared::types::base::{CacheStats, HandlerMetadata};

/// Convert proto WorkerTemplateListResponse to domain.
pub fn proto_worker_template_list_to_domain(
    response: proto::WorkerTemplateListResponse,
) -> WorkerTemplateList {
    WorkerTemplateList {
        supported_namespaces: response.supported_namespaces.clone(),
        template_count: response.template_count as usize,
        cache_stats: response.cache_stats.map(|c| CacheStats {
            total_cached: c.total_cached as usize,
            cache_hits: c.cache_hits,
            cache_misses: c.cache_misses,
            cache_evictions: c.cache_evictions,
            oldest_entry_age_seconds: c.oldest_entry_age_seconds,
            average_access_count: c.average_access_count,
            supported_namespaces: c.supported_namespaces,
        }),
        worker_capabilities: response.worker_capabilities,
    }
}

/// Convert proto WorkerTemplateResponse to domain.
pub fn proto_worker_template_to_domain(
    response: proto::WorkerTemplateResponse,
) -> Result<WorkerTemplateResponse, ClientError> {
    let proto_template = response
        .template
        .ok_or_else(|| ClientError::Internal("Server returned empty template".to_string()))?;
    let handler_meta = response.handler_metadata.unwrap_or_default();

    let steps: Vec<StepDefinition> = proto_template
        .steps
        .into_iter()
        .map(|s| StepDefinition {
            name: s.name,
            description: s.description,
            handler: HandlerDefinition {
                callable: String::new(),
                method: None,
                resolver: None,
                initialization: HashMap::new(),
            },
            step_type: Default::default(),
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration {
                retryable: s.retryable,
                max_attempts: s.max_attempts as u32,
                ..Default::default()
            },
            timeout_seconds: None,
            publishes_events: vec![],
            batch_config: None,
        })
        .collect();

    let task_template = TaskTemplate {
        name: proto_template.name.clone(),
        namespace_name: proto_template.namespace.clone(),
        version: proto_template.version.clone(),
        description: proto_template.description,
        metadata: None,
        task_handler: None,
        system_dependencies: SystemDependencies::default(),
        domain_events: vec![],
        input_schema: None,
        steps,
        environments: HashMap::new(),
        lifecycle: None,
    };

    let resolved = ResolvedTaskTemplate {
        template: task_template,
        environment: String::new(),
        resolved_at: chrono::Utc::now(),
    };

    Ok(WorkerTemplateResponse {
        template: resolved,
        handler_metadata: HandlerMetadata {
            namespace: handler_meta.namespace,
            name: handler_meta.handler_name,
            version: handler_meta.version,
            handler_class: String::new(),
            config_schema: None,
            default_dependent_system: None,
            registered_at: chrono::Utc::now(),
        },
        cached: response.cached,
        cache_age_seconds: response.cache_age_seconds,
        access_count: response.access_count,
    })
}

// ============================================================================
// Config Response Conversions
// ============================================================================

use tasker_shared::types::api::orchestration::{
    ConfigMetadata, SafeAuthConfig, SafeMessagingConfig, WorkerConfigResponse,
};

/// Convert proto WorkerGetConfigResponse to domain.
///
/// # Errors
/// Returns `InvalidResponse` if required config sections are missing.
pub fn proto_worker_config_to_domain(
    response: proto::WorkerGetConfigResponse,
) -> Result<WorkerConfigResponse, ClientError> {
    let metadata = response
        .metadata
        .map(|m| ConfigMetadata {
            timestamp: proto_timestamp_to_datetime(m.timestamp),
            environment: m.environment,
            version: m.version,
        })
        .ok_or_else(|| {
            ClientError::invalid_response(
                "WorkerGetConfigResponse.metadata",
                "Worker config response missing required metadata",
            )
        })?;

    let auth = response
        .auth
        .map(|a| SafeAuthConfig {
            enabled: a.enabled,
            verification_method: a.verification_method,
            jwt_issuer: a.jwt_issuer,
            jwt_audience: a.jwt_audience,
            api_key_header: a.api_key_header,
            api_key_count: a.api_key_count as usize,
            strict_validation: a.strict_validation,
            allowed_algorithms: a.allowed_algorithms,
        })
        .ok_or_else(|| {
            ClientError::invalid_response(
                "WorkerGetConfigResponse.auth",
                "Worker config response missing required auth section",
            )
        })?;

    let messaging = response
        .messaging
        .map(|m| SafeMessagingConfig {
            backend: m.backend,
            queues: m.queues,
        })
        .ok_or_else(|| {
            ClientError::invalid_response(
                "WorkerGetConfigResponse.messaging",
                "Worker config response missing required messaging section",
            )
        })?;

    Ok(WorkerConfigResponse {
        metadata,
        worker_id: response.worker_id,
        worker_type: response.worker_type,
        auth,
        messaging,
    })
}

// Note: Default value helpers removed - we now fail loudly on missing required fields
// per the "fail loudly" principle to avoid returning phantom data that looks valid.

// ============================================================================
// Template Response Conversions (Orchestration)
// ============================================================================

use tasker_shared::types::api::templates::{
    NamespaceSummary, StepDefinition as TemplateStepDefinition, TemplateDetail,
    TemplateListResponse, TemplateSummary,
};

/// Convert proto ListTemplatesResponse to domain.
pub fn proto_template_list_to_domain(
    response: proto::ListTemplatesResponse,
) -> Result<TemplateListResponse, ClientError> {
    Ok(TemplateListResponse {
        namespaces: response
            .namespaces
            .into_iter()
            .map(|ns| NamespaceSummary {
                name: ns.name,
                description: ns.description,
                template_count: ns.template_count as usize,
            })
            .collect(),
        templates: response
            .templates
            .into_iter()
            .map(|t| TemplateSummary {
                name: t.name,
                namespace: t.namespace,
                version: t.version,
                description: t.description,
                step_count: t.step_count as usize,
            })
            .collect(),
        total_count: response.total_count as usize,
    })
}

/// Convert proto TemplateDetail to domain.
pub fn proto_template_detail_to_domain(
    template: proto::TemplateDetail,
) -> Result<TemplateDetail, ClientError> {
    Ok(TemplateDetail {
        name: template.name,
        namespace: template.namespace,
        version: template.version,
        description: template.description,
        configuration: template.configuration.map(proto_struct_to_json),
        steps: template
            .steps
            .into_iter()
            .map(|s| TemplateStepDefinition {
                name: s.name,
                description: s.description,
                default_retryable: s.default_retryable,
                default_max_attempts: s.default_max_attempts,
            })
            .collect(),
    })
}

// ============================================================================
// Analytics Response Conversions
// ============================================================================

use tasker_shared::database::sql_functions::SystemHealthCounts;
use tasker_shared::types::api::orchestration::{
    BottleneckAnalysis, PerformanceMetrics, ResourceUtilization, SlowStepInfo, SlowTaskInfo,
};

/// Convert proto GetPerformanceMetricsResponse to domain.
pub fn proto_performance_metrics_to_domain(
    response: proto::GetPerformanceMetricsResponse,
) -> Result<PerformanceMetrics, ClientError> {
    Ok(PerformanceMetrics {
        total_tasks: response.total_tasks,
        active_tasks: response.active_tasks,
        completed_tasks: response.completed_tasks,
        failed_tasks: response.failed_tasks,
        completion_rate: response.completion_rate,
        error_rate: response.error_rate,
        average_task_duration_seconds: response.average_task_duration_seconds,
        average_step_duration_seconds: response.average_step_duration_seconds,
        tasks_per_hour: response.tasks_per_hour,
        steps_per_hour: response.steps_per_hour,
        system_health_score: response.system_health_score,
        analysis_period_start: response.analysis_period_start,
        calculated_at: response.calculated_at,
    })
}

/// Convert proto GetBottleneckAnalysisResponse to domain.
///
/// # Errors
/// Returns `InvalidResponse` if resource_utilization or system_health is missing.
pub fn proto_bottleneck_to_domain(
    response: proto::GetBottleneckAnalysisResponse,
) -> Result<BottleneckAnalysis, ClientError> {
    let resource_utilization = proto_resource_utilization_to_domain(response.resource_utilization)?;

    Ok(BottleneckAnalysis {
        slow_steps: response
            .slow_steps
            .into_iter()
            .map(|s| SlowStepInfo {
                namespace_name: s.namespace_name,
                task_name: s.task_name,
                version: s.version,
                step_name: s.step_name,
                average_duration_seconds: s.average_duration_seconds,
                max_duration_seconds: s.max_duration_seconds,
                execution_count: s.execution_count,
                error_count: s.error_count,
                error_rate: s.error_rate,
                last_executed_at: s.last_executed_at,
            })
            .collect(),
        slow_tasks: response
            .slow_tasks
            .into_iter()
            .map(|t| SlowTaskInfo {
                namespace_name: t.namespace_name,
                task_name: t.task_name,
                version: t.version,
                average_duration_seconds: t.average_duration_seconds,
                max_duration_seconds: t.max_duration_seconds,
                execution_count: t.execution_count,
                average_step_count: t.average_step_count,
                error_count: t.error_count,
                error_rate: t.error_rate,
                last_executed_at: t.last_executed_at,
            })
            .collect(),
        resource_utilization,
        recommendations: response.recommendations,
    })
}

fn proto_resource_utilization_to_domain(
    utilization: Option<proto::ResourceUtilization>,
) -> Result<ResourceUtilization, ClientError> {
    let u = utilization.ok_or_else(|| {
        ClientError::invalid_response(
            "BottleneckAnalysis.resource_utilization",
            "Bottleneck analysis missing required resource utilization",
        )
    })?;

    let health = u.system_health.ok_or_else(|| {
        ClientError::invalid_response(
            "ResourceUtilization.system_health",
            "Resource utilization missing required system health counts",
        )
    })?;

    Ok(ResourceUtilization {
        database_pool_utilization: u.database_pool_utilization,
        system_health: SystemHealthCounts {
            pending_tasks: health.pending_tasks,
            initializing_tasks: health.initializing_tasks,
            enqueuing_steps_tasks: health.enqueuing_steps_tasks,
            steps_in_process_tasks: health.steps_in_process_tasks,
            evaluating_results_tasks: health.evaluating_results_tasks,
            waiting_for_dependencies_tasks: health.waiting_for_dependencies_tasks,
            waiting_for_retry_tasks: health.waiting_for_retry_tasks,
            blocked_by_failures_tasks: health.blocked_by_failures_tasks,
            complete_tasks: health.complete_tasks,
            error_tasks: health.error_tasks,
            cancelled_tasks: health.cancelled_tasks,
            resolved_manually_tasks: health.resolved_manually_tasks,
            total_tasks: health.total_tasks,
            pending_steps: health.pending_steps,
            enqueued_steps: health.enqueued_steps,
            in_progress_steps: health.in_progress_steps,
            enqueued_for_orchestration_steps: health.enqueued_for_orchestration_steps,
            enqueued_as_error_for_orchestration_steps: health
                .enqueued_as_error_for_orchestration_steps,
            waiting_for_retry_steps: health.waiting_for_retry_steps,
            complete_steps: health.complete_steps,
            error_steps: health.error_steps,
            cancelled_steps: health.cancelled_steps,
            resolved_manually_steps: health.resolved_manually_steps,
            total_steps: health.total_steps,
        },
    })
}

// ============================================================================
// Config Response Conversions
// ============================================================================

use tasker_shared::types::api::orchestration::{
    OrchestrationConfigResponse, SafeCircuitBreakerConfig, SafeDatabasePoolConfig,
};

/// Convert proto GetConfigResponse to domain.
///
/// # Errors
/// Returns `InvalidResponse` if required config sections are missing.
pub fn proto_config_to_domain(
    response: proto::GetConfigResponse,
) -> Result<OrchestrationConfigResponse, ClientError> {
    let metadata = response.metadata.ok_or_else(|| {
        ClientError::invalid_response(
            "GetConfigResponse.metadata",
            "Config response missing required metadata",
        )
    })?;

    let auth = response
        .auth
        .map(|a| SafeAuthConfig {
            enabled: a.enabled,
            verification_method: a.verification_method,
            jwt_issuer: a.jwt_issuer,
            jwt_audience: a.jwt_audience,
            api_key_header: a.api_key_header,
            api_key_count: a.api_key_count as usize,
            strict_validation: a.strict_validation,
            allowed_algorithms: a.allowed_algorithms,
        })
        .ok_or_else(|| {
            ClientError::invalid_response(
                "GetConfigResponse.auth",
                "Config response missing required auth section",
            )
        })?;

    let circuit_breakers = response
        .circuit_breakers
        .map(|c| SafeCircuitBreakerConfig {
            enabled: c.enabled,
            failure_threshold: c.failure_threshold,
            success_threshold: c.success_threshold,
            timeout_seconds: c.timeout_seconds,
        })
        .ok_or_else(|| {
            ClientError::invalid_response(
                "GetConfigResponse.circuit_breakers",
                "Config response missing required circuit_breakers section",
            )
        })?;

    let database_pools = response
        .database_pools
        .map(|d| SafeDatabasePoolConfig {
            web_api_pool_size: d.web_api_pool_size,
            web_api_max_connections: d.web_api_max_connections,
        })
        .ok_or_else(|| {
            ClientError::invalid_response(
                "GetConfigResponse.database_pools",
                "Config response missing required database_pools section",
            )
        })?;

    let messaging = response
        .messaging
        .map(|m| SafeMessagingConfig {
            backend: m.backend,
            queues: m.queues,
        })
        .ok_or_else(|| {
            ClientError::invalid_response(
                "GetConfigResponse.messaging",
                "Config response missing required messaging section",
            )
        })?;

    Ok(OrchestrationConfigResponse {
        metadata: ConfigMetadata {
            timestamp: proto_timestamp_to_datetime(metadata.timestamp),
            environment: metadata.environment.clone(),
            version: metadata.version,
        },
        auth,
        circuit_breakers,
        database_pools,
        deployment_mode: metadata.environment,
        messaging,
    })
}

// ============================================================================
// DLQ Response Conversions
// ============================================================================

use tasker_shared::models::orchestration::{
    DlqEntry, DlqInvestigationQueueEntry, DlqReason, DlqResolutionStatus, DlqStats,
    StalenessHealthStatus, StalenessMonitoring,
};

/// Convert domain DlqResolutionStatus to proto.
pub fn dlq_resolution_status_to_proto(status: &DlqResolutionStatus) -> proto::DlqResolutionStatus {
    match status {
        DlqResolutionStatus::Pending => proto::DlqResolutionStatus::Pending,
        DlqResolutionStatus::ManuallyResolved => proto::DlqResolutionStatus::ManuallyResolved,
        DlqResolutionStatus::PermanentlyFailed => proto::DlqResolutionStatus::PermanentlyFailed,
        DlqResolutionStatus::Cancelled => proto::DlqResolutionStatus::Cancelled,
    }
}

fn proto_dlq_resolution_status_to_domain(status: i32) -> DlqResolutionStatus {
    match proto::DlqResolutionStatus::try_from(status) {
        Ok(proto::DlqResolutionStatus::Pending) => DlqResolutionStatus::Pending,
        Ok(proto::DlqResolutionStatus::ManuallyResolved) => DlqResolutionStatus::ManuallyResolved,
        Ok(proto::DlqResolutionStatus::PermanentlyFailed) => DlqResolutionStatus::PermanentlyFailed,
        Ok(proto::DlqResolutionStatus::Cancelled) => DlqResolutionStatus::Cancelled,
        _ => DlqResolutionStatus::Pending,
    }
}

fn proto_dlq_reason_to_domain(reason: i32) -> DlqReason {
    match proto::DlqReason::try_from(reason) {
        Ok(proto::DlqReason::StalenessTimeout) => DlqReason::StalenessTimeout,
        Ok(proto::DlqReason::MaxRetriesExceeded) => DlqReason::MaxRetriesExceeded,
        Ok(proto::DlqReason::DependencyCycleDetected) => DlqReason::DependencyCycleDetected,
        Ok(proto::DlqReason::WorkerUnavailable) => DlqReason::WorkerUnavailable,
        Ok(proto::DlqReason::ManualDlq) => DlqReason::ManualDlq,
        _ => DlqReason::StalenessTimeout,
    }
}

/// Convert proto DlqEntry to domain.
pub fn proto_dlq_entry_to_domain(entry: proto::DlqEntry) -> Result<DlqEntry, ClientError> {
    Ok(DlqEntry {
        dlq_entry_uuid: entry
            .dlq_entry_uuid
            .parse()
            .map_err(|_| ClientError::Internal("Invalid DLQ entry UUID".to_string()))?,
        task_uuid: entry
            .task_uuid
            .parse()
            .map_err(|_| ClientError::Internal("Invalid task UUID".to_string()))?,
        original_state: entry.original_state,
        dlq_reason: proto_dlq_reason_to_domain(entry.dlq_reason),
        dlq_timestamp: proto_timestamp_to_datetime(entry.dlq_timestamp).naive_utc(),
        resolution_status: proto_dlq_resolution_status_to_domain(entry.resolution_status),
        resolution_timestamp: entry
            .resolution_timestamp
            .map(|ts| proto_timestamp_to_datetime(Some(ts)).naive_utc()),
        resolution_notes: entry.resolution_notes,
        resolved_by: entry.resolved_by,
        task_snapshot: entry
            .task_snapshot
            .map(proto_struct_to_json)
            .unwrap_or(serde_json::Value::Null),
        metadata: entry.metadata.map(proto_struct_to_json),
        created_at: proto_timestamp_to_datetime(entry.created_at).naive_utc(),
        updated_at: proto_timestamp_to_datetime(entry.updated_at).naive_utc(),
    })
}

/// Convert proto DlqStats to domain.
pub fn proto_dlq_stats_to_domain(stats: proto::DlqStats) -> Result<DlqStats, ClientError> {
    Ok(DlqStats {
        dlq_reason: proto_dlq_reason_to_domain(stats.dlq_reason),
        total_entries: stats.total_entries,
        pending: stats.pending,
        manually_resolved: stats.manually_resolved,
        permanent_failures: stats.permanent_failures,
        cancelled: stats.cancelled,
        oldest_entry: stats
            .oldest_entry
            .map(|ts| proto_timestamp_to_datetime(Some(ts)).naive_utc()),
        newest_entry: stats
            .newest_entry
            .map(|ts| proto_timestamp_to_datetime(Some(ts)).naive_utc()),
        avg_resolution_time_minutes: stats.avg_resolution_time_minutes,
    })
}

/// Convert proto DlqInvestigationQueueEntry to domain.
pub fn proto_dlq_queue_entry_to_domain(
    entry: proto::DlqInvestigationQueueEntry,
) -> Result<DlqInvestigationQueueEntry, ClientError> {
    Ok(DlqInvestigationQueueEntry {
        dlq_entry_uuid: entry
            .dlq_entry_uuid
            .parse()
            .map_err(|_| ClientError::Internal("Invalid DLQ entry UUID".to_string()))?,
        task_uuid: entry
            .task_uuid
            .parse()
            .map_err(|_| ClientError::Internal("Invalid task UUID".to_string()))?,
        original_state: entry.original_state,
        dlq_reason: proto_dlq_reason_to_domain(entry.dlq_reason),
        dlq_timestamp: proto_timestamp_to_datetime(entry.dlq_timestamp).naive_utc(),
        minutes_in_dlq: entry.minutes_in_dlq,
        namespace_name: entry.namespace_name,
        task_name: entry.task_name,
        current_state: entry.current_state,
        time_in_state_minutes: entry.time_in_state_minutes,
        priority_score: entry.priority_score,
    })
}

fn proto_staleness_health_to_domain(status: i32) -> StalenessHealthStatus {
    match proto::StalenessHealthStatus::try_from(status) {
        Ok(proto::StalenessHealthStatus::Healthy) => StalenessHealthStatus::Healthy,
        Ok(proto::StalenessHealthStatus::Warning) => StalenessHealthStatus::Warning,
        Ok(proto::StalenessHealthStatus::Stale) => StalenessHealthStatus::Stale,
        _ => StalenessHealthStatus::Healthy,
    }
}

/// Convert proto StalenessMonitoringEntry to domain.
pub fn proto_staleness_to_domain(
    entry: proto::StalenessMonitoringEntry,
) -> Result<StalenessMonitoring, ClientError> {
    Ok(StalenessMonitoring {
        task_uuid: entry
            .task_uuid
            .parse()
            .map_err(|_| ClientError::Internal("Invalid task UUID".to_string()))?,
        namespace_name: entry.namespace_name,
        task_name: entry.task_name,
        current_state: entry.current_state,
        time_in_state_minutes: entry.time_in_state_minutes,
        task_age_minutes: entry.task_age_minutes,
        staleness_threshold_minutes: entry.staleness_threshold_minutes,
        health_status: proto_staleness_health_to_domain(entry.health_status),
        priority: entry.priority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proto_timestamp_to_datetime() {
        let ts = prost_types::Timestamp {
            seconds: 1704067200,
            nanos: 0,
        };
        let dt = proto_timestamp_to_datetime(Some(ts));
        assert_eq!(dt.timestamp(), 1704067200);
    }

    #[test]
    fn test_proto_timestamp_none_returns_default() {
        let dt = proto_timestamp_to_datetime(None);
        assert_eq!(dt.timestamp(), 0);
    }

    #[test]
    fn test_proto_task_state_to_string() {
        assert_eq!(
            proto_task_state_to_string(proto::TaskState::Pending as i32),
            "pending"
        );
        assert_eq!(
            proto_task_state_to_string(proto::TaskState::Complete as i32),
            "complete"
        );
        assert_eq!(
            proto_task_state_to_string(proto::TaskState::Error as i32),
            "error"
        );
        assert_eq!(proto_task_state_to_string(999), "unspecified");
    }

    #[test]
    fn test_proto_step_state_to_string() {
        assert_eq!(
            proto_step_state_to_string(proto::StepState::Pending as i32),
            "pending"
        );
        assert_eq!(
            proto_step_state_to_string(proto::StepState::InProgress as i32),
            "in_progress"
        );
        assert_eq!(
            proto_step_state_to_string(proto::StepState::Complete as i32),
            "complete"
        );
        assert_eq!(proto_step_state_to_string(999), "unspecified");
    }

    #[test]
    fn test_json_struct_conversion() {
        use prost_types::value::Kind;

        let proto_struct = prost_types::Struct {
            fields: [
                (
                    "name".to_string(),
                    prost_types::Value {
                        kind: Some(Kind::StringValue("test".to_string())),
                    },
                ),
                (
                    "count".to_string(),
                    prost_types::Value {
                        kind: Some(Kind::NumberValue(42.0)),
                    },
                ),
                (
                    "active".to_string(),
                    prost_types::Value {
                        kind: Some(Kind::BoolValue(true)),
                    },
                ),
            ]
            .into_iter()
            .collect(),
        };

        let json = proto_struct_to_json_opt(Some(proto_struct));

        assert_eq!(json["name"], "test");
        assert_eq!(json["count"], 42.0);
        assert_eq!(json["active"], true);
    }
}

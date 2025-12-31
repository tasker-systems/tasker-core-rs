//! Data Transfer Objects for TypeScript FFI
//!
//! These DTOs represent the exact JSON structure that TypeScript expects.
//! Using explicit structs with `From` implementations provides:
//! - Compile-time guarantees on field mappings
//! - Clear intentional divergence between Rust internals and FFI contract
//! - Proper serde handling for enums and complex types
//!
//! ## TypeScript Binding Generation
//!
//! These DTOs use `ts-rs` to automatically generate TypeScript type definitions.
//! Run `cargo test export_bindings --package tasker-worker-ts` to regenerate
//! the TypeScript types in `workers/typescript/src/ffi/generated/`.

use std::collections::HashMap;

use serde::Serialize;
use tasker_shared::events::domain_events::DomainEvent;
use tasker_shared::messaging::{StepExecutionError, StepExecutionResult};
use tasker_shared::models::core::{
    task::TaskForOrchestration,
    task_template::{HandlerDefinition, RetryConfiguration, StepDefinition},
    workflow_step::WorkflowStepWithName,
};
use tasker_worker::worker::{FfiDispatchMetrics, FfiStepEvent};

#[cfg(test)]
use ts_rs::TS;

/// DTO for FfiStepEvent - the main event payload sent to TypeScript handlers
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct FfiStepEventDto {
    pub event_id: String,
    pub task_uuid: String,
    pub step_uuid: String,
    pub correlation_id: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub task_correlation_id: String,
    pub parent_correlation_id: Option<String>,
    pub task: TaskDto,
    pub workflow_step: WorkflowStepDto,
    pub step_definition: StepDefinitionDto,
    pub dependency_results: HashMap<String, DependencyResultDto>,
}

impl From<&FfiStepEvent> for FfiStepEventDto {
    fn from(event: &FfiStepEvent) -> Self {
        let payload = &event.execution_event.payload;
        let task_sequence_step = &payload.task_sequence_step;

        Self {
            event_id: event.event_id.to_string(),
            task_uuid: event.task_uuid.to_string(),
            step_uuid: event.step_uuid.to_string(),
            correlation_id: event.correlation_id.to_string(),
            trace_id: event.trace_id.clone(),
            span_id: event.span_id.clone(),
            task_correlation_id: task_sequence_step.task.task.correlation_id.to_string(),
            parent_correlation_id: task_sequence_step
                .task
                .task
                .parent_correlation_id
                .map(|id| id.to_string()),
            task: TaskDto::from(&task_sequence_step.task),
            workflow_step: WorkflowStepDto::from(&task_sequence_step.workflow_step),
            step_definition: StepDefinitionDto::from(&task_sequence_step.step_definition),
            dependency_results: task_sequence_step
                .dependency_results
                .iter()
                .map(|(k, v)| (k.clone(), DependencyResultDto::from(v)))
                .collect(),
        }
    }
}

/// DTO for Task information
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct TaskDto {
    pub task_uuid: String,
    pub named_task_uuid: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    #[cfg_attr(test, ts(type = "Record<string, unknown> | null"))]
    pub context: Option<serde_json::Value>,
    pub correlation_id: String,
    pub parent_correlation_id: Option<String>,
    pub complete: bool,
    pub priority: i32,
    pub initiator: Option<String>,
    pub source_system: Option<String>,
    pub reason: Option<String>,
    #[cfg_attr(test, ts(type = "Record<string, unknown> | null"))]
    pub tags: Option<serde_json::Value>,
    pub identity_hash: String,
    pub created_at: String,
    pub updated_at: String,
    pub requested_at: String,
}

impl From<&TaskForOrchestration> for TaskDto {
    fn from(task_orch: &TaskForOrchestration) -> Self {
        let task = &task_orch.task;
        Self {
            task_uuid: task.task_uuid.to_string(),
            named_task_uuid: task.named_task_uuid.to_string(),
            name: task_orch.task_name.clone(),
            namespace: task_orch.namespace_name.clone(),
            version: task_orch.task_version.clone(),
            context: task.context.clone(),
            correlation_id: task.correlation_id.to_string(),
            parent_correlation_id: task.parent_correlation_id.map(|id| id.to_string()),
            complete: task.complete,
            priority: task.priority,
            initiator: task.initiator.clone(),
            source_system: task.source_system.clone(),
            reason: task.reason.clone(),
            tags: task.tags.clone(),
            identity_hash: task.identity_hash.clone(),
            created_at: task.created_at.to_string(),
            updated_at: task.updated_at.to_string(),
            requested_at: task.requested_at.to_string(),
        }
    }
}

/// DTO for WorkflowStep information (uses WorkflowStepWithName which has name fields)
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct WorkflowStepDto {
    pub workflow_step_uuid: String,
    pub task_uuid: String,
    pub named_step_uuid: String,
    pub name: String,
    pub template_step_name: String,
    pub retryable: bool,
    pub max_attempts: i32,
    pub attempts: i32,
    pub in_process: bool,
    pub processed: bool,
    pub skippable: bool,
    #[cfg_attr(test, ts(type = "Record<string, unknown> | null"))]
    pub inputs: Option<serde_json::Value>,
    #[cfg_attr(test, ts(type = "Record<string, unknown> | null"))]
    pub results: Option<serde_json::Value>,
    pub backoff_request_seconds: Option<i32>,
    pub processed_at: Option<String>,
    pub last_attempted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&WorkflowStepWithName> for WorkflowStepDto {
    fn from(step: &WorkflowStepWithName) -> Self {
        Self {
            workflow_step_uuid: step.workflow_step_uuid.to_string(),
            task_uuid: step.task_uuid.to_string(),
            named_step_uuid: step.named_step_uuid.to_string(),
            name: step.name.clone(),
            template_step_name: step.template_step_name.clone(),
            retryable: step.retryable,
            max_attempts: step.max_attempts.unwrap_or(1),
            attempts: step.attempts.unwrap_or(0),
            in_process: step.in_process,
            processed: step.processed,
            skippable: step.skippable,
            inputs: step.inputs.clone(),
            results: step.results.clone(),
            backoff_request_seconds: step.backoff_request_seconds,
            processed_at: step.processed_at.map(|t| t.to_string()),
            last_attempted_at: step.last_attempted_at.map(|t| t.to_string()),
            created_at: step.created_at.to_string(),
            updated_at: step.updated_at.to_string(),
        }
    }
}

/// DTO for StepDefinition (from task template)
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct StepDefinitionDto {
    pub name: String,
    pub description: Option<String>,
    pub handler: HandlerDefinitionDto,
    pub system_dependency: Option<String>,
    pub dependencies: Vec<String>,
    pub timeout_seconds: Option<u64>,
    pub retry: RetryConfigurationDto,
}

impl From<&StepDefinition> for StepDefinitionDto {
    fn from(def: &StepDefinition) -> Self {
        Self {
            name: def.name.clone(),
            description: def.description.clone(),
            handler: HandlerDefinitionDto::from(&def.handler),
            system_dependency: def.system_dependency.clone(),
            dependencies: def.dependencies.clone(),
            timeout_seconds: def.timeout_seconds.map(|v| v as u64),
            retry: RetryConfigurationDto::from(&def.retry),
        }
    }
}

/// DTO for HandlerDefinition
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct HandlerDefinitionDto {
    pub callable: String,
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub initialization: serde_json::Value,
}

impl From<&HandlerDefinition> for HandlerDefinitionDto {
    fn from(handler: &HandlerDefinition) -> Self {
        Self {
            callable: handler.callable.clone(),
            // Convert HashMap to Value for consistent JSON output
            initialization: serde_json::to_value(&handler.initialization)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        }
    }
}

/// DTO for RetryConfiguration
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct RetryConfigurationDto {
    pub retryable: bool,
    pub max_attempts: u32,
    pub backoff: String,
    pub backoff_base_ms: Option<u64>,
    pub max_backoff_ms: Option<u64>,
}

impl From<&RetryConfiguration> for RetryConfigurationDto {
    fn from(retry: &RetryConfiguration) -> Self {
        Self {
            retryable: retry.retryable,
            max_attempts: retry.max_attempts,
            // Serialize enum variant name in snake_case
            backoff: format!("{:?}", retry.backoff).to_lowercase(),
            backoff_base_ms: retry.backoff_base_ms,
            max_backoff_ms: retry.max_backoff_ms,
        }
    }
}

/// DTO for dependency step results
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct DependencyResultDto {
    pub step_uuid: String,
    pub success: bool,
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub result: serde_json::Value,
    pub status: String,
    pub error: Option<StepExecutionErrorDto>,
}

impl From<&StepExecutionResult> for DependencyResultDto {
    fn from(result: &StepExecutionResult) -> Self {
        Self {
            step_uuid: result.step_uuid.to_string(),
            success: result.success,
            result: result.result.clone(),
            status: result.status.clone(),
            error: result.error.as_ref().map(StepExecutionErrorDto::from),
        }
    }
}

/// DTO for step execution errors
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct StepExecutionErrorDto {
    pub message: String,
    pub error_type: Option<String>,
    pub retryable: bool,
    pub status_code: Option<u16>,
    pub backtrace: Option<Vec<String>>,
}

impl From<&StepExecutionError> for StepExecutionErrorDto {
    fn from(error: &StepExecutionError) -> Self {
        Self {
            message: error.message.clone(),
            error_type: error.error_type.clone(),
            retryable: error.retryable,
            status_code: error.status_code,
            backtrace: error.backtrace.clone(),
        }
    }
}

/// DTO for FFI dispatch metrics
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct FfiDispatchMetricsDto {
    pub pending_count: usize,
    pub starvation_detected: bool,
    pub starving_event_count: usize,
    pub oldest_pending_age_ms: Option<u64>,
    pub newest_pending_age_ms: Option<u64>,
    pub oldest_event_id: Option<String>,
}

impl From<&FfiDispatchMetrics> for FfiDispatchMetricsDto {
    fn from(metrics: &FfiDispatchMetrics) -> Self {
        Self {
            pending_count: metrics.pending_count,
            starvation_detected: metrics.starvation_detected,
            starving_event_count: metrics.starving_event_count,
            oldest_pending_age_ms: metrics.oldest_pending_age_ms,
            newest_pending_age_ms: metrics.newest_pending_age_ms,
            oldest_event_id: metrics.oldest_event_id.map(|id| id.to_string()),
        }
    }
}

// =============================================================================
// Domain Event DTOs (TAS-112: Type-safe FFI for in-process events)
// =============================================================================

/// DTO for domain event metadata sent to TypeScript via FFI
///
/// This provides compile-time type safety for the FFI boundary,
/// replacing the previous manual json!() macro construction.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct FfiDomainEventMetadataDto {
    pub task_uuid: String,
    pub step_uuid: Option<String>,
    pub step_name: Option<String>,
    pub namespace: String,
    pub correlation_id: String,
    pub fired_at: String,
    pub fired_by: Option<String>,
}

/// DTO for domain events sent to TypeScript via FFI (in-process event bus)
///
/// Provides type-safe serialization for the fast path domain events.
/// The payload contains the full DomainEventPayload which includes:
/// - task_sequence_step: Complete execution context
/// - execution_result: Step result with success/failure/data
/// - payload: Business-specific event data
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(TS))]
#[cfg_attr(test, ts(export, export_to = "src/ffi/generated/"))]
pub struct FfiDomainEventDto {
    pub event_id: String,
    pub event_name: String,
    pub event_version: String,
    pub metadata: FfiDomainEventMetadataDto,
    /// The full domain event payload (task_sequence_step, execution_result, payload)
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    pub payload: serde_json::Value,
}

impl From<&DomainEvent> for FfiDomainEventDto {
    fn from(event: &DomainEvent) -> Self {
        Self {
            event_id: event.event_id.to_string(),
            event_name: event.event_name.clone(),
            event_version: event.event_version.clone(),
            metadata: FfiDomainEventMetadataDto {
                task_uuid: event.metadata.task_uuid.to_string(),
                step_uuid: event.metadata.step_uuid.map(|id| id.to_string()),
                step_name: event.metadata.step_name.clone(),
                namespace: event.metadata.namespace.clone(),
                correlation_id: event.metadata.correlation_id.to_string(),
                fired_at: event.metadata.fired_at.to_rfc3339(),
                // Note: fired_by is String in Rust but Option<String> in TS for flexibility
                fired_by: Some(event.metadata.fired_by.clone()),
            },
            // Serialize the full payload (includes task_sequence_step, execution_result, payload)
            payload: serde_json::to_value(&event.payload)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        }
    }
}

/// Convert FfiDomainEventDto to JSON string for FFI transport
impl FfiDomainEventDto {
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_dto_serialization() {
        let metrics = FfiDispatchMetrics {
            pending_count: 5,
            starvation_detected: false,
            starving_event_count: 0,
            oldest_pending_age_ms: Some(100),
            newest_pending_age_ms: Some(10),
            oldest_event_id: None,
        };

        let dto = FfiDispatchMetricsDto::from(&metrics);
        let json = serde_json::to_string(&dto).unwrap();

        assert!(json.contains("\"pending_count\":5"));
        assert!(json.contains("\"starvation_detected\":false"));
    }

    /// Export all TypeScript bindings to `workers/typescript/src/ffi/generated/`
    ///
    /// Run with: `cargo test export_bindings --package tasker-worker-ts`
    ///
    /// This test generates TypeScript type definitions from the Rust DTOs,
    /// ensuring the TypeScript types are always in sync with the Rust source of truth.
    #[test]
    fn export_bindings() {
        // ts-rs export_to paths are relative to CARGO_MANIFEST_DIR
        // Our Cargo.toml is at workers/typescript/Cargo.toml
        // So export_to = "src/ffi/generated/" writes to workers/typescript/src/ffi/generated/
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let output_dir = std::path::Path::new(manifest_dir).join("src/ffi/generated");
        std::fs::create_dir_all(&output_dir).expect("Failed to create generated directory");

        println!("CARGO_MANIFEST_DIR: {}", manifest_dir);
        println!("Output directory: {}", output_dir.display());

        // Export all DTOs - ts-rs handles this automatically via #[ts(export)]
        // The types are exported when their TS::export() method is called
        FfiStepEventDto::export_all().expect("Failed to export FfiStepEventDto");
        TaskDto::export_all().expect("Failed to export TaskDto");
        WorkflowStepDto::export_all().expect("Failed to export WorkflowStepDto");
        StepDefinitionDto::export_all().expect("Failed to export StepDefinitionDto");
        HandlerDefinitionDto::export_all().expect("Failed to export HandlerDefinitionDto");
        RetryConfigurationDto::export_all().expect("Failed to export RetryConfigurationDto");
        DependencyResultDto::export_all().expect("Failed to export DependencyResultDto");
        StepExecutionErrorDto::export_all().expect("Failed to export StepExecutionErrorDto");
        FfiDispatchMetricsDto::export_all().expect("Failed to export FfiDispatchMetricsDto");

        // Domain Event DTOs (TAS-112)
        FfiDomainEventDto::export_all().expect("Failed to export FfiDomainEventDto");
        FfiDomainEventMetadataDto::export_all().expect("Failed to export FfiDomainEventMetadataDto");

        println!("✅ TypeScript bindings exported to {}", output_dir.display());
    }
}

//! Type conversions between Protocol Buffer types and domain types.
//!
//! This module provides bidirectional conversion between the gRPC proto types
//! (defined in `tasker_shared::proto`) and the domain types used throughout
//! the application.
//!
//! Core type conversions (`From`/`Into` traits) are defined in `tasker_shared::proto::conversions`.
//! This module provides additional orchestration-specific conversion utilities.

use serde_json::Value as JsonValue;
use tasker_shared::proto::v1 as proto;
use tasker_shared::state_machine::states::{TaskState, WorkflowStepState};
use uuid::Uuid;

// Re-export commonly used conversion from tasker-shared
pub use tasker_shared::proto::conversions::datetime_to_timestamp;

// ============================================================================
// Task State Conversions
// ============================================================================

/// Convert task state string to proto (convenience for string-based state storage).
pub fn task_state_to_proto(state: &str) -> proto::TaskState {
    TaskState::try_from(state)
        .map(proto::TaskState::from)
        .unwrap_or(proto::TaskState::Unspecified)
}

/// Convert proto TaskState to domain state string.
pub fn proto_to_task_state(state: proto::TaskState) -> String {
    TaskState::try_from(state)
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|_| "unspecified".to_string())
}

// ============================================================================
// Step State Conversions
// ============================================================================

/// Convert step state string to proto (convenience for string-based state storage).
pub fn step_state_to_proto(state: &str) -> proto::StepState {
    WorkflowStepState::try_from(state)
        .map(proto::StepState::from)
        .unwrap_or(proto::StepState::Unspecified)
}

// ============================================================================
// JSON/Struct Conversions
// ============================================================================

/// Convert serde_json Value to prost_types Struct.
pub fn json_to_struct(value: JsonValue) -> Option<prost_types::Struct> {
    match value {
        JsonValue::Object(map) => {
            let fields = map
                .into_iter()
                .map(|(k, v)| (k, json_to_prost_value(v)))
                .collect();
            Some(prost_types::Struct { fields })
        }
        _ => None,
    }
}

fn json_to_prost_value(value: JsonValue) -> prost_types::Value {
    use prost_types::value::Kind;
    let kind = match value {
        JsonValue::Null => Kind::NullValue(0),
        JsonValue::Bool(b) => Kind::BoolValue(b),
        JsonValue::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        JsonValue::String(s) => Kind::StringValue(s),
        JsonValue::Array(arr) => Kind::ListValue(prost_types::ListValue {
            values: arr.into_iter().map(json_to_prost_value).collect(),
        }),
        JsonValue::Object(map) => Kind::StructValue(prost_types::Struct {
            fields: map
                .into_iter()
                .map(|(k, v)| (k, json_to_prost_value(v)))
                .collect(),
        }),
    };
    prost_types::Value { kind: Some(kind) }
}

/// Convert prost_types Struct to serde_json Value.
pub fn struct_to_json(s: prost_types::Struct) -> JsonValue {
    JsonValue::Object(
        s.fields
            .into_iter()
            .map(|(k, v)| (k, prost_value_to_json(v)))
            .collect(),
    )
}

fn prost_value_to_json(value: prost_types::Value) -> JsonValue {
    use prost_types::value::Kind;
    match value.kind {
        Some(Kind::NullValue(_)) => JsonValue::Null,
        Some(Kind::BoolValue(b)) => JsonValue::Bool(b),
        Some(Kind::NumberValue(n)) => {
            serde_json::Number::from_f64(n).map_or(JsonValue::Null, JsonValue::Number)
        }
        Some(Kind::StringValue(s)) => JsonValue::String(s),
        Some(Kind::ListValue(l)) => {
            JsonValue::Array(l.values.into_iter().map(prost_value_to_json).collect())
        }
        Some(Kind::StructValue(s)) => struct_to_json(s),
        None => JsonValue::Null,
    }
}

// ============================================================================
// UUID Parsing
// ============================================================================

/// Parse a UUID string, returning a Status error on failure.
#[expect(
    clippy::result_large_err,
    reason = "tonic::Status is the standard gRPC error type"
)]
pub fn parse_uuid(s: &str) -> Result<Uuid, tonic::Status> {
    Uuid::parse_str(s).map_err(|e| tonic::Status::invalid_argument(format!("Invalid UUID: {e}")))
}

// ============================================================================
// Error Conversions
// ============================================================================

/// Convert a TaskServiceError to a gRPC Status.
pub fn task_service_error_to_status(error: &crate::services::TaskServiceError) -> tonic::Status {
    use crate::services::TaskServiceError;
    match error {
        TaskServiceError::NotFound(_) => tonic::Status::not_found(error.to_string()),
        TaskServiceError::Validation(_)
        | TaskServiceError::TemplateNotFound(_)
        | TaskServiceError::InvalidConfiguration(_)
        | TaskServiceError::DuplicateTask(_)
        | TaskServiceError::CannotCancel(_) => tonic::Status::invalid_argument(error.to_string()),
        TaskServiceError::Backpressure {
            retry_after_seconds,
            ..
        } => {
            let mut status = tonic::Status::unavailable(error.to_string());
            status.metadata_mut().insert(
                "retry-after",
                retry_after_seconds.to_string().parse().unwrap(),
            );
            status
        }
        TaskServiceError::CircuitBreakerOpen => tonic::Status::unavailable(error.to_string()),
        TaskServiceError::Database(_) | TaskServiceError::Internal(_) => {
            tonic::Status::internal(error.to_string())
        }
    }
}

/// Convert a StepServiceError to a gRPC Status.
pub fn step_service_error_to_status(error: &crate::services::StepServiceError) -> tonic::Status {
    use crate::services::StepServiceError;
    match error {
        StepServiceError::NotFound(_) => tonic::Status::not_found(error.to_string()),
        StepServiceError::Validation(_)
        | StepServiceError::InvalidTransition(_)
        | StepServiceError::OwnershipMismatch => tonic::Status::invalid_argument(error.to_string()),
        StepServiceError::Database(_) | StepServiceError::Internal(_) => {
            tonic::Status::internal(error.to_string())
        }
    }
}

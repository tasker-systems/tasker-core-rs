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
        TaskServiceError::DuplicateTask(_) => tonic::Status::already_exists(error.to_string()),
        TaskServiceError::Validation(_)
        | TaskServiceError::TemplateNotFound(_)
        | TaskServiceError::InvalidConfiguration(_)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- Task state conversions ---

    #[test]
    fn test_task_state_to_proto_valid_states() {
        assert_eq!(task_state_to_proto("pending"), proto::TaskState::Pending);
        assert_eq!(task_state_to_proto("complete"), proto::TaskState::Complete);
        assert_eq!(task_state_to_proto("error"), proto::TaskState::Error);
    }

    #[test]
    fn test_task_state_to_proto_unknown_state() {
        assert_eq!(
            task_state_to_proto("nonexistent"),
            proto::TaskState::Unspecified
        );
        assert_eq!(task_state_to_proto(""), proto::TaskState::Unspecified);
    }

    #[test]
    fn test_proto_to_task_state_valid() {
        assert_eq!(proto_to_task_state(proto::TaskState::Pending), "pending");
        assert_eq!(proto_to_task_state(proto::TaskState::Complete), "complete");
        assert_eq!(proto_to_task_state(proto::TaskState::Error), "error");
    }

    #[test]
    fn test_proto_to_task_state_unspecified() {
        assert_eq!(
            proto_to_task_state(proto::TaskState::Unspecified),
            "unspecified"
        );
    }

    // --- Step state conversions ---

    #[test]
    fn test_step_state_to_proto_valid_states() {
        assert_eq!(step_state_to_proto("pending"), proto::StepState::Pending);
        assert_eq!(
            step_state_to_proto("in_progress"),
            proto::StepState::InProgress
        );
        assert_eq!(step_state_to_proto("complete"), proto::StepState::Complete);
        assert_eq!(step_state_to_proto("error"), proto::StepState::Error);
    }

    #[test]
    fn test_step_state_to_proto_unknown_state() {
        assert_eq!(
            step_state_to_proto("invalid"),
            proto::StepState::Unspecified
        );
    }

    // --- JSON / Struct conversions ---

    #[test]
    fn test_json_to_struct_simple_object() {
        let json = json!({
            "name": "test",
            "value": 42
        });

        let result = json_to_struct(json);
        assert!(result.is_some());

        let s = result.unwrap();
        assert_eq!(s.fields.len(), 2);
        assert!(s.fields.contains_key("name"));
        assert!(s.fields.contains_key("value"));
    }

    #[test]
    fn test_json_to_struct_non_object_returns_none() {
        assert!(json_to_struct(json!("string")).is_none());
        assert!(json_to_struct(json!(42)).is_none());
        assert!(json_to_struct(json!(true)).is_none());
        assert!(json_to_struct(json!(null)).is_none());
        assert!(json_to_struct(json!([1, 2, 3])).is_none());
    }

    #[test]
    fn test_json_to_struct_empty_object() {
        let result = json_to_struct(json!({}));
        assert!(result.is_some());
        assert_eq!(result.unwrap().fields.len(), 0);
    }

    #[test]
    fn test_json_to_struct_nested_object() {
        let json = json!({
            "outer": {
                "inner": "value"
            }
        });

        let result = json_to_struct(json);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.fields.contains_key("outer"));
    }

    #[test]
    fn test_json_struct_roundtrip() {
        let original = json!({
            "string_val": "hello",
            "number_val": 3.14,
            "bool_val": true,
            "null_val": null,
            "array_val": [1, 2, 3],
            "nested": {"key": "value"}
        });

        let proto_struct = json_to_struct(original.clone()).unwrap();
        let roundtripped = struct_to_json(proto_struct);

        assert_eq!(roundtripped["string_val"], "hello");
        assert_eq!(roundtripped["bool_val"], true);
        assert!(roundtripped["null_val"].is_null());
        assert_eq!(roundtripped["nested"]["key"], "value");
    }

    #[test]
    fn test_json_to_prost_value_all_types() {
        // Null
        let v = json_to_prost_value(json!(null));
        assert!(matches!(
            v.kind,
            Some(prost_types::value::Kind::NullValue(_))
        ));

        // Bool
        let v = json_to_prost_value(json!(true));
        assert!(matches!(
            v.kind,
            Some(prost_types::value::Kind::BoolValue(true))
        ));

        // Number
        let v = json_to_prost_value(json!(42));
        assert!(matches!(
            v.kind,
            Some(prost_types::value::Kind::NumberValue(n)) if (n - 42.0).abs() < f64::EPSILON
        ));

        // String
        let v = json_to_prost_value(json!("hello"));
        assert!(matches!(
            v.kind,
            Some(prost_types::value::Kind::StringValue(ref s)) if s == "hello"
        ));

        // Array
        let v = json_to_prost_value(json!([1, 2]));
        assert!(matches!(
            v.kind,
            Some(prost_types::value::Kind::ListValue(_))
        ));

        // Object
        let v = json_to_prost_value(json!({"k": "v"}));
        assert!(matches!(
            v.kind,
            Some(prost_types::value::Kind::StructValue(_))
        ));
    }

    #[test]
    fn test_prost_value_to_json_none_kind() {
        let v = prost_types::Value { kind: None };
        assert!(prost_value_to_json(v).is_null());
    }

    // --- UUID parsing ---

    #[test]
    fn test_parse_uuid_valid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_uuid(uuid_str);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), uuid_str);
    }

    #[test]
    fn test_parse_uuid_invalid() {
        let result = parse_uuid("not-a-uuid");
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("Invalid UUID"));
    }

    #[test]
    fn test_parse_uuid_empty() {
        let result = parse_uuid("");
        assert!(result.is_err());
    }

    // --- Service error conversions ---

    #[test]
    fn test_task_service_error_to_status_not_found() {
        let err = crate::services::TaskServiceError::NotFound(Uuid::now_v7());
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[test]
    fn test_task_service_error_to_status_duplicate() {
        let err = crate::services::TaskServiceError::DuplicateTask("dup".to_string());
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::AlreadyExists);
    }

    #[test]
    fn test_task_service_error_to_status_validation() {
        let err = crate::services::TaskServiceError::Validation("bad input".to_string());
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_task_service_error_to_status_template_not_found() {
        let err = crate::services::TaskServiceError::TemplateNotFound("missing".to_string());
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_task_service_error_to_status_backpressure() {
        let err = crate::services::TaskServiceError::Backpressure {
            reason: "queue full".to_string(),
            retry_after_seconds: 30,
        };
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::Unavailable);
        assert!(status.metadata().contains_key("retry-after"));
    }

    #[test]
    fn test_task_service_error_to_status_circuit_breaker() {
        let err = crate::services::TaskServiceError::CircuitBreakerOpen;
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::Unavailable);
    }

    #[test]
    fn test_task_service_error_to_status_database() {
        let err = crate::services::TaskServiceError::Database("connection failed".to_string());
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn test_task_service_error_to_status_internal() {
        let err = crate::services::TaskServiceError::Internal("unexpected".to_string());
        let status = task_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn test_step_service_error_to_status_not_found() {
        let err = crate::services::StepServiceError::NotFound(Uuid::now_v7());
        let status = step_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[test]
    fn test_step_service_error_to_status_validation() {
        let err = crate::services::StepServiceError::Validation("invalid".to_string());
        let status = step_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_step_service_error_to_status_ownership_mismatch() {
        let err = crate::services::StepServiceError::OwnershipMismatch;
        let status = step_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_step_service_error_to_status_database() {
        let err = crate::services::StepServiceError::Database("query failed".to_string());
        let status = step_service_error_to_status(&err);
        assert_eq!(status.code(), tonic::Code::Internal);
    }
}

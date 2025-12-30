//! # TAS-112: Ergonomic Handler Capability Traits
//!
//! This module re-exports capability traits from `tasker_worker` for backward compatibility.
//! The canonical implementation lives in `tasker_worker::handler_capabilities`.
//!
//! ## Usage
//!
//! For new handlers, prefer importing directly from `tasker_worker`:
//!
//! ```ignore
//! use tasker_worker::{APICapable, DecisionCapable, BatchableCapable};
//! ```
//!
//! For compatibility with existing code, this module provides the same exports:
//!
//! ```ignore
//! use tasker_worker_rust::step_handlers::capabilities::{APICapable, DecisionCapable};
//! ```

// Re-export all capability types from tasker_worker
pub use tasker_worker::handler_capabilities::{
    APICapable, BatchableCapable, DecisionCapable, ErrorClassification, HandlerCapabilities,
};

// Re-export tests module for this crate's test suite
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    // Verify re-exports work correctly
    struct TestHandler;

    impl APICapable for TestHandler {}
    impl DecisionCapable for TestHandler {}
    impl BatchableCapable for TestHandler {}

    #[test]
    fn test_reexported_traits_work() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        // Test APICapable
        let result = handler.api_success(step_uuid, json!({"test": true}), 200, None, 100);
        assert!(result.is_success());

        // Test DecisionCapable
        let result =
            handler.decision_success(step_uuid, vec!["step_a".to_string()], None, 100);
        assert!(result.is_success());

        // Test BatchableCapable
        let configs = handler.create_cursor_configs(100, 4);
        assert_eq!(configs.len(), 4);
    }

    #[test]
    fn test_error_classification_reexport() {
        assert!(!ErrorClassification::Success.is_retryable());
        assert!(ErrorClassification::RateLimited.is_retryable());
        assert!(ErrorClassification::TransientError.is_retryable());
    }

    #[test]
    fn test_handler_capabilities_reexport() {
        let caps = HandlerCapabilities::new().with_api().with_decision();
        assert!(caps.api_capable);
        assert!(caps.decision_capable);
    }
}

//! # Event Types and Constants
//!
//! Unified event type system that supports both simple generic events
//! and structured orchestration events with Rails compatibility.
//!
//! ## Architecture
//!
//! - **Generic Events**: Simple name/context pairs for basic event publishing
//! - **Structured Events**: Typed events with specific payloads for orchestration
//! - **Rails Compatibility**: Event constants and metadata matching Rails engine
//! - **FFI Bridge Support**: Serializable events for cross-language publishing

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Unified event type that supports both generic and structured events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// Generic event with name and JSON context
    Generic(GenericEvent),
    /// Structured orchestration event with typed payload
    Orchestration(OrchestrationEvent),
}

/// Generic event for simple publishing scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericEvent {
    /// Event name (e.g., "user.created", "payment.processed")
    pub name: String,
    /// Event context as JSON
    pub context: Value,
    /// When the event was published
    pub published_at: DateTime<Utc>,
}

/// Structured orchestration events for workflow management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestrationEvent {
    /// Task orchestration started
    TaskOrchestrationStarted {
        task_uuid: Uuid,
        framework: String,
        started_at: DateTime<Utc>,
    },
    /// Viable steps discovered for execution
    ViableStepsDiscovered {
        task_uuid: Uuid,
        step_count: usize,
        steps: Vec<ViableStep>,
    },
    /// Task orchestration completed
    TaskOrchestrationCompleted {
        task_uuid: Uuid,
        result: TaskResult,
        completed_at: DateTime<Utc>,
    },
    /// Step execution started
    StepExecutionStarted {
        step_uuid: Uuid,
        task_uuid: Uuid,
        step_name: String,
        started_at: DateTime<Utc>,
    },
    /// Step execution completed
    StepExecutionCompleted {
        step_uuid: Uuid,
        task_uuid: Uuid,
        result: StepResult,
        completed_at: DateTime<Utc>,
    },
    /// Handler registered with orchestration system
    HandlerRegistered {
        handler_name: String,
        handler_type: String,
        registered_at: DateTime<Utc>,
    },
}

/// Viable step information for orchestration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViableStep {
    pub step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub name: String,
    pub named_step_uuid: Uuid,
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub attempts: u32,
    pub max_attempts: u32,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
}

/// Task execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskResult {
    Success,
    Failed { error: String },
    Cancelled { reason: String },
    Timeout,
}

/// Step execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepResult {
    Success,
    Failed {
        error: String,
    },
    Skipped {
        reason: String,
    },
    Retry {
        attempt: u32,
        next_retry_at: DateTime<Utc>,
    },
}

/// Event metadata for Rails compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Human-readable description
    pub description: String,
    /// Reference to constant
    pub constant_ref: String,
    /// Payload schema definition
    pub payload_schema: HashMap<String, SchemaField>,
    /// Components that can fire this event
    pub fired_by: Vec<String>,
}

/// Schema field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
}

/// Event constants matching Rails implementation
pub mod constants {
    /// Task lifecycle events
    pub mod task {
        pub const INITIALIZE_REQUESTED: &str = "task.initialize_requested";
        pub const START_REQUESTED: &str = "task.start_requested";
        pub const COMPLETED: &str = "task.completed";
        pub const FAILED: &str = "task.failed";
        pub const RETRY_REQUESTED: &str = "task.retry_requested";
        pub const CANCELLED: &str = "task.cancelled";
        pub const RESOLVED_MANUALLY: &str = "task.resolved_manually";
    }

    /// Step lifecycle events
    pub mod step {
        pub const EXECUTION_REQUESTED: &str = "step.execution_requested";
        pub const BEFORE_HANDLE: &str = "step.before_handle";
        pub const COMPLETED: &str = "step.completed";
        pub const FAILED: &str = "step.failed";
        pub const CANCELLED: &str = "step.cancelled";
        pub const RESOLVED_MANUALLY: &str = "step.resolved_manually";
    }

    /// Workflow orchestration events
    pub mod workflow {
        pub const VIABLE_STEPS_DISCOVERED: &str = "workflow.viable_steps_discovered";
    }

    /// Registry system events
    pub mod registry {
        pub const HANDLER_REGISTERED: &str = "handler.registered";
        pub const HANDLER_UNREGISTERED: &str = "handler.unregistered";
        pub const HANDLER_VALIDATION_FAILED: &str = "handler.validation_failed";
    }
}

impl Event {
    /// Create a generic event
    pub fn generic(name: impl Into<String>, context: Value) -> Self {
        Self::Generic(GenericEvent {
            name: name.into(),
            context,
            published_at: Utc::now(),
        })
    }

    /// Create an orchestration event
    pub fn orchestration(event: OrchestrationEvent) -> Self {
        Self::Orchestration(event)
    }

    /// Get the event name for categorization
    pub fn name(&self) -> String {
        match self {
            Event::Generic(event) => event.name.clone(),
            Event::Orchestration(event) => event.event_name(),
        }
    }

    /// Get the event timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Event::Generic(event) => event.published_at,
            Event::Orchestration(event) => event.timestamp(),
        }
    }

    /// Convert to JSON for serialization
    pub fn to_json(&self) -> serde_json::Result<Value> {
        serde_json::to_value(self)
    }
}

impl OrchestrationEvent {
    /// Get the event name as a string
    pub fn event_name(&self) -> String {
        match self {
            OrchestrationEvent::TaskOrchestrationStarted { .. } => {
                "task_orchestration_started".to_string()
            }
            OrchestrationEvent::ViableStepsDiscovered { .. } => {
                "viable_steps_discovered".to_string()
            }
            OrchestrationEvent::TaskOrchestrationCompleted { .. } => {
                "task_orchestration_completed".to_string()
            }
            OrchestrationEvent::StepExecutionStarted { .. } => "step_execution_started".to_string(),
            OrchestrationEvent::StepExecutionCompleted { .. } => {
                "step_execution_completed".to_string()
            }
            OrchestrationEvent::HandlerRegistered { .. } => "handler_registered".to_string(),
        }
    }

    /// Get the event timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            OrchestrationEvent::TaskOrchestrationStarted { started_at, .. } => *started_at,
            OrchestrationEvent::ViableStepsDiscovered { .. } => Utc::now(),
            OrchestrationEvent::TaskOrchestrationCompleted { completed_at, .. } => *completed_at,
            OrchestrationEvent::StepExecutionStarted { started_at, .. } => *started_at,
            OrchestrationEvent::StepExecutionCompleted { completed_at, .. } => *completed_at,
            OrchestrationEvent::HandlerRegistered { registered_at, .. } => *registered_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generic_event_creation() {
        let context = serde_json::json!({"user_id": 123});
        let event = Event::generic("user.created", context.clone());

        match event {
            Event::Generic(generic_event) => {
                assert_eq!(generic_event.name, "user.created");
                assert_eq!(generic_event.context, context);
            }
            _ => panic!("Expected generic event"),
        }
    }

    #[test]
    fn test_orchestration_event_creation() {
        let orch_event = OrchestrationEvent::TaskOrchestrationStarted {
            task_uuid: Uuid::now_v7(),
            framework: "rust".to_string(),
            started_at: Utc::now(),
        };

        let event = Event::orchestration(orch_event);
        assert_eq!(event.name(), "task_orchestration_started");
    }

    #[test]
    fn test_event_constants() {
        assert_eq!(constants::task::COMPLETED, "task.completed");
        assert_eq!(constants::step::BEFORE_HANDLE, "step.before_handle");
        assert_eq!(
            constants::workflow::VIABLE_STEPS_DISCOVERED,
            "workflow.viable_steps_discovered"
        );
        assert_eq!(
            constants::registry::HANDLER_REGISTERED,
            "handler.registered"
        );
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::generic("test.event", serde_json::json!({"data": "value"}));
        let serialized = event.to_json().unwrap();

        assert!(serialized.is_object());
        assert!(serialized.get("Generic").is_some());
    }
}

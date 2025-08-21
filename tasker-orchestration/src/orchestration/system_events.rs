//! # System Events Configuration
//!
//! ## Architecture: Rails Engine Compatibility Layer
//!
//! This module provides a comprehensive system events configuration that mirrors
//! the Rails Tasker engine's system_events.yml file, ensuring compatibility and
//! consistency across language boundaries.
//!
//! ## Key Components:
//!
//! - **Event Metadata**: Rich descriptions, payload schemas, and source tracking
//! - **State Machine Mappings**: Declarative state transition configuration
//! - **Event Constants**: Rust equivalents of Rails event constants
//! - **Event Publishing**: Integration with EventPublisher for cross-language events
//!
//! ## Usage:
//!
//! ```rust
//! use tasker_shared::orchestration::system_events::{SystemEventsConfig, EventMetadata};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let events_config = SystemEventsConfig::load_from_file("config/system_events.yaml").await?;
//!
//! // Get metadata for a specific event
//! let step_completed_metadata = events_config.get_event_metadata("step", "completed")?;
//! assert_eq!(step_completed_metadata.description, "Fired when a step completes successfully");
//!
//! // Check state transitions
//! let transitions = events_config.get_step_transitions();
//! assert!(!transitions.is_empty());
//!
//! // Verify we can find specific transitions
//! let pending_to_progress = transitions.iter()
//!     .find(|t| t.from_state == Some("pending".to_string()) && t.to_state == "in_progress");
//! assert!(pending_to_progress.is_some());
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tasker_shared::errors::{OrchestrationError, OrchestrationResult};
use tracing::{debug, info, instrument};
use uuid::Uuid;

/// Main system events configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEventsConfig {
    /// Event metadata organized by category and event name
    pub event_metadata: HashMap<String, HashMap<String, EventMetadata>>,

    /// State machine transition mappings
    pub state_machine_mappings: StateMachineMappings,
}

/// Metadata for a specific event type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Human-readable description of the event
    pub description: String,

    /// Reference to the constant that represents this event
    pub constant_ref: String,

    /// Schema definition for the event payload
    pub payload_schema: HashMap<String, SchemaField>,

    /// Components that can fire this event
    pub fired_by: Vec<String>,
}

/// Schema field definition for event payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    /// Data type (String, Integer, Float, Array, etc.)
    #[serde(rename = "type")]
    pub field_type: String,

    /// Whether this field is required
    pub required: bool,
}

/// State machine mappings for tasks and steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineMappings {
    /// Task state transitions
    pub task_transitions: Vec<StateTransition>,

    /// Step state transitions
    pub step_transitions: Vec<StateTransition>,
}

/// Definition of a state machine transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Starting state (null for initial transitions)
    pub from_state: Option<String>,

    /// Target state
    pub to_state: String,

    /// Event constant that triggers this transition
    pub event_constant: String,

    /// Description of what this transition represents
    pub description: String,
}

/// Event constants matching Rails implementation
pub mod constants {
    /// Task lifecycle event constants
    pub mod task_events {
        pub const INITIALIZE_REQUESTED: &str = "task.initialize_requested";
        pub const START_REQUESTED: &str = "task.start_requested";
        pub const COMPLETED: &str = "task.completed";
        pub const FAILED: &str = "task.failed";
        pub const RETRY_REQUESTED: &str = "task.retry_requested";
        pub const CANCELLED: &str = "task.cancelled";
        pub const RESOLVED_MANUALLY: &str = "task.resolved_manually";
    }

    /// Step lifecycle event constants
    pub mod step_events {
        pub const EXECUTION_REQUESTED: &str = "step.execution_requested";
        pub const BEFORE_HANDLE: &str = "step.before_handle";
        pub const COMPLETED: &str = "step.completed";
        pub const FAILED: &str = "step.failed";
        pub const CANCELLED: &str = "step.cancelled";
        pub const RESOLVED_MANUALLY: &str = "step.resolved_manually";
    }

    /// Workflow orchestration event constants
    pub mod workflow_events {
        pub const VIABLE_STEPS_DISCOVERED: &str = "workflow.viable_steps_discovered";
    }

    /// Registry system event constants
    pub mod registry_events {
        pub const HANDLER_REGISTERED: &str = "handler.registered";
        pub const HANDLER_UNREGISTERED: &str = "handler.unregistered";
        pub const HANDLER_VALIDATION_FAILED: &str = "handler.validation_failed";
    }
}

/// System events configuration manager
pub struct SystemEventsManager {
    config: Arc<SystemEventsConfig>,
}

impl SystemEventsConfig {
    /// Load system events configuration from YAML file
    #[instrument]
    pub async fn load_from_file<P: AsRef<Path> + std::fmt::Debug>(
        path: P,
    ) -> OrchestrationResult<Self> {
        let path = path.as_ref();
        info!("Loading system events configuration from: {:?}", path);

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            OrchestrationError::ConfigurationError {
                config_source: format!("{path:?}"),
                reason: format!("Failed to read system events file: {e}"),
            }
        })?;

        let config: SystemEventsConfig =
            serde_yaml::from_str(&content).map_err(|e| OrchestrationError::ConfigurationError {
                config_source: format!("{path:?}"),
                reason: format!("Failed to parse system events YAML: {e}"),
            })?;

        debug!("System events configuration loaded successfully");
        Ok(config)
    }

    /// Get event metadata for a specific category and event
    pub fn get_event_metadata(
        &self,
        category: &str,
        event_name: &str,
    ) -> OrchestrationResult<&EventMetadata> {
        self.event_metadata
            .get(category)
            .and_then(|events| events.get(event_name))
            .ok_or_else(|| OrchestrationError::ConfigurationError {
                config_source: "system_events".to_string(),
                reason: format!("Event metadata not found: {category}.{event_name}"),
            })
    }

    /// Get all task state transitions
    pub fn get_task_transitions(&self) -> &Vec<StateTransition> {
        &self.state_machine_mappings.task_transitions
    }

    /// Get all step state transitions
    pub fn get_step_transitions(&self) -> &Vec<StateTransition> {
        &self.state_machine_mappings.step_transitions
    }

    /// Find valid transitions from a given state
    pub fn get_transitions_from_state(
        &self,
        entity_type: &str,
        from_state: Option<&str>,
    ) -> Vec<&StateTransition> {
        let transitions = match entity_type {
            "task" => &self.state_machine_mappings.task_transitions,
            "step" => &self.state_machine_mappings.step_transitions,
            _ => return vec![],
        };

        transitions
            .iter()
            .filter(|t| t.from_state.as_deref() == from_state)
            .collect()
    }

    /// Validate that an event payload matches the expected schema
    pub fn validate_event_payload(
        &self,
        category: &str,
        event_name: &str,
        payload: &serde_json::Value,
    ) -> OrchestrationResult<()> {
        let metadata = self.get_event_metadata(category, event_name)?;

        // Check that all required fields are present
        for (field_name, schema_field) in &metadata.payload_schema {
            if schema_field.required && payload.get(field_name).is_none() {
                return Err(OrchestrationError::ValidationError {
                    field: field_name.clone(),
                    reason: format!(
                        "Required field '{field_name}' missing from {event_name} event payload"
                    ),
                });
            }
        }

        // TODO: Add type validation for field values
        Ok(())
    }
}

impl SystemEventsManager {
    /// Create a new system events manager
    pub fn new(config: SystemEventsConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Load system events manager from file
    pub async fn load_from_file<P: AsRef<Path> + std::fmt::Debug>(
        path: P,
    ) -> OrchestrationResult<Self> {
        let config = SystemEventsConfig::load_from_file(path).await?;
        Ok(Self::new(config))
    }

    /// Get the underlying configuration
    pub fn config(&self) -> Arc<SystemEventsConfig> {
        Arc::clone(&self.config)
    }

    /// Create a properly formatted event payload for step completion
    pub fn create_step_completed_payload(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: &str,
        execution_duration: f64,
        attempt_number: u32,
    ) -> serde_json::Value {
        serde_json::json!({
            "task_uuid": task_uuid.to_string(),
            "step_uuid": step_uuid.to_string(),
            "step_name": step_name,
            "execution_duration": execution_duration,
            "attempt_number": attempt_number,
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
    }

    /// Create a properly formatted event payload for step failure
    pub fn create_step_failed_payload(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: &str,
        error_message: &str,
        error_class: &str,
        attempt_number: u32,
    ) -> serde_json::Value {
        serde_json::json!({
            "task_uuid": task_uuid.to_string(),
            "step_uuid": step_uuid.to_string(),
            "step_name": step_name,
            "error_message": error_message,
            "error_class": error_class,
            "attempt_number": attempt_number,
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
    }

    /// Create a properly formatted event payload for task completion
    pub fn create_task_completed_payload(
        &self,
        task_uuid: Uuid,
        task_name: &str,
        total_steps: u32,
        completed_steps: u32,
        total_duration: Option<f64>,
    ) -> serde_json::Value {
        let mut payload = serde_json::json!({
            "task_uuid": task_uuid.to_string(),
            "task_name": task_name,
            "total_steps": total_steps,
            "completed_steps": completed_steps,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        if let Some(duration) = total_duration {
            payload["total_duration"] = serde_json::Value::Number(
                serde_json::Number::from_f64(duration)
                    .unwrap_or_else(|| serde_json::Number::from(0)),
            );
        }

        payload
    }

    /// Create a properly formatted event payload for viable steps discovery
    pub fn create_viable_steps_discovered_payload(
        &self,
        task_uuid: Uuid,
        step_uuids: &[Uuid],
        processing_mode: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "task_uuid": task_uuid.to_string(),
            "step_uuids": step_uuids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            "step_count": step_uuids.len(),
            "processing_mode": processing_mode,
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_constants() {
        assert_eq!(constants::task_events::COMPLETED, "task.completed");
        assert_eq!(constants::step_events::BEFORE_HANDLE, "step.before_handle");
        assert_eq!(
            constants::workflow_events::VIABLE_STEPS_DISCOVERED,
            "workflow.viable_steps_discovered"
        );
        assert_eq!(
            constants::registry_events::HANDLER_REGISTERED,
            "handler.registered"
        );
    }

    #[test]
    fn test_system_events_manager_payload_creation() {
        let config = SystemEventsConfig {
            event_metadata: HashMap::new(),
            state_machine_mappings: StateMachineMappings {
                task_transitions: vec![],
                step_transitions: vec![],
            },
        };

        let manager = SystemEventsManager::new(config);

        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();

        // Test step completed payload
        let payload =
            manager.create_step_completed_payload(task_uuid, step_uuid, "test_step", 1.5, 1);
        assert_eq!(payload["task_uuid"], task_uuid.to_string());
        assert_eq!(payload["step_uuid"], step_uuid.to_string());
        assert_eq!(payload["step_name"], "test_step");
        assert_eq!(payload["execution_duration"], 1.5);
        assert_eq!(payload["attempt_number"], 1);
        assert!(payload["timestamp"].is_string());

        // Test task completed payload
        let payload =
            manager.create_task_completed_payload(task_uuid, "test_task", 5, 5, Some(10.0));
        assert_eq!(payload["task_uuid"], task_uuid.to_string());
        assert_eq!(payload["task_name"], "test_task");
        assert_eq!(payload["total_steps"], 5);
        assert_eq!(payload["completed_steps"], 5);
        assert_eq!(payload["total_duration"], 10.0);

        // Test viable steps payload
        let step_uuids = vec![step_uuid];
        let payload =
            manager.create_viable_steps_discovered_payload(task_uuid, &step_uuids, "parallel");
        assert_eq!(payload["task_uuid"], task_uuid.to_string());
        assert_eq!(payload["step_count"], 1);
        assert_eq!(payload["processing_mode"], "parallel");
        assert!(payload["step_uuids"].is_array());
    }

    #[tokio::test]
    async fn test_system_events_config_from_yaml() {
        let yaml_content = r#"
event_metadata:
  task:
    completed:
      description: "Task completed successfully"
      constant_ref: "TaskEvents::COMPLETED"
      payload_schema:
        task_uuid: { type: "String", required: true }
        task_name: { type: "String", required: true }
      fired_by: ["TaskHandler"]

state_machine_mappings:
  task_transitions:
    - from_state: "in_progress"
      to_state: "complete"
      event_constant: "task.completed"
      description: "Task completed successfully"
  step_transitions: []
"#;

        let config: SystemEventsConfig = serde_yaml::from_str(yaml_content).unwrap();

        // Test event metadata access
        let task_completed = config.get_event_metadata("task", "completed").unwrap();
        assert_eq!(task_completed.description, "Task completed successfully");
        assert_eq!(task_completed.constant_ref, "TaskEvents::COMPLETED");
        assert_eq!(task_completed.fired_by[0], "TaskHandler");

        // Test payload schema
        assert!(task_completed.payload_schema.contains_key("task_uuid"));
        assert!(task_completed.payload_schema["task_uuid"].required);

        // Test state transitions
        let transitions = config.get_task_transitions();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].from_state, Some("in_progress".to_string()));
        assert_eq!(transitions[0].to_state, "complete");
        assert_eq!(transitions[0].event_constant, "task.completed");
    }
}

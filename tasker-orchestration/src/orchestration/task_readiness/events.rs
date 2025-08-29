//! Task readiness event types and config-driven classification for TAS-43
//!
//! This module implements config-driven event classification similar to ConfigDrivenMessageEvent
//! from event_driven_coordinator.rs, providing type-safe handling of PostgreSQL LISTEN/NOTIFY
//! events for task readiness coordination.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Config-driven task readiness event classification (similar to ConfigDrivenMessageEvent)
/// 
/// Uses exhaustive enum matching with config-driven classification to avoid hardcoded
/// string matching patterns, following the same approach as ConfigDrivenMessageEvent::classify
#[derive(Debug, Clone)]
pub enum TaskReadinessEvent {
    /// Task became ready for processing
    TaskReady(TaskReadyEvent),
    /// Task state changed (completion, error, etc.)
    TaskStateChange(TaskStateChangeEvent),
    /// New namespace created
    NamespaceCreated(NamespaceCreatedEvent),
    /// Unknown notification type (for monitoring and debugging)
    Unknown { channel: String, payload: String },
}

/// Task ready event from database triggers
/// 
/// Represents a task that has become ready for step processing due to step transitions
/// or task state changes. This is the primary event type for event-driven coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadyEvent {
    /// Task UUID that became ready
    pub task_uuid: Uuid,
    /// Namespace of the task
    pub namespace: String,
    /// Task priority for processing order
    pub priority: i32,
    /// Number of ready steps
    pub ready_steps: i32,
    /// What triggered this readiness event
    pub triggered_by: ReadinessTrigger,
    /// Optional step UUID if triggered by step transition
    pub step_uuid: Option<Uuid>,
    /// Optional step state if triggered by step transition
    pub step_state: Option<String>,
    /// Optional task state if triggered by task transition
    pub task_state: Option<String>,
}

/// Source of task readiness event for monitoring and debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessTrigger {
    /// Triggered by step transition (most common case)
    StepTransition,
    /// Triggered by task starting
    TaskStart,
    /// Triggered by fallback polling (reliability mechanism)
    FallbackPolling,
    /// Manual trigger for testing and debugging
    Manual,
}

/// Task state change event for completion, error, and cleanup coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStateChangeEvent {
    /// Task UUID that changed state
    pub task_uuid: Uuid,
    /// Namespace of the task
    pub namespace: String,
    /// New task state
    pub task_state: String,
    /// What triggered this state change
    pub triggered_by: String,
    /// Action needed (finalization, error_handling, cleanup)
    pub action_needed: String,
}

/// Namespace creation event for dynamic coordinator spawning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceCreatedEvent {
    /// Namespace UUID
    pub namespace_uuid: Uuid,
    /// Namespace name for coordinator registration
    pub namespace_name: String,
    /// Optional description
    pub description: Option<String>,
    /// When the namespace was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Trigger source (always "namespace_creation")
    pub triggered_by: String,
}

impl TaskReadyEvent {
    /// Parse from pg_notify payload JSON
    pub fn from_pg_notify_payload(payload: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(payload)
    }

    /// Convert to pg_notify payload JSON
    pub fn to_pg_notify_payload(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get notification channel name for namespace-specific notifications
    pub fn namespace_channel(&self) -> String {
        format!("task_ready.{}", self.namespace)
    }

    /// Global notification channel name
    pub fn global_channel() -> &'static str {
        "task_ready"
    }
}

impl TaskStateChangeEvent {
    /// Parse from pg_notify payload JSON
    pub fn from_pg_notify_payload(payload: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(payload)
    }

    /// Get notification channel name for namespace-specific state changes
    pub fn namespace_channel(&self) -> String {
        format!("task_state_change.{}", self.namespace)
    }

    /// Global state change channel name
    pub fn global_channel() -> &'static str {
        "task_state_change"
    }
}

impl NamespaceCreatedEvent {
    /// Parse from pg_notify payload JSON
    pub fn from_pg_notify_payload(payload: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(payload)
    }

    /// Notification channel name for namespace creation
    pub fn channel() -> &'static str {
        "namespace_created"
    }
}

/// Config-driven classifier for task readiness events
/// 
/// Similar to tasker_shared::config::QueueClassifier but for pg_notify channels.
/// Uses configuration-driven classification to avoid hardcoded string matching,
/// following the same patterns as ConfigDrivenMessageEvent::classify.
#[derive(Clone)]
pub struct ReadinessEventClassifier {
    /// Configuration for readiness notification channels
    config: TaskReadinessNotificationConfig,
}

/// Configuration for task readiness notification channels
/// 
/// Provides configurable channel names and patterns for PostgreSQL LISTEN/NOTIFY
/// events, enabling environment-specific customization while maintaining type safety.
#[derive(Debug, Clone)]
pub struct TaskReadinessNotificationConfig {
    /// Global task ready channel
    pub task_ready_channel: String,
    /// Global task state change channel  
    pub task_state_change_channel: String,
    /// Namespace creation channel
    pub namespace_created_channel: String,
    /// Pattern for namespace-specific task ready channels
    pub namespace_channel_pattern: String,
    /// Pattern for namespace-specific state change channels
    pub state_change_pattern: String,
}

impl Default for TaskReadinessNotificationConfig {
    fn default() -> Self {
        Self {
            task_ready_channel: "task_ready".to_string(),
            task_state_change_channel: "task_state_change".to_string(),
            namespace_created_channel: "namespace_created".to_string(),
            namespace_channel_pattern: "task_ready.{namespace}".to_string(),
            state_change_pattern: "task_state_change.{namespace}".to_string(),
        }
    }
}

impl ReadinessEventClassifier {
    /// Create new classifier with configuration
    pub fn new(config: TaskReadinessNotificationConfig) -> Self {
        Self { config }
    }

    /// Create classifier with default configuration
    pub fn default() -> Self {
        Self::new(TaskReadinessNotificationConfig::default())
    }

    /// Classify PostgreSQL notification into typed event
    /// 
    /// Similar to ConfigDrivenMessageEvent::classify pattern from event_driven_coordinator.rs,
    /// this method uses exhaustive enum matching based on configuration rather than
    /// hardcoded string matching. This provides type safety and configuration flexibility.
    pub fn classify(&self, channel: &str, payload: &str) -> TaskReadinessEvent {
        match channel {
            // Global task ready channel
            ch if ch == self.config.task_ready_channel => {
                match TaskReadyEvent::from_pg_notify_payload(payload) {
                    Ok(event) => TaskReadinessEvent::TaskReady(event),
                    Err(_) => TaskReadinessEvent::Unknown {
                        channel: channel.to_string(),
                        payload: payload.to_string(),
                    },
                }
            },
            
            // Global task state change channel
            ch if ch == self.config.task_state_change_channel => {
                match TaskStateChangeEvent::from_pg_notify_payload(payload) {
                    Ok(event) => TaskReadinessEvent::TaskStateChange(event),
                    Err(_) => TaskReadinessEvent::Unknown {
                        channel: channel.to_string(),
                        payload: payload.to_string(),
                    },
                }
            },
            
            // Namespace creation channel
            ch if ch == self.config.namespace_created_channel => {
                match NamespaceCreatedEvent::from_pg_notify_payload(payload) {
                    Ok(event) => TaskReadinessEvent::NamespaceCreated(event),
                    Err(_) => TaskReadinessEvent::Unknown {
                        channel: channel.to_string(),
                        payload: payload.to_string(),
                    },
                }
            },
            
            // Namespace-specific task ready channels (task_ready.namespace_name)
            ch if self.is_namespace_task_ready_channel(ch) => {
                match TaskReadyEvent::from_pg_notify_payload(payload) {
                    Ok(event) => TaskReadinessEvent::TaskReady(event),
                    Err(_) => TaskReadinessEvent::Unknown {
                        channel: channel.to_string(),
                        payload: payload.to_string(),
                    },
                }
            },
            
            // Namespace-specific state change channels (task_state_change.namespace_name)
            ch if self.is_namespace_state_change_channel(ch) => {
                match TaskStateChangeEvent::from_pg_notify_payload(payload) {
                    Ok(event) => TaskReadinessEvent::TaskStateChange(event),
                    Err(_) => TaskReadinessEvent::Unknown {
                        channel: channel.to_string(),
                        payload: payload.to_string(),
                    },
                }
            },
            
            // Unknown channel - log for monitoring but don't crash
            _ => TaskReadinessEvent::Unknown {
                channel: channel.to_string(),
                payload: payload.to_string(),
            },
        }
    }

    /// Check if channel matches namespace-specific task ready pattern
    /// 
    /// Pattern matching for channels like "task_ready.fulfillment", "task_ready.inventory", etc.
    fn is_namespace_task_ready_channel(&self, channel: &str) -> bool {
        channel.starts_with("task_ready.") && channel.len() > "task_ready.".len()
    }

    /// Check if channel matches namespace-specific state change pattern  
    /// 
    /// Pattern matching for channels like "task_state_change.fulfillment", etc.
    fn is_namespace_state_change_channel(&self, channel: &str) -> bool {
        channel.starts_with("task_state_change.") && channel.len() > "task_state_change.".len()
    }

    /// Extract namespace from namespace-specific channel
    /// 
    /// Returns the namespace portion from channels like "task_ready.namespace_name"
    pub fn extract_namespace_from_channel(&self, channel: &str) -> Option<String> {
        if self.is_namespace_task_ready_channel(channel) {
            Some(channel["task_ready.".len()..].to_string())
        } else if self.is_namespace_state_change_channel(channel) {
            Some(channel["task_state_change.".len()..].to_string())
        } else {
            None
        }
    }

    /// Get all channel names to listen to for a namespace
    /// 
    /// Returns the channels that should be listened to for comprehensive
    /// event coverage for a specific namespace
    pub fn get_namespace_channels(&self, namespace: &str) -> Vec<String> {
        vec![
            format!("task_ready.{}", namespace),
            format!("task_state_change.{}", namespace),
        ]
    }

    /// Get all global channel names to listen to
    /// 
    /// Returns the global channels that provide events across all namespaces
    pub fn get_global_channels(&self) -> Vec<String> {
        vec![
            self.config.task_ready_channel.clone(),
            self.config.task_state_change_channel.clone(),
            self.config.namespace_created_channel.clone(),
        ]
    }

    /// Get configuration reference
    pub fn config(&self) -> &TaskReadinessNotificationConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_task_ready_event_serialization() {
        let event = TaskReadyEvent {
            task_uuid: Uuid::new_v4(),
            namespace: "test_namespace".to_string(),
            priority: 2,
            ready_steps: 3,
            triggered_by: ReadinessTrigger::StepTransition,
            step_uuid: Some(Uuid::new_v4()),
            step_state: Some("complete".to_string()),
            task_state: None,
        };

        // Test serialization
        let json = event.to_pg_notify_payload().expect("Should serialize");
        assert!(json.contains("test_namespace"));
        assert!(json.contains("step_transition"));

        // Test deserialization
        let parsed = TaskReadyEvent::from_pg_notify_payload(&json).expect("Should deserialize");
        assert_eq!(parsed.namespace, event.namespace);
        assert_eq!(parsed.priority, event.priority);
        assert_eq!(parsed.ready_steps, event.ready_steps);
    }

    #[test]
    fn test_readiness_event_classifier_config_driven() {
        let config = TaskReadinessNotificationConfig::default();
        let classifier = ReadinessEventClassifier::new(config);

        // Test global task ready channel classification
        let task_ready_payload = r#"{"task_uuid":"123e4567-e89b-12d3-a456-426614174000","namespace":"test","priority":1,"ready_steps":2,"triggered_by":"step_transition"}"#;
        
        let event = classifier.classify("task_ready", task_ready_payload);
        match event {
            TaskReadinessEvent::TaskReady(task_event) => {
                assert_eq!(task_event.namespace, "test");
                assert_eq!(task_event.ready_steps, 2);
            }
            _ => panic!("Expected TaskReady event"),
        }

        // Test namespace-specific channel classification  
        let namespace_event = classifier.classify("task_ready.fulfillment", task_ready_payload);
        match namespace_event {
            TaskReadinessEvent::TaskReady(task_event) => {
                assert_eq!(task_event.namespace, "test");
            }
            _ => panic!("Expected TaskReady event for namespace channel"),
        }

        // Test unknown channel classification
        let unknown_event = classifier.classify("unknown_channel", "test_payload");
        match unknown_event {
            TaskReadinessEvent::Unknown { channel, payload } => {
                assert_eq!(channel, "unknown_channel");
                assert_eq!(payload, "test_payload");
            }
            _ => panic!("Expected Unknown event"),
        }
    }

    #[test]
    fn test_namespace_channel_pattern_matching() {
        let classifier = ReadinessEventClassifier::default();

        // Test namespace task ready channel detection
        assert!(classifier.is_namespace_task_ready_channel("task_ready.fulfillment"));
        assert!(classifier.is_namespace_task_ready_channel("task_ready.inventory"));
        assert!(!classifier.is_namespace_task_ready_channel("task_ready"));
        assert!(!classifier.is_namespace_task_ready_channel("task_ready."));

        // Test namespace state change channel detection
        assert!(classifier.is_namespace_state_change_channel("task_state_change.fulfillment"));
        assert!(!classifier.is_namespace_state_change_channel("task_state_change"));

        // Test namespace extraction
        assert_eq!(
            classifier.extract_namespace_from_channel("task_ready.fulfillment"),
            Some("fulfillment".to_string())
        );
        assert_eq!(
            classifier.extract_namespace_from_channel("task_state_change.inventory"),
            Some("inventory".to_string())
        );
        assert_eq!(
            classifier.extract_namespace_from_channel("task_ready"),
            None
        );
    }

    #[test]
    fn test_channel_enumeration() {
        let classifier = ReadinessEventClassifier::default();

        // Test namespace channels
        let namespace_channels = classifier.get_namespace_channels("fulfillment");
        assert_eq!(namespace_channels.len(), 2);
        assert!(namespace_channels.contains(&"task_ready.fulfillment".to_string()));
        assert!(namespace_channels.contains(&"task_state_change.fulfillment".to_string()));

        // Test global channels
        let global_channels = classifier.get_global_channels();
        assert_eq!(global_channels.len(), 3);
        assert!(global_channels.contains(&"task_ready".to_string()));
        assert!(global_channels.contains(&"task_state_change".to_string()));
        assert!(global_channels.contains(&"namespace_created".to_string()));
    }

    #[test]
    fn test_task_state_change_event() {
        let event = TaskStateChangeEvent {
            task_uuid: Uuid::new_v4(),
            namespace: "test_ns".to_string(),
            task_state: "complete".to_string(),
            triggered_by: "task_transition".to_string(),
            action_needed: "finalization".to_string(),
        };

        // Test channel methods
        assert_eq!(event.namespace_channel(), "task_state_change.test_ns");
        assert_eq!(TaskStateChangeEvent::global_channel(), "task_state_change");

        // Test serialization round-trip
        let json = serde_json::to_string(&event).expect("Should serialize");
        let parsed: TaskStateChangeEvent = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(parsed.namespace, event.namespace);
        assert_eq!(parsed.task_state, event.task_state);
        assert_eq!(parsed.action_needed, event.action_needed);
    }

    #[test]
    fn test_namespace_created_event() {
        let event = NamespaceCreatedEvent {
            namespace_uuid: Uuid::new_v4(),
            namespace_name: "new_namespace".to_string(),
            description: Some("Test namespace".to_string()),
            created_at: chrono::Utc::now(),
            triggered_by: "namespace_creation".to_string(),
        };

        assert_eq!(NamespaceCreatedEvent::channel(), "namespace_created");

        // Test serialization
        let json = serde_json::to_string(&event).expect("Should serialize");
        let parsed: NamespaceCreatedEvent = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(parsed.namespace_name, event.namespace_name);
        assert_eq!(parsed.triggered_by, event.triggered_by);
    }

    #[test]
    fn test_readiness_trigger_serialization() {
        let triggers = vec![
            ReadinessTrigger::StepTransition,
            ReadinessTrigger::TaskStart,
            ReadinessTrigger::FallbackPolling,
            ReadinessTrigger::Manual,
        ];

        for trigger in triggers {
            let json = serde_json::to_string(&trigger).expect("Should serialize");
            let parsed: ReadinessTrigger = serde_json::from_str(&json).expect("Should deserialize");
            
            // Match on both to ensure they're the same variant
            match (trigger, parsed) {
                (ReadinessTrigger::StepTransition, ReadinessTrigger::StepTransition) => {},
                (ReadinessTrigger::TaskStart, ReadinessTrigger::TaskStart) => {},
                (ReadinessTrigger::FallbackPolling, ReadinessTrigger::FallbackPolling) => {},
                (ReadinessTrigger::Manual, ReadinessTrigger::Manual) => {},
                _ => panic!("Serialization/deserialization mismatch"),
            }
        }
    }
}
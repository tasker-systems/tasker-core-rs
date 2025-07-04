use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Events that can trigger task state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum TaskEvent {
    /// Start processing the task
    Start,
    /// Mark task as complete
    Complete,
    /// Mark task as failed with error message
    Fail(String),
    /// Cancel the task
    Cancel,
    /// Manually resolve the task
    ResolveManually,
    /// Reset task to initial state for retry
    Reset,
}

impl TaskEvent {
    /// Get a string representation of the event type for logging
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Complete => "complete",
            Self::Fail(_) => "fail",
            Self::Cancel => "cancel",
            Self::ResolveManually => "resolve_manually",
            Self::Reset => "reset",
        }
    }

    /// Extract error message if this is a failure event
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Fail(msg) => Some(msg),
            _ => None,
        }
    }

    /// Check if this event represents a terminal transition
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::Cancel | Self::ResolveManually)
    }
}

/// Events that can trigger workflow step state transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StepEvent {
    /// Start processing the step
    Start,
    /// Mark step as complete with optional results
    Complete(Option<Value>),
    /// Mark step as failed with error message
    Fail(String),
    /// Cancel the step
    Cancel,
    /// Manually resolve the step
    ResolveManually,
    /// Retry the step (from error state)
    Retry,
}

impl StepEvent {
    /// Get a string representation of the event type for logging
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Complete(_) => "complete",
            Self::Fail(_) => "fail",
            Self::Cancel => "cancel",
            Self::ResolveManually => "resolve_manually",
            Self::Retry => "retry",
        }
    }

    /// Extract error message if this is a failure event
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Fail(msg) => Some(msg),
            _ => None,
        }
    }

    /// Extract results if this is a completion event
    pub fn results(&self) -> Option<&Value> {
        match self {
            Self::Complete(results) => results.as_ref(),
            _ => None,
        }
    }

    /// Check if this event represents a terminal transition
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Complete(_) | Self::Cancel | Self::ResolveManually
        )
    }

    /// Check if this event can be applied from error state
    pub fn allows_retry(&self) -> bool {
        matches!(self, Self::Retry | Self::Cancel | Self::ResolveManually)
    }
}

/// Helper for creating common events
impl TaskEvent {
    /// Create a failure event with the given error message
    pub fn fail_with_error(error: impl Into<String>) -> Self {
        Self::Fail(error.into())
    }
}

impl StepEvent {
    /// Create a failure event with the given error message
    pub fn fail_with_error(error: impl Into<String>) -> Self {
        Self::Fail(error.into())
    }

    /// Create a completion event with results
    pub fn complete_with_results(results: Value) -> Self {
        Self::Complete(Some(results))
    }

    /// Create a simple completion event without results
    pub fn complete_simple() -> Self {
        Self::Complete(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_task_event_properties() {
        let start = TaskEvent::Start;
        assert_eq!(start.event_type(), "start");
        assert!(!start.is_terminal());

        let complete = TaskEvent::Complete;
        assert_eq!(complete.event_type(), "complete");
        assert!(complete.is_terminal());

        let fail = TaskEvent::fail_with_error("Database error");
        assert_eq!(fail.event_type(), "fail");
        assert_eq!(fail.error_message(), Some("Database error"));
        assert!(!fail.is_terminal());
    }

    #[test]
    fn test_step_event_properties() {
        let start = StepEvent::Start;
        assert_eq!(start.event_type(), "start");
        assert!(!start.is_terminal());

        let results = json!({"processed": 42, "status": "ok"});
        let complete = StepEvent::complete_with_results(results.clone());
        assert_eq!(complete.event_type(), "complete");
        assert_eq!(complete.results(), Some(&results));
        assert!(complete.is_terminal());

        let retry = StepEvent::Retry;
        assert!(retry.allows_retry());
        assert!(!retry.is_terminal());
    }

    #[test]
    fn test_event_serde() {
        let task_event = TaskEvent::fail_with_error("Network timeout");
        let json = serde_json::to_string(&task_event).unwrap();
        let parsed: TaskEvent = serde_json::from_str(&json).unwrap();

        match parsed {
            TaskEvent::Fail(msg) => assert_eq!(msg, "Network timeout"),
            _ => panic!("Expected Fail event"),
        }

        let step_event = StepEvent::complete_with_results(json!({"count": 5}));
        let json = serde_json::to_string(&step_event).unwrap();
        let parsed: StepEvent = serde_json::from_str(&json).unwrap();

        match parsed {
            StepEvent::Complete(Some(results)) => {
                assert_eq!(results["count"], 5);
            }
            _ => panic!("Expected Complete event with results"),
        }
    }
}

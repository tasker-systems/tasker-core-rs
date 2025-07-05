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

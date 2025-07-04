use serde::{Deserialize, Serialize};
use std::fmt;

/// Task state definitions matching Rails Statesman implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Initial state when task is created
    Pending,
    /// Task is currently being executed
    InProgress,
    /// Task completed successfully
    Complete,
    /// Task failed with an error
    Error,
    /// Task was cancelled
    Cancelled,
    /// Task was manually resolved by operator
    ResolvedManually,
}

impl TaskState {
    /// Check if this is a terminal state (no further transitions allowed)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Complete | Self::Cancelled | Self::ResolvedManually
        )
    }

    /// Check if this is an error state that may allow recovery
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    /// Check if this is an active state (task is being processed)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::InProgress)
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Complete => write!(f, "complete"),
            Self::Error => write!(f, "error"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::ResolvedManually => write!(f, "resolved_manually"),
        }
    }
}

impl std::str::FromStr for TaskState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "complete" => Ok(Self::Complete),
            "error" => Ok(Self::Error),
            "cancelled" => Ok(Self::Cancelled),
            "resolved_manually" => Ok(Self::ResolvedManually),
            _ => Err(format!("Invalid task state: {s}")),
        }
    }
}

/// Workflow step state definitions matching Rails implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepState {
    /// Initial state when step is created
    Pending,
    /// Step is currently being executed
    InProgress,
    /// Step completed successfully
    Complete,
    /// Step failed with an error
    Error,
    /// Step was cancelled
    Cancelled,
    /// Step was manually resolved by operator
    ResolvedManually,
}

impl WorkflowStepState {
    /// Check if this is a terminal state (no further transitions allowed)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Complete | Self::Cancelled | Self::ResolvedManually
        )
    }

    /// Check if this is an error state that may allow recovery
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    /// Check if this is an active state (step is being processed)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::InProgress)
    }

    /// Check if this step satisfies dependencies for other steps
    pub fn satisfies_dependencies(&self) -> bool {
        matches!(self, Self::Complete | Self::ResolvedManually)
    }
}

impl fmt::Display for WorkflowStepState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Complete => write!(f, "complete"),
            Self::Error => write!(f, "error"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::ResolvedManually => write!(f, "resolved_manually"),
        }
    }
}

impl std::str::FromStr for WorkflowStepState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "complete" => Ok(Self::Complete),
            "error" => Ok(Self::Error),
            "cancelled" => Ok(Self::Cancelled),
            "resolved_manually" => Ok(Self::ResolvedManually),
            _ => Err(format!("Invalid workflow step state: {s}")),
        }
    }
}

/// Default state for new tasks
impl Default for TaskState {
    fn default() -> Self {
        Self::Pending
    }
}

/// Default state for new workflow steps
impl Default for WorkflowStepState {
    fn default() -> Self {
        Self::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_terminal_check() {
        assert!(TaskState::Complete.is_terminal());
        assert!(TaskState::Cancelled.is_terminal());
        assert!(TaskState::ResolvedManually.is_terminal());
        assert!(!TaskState::Pending.is_terminal());
        assert!(!TaskState::InProgress.is_terminal());
        assert!(!TaskState::Error.is_terminal());
    }

    #[test]
    fn test_step_state_dependency_satisfaction() {
        assert!(WorkflowStepState::Complete.satisfies_dependencies());
        assert!(WorkflowStepState::ResolvedManually.satisfies_dependencies());
        assert!(!WorkflowStepState::Pending.satisfies_dependencies());
        assert!(!WorkflowStepState::InProgress.satisfies_dependencies());
        assert!(!WorkflowStepState::Error.satisfies_dependencies());
        assert!(!WorkflowStepState::Cancelled.satisfies_dependencies());
    }

    #[test]
    fn test_state_string_conversion() {
        assert_eq!(TaskState::InProgress.to_string(), "in_progress");
        assert_eq!(
            "complete".parse::<TaskState>().unwrap(),
            TaskState::Complete
        );

        assert_eq!(WorkflowStepState::Error.to_string(), "error");
        assert_eq!(
            "resolved_manually".parse::<WorkflowStepState>().unwrap(),
            WorkflowStepState::ResolvedManually
        );
    }

    #[test]
    fn test_state_serde() {
        let task_state = TaskState::InProgress;
        let json = serde_json::to_string(&task_state).unwrap();
        assert_eq!(json, "\"in_progress\"");

        let parsed: TaskState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, task_state);
    }
}

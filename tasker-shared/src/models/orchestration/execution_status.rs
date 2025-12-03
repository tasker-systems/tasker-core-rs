//! # Task Execution Status and Action Enums
//!
//! Unified enums for task execution context, used across orchestration components.
//! These replace the raw string usage in SQL functions with proper typed enums.
//!
//! This is the canonical location for `ExecutionStatus` and `RecommendedAction`.

use serde::{Deserialize, Serialize};

/// Execution status returned by the SQL get_task_execution_context function
/// Matches the execution_status values from the SQL function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Task has ready steps that can be executed
    HasReadySteps,
    /// Task has steps currently being processed
    Processing,
    /// Task is blocked by failures that cannot be retried
    BlockedByFailures,
    /// All steps are complete
    AllComplete,
    /// Task is waiting for dependencies to complete
    WaitingForDependencies,
}

impl ExecutionStatus {
    /// Convert to SQL string value
    pub fn as_str(&self) -> &str {
        match self {
            Self::HasReadySteps => "has_ready_steps",
            Self::Processing => "processing",
            Self::BlockedByFailures => "blocked_by_failures",
            Self::AllComplete => "all_complete",
            Self::WaitingForDependencies => "waiting_for_dependencies",
        }
    }

    /// Check if this status indicates active work is happening
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Processing | Self::HasReadySteps)
    }

    /// Check if this status indicates a blocked or waiting state
    pub const fn is_blocked(&self) -> bool {
        matches!(self, Self::BlockedByFailures | Self::WaitingForDependencies)
    }

    /// Check if this status indicates completion
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::AllComplete)
    }
}

impl From<&str> for ExecutionStatus {
    fn from(value: &str) -> Self {
        match value {
            "has_ready_steps" => Self::HasReadySteps,
            "processing" => Self::Processing,
            "blocked_by_failures" => Self::BlockedByFailures,
            "all_complete" => Self::AllComplete,
            "waiting_for_dependencies" => Self::WaitingForDependencies,
            _ => Self::WaitingForDependencies, // Default fallback
        }
    }
}

impl From<String> for ExecutionStatus {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl From<ExecutionStatus> for String {
    fn from(value: ExecutionStatus) -> Self {
        value.as_str().to_string()
    }
}

/// Recommended action returned by the SQL get_task_execution_context function
/// Matches the recommended_action values from the SQL function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedAction {
    /// Execute the ready steps
    ExecuteReadySteps,
    /// Wait for in-progress steps to complete
    WaitForCompletion,
    /// Handle permanent failures
    HandleFailures,
    /// Finalize the completed task
    FinalizeTask,
    /// Wait for dependencies to become ready
    WaitForDependencies,
}

impl RecommendedAction {
    /// Convert to SQL string value
    pub fn as_str(&self) -> &str {
        match self {
            Self::ExecuteReadySteps => "execute_ready_steps",
            Self::WaitForCompletion => "wait_for_completion",
            Self::HandleFailures => "handle_failures",
            Self::FinalizeTask => "finalize_task",
            Self::WaitForDependencies => "wait_for_dependencies",
        }
    }
}

impl From<&str> for RecommendedAction {
    fn from(value: &str) -> Self {
        match value {
            "execute_ready_steps" => Self::ExecuteReadySteps,
            "wait_for_completion" => Self::WaitForCompletion,
            "handle_failures" => Self::HandleFailures,
            "finalize_task" => Self::FinalizeTask,
            "wait_for_dependencies" => Self::WaitForDependencies,
            _ => Self::WaitForDependencies, // Default fallback
        }
    }
}

impl From<String> for RecommendedAction {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl From<RecommendedAction> for String {
    fn from(value: RecommendedAction) -> Self {
        value.as_str().to_string()
    }
}

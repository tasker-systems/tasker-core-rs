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
    /// Step has been enqueued for processing but not yet claimed by a worker
    Enqueued,
    /// Step is currently being executed by a worker
    InProgress,
    /// Step completed by worker, enqueued for orchestration processing
    EnqueuedForOrchestration,
    /// Step completed successfully (after orchestration processing)
    Complete,
    /// Step failed with an error (after orchestration processing)
    Error,
    /// Step was cancelled
    Cancelled,
    /// Step was manually resolved by operator
    ResolvedManually,
}

impl WorkflowStepState {
    /// Check if this is a terminal state (no further transitions allowed)
    /// EnqueuedForOrchestration is NOT terminal as orchestration can still process it
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

    /// Check if this is an active state (step is being processed by a worker)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::InProgress)
    }

    /// Check if this step is in the processing pipeline (enqueued or actively processing)
    pub fn is_in_processing_pipeline(&self) -> bool {
        matches!(
            self,
            Self::Enqueued | Self::InProgress | Self::EnqueuedForOrchestration
        )
    }

    /// Check if this step is ready to be claimed by a worker
    pub fn is_ready_for_claiming(&self) -> bool {
        matches!(self, Self::Enqueued)
    }

    /// Check if this step satisfies dependencies for other steps
    /// EnqueuedForOrchestration is NOT included as it's not yet fully processed
    pub fn satisfies_dependencies(&self) -> bool {
        matches!(self, Self::Complete | Self::ResolvedManually)
    }
}

impl fmt::Display for WorkflowStepState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Enqueued => write!(f, "enqueued"),
            Self::InProgress => write!(f, "in_progress"),
            Self::EnqueuedForOrchestration => write!(f, "enqueued_for_orchestration"),
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
            "enqueued" => Ok(Self::Enqueued),
            "in_progress" => Ok(Self::InProgress),
            "enqueued_for_orchestration" => Ok(Self::EnqueuedForOrchestration),
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

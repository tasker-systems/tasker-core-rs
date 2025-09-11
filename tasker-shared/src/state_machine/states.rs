// TAS-41: Richer Task States - Comprehensive task state machine
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// The comprehensive task state enum with 12 states replacing the claim subsystem
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    // Initial states
    /// Created but not started
    Pending,
    /// Discovering initial ready steps
    Initializing,

    // Active processing states
    /// Actively enqueuing ready steps to queues
    EnqueuingSteps,
    /// Steps are being processed by workers
    StepsInProcess,
    /// Processing results from completed steps
    EvaluatingResults,

    // Waiting states
    /// No ready steps, waiting for dependencies
    WaitingForDependencies,
    /// Waiting for retry timeout
    WaitingForRetry,

    // Problem states
    /// Has failures that prevent progress
    BlockedByFailures,

    // Terminal states
    /// All steps completed successfully
    Complete,
    /// Task failed permanently
    Error,
    /// Task was cancelled
    Cancelled,
    /// Manually resolved by operator
    ResolvedManually,
}

impl TaskState {
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Complete
                | TaskState::Error
                | TaskState::Cancelled
                | TaskState::ResolvedManually
        )
    }

    /// Check if this state requires processor ownership
    pub fn requires_ownership(&self) -> bool {
        matches!(
            self,
            TaskState::Initializing
                | TaskState::EnqueuingSteps
                | TaskState::StepsInProcess
                | TaskState::EvaluatingResults
        )
    }

    /// Check if this is an active processing state
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TaskState::Initializing
                | TaskState::EnqueuingSteps
                | TaskState::StepsInProcess
                | TaskState::EvaluatingResults
        )
    }

    /// Check if this is a waiting state
    pub fn is_waiting(&self) -> bool {
        matches!(
            self,
            TaskState::WaitingForDependencies
                | TaskState::WaitingForRetry
                | TaskState::BlockedByFailures
        )
    }

    /// Get the database string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Initializing => "initializing",
            TaskState::EnqueuingSteps => "enqueuing_steps",
            TaskState::StepsInProcess => "steps_in_process",
            TaskState::EvaluatingResults => "evaluating_results",
            TaskState::WaitingForDependencies => "waiting_for_dependencies",
            TaskState::WaitingForRetry => "waiting_for_retry",
            TaskState::BlockedByFailures => "blocked_by_failures",
            TaskState::Complete => "complete",
            TaskState::Error => "error",
            TaskState::Cancelled => "cancelled",
            TaskState::ResolvedManually => "resolved_manually",
        }
    }

    /// Check if this state can be picked up for processing
    pub fn can_be_processed(&self) -> bool {
        matches!(
            self,
            TaskState::Pending | TaskState::WaitingForDependencies | TaskState::WaitingForRetry
        )
    }

    /// All possible states as a slice (useful for validation)
    pub fn all_states() -> &'static [TaskState] {
        &[
            TaskState::Pending,
            TaskState::Initializing,
            TaskState::EnqueuingSteps,
            TaskState::StepsInProcess,
            TaskState::EvaluatingResults,
            TaskState::WaitingForDependencies,
            TaskState::WaitingForRetry,
            TaskState::BlockedByFailures,
            TaskState::Complete,
            TaskState::Error,
            TaskState::Cancelled,
            TaskState::ResolvedManually,
        ]
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<TaskState> for String {
    fn from(state: TaskState) -> Self {
        state.as_str().to_string()
    }
}

/// Error type for parsing invalid state strings
#[derive(Debug, Clone, thiserror::Error)]
#[error("Invalid task state: {0}")]
pub struct StateParseError(String);

impl TryFrom<&str> for TaskState {
    type Error = StateParseError;

    fn try_from(value: &str) -> Result<Self, <TaskState as TryFrom<&str>>::Error> {
        match value {
            "pending" => Ok(TaskState::Pending),
            "initializing" => Ok(TaskState::Initializing),
            "enqueuing_steps" => Ok(TaskState::EnqueuingSteps),
            "steps_in_process" => Ok(TaskState::StepsInProcess),
            "evaluating_results" => Ok(TaskState::EvaluatingResults),
            "waiting_for_dependencies" => Ok(TaskState::WaitingForDependencies),
            "waiting_for_retry" => Ok(TaskState::WaitingForRetry),
            "blocked_by_failures" => Ok(TaskState::BlockedByFailures),
            "complete" => Ok(TaskState::Complete),
            "error" => Ok(TaskState::Error),
            "cancelled" => Ok(TaskState::Cancelled),
            "resolved_manually" => Ok(TaskState::ResolvedManually),
            // Legacy state mapping for backward compatibility during migration
            "in_progress" => Ok(TaskState::StepsInProcess),
            _ => Err(StateParseError(value.to_string())),
        }
    }
}

impl TryFrom<String> for TaskState {
    type Error = StateParseError;

    fn try_from(value: String) -> Result<Self, <TaskState as TryFrom<String>>::Error> {
        Self::try_from(value.as_str())
    }
}

impl FromStr for TaskState {
    type Err = StateParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
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

    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowStepState::Pending => "pending",
            WorkflowStepState::Enqueued => "enqueued",
            WorkflowStepState::InProgress => "in_progress",
            WorkflowStepState::EnqueuedForOrchestration => "enqueued_for_orchestration",
            WorkflowStepState::Complete => "complete",
            WorkflowStepState::Error => "error",
            WorkflowStepState::Cancelled => "cancelled",
            WorkflowStepState::ResolvedManually => "resolved_manually",
        }
    }
}

impl fmt::Display for WorkflowStepState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<WorkflowStepState> for String {
    fn from(state: WorkflowStepState) -> Self {
        state.as_str().to_string()
    }
}

impl TryFrom<&str> for WorkflowStepState {
    type Error = StateParseError;

    fn try_from(value: &str) -> Result<Self, <WorkflowStepState as TryFrom<&str>>::Error> {
        match value {
            "pending" => Ok(WorkflowStepState::Pending),
            "enqueued" => Ok(WorkflowStepState::Enqueued),
            "in_progress" => Ok(WorkflowStepState::InProgress),
            "enqueued_for_orchestration" => Ok(WorkflowStepState::EnqueuedForOrchestration),
            "complete" => Ok(WorkflowStepState::Complete),
            "error" => Ok(WorkflowStepState::Error),
            "cancelled" => Ok(WorkflowStepState::Cancelled),
            "resolved_manually" => Ok(WorkflowStepState::ResolvedManually),
            _ => Err(StateParseError(value.to_string())),
        }
    }
}

impl TryFrom<String> for WorkflowStepState {
    type Error = StateParseError;

    fn try_from(value: String) -> Result<Self, <WorkflowStepState as TryFrom<String>>::Error> {
        Self::try_from(value.as_str())
    }
}

impl FromStr for WorkflowStepState {
    type Err = StateParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
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
    fn test_task_state_serialization() {
        assert_eq!(TaskState::Pending.as_str(), "pending");
        assert_eq!(TaskState::Initializing.as_str(), "initializing");
        assert_eq!(TaskState::EnqueuingSteps.as_str(), "enqueuing_steps");
        assert_eq!(TaskState::StepsInProcess.as_str(), "steps_in_process");
        assert_eq!(TaskState::EvaluatingResults.as_str(), "evaluating_results");
        assert_eq!(
            TaskState::WaitingForDependencies.as_str(),
            "waiting_for_dependencies"
        );
        assert_eq!(TaskState::WaitingForRetry.as_str(), "waiting_for_retry");
        assert_eq!(TaskState::BlockedByFailures.as_str(), "blocked_by_failures");
        assert_eq!(TaskState::Complete.as_str(), "complete");
        assert_eq!(TaskState::Error.as_str(), "error");
        assert_eq!(TaskState::Cancelled.as_str(), "cancelled");
        assert_eq!(TaskState::ResolvedManually.as_str(), "resolved_manually");
    }

    #[test]
    fn test_task_state_deserialization() {
        assert_eq!(TaskState::try_from("pending").unwrap(), TaskState::Pending);
        assert_eq!(
            TaskState::try_from("initializing").unwrap(),
            TaskState::Initializing
        );
        assert_eq!(
            TaskState::try_from("enqueuing_steps").unwrap(),
            TaskState::EnqueuingSteps
        );
        assert_eq!(
            TaskState::try_from("steps_in_process").unwrap(),
            TaskState::StepsInProcess
        );
        assert_eq!(
            TaskState::try_from("evaluating_results").unwrap(),
            TaskState::EvaluatingResults
        );
        assert_eq!(
            TaskState::try_from("waiting_for_dependencies").unwrap(),
            TaskState::WaitingForDependencies
        );
        assert_eq!(
            TaskState::try_from("waiting_for_retry").unwrap(),
            TaskState::WaitingForRetry
        );
        assert_eq!(
            TaskState::try_from("blocked_by_failures").unwrap(),
            TaskState::BlockedByFailures
        );
        assert_eq!(
            TaskState::try_from("complete").unwrap(),
            TaskState::Complete
        );
        assert_eq!(TaskState::try_from("error").unwrap(), TaskState::Error);
        assert_eq!(
            TaskState::try_from("cancelled").unwrap(),
            TaskState::Cancelled
        );
        assert_eq!(
            TaskState::try_from("resolved_manually").unwrap(),
            TaskState::ResolvedManually
        );

        // Legacy mapping
        assert_eq!(
            TaskState::try_from("in_progress").unwrap(),
            TaskState::StepsInProcess
        );

        // Invalid state
        assert!(TaskState::try_from("invalid_state").is_err());
    }

    #[test]
    fn test_terminal_states() {
        assert!(!TaskState::Pending.is_terminal());
        assert!(!TaskState::Initializing.is_terminal());
        assert!(!TaskState::EnqueuingSteps.is_terminal());
        assert!(!TaskState::StepsInProcess.is_terminal());
        assert!(!TaskState::EvaluatingResults.is_terminal());
        assert!(!TaskState::WaitingForDependencies.is_terminal());
        assert!(!TaskState::WaitingForRetry.is_terminal());
        assert!(!TaskState::BlockedByFailures.is_terminal());
        assert!(TaskState::Complete.is_terminal());
        assert!(TaskState::Error.is_terminal());
        assert!(TaskState::Cancelled.is_terminal());
        assert!(TaskState::ResolvedManually.is_terminal());
    }

    #[test]
    fn test_ownership_required_states() {
        assert!(!TaskState::Pending.requires_ownership());
        assert!(TaskState::Initializing.requires_ownership());
        assert!(TaskState::EnqueuingSteps.requires_ownership());
        assert!(TaskState::StepsInProcess.requires_ownership());
        assert!(TaskState::EvaluatingResults.requires_ownership());
        assert!(!TaskState::WaitingForDependencies.requires_ownership());
        assert!(!TaskState::WaitingForRetry.requires_ownership());
        assert!(!TaskState::BlockedByFailures.requires_ownership());
        assert!(!TaskState::Complete.requires_ownership());
        assert!(!TaskState::Error.requires_ownership());
        assert!(!TaskState::Cancelled.requires_ownership());
        assert!(!TaskState::ResolvedManually.requires_ownership());
    }

    #[test]
    fn test_active_states() {
        assert!(!TaskState::Pending.is_active());
        assert!(TaskState::Initializing.is_active());
        assert!(TaskState::EnqueuingSteps.is_active());
        assert!(TaskState::StepsInProcess.is_active());
        assert!(TaskState::EvaluatingResults.is_active());
        assert!(!TaskState::WaitingForDependencies.is_active());
        assert!(!TaskState::WaitingForRetry.is_active());
        assert!(!TaskState::BlockedByFailures.is_active());
        assert!(!TaskState::Complete.is_active());
        assert!(!TaskState::Error.is_active());
        assert!(!TaskState::Cancelled.is_active());
        assert!(!TaskState::ResolvedManually.is_active());
    }

    #[test]
    fn test_waiting_states() {
        assert!(!TaskState::Pending.is_waiting());
        assert!(!TaskState::Initializing.is_waiting());
        assert!(!TaskState::EnqueuingSteps.is_waiting());
        assert!(!TaskState::StepsInProcess.is_waiting());
        assert!(!TaskState::EvaluatingResults.is_waiting());
        assert!(TaskState::WaitingForDependencies.is_waiting());
        assert!(TaskState::WaitingForRetry.is_waiting());
        assert!(TaskState::BlockedByFailures.is_waiting());
        assert!(!TaskState::Complete.is_waiting());
        assert!(!TaskState::Error.is_waiting());
        assert!(!TaskState::Cancelled.is_waiting());
        assert!(!TaskState::ResolvedManually.is_waiting());
    }

    #[test]
    fn test_processable_states() {
        assert!(TaskState::Pending.can_be_processed());
        assert!(!TaskState::Initializing.can_be_processed());
        assert!(!TaskState::EnqueuingSteps.can_be_processed());
        assert!(!TaskState::StepsInProcess.can_be_processed());
        assert!(!TaskState::EvaluatingResults.can_be_processed());
        assert!(TaskState::WaitingForDependencies.can_be_processed());
        assert!(TaskState::WaitingForRetry.can_be_processed());
        assert!(!TaskState::BlockedByFailures.can_be_processed());
        assert!(!TaskState::Complete.can_be_processed());
        assert!(!TaskState::Error.can_be_processed());
        assert!(!TaskState::Cancelled.can_be_processed());
        assert!(!TaskState::ResolvedManually.can_be_processed());
    }

    #[test]
    fn test_all_states() {
        let states = TaskState::all_states();
        assert_eq!(states.len(), 12);
        assert!(states.contains(&TaskState::Pending));
        assert!(states.contains(&TaskState::Initializing));
        assert!(states.contains(&TaskState::EnqueuingSteps));
        assert!(states.contains(&TaskState::StepsInProcess));
        assert!(states.contains(&TaskState::EvaluatingResults));
        assert!(states.contains(&TaskState::WaitingForDependencies));
        assert!(states.contains(&TaskState::WaitingForRetry));
        assert!(states.contains(&TaskState::BlockedByFailures));
        assert!(states.contains(&TaskState::Complete));
        assert!(states.contains(&TaskState::Error));
        assert!(states.contains(&TaskState::Cancelled));
        assert!(states.contains(&TaskState::ResolvedManually));
    }
}

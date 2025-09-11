//! # System Constants and Configuration
//!
//! Core constants, enums, and configuration types that define the operational
//! boundaries of the Tasker workflow orchestration system.
//!
//! This module maintains compatibility with the Rails Tasker engine while
//! providing type-safe Rust equivalents of all system constants.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Import state types directly from state_machine module
use crate::state_machine::{TaskState, WorkflowStepState};

/// Core system events that trigger state transitions and orchestration actions
pub mod events {
    // Task lifecycle events
    pub const TASK_INITIALIZE_REQUESTED: &str = "task.initialize_requested";
    pub const TASK_START_REQUESTED: &str = "task.start_requested";
    pub const TASK_COMPLETED: &str = "task.completed";
    pub const TASK_FAILED: &str = "task.failed";
    pub const TASK_RETRY_REQUESTED: &str = "task.retry_requested";
    pub const TASK_CANCELLED: &str = "task.cancelled";
    pub const TASK_RESOLVED_MANUALLY: &str = "task.resolved_manually";
    pub const TASK_BEFORE_TRANSITION: &str = "task.before_transition";

    // TAS-41 Task orchestration lifecycle events
    pub const TASK_STEPS_DISCOVERY_COMPLETED: &str = "task.steps_discovery_completed";
    pub const TASK_STEPS_ENQUEUED: &str = "task.steps_enqueued";
    pub const TASK_STEP_RESULTS_RECEIVED: &str = "task.step_results_received";
    pub const TASK_AWAITING_DEPENDENCIES: &str = "task.awaiting_dependencies";
    pub const TASK_RETRY_BACKOFF_STARTED: &str = "task.retry_backoff_started";
    pub const TASK_BLOCKED_BY_FAILURES: &str = "task.blocked_by_failures";
    pub const TASK_DEPENDENCIES_SATISFIED: &str = "task.dependencies_satisfied";

    // Step lifecycle events
    pub const STEP_INITIALIZE_REQUESTED: &str = "step.initialize_requested";
    pub const STEP_ENQUEUE_REQUESTED: &str = "step.enqueue_requested";
    pub const STEP_EXECUTION_REQUESTED: &str = "step.execution_requested";
    pub const STEP_BEFORE_HANDLE: &str = "step.before_handle";
    pub const STEP_HANDLE: &str = "step.handle";
    pub const STEP_COMPLETED: &str = "step.completed";
    pub const STEP_FAILED: &str = "step.failed";
    pub const STEP_RETRY_REQUESTED: &str = "step.retry_requested";
    pub const STEP_CANCELLED: &str = "step.cancelled";
    pub const STEP_RESOLVED_MANUALLY: &str = "step.resolved_manually";
    pub const STEP_BEFORE_TRANSITION: &str = "step.before_transition";

    // Step orchestration events
    pub const STEP_RESULT_SUBMITTED: &str = "step.result_submitted";

    // Workflow orchestration events
    pub const WORKFLOW_TASK_STARTED: &str = "workflow.task_started";
    pub const WORKFLOW_TASK_COMPLETED: &str = "workflow.task_completed";
    pub const WORKFLOW_TASK_FAILED: &str = "workflow.task_failed";
    pub const WORKFLOW_STEP_COMPLETED: &str = "workflow.step_completed";
    pub const WORKFLOW_STEP_FAILED: &str = "workflow.step_failed";
    pub const WORKFLOW_VIABLE_STEPS_DISCOVERED: &str = "workflow.viable_steps_discovered";
    pub const WORKFLOW_NO_VIABLE_STEPS: &str = "workflow.no_viable_steps";
    pub const WORKFLOW_ORCHESTRATION_REQUESTED: &str = "workflow.orchestration_requested";
}

/// Task execution context states for orchestration decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    HasReadySteps,
    Processing,
    BlockedByFailures,
    AllComplete,
    WaitingForDependencies,
}

/// Recommended orchestration actions based on task analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedAction {
    ExecuteReadySteps,
    WaitForCompletion,
    HandleFailures,
    FinalizeTask,
    WaitForDependencies,
}

/// System health indicators for monitoring and alerting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Recovering,
    Blocked,
    Unknown,
}

/// Workflow edge relationship types for DAG construction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEdgeType {
    Provides,
    // Future edge types can be added here
}

impl WorkflowEdgeType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Provides => "provides",
        }
    }
}

/// Reasons for task reenqueue decisions in orchestration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReenqueueReason {
    ContextUnavailable,
    StepsInProgress,
    AwaitingDependencies,
    ReadyStepsAvailable,
    ContinuingWorkflow,
    PendingStepsRemaining,
    RetryBackoff,
}

/// Reasons for task pending state in workflow analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingReason {
    ContextUnavailable,
    WaitingForStepCompletion,
    WaitingForDependencies,
    ReadyForProcessing,
    WorkflowPaused,
}

/// System-wide constants
pub mod system {
    /// Unknown value placeholder
    pub const UNKNOWN: &str = "unknown";

    /// Default edge name for workflow dependencies
    pub const PROVIDES_EDGE_NAME: &str = "provides";

    /// Version compatibility marker
    pub const TASKER_CORE_VERSION: &str = "0.1.0";

    /// Maximum recursion depth for dependency resolution
    pub const MAX_DEPENDENCY_DEPTH: usize = 50;

    /// Maximum number of steps in a single workflow
    pub const MAX_WORKFLOW_STEPS: usize = 1000;
}

/// Status groupings for validation and logic
pub mod status_groups {
    use super::{TaskState, WorkflowStepState};

    /// Workflow step statuses that indicate completion
    pub const VALID_STEP_COMPLETION_STATES: &[WorkflowStepState] = &[
        WorkflowStepState::Complete,
        WorkflowStepState::ResolvedManually,
        WorkflowStepState::Cancelled,
    ];

    /// Workflow step statuses that indicate active work
    pub const VALID_STEP_STILL_WORKING_STATES: &[WorkflowStepState] = &[
        WorkflowStepState::Pending,
        WorkflowStepState::Enqueued,
        WorkflowStepState::InProgress,
        WorkflowStepState::EnqueuedForOrchestration,
    ];

    /// Workflow step statuses that make steps unavailable for execution
    pub const UNREADY_WORKFLOW_STEP_STATUSES: &[WorkflowStepState] = &[
        WorkflowStepState::Enqueued,
        WorkflowStepState::InProgress,
        WorkflowStepState::EnqueuedForOrchestration,
        WorkflowStepState::Complete,
        WorkflowStepState::Cancelled,
        WorkflowStepState::ResolvedManually,
    ];

    /// Task statuses that indicate final completion (terminal states)
    pub const TASK_FINAL_STATES: &[TaskState] = &[
        TaskState::Complete,
        TaskState::Error,
        TaskState::Cancelled,
        TaskState::ResolvedManually,
    ];

    /// Task statuses that indicate active orchestration is happening
    pub const TASK_ACTIVE_STATES: &[TaskState] = &[
        TaskState::Initializing,
        TaskState::EnqueuingSteps,
        TaskState::StepsInProcess,
        TaskState::EvaluatingResults,
    ];

    /// Task statuses that indicate waiting/blocked states
    pub const TASK_WAITING_STATES: &[TaskState] = &[
        TaskState::WaitingForDependencies,
        TaskState::WaitingForRetry,
        TaskState::BlockedByFailures,
    ];

    /// Task statuses that can be picked up for processing
    pub const TASK_PROCESSABLE_STATES: &[TaskState] = &[
        TaskState::Pending,
        TaskState::WaitingForDependencies,
        TaskState::WaitingForRetry,
    ];

    /// All TAS-41 task states for validation
    pub const ALL_TASK_STATES: &[TaskState] = &[
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
    ];

    /// All workflow step states for validation
    pub const ALL_STEP_STATES: &[WorkflowStepState] = &[
        WorkflowStepState::Pending,
        WorkflowStepState::Enqueued,
        WorkflowStepState::InProgress,
        WorkflowStepState::EnqueuedForOrchestration,
        WorkflowStepState::Complete,
        WorkflowStepState::Error,
        WorkflowStepState::Cancelled,
        WorkflowStepState::ResolvedManually,
    ];
}

/// State transition event mapping
pub type TaskTransitionKey = (Option<TaskState>, TaskState);
pub type TaskTransitionMap = HashMap<TaskTransitionKey, &'static str>;
pub type StepTransitionKey = (Option<WorkflowStepState>, WorkflowStepState);
pub type StepTransitionMap = HashMap<StepTransitionKey, &'static str>;

/// Build task transition event map with TAS-41 comprehensive states
#[must_use]
pub fn build_task_transition_map() -> TaskTransitionMap {
    let mut map = HashMap::new();

    // Initial state transitions (from None/creation)
    map.insert(
        (None, TaskState::Pending),
        events::TASK_INITIALIZE_REQUESTED,
    );

    // Forward progression through TAS-41 lifecycle
    map.insert(
        (Some(TaskState::Pending), TaskState::Initializing),
        events::TASK_START_REQUESTED,
    );
    map.insert(
        (Some(TaskState::Initializing), TaskState::EnqueuingSteps),
        events::TASK_STEPS_DISCOVERY_COMPLETED,
    );
    map.insert(
        (Some(TaskState::EnqueuingSteps), TaskState::StepsInProcess),
        events::TASK_STEPS_ENQUEUED,
    );
    map.insert(
        (
            Some(TaskState::StepsInProcess),
            TaskState::EvaluatingResults,
        ),
        events::TASK_STEP_RESULTS_RECEIVED,
    );

    // Completion flow
    map.insert(
        (Some(TaskState::EvaluatingResults), TaskState::Complete),
        events::TASK_COMPLETED,
    );

    // Waiting states transitions
    map.insert(
        (
            Some(TaskState::EvaluatingResults),
            TaskState::WaitingForDependencies,
        ),
        events::TASK_AWAITING_DEPENDENCIES,
    );
    map.insert(
        (
            Some(TaskState::EvaluatingResults),
            TaskState::WaitingForRetry,
        ),
        events::TASK_RETRY_BACKOFF_STARTED,
    );
    map.insert(
        (
            Some(TaskState::EvaluatingResults),
            TaskState::BlockedByFailures,
        ),
        events::TASK_BLOCKED_BY_FAILURES,
    );

    // Return to processing from waiting states
    map.insert(
        (
            Some(TaskState::WaitingForDependencies),
            TaskState::EnqueuingSteps,
        ),
        events::TASK_DEPENDENCIES_SATISFIED,
    );
    map.insert(
        (Some(TaskState::WaitingForRetry), TaskState::EnqueuingSteps),
        events::TASK_RETRY_REQUESTED,
    );

    // Error transitions
    map.insert(
        (Some(TaskState::Initializing), TaskState::Error),
        events::TASK_FAILED,
    );
    map.insert(
        (Some(TaskState::EnqueuingSteps), TaskState::Error),
        events::TASK_FAILED,
    );
    map.insert(
        (Some(TaskState::StepsInProcess), TaskState::Error),
        events::TASK_FAILED,
    );
    map.insert(
        (Some(TaskState::EvaluatingResults), TaskState::Error),
        events::TASK_FAILED,
    );

    // Manual intervention transitions (from any state)
    for &state in status_groups::ALL_TASK_STATES {
        if state != TaskState::Cancelled && state != TaskState::ResolvedManually {
            map.insert((Some(state), TaskState::Cancelled), events::TASK_CANCELLED);
            map.insert(
                (Some(state), TaskState::ResolvedManually),
                events::TASK_RESOLVED_MANUALLY,
            );
        }
    }

    // Direct transitions for initialization
    map.insert((None, TaskState::Complete), events::TASK_COMPLETED);
    map.insert((None, TaskState::Error), events::TASK_FAILED);
    map.insert((None, TaskState::Cancelled), events::TASK_CANCELLED);
    map.insert(
        (None, TaskState::ResolvedManually),
        events::TASK_RESOLVED_MANUALLY,
    );

    map
}

/// Build step transition event map with orchestration states
#[must_use]
pub fn build_step_transition_map() -> StepTransitionMap {
    let mut map = HashMap::new();

    // Initial state transitions (from None)
    map.insert(
        (None, WorkflowStepState::Pending),
        events::STEP_INITIALIZE_REQUESTED,
    );
    map.insert(
        (None, WorkflowStepState::Enqueued),
        events::STEP_ENQUEUE_REQUESTED,
    );
    map.insert(
        (None, WorkflowStepState::InProgress),
        events::STEP_EXECUTION_REQUESTED,
    );
    map.insert((None, WorkflowStepState::Complete), events::STEP_COMPLETED);
    map.insert((None, WorkflowStepState::Error), events::STEP_FAILED);
    map.insert((None, WorkflowStepState::Cancelled), events::STEP_CANCELLED);
    map.insert(
        (None, WorkflowStepState::ResolvedManually),
        events::STEP_RESOLVED_MANUALLY,
    );

    // Standard lifecycle transitions
    map.insert(
        (
            Some(WorkflowStepState::Pending),
            WorkflowStepState::Enqueued,
        ),
        events::STEP_ENQUEUE_REQUESTED,
    );
    map.insert(
        (
            Some(WorkflowStepState::Enqueued),
            WorkflowStepState::InProgress,
        ),
        events::STEP_EXECUTION_REQUESTED,
    );

    // Worker completion flow with orchestration queue
    map.insert(
        (
            Some(WorkflowStepState::InProgress),
            WorkflowStepState::EnqueuedForOrchestration,
        ),
        events::STEP_RESULT_SUBMITTED,
    );
    map.insert(
        (
            Some(WorkflowStepState::EnqueuedForOrchestration),
            WorkflowStepState::Complete,
        ),
        events::STEP_COMPLETED,
    );
    map.insert(
        (
            Some(WorkflowStepState::EnqueuedForOrchestration),
            WorkflowStepState::Error,
        ),
        events::STEP_FAILED,
    );

    // Direct completion/failure from in_progress (for backwards compatibility)
    map.insert(
        (
            Some(WorkflowStepState::InProgress),
            WorkflowStepState::Complete,
        ),
        events::STEP_COMPLETED,
    );
    map.insert(
        (
            Some(WorkflowStepState::InProgress),
            WorkflowStepState::Error,
        ),
        events::STEP_FAILED,
    );

    // Retry transitions
    map.insert(
        (Some(WorkflowStepState::Error), WorkflowStepState::Pending),
        events::STEP_RETRY_REQUESTED,
    );

    // Manual intervention transitions (from any non-terminal state)
    for &state in status_groups::ALL_STEP_STATES {
        if !matches!(
            state,
            WorkflowStepState::Complete
                | WorkflowStepState::Cancelled
                | WorkflowStepState::ResolvedManually
        ) {
            map.insert(
                (Some(state), WorkflowStepState::Cancelled),
                events::STEP_CANCELLED,
            );
            map.insert(
                (Some(state), WorkflowStepState::ResolvedManually),
                events::STEP_RESOLVED_MANUALLY,
            );
        }
    }

    map
}

/// Convenience functions for status checking
impl ExecutionStatus {
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

impl HealthStatus {
    /// Check if this health status indicates a problem
    pub const fn is_problematic(&self) -> bool {
        matches!(self, Self::Blocked | Self::Unknown)
    }

    /// Check if this health status indicates normal operation
    pub const fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy | Self::Recovering)
    }
}

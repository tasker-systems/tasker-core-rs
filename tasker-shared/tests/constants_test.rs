//! Tests for system constants and configuration

use tasker_shared::constants::*;
use tasker_shared::state_machine::{TaskState, WorkflowStepState};
use tasker_shared::{
    config::{BackoffConfig, ExecutionConfig, ReenqueueDelays},
    system_events,
};

#[test]
fn test_execution_config_defaults() {
    let config = ExecutionConfig::default();
    assert_eq!(config.max_concurrent_tasks, 100);
    assert_eq!(config.max_concurrent_steps, 1000);
    assert_eq!(config.default_timeout_seconds, 3600);
    assert_eq!(config.step_execution_timeout_seconds, 300);
}

#[test]
fn test_backoff_config_defaults() {
    let config = BackoffConfig::default();
    assert_eq!(config.default_backoff_seconds, vec![1, 2, 4, 8, 16, 32]);
    assert_eq!(config.max_backoff_seconds, 60);
    assert!(config.jitter_enabled);
    assert_eq!(config.backoff_multiplier, 2.0);
    assert_eq!(config.jitter_max_percentage, 0.5);
}

#[test]
fn test_reenqueue_delays_defaults() {
    let delays = ReenqueueDelays::default();
    assert_eq!(delays.initializing, 5);
    assert_eq!(delays.enqueuing_steps, 0);
    assert_eq!(delays.evaluating_results, 5);
    assert_eq!(delays.steps_in_process, 10);
    assert_eq!(delays.waiting_for_dependencies, 45);
    assert_eq!(delays.waiting_for_retry, 30);
    assert_eq!(delays.blocked_by_failures, 60);
}

#[test]
fn test_system_events() {
    assert_eq!(
        system_events::TASK_INITIALIZE_REQUESTED,
        "task.initialize_requested"
    );
    assert_eq!(system_events::TASK_START_REQUESTED, "task.start_requested");
    assert_eq!(system_events::TASK_COMPLETED, "task.completed");
    assert_eq!(
        system_events::STEP_EXECUTION_REQUESTED,
        "step.execution_requested"
    );
    assert_eq!(
        system_events::WORKFLOW_ORCHESTRATION_REQUESTED,
        "workflow.orchestration_requested"
    );
}

#[test]
fn test_execution_status_helpers() {
    assert!(ExecutionStatus::Processing.is_active());
    assert!(ExecutionStatus::HasReadySteps.is_active());
    assert!(!ExecutionStatus::AllComplete.is_active());

    assert!(ExecutionStatus::BlockedByFailures.is_blocked());
    assert!(ExecutionStatus::WaitingForDependencies.is_blocked());
    assert!(!ExecutionStatus::Processing.is_blocked());

    assert!(ExecutionStatus::AllComplete.is_complete());
    assert!(!ExecutionStatus::Processing.is_complete());
}

#[test]
fn test_health_status_helpers() {
    assert!(HealthStatus::Blocked.is_problematic());
    assert!(HealthStatus::Unknown.is_problematic());
    assert!(!HealthStatus::Healthy.is_problematic());

    assert!(HealthStatus::Healthy.is_healthy());
    assert!(HealthStatus::Recovering.is_healthy());
    assert!(!HealthStatus::Blocked.is_healthy());
}

#[test]
fn test_workflow_edge_type() {
    assert_eq!(WorkflowEdgeType::Provides.as_str(), "provides");
}

#[test]
fn test_status_groups() {
    // Test step completion states
    assert!(status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepState::Complete));
    assert!(
        status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepState::ResolvedManually)
    );
    assert!(status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepState::Cancelled));
    assert!(!status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepState::Pending));

    // Test step working states
    assert!(status_groups::VALID_STEP_STILL_WORKING_STATES.contains(&WorkflowStepState::Pending));
    assert!(status_groups::VALID_STEP_STILL_WORKING_STATES.contains(&WorkflowStepState::Enqueued));
    assert!(status_groups::VALID_STEP_STILL_WORKING_STATES.contains(&WorkflowStepState::InProgress));
    assert!(status_groups::VALID_STEP_STILL_WORKING_STATES
        .contains(&WorkflowStepState::EnqueuedForOrchestration));
    assert!(!status_groups::VALID_STEP_STILL_WORKING_STATES.contains(&WorkflowStepState::Complete));

    // Test unready step statuses
    assert!(status_groups::UNREADY_WORKFLOW_STEP_STATUSES.contains(&WorkflowStepState::Enqueued));
    assert!(status_groups::UNREADY_WORKFLOW_STEP_STATUSES.contains(&WorkflowStepState::InProgress));
    assert!(status_groups::UNREADY_WORKFLOW_STEP_STATUSES
        .contains(&WorkflowStepState::EnqueuedForOrchestration));
    assert!(status_groups::UNREADY_WORKFLOW_STEP_STATUSES.contains(&WorkflowStepState::Complete));
    assert!(!status_groups::UNREADY_WORKFLOW_STEP_STATUSES.contains(&WorkflowStepState::Pending));

    // Test task final states (terminal states)
    assert!(status_groups::TASK_FINAL_STATES.contains(&TaskState::Complete));
    assert!(status_groups::TASK_FINAL_STATES.contains(&TaskState::Error));
    assert!(status_groups::TASK_FINAL_STATES.contains(&TaskState::Cancelled));
    assert!(status_groups::TASK_FINAL_STATES.contains(&TaskState::ResolvedManually));
    assert!(!status_groups::TASK_FINAL_STATES.contains(&TaskState::Pending));

    // Test task active states (TAS-41 orchestration states)
    assert!(status_groups::TASK_ACTIVE_STATES.contains(&TaskState::Initializing));
    assert!(status_groups::TASK_ACTIVE_STATES.contains(&TaskState::EnqueuingSteps));
    assert!(status_groups::TASK_ACTIVE_STATES.contains(&TaskState::StepsInProcess));
    assert!(status_groups::TASK_ACTIVE_STATES.contains(&TaskState::EvaluatingResults));
    assert!(!status_groups::TASK_ACTIVE_STATES.contains(&TaskState::Pending));
    assert!(!status_groups::TASK_ACTIVE_STATES.contains(&TaskState::Complete));

    // Test task waiting states
    assert!(status_groups::TASK_WAITING_STATES.contains(&TaskState::WaitingForDependencies));
    assert!(status_groups::TASK_WAITING_STATES.contains(&TaskState::WaitingForRetry));
    assert!(status_groups::TASK_WAITING_STATES.contains(&TaskState::BlockedByFailures));
    assert!(!status_groups::TASK_WAITING_STATES.contains(&TaskState::Pending));
    assert!(!status_groups::TASK_WAITING_STATES.contains(&TaskState::Complete));

    // Test task processable states
    assert!(status_groups::TASK_PROCESSABLE_STATES.contains(&TaskState::Pending));
    assert!(status_groups::TASK_PROCESSABLE_STATES.contains(&TaskState::WaitingForDependencies));
    assert!(status_groups::TASK_PROCESSABLE_STATES.contains(&TaskState::WaitingForRetry));
    assert!(!status_groups::TASK_PROCESSABLE_STATES.contains(&TaskState::Initializing));
    assert!(!status_groups::TASK_PROCESSABLE_STATES.contains(&TaskState::Complete));

    // Test all states arrays have correct counts
    assert_eq!(status_groups::ALL_TASK_STATES.len(), 12); // TAS-41 comprehensive states
    assert_eq!(status_groups::ALL_STEP_STATES.len(), 8); // Including EnqueuedForOrchestration
}

#[test]
fn test_transition_maps() {
    let task_map = build_task_transition_map();
    let step_map = build_step_transition_map();

    // Test key TAS-41 task transitions exist
    assert_eq!(
        task_map.get(&(None, TaskState::Pending)),
        Some(&system_events::TASK_INITIALIZE_REQUESTED)
    );
    assert_eq!(
        task_map.get(&(Some(TaskState::Pending), TaskState::Initializing)),
        Some(&system_events::TASK_START_REQUESTED)
    );
    assert_eq!(
        task_map.get(&(Some(TaskState::EvaluatingResults), TaskState::Complete)),
        Some(&system_events::TASK_COMPLETED)
    );

    // Test key step transitions exist
    assert_eq!(
        step_map.get(&(None, WorkflowStepState::Pending)),
        Some(&system_events::STEP_INITIALIZE_REQUESTED)
    );
    assert_eq!(
        step_map.get(&(
            Some(WorkflowStepState::Pending),
            WorkflowStepState::Enqueued
        )),
        Some(&system_events::STEP_ENQUEUE_REQUESTED)
    );
    assert_eq!(
        step_map.get(&(
            Some(WorkflowStepState::Enqueued),
            WorkflowStepState::InProgress
        )),
        Some(&system_events::STEP_EXECUTION_REQUESTED)
    );
    assert_eq!(
        step_map.get(&(
            Some(WorkflowStepState::EnqueuedForOrchestration),
            WorkflowStepState::Complete
        )),
        Some(&system_events::STEP_COMPLETED)
    );
}

#[test]
fn test_system_constants() {
    assert_eq!(system::UNKNOWN, "unknown");
    assert_eq!(system::PROVIDES_EDGE_NAME, "provides");
    // Configuration-driven constants - tested via config system instead of hardcoded values
    assert_eq!(system::TASKER_CORE_VERSION, "0.1.0");
}

#[test]
fn test_enum_serialization() {
    // Test that our enums serialize correctly for JSON APIs
    use serde_json;

    assert_eq!(
        serde_json::to_string(&ExecutionStatus::HasReadySteps).unwrap(),
        "\"has_ready_steps\""
    );
    assert_eq!(
        serde_json::to_string(&RecommendedAction::ExecuteReadySteps).unwrap(),
        "\"execute_ready_steps\""
    );
    assert_eq!(
        serde_json::to_string(&WorkflowEdgeType::Provides).unwrap(),
        "\"provides\""
    );
}

#[test]
fn test_enum_deserialization() {
    // Test that our enums deserialize correctly from JSON APIs
    use serde_json;

    let execution_status: ExecutionStatus = serde_json::from_str("\"has_ready_steps\"").unwrap();
    assert_eq!(execution_status, ExecutionStatus::HasReadySteps);

    let action: RecommendedAction = serde_json::from_str("\"execute_ready_steps\"").unwrap();
    assert_eq!(action, RecommendedAction::ExecuteReadySteps);

    let edge_type: WorkflowEdgeType = serde_json::from_str("\"provides\"").unwrap();
    assert_eq!(edge_type, WorkflowEdgeType::Provides);
}

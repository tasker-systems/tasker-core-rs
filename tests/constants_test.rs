//! Tests for system constants and configuration

use tasker_core::constants::*;
use tasker_core::{system_events, BackoffConfig, ExecutionConfig, ReenqueueDelays};

#[test]
fn test_execution_config_defaults() {
    let config = ExecutionConfig::default();
    assert_eq!(config.min_concurrent_steps, 3);
    assert_eq!(config.max_concurrent_steps_limit, 12);
    assert_eq!(config.concurrency_cache_duration, 30);
    assert_eq!(config.batch_timeout_base_seconds, 30);
    assert_eq!(config.max_batch_timeout_seconds, 120);
}

#[test]
fn test_backoff_config_defaults() {
    let config = BackoffConfig::default();
    assert_eq!(config.default_backoff_seconds, vec![1, 2, 4, 8, 16, 32]);
    assert_eq!(config.max_backoff_seconds, 300);
    assert!(config.jitter_enabled);
    assert_eq!(config.backoff_multiplier, 2.0);
    assert_eq!(config.jitter_max_percentage, 0.1);
}

#[test]
fn test_reenqueue_delays_defaults() {
    let delays = ReenqueueDelays::default();
    assert_eq!(delays.has_ready_steps, 0);
    assert_eq!(delays.waiting_for_dependencies, 45);
    assert_eq!(delays.processing, 10);
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
    assert!(status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepStatus::Complete));
    assert!(
        status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepStatus::ResolvedManually)
    );
    assert!(status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepStatus::Cancelled));
    assert!(!status_groups::VALID_STEP_COMPLETION_STATES.contains(&WorkflowStepStatus::Pending));

    // Test step working states
    assert!(status_groups::VALID_STEP_STILL_WORKING_STATES.contains(&WorkflowStepStatus::Pending));
    assert!(
        status_groups::VALID_STEP_STILL_WORKING_STATES.contains(&WorkflowStepStatus::InProgress)
    );
    assert!(!status_groups::VALID_STEP_STILL_WORKING_STATES.contains(&WorkflowStepStatus::Complete));

    // Test unready step statuses
    assert!(status_groups::UNREADY_WORKFLOW_STEP_STATUSES.contains(&WorkflowStepStatus::InProgress));
    assert!(status_groups::UNREADY_WORKFLOW_STEP_STATUSES.contains(&WorkflowStepStatus::Complete));
    assert!(!status_groups::UNREADY_WORKFLOW_STEP_STATUSES.contains(&WorkflowStepStatus::Pending));

    // Test task final states
    assert!(status_groups::TASK_FINAL_STATES.contains(&TaskStatus::Complete));
    assert!(status_groups::TASK_FINAL_STATES.contains(&TaskStatus::Cancelled));
    assert!(status_groups::TASK_FINAL_STATES.contains(&TaskStatus::ResolvedManually));
    assert!(!status_groups::TASK_FINAL_STATES.contains(&TaskStatus::Pending));

    // Test task active states
    assert!(status_groups::TASK_ACTIVE_STATES.contains(&TaskStatus::Pending));
    assert!(status_groups::TASK_ACTIVE_STATES.contains(&TaskStatus::InProgress));
    assert!(status_groups::TASK_ACTIVE_STATES.contains(&TaskStatus::Error));
    assert!(!status_groups::TASK_ACTIVE_STATES.contains(&TaskStatus::Complete));
}

#[test]
fn test_transition_maps() {
    let task_map = build_task_transition_map();
    let step_map = build_step_transition_map();

    // Test key task transitions exist
    assert_eq!(
        task_map.get(&(None, TaskStatus::Pending)),
        Some(&system_events::TASK_INITIALIZE_REQUESTED)
    );
    assert_eq!(
        task_map.get(&(Some(TaskStatus::Pending), TaskStatus::InProgress)),
        Some(&system_events::TASK_START_REQUESTED)
    );
    assert_eq!(
        task_map.get(&(Some(TaskStatus::InProgress), TaskStatus::Complete)),
        Some(&system_events::TASK_COMPLETED)
    );

    // Test key step transitions exist
    assert_eq!(
        step_map.get(&(None, WorkflowStepStatus::Pending)),
        Some(&system_events::STEP_INITIALIZE_REQUESTED)
    );
    assert_eq!(
        step_map.get(&(
            Some(WorkflowStepStatus::Pending),
            WorkflowStepStatus::InProgress
        )),
        Some(&system_events::STEP_EXECUTION_REQUESTED)
    );
    assert_eq!(
        step_map.get(&(
            Some(WorkflowStepStatus::InProgress),
            WorkflowStepStatus::Complete
        )),
        Some(&system_events::STEP_COMPLETED)
    );
}

#[test]
fn test_system_constants() {
    assert_eq!(system::UNKNOWN, "unknown");
    assert_eq!(system::PROVIDES_EDGE_NAME, "provides");
    assert_eq!(system::TASKER_CORE_VERSION, "0.1.0");
    assert_eq!(system::MAX_DEPENDENCY_DEPTH, 50);
    assert_eq!(system::MAX_WORKFLOW_STEPS, 1000);
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

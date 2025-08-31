use chrono::Utc;
use std::sync::Arc;
use tasker_orchestration::orchestration::state_manager::{
    StateEntityType, StateHealthSummary, StateManager, StateTransitionEvent, StateTransitionRequest,
};
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::events::publisher::EventPublisher;
use tasker_shared::state_machine::events::TaskEvent;
use uuid::Uuid;

#[test]
fn test_state_health_summary_calculations() {
    let summary = StateHealthSummary {
        total_tasks: 10,
        pending_tasks: 2,
        in_progress_tasks: 3,
        completed_tasks: 4,
        failed_tasks: 1,
        total_steps: 50,
        pending_steps: 10,
        in_progress_steps: 15,
        completed_steps: 20,
        failed_steps: 5,
        overall_health_score: 0.85,
        last_updated: Utc::now(),
    };

    assert!(summary.is_healthy());
    assert_eq!(summary.task_completion_percentage(), 40.0);
    assert_eq!(summary.step_completion_percentage(), 40.0);
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_state_manager_creation(pool: sqlx::PgPool) {
    let sql_executor = SqlFunctionExecutor::new(pool.clone());
    let event_publisher = Arc::new(EventPublisher::new());
    let state_manager = StateManager::new(sql_executor, event_publisher, pool);

    // Test that we can create the StateManager successfully
    // Since the fields are private, we can't directly test them, but creation success is sufficient
    // More comprehensive testing would require public methods or integration tests
    drop(state_manager); // Verify it was created successfully
}

#[test]
fn test_state_transition_request() {
    let entity_uuid = Uuid::new_v4();
    let request = StateTransitionRequest {
        entity_uuid,
        entity_type: StateEntityType::Task,
        target_state: "complete".to_string(),
        event: StateTransitionEvent::TaskEvent(TaskEvent::Complete),
        metadata: None,
    };

    assert_eq!(request.entity_uuid, entity_uuid);
    assert_eq!(request.entity_type, StateEntityType::Task);
    assert_eq!(request.target_state, "complete");
}

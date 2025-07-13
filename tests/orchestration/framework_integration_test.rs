//! Framework Integration Tests
//!
//! Tests the framework integration interface using the mock framework implementation.

use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tasker_core::orchestration::types::{
    FrameworkIntegration, StepStatus, TaskContext, TaskResult, ViableStep,
};

// Import our mock framework
#[path = "../mocks/mod.rs"]
mod mocks;
use mocks::mock_framework::MockFramework;

/// Helper to create a test viable step
fn create_viable_step(step_id: i64, task_id: i64, name: &str) -> ViableStep {
    ViableStep {
        step_id,
        task_id,
        name: name.to_string(),
        named_step_id: (step_id + 1000) as i32,
        current_state: "pending".to_string(),
        dependencies_satisfied: true,
        retry_eligible: true,
        attempts: 0,
        retry_limit: 3,
        last_failure_at: None,
        next_retry_at: None,
    }
}

/// Helper to create test task context
#[allow(dead_code)]
fn create_task_context(task_id: i64) -> TaskContext {
    TaskContext {
        task_id,
        data: json!({
            "test_data": "example",
            "task_type": "integration_test"
        }),
        metadata: Default::default(),
    }
}

#[tokio::test]
async fn test_single_step_execution_with_mock_framework() {
    // Create mock framework
    let framework = MockFramework::new("test_framework");

    // Add task context
    framework.add_task_context(123, json!({"test": "data"}));

    // Create a viable step
    let step = create_viable_step(1, 123, "test_step");

    // Get task context from framework
    let task_context = framework.get_task_context(123).await.unwrap();

    // Execute the step directly through the framework interface
    let result = framework
        .execute_single_step(&step, &task_context)
        .await
        .unwrap();

    // Verify the result
    assert_eq!(result.step_id, 1);
    assert_eq!(result.status, StepStatus::Completed);
    assert!(result.output["processed"].as_bool().unwrap());
    assert_eq!(result.output["step_name"], "test_step");

    // Verify framework tracked the execution
    let state = framework.get_state();
    assert_eq!(state.executed_steps.len(), 1);
    assert_eq!(state.executed_steps[0], (1, "test_step".to_string()));
}

#[tokio::test]
async fn test_concurrent_step_execution() {
    let framework = Arc::new(
        MockFramework::new("concurrent_test").with_execution_delay(Duration::from_millis(50)),
    );

    // Add task context
    let task_id = 456;
    framework.add_task_context(
        task_id,
        json!({
            "workflow": "concurrent_test",
            "parallel_steps": true
        }),
    );

    // Create multiple steps that can run concurrently
    let steps = vec![
        create_viable_step(1, task_id, "step_1"),
        create_viable_step(2, task_id, "step_2"),
        create_viable_step(3, task_id, "step_3"),
    ];

    let task_context = framework.get_task_context(task_id).await.unwrap();

    // Execute steps concurrently using tokio
    let start = std::time::Instant::now();
    let handles: Vec<_> = steps
        .into_iter()
        .map(|step| {
            let framework = framework.clone();
            let task_context = task_context.clone();
            tokio::spawn(async move { framework.execute_single_step(&step, &task_context).await })
        })
        .collect();

    // Wait for all to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap().unwrap());
    }
    let duration = start.elapsed();

    // Verify all steps completed
    assert_eq!(results.len(), 3);
    for result in &results {
        assert_eq!(result.status, StepStatus::Completed);
    }

    // Verify concurrent execution (should be faster than sequential)
    // 3 steps with 50ms delay each = 150ms sequential, but concurrent should be ~50ms
    assert!(duration.as_millis() < 100);

    // Verify framework tracked all executions
    let state = framework.get_state();
    assert_eq!(state.executed_steps.len(), 3);
}

#[tokio::test]
async fn test_step_retry_on_failure() {
    let task_id = 789;

    // Create framework that always fails
    let failing_framework = MockFramework::new("retry_test").with_failure_rate(1.0); // Always fail
    failing_framework.add_task_context(task_id, json!({"retry_test": true}));

    // Create framework that always succeeds
    let succeeding_framework = MockFramework::new("retry_test");
    succeeding_framework.add_task_context(task_id, json!({"retry_test": true}));

    let step = create_viable_step(1, task_id, "retry_step");

    // First attempt (will fail)
    let task_context1 = failing_framework.get_task_context(task_id).await.unwrap();
    let result1 = failing_framework
        .execute_single_step(&step, &task_context1)
        .await
        .unwrap();

    assert_eq!(result1.status, StepStatus::Failed);
    assert!(result1.retry_after.is_some());
    assert_eq!(result1.error_code, Some("MOCK_RANDOM_FAILURE".to_string()));

    // Second attempt (will succeed)
    let task_context2 = succeeding_framework
        .get_task_context(task_id)
        .await
        .unwrap();
    let result2 = succeeding_framework
        .execute_single_step(&step, &task_context2)
        .await
        .unwrap();

    assert_eq!(result2.status, StepStatus::Completed);
    assert!(result2.error_message.is_none());
}

#[tokio::test]
async fn test_framework_event_publishing() {
    let framework = Arc::new(MockFramework::new("event_test"));
    framework.add_task_context(999, json!({"event_test": true}));

    // Publish various events
    use tasker_core::orchestration::types::OrchestrationEvent;

    let events = vec![
        OrchestrationEvent::TaskOrchestrationStarted {
            task_id: 999,
            framework: "event_test".to_string(),
            started_at: Utc::now(),
        },
        OrchestrationEvent::ViableStepsDiscovered {
            task_id: 999,
            step_count: 3,
            steps: vec![
                create_viable_step(1, 999, "step1"),
                create_viable_step(2, 999, "step2"),
                create_viable_step(3, 999, "step3"),
            ],
        },
        OrchestrationEvent::TaskOrchestrationCompleted {
            task_id: 999,
            result: TaskResult::Complete(tasker_core::orchestration::types::TaskCompletionInfo {
                task_id: 999,
                steps_executed: 3,
                total_execution_time_ms: 150,
                completed_at: Utc::now(),
                step_results: vec![],
            }),
            completed_at: Utc::now(),
        },
    ];

    for event in &events {
        framework.publish_event(event).await.unwrap();
    }

    // Verify all events were tracked
    let state = framework.get_state();
    assert_eq!(state.published_events.len(), 3);

    // Verify event types
    match &state.published_events[0] {
        OrchestrationEvent::TaskOrchestrationStarted { task_id, .. } => {
            assert_eq!(*task_id, 999);
        }
        _ => panic!("Unexpected event type"),
    }
}

#[tokio::test]
async fn test_framework_task_lifecycle() {
    let framework = Arc::new(MockFramework::new("lifecycle_test"));
    let task_id = 1234;

    framework.add_task_context(
        task_id,
        json!({
            "lifecycle": "test",
            "important": true
        }),
    );

    // Test task lifecycle methods

    // 1. Task start
    framework.on_task_start(task_id).await.unwrap();

    // 2. Enqueue task
    framework
        .enqueue_task(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    // 3. Update step states
    framework
        .update_step_state(1, "in_progress", None)
        .await
        .unwrap();

    framework
        .update_step_state(1, "completed", Some(&json!({"output": "success"})))
        .await
        .unwrap();

    // 4. Task complete
    let result = TaskResult::Complete(tasker_core::orchestration::types::TaskCompletionInfo {
        task_id,
        steps_executed: 1,
        total_execution_time_ms: 100,
        completed_at: Utc::now(),
        step_results: vec![],
    });

    framework.on_task_complete(task_id, &result).await.unwrap();

    // Verify tracked operations
    let state = framework.get_state();
    assert_eq!(state.enqueued_tasks.len(), 1);
    assert_eq!(state.enqueued_tasks[0].0, task_id);
    assert_eq!(state.step_state_updates.len(), 2);
    assert_eq!(state.step_state_updates[0].1, "in_progress");
    assert_eq!(state.step_state_updates[1].1, "completed");
}

#[tokio::test]
async fn test_framework_health_and_config() {
    let framework = Arc::new(MockFramework::new("health_test"));

    // Test health check
    assert!(framework.health_check().await.unwrap());

    framework.set_healthy(false);
    assert!(!framework.health_check().await.unwrap());

    framework.set_healthy(true);
    assert!(framework.health_check().await.unwrap());

    // Test configuration
    let max_retries = framework.get_config("max_retries").await.unwrap();
    assert_eq!(max_retries, Some(json!(3)));

    let timeout = framework.get_config("timeout_seconds").await.unwrap();
    assert_eq!(timeout, Some(json!(300)));

    let missing = framework.get_config("non_existent").await.unwrap();
    assert_eq!(missing, None);
}

#[tokio::test]
async fn test_framework_error_handling() {
    let framework = Arc::new(MockFramework::new("error_test"));

    // Test task not found
    let result = framework.get_task_context(9999).await;
    assert!(result.is_err());

    // Test marking task as failed
    let task_id = 5555;
    framework.add_task_context(task_id, json!({}));

    framework
        .mark_task_failed(task_id, "Critical error occurred")
        .await
        .unwrap();

    let state = framework.get_state();
    assert_eq!(state.failed_tasks.len(), 1);
    assert_eq!(state.failed_tasks[0].0, task_id);
    assert_eq!(state.failed_tasks[0].1, "Critical error occurred");
}

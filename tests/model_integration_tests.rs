mod common;

use common::*;
use tasker_core::models::{
    named_task::NamedTask,
    task::{Task, TaskForOrchestration},
    workflow_step::WorkflowStep,
    workflow_step_edge::{WorkflowStepEdge, NewWorkflowStepEdge},
    transitions::{TaskTransition, NewTaskTransition, WorkflowStepTransition, NewWorkflowStepTransition},
};
use serde_json::json;

/// Test comprehensive model relationships and workflows
#[tokio::test]
async fn test_complete_workflow_creation() {
    let test_db = TestDatabase::new().await;
    let pool = test_db.pool();
    
    // Run test logic
    let result: Result<(), sqlx::Error> = async {
        // Create a complete workflow hierarchy
        let namespace = TaskNamespaceBuilder::new()
            .with_name(&format!("integration_test_namespace_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)))
            .with_description("Integration test namespace")
            .build_in_tx(pool)
            .await;

        // Create named steps for the workflow
        let step1 = NamedStepBuilder::new()
            .with_name(&unique_name("fetch_data"))
            .with_handler_class("FetchDataHandler")
            .with_description("Fetch data from external API")
            .build_in_tx(pool)
            .await;

        let step2 = NamedStepBuilder::new()
            .with_name(&unique_name("process_data"))
            .with_handler_class("ProcessDataHandler")
            .with_description("Process the fetched data")
            .build_in_tx(pool)
            .await;

        let step3 = NamedStepBuilder::new()
            .with_name(&unique_name("store_data"))
            .with_handler_class("StoreDataHandler")
            .with_description("Store processed data")
            .build_in_tx(pool)
            .await;

        // Create named task
        let named_task = NamedTaskBuilder::new()
            .with_name("data_pipeline")
            .with_version(1)
            .with_namespace(namespace.clone())
            .with_description("Complete data processing pipeline")
            .build_in_tx(pool)
            .await;

        // Associate steps with the named task
        NamedTask::add_step(pool, named_task.named_task_id, step1.named_step_id).await?;
        NamedTask::add_step(pool, named_task.named_task_id, step2.named_step_id).await?;
        NamedTask::add_step(pool, named_task.named_task_id, step3.named_step_id).await?;

        // Create a task instance
        let task = TaskBuilder::new()
            .with_context(json!({
                "source_url": "https://api.example.com/data",
                "batch_id": "batch_001"
            }))
            .with_named_task(named_task.clone())
            .build_in_tx(pool)
            .await;

        // Create workflow steps with dependencies: step1 -> step2 -> step3
        let workflow_step1 = WorkflowStepBuilder::new()
            .with_context(json!({"timeout": 30}))
            .with_max_retries(3)
            .with_task(task.clone())
            .with_named_step(step1.clone())
            .build_in_tx(pool)
            .await;

        let workflow_step2 = WorkflowStepBuilder::new()
            .with_context(json!({"batch_size": 100}))
            .with_max_retries(2)
            .with_task(task.clone())
            .with_named_step(step2.clone())
            .build_in_tx(pool)
            .await;

        let workflow_step3 = WorkflowStepBuilder::new()
            .with_context(json!({"storage_type": "database"}))
            .with_max_retries(1)
            .with_task(task.clone())
            .with_named_step(step3.clone())
            .build_in_tx(pool)
            .await;

        // Create dependencies
        let _edge1 = WorkflowStepEdge::create(
            pool,
            NewWorkflowStepEdge {
                from_step_id: workflow_step1.workflow_step_id,
                to_step_id: workflow_step2.workflow_step_id,
            },
        ).await?;

        let _edge2 = WorkflowStepEdge::create(
            pool,
            NewWorkflowStepEdge {
                from_step_id: workflow_step2.workflow_step_id,
                to_step_id: workflow_step3.workflow_step_id,
            },
        ).await?;

        // Test delegation-ready queries
        let task_for_orchestration = Task::find_for_orchestration(pool, task.task_id)
            .await?
            .expect("Task for orchestration should exist");

        assert_eq!(task_for_orchestration.task_name, "data_pipeline");
        assert_eq!(task_for_orchestration.task_version, 1);
        assert!(task_for_orchestration.namespace_name.starts_with("integration_test_namespace"));

        // Test step names retrieval for delegation
        let step_names = NamedTask::get_step_names(pool, named_task.named_task_id).await?;
        assert_eq!(step_names.len(), 3);
        assert!(step_names.iter().any(|name| name.contains("fetch_data")));
        assert!(step_names.iter().any(|name| name.contains("process_data")));
        assert!(step_names.iter().any(|name| name.contains("store_data")));

        // Test DAG analysis
        let root_steps = WorkflowStepEdge::find_root_steps(pool, task.task_id).await?;
        assert_eq!(root_steps.len(), 1);
        assert_eq!(root_steps[0], workflow_step1.workflow_step_id);

        let leaf_steps = WorkflowStepEdge::find_leaf_steps(pool, task.task_id).await?;
        assert_eq!(leaf_steps.len(), 1);
        assert_eq!(leaf_steps[0], workflow_step3.workflow_step_id);

        // Test dependency queries
        let step2_deps = WorkflowStepEdge::find_dependencies(pool, workflow_step2.workflow_step_id).await?;
        assert_eq!(step2_deps.len(), 1);
        assert_eq!(step2_deps[0], workflow_step1.workflow_step_id);

        let step1_dependents = WorkflowStepEdge::find_dependents(pool, workflow_step1.workflow_step_id).await?;
        assert_eq!(step1_dependents.len(), 1);
        assert_eq!(step1_dependents[0], workflow_step2.workflow_step_id);

        // Test delegation methods
        assert!(task.get_task_identifier().starts_with("task:"));
        assert_eq!(step1.get_handler_class(), "FetchDataHandler");
        assert!(step1.get_step_identifier().contains("fetch_data"));
        assert_eq!(workflow_step1.get_step_identifier(), format!("step:{}", workflow_step1.workflow_step_id));

        // Test state management helpers
        assert!(task.is_active_state());
        assert!(!task.is_final_state());
        assert!(workflow_step1.can_retry());
        assert!(workflow_step1.is_ready_for_retry());

        Ok(())
    }.await;
    
    result.expect("Test should pass");
    test_db.close().await;
}

/// Test state transitions and audit trails
#[tokio::test]
async fn test_state_transitions_and_audit_trails() {
    let test_db = TestDatabase::new().await;
    let pool = test_db.pool();
    
    // Run test logic
    let result: Result<(), sqlx::Error> = async {
        // Create a simple task
        let task = TaskBuilder::new()
            .with_context(json!({"test": "state_transitions"}))
            .build_in_tx(pool)
            .await;

        let workflow_step = WorkflowStepBuilder::new()
            .with_task(task.clone())
            .build_in_tx(pool)
            .await;

        // Test task state transitions
        Task::update_state(pool, task.task_id, "running").await?;
        let updated_task = Task::find_by_id(pool, task.task_id).await?.unwrap();
        assert_eq!(updated_task.state, "running");

        // Create task transitions
        let _transition1 = TaskTransition::create(
            pool,
            NewTaskTransition {
                to_state: "running".to_string(),
                from_state: Some("created".to_string()),
                metadata: json!({"trigger": "manual"}),
                task_id: task.task_id,
            },
        ).await?;

        let _transition2 = TaskTransition::create(
            pool,
            NewTaskTransition {
                to_state: "complete".to_string(),
                from_state: Some("running".to_string()),
                metadata: json!({"duration_ms": 1500}),
                task_id: task.task_id,
            },
        ).await?;

        // Test most recent transition
        let most_recent = TaskTransition::get_most_recent(pool, task.task_id).await?.unwrap();
        assert_eq!(most_recent.to_state, "complete");
        assert_eq!(most_recent.from_state, Some("running".to_string()));
        assert!(most_recent.most_recent);

        // Test transition history
        let history = TaskTransition::get_history(pool, task.task_id).await?;
        assert_eq!(history.len(), 2);
        assert!(history[0].sort_key > history[1].sort_key); // Ordered by sort_key DESC

        // Test workflow step transitions
        WorkflowStep::update_state(pool, workflow_step.workflow_step_id, "running").await?;
        let _step_transition = WorkflowStepTransition::create(
            pool,
            NewWorkflowStepTransition {
                to_state: "running".to_string(),
                from_state: Some("created".to_string()),
                metadata: json!({"step_context": "execution_started"}),
                workflow_step_id: workflow_step.workflow_step_id,
            },
        ).await?;

        let step_most_recent = WorkflowStepTransition::get_most_recent(pool, workflow_step.workflow_step_id).await?.unwrap();
        assert_eq!(step_most_recent.to_state, "running");
        assert!(step_most_recent.most_recent);

        Ok(())
    }.await;
    
    result.expect("Test should pass");
    test_db.close().await;
}

/// Test DAG cycle detection
#[tokio::test]
async fn test_dag_cycle_detection() {
    let test_db = TestDatabase::new().await;
    let pool = test_db.pool();
    
    // Run test logic
    let result: Result<(), sqlx::Error> = async {
        // Create a task with 4 steps
        let task = TaskBuilder::new().build_in_tx(pool).await;
        let step1 = WorkflowStepBuilder::new().with_task(task.clone()).build_in_tx(pool).await;
        let step2 = WorkflowStepBuilder::new().with_task(task.clone()).build_in_tx(pool).await;
        let step3 = WorkflowStepBuilder::new().with_task(task.clone()).build_in_tx(pool).await;
        let step4 = WorkflowStepBuilder::new().with_task(task.clone()).build_in_tx(pool).await;

        // Create a valid DAG: step1 -> step2 -> step3
        WorkflowStepEdge::create(
            pool,
            NewWorkflowStepEdge {
                from_step_id: step1.workflow_step_id,
                to_step_id: step2.workflow_step_id,
            },
        ).await?;

        WorkflowStepEdge::create(
            pool,
            NewWorkflowStepEdge {
                from_step_id: step2.workflow_step_id,
                to_step_id: step3.workflow_step_id,
            },
        ).await?;

        // Test that adding step4 -> step1 doesn't create a cycle (new branch)
        let would_create_cycle = WorkflowStepEdge::would_create_cycle(
            pool,
            step4.workflow_step_id,
            step1.workflow_step_id,
        ).await?;
        assert!(!would_create_cycle);

        // Test that adding step3 -> step1 would create a cycle
        let would_create_cycle = WorkflowStepEdge::would_create_cycle(
            pool,
            step3.workflow_step_id,
            step1.workflow_step_id,
        ).await?;
        assert!(would_create_cycle);

        // Test that adding step3 -> step2 would create a cycle
        let would_create_cycle = WorkflowStepEdge::would_create_cycle(
            pool,
            step3.workflow_step_id,
            step2.workflow_step_id,
        ).await?;
        assert!(would_create_cycle);

        Ok(())
    }.await;
    
    result.expect("Test should pass");
    test_db.close().await;
}

/// Test delegation serialization patterns
#[tokio::test]
async fn test_delegation_serialization() {
    let test_db = TestDatabase::new().await;
    let pool = test_db.pool();
    
    // Run test logic
    let result: Result<(), Box<dyn std::error::Error>> = async {
        // Create a realistic task for delegation
        let namespace = TaskNamespaceBuilder::new()
            .with_name(&format!("api_namespace_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)))
            .build_in_tx(pool)
            .await;

        let named_task = NamedTaskBuilder::new()
            .with_name("api_workflow")
            .with_version(2)
            .with_namespace(namespace)
            .build_in_tx(pool)
            .await;

        let task = TaskBuilder::new()
            .with_context(json!({
                "user_id": 12345,
                "action": "process_payment",
                "amount": 99.99,
                "currency": "USD"
            }))
            .with_named_task(named_task)
            .build_in_tx(pool)
            .await;

        // Get task for orchestration (delegation-ready)
        let task_for_orchestration = Task::find_for_orchestration(pool, task.task_id)
            .await?
            .expect("Task should exist");

        // Test JSON serialization (for Ruby FFI)
        let json_serialized = serde_json::to_string_pretty(&task_for_orchestration)?;
        assert!(json_serialized.contains("api_workflow"));
        assert!(json_serialized.contains("api_namespace"));
        assert!(json_serialized.contains("process_payment"));

        // Test round-trip serialization
        let deserialized: TaskForOrchestration = serde_json::from_str(&json_serialized)?;
        assert_eq!(deserialized.task_name, task_for_orchestration.task_name);
        assert_eq!(deserialized.namespace_name, task_for_orchestration.namespace_name);
        assert_eq!(deserialized.task.context, task_for_orchestration.task.context);

        // Test that the format is stable for delegation (excluding dynamic fields)
        let mut test_data = task_for_orchestration.clone();
        // Normalize dynamic fields for consistent snapshots
        test_data.task.task_id = 1;
        test_data.task.named_task_id = 1;
        test_data.task.created_at = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc);
        test_data.task.updated_at = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc);
        test_data.namespace_name = "api_namespace".to_string();
        
        insta::assert_json_snapshot!(test_data, @r#"
        {
          "task": {
            "task_id": 1,
            "state": "created",
            "context": {
              "action": "process_payment",
              "amount": 99.99,
              "currency": "USD",
              "user_id": 12345
            },
            "most_recent_error_message": null,
            "most_recent_error_backtrace": null,
            "named_task_id": 1,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
          },
          "task_name": "api_workflow",
          "task_version": 2,
          "namespace_name": "api_namespace"
        }
        "#);

        Ok(())
    }.await;
    
    result.expect("Test should pass");
    test_db.close().await;
}

//! Tests for the query scopes system

use chrono::{Duration, Utc};
use sqlx::PgPool;
use tasker_shared::models::{
    NamedTask, Task, TaskNamespace, TaskTransition, WorkflowStep, WorkflowStepEdge,
    WorkflowStepTransition,
};
use tasker_shared::scopes::ScopeBuilder;
use tasker_shared::state_machine::{TaskState, WorkflowStepState};
use uuid::Uuid;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_scopes_basic(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test basic scope chaining
    let tasks = Task::scope()
        .in_namespace("default".to_string())
        .with_task_name("test_task".to_string())
        .limit(10)
        .all(&pool)
        .await?;

    // Should not error even with no data
    assert!(tasks.len() <= 10);
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_scope_by_current_state(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test state-based filtering
    let pending_tasks = Task::scope()
        .by_current_state(TaskState::Pending)
        .all(&pool)
        .await?;

    // Should not error - we're just testing that the query executes successfully
    let _ = pending_tasks.len();
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_scope_active(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test active task filtering
    let active_tasks = Task::scope().active().all(&pool).await?;

    // Should not error - we're just testing that the query executes successfully
    let _ = active_tasks.len();
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_scope_time_based(pool: PgPool) -> Result<(), sqlx::Error> {
    let yesterday = Utc::now() - Duration::hours(24);

    // Test time-based scopes
    let recent_tasks = Task::scope().created_since(yesterday).all(&pool).await?;

    let failed_tasks = Task::scope().failed_since(yesterday).all(&pool).await?;

    let completed_tasks = Task::scope().completed_since(yesterday).all(&pool).await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (
        recent_tasks.len(),
        failed_tasks.len(),
        completed_tasks.len(),
    );
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_scopes(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test workflow step scopes
    let failed_steps = WorkflowStep::scope()
        .failed()
        .failed_since(Utc::now() - Duration::hours(24))
        .all(&pool)
        .await?;

    let completed_steps = WorkflowStep::scope().completed().all(&pool).await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (failed_steps.len(), completed_steps.len());
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_scope_by_current_state(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test state-based filtering for workflow steps
    let pending_steps = WorkflowStep::scope()
        .by_current_state(WorkflowStepState::Pending)
        .all(&pool)
        .await?;

    // Should not error - we're just testing that the query executes successfully
    let _ = pending_steps.len();
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_scope_for_task(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test filtering steps by task ID (using a dummy ID)
    let steps_for_task = WorkflowStep::scope()
        .for_task(Uuid::now_v7()) // Non-existent task UUID
        .all(&pool)
        .await?;

    // Should return empty result but no error
    assert_eq!(steps_for_task.len(), 0);
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_scope_time_based(pool: PgPool) -> Result<(), sqlx::Error> {
    let yesterday = Utc::now() - Duration::hours(24);

    // Test time-based workflow step scopes
    let completed_since = WorkflowStep::scope()
        .completed_since(yesterday)
        .all(&pool)
        .await?;

    let failed_since = WorkflowStep::scope()
        .failed_since(yesterday)
        .all(&pool)
        .await?;

    let for_tasks_since = WorkflowStep::scope()
        .for_tasks_since(yesterday)
        .all(&pool)
        .await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (
        completed_since.len(),
        failed_since.len(),
        for_tasks_since.len(),
    );
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_transition_scopes(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test task transition scopes
    let recent_transitions = TaskTransition::scope()
        .to_state(TaskState::Pending)
        .recent()
        .all(&pool)
        .await?;

    let transitions_with_metadata = TaskTransition::scope()
        .with_metadata_key("test_key".to_string())
        .all(&pool)
        .await?;

    let transitions_for_task = TaskTransition::scope()
        .for_task(Uuid::now_v7()) // Non-existent task UUID
        .all(&pool)
        .await?;

    // Should not error - we're just testing that queries execute successfully
    let _ = (recent_transitions.len(), transitions_with_metadata.len());
    assert_eq!(transitions_for_task.len(), 0); // Should be empty for non-existent task
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_scope_count_and_exists(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test count and exists methods
    let active_count = Task::scope().active().count(&pool).await?;

    let active_exists = Task::scope().active().exists(&pool).await?;

    let step_count = WorkflowStep::scope().completed().count(&pool).await?;

    let step_exists = WorkflowStep::scope().completed().exists(&pool).await?;

    // Count should be >= 0, exists should be consistent
    assert!(active_count >= 0);
    assert_eq!(active_exists, active_count > 0);
    assert!(step_count >= 0);
    assert_eq!(step_exists, step_count > 0);
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_scope_chaining(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test complex scope chaining
    let complex_query = Task::scope()
        .active()
        .in_namespace("default".to_string())
        .created_since(Utc::now() - Duration::days(7))
        .order_by_created_at(false) // Most recent first
        .limit(5)
        .all(&pool)
        .await?;

    // Should not error and respect limit
    assert!(complex_query.len() <= 5);
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_scope_first_method(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test first() method
    let first_task = Task::scope().order_by_created_at(true).first(&pool).await?;

    let first_step = WorkflowStep::scope().first(&pool).await?;

    // Should not error (results can be None)
    assert!(first_task.is_some() || first_task.is_none());
    assert!(first_step.is_some() || first_step.is_none());
    Ok(())
}

// WorkflowStepEdge scope tests
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_edge_scopes_basic(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test basic WorkflowStepEdge scopes with empty data
    let children = WorkflowStepEdge::scope()
        .children_of(Uuid::now_v7()) // Non-existent step
        .all(&pool)
        .await?;

    let parents = WorkflowStepEdge::scope()
        .parents_of(Uuid::now_v7()) // Non-existent step
        .all(&pool)
        .await?;

    let provides_edges = WorkflowStepEdge::scope()
        .provides_edges()
        .all(&pool)
        .await?;

    // Should not error and return empty results
    assert_eq!(children.len(), 0);
    assert_eq!(parents.len(), 0);
    let _ = provides_edges.len(); // Could have data or not

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_edge_siblings_scope(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test the complex siblings_of CTE scope
    let siblings = WorkflowStepEdge::scope()
        .siblings_of(Uuid::now_v7()) // Non-existent step
        .all(&pool)
        .await?;

    // Should not error even with complex CTE
    assert_eq!(siblings.len(), 0);

    // Test count method with CTE
    let sibling_count = WorkflowStepEdge::scope()
        .siblings_of(Uuid::now_v7())
        .count(&pool)
        .await?;

    assert_eq!(sibling_count, 0);

    // Test exists method with CTE
    let siblings_exist = WorkflowStepEdge::scope()
        .siblings_of(Uuid::now_v7())
        .exists(&pool)
        .await?;

    assert!(!siblings_exist);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_edge_provides_to_children(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test the provides_to_children scope with subquery
    let provides_to_children = WorkflowStepEdge::scope()
        .provides_to_children(Uuid::now_v7()) // Non-existent step
        .all(&pool)
        .await?;

    // Should not error and return empty results
    assert_eq!(provides_to_children.len(), 0);

    // Test count and exists for this scope
    let count = WorkflowStepEdge::scope()
        .provides_to_children(Uuid::now_v7())
        .count(&pool)
        .await?;

    let exists = WorkflowStepEdge::scope()
        .provides_to_children(Uuid::now_v7())
        .exists(&pool)
        .await?;

    assert_eq!(count, 0);
    assert!(!exists);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_edge_scope_chaining(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test that basic scopes can be chained (though not very useful with current scopes)
    let edges = WorkflowStepEdge::scope()
        .provides_edges()
        .first(&pool)
        .await?;

    // Should not error
    assert!(edges.is_some() || edges.is_none());

    Ok(())
}

// WorkflowStepTransition scope tests
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_transition_scopes_basic(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test basic WorkflowStepTransition scopes with empty data
    let recent_transitions = WorkflowStepTransition::scope().recent().all(&pool).await?;

    let pending_transitions = WorkflowStepTransition::scope()
        .to_state("pending".to_string())
        .all(&pool)
        .await?;

    let metadata_transitions = WorkflowStepTransition::scope()
        .with_metadata_key("test_key".to_string())
        .all(&pool)
        .await?;

    let task_transitions = WorkflowStepTransition::scope()
        .for_task(Uuid::now_v7()) // Non-existent task
        .all(&pool)
        .await?;

    // Should not error and return empty results for non-existent data
    let _ = recent_transitions.len();
    let _ = pending_transitions.len();
    let _ = metadata_transitions.len();
    assert_eq!(task_transitions.len(), 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_transition_scope_chaining(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test chaining scopes together
    let chained_transitions = WorkflowStepTransition::scope()
        .to_state("complete".to_string())
        .recent()
        .all(&pool)
        .await?;

    let task_and_state = WorkflowStepTransition::scope()
        .for_task(Uuid::now_v7())
        .to_state("error".to_string())
        .all(&pool)
        .await?;

    // Should not error even with complex chaining
    let _ = chained_transitions.len();
    assert_eq!(task_and_state.len(), 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_transition_scope_count_and_exists(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    // Test count and exists methods for WorkflowStepTransition scopes
    let recent_count = WorkflowStepTransition::scope()
        .recent()
        .count(&pool)
        .await?;

    let recent_exists = WorkflowStepTransition::scope()
        .recent()
        .exists(&pool)
        .await?;

    let pending_count = WorkflowStepTransition::scope()
        .to_state("pending".to_string())
        .count(&pool)
        .await?;

    let pending_exists = WorkflowStepTransition::scope()
        .to_state("pending".to_string())
        .exists(&pool)
        .await?;

    // Count should be >= 0, exists should be consistent
    assert!(recent_count >= 0);
    assert_eq!(recent_exists, recent_count > 0);
    assert!(pending_count >= 0);
    assert_eq!(pending_exists, pending_count > 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_workflow_step_transition_for_task_scope(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test the for_task scope with JOIN
    let transitions_for_task = WorkflowStepTransition::scope()
        .for_task(Uuid::now_v7()) // Non-existent task UUID
        .all(&pool)
        .await?;

    // Should return empty result but no error
    assert_eq!(transitions_for_task.len(), 0);

    // Test count and exists with JOIN
    let count = WorkflowStepTransition::scope()
        .for_task(Uuid::now_v7())
        .count(&pool)
        .await?;

    let exists = WorkflowStepTransition::scope()
        .for_task(Uuid::now_v7())
        .exists(&pool)
        .await?;

    assert_eq!(count, 0);
    assert!(!exists);

    Ok(())
}

// NamedTask scope tests
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_scopes_basic(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test basic NamedTask scopes with empty data
    let namespace_tasks = NamedTask::scope()
        .in_namespace("default".to_string())
        .all(&pool)
        .await?;

    let version_tasks = NamedTask::scope()
        .with_version("1.0.0".to_string())
        .all(&pool)
        .await?;

    let latest_tasks = NamedTask::scope().latest_versions().all(&pool).await?;

    // Should not error and return appropriate results
    let _ = namespace_tasks.len();
    let _ = version_tasks.len();
    let _ = latest_tasks.len();

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_scope_chaining(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test chaining scopes together
    let chained_tasks = NamedTask::scope()
        .in_namespace("default".to_string())
        .with_version("1.0.0".to_string())
        .all(&pool)
        .await?;

    // Should not error even with complex chaining
    let _ = chained_tasks.len();

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_scope_count_and_exists(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test count and exists methods for NamedTask scopes
    let namespace_count = NamedTask::scope()
        .in_namespace("default".to_string())
        .count(&pool)
        .await?;

    let namespace_exists = NamedTask::scope()
        .in_namespace("default".to_string())
        .exists(&pool)
        .await?;

    let latest_count = NamedTask::scope().latest_versions().count(&pool).await?;

    let latest_exists = NamedTask::scope().latest_versions().exists(&pool).await?;

    // Count should be >= 0, exists should be consistent
    assert!(namespace_count >= 0);
    assert_eq!(namespace_exists, namespace_count > 0);
    assert!(latest_count >= 0);
    assert_eq!(latest_exists, latest_count > 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_task_latest_versions_scope(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test the complex latest_versions scope with DISTINCT ON
    let latest_tasks = NamedTask::scope().latest_versions().all(&pool).await?;

    // Should not error with complex DISTINCT ON query
    let _ = latest_tasks.len();

    // Test count and exists with DISTINCT ON
    let count = NamedTask::scope().latest_versions().count(&pool).await?;

    let exists = NamedTask::scope().latest_versions().exists(&pool).await?;

    assert!(count >= 0);
    assert_eq!(exists, count > 0);

    Ok(())
}

// TaskNamespace scope tests
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_namespace_custom_scope(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test the custom scope - excludes 'default' namespace
    let custom_namespaces = TaskNamespace::scope().custom().all(&pool).await?;

    // Should not error and return appropriate results
    let _ = custom_namespaces.len();

    // Verify none of the results are named 'default'
    for namespace in &custom_namespaces {
        assert_ne!(namespace.name, "default");
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_namespace_scope_count_and_exists(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test count and exists methods for TaskNamespace scopes
    let custom_count = TaskNamespace::scope().custom().count(&pool).await?;

    let custom_exists = TaskNamespace::scope().custom().exists(&pool).await?;

    // Count should be >= 0, exists should be consistent
    assert!(custom_count >= 0);
    assert_eq!(custom_exists, custom_count > 0);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_namespace_scope_basic(pool: PgPool) -> Result<(), sqlx::Error> {
    // Test basic TaskNamespace scope operations
    let all_namespaces = TaskNamespace::scope().all(&pool).await?;

    let first_namespace = TaskNamespace::scope().first(&pool).await?;

    // Should not error
    let _ = all_namespaces.len();
    assert!(first_namespace.is_some() || first_namespace.is_none());

    Ok(())
}

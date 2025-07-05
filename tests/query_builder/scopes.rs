use tasker_core::query_builder::{NamedTaskScopes, ScopeHelpers, TaskScopes, WorkflowStepScopes};

#[test]
fn test_task_by_current_state_scope() {
    let query = TaskScopes::by_current_state(Some("running"));
    let sql = query.build_sql();

    assert!(sql.contains("DISTINCT ON (task_id)"));
    assert!(sql.contains("tasker_task_transitions"));
    assert!(sql.contains("ORDER BY task_id, sort_key DESC"));
    assert!(sql.contains("current_transitions.to_state = 'running'"));
}

#[test]
fn test_active_tasks_scope() {
    let query = TaskScopes::active();
    let sql = query.build_sql();

    assert!(sql.contains("EXISTS"));
    assert!(sql.contains("tasker_workflow_steps ws"));
    assert!(sql.contains("tasker_workflow_step_transitions wst"));
    assert!(sql.contains("NOT IN ('complete', 'error', 'skipped', 'resolved_manually')"));
}

#[test]
fn test_workflow_step_pending_scope() {
    let query = WorkflowStepScopes::pending();
    let sql = query.build_sql();

    assert!(sql.contains("LEFT JOIN"));
    assert!(sql.contains("current_transitions.to_state IS NULL"));
    assert!(sql.contains("OR"));
}

#[test]
fn test_named_task_latest_versions_scope() {
    let query = NamedTaskScopes::latest_versions();
    let sql = query.build_sql();

    assert!(sql.contains("DISTINCT ON (task_namespace_id, name)"));
    assert!(sql.contains("ORDER BY task_namespace_id ASC, name ASC, version DESC"));
}

#[test]
fn test_ready_steps_complex_scope() {
    let query = ScopeHelpers::ready_steps_for_task(123);
    let sql = query.build_sql();

    assert!(sql.contains("total_dependencies"));
    assert!(sql.contains("completed_dependencies"));
    assert!(sql.contains("dependencies.total_dependencies = dependencies.completed_dependencies"));
}

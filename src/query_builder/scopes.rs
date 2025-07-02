use chrono::{DateTime, Utc};
use super::{QueryBuilder, WhereClause};

/// Task-specific query scopes that mirror Rails ActiveRecord scopes
pub struct TaskScopes;

impl TaskScopes {
    /// Create a new TaskScopes instance
    pub fn new() -> Self {
        Self
    }
    /// Find tasks by current state using transition table
    /// Equivalent to Rails: scope :by_current_state
    pub fn by_current_state(state: Option<&str>) -> QueryBuilder {
        let mut query = QueryBuilder::new("tasker_tasks")
            .inner_join(
                "(\
                    SELECT DISTINCT ON (task_id) task_id, to_state \
                    FROM tasker_task_transitions \
                    ORDER BY task_id, sort_key DESC\
                ) current_transitions",
                "current_transitions.task_id = tasker_tasks.task_id"
            );

        if let Some(state_value) = state {
            query = query.where_eq("current_transitions.to_state", serde_json::Value::String(state_value.to_string()));
        }

        query
    }

    /// Find active tasks (non-terminal states)
    /// Equivalent to Rails: scope :active
    pub fn active() -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .where_exists(
                "SELECT 1 FROM tasker_workflow_steps ws \
                 INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_id = ws.workflow_step_id \
                 WHERE ws.task_id = tasker_tasks.task_id \
                   AND wst.most_recent = true \
                   AND wst.to_state NOT IN ('complete', 'error', 'skipped', 'resolved_manually')"
            )
    }

    /// Find tasks by annotation
    /// Equivalent to Rails: scope :by_annotation
    pub fn by_annotation(annotation_name: &str, key: &str, value: &str) -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .inner_join("tasker_task_annotations", "tasker_task_annotations.task_id = tasker_tasks.task_id")
            .inner_join("tasker_annotation_types", "tasker_task_annotations.annotation_type_id = tasker_annotation_types.annotation_type_id")
            .where_eq("tasker_annotation_types.name", serde_json::Value::String(annotation_name.to_string()))
            .where_jsonb("tasker_task_annotations.annotation", "->>", serde_json::json!({key: value}))
    }

    /// Find tasks created since a specific time
    /// Equivalent to Rails: scope :created_since
    pub fn created_since(since_time: DateTime<Utc>) -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .where_clause(WhereClause::simple(
                "tasker_tasks.created_at", 
                ">", 
                serde_json::Value::String(since_time.to_rfc3339())
            ))
    }

    /// Find completed tasks since a specific time
    /// Equivalent to Rails: scope :completed_since
    pub fn completed_since(_since_time: DateTime<Utc>) -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .inner_join(
                "(\
                    SELECT DISTINCT task_id \
                    FROM tasker_workflow_step_transitions wst \
                    INNER JOIN tasker_workflow_steps ws ON wst.workflow_step_id = ws.workflow_step_id \
                    WHERE wst.to_state = 'complete' \
                      AND wst.created_at > $1 \
                      AND wst.most_recent = true\
                ) completed_tasks",
                "completed_tasks.task_id = tasker_tasks.task_id"
            )
    }

    /// Find failed tasks since a specific time
    /// Equivalent to Rails: scope :failed_since
    pub fn failed_since(_since_time: DateTime<Utc>) -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .inner_join(
                "(\
                    SELECT DISTINCT task_id \
                    FROM tasker_workflow_step_transitions wst \
                    INNER JOIN tasker_workflow_steps ws ON wst.workflow_step_id = ws.workflow_step_id \
                    WHERE wst.to_state = 'error' \
                      AND wst.created_at > $1 \
                      AND wst.most_recent = true\
                ) failed_tasks",
                "failed_tasks.task_id = tasker_tasks.task_id"
            )
    }

    /// Find tasks in a specific namespace
    /// Equivalent to Rails: scope :in_namespace
    pub fn in_namespace(namespace_name: &str) -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .inner_join("tasker_named_tasks", "tasker_tasks.named_task_id = tasker_named_tasks.named_task_id")
            .inner_join("tasker_task_namespaces", "tasker_named_tasks.task_namespace_id = tasker_task_namespaces.task_namespace_id")
            .where_eq("tasker_task_namespaces.name", serde_json::Value::String(namespace_name.to_string()))
    }

    /// Find tasks with a specific task name
    /// Equivalent to Rails: scope :with_task_name
    pub fn with_task_name(task_name: &str) -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .inner_join("tasker_named_tasks", "tasker_tasks.named_task_id = tasker_named_tasks.named_task_id")
            .where_eq("tasker_named_tasks.name", serde_json::Value::String(task_name.to_string()))
    }

    /// Find tasks with a specific version
    /// Equivalent to Rails: scope :with_version
    pub fn with_version(version: &str) -> QueryBuilder {
        QueryBuilder::new("tasker_tasks")
            .inner_join("tasker_named_tasks", "tasker_tasks.named_task_id = tasker_named_tasks.named_task_id")
            .where_eq("tasker_named_tasks.version", serde_json::Value::String(version.to_string()))
    }
}

/// WorkflowStep-specific query scopes
pub struct WorkflowStepScopes;

impl WorkflowStepScopes {
    /// Find workflow steps by current state using transition table
    /// Equivalent to Rails: scope :by_current_state
    pub fn by_current_state(state: Option<&str>) -> QueryBuilder {
        let mut query = QueryBuilder::new("tasker_workflow_steps")
            .inner_join(
                "(\
                    SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state \
                    FROM tasker_workflow_step_transitions \
                    WHERE most_recent = true \
                    ORDER BY workflow_step_id, sort_key DESC\
                ) current_transitions",
                "current_transitions.workflow_step_id = tasker_workflow_steps.workflow_step_id"
            );

        if let Some(state_value) = state {
            query = query.where_eq("current_transitions.to_state", serde_json::Value::String(state_value.to_string()));
        }

        query
    }

    /// Find completed workflow steps
    /// Equivalent to Rails: scope :completed
    pub fn completed() -> QueryBuilder {
        Self::by_current_state(Some("complete"))
    }

    /// Find failed workflow steps
    /// Equivalent to Rails: scope :failed
    pub fn failed() -> QueryBuilder {
        Self::by_current_state(Some("error"))
    }

    /// Find pending workflow steps (including missing transitions)
    /// Equivalent to Rails: scope :pending
    pub fn pending() -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_steps")
            .left_join(
                "(\
                    SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state \
                    FROM tasker_workflow_step_transitions \
                    WHERE most_recent = true \
                    ORDER BY workflow_step_id, sort_key DESC\
                ) current_transitions",
                "current_transitions.workflow_step_id = tasker_workflow_steps.workflow_step_id"
            )
            .where_clause(WhereClause::or(vec![
                super::conditions::Condition::IsNull {
                    field: "current_transitions.to_state".to_string(),
                },
                super::conditions::Condition::Simple {
                    field: "current_transitions.to_state".to_string(),
                    operator: "IN".to_string(),
                    value: serde_json::json!(["created", "pending", "retrying"]),
                },
            ]))
    }

    /// Find workflow steps completed since a specific time
    /// Equivalent to Rails: scope :completed_since
    pub fn completed_since(since_time: DateTime<Utc>) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_steps")
            .inner_join(
                "tasker_workflow_step_transitions wst",
                "wst.workflow_step_id = tasker_workflow_steps.workflow_step_id"
            )
            .where_eq("wst.to_state", serde_json::Value::String("complete".to_string()))
            .where_eq("wst.most_recent", serde_json::Value::Bool(true))
            .where_clause(WhereClause::simple(
                "wst.created_at",
                ">",
                serde_json::Value::String(since_time.to_rfc3339())
            ))
    }

    /// Find workflow steps failed since a specific time
    /// Equivalent to Rails: scope :failed_since
    pub fn failed_since(since_time: DateTime<Utc>) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_steps")
            .inner_join(
                "tasker_workflow_step_transitions wst",
                "wst.workflow_step_id = tasker_workflow_steps.workflow_step_id"
            )
            .where_eq("wst.to_state", serde_json::Value::String("error".to_string()))
            .where_eq("wst.most_recent", serde_json::Value::Bool(true))
            .where_clause(WhereClause::simple(
                "wst.created_at",
                ">",
                serde_json::Value::String(since_time.to_rfc3339())
            ))
    }

    /// Find workflow steps for tasks since a specific time
    /// Equivalent to Rails: scope :for_tasks_since
    pub fn for_tasks_since(since_time: DateTime<Utc>) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_steps")
            .inner_join("tasker_tasks", "tasker_workflow_steps.task_id = tasker_tasks.task_id")
            .where_clause(WhereClause::simple(
                "tasker_tasks.created_at",
                ">",
                serde_json::Value::String(since_time.to_rfc3339())
            ))
    }
}

/// NamedTask-specific query scopes
pub struct NamedTaskScopes;

impl NamedTaskScopes {
    /// Find named tasks in a specific namespace
    /// Equivalent to Rails: scope :in_namespace
    pub fn in_namespace(namespace_name: &str) -> QueryBuilder {
        QueryBuilder::new("tasker_named_tasks")
            .inner_join("tasker_task_namespaces", "tasker_named_tasks.task_namespace_id = tasker_task_namespaces.task_namespace_id")
            .where_eq("tasker_task_namespaces.name", serde_json::Value::String(namespace_name.to_string()))
    }

    /// Find named tasks with a specific version
    /// Equivalent to Rails: scope :with_version
    pub fn with_version(version: &str) -> QueryBuilder {
        QueryBuilder::new("tasker_named_tasks")
            .where_eq("version", serde_json::Value::String(version.to_string()))
    }

    /// Find latest versions of each named task
    /// Equivalent to Rails: scope :latest_versions
    pub fn latest_versions() -> QueryBuilder {
        QueryBuilder::new("tasker_named_tasks")
            .distinct_on(&["task_namespace_id", "name"])
            .order_by("task_namespace_id", "ASC")
            .order_by("name", "ASC")
            .order_desc("version")
    }
}

/// WorkflowStepEdge-specific query scopes
pub struct WorkflowStepEdgeScopes;

impl WorkflowStepEdgeScopes {
    /// Find children of a specific step
    /// Equivalent to Rails: scope :children_of
    pub fn children_of(step_id: i64) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_step_edges")
            .where_eq("from_step_id", serde_json::Value::Number(serde_json::Number::from(step_id)))
    }

    /// Find parents of a specific step
    /// Equivalent to Rails: scope :parents_of
    pub fn parents_of(step_id: i64) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_step_edges")
            .where_eq("to_step_id", serde_json::Value::Number(serde_json::Number::from(step_id)))
    }

    /// Find siblings of a specific step (steps with identical parent sets)
    /// Equivalent to Rails: scope :siblings_of
    pub fn siblings_of(step_id: i64) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_step_edges e1")
            .inner_join("tasker_workflow_step_edges e2", "e1.from_step_id = e2.from_step_id")
            .where_eq("e1.to_step_id", serde_json::Value::Number(serde_json::Number::from(step_id)))
            .where_clause(WhereClause::simple("e2.to_step_id", "!=", serde_json::Value::Number(serde_json::Number::from(step_id))))
            .group_by(&["e2.to_step_id"])
            .having_clause(WhereClause::raw(&format!(
                "COUNT(e2.from_step_id) = (SELECT COUNT(*) FROM tasker_workflow_step_edges WHERE to_step_id = {})",
                step_id
            )))
    }

    /// Find "provides" edges (special edge type)
    /// Equivalent to Rails: scope :provides_edges
    pub fn provides_edges() -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_step_edges")
            .where_eq("name", serde_json::Value::String("provides".to_string()))
    }
}

/// Utility functions for complex scope combinations
pub struct ScopeHelpers;

impl ScopeHelpers {
    /// Get task completion statistics efficiently
    /// Equivalent to Rails: WorkflowStep.task_completion_stats
    pub fn task_completion_stats(task_id: i64) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_steps ws")
            .left_join(
                "(\
                    SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state \
                    FROM tasker_workflow_step_transitions \
                    WHERE most_recent = true \
                    ORDER BY workflow_step_id, sort_key DESC\
                ) current_states",
                "current_states.workflow_step_id = ws.workflow_step_id"
            )
            .select(&[
                "COUNT(*) as total_steps",
                "COUNT(CASE WHEN current_states.to_state = 'complete' THEN 1 END) as completed_steps",
                "COUNT(CASE WHEN current_states.to_state = 'error' THEN 1 END) as failed_steps",
                "COUNT(CASE WHEN current_states.to_state IS NULL OR current_states.to_state IN ('created', 'pending') THEN 1 END) as pending_steps"
            ])
            .where_eq("ws.task_id", serde_json::Value::Number(serde_json::Number::from(task_id)))
    }

    /// Find ready steps for execution using DAG analysis
    /// High-performance equivalent to Rails step readiness calculation
    pub fn ready_steps_for_task(task_id: i64) -> QueryBuilder {
        QueryBuilder::new("tasker_workflow_steps ws")
            .left_join(
                "(\
                    SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state \
                    FROM tasker_workflow_step_transitions \
                    WHERE most_recent = true \
                    ORDER BY workflow_step_id, sort_key DESC\
                ) current_states",
                "current_states.workflow_step_id = ws.workflow_step_id"
            )
            .left_join(
                "(\
                    SELECT \
                        wse.to_step_id,\
                        COUNT(*) as total_dependencies,\
                        COUNT(CASE WHEN parent_states.to_state = 'complete' THEN 1 END) as completed_dependencies\
                    FROM tasker_workflow_step_edges wse\
                    INNER JOIN tasker_workflow_steps parent_ws ON wse.from_step_id = parent_ws.workflow_step_id\
                    LEFT JOIN (\
                        SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state \
                        FROM tasker_workflow_step_transitions \
                        WHERE most_recent = true \
                        ORDER BY workflow_step_id, sort_key DESC\
                    ) parent_states ON parent_states.workflow_step_id = parent_ws.workflow_step_id\
                    WHERE parent_ws.task_id = $1\
                    GROUP BY wse.to_step_id\
                ) dependencies",
                "dependencies.to_step_id = ws.workflow_step_id"
            )
            .where_eq("ws.task_id", serde_json::Value::Number(serde_json::Number::from(task_id)))
            .where_clause(WhereClause::or(vec![
                super::conditions::Condition::IsNull {
                    field: "current_states.to_state".to_string(),
                },
                super::conditions::Condition::Simple {
                    field: "current_states.to_state".to_string(),
                    operator: "IN".to_string(),
                    value: serde_json::json!(["created", "pending", "error"]),
                },
            ]))
            .where_clause(WhereClause::or(vec![
                super::conditions::Condition::IsNull {
                    field: "dependencies.total_dependencies".to_string(),
                },
                super::conditions::Condition::Raw {
                    sql: "dependencies.total_dependencies = dependencies.completed_dependencies".to_string(),
                },
            ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
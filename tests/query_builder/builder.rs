use tasker_core::query_builder::QueryBuilder;

#[test]
fn test_basic_query_building() {
    let query = QueryBuilder::new("tasker_tasks")
        .select(&["task_id", "context", "named_task_id"])
        .where_eq(
            "named_task_id",
            serde_json::Value::Number(serde_json::Number::from(1)),
        )
        .order_desc("created_at")
        .limit(10);

    let sql = query.build_sql();
    assert!(sql.contains("SELECT task_id, context, named_task_id"));
    assert!(sql.contains("FROM tasker_tasks"));
    assert!(sql.contains("ORDER BY created_at DESC"));
    assert!(sql.contains("LIMIT 10"));
}

#[test]
fn test_join_query_building() {
    let query = QueryBuilder::new("tasker_tasks t")
        .inner_join(
            "tasker_named_tasks nt",
            "t.named_task_id = nt.named_task_id",
        )
        .left_join(
            "tasker_task_namespaces tn",
            "nt.task_namespace_id = tn.task_namespace_id",
        )
        .where_eq("tn.name", serde_json::Value::String("default".to_string()));

    let sql = query.build_sql();
    assert!(sql.contains("INNER JOIN tasker_named_tasks nt"));
    assert!(sql.contains("LEFT JOIN tasker_task_namespaces tn"));
}

#[test]
fn test_cte_query_building() {
    let query = QueryBuilder::new("current_transitions")
        .with_cte(
            "current_transitions",
            "SELECT DISTINCT ON (task_id) task_id, to_state FROM tasker_task_transitions ORDER BY task_id, sort_key DESC"
        )
        .inner_join("tasker_tasks t", "current_transitions.task_id = t.task_id");

    let sql = query.build_sql();
    assert!(sql.contains("WITH current_transitions AS"));
    assert!(sql.contains("DISTINCT ON (task_id)"));
}

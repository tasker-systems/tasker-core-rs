use chrono::Utc;
use tasker_core::{QueryBuilder, TaskScopes, WorkflowStepScopes, ScopeHelpers};

/// Demonstrates the advanced query builder capabilities
/// 
/// This example shows how the Rust query builder provides equivalent functionality
/// to Rails ActiveRecord scopes with PostgreSQL-specific optimizations.
fn main() {
    println!("=== Tasker Core RS Query Builder Demo ===\n");

    // Example 1: Basic query building
    println!("1. Basic Query Building:");
    let basic_query = QueryBuilder::new("tasker_tasks")
        .select(&["task_id", "context", "named_task_id"])
        .where_eq("named_task_id", serde_json::Value::Number(serde_json::Number::from(1)))
        .order_desc("created_at")
        .limit(10);
    
    println!("SQL: {}\n", basic_query.build_sql());

    // Example 2: Complex JOIN with window functions
    println!("2. Complex JOIN with Window Functions (ActiveRecord equivalent):");
    let window_query = TaskScopes::by_current_state(Some("running"));
    println!("SQL: {}\n", window_query.build_sql());

    // Example 3: EXISTS subquery for active tasks
    println!("3. EXISTS Subquery for Active Tasks:");
    let active_query = TaskScopes::active();
    println!("SQL: {}\n", active_query.build_sql());

    // Example 4: Complex annotations query
    println!("4. JSONB Annotation Query:");
    let annotation_query = TaskScopes::by_annotation("error_tracking", "error_code", "404");
    println!("SQL: {}\n", annotation_query.build_sql());

    // Example 5: Recursive CTE for DAG traversal
    println!("5. Recursive CTE for Cycle Detection:");
    let cycle_query = QueryBuilder::new("step_path")
        .with_recursive_cte(
            "step_path",
            "SELECT from_step_id, to_step_id, 1 as depth FROM tasker_workflow_step_edges WHERE from_step_id = $1",
            "SELECT sp.from_step_id, wse.to_step_id, sp.depth + 1 FROM step_path sp JOIN tasker_workflow_step_edges wse ON sp.to_step_id = wse.from_step_id WHERE sp.depth < 100"
        )
        .select(&["COUNT(*) as count"])
        .where_eq("to_step_id", serde_json::Value::Number(serde_json::Number::from(42)));
    
    println!("SQL: {}\n", cycle_query.build_sql());

    // Example 6: DISTINCT ON for latest versions
    println!("6. DISTINCT ON for Latest Versions:");
    let latest_query = QueryBuilder::new("tasker_named_tasks")
        .distinct_on(&["task_namespace_id", "name"])
        .order_by("task_namespace_id", "ASC")
        .order_by("name", "ASC")
        .order_desc("version");
    
    println!("SQL: {}\n", latest_query.build_sql());

    // Example 7: Complex step readiness calculation
    println!("7. High-Performance Step Readiness Calculation:");
    let readiness_query = ScopeHelpers::ready_steps_for_task(123);
    println!("SQL: {}\n", readiness_query.build_sql());

    // Example 8: Workflow step scopes
    println!("8. Workflow Step Pending Scope:");
    let pending_query = WorkflowStepScopes::pending();
    println!("SQL: {}\n", pending_query.build_sql());

    // Example 9: Combined scopes - Active tasks in namespace from last 24 hours
    println!("9. Combined Scopes - Complex Business Logic:");
    let since_time = Utc::now() - chrono::Duration::hours(24);
    let combined_query = TaskScopes::active()
        .inner_join("tasker_named_tasks", "tasker_tasks.named_task_id = tasker_named_tasks.named_task_id")
        .inner_join("tasker_task_namespaces", "tasker_named_tasks.task_namespace_id = tasker_task_namespaces.task_namespace_id")
        .where_eq("tasker_task_namespaces.name", serde_json::Value::String("production".to_string()))
        .where_clause(tasker_core::query_builder::WhereClause::simple(
            "tasker_tasks.created_at",
            ">",
            serde_json::Value::String(since_time.to_rfc3339())
        ))
        .order_desc("tasker_tasks.created_at")
        .limit(50);
    
    println!("SQL: {}\n", combined_query.build_sql());

    // Example 10: JSONB operations
    println!("10. Advanced JSONB Operations:");
    let jsonb_query = QueryBuilder::new("tasker_tasks")
        .where_jsonb("context", "@>", serde_json::json!({"environment": "production"}))
        .where_jsonb("context", "?", serde_json::Value::String("user_id".to_string()))
        .where_jsonb("context", "->>", serde_json::json!({"status": "active"}));
    
    println!("SQL: {}\n", jsonb_query.build_sql());

    println!("=== Key Features Demonstrated ===");
    println!("✅ Complex JOIN operations with subqueries");
    println!("✅ Window functions (DISTINCT ON)");
    println!("✅ EXISTS/NOT EXISTS subqueries");
    println!("✅ Recursive CTEs for DAG operations");
    println!("✅ JSONB query operations (@>, ?, ->>, etc.)");
    println!("✅ Pagination and ordering");
    println!("✅ ActiveRecord scope equivalents");
    println!("✅ High-performance dependency resolution");
    println!("✅ Type-safe query building");
    println!("✅ Composable and reusable scopes");
}
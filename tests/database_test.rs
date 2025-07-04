use sqlx::{PgPool, Row};

#[sqlx::test]
async fn test_database_connection(pool: PgPool) -> sqlx::Result<()> {
    // Test basic database connectivity
    let row = sqlx::query("SELECT 1 as test").fetch_one(&pool).await?;

    let test_value: i32 = row.get("test");
    assert_eq!(test_value, 1);

    Ok(())
}

#[sqlx::test]
async fn test_database_schema_exists(pool: PgPool) -> sqlx::Result<()> {
    // Test that our core tables exist
    let tables = vec![
        "tasker_task_namespaces",
        "tasker_named_tasks",
        "tasker_named_steps",
        "tasker_tasks",
        "tasker_workflow_steps",
        "tasker_workflow_step_edges",
    ];

    for table in tables {
        let query = format!(
            "SELECT COUNT(*) as count FROM information_schema.tables WHERE table_name = '{}' AND table_schema = 'public'",
            table
        );

        let row = sqlx::query(&query)
            .fetch_one(&pool)
            .await
            .expect(&format!("Failed to query for table {}", table));

        let count: i64 = row.get("count");
        assert_eq!(count, 1, "Table {} does not exist", table);
    }

    Ok(())
}

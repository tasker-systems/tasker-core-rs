use sqlx::{PgPool, Row};

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_database_connection(pool: PgPool) -> sqlx::Result<()> {
    // Test basic database connectivity
    let row = sqlx::query("SELECT 1 as test").fetch_one(&pool).await?;

    let test_value: i32 = row.get("test");
    assert_eq!(test_value, 1);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_database_schema_exists(pool: PgPool) -> sqlx::Result<()> {
    // Test that our core tables exist in the tasker schema
    let tables = vec![
        "task_namespaces",
        "named_tasks",
        "named_steps",
        "tasks",
        "workflow_steps",
        "workflow_step_edges",
    ];

    for table in tables {
        let query = format!(
            "SELECT COUNT(*) as count FROM information_schema.tables WHERE table_name = '{table}' AND table_schema = 'tasker'"
        );

        let row = sqlx::query(&query)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|_| panic!("Failed to query for table tasker.{table}"));

        let count: i64 = row.get("count");
        assert_eq!(count, 1, "Table tasker.{table} does not exist");
    }

    Ok(())
}

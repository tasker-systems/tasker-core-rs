//! Database Pool and Migration Infrastructure Tests
//!
//! Tests for verifying database connectivity, the MIGRATOR applies cleanly,
//! and basic pool health on a fresh database provided by sqlx::test.

use sqlx::{PgPool, Row};

/// Test that the pool provided by sqlx::test is functional and can execute
/// a simple query. This validates that the MIGRATOR was applied successfully
/// and the pool is in a healthy state.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_pool_connectivity(pool: PgPool) -> sqlx::Result<()> {
    // Run a trivial query to verify the pool is connected
    let row = sqlx::query("SELECT 1 + 1 AS result")
        .fetch_one(&pool)
        .await?;
    let result: i32 = row.get("result");
    assert_eq!(result, 2, "Simple query should return 2");

    // Verify the tasker schema exists (created by migrations)
    let schema_row = sqlx::query(
        "SELECT COUNT(*) AS count FROM information_schema.schemata WHERE schema_name = 'tasker'",
    )
    .fetch_one(&pool)
    .await?;
    let schema_count: i64 = schema_row.get("count");
    assert_eq!(
        schema_count, 1,
        "The tasker schema should exist after migrations"
    );

    Ok(())
}

/// Test that the MIGRATOR applies cleanly by verifying critical tables
/// exist in the tasker schema after migration. This is effectively a
/// migration smoke test.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_migrator_applies_cleanly(pool: PgPool) -> sqlx::Result<()> {
    // Verify all critical tables required by the state machine and persistence
    // layer are present after migrations
    let critical_tables = vec![
        "tasks",
        "task_transitions",
        "workflow_steps",
        "workflow_step_transitions",
        "named_tasks",
        "named_steps",
        "task_namespaces",
    ];

    for table_name in &critical_tables {
        let row = sqlx::query(
            "SELECT COUNT(*) AS count FROM information_schema.tables \
             WHERE table_schema = 'tasker' AND table_name = $1",
        )
        .bind(table_name)
        .fetch_one(&pool)
        .await?;

        let count: i64 = row.get("count");
        assert_eq!(
            count, 1,
            "Table tasker.{table_name} should exist after migrations"
        );
    }

    // Verify the transition tables have the expected columns
    let transition_columns = sqlx::query(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'tasker' AND table_name = 'task_transitions' \
         ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await?;

    let column_names: Vec<String> = transition_columns
        .iter()
        .map(|r| r.get::<String, _>("column_name"))
        .collect();

    // Verify critical columns exist
    assert!(
        column_names.contains(&"task_uuid".to_string()),
        "task_transitions should have task_uuid column"
    );
    assert!(
        column_names.contains(&"to_state".to_string()),
        "task_transitions should have to_state column"
    );
    assert!(
        column_names.contains(&"sort_key".to_string()),
        "task_transitions should have sort_key column"
    );
    assert!(
        column_names.contains(&"most_recent".to_string()),
        "task_transitions should have most_recent column"
    );
    assert!(
        column_names.contains(&"metadata".to_string()),
        "task_transitions should have metadata column"
    );

    Ok(())
}

/// Test that the pool can handle multiple concurrent queries without issues,
/// simulating basic health check behavior.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_pool_health_check(pool: PgPool) -> sqlx::Result<()> {
    // Execute several queries concurrently to verify pool health
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let pool = pool.clone();
            tokio::spawn(async move {
                let row = sqlx::query("SELECT $1::int AS val")
                    .bind(i)
                    .fetch_one(&pool)
                    .await
                    .expect("concurrent query should succeed");
                let val: i32 = row.get("val");
                assert_eq!(val, i, "Concurrent query should return correct value");
            })
        })
        .collect();

    for handle in handles {
        handle.await.expect("task should complete");
    }

    // Verify pool is still healthy after concurrent usage
    let row = sqlx::query("SELECT current_timestamp AS ts")
        .fetch_one(&pool)
        .await?;
    // current_timestamp returns TIMESTAMP WITH TIME ZONE, which maps to DateTime<Utc>
    assert!(
        row.try_get::<chrono::DateTime<chrono::Utc>, _>("ts")
            .is_ok(),
        "Pool should still be healthy after concurrent usage"
    );

    Ok(())
}

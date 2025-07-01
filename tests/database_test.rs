use tasker_core::database::DatabaseConnection;
use sqlx::Row;

#[tokio::test]
async fn test_database_connection() {
    let db = DatabaseConnection::new().await.expect("Failed to connect to database");
    
    let health = db.health_check().await.expect("Failed to check database health");
    assert!(health, "Database health check failed");
    
    db.close().await;
}

#[tokio::test]
async fn test_database_schema_exists() {
    let db = DatabaseConnection::new().await.expect("Failed to connect to database");
    
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
            .fetch_one(db.pool())
            .await
            .expect(&format!("Failed to query for table {}", table));
        
        let count: i64 = row.get("count");
        assert_eq!(count, 1, "Table {} does not exist", table);
    }
    
    db.close().await;
}
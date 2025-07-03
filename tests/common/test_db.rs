use sqlx::{PgPool, Postgres, Transaction};
use std::future::Future;
use std::pin::Pin;
use tokio::sync::OnceCell;
use tasker_core::database::DatabaseMigrations;

static MIGRATION_INIT: OnceCell<()> = OnceCell::const_new();

/// Test database helper with automatic transaction rollback
pub struct TestDatabase {
    pool: PgPool,
}

impl TestDatabase {
    /// Create a new test database connection with proper migration setup
    pub async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

        let pool = PgPool::connect(&database_url).await
            .expect("Failed to connect to test database");

        // Ensure migrations are run once per test session
        Self::ensure_migrations(&pool).await;

        Self { pool }
    }

    /// Ensure database migrations are properly set up for tests
    async fn ensure_migrations(pool: &PgPool) {
        MIGRATION_INIT.get_or_init(|| async {
            // Force test environment
            std::env::set_var("DATABASE_URL", 
                std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string())
            );
            
            DatabaseMigrations::run_all(pool).await
                .expect("Failed to run test database migrations");
        }).await;
    }

    /// Get the underlying pool for direct access
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Run a test within a transaction that automatically rolls back
    pub async fn with_transaction<F, R>(&self, test: F) -> R
    where
        F: for<'a> FnOnce(&'a mut Transaction<'static, Postgres>) -> Pin<Box<dyn Future<Output = R> + Send + 'a>>,
    {
        let mut tx = self.pool.begin().await.expect("Failed to start transaction");
        let result = test(&mut tx).await;
        tx.rollback().await.expect("Failed to rollback transaction");
        result
    }

    /// Close the database connection
    pub async fn close(self) {
        self.pool.close().await;
    }
}

/// Generate a unique name for test data
pub fn unique_name(prefix: &str) -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let thread_id = std::thread::current().id();
    let random: u32 = fastrand::u32(..);
    format!("{}_{}_{}_{:?}", prefix, timestamp, random, thread_id)
}

/// Helper macro for transactional tests
#[macro_export]
macro_rules! test_with_transaction {
    ($test_db:expr, $tx:ident, $body:expr) => {{
        $test_db.with_transaction(|$tx| {
            Box::pin(async move { $body })
        }).await
    }};
}

/// Helper macro for pool-based tests with transactional cleanup
#[macro_export]
macro_rules! test_with_pool {
    ($test_db:expr, $pool:ident, $body:expr) => {{
        let $pool = $test_db.pool();
        $test_db.with_transaction(|_tx| {
            let pool_ref = $pool;
            Box::pin(async move {
                let $pool = pool_ref;
                let result = $body;
                result
            })
        }).await
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_connection() {
        let test_db = TestDatabase::new().await;

        // Test that we can execute a simple query
        let result = sqlx::query!("SELECT 1 as one")
            .fetch_one(test_db.pool())
            .await
            .expect("Failed to execute test query");

        assert_eq!(result.one, Some(1));
        test_db.close().await;
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let test_db = TestDatabase::new().await;

        // Insert data in a transaction
        test_with_transaction!(test_db, tx, {
            sqlx::query!(
                "INSERT INTO tasker_task_namespaces (name, description) VALUES ($1, $2)",
                "test_rollback_namespace",
                "This should be rolled back"
            )
            .execute(&mut **tx)
            .await
            .expect("Failed to insert test data");
        });

        // Verify the data was rolled back
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_task_namespaces WHERE name = $1",
            "test_rollback_namespace"
        )
        .fetch_one(test_db.pool())
        .await
        .expect("Failed to check rollback")
        .count
        .unwrap_or(0);

        assert_eq!(count, 0, "Transaction was not properly rolled back");
        test_db.close().await;
    }
}

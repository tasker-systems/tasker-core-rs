//! # Shared Database Cleanup Operations
//!
//! Language-agnostic database cleanup operations that can be used across
//! Ruby, Python, Node.js, WASM, and JNI bindings. These operations handle
//! test database setup, migration management, and schema cleanup using
//! the shared handle-based architecture.

use crate::ffi::shared::errors::SharedFFIError;
use crate::ffi::shared::handles::SharedOrchestrationHandle;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::time::SystemTime;
use tracing::{debug, error, info};

/// **SHARED**: Database cleanup operations using handle-based architecture
///
/// This struct provides language-agnostic database cleanup operations
/// that use the shared orchestration handle's persistent database pool.
///
/// ## Benefits over Ruby-specific implementation:
/// - **Shared Pool**: Uses handle's persistent pool instead of creating temporary pools
/// - **No Pool Exhaustion**: Single pool shared across all operations
/// - **Multi-Language**: Can be used by Python, Node.js, WASM, JNI bindings
/// - **Handle-Based**: Consistent with the shared FFI architecture
#[derive(Clone)]
pub struct SharedDatabaseCleanup {
    handle: std::sync::Arc<SharedOrchestrationHandle>,
}

impl SharedDatabaseCleanup {
    /// Create new SharedDatabaseCleanup using existing handle
    pub fn new(handle: std::sync::Arc<SharedOrchestrationHandle>) -> Result<Self, SharedFFIError> {
        Ok(SharedDatabaseCleanup { handle })
    }

    /// Get reference to the handle's database pool
    pub fn database_pool(&self) -> &PgPool {
        self.handle.database_pool()
    }

    /// **SHARED**: Sequential database setup for test environments
    ///
    /// This function provides comprehensive database setup including:
    /// - Connection termination
    /// - Schema drop and recreation
    /// - Migration tracking table creation
    ///
    /// Uses the handle's persistent pool to avoid connection exhaustion.
    pub async fn sequential_database_setup(
        &self,
        database_url: &str,
    ) -> Result<Value, SharedFFIError> {
        info!(
            "🔧 Shared FFI: Starting sequential database setup for {}",
            database_url
        );

        let pool = self.database_pool();

        // Step 1: Terminate existing connections
        let terminate_result = sqlx::raw_sql(
            r"
            SELECT pg_terminate_backend(pid)
            FROM pg_stat_activity
            WHERE datname = current_database()
            AND pid <> pg_backend_pid()
            AND state IN ('active', 'idle', 'idle in transaction', 'idle in transaction (aborted)')
            ",
        )
        .execute(pool)
        .await;

        if let Err(e) = terminate_result {
            debug!("Failed to terminate connections (continuing): {}", e);
        }

        // Step 2: Drop and recreate schema
        let schema_drop_result = sqlx::raw_sql(
            r"
            DROP SCHEMA public CASCADE;
            CREATE SCHEMA public;
            GRANT ALL ON SCHEMA public TO PUBLIC;
            ",
        )
        .execute(pool)
        .await;

        if let Err(e) = schema_drop_result {
            let error_msg = format!("Schema drop failed: {e}");
            error!("❌ Shared FFI: {}", error_msg);
            return Ok(json!({
                "status": "error",
                "error": error_msg,
                "error_type": "schema_drop_failure",
                "database_url": database_url,
                "step": "schema_drop"
            }));
        }

        // Step 3: Create migration tracking table
        let migration_result = sqlx::raw_sql(
            r"
            CREATE TABLE IF NOT EXISTS tasker_schema_migrations (
                version VARCHAR(14) PRIMARY KEY,
                applied_at TIMESTAMP WITHOUT TIME ZONE DEFAULT NOW()
            )
            ",
        )
        .execute(pool)
        .await;

        match migration_result {
            Ok(_) => {
                info!("✅ Shared FFI: Sequential database setup completed successfully");
                Ok(json!({
                    "status": "success",
                    "message": "Sequential database setup completed successfully",
                    "database_url": database_url,
                    "steps_completed": ["terminate_connections", "schema_drop", "migration_setup"],
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                    "handle_id": self.handle.handle_id,
                    "pool_connections": pool.size()
                }))
            }
            Err(e) => {
                let error_msg = format!("Migration setup failed: {e}");
                error!("❌ Shared FFI: {}", error_msg);
                Ok(json!({
                    "status": "error",
                    "error": error_msg,
                    "error_type": "migration_setup_failure",
                    "database_url": database_url,
                    "step": "migration_setup"
                }))
            }
        }
    }

    /// **SHARED**: Run database migrations using handle's pool
    pub async fn run_migrations(&self, database_url: &str) -> Result<Value, SharedFFIError> {
        info!(
            "🔧 Shared FFI: Running database migrations for {}",
            database_url
        );

        let pool = self.database_pool();

        // Check if we're in test environment
        let is_test_env = std::env::var("RAILS_ENV").unwrap_or_default() == "test"
            || std::env::var("APP_ENV").unwrap_or_default() == "test";

        if is_test_env {
            // Use sequential database setup for test environment
            self.sequential_database_setup(database_url).await
        } else {
            // For non-test environments, run normal migrations

            // Use the migrations from the core database module
            match tasker_shared::database::migrations::DatabaseMigrations::run_all(pool).await {
                Ok(()) => {
                    info!("✅ Shared FFI: DatabaseMigrations::run_all completed successfully");

                    // List tables after migration to verify schema
                    let table_query = r"
                        SELECT table_name, table_type
                        FROM information_schema.tables
                        WHERE table_schema = 'public'
                        ORDER BY table_name
                    ";

                    let mut tables = Vec::new();
                    match sqlx::query(table_query).fetch_all(pool).await {
                        Ok(rows) => {
                            for row in rows {
                                let table_name: String = row.get("table_name");
                                let table_type: String = row.get("table_type");
                                tables.push(json!({
                                    "name": table_name,
                                    "type": table_type
                                }));
                            }
                        }
                        Err(_e) => {}
                    }

                    Ok(json!({
                        "status": "success",
                        "message": "All migrations completed successfully",
                        "database_url": database_url,
                        "tables_created": tables,
                        "table_count": tables.len(),
                        "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                        "handle_id": self.handle.handle_id,
                        "pool_connections": pool.size()
                    }))
                }
                Err(e) => {
                    let error_details = format!("Migration failed: {e}");
                    error!("❌ Shared FFI: {}", error_details);

                    Ok(json!({
                        "status": "error",
                        "error": error_details,
                        "error_type": "migration_failure",
                        "database_url": database_url,
                        "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                        "handle_id": self.handle.handle_id,
                        "suggestions": [
                            "Check database connection parameters",
                            "Verify database exists and is accessible",
                            "Ensure user has migration permissions",
                            "Check if migrations have already been applied"
                        ]
                    }))
                }
            }
        }
    }

    /// **SHARED**: Drop database schema using handle's pool
    pub async fn drop_schema(&self, database_url: &str) -> Result<Value, SharedFFIError> {
        info!("🔧 Shared FFI: Starting schema drop for {}", database_url);

        let pool = self.database_pool();

        // Step 1: Terminate existing connections
        let terminate_result = sqlx::raw_sql(
            r"
            SELECT pg_terminate_backend(pid)
            FROM pg_stat_activity
            WHERE datname = current_database()
            AND pid <> pg_backend_pid()
            AND state IN ('active', 'idle', 'idle in transaction', 'idle in transaction (aborted)')
            ",
        )
        .execute(pool)
        .await;

        match terminate_result {
            Ok(_) => {}
            Err(e) => debug!("Failed to terminate connections (continuing): {}", e),
        }

        // Step 2: Drop and recreate schema
        let drop_result = sqlx::raw_sql(
            r"
            DROP SCHEMA public CASCADE;
            CREATE SCHEMA public;
            GRANT ALL ON SCHEMA public TO PUBLIC;
            ",
        )
        .execute(pool)
        .await;

        match drop_result {
            Ok(_) => {
                info!("✅ Shared FFI: Schema drop and recreate completed successfully");
                Ok(json!({
                    "status": "success",
                    "message": "Schema dropped and recreated successfully",
                    "database_url": database_url,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                    "handle_id": self.handle.handle_id,
                    "pool_connections": pool.size()
                }))
            }
            Err(e) => {
                let error_details = format!("Schema drop failed: {e}");
                error!("❌ Shared FFI: {}", error_details);
                Ok(json!({
                    "status": "error",
                    "error": error_details,
                    "error_type": "schema_drop_failure",
                    "database_url": database_url,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                    "handle_id": self.handle.handle_id,
                    "suggestions": [
                        "Check if database connection is still active",
                        "Verify user has DROP and CREATE schema permissions",
                        "Ensure no other processes are using the database"
                    ]
                }))
            }
        }
    }

    /// **SHARED**: List all tables in the database using handle's pool
    pub async fn list_database_tables(&self, database_url: &str) -> Result<Value, SharedFFIError> {
        let pool = self.database_pool();

        // Query to get all tables in the public schema
        let query = r"
            SELECT table_name, table_type
            FROM information_schema.tables
            WHERE table_schema = 'public'
            ORDER BY table_name
        ";

        match sqlx::query(query).fetch_all(pool).await {
            Ok(rows) => {
                let mut tables = Vec::new();

                for row in rows {
                    let table_name: String = row.get("table_name");
                    let table_type: String = row.get("table_type");
                    tables.push(json!({
                        "name": table_name,
                        "type": table_type
                    }));
                }

                Ok(json!({
                    "status": "success",
                    "database_url": database_url,
                    "table_count": tables.len(),
                    "tables": tables,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                    "handle_id": self.handle.handle_id,
                    "pool_connections": pool.size()
                }))
            }
            Err(e) => {
                let error_details = format!("Failed to list tables: {e}");
                error!("❌ Shared FFI: {}", error_details);

                Ok(json!({
                    "status": "error",
                    "error": error_details,
                    "error_type": "table_listing_failure",
                    "database_url": database_url,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                    "handle_id": self.handle.handle_id
                }))
            }
        }
    }
}

/// **SHARED**: Get global database cleanup instance using shared handle
///
/// This function provides access to database cleanup operations using
/// the shared orchestration handle, ensuring consistent pool usage.
pub fn get_global_database_cleanup() -> Result<SharedDatabaseCleanup, SharedFFIError> {
    let handle = super::handles::SharedOrchestrationHandle::get_global();
    SharedDatabaseCleanup::new(handle)
}

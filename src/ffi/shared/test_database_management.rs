//! # Comprehensive Test Database Management
//!
//! Environment-aware database management system for testing across all language bindings.
//! Provides setup/teardown operations for database schema, test data, and message queues.
//!
//! ## Key Features
//! - **Environment Detection**: Only operates in test environments (TASKER_ENV=test, RAILS_ENV=test)
//! - **Comprehensive Cleanup**: Tasks, steps, transitions, edges, named entities, queues
//! - **Queue Integration**: Manages both database and pgmq queue lifecycle
//! - **Cross-Language**: Available through embedded_bridge for Ruby, Python, etc.
//! - **Safe Defaults**: Fail-fast if not in test environment

use crate::ffi::shared::errors::SharedFFIError;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::time::SystemTime;
use tracing::{debug, error, info, warn};

/// Environment detection for test safety
#[derive(Debug, Clone, PartialEq)]
pub enum TestEnvironment {
    Test,
    Development,
    Production,
    Unknown,
}

impl TestEnvironment {
    /// Detect current environment from various environment variables
    pub fn detect() -> Self {
        // Check common test environment variables
        let rails_env = std::env::var("RAILS_ENV")
            .unwrap_or_default()
            .to_lowercase();
        let tasker_env = std::env::var("TASKER_ENV")
            .unwrap_or_default()
            .to_lowercase();
        let app_env = std::env::var("APP_ENV").unwrap_or_default().to_lowercase();
        let node_env = std::env::var("NODE_ENV").unwrap_or_default().to_lowercase();

        // Check for test environment indicators
        if rails_env == "test" || tasker_env == "test" || app_env == "test" || node_env == "test" {
            return Self::Test;
        }

        // Check for development environment
        if rails_env == "development"
            || tasker_env == "development"
            || app_env == "development"
            || node_env == "development"
        {
            return Self::Development;
        }

        // Check for production environment
        if rails_env == "production"
            || tasker_env == "production"
            || app_env == "production"
            || node_env == "production"
        {
            return Self::Production;
        }

        Self::Unknown
    }

    /// Check if current environment is safe for destructive operations
    pub fn is_test_safe(&self) -> bool {
        matches!(self, Self::Test)
    }
}

/// Migration status for idempotent setup
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub needs_migration: bool,
    pub current_version: String,
    pub existing_tables: u32,
    pub total_expected_tables: u32,
}

/// Comprehensive test database management operations
#[derive(Clone)]
pub struct TestDatabaseManager {
    pool: PgPool,
    environment: TestEnvironment,
}

impl TestDatabaseManager {
    /// Create new TestDatabaseManager with environment validation
    pub fn new(pool: PgPool) -> Result<Self, SharedFFIError> {
        let environment = TestEnvironment::detect();

        info!(
            "🧪 TestDatabaseManager: Detected environment: {:?}",
            environment
        );

        Ok(Self { pool, environment })
    }

    /// Ensure we're in a test environment before destructive operations
    fn ensure_test_environment(&self) -> Result<(), SharedFFIError> {
        if !self.environment.is_test_safe() {
            let error_msg = format!(
                "TestDatabaseManager destructive operations only allowed in test environment. Current: {:?}",
                self.environment
            );
            error!("🛑 {}", error_msg);
            return Err(SharedFFIError::EnvironmentNotSafe(error_msg));
        }
        Ok(())
    }

    /// **SETUP**: Complete test database setup with schema and initial data
    pub async fn setup_test_database(&self, database_url: &str) -> Result<Value, SharedFFIError> {
        self.ensure_test_environment()?;

        info!("🧪 Setting up test database for {}", database_url);

        // Step 1: Check if migrations are needed (idempotent setup)
        let migration_result = match self.check_migration_status().await {
            Ok(status) if status.needs_migration => {
                match crate::database::migrations::DatabaseMigrations::run_all(&self.pool).await {
                    Ok(()) => {
                        json!({ "migrations": "applied", "schema_version": status.current_version })
                    }
                    Err(e) => {
                        warn!("⚠️ Migration failed (schema may already exist): {}", e);
                        json!({ "migrations": "skipped", "reason": e.to_string(), "schema_version": status.current_version })
                    }
                }
            }
            Ok(status) => {
                json!({ "migrations": "up_to_date", "schema_version": status.current_version })
            }
            Err(e) => {
                warn!("⚠️ Could not check migration status: {}", e);
                json!({ "migrations": "unknown", "error": e })
            }
        };

        // Step 2: Initialize pgmq extension if not exists
        let pgmq_result = match sqlx::query("CREATE EXTENSION IF NOT EXISTS pgmq")
            .execute(&self.pool)
            .await
        {
            Ok(_) => {
                info!("✅ pgmq extension ready");

                // Step 2a: Create required queues for orchestration executors
                let required_queues = vec![
                    // Core orchestration queues
                    "orchestration_step_results", // For StepResultProcessor (from config)
                    "task_requests_queue",        // For TaskRequestProcessor (from config)
                    "task_processing_queue",      // Task processing queue
                    "batch_results_queue",        // Batch results queue
                    // Worker queues for workflow namespaces (matching YAML configs)
                    "linear_workflow_queue", // For linear_workflow namespace
                    "tree_workflow_queue",   // For tree_workflow namespace
                    "mixed_dag_workflow_queue", // For mixed_dag_workflow namespace
                    "diamond_workflow_queue", // For diamond_workflow namespace
                    "fulfillment_queue",     // For fulfillment namespace (order_fulfillment)
                    // Additional test queues
                    "test_inventory_queue",     // For test_inventory namespace
                    "test_notifications_queue", // For test_notifications namespace
                    // Default worker queue
                    "default_queue", // Default worker queue
                ];

                let mut queue_creation_results = Vec::new();
                for queue_name in required_queues {
                    match sqlx::query(&format!("SELECT pgmq.create('{queue_name}');"))
                        .execute(&self.pool)
                        .await
                    {
                        Ok(_) => {
                            debug!("✅ Created queue: {}", queue_name);
                            queue_creation_results
                                .push(json!({ "queue": queue_name, "status": "created" }));
                        }
                        Err(e) => {
                            // Queue might already exist, which is fine
                            debug!(
                                "⚠️ Queue creation for {} failed (may already exist): {}",
                                queue_name, e
                            );
                            queue_creation_results.push(json!({ "queue": queue_name, "status": "exists_or_failed", "message": e.to_string() }));
                        }
                    }
                }

                json!({
                    "pgmq_extension": "ready",
                    "queues_created": queue_creation_results
                })
            }
            Err(e) => {
                warn!("⚠️ pgmq extension setup issue (may already exist): {}", e);
                json!({ "pgmq_extension": "warning", "message": e.to_string() })
            }
        };

        // Step 3: Clean any existing test data (idempotent setup)
        let cleanup_result = self.cleanup_test_data().await?;

        let cleanup_queues_result = self.cleanup_test_queues().await?;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(json!({
            "status": "success",
            "message": "Test database setup completed successfully",
            "database_url": database_url,
            "environment": format!("{:?}", self.environment),
            "operations": {
                "migrations": migration_result,
                "pgmq_extension": pgmq_result,
                "cleanup": cleanup_result,
                "cleanup_queues": cleanup_queues_result
            },
            "timestamp": timestamp
        }))
    }

    /// **TEARDOWN**: Complete test database teardown
    pub async fn teardown_test_database(
        &self,
        database_url: &str,
    ) -> Result<Value, SharedFFIError> {
        self.ensure_test_environment()?;

        info!("🧪 Tearing down test database for {}", database_url);

        // Step 1: Clean all test data
        let data_cleanup = self.cleanup_test_data().await?;

        // Step 2: Clean all queues
        let queue_cleanup = self.cleanup_test_queues().await?;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(json!({
            "status": "success",
            "message": "Test database teardown completed successfully",
            "database_url": database_url,
            "environment": format!("{:?}", self.environment),
            "operations": {
                "data_cleanup": data_cleanup,
                "queue_cleanup": queue_cleanup
            },
            "timestamp": timestamp
        }))
    }

    /// **CLEANUP**: Comprehensive test data cleanup
    pub async fn cleanup_test_data(&self) -> Result<Value, SharedFFIError> {
        self.ensure_test_environment()?;

        info!("🧹 Cleaning up all test data");

        let mut cleanup_operations = Vec::new();
        let mut total_deleted = 0i64;

        // TRUNCATE with CASCADE handles foreign key constraints automatically
        // Order is less critical, but we'll keep a logical flow: data tables -> reference tables
        let tables_to_clean = vec![
            ("tasker_task_annotations", "task annotations"),
            ("tasker_workflow_step_edges", "workflow step edges"),
            (
                "tasker_workflow_step_transitions",
                "workflow step transitions",
            ),
            ("tasker_task_transitions", "task transitions"),
            ("tasker_workflow_steps", "workflow steps"),
            ("tasker_tasks", "tasks"),
            ("tasker_named_tasks_named_steps", "named tasks named steps"),
            ("tasker_named_tasks", "named tasks"),
            ("tasker_named_steps", "named steps"),
            ("tasker_task_namespaces", "task namespaces"),
            (
                "tasker_dependent_system_object_maps",
                "dependent system object maps",
            ),
            ("tasker_dependent_systems", "dependent systems"),
            ("tasker_annotation_types", "annotation types"),
        ];

        for (table_name, description) in tables_to_clean {
            match self.cleanup_table(table_name, description).await {
                Ok(count) => {
                    cleanup_operations.push(json!({ "table": table_name, "truncated": count }));
                    total_deleted += count;
                }
                Err(e) => {
                    cleanup_operations.push(json!({ "table": table_name, "error": e.to_string() }))
                }
            }
        }

        info!(
            "✅ Test data cleanup completed: {} total records deleted",
            total_deleted
        );

        Ok(json!({
            "status": "success",
            "message": "Test data cleanup completed",
            "total_deleted": total_deleted,
            "operations": cleanup_operations,
            "environment": format!("{:?}", self.environment)
        }))
    }

    /// **QUEUE CLEANUP**: Clear all messages from pgmq queues (don't delete queues)
    pub async fn cleanup_test_queues(&self) -> Result<Value, SharedFFIError> {
        self.ensure_test_environment()?;

        info!("🧹 Clearing all messages from pgmq queues");

        // List all existing queues
        let queue_list_query = "SELECT queue_name FROM pgmq.list_queues()";
        let queue_rows = sqlx::query(queue_list_query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                SharedFFIError::QueueOperationFailed(format!("Failed to list queues: {e}"))
            })?;

        let mut cleanup_operations = Vec::new();
        let mut total_queues_cleared = 0;
        let mut total_messages_deleted = 0i64;

        for row in queue_rows {
            let queue_name: String = row.get("queue_name");

            // Get message count before clearing
            let count_query = format!("SELECT COUNT(*) as count FROM pgmq.{queue_name}");
            let message_count = match sqlx::query(&count_query).fetch_one(&self.pool).await {
                Ok(row) => row.get::<i64, _>("count"),
                Err(_) => 0, // If count fails, assume 0 messages
            };

            // Clear all messages from the queue using direct SQL DELETE
            let delete_query = format!("DELETE FROM pgmq.q_{queue_name}");
            match sqlx::query(&delete_query).execute(&self.pool).await {
                Ok(_) => {
                    cleanup_operations.push(json!({
                        "queue": queue_name,
                        "status": "cleared",
                        "messages_deleted": message_count
                    }));
                    total_queues_cleared += 1;
                    total_messages_deleted += message_count;
                }
                Err(e) => {
                    warn!("⚠️ Failed to clear queue {}: {}", queue_name, e);
                    cleanup_operations.push(json!({
                        "queue": queue_name,
                        "status": "error",
                        "error": e.to_string()
                    }));
                }
            }
        }

        info!(
            "✅ Queue cleanup completed: {} queues cleared, {} messages deleted",
            total_queues_cleared, total_messages_deleted
        );

        Ok(json!({
            "status": "success",
            "message": "Queue cleanup completed - all messages cleared",
            "total_queues_cleared": total_queues_cleared,
            "total_messages_deleted": total_messages_deleted,
            "operations": cleanup_operations,
            "environment": format!("{:?}", self.environment)
        }))
    }

    /// Helper: Clean individual table with row count using TRUNCATE
    async fn cleanup_table(&self, table_name: &str, description: &str) -> Result<i64, String> {
        // First get count for reporting
        let count_query = format!("SELECT COUNT(*) as count FROM {table_name}");
        let row_count = match sqlx::query(&count_query).fetch_one(&self.pool).await {
            Ok(row) => row.get::<i64, _>("count"),
            Err(_) => 0, // If count fails, just proceed with truncate
        };

        // Use TRUNCATE for efficient cleanup (faster than DELETE and resets sequences)
        let truncate_query = format!("TRUNCATE TABLE {table_name} RESTART IDENTITY CASCADE");

        match sqlx::query(&truncate_query).execute(&self.pool).await {
            Ok(_) => Ok(row_count),
            Err(e) => {
                let error_msg = format!("Failed to truncate {description}: {e}");
                warn!("⚠️ {}", error_msg);
                Err(error_msg)
            }
        }
    }

    /// **UTILITY**: Get test database statistics
    pub async fn get_test_database_stats(&self) -> Result<Value, SharedFFIError> {
        info!("📊 Getting test database statistics");

        let tables = vec![
            "tasker_task_namespaces",
            "tasker_named_tasks",
            "tasker_named_steps",
            "tasker_tasks",
            "tasker_workflow_steps",
            "tasker_workflow_step_edges",
            "tasker_task_transitions",
            "tasker_workflow_step_transitions",
            "tasker_dependent_systems",
        ];

        let mut stats = Vec::new();
        let mut total_records = 0i64;

        for table in tables {
            let count_query = format!("SELECT COUNT(*) as count FROM {table}");

            match sqlx::query(&count_query).fetch_one(&self.pool).await {
                Ok(row) => {
                    let count: i64 = row.get("count");
                    stats.push(json!({
                        "table": table,
                        "count": count
                    }));
                    total_records += count;
                }
                Err(e) => {
                    stats.push(json!({
                        "table": table,
                        "error": e.to_string()
                    }));
                }
            }
        }

        // Get queue statistics
        let queue_stats = match self.get_queue_statistics().await {
            Ok(qs) => qs,
            Err(_) => json!({ "error": "Failed to get queue statistics" }),
        };

        Ok(json!({
            "status": "success",
            "environment": format!("{:?}", self.environment),
            "total_records": total_records,
            "table_stats": stats,
            "queue_stats": queue_stats,
            "timestamp": SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        }))
    }

    /// Helper: Check migration status to determine if migrations are needed
    async fn check_migration_status(&self) -> Result<MigrationStatus, String> {
        // Check if core tables exist to determine if schema is set up
        let core_tables = vec![
            "tasker_tasks",
            "tasker_workflow_steps",
            "tasker_task_namespaces",
            "tasker_named_tasks",
            "tasker_named_steps",
        ];

        let mut existing_tables = 0;
        for table in &core_tables {
            let check_query = format!(
                "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = '{table}' AND table_schema = 'public')"
            );

            match sqlx::query(&check_query).fetch_one(&self.pool).await {
                Ok(row) => {
                    if row.get::<bool, _>("exists") {
                        existing_tables += 1;
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to check table existence: {e}"));
                }
            }
        }

        // Get current migration version if migrations table exists
        let current_version = match sqlx::query(
            "SELECT version FROM tasker_schema_migrations ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        {
            Ok(Some(row)) => row.get::<String, _>("version"),
            Ok(None) => "none".to_string(),
            Err(_) => "no_migration_table".to_string(),
        };

        let needs_migration = existing_tables < core_tables.len();

        debug!(
            "Migration check: {}/{} core tables exist, needs_migration: {}",
            existing_tables,
            core_tables.len(),
            needs_migration
        );

        Ok(MigrationStatus {
            needs_migration,
            current_version,
            existing_tables: existing_tables as u32,
            total_expected_tables: core_tables.len() as u32,
        })
    }

    /// Helper: Get queue statistics
    async fn get_queue_statistics(&self) -> Result<Value, SharedFFIError> {
        let queue_list_query = "SELECT queue_name FROM pgmq.list_queues()";

        match sqlx::query(queue_list_query).fetch_all(&self.pool).await {
            Ok(rows) => {
                let queue_names: Vec<String> = rows
                    .iter()
                    .map(|row| row.get::<String, _>("queue_name"))
                    .collect();

                Ok(json!({
                    "total_queues": queue_names.len(),
                    "queue_names": queue_names
                }))
            }
            Err(e) => Ok(json!({
                "error": format!("Failed to get queue stats: {}", e)
            })),
        }
    }
}

/// Create TestDatabaseManager from existing database pool
pub fn create_test_database_manager(pool: PgPool) -> Result<TestDatabaseManager, SharedFFIError> {
    TestDatabaseManager::new(pool)
}

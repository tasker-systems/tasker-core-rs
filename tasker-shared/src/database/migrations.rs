//! # Database Migration System
//!
//! Simplified migration utilities that delegate to standard sqlx patterns.
//!
//! ## Recommended Usage
//!
//! For most use cases, use the standard sqlx approach:
//! ```rust
//! #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
//! async fn test_something(pool: PgPool) {
//!     // Test code here - database is automatically set up and cleaned up
//! }
//! ```
//!
//! For integration tests where you need manual control:
//! ```rust
//! let pool = tasker_shared::test_utils::setup_test_db().await;
//! // Test code here
//! ```
//!
//! This module provides compatibility wrappers for existing code that may
//! reference the old complex migration system, but delegates to standard sqlx patterns.

use sqlx::PgPool;

/// Simple migration status for connection startup checks
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub needs_migration: bool,
    pub current_version: String,
    pub existing_tables: u32,
    pub total_expected_tables: u32,
}

/// Simplified migration utilities that delegate to standard sqlx patterns
///
/// This is a compatibility wrapper around standard sqlx migration patterns.
/// For most cases, use `#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]`
/// or `tasker_shared::test_utils::setup_test_db()` instead.
pub struct DatabaseMigrations;

impl DatabaseMigrations {
    /// Run all migrations using standard sqlx migrator
    ///
    /// This delegates to the standard MIGRATOR from tasker_shared::test_utils.
    pub async fn run_all(pool: &PgPool) -> Result<(), sqlx::Error> {
        crate::test_utils::MIGRATOR
            .run(pool)
            .await
            .map_err(|e| sqlx::Error::Migrate(Box::new(e)))
    }

    /// Check migration status by examining core tables
    ///
    /// Returns a simple status indicating if migrations are needed.
    pub async fn check_status(pool: &PgPool) -> Result<MigrationStatus, sqlx::Error> {
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
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1 AND table_schema = 'public')"
            )
            .bind(table)
            .fetch_one(pool)
            .await?;

            if exists {
                existing_tables += 1;
            }
        }

        // Get current migration version from sqlx migrations table
        let current_version = match sqlx::query_scalar::<_, String>(
            "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        {
            Ok(Some(version)) => version,
            Ok(None) => "none".to_string(),
            Err(_) => "no_migration_table".to_string(),
        };

        let needs_migration = existing_tables < core_tables.len();

        Ok(MigrationStatus {
            needs_migration,
            current_version,
            existing_tables: existing_tables as u32,
            total_expected_tables: core_tables.len() as u32,
        })
    }
}

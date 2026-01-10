//! # Database Migration Support
//!
//! TAS-78: Provides migrators for both single-database and split-database deployments.
//!
//! ## Migration Directory Structure
//!
//! ```text
//! migrations/
//! ├── tasker/     # Tasker-specific migrations (tables, functions, views)
//! ├── pgmq/       # PGMQ-specific migrations (queue extensions, notification functions)
//! └── *.sql       # Combined migrations (for backward compatibility with sqlx::test)
//! ```
//!
//! ## Usage
//!
//! ### Single Database Mode (Development/Testing)
//! ```rust,ignore
//! // Uses MIGRATOR which contains all migrations
//! #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
//! async fn test_something(pool: PgPool) { /* ... */ }
//! ```
//!
//! ### Split Database Mode (Production)
//! ```rust,ignore
//! use tasker_shared::database::{migrator, DatabasePools};
//!
//! // Run Tasker migrations against Tasker pool
//! migrator::TASKER_MIGRATOR.run(&pools.tasker()).await?;
//!
//! // Run PGMQ migrations against PGMQ pool
//! migrator::PGMQ_MIGRATOR.run(&pools.pgmq()).await?;
//! ```

use crate::database::DatabasePools;
use sqlx::PgPool;
use tracing::info;

/// Combined migrator for single-database deployments and testing.
///
/// This migrator includes all migrations from the root `migrations/` directory.
/// Use this for:
/// - Development environments
/// - Testing with `#[sqlx::test]`
/// - Single-database production deployments
///
/// ## Example
/// ```rust,ignore
/// #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
/// async fn test_feature(pool: PgPool) { /* ... */ }
/// ```
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");

/// Tasker-specific migrator for split-database deployments.
///
/// Contains migrations for:
/// - Tasker tables (tasks, steps, transitions)
/// - SQL functions (readiness, execution context)
/// - Views (DAG relationships)
/// - Indexes and constraints
///
/// Run against the Tasker database pool in split-database mode.
pub static TASKER_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations/tasker");

/// PGMQ-specific migrator for split-database deployments.
///
/// Contains migrations for:
/// - PGMQ and pg_uuidv7 extensions
/// - Queue headers support functions
/// - Notification wrapper functions
/// - Event triggers
///
/// Run against the PGMQ database pool in split-database mode.
pub static PGMQ_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations/pgmq");

/// Result type for migration operations
pub type MigrationResult<T> = Result<T, sqlx::migrate::MigrateError>;

/// Run migrations against the appropriate database pools.
///
/// TAS-78: Handles both single-database and split-database deployments:
///
/// - **Single-database**: Runs combined migrations against the shared pool
/// - **Split-database**: Runs Tasker migrations against Tasker pool,
///   PGMQ migrations against PGMQ pool
///
/// # Arguments
/// * `pools` - Database pools (may be same pool for single-DB mode)
///
/// # Returns
/// Ok(()) on success, MigrateError on failure
///
/// # Example
/// ```rust,ignore
/// use tasker_shared::database::{migrator, DatabasePools};
///
/// let pools = DatabasePools::from_config(&config).await?;
/// migrator::run_migrations(&pools).await?;
/// ```
pub async fn run_migrations(pools: &DatabasePools) -> MigrationResult<()> {
    if pools.pgmq_is_separate() {
        info!("Running migrations in split-database mode");

        // Run PGMQ migrations first (PGMQ extension needed by Tasker queues)
        info!("Running PGMQ migrations against PGMQ pool...");
        PGMQ_MIGRATOR.run(pools.pgmq()).await?;
        info!("PGMQ migrations complete");

        // Run Tasker migrations
        info!("Running Tasker migrations against Tasker pool...");
        TASKER_MIGRATOR.run(pools.tasker()).await?;
        info!("Tasker migrations complete");
    } else {
        info!("Running migrations in single-database mode");

        // Single database - use combined migrator
        // The combined migrations include both PGMQ and Tasker objects
        MIGRATOR.run(pools.tasker()).await?;
        info!("Combined migrations complete");
    }

    Ok(())
}

/// Run Tasker migrations only (for explicit control).
///
/// Useful when you need to run migrations against a specific pool
/// without the automatic single/split database detection.
pub async fn run_tasker_migrations(pool: &PgPool) -> MigrationResult<()> {
    info!("Running Tasker-specific migrations...");
    TASKER_MIGRATOR.run(pool).await?;
    info!("Tasker migrations complete");
    Ok(())
}

/// Run PGMQ migrations only (for explicit control).
///
/// Useful when you need to run migrations against a specific pool
/// without the automatic single/split database detection.
pub async fn run_pgmq_migrations(pool: &PgPool) -> MigrationResult<()> {
    info!("Running PGMQ-specific migrations...");
    PGMQ_MIGRATOR.run(pool).await?;
    info!("PGMQ migrations complete");
    Ok(())
}

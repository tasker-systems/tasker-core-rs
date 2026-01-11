//! # Database Migration Support
//!
//! TAS-128: Unified migrator with split-database awareness.
//!
//! ## Architecture
//!
//! - **Compile-time / Tests**: Single `MIGRATOR` embeds all migrations from `migrations/`
//! - **Runtime**: `run_migrations()` detects split-database mode and routes appropriately
//!
//! ## Migration Naming Convention
//!
//! Migrations are identified by filename pattern:
//! - `*_pgmq_*.sql` → PGMQ-specific (extensions, queues, notifications)
//! - All others → Tasker-specific (tables, functions, views)
//!
//! ## Usage
//!
//! ### Testing (Single Database)
//! ```rust,ignore
//! #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
//! async fn test_something(pool: PgPool) { /* ... */ }
//! ```
//!
//! ### Production (Auto-detects Split Mode)
//! ```rust,ignore
//! use tasker_shared::database::{migrator, DatabasePools};
//!
//! let pools = DatabasePools::from_config(&config).await?;
//! migrator::run_migrations(&pools).await?;
//! ```

use crate::database::DatabasePools;
use sqlx::PgPool;
use tracing::info;

/// Primary migrator containing all migrations.
///
/// Use this for:
/// - All `#[sqlx::test]` tests
/// - SQLx compile-time query verification
/// - Single-database deployments
///
/// For split-database production deployments, use `run_migrations()` which
/// automatically routes migrations to the appropriate pools based on naming conventions.
///
/// ## Example
/// ```rust,ignore
/// #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
/// async fn test_feature(pool: PgPool) { /* ... */ }
/// ```
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");

/// Result type for migration operations
pub type MigrationResult<T> = Result<T, sqlx::migrate::MigrateError>;

/// Check if a migration is PGMQ-specific based on its name.
/// PGMQ migrations contain "pgmq" in their description/filename.
fn is_pgmq_migration(migration: &sqlx::migrate::Migration) -> bool {
    migration.description.to_lowercase().contains("pgmq")
}

/// Run migrations against the appropriate database pools.
///
/// Automatically detects single vs split-database mode:
///
/// - **Single-database**: Runs all migrations against the shared pool
/// - **Split-database**: Routes PGMQ migrations to PGMQ pool, Tasker migrations to Tasker pool
///   based on naming conventions (migrations with "pgmq" in the name go to PGMQ pool)
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

        // In split mode, we run PGMQ migrations first (needed for queue infrastructure),
        // then Tasker migrations. We filter based on naming convention.

        // Run PGMQ migrations against PGMQ pool
        info!("Running PGMQ migrations against PGMQ pool...");
        for migration in MIGRATOR.migrations.iter() {
            if is_pgmq_migration(migration) {
                info!("  Running PGMQ migration: {}", migration.description);
                run_single_migration(pools.pgmq(), migration).await?;
            }
        }
        info!("PGMQ migrations complete");

        // Run Tasker migrations against Tasker pool
        info!("Running Tasker migrations against Tasker pool...");
        for migration in MIGRATOR.migrations.iter() {
            if !is_pgmq_migration(migration) {
                info!("  Running Tasker migration: {}", migration.description);
                run_single_migration(pools.tasker(), migration).await?;
            }
        }
        info!("Tasker migrations complete");
    } else {
        info!("Running migrations in single-database mode");

        // Single database - use combined migrator directly
        MIGRATOR.run(pools.tasker()).await?;
        info!("Combined migrations complete");
    }

    Ok(())
}

/// Run a single migration against a pool.
/// This handles the migration tracking table and execution.
async fn run_single_migration(
    pool: &PgPool,
    migration: &sqlx::migrate::Migration,
) -> MigrationResult<()> {
    use sqlx::Executor;

    // Ensure migrations table exists
    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
            success BOOLEAN NOT NULL,
            checksum BYTEA NOT NULL,
            execution_time BIGINT NOT NULL
        )
        "#,
    )
    .await
    .map_err(sqlx::migrate::MigrateError::Execute)?;

    // Check if already applied
    let applied: Option<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE version = $1 AND success = true",
    )
    .bind(migration.version)
    .fetch_optional(pool)
    .await
    .map_err(sqlx::migrate::MigrateError::Execute)?;

    if applied.is_some() {
        info!("    Migration {} already applied, skipping", migration.version);
        return Ok(());
    }

    // Run the migration
    let start = std::time::Instant::now();
    pool.execute(migration.sql.as_ref())
        .await
        .map_err(sqlx::migrate::MigrateError::Execute)?;
    let elapsed = start.elapsed();

    // Record the migration
    sqlx::query(
        r#"
        INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
        VALUES ($1, $2, true, $3, $4)
        ON CONFLICT (version) DO UPDATE SET success = true, execution_time = $4
        "#,
    )
    .bind(migration.version)
    .bind(&*migration.description)
    .bind(&*migration.checksum)
    .bind(elapsed.as_nanos() as i64)
    .execute(pool)
    .await
    .map_err(sqlx::migrate::MigrateError::Execute)?;

    Ok(())
}

/// Run all migrations against a single pool.
///
/// Convenience function for simple single-database setups without DatabasePools.
///
/// # Example
/// ```rust,ignore
/// let pool = PgPool::connect(&database_url).await?;
/// migrator::run_all_migrations(&pool).await?;
/// ```
pub async fn run_all_migrations(pool: &PgPool) -> MigrationResult<()> {
    info!("Running all migrations against single pool...");
    MIGRATOR.run(pool).await?;
    info!("All migrations complete");
    Ok(())
}

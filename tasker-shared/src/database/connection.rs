use super::DatabaseMigrations;
use crate::config::ConfigManager;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::env;
use std::time::Duration;

pub struct DatabaseConnection {
    pool: PgPool,
}

impl DatabaseConnection {
    pub async fn new() -> Result<Self, sqlx::Error> {
        // Use global ConfigManager for backward compatibility
        let config_manager = ConfigManager::global().map_err(|e| {
            sqlx::Error::Configuration(format!("Failed to load configuration: {e}").into())
        })?;
        Self::new_with_config_manager(&config_manager).await
    }

    pub async fn new_with_config_manager(
        config_manager: &ConfigManager,
    ) -> Result<Self, sqlx::Error> {
        // Use provided ConfigManager for database configuration
        let database_url = config_manager.config().database_url();
        let database_config = &config_manager.config().database;

        // Create pool with structured configuration values
        let pool = PgPoolOptions::new()
            .max_connections(database_config.pool.max_connections)
            .min_connections(database_config.pool.min_connections)
            .acquire_timeout(Duration::from_secs(
                database_config.pool.acquire_timeout_seconds,
            ))
            .idle_timeout(Duration::from_secs(
                database_config.pool.idle_timeout_seconds,
            ))
            .max_lifetime(Duration::from_secs(
                database_config.pool.max_lifetime_seconds,
            ))
            .test_before_acquire(true) // Ensure connections are valid
            .connect(&database_url)
            .await?;

        // Check migration status (never auto-run migrations in production)
        // Migrations should be run separately via `cargo sqlx migrate run` or similar
        if !Self::should_skip_migration_check(config_manager) {
            match DatabaseMigrations::check_status(&pool).await {
                Ok(status) if status.needs_migration => {
                    return Err(sqlx::Error::Configuration(format!(
                        "Database schema is not up to date. Please run migrations before starting the system.\n\
                         Current schema: {}/{} core tables exist, version: {}\n\
                         Expected: All core tables present with latest migrations.\n\
                         \n\
                         To fix this:\n\
                         1. Run: cargo sqlx migrate run\n\
                         2. Or set SKIP_MIGRATION_CHECK=1 to bypass this check (not recommended for production)",
                        status.existing_tables, status.total_expected_tables, status.current_version
                    ).into()));
                }
                Ok(_) => {
                    // Migrations are up to date, continue
                    tracing::debug!("✅ Database schema is up to date");
                }
                Err(e) => {
                    return Err(sqlx::Error::Configuration(
                        format!(
                            "Could not check migration status: {e}.\n\
                         This may indicate a database connectivity issue.\n\
                         Use SKIP_MIGRATION_CHECK=1 to bypass this check if needed."
                        )
                        .into(),
                    ));
                }
            }
        }

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn health_check(&self) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT 1 as health")
            .fetch_one(&self.pool)
            .await?;

        let health: i32 = row.get("health");
        Ok(health == 1)
    }

    pub async fn close(self) {
        self.pool.close().await;
    }

    /// Determine if migration check should be skipped based on configuration
    fn should_skip_migration_check(config_manager: &ConfigManager) -> bool {
        // First check environment variable for backward compatibility
        if env::var("SKIP_MIGRATION_CHECK").is_ok() {
            return true;
        }

        // Check configuration setting
        config_manager.config().database.skip_migration_check
    }
}

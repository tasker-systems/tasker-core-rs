use super::{DatabaseMigrations, MigrationStatus};
use sqlx::{PgPool, Row};
use std::env;

pub struct DatabaseConnection {
    pool: PgPool,
}

impl DatabaseConnection {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://tasker:tasker@localhost/tasker_rust_development".to_string()
        });

        let pool = PgPool::connect(&database_url).await?;

        // Check migration status (never auto-run migrations in production)
        // Migrations should be run separately via `cargo sqlx migrate run` or similar
        if std::env::var("SKIP_MIGRATION_CHECK").is_err() {
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
                    return Err(sqlx::Error::Configuration(format!(
                        "Could not check migration status: {}.\n\
                         This may indicate a database connectivity issue.\n\
                         Use SKIP_MIGRATION_CHECK=1 to bypass this check if needed.",
                        e
                    ).into()));
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
}

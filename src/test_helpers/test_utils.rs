//! # Test Utilities
//!
//! Centralized utilities for testing that work both locally and in CI environments.
//! This module provides database setup and environment variable helpers that
//! check for existing environment variables before falling back to defaults.
//!
//! ## Migration Support
//!
//! Uses sqlx::migrate! macro for simple, reliable test database setup.

use dotenvy::dotenv;
use sqlx::PgPool;
use std::env;

/// Setup DATABASE_URL environment variable for tests
///
/// This function checks if DATABASE_URL is already set (e.g., in CI environments)
/// and only sets it to the local test default if it's not already present.
/// This ensures tests work both locally and in CI environments.
///
/// # Examples
///
/// ```rust
/// use tasker_core::setup_test_database_url;
/// use std::env;
///
/// // Remove any existing DATABASE_URL to test the fallback
/// env::remove_var("DATABASE_URL");
///
/// setup_test_database_url();
///
/// let db_url = env::var("DATABASE_URL").unwrap();
/// assert_eq!(db_url, "postgresql://tasker:tasker@localhost/tasker_rust_test");
/// ```
pub fn setup_test_database_url() {
    if env::var("DATABASE_URL").is_err() {
        env::set_var(
            "DATABASE_URL",
            "postgresql://tasker:tasker@localhost/tasker_rust_test",
        );
    }
}

/// Get database URL for tests with fallback
///
/// Returns the DATABASE_URL if set, otherwise returns the local test default.
/// This is safer than setting environment variables in concurrent test scenarios.
///
/// # Examples
///
/// ```rust
/// use tasker_core::get_test_database_url;
/// use std::env;
///
/// // Test fallback behavior when DATABASE_URL is not set
/// env::remove_var("DATABASE_URL");
/// let db_url = get_test_database_url();
/// assert_eq!(db_url, "postgresql://tasker:tasker@localhost/tasker_rust_test");
///
/// // Test that it respects existing environment variable
/// env::set_var("DATABASE_URL", "postgresql://custom:custom@localhost/custom_db");
/// let custom_url = get_test_database_url();
/// assert_eq!(custom_url, "postgresql://custom:custom@localhost/custom_db");
/// ```
pub fn get_test_database_url() -> String {
    env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string())
}

/// Setup test environment with all necessary environment variables
///
/// This function sets up common test environment variables if they're not
/// already present, making tests work in both local and CI environments.
///
/// # Examples
///
/// ```rust
/// use tasker_core::setup_test_environment;
/// use std::env;
///
/// // Clear environment for clean test
/// env::remove_var("DATABASE_URL");
/// env::remove_var("TASKER_ENV");
///
/// setup_test_environment();
///
/// // Verify both environment variables are set
/// assert_eq!(env::var("DATABASE_URL").unwrap(),
///            "postgresql://tasker:tasker@localhost/tasker_rust_test");
/// assert_eq!(env::var("TASKER_ENV").unwrap(), "test");
/// ```
pub fn setup_test_environment() {
    setup_test_database_url();

    // Set TASKER_ENV if not already set
    if env::var("TASKER_ENV").is_err() {
        env::set_var("TASKER_ENV", "test");
    }
}

/// Simple sqlx migrator using the workspace migrations directory
///
/// This replaces our complex custom migration system with sqlx's built-in support.
/// Use this in tests with: #[sqlx::test(migrator = "MIGRATOR")]
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Set up test database pool with migrations applied
///
/// Alternative to #[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")] macro for cases where you need manual setup.
pub async fn setup_test_db() -> PgPool {
    // Load .env file for tests
    dotenv().ok();

    let database_url = get_test_database_url();

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Run migrations using sqlx's built-in migration support
    MIGRATOR.run(&pool).await.expect("Failed to run migrations");

    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_functions_exist() {
        // Just test that the setup functions exist and don't crash
        // We can't easily test they work correctly without affecting other tests
        // since environment variables are shared across concurrent tests
        setup_test_database_url();
        setup_test_environment();
    }
}

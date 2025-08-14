//! # Test Utilities
//!
//! Centralized utilities for testing that work both locally and in CI environments.
//! This module provides database setup and environment variable helpers that
//! check for existing environment variables before falling back to defaults.

use std::env;

/// Setup DATABASE_URL environment variable for tests
///
/// This function checks if DATABASE_URL is already set (e.g., in CI environments)
/// and only sets it to the local test default if it's not already present.
/// This ensures tests work both locally and in CI environments.
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
pub fn get_test_database_url() -> String {
    env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string())
}

/// Setup test environment with all necessary environment variables
///
/// This function sets up common test environment variables if they're not
/// already present, making tests work in both local and CI environments.
pub fn setup_test_environment() {
    setup_test_database_url();

    // Set TASKER_ENV if not already set
    if env::var("TASKER_ENV").is_err() {
        env::set_var("TASKER_ENV", "test");
    }
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

    #[test]
    fn test_get_test_database_url_returns_string() {
        // Test that the function returns a valid database URL string
        let url = get_test_database_url();
        assert!(
            url.starts_with("postgresql://"),
            "Should return a PostgreSQL URL"
        );
        assert!(!url.is_empty(), "Should not return empty string");
    }
}

//! # Worker Database Testing Utilities
//!
//! Simplified database utilities for worker testing using standard sqlx patterns.
//!
//! ## Recommended Usage
//!
//! For most tests, use the standard sqlx approach:
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

use sqlx::PgPool;
use std::sync::Arc;

/// Simple test database error type
#[derive(Debug, thiserror::Error)]
pub enum TestDatabaseError {
    #[error("Database operation failed: {0}")]
    DatabaseError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Simplified worker database utilities for testing
///
/// This is a thin wrapper around standard sqlx testing patterns.
/// For most cases, use `#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]` instead.
#[derive(Clone, Debug)]
pub struct WorkerDatabaseUtils {
    pool: Arc<PgPool>,
}

impl WorkerDatabaseUtils {
    /// Create new database utils from existing pool
    ///
    /// Prefer using `tasker_shared::test_utils::setup_test_db()` or `#[sqlx::test]` instead.
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Get the database pool for custom operations
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// Convenience wrapper for test database lifecycle
///
/// This provides a simple way to get a test database with migrations applied.
/// For individual tests, prefer using `#[sqlx::test]` instead.
pub struct WorkerTestDatabase {
    utils: WorkerDatabaseUtils,
}

impl WorkerTestDatabase {
    /// Create a test database using standard sqlx patterns
    pub async fn create() -> Result<Self, TestDatabaseError> {
        let pool = tasker_shared::test_utils::setup_test_db().await;

        Ok(Self {
            utils: WorkerDatabaseUtils::from_pool(pool),
        })
    }

    /// Get the underlying utilities
    pub fn utils(&self) -> &WorkerDatabaseUtils {
        &self.utils
    }

    /// Get the database pool
    pub fn pool(&self) -> &PgPool {
        self.utils.pool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_standard_sqlx_pattern_works(pool: PgPool) {
        // This test demonstrates the recommended pattern
        let utils = WorkerDatabaseUtils::from_pool(pool);
        assert!(!utils.pool().is_closed());
    }
}

//! PostgreSQL Error Codes
//!
//! Provides constants for PostgreSQL SQLSTATE error codes used throughout the codebase.
//! This eliminates magic strings and provides documentation for error handling patterns.
//!
//! ## SQLSTATE Format
//!
//! PostgreSQL error codes follow the SQL standard SQLSTATE format:
//! - 5-character codes representing error classes and conditions
//! - First 2 characters: error class
//! - Last 3 characters: specific condition
//!
//! ## Reference
//!
//! Full list: <https://www.postgresql.org/docs/current/errcodes-appendix.html>
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_shared::database::error_codes::PgErrorCode;
//!
//! match db_error.code() {
//!     Some(code) if PgErrorCode::is_unique_violation(code) => {
//!         // Handle duplicate key
//!     }
//!     _ => return Err(db_error),
//! }
//! ```

/// PostgreSQL SQLSTATE error codes
///
/// This struct provides constants and helper methods for working with PostgreSQL error codes.
/// Only codes actively used in the codebase are included; add more as needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PgErrorCode;

impl PgErrorCode {
    // =========================================================================
    // Class 23 — Integrity Constraint Violation
    // =========================================================================

    /// Unique violation (duplicate key) - Code 23505
    ///
    /// Occurs when an INSERT or UPDATE would violate a unique constraint.
    /// Common in idempotent operations where concurrent processes may attempt
    /// to create the same record.
    ///
    /// ## Example Scenarios
    ///
    /// - Two orchestrators simultaneously creating the same workflow step
    /// - Concurrent task creation with the same identity hash
    /// - Race conditions in "first past the finish" patterns
    pub const UNIQUE_VIOLATION: &'static str = "23505";

    /// Foreign key violation - Code 23503
    ///
    /// Occurs when an INSERT or UPDATE references a non-existent foreign key value,
    /// or when a DELETE would leave orphaned references.
    pub const FOREIGN_KEY_VIOLATION: &'static str = "23503";

    /// Not null violation - Code 23502
    ///
    /// Occurs when an INSERT or UPDATE attempts to set a NOT NULL column to NULL.
    pub const NOT_NULL_VIOLATION: &'static str = "23502";

    /// Check constraint violation - Code 23514
    ///
    /// Occurs when an INSERT or UPDATE violates a CHECK constraint.
    pub const CHECK_VIOLATION: &'static str = "23514";

    /// Exclusion constraint violation - Code 23P01
    ///
    /// Occurs when an INSERT or UPDATE violates an exclusion constraint.
    pub const EXCLUSION_VIOLATION: &'static str = "23P01";

    // =========================================================================
    // Class 40 — Transaction Rollback
    // =========================================================================

    /// Serialization failure - Code 40001
    ///
    /// Occurs in SERIALIZABLE isolation level when a transaction cannot be
    /// serialized with concurrent transactions. Should be retried.
    pub const SERIALIZATION_FAILURE: &'static str = "40001";

    /// Deadlock detected - Code 40P01
    ///
    /// Occurs when two or more transactions are waiting for each other.
    /// One transaction is rolled back to break the deadlock.
    pub const DEADLOCK_DETECTED: &'static str = "40P01";

    // =========================================================================
    // Class 57 — Operator Intervention
    // =========================================================================

    /// Query canceled - Code 57014
    ///
    /// Occurs when a query is canceled by user request or timeout.
    pub const QUERY_CANCELED: &'static str = "57014";

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Check if the error code is a unique constraint violation
    #[inline]
    pub fn is_unique_violation(code: &str) -> bool {
        code == Self::UNIQUE_VIOLATION
    }

    /// Check if the error code is any integrity constraint violation (Class 23)
    #[inline]
    pub fn is_integrity_constraint_violation(code: &str) -> bool {
        code.starts_with("23")
    }

    /// Check if the error is retryable (serialization failure or deadlock)
    #[inline]
    pub fn is_retryable_transaction_error(code: &str) -> bool {
        code == Self::SERIALIZATION_FAILURE || code == Self::DEADLOCK_DETECTED
    }

    /// Check if the error code is a foreign key violation
    #[inline]
    pub fn is_foreign_key_violation(code: &str) -> bool {
        code == Self::FOREIGN_KEY_VIOLATION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_violation_detection() {
        assert!(PgErrorCode::is_unique_violation("23505"));
        assert!(!PgErrorCode::is_unique_violation("23503"));
        assert!(!PgErrorCode::is_unique_violation("40001"));
    }

    #[test]
    fn test_integrity_constraint_class() {
        assert!(PgErrorCode::is_integrity_constraint_violation("23505"));
        assert!(PgErrorCode::is_integrity_constraint_violation("23503"));
        assert!(PgErrorCode::is_integrity_constraint_violation("23502"));
        assert!(!PgErrorCode::is_integrity_constraint_violation("40001"));
    }

    #[test]
    fn test_retryable_errors() {
        assert!(PgErrorCode::is_retryable_transaction_error("40001"));
        assert!(PgErrorCode::is_retryable_transaction_error("40P01"));
        assert!(!PgErrorCode::is_retryable_transaction_error("23505"));
    }

    #[test]
    fn test_foreign_key_violation() {
        assert!(PgErrorCode::is_foreign_key_violation("23503"));
        assert!(!PgErrorCode::is_foreign_key_violation("23505"));
    }
}

//! Recommended error_type values for cross-language consistency.
//!
//! These constants provide standardized error type values that should be used
//! across Ruby, Python, and Rust workers to ensure consistent error classification.
//!
//! # Usage
//!
//! ```rust
//! use tasker_shared::types::error_types;
//!
//! // Use constants for error classification
//! let is_transient_failure = true;
//! let error_type = if is_transient_failure {
//!     error_types::RETRYABLE_ERROR
//! } else {
//!     error_types::PERMANENT_ERROR
//! };
//!
//! // Check if an error type is standard
//! assert!(error_types::is_standard(error_types::TIMEOUT));
//! ```

/// Error indicating a permanent, non-recoverable failure.
/// Examples: invalid input, resource not found, authentication failure.
pub const PERMANENT_ERROR: &str = "permanent_error";

/// Error indicating a transient failure that may succeed on retry.
/// Examples: network timeout, service unavailable, rate limiting.
pub const RETRYABLE_ERROR: &str = "retryable_error";

/// Error indicating input validation failure.
/// Examples: missing required field, invalid format, constraint violation.
pub const VALIDATION_ERROR: &str = "validation_error";

/// Error indicating an operation timed out.
/// Examples: HTTP request timeout, database query timeout.
pub const TIMEOUT: &str = "timeout";

/// Error indicating a failure within the step handler itself.
/// Examples: unhandled exception, handler misconfiguration.
pub const HANDLER_ERROR: &str = "handler_error";

/// All standard error types.
pub const ALL: &[&str] = &[
    PERMANENT_ERROR,
    RETRYABLE_ERROR,
    VALIDATION_ERROR,
    TIMEOUT,
    HANDLER_ERROR,
];

/// Check if an error type is one of the standard values.
///
/// # Arguments
///
/// * `error_type` - The error type string to check
///
/// # Returns
///
/// `true` if the error type matches one of the standard values
///
/// # Examples
///
/// ```rust
/// use tasker_shared::types::error_types;
///
/// assert!(error_types::is_standard("permanent_error"));
/// assert!(error_types::is_standard(error_types::TIMEOUT));
/// assert!(!error_types::is_standard("custom_error"));
/// ```
#[must_use]
pub fn is_standard(error_type: &str) -> bool {
    ALL.contains(&error_type)
}

/// Get the recommended retryable flag for a given error type.
///
/// This provides a sensible default mapping between error types and
/// whether they should be retried, though handlers may override this.
///
/// # Arguments
///
/// * `error_type` - The error type string
///
/// # Returns
///
/// `true` if the error type is typically retryable
///
/// # Examples
///
/// ```rust
/// use tasker_shared::types::error_types;
///
/// assert!(!error_types::is_typically_retryable(error_types::PERMANENT_ERROR));
/// assert!(error_types::is_typically_retryable(error_types::RETRYABLE_ERROR));
/// assert!(error_types::is_typically_retryable(error_types::TIMEOUT));
/// ```
#[must_use]
pub fn is_typically_retryable(error_type: &str) -> bool {
    matches!(error_type, RETRYABLE_ERROR | TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_error_types() {
        assert!(is_standard(PERMANENT_ERROR));
        assert!(is_standard(RETRYABLE_ERROR));
        assert!(is_standard(VALIDATION_ERROR));
        assert!(is_standard(TIMEOUT));
        assert!(is_standard(HANDLER_ERROR));
    }

    #[test]
    fn test_non_standard_error_type() {
        assert!(!is_standard("custom_error"));
        assert!(!is_standard("unknown"));
        assert!(!is_standard(""));
    }

    #[test]
    fn test_all_contains_expected_values() {
        assert_eq!(ALL.len(), 5);
        assert!(ALL.contains(&"permanent_error"));
        assert!(ALL.contains(&"retryable_error"));
        assert!(ALL.contains(&"validation_error"));
        assert!(ALL.contains(&"timeout"));
        assert!(ALL.contains(&"handler_error"));
    }

    #[test]
    fn test_typically_retryable() {
        assert!(!is_typically_retryable(PERMANENT_ERROR));
        assert!(is_typically_retryable(RETRYABLE_ERROR));
        assert!(!is_typically_retryable(VALIDATION_ERROR));
        assert!(is_typically_retryable(TIMEOUT));
        assert!(!is_typically_retryable(HANDLER_ERROR));
    }
}

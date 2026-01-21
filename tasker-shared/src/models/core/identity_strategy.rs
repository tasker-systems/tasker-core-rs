//! TAS-154: Task Identity Strategy Pattern
//!
//! Defines strategies for computing task identity hashes, enabling configurable
//! deduplication behavior based on domain requirements.
//!
//! # Strategies
//!
//! - **Strict** (default): `hash(named_task_uuid, context)` - Full idempotency
//! - **CallerProvided**: Caller must provide `idempotency_key` - Like Stripe's pattern
//! - **AlwaysUnique**: `uuidv7()` - No deduplication, every request creates new task
//!
//! # Example
//!
//! ```rust
//! use tasker_shared::models::core::IdentityStrategy;
//!
//! // Default is Strict for safety
//! let strategy = IdentityStrategy::default();
//! assert_eq!(strategy, IdentityStrategy::Strict);
//!
//! // Parse from string (e.g., from database)
//! let strategy: IdentityStrategy = "caller_provided".parse().unwrap();
//! assert_eq!(strategy, IdentityStrategy::CallerProvided);
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Strategy for computing task identity hash.
///
/// Named tasks can configure their identity strategy to control deduplication behavior.
/// Per-request overrides via `idempotency_key` take precedence over the named task's strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "identity_strategy", rename_all = "snake_case")]
pub enum IdentityStrategy {
    /// Hash of (named_task_uuid, context) - strict idempotency (default)
    ///
    /// Same named task + same context = same identity hash = deduplicated.
    /// This is the safest option for preventing accidental duplicate task creation.
    #[default]
    Strict,

    /// Caller must provide `idempotency_key`, reject if missing
    ///
    /// Similar to Stripe's Idempotency-Key pattern. The caller controls uniqueness
    /// by providing their own key. Useful when the caller has a natural idempotency
    /// key (e.g., payment_id, order_id).
    CallerProvided,

    /// Always generate unique identity (uuidv7)
    ///
    /// Every request creates a new task, no deduplication. Useful for event-driven
    /// triggers or scheduled jobs where repetition is intentional.
    AlwaysUnique,
}

impl IdentityStrategy {
    /// Returns the database string representation of this strategy
    pub fn as_str(&self) -> &'static str {
        match self {
            IdentityStrategy::Strict => "strict",
            IdentityStrategy::CallerProvided => "caller_provided",
            IdentityStrategy::AlwaysUnique => "always_unique",
        }
    }

    /// Returns true if this strategy requires a caller-provided idempotency key
    pub fn requires_idempotency_key(&self) -> bool {
        matches!(self, IdentityStrategy::CallerProvided)
    }

    /// Returns true if this strategy performs any deduplication
    pub fn deduplicates(&self) -> bool {
        !matches!(self, IdentityStrategy::AlwaysUnique)
    }
}

impl fmt::Display for IdentityStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Error type for parsing IdentityStrategy from string
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIdentityStrategyError {
    pub invalid_value: String,
}

impl fmt::Display for ParseIdentityStrategyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid identity strategy '{}': expected one of 'strict', 'caller_provided', 'always_unique'",
            self.invalid_value
        )
    }
}

impl std::error::Error for ParseIdentityStrategyError {}

impl FromStr for IdentityStrategy {
    type Err = ParseIdentityStrategyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "strict" => Ok(IdentityStrategy::Strict),
            "caller_provided" | "callerprovided" => Ok(IdentityStrategy::CallerProvided),
            "always_unique" | "alwaysunique" => Ok(IdentityStrategy::AlwaysUnique),
            _ => Err(ParseIdentityStrategyError {
                invalid_value: s.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_strict() {
        assert_eq!(IdentityStrategy::default(), IdentityStrategy::Strict);
    }

    #[test]
    fn test_as_str() {
        assert_eq!(IdentityStrategy::Strict.as_str(), "strict");
        assert_eq!(IdentityStrategy::CallerProvided.as_str(), "caller_provided");
        assert_eq!(IdentityStrategy::AlwaysUnique.as_str(), "always_unique");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", IdentityStrategy::Strict), "strict");
        assert_eq!(
            format!("{}", IdentityStrategy::CallerProvided),
            "caller_provided"
        );
        assert_eq!(
            format!("{}", IdentityStrategy::AlwaysUnique),
            "always_unique"
        );
    }

    #[test]
    fn test_from_str() {
        assert_eq!(
            "strict".parse::<IdentityStrategy>().unwrap(),
            IdentityStrategy::Strict
        );
        assert_eq!(
            "STRICT".parse::<IdentityStrategy>().unwrap(),
            IdentityStrategy::Strict
        );
        assert_eq!(
            "caller_provided".parse::<IdentityStrategy>().unwrap(),
            IdentityStrategy::CallerProvided
        );
        assert_eq!(
            "CallerProvided".parse::<IdentityStrategy>().unwrap(),
            IdentityStrategy::CallerProvided
        );
        assert_eq!(
            "always_unique".parse::<IdentityStrategy>().unwrap(),
            IdentityStrategy::AlwaysUnique
        );
        assert_eq!(
            "AlwaysUnique".parse::<IdentityStrategy>().unwrap(),
            IdentityStrategy::AlwaysUnique
        );
    }

    #[test]
    fn test_from_str_error() {
        let err = "invalid".parse::<IdentityStrategy>().unwrap_err();
        assert_eq!(err.invalid_value, "invalid");
        assert!(err.to_string().contains("invalid identity strategy"));
    }

    #[test]
    fn test_requires_idempotency_key() {
        assert!(!IdentityStrategy::Strict.requires_idempotency_key());
        assert!(IdentityStrategy::CallerProvided.requires_idempotency_key());
        assert!(!IdentityStrategy::AlwaysUnique.requires_idempotency_key());
    }

    #[test]
    fn test_deduplicates() {
        assert!(IdentityStrategy::Strict.deduplicates());
        assert!(IdentityStrategy::CallerProvided.deduplicates());
        assert!(!IdentityStrategy::AlwaysUnique.deduplicates());
    }

    #[test]
    fn test_serde_roundtrip() {
        for strategy in [
            IdentityStrategy::Strict,
            IdentityStrategy::CallerProvided,
            IdentityStrategy::AlwaysUnique,
        ] {
            let json = serde_json::to_string(&strategy).unwrap();
            let parsed: IdentityStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(strategy, parsed);
        }
    }

    #[test]
    fn test_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&IdentityStrategy::CallerProvided).unwrap(),
            "\"caller_provided\""
        );
        assert_eq!(
            serde_json::to_string(&IdentityStrategy::AlwaysUnique).unwrap(),
            "\"always_unique\""
        );
    }
}

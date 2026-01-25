//! Cache Usage Context and Constraints (TAS-168)
//!
//! Provides type-safe constraints for cache usage to prevent operational errors.
//!
//! ## Problem
//!
//! In-memory caches (like Moka) are per-process and don't share state across
//! instances. This is problematic for template caching because:
//!
//! 1. Workers invalidate template cache on bootstrap
//! 2. An in-memory cache on the orchestration server never sees these invalidations
//! 3. This leads to stale templates being served → operational errors
//!
//! ## Solution
//!
//! We define usage contexts that determine which cache backends are valid:
//!
//! - **Templates**: Require distributed cache (Redis) or no cache (NoOp)
//! - **Analytics**: Can use any backend (data is informational, TTL-bounded)
//! - **Generic**: No restrictions
//!
//! The `ConstrainedCacheProvider` wrapper validates the backend against the
//! usage context at construction time.

use super::CacheProvider;
use std::sync::Arc;

/// Cache usage context - determines which backends are valid
///
/// Different use cases have different consistency requirements:
///
/// - Templates need distributed cache to ensure workers and orchestration
///   share the same cache state
/// - Analytics can use local cache since data is informational and TTL-bounded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheUsageContext {
    /// Templates require distributed cache (Redis) or no cache (NoOp)
    ///
    /// In-memory cache would drift from worker invalidations, causing
    /// stale templates to be served. This is an operational risk.
    Templates,

    /// Analytics can use any backend including in-memory
    ///
    /// Analytics data is informational and TTL-bounded. Brief staleness
    /// is acceptable and in-memory cache provides DoS protection.
    Analytics,

    /// Generic caching with no restrictions
    ///
    /// Use when the caller understands and accepts the tradeoffs of
    /// any backend type.
    Generic,
}

impl CacheUsageContext {
    /// Check if the given provider is valid for this usage context
    pub fn is_valid_provider(&self, provider: &CacheProvider) -> bool {
        match self {
            Self::Templates => {
                // Templates can only use distributed cache or no cache
                provider.is_distributed()
            }
            Self::Analytics | Self::Generic => {
                // No restrictions
                true
            }
        }
    }

    /// Human-readable description of why a provider might be invalid
    pub fn validation_reason(&self) -> &'static str {
        match self {
            Self::Templates => {
                "Templates require a distributed cache (Redis) because workers \
                 invalidate cache on bootstrap. In-memory cache would drift and \
                 serve stale templates."
            }
            Self::Analytics => {
                "Analytics can use any cache backend. Data is informational and \
                 TTL-bounded, so brief staleness is acceptable."
            }
            Self::Generic => "Generic usage has no cache backend restrictions.",
        }
    }
}

impl std::fmt::Display for CacheUsageContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Templates => write!(f, "Templates"),
            Self::Analytics => write!(f, "Analytics"),
            Self::Generic => write!(f, "Generic"),
        }
    }
}

/// Type-safe cache provider wrapper that enforces usage constraints
///
/// This wrapper validates at construction time that the underlying
/// cache provider is appropriate for the given usage context.
///
/// ## Example
///
/// ```ignore
/// use tasker_shared::cache::constraints::{CacheUsageContext, ConstrainedCacheProvider};
///
/// // For template caching, only distributed providers are allowed
/// let constrained = ConstrainedCacheProvider::new(
///     provider.clone(),
///     CacheUsageContext::Templates,
/// );
///
/// if constrained.is_none() {
///     // Provider is in-memory, not safe for templates
///     warn!("Template caching disabled: in-memory cache not safe for templates");
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ConstrainedCacheProvider {
    inner: Arc<CacheProvider>,
    context: CacheUsageContext,
}

impl ConstrainedCacheProvider {
    /// Create a constrained provider if the backend is valid for the context
    ///
    /// Returns `None` if the provider is not valid for the given context.
    /// This allows callers to handle the constraint violation gracefully.
    pub fn new(provider: Arc<CacheProvider>, context: CacheUsageContext) -> Option<Self> {
        if context.is_valid_provider(&provider) {
            Some(Self {
                inner: provider,
                context,
            })
        } else {
            None
        }
    }

    /// Create a constrained provider, returning an error description on failure
    ///
    /// Useful when you want to log a detailed reason for the validation failure.
    pub fn try_new(
        provider: Arc<CacheProvider>,
        context: CacheUsageContext,
    ) -> Result<Self, ConstraintViolation> {
        if context.is_valid_provider(&provider) {
            Ok(Self {
                inner: provider,
                context,
            })
        } else {
            Err(ConstraintViolation {
                provider_name: provider.provider_name().to_string(),
                context,
            })
        }
    }

    /// Get the underlying cache provider
    pub fn provider(&self) -> &Arc<CacheProvider> {
        &self.inner
    }

    /// Get the usage context this provider is constrained to
    pub fn context(&self) -> CacheUsageContext {
        self.context
    }

    /// Check if the cache is enabled (not NoOp)
    pub fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }
}

/// Error returned when a cache provider violates a usage constraint
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    /// Name of the provider that violated the constraint
    pub provider_name: String,
    /// The context that was violated
    pub context: CacheUsageContext,
}

impl std::fmt::Display for ConstraintViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache provider '{}' is not valid for {} context: {}",
            self.provider_name,
            self.context,
            self.context.validation_reason()
        )
    }
}

impl std::error::Error for ConstraintViolation {}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_provider() -> Arc<CacheProvider> {
        Arc::new(CacheProvider::noop())
    }

    #[test]
    fn test_noop_is_valid_for_templates() {
        let provider = noop_provider();
        assert!(CacheUsageContext::Templates.is_valid_provider(&provider));
    }

    #[test]
    fn test_noop_is_valid_for_analytics() {
        let provider = noop_provider();
        assert!(CacheUsageContext::Analytics.is_valid_provider(&provider));
    }

    #[test]
    fn test_noop_is_valid_for_generic() {
        let provider = noop_provider();
        assert!(CacheUsageContext::Generic.is_valid_provider(&provider));
    }

    #[cfg(feature = "cache-moka")]
    #[test]
    fn test_moka_is_not_valid_for_templates() {
        use crate::cache::MokaCacheService;
        use std::time::Duration;

        let moka = MokaCacheService::new(100, Duration::from_secs(60));
        let provider = Arc::new(CacheProvider::Moka(Box::new(moka)));

        assert!(!CacheUsageContext::Templates.is_valid_provider(&provider));
    }

    #[cfg(feature = "cache-moka")]
    #[test]
    fn test_moka_is_valid_for_analytics() {
        use crate::cache::MokaCacheService;
        use std::time::Duration;

        let moka = MokaCacheService::new(100, Duration::from_secs(60));
        let provider = Arc::new(CacheProvider::Moka(Box::new(moka)));

        assert!(CacheUsageContext::Analytics.is_valid_provider(&provider));
    }

    #[cfg(feature = "cache-moka")]
    #[test]
    fn test_moka_is_valid_for_generic() {
        use crate::cache::MokaCacheService;
        use std::time::Duration;

        let moka = MokaCacheService::new(100, Duration::from_secs(60));
        let provider = Arc::new(CacheProvider::Moka(Box::new(moka)));

        assert!(CacheUsageContext::Generic.is_valid_provider(&provider));
    }

    #[test]
    fn test_constrained_provider_accepts_valid_noop_for_templates() {
        let provider = noop_provider();
        let constrained = ConstrainedCacheProvider::new(provider, CacheUsageContext::Templates);
        assert!(constrained.is_some());
    }

    #[cfg(feature = "cache-moka")]
    #[test]
    fn test_constrained_provider_rejects_moka_for_templates() {
        use crate::cache::MokaCacheService;
        use std::time::Duration;

        let moka = MokaCacheService::new(100, Duration::from_secs(60));
        let provider = Arc::new(CacheProvider::Moka(Box::new(moka)));

        let constrained = ConstrainedCacheProvider::new(provider, CacheUsageContext::Templates);
        assert!(constrained.is_none());
    }

    #[cfg(feature = "cache-moka")]
    #[test]
    fn test_constrained_provider_accepts_moka_for_analytics() {
        use crate::cache::MokaCacheService;
        use std::time::Duration;

        let moka = MokaCacheService::new(100, Duration::from_secs(60));
        let provider = Arc::new(CacheProvider::Moka(Box::new(moka)));

        let constrained = ConstrainedCacheProvider::new(provider, CacheUsageContext::Analytics);
        assert!(constrained.is_some());
    }

    #[cfg(feature = "cache-moka")]
    #[test]
    fn test_try_new_returns_error_for_moka_templates() {
        use crate::cache::MokaCacheService;
        use std::time::Duration;

        let moka = MokaCacheService::new(100, Duration::from_secs(60));
        let provider = Arc::new(CacheProvider::Moka(Box::new(moka)));

        let result = ConstrainedCacheProvider::try_new(provider, CacheUsageContext::Templates);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.provider_name, "moka");
        assert_eq!(err.context, CacheUsageContext::Templates);
        assert!(err.to_string().contains("moka"));
        assert!(err.to_string().contains("Templates"));
    }

    #[test]
    fn test_constrained_provider_is_enabled_noop() {
        let provider = noop_provider();
        let constrained =
            ConstrainedCacheProvider::new(provider, CacheUsageContext::Templates).unwrap();
        assert!(!constrained.is_enabled()); // NoOp is not "enabled"
    }

    #[test]
    fn test_context_display() {
        assert_eq!(format!("{}", CacheUsageContext::Templates), "Templates");
        assert_eq!(format!("{}", CacheUsageContext::Analytics), "Analytics");
        assert_eq!(format!("{}", CacheUsageContext::Generic), "Generic");
    }

    #[test]
    fn test_validation_reason_not_empty() {
        assert!(!CacheUsageContext::Templates.validation_reason().is_empty());
        assert!(!CacheUsageContext::Analytics.validation_reason().is_empty());
        assert!(!CacheUsageContext::Generic.validation_reason().is_empty());
    }
}

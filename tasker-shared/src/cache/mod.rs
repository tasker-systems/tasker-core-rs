//! # Distributed Cache Module (TAS-156, TAS-168, TAS-171)
//!
//! Provides opt-in caching for task templates and analytics responses.
//!
//! ## Architecture
//!
//! ```text
//! CacheProvider (struct)          <- Unified interface with integrated circuit breaker
//!   └── CacheBackend (internal)   <- Zero-cost dispatch, no vtable
//!         ├── Redis               <- Distributed (Redis/Dragonfly)
//!         ├── Memcached           <- Distributed (Memcached protocol)
//!         ├── Moka                <- In-process, TTL-based
//!         └── NoOp                <- Always-miss fallback
//! ```
//!
//! ## Backend Selection
//!
//! | Backend     | Distributed | Use Case |
//! |-------------|-------------|----------|
//! | Redis       | Yes         | Multi-instance production deployments |
//! | Dragonfly   | Yes         | Redis-compatible with better performance |
//! | Memcached   | Yes         | Memcached protocol support (TAS-171) |
//! | Moka        | No          | Single-instance, analytics caching, DoS protection |
//! | NoOp        | N/A         | Caching disabled or fallback |
//!
//! ## Design Decisions
//!
//! - **Struct with internal enum**: Circuit breaker is an implementation detail
//! - **Graceful degradation**: Backend failure → NoOp fallback, never blocks startup
//! - **Circuit breaker protection**: Fail-fast when Redis/Dragonfly is down (TAS-171)
//! - **Best-effort writes**: Cache errors logged but never propagated
//! - **SCAN for patterns**: Non-blocking key iteration (never uses KEYS)
//! - **Type constraints**: Templates require distributed cache (TAS-168)
//!
//! ## Usage
//!
//! The cache is used by `TaskHandlerRegistry` for template resolution and
//! `AnalyticsService` for analytics response caching.

pub mod constraints;
pub mod errors;
pub mod provider;
pub mod providers;
pub mod traits;

pub use constraints::{CacheUsageContext, ConstrainedCacheProvider, ConstraintViolation};
pub use errors::{CacheError, CacheResult};
pub use provider::CacheProvider;
pub use providers::NoOpCacheService;
pub use traits::CacheService;

// Re-export CircuitState for health endpoint visibility
pub use crate::resilience::CircuitState;

#[cfg(feature = "cache-redis")]
pub use providers::RedisCacheService;

#[cfg(feature = "cache-moka")]
pub use providers::MokaCacheService;

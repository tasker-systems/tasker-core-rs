//! # Distributed Cache Module (TAS-156, TAS-168)
//!
//! Provides opt-in caching for task templates and analytics responses.
//!
//! ## Architecture
//!
//! ```text
//! CacheProvider (enum)            <- Zero-cost dispatch, no vtable
//!   ├── Redis(RedisCacheService)  <- Distributed, ConnectionManager-based async
//!   ├── Moka(MokaCacheService)    <- In-process, TTL-based (TAS-168)
//!   └── NoOp(NoOpCacheService)    <- Always-miss, always-succeed fallback
//! ```
//!
//! ## Backend Selection
//!
//! | Backend | Distributed | Use Case |
//! |---------|-------------|----------|
//! | Redis   | Yes         | Multi-instance production deployments |
//! | Moka    | No          | Single-instance, analytics caching, DoS protection |
//! | NoOp    | N/A         | Caching disabled or fallback |
//!
//! ## Design Decisions
//!
//! - **Enum dispatch** (like MessagingProvider): zero vtable overhead
//! - **Graceful degradation**: Backend failure → NoOp fallback, never blocks startup
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

#[cfg(feature = "cache-redis")]
pub use providers::RedisCacheService;

#[cfg(feature = "cache-moka")]
pub use providers::MokaCacheService;

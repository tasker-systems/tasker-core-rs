//! # Distributed Cache Module (TAS-156)
//!
//! Provides opt-in distributed caching for task template resolution.
//!
//! ## Architecture
//!
//! ```text
//! CacheProvider (enum)          <- Zero-cost dispatch, no vtable
//!   ├── Redis(RedisCacheService)  <- ConnectionManager-based async Redis
//!   └── NoOp(NoOpCacheService)    <- Always-miss, always-succeed fallback
//! ```
//!
//! ## Design Decisions
//!
//! - **Enum dispatch** (like MessagingProvider): zero vtable overhead
//! - **Graceful degradation**: Redis failure → NoOp fallback, never blocks startup
//! - **Best-effort writes**: Cache errors logged but never propagated
//! - **SCAN for patterns**: Non-blocking key iteration (never uses KEYS)
//!
//! ## Usage
//!
//! The cache is internal to `TaskHandlerRegistry`. External code does not
//! interact with it directly - template resolution transparently uses
//! the cache when available.

pub mod errors;
pub mod provider;
pub mod providers;
pub mod traits;

pub use errors::{CacheError, CacheResult};
pub use provider::CacheProvider;
pub use providers::NoOpCacheService;
pub use traits::CacheService;

#[cfg(feature = "cache-redis")]
pub use providers::RedisCacheService;

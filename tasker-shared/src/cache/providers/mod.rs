//! Cache provider implementations (TAS-156, TAS-168)

pub mod noop;

#[cfg(feature = "cache-redis")]
pub mod redis;

#[cfg(feature = "cache-moka")]
pub mod moka;

pub use noop::NoOpCacheService;

#[cfg(feature = "cache-redis")]
pub use self::redis::RedisCacheService;

#[cfg(feature = "cache-moka")]
pub use self::moka::MokaCacheService;

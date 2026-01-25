//! Cache provider implementations (TAS-156)

pub mod noop;

#[cfg(feature = "cache-redis")]
pub mod redis;

pub use noop::NoOpCacheService;

#[cfg(feature = "cache-redis")]
pub use self::redis::RedisCacheService;

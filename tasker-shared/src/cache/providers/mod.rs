//! Cache provider implementations (TAS-156, TAS-168, TAS-171)

pub mod noop;

#[cfg(feature = "cache-redis")]
pub mod redis;

#[cfg(feature = "cache-moka")]
pub mod moka;

#[cfg(feature = "cache-memcached")]
pub mod memcached;

pub use noop::NoOpCacheService;

#[cfg(feature = "cache-redis")]
pub use self::redis::RedisCacheService;

#[cfg(feature = "cache-moka")]
pub use self::moka::MokaCacheService;

#[cfg(feature = "cache-memcached")]
pub use self::memcached::MemcachedCacheService;

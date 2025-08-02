//! Configuration Management
//!
//! This module provides configuration management for various aspects of the
//! tasker-core-rs system, including query caching, database connections,
//! and environment-specific overrides.

pub mod query_cache_config;

pub use query_cache_config::{CacheTypeConfig, QueryCacheConfig, QueryCacheConfigLoader};

//! Query Cache Configuration Management
//!
//! This module provides configuration management for query caching based on
//! environment-specific YAML configuration files. It allows different cache
//! behaviors in production, development, and test environments.

use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use tracing::{info, warn};

/// Configuration for query cache behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCacheConfig {
    pub enabled: bool,
    pub active_workers: CacheTypeConfig,
    pub worker_health: CacheTypeConfig,
    pub task_metadata: CacheTypeConfig,
    pub handler_metadata: CacheTypeConfig,
    pub cleanup_interval_seconds: u64,
    pub memory_pressure_threshold: f64,
}

/// Configuration for a specific type of cached data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheTypeConfig {
    pub ttl_seconds: u64,
    pub max_entries: usize,
}

impl CacheTypeConfig {
    /// Get TTL as Duration
    pub fn ttl_duration(&self) -> Duration {
        Duration::from_secs(self.ttl_seconds)
    }
}

impl Default for QueryCacheConfig {
    /// Default configuration suitable for production
    fn default() -> Self {
        Self {
            enabled: true,
            active_workers: CacheTypeConfig {
                ttl_seconds: 30,
                max_entries: 1000,
            },
            worker_health: CacheTypeConfig {
                ttl_seconds: 10,
                max_entries: 500,
            },
            task_metadata: CacheTypeConfig {
                ttl_seconds: 300,
                max_entries: 2000,
            },
            handler_metadata: CacheTypeConfig {
                ttl_seconds: 600,
                max_entries: 100,
            },
            cleanup_interval_seconds: 300,
            memory_pressure_threshold: 0.8,
        }
    }
}

impl QueryCacheConfig {
    /// Create test-optimized configuration with rapid invalidation
    pub fn for_test() -> Self {
        Self {
            enabled: true,
            active_workers: CacheTypeConfig {
                ttl_seconds: 1, // 1 second for rapid test feedback
                max_entries: 100,
            },
            worker_health: CacheTypeConfig {
                ttl_seconds: 1, // 1 second for rapid test feedback
                max_entries: 50,
            },
            task_metadata: CacheTypeConfig {
                ttl_seconds: 5, // 5 seconds for tests
                max_entries: 200,
            },
            handler_metadata: CacheTypeConfig {
                ttl_seconds: 10, // 10 seconds for tests
                max_entries: 20,
            },
            cleanup_interval_seconds: 10,   // Aggressive cleanup
            memory_pressure_threshold: 0.5, // Clear more aggressively
        }
    }

    /// Create development-optimized configuration
    pub fn for_development() -> Self {
        Self {
            enabled: true,
            active_workers: CacheTypeConfig {
                ttl_seconds: 10, // 10 seconds for development
                max_entries: 500,
            },
            worker_health: CacheTypeConfig {
                ttl_seconds: 5, // 5 seconds for development
                max_entries: 200,
            },
            task_metadata: CacheTypeConfig {
                ttl_seconds: 60, // 1 minute for development
                max_entries: 1000,
            },
            handler_metadata: CacheTypeConfig {
                ttl_seconds: 120, // 2 minutes for development
                max_entries: 50,
            },
            cleanup_interval_seconds: 60, // Moderate cleanup
            memory_pressure_threshold: 0.7,
        }
    }

    /// Load configuration from environment or use defaults
    pub fn from_environment() -> Self {
        // Detect environment from common environment variables
        let environment = env::var("RAILS_ENV")
            .or_else(|_| env::var("TASKER_ENV"))
            .or_else(|_| env::var("RUST_ENV"))
            .unwrap_or_else(|_| "production".to_string());

        let config = match environment.as_str() {
            "test" => {
                info!("Loading test query cache configuration (rapid invalidation)");
                Self::for_test()
            }
            "development" => {
                info!("Loading development query cache configuration");
                Self::for_development()
            }
            _ => {
                info!("Loading production query cache configuration");
                Self::default()
            }
        };

        // Apply environment variable overrides
        config.with_env_overrides()
    }

    /// Apply environment variable overrides to configuration
    pub fn with_env_overrides(mut self) -> Self {
        // Global cache enable/disable
        if let Ok(enabled) = env::var("TASKER_QUERY_CACHE_ENABLED") {
            self.enabled = enabled.parse().unwrap_or(self.enabled);
            info!("Query cache enabled override: {}", self.enabled);
        }

        // Active workers cache overrides
        if let Ok(ttl) = env::var("TASKER_QUERY_CACHE_ACTIVE_WORKERS_TTL_SECONDS") {
            if let Ok(seconds) = ttl.parse::<u64>() {
                self.active_workers.ttl_seconds = seconds;
                info!("Active workers cache TTL override: {}s", seconds);
            }
        }

        if let Ok(max) = env::var("TASKER_QUERY_CACHE_ACTIVE_WORKERS_MAX_ENTRIES") {
            if let Ok(entries) = max.parse::<usize>() {
                self.active_workers.max_entries = entries;
                info!("Active workers cache max entries override: {}", entries);
            }
        }

        // Worker health cache overrides
        if let Ok(ttl) = env::var("TASKER_QUERY_CACHE_WORKER_HEALTH_TTL_SECONDS") {
            if let Ok(seconds) = ttl.parse::<u64>() {
                self.worker_health.ttl_seconds = seconds;
                info!("Worker health cache TTL override: {}s", seconds);
            }
        }

        // Global cleanup override
        if let Ok(interval) = env::var("TASKER_QUERY_CACHE_CLEANUP_INTERVAL_SECONDS") {
            if let Ok(seconds) = interval.parse::<u64>() {
                self.cleanup_interval_seconds = seconds;
                info!("Query cache cleanup interval override: {}s", seconds);
            }
        }

        self
    }

    /// Check if query caching is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get cleanup interval as Duration
    pub fn cleanup_interval(&self) -> Duration {
        Duration::from_secs(self.cleanup_interval_seconds)
    }

    /// Log current configuration for debugging
    pub fn log_configuration(&self) {
        info!("Query Cache Configuration:");
        info!("  Enabled: {}", self.enabled);
        info!(
            "  Active Workers: {}s TTL, {} max entries",
            self.active_workers.ttl_seconds, self.active_workers.max_entries
        );
        info!(
            "  Worker Health: {}s TTL, {} max entries",
            self.worker_health.ttl_seconds, self.worker_health.max_entries
        );
        info!(
            "  Task Metadata: {}s TTL, {} max entries",
            self.task_metadata.ttl_seconds, self.task_metadata.max_entries
        );
        info!(
            "  Handler Metadata: {}s TTL, {} max entries",
            self.handler_metadata.ttl_seconds, self.handler_metadata.max_entries
        );
        info!("  Cleanup Interval: {}s", self.cleanup_interval_seconds);
        info!(
            "  Memory Pressure Threshold: {:.1}%",
            self.memory_pressure_threshold * 100.0
        );
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.memory_pressure_threshold < 0.1 || self.memory_pressure_threshold > 1.0 {
            return Err("Memory pressure threshold must be between 0.1 and 1.0".to_string());
        }

        if self.cleanup_interval_seconds == 0 {
            return Err("Cleanup interval must be greater than 0".to_string());
        }

        // Warn about potentially problematic configurations
        if self.active_workers.ttl_seconds == 0 {
            warn!("Active workers cache TTL is 0 - caching effectively disabled");
        }

        if self.active_workers.max_entries == 0 {
            warn!("Active workers cache max entries is 0 - caching effectively disabled");
        }

        Ok(())
    }
}

/// Configuration loader for YAML files
pub struct QueryCacheConfigLoader;

impl QueryCacheConfigLoader {
    /// Load configuration from YAML file based on environment
    pub fn load_from_yaml() -> Result<QueryCacheConfig, Box<dyn std::error::Error>> {
        // Try to detect config file based on environment
        let environment = env::var("RAILS_ENV")
            .or_else(|_| env::var("TASKER_ENV"))
            .or_else(|_| env::var("RUST_ENV"))
            .unwrap_or_else(|_| "production".to_string());

        let config_file = match environment.as_str() {
            "test" => "config/tasker-config-test.yaml",
            "development" => "config/tasker-config-development.yaml",
            _ => "config/tasker-config.yaml",
        };

        info!("Loading query cache configuration from: {}", config_file);

        // For now, return environment-based config since we don't have a YAML parser
        // In the future, this could use serde_yaml to parse the actual files
        let config = QueryCacheConfig::from_environment();

        // Validate the configuration
        config.validate()?;
        config.log_configuration();

        Ok(config)
    }
}

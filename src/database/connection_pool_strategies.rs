//! Advanced Connection Pool Strategies for Different Deployment Patterns
//!
//! This module provides optimized database connection pooling strategies tailored
//! for different deployment scenarios including single-process, distributed,
//! high-throughput, and resource-constrained environments.

use sqlx::{postgres::PgPoolOptions, PgPool};
use std::env;
use std::time::Duration;
use tracing::{info, warn};

/// Deployment patterns requiring different connection strategies
#[derive(Debug, Clone, Copy)]
pub enum DeploymentPattern {
    /// Single process with embedded TCP executor
    SingleProcess,
    /// Distributed across multiple instances
    Distributed,
    /// High-throughput processing with many concurrent operations
    HighThroughput,
    /// Resource-constrained environments (containers, lambdas)
    ResourceConstrained,
    /// Development and testing environments
    Development,
}

/// Connection pool configuration for specific deployment patterns
#[derive(Debug, Clone)]
pub struct PoolConfiguration {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub test_before_acquire: bool,
    pub deployment_pattern: DeploymentPattern,
}

impl PoolConfiguration {
    /// Get optimized configuration for single-process deployment
    /// 
    /// Characteristics:
    /// - Moderate connection pool (good for embedded TCP executor)
    /// - Fast acquire timeout for responsiveness
    /// - Reasonable idle timeout for resource efficiency
    pub fn single_process() -> Self {
        Self {
            max_connections: 50,
            min_connections: 5,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            max_lifetime: Duration::from_secs(3600), // 1 hour
            test_before_acquire: true,
            deployment_pattern: DeploymentPattern::SingleProcess,
        }
    }

    /// Get optimized configuration for distributed deployment
    /// 
    /// Characteristics:
    /// - Larger connection pool to handle distributed load
    /// - Longer timeouts for network latency tolerance
    /// - Connection testing for reliability across network
    pub fn distributed() -> Self {
        Self {
            max_connections: 100,
            min_connections: 10,
            acquire_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(600), // 10 minutes
            max_lifetime: Duration::from_secs(7200), // 2 hours
            test_before_acquire: true,
            deployment_pattern: DeploymentPattern::Distributed,
        }
    }

    /// Get optimized configuration for high-throughput scenarios
    /// 
    /// Characteristics:
    /// - Maximum connection pool for concurrent processing
    /// - Very fast acquire timeout to avoid blocking
    /// - Shorter idle timeout for rapid connection recycling
    pub fn high_throughput() -> Self {
        Self {
            max_connections: 200,
            min_connections: 20,
            acquire_timeout: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(120), // 2 minutes
            max_lifetime: Duration::from_secs(1800), // 30 minutes
            test_before_acquire: false, // Skip for performance
            deployment_pattern: DeploymentPattern::HighThroughput,
        }
    }

    /// Get optimized configuration for resource-constrained environments
    /// 
    /// Characteristics:
    /// - Minimal connection pool to conserve memory
    /// - Longer timeouts to maximize connection reuse
    /// - Extended lifetime to avoid reconnection overhead
    pub fn resource_constrained() -> Self {
        Self {
            max_connections: 10,
            min_connections: 2,
            acquire_timeout: Duration::from_secs(15),
            idle_timeout: Duration::from_secs(900), // 15 minutes
            max_lifetime: Duration::from_secs(10800), // 3 hours
            test_before_acquire: false, // Skip for resource conservation
            deployment_pattern: DeploymentPattern::ResourceConstrained,
        }
    }

    /// Get optimized configuration for development environments
    /// 
    /// Characteristics:
    /// - Small pool for development use
    /// - Fast timeouts for quick feedback
    /// - Connection testing for debugging
    pub fn development() -> Self {
        Self {
            max_connections: 20,
            min_connections: 3,
            acquire_timeout: Duration::from_secs(3),
            idle_timeout: Duration::from_secs(180), // 3 minutes
            max_lifetime: Duration::from_secs(900), // 15 minutes
            test_before_acquire: true,
            deployment_pattern: DeploymentPattern::Development,
        }
    }

    /// Auto-detect deployment pattern from environment variables
    pub fn from_environment() -> Self {
        let pattern = if env::var("TASKER_DEPLOYMENT_PATTERN").is_ok() {
            match env::var("TASKER_DEPLOYMENT_PATTERN").unwrap().as_str() {
                "single_process" => DeploymentPattern::SingleProcess,
                "distributed" => DeploymentPattern::Distributed,
                "high_throughput" => DeploymentPattern::HighThroughput,
                "resource_constrained" => DeploymentPattern::ResourceConstrained,
                "development" => DeploymentPattern::Development,
                _ => {
                    warn!("Unknown deployment pattern, defaulting to single_process");
                    DeploymentPattern::SingleProcess
                }
            }
        } else if env::var("KUBERNETES_SERVICE_HOST").is_ok() {
            // Running in Kubernetes - likely distributed
            DeploymentPattern::Distributed
        } else if env::var("AWS_LAMBDA_FUNCTION_NAME").is_ok() {
            // Running in Lambda - resource constrained
            DeploymentPattern::ResourceConstrained
        } else if env::var("RAILS_ENV").unwrap_or_default() == "development" {
            // Development environment
            DeploymentPattern::Development
        } else {
            // Default to single process
            DeploymentPattern::SingleProcess
        };

        match pattern {
            DeploymentPattern::SingleProcess => Self::single_process(),
            DeploymentPattern::Distributed => Self::distributed(),
            DeploymentPattern::HighThroughput => Self::high_throughput(),
            DeploymentPattern::ResourceConstrained => Self::resource_constrained(),
            DeploymentPattern::Development => Self::development(),
        }
    }

    /// Apply environment variable overrides to configuration
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(max_conn) = env::var("TASKER_DB_MAX_CONNECTIONS") {
            if let Ok(max) = max_conn.parse::<u32>() {
                self.max_connections = max;
                info!("Overriding max_connections from env: {}", max);
            }
        }

        if let Ok(min_conn) = env::var("TASKER_DB_MIN_CONNECTIONS") {
            if let Ok(min) = min_conn.parse::<u32>() {
                self.min_connections = min;
                info!("Overriding min_connections from env: {}", min);
            }
        }

        if let Ok(timeout) = env::var("TASKER_DB_ACQUIRE_TIMEOUT_SECS") {
            if let Ok(secs) = timeout.parse::<u64>() {
                self.acquire_timeout = Duration::from_secs(secs);
                info!("Overriding acquire_timeout from env: {}s", secs);
            }
        }

        self
    }
}

/// Enhanced database connection with deployment-aware pooling
pub struct OptimizedDatabaseConnection {
    pool: PgPool,
    config: PoolConfiguration,
}

impl OptimizedDatabaseConnection {
    /// Create connection with automatic deployment pattern detection
    pub async fn new() -> Result<Self, sqlx::Error> {
        let config = PoolConfiguration::from_environment().with_env_overrides();
        Self::new_with_config(config).await
    }

    /// Create connection with specific deployment pattern
    pub async fn new_for_pattern(pattern: DeploymentPattern) -> Result<Self, sqlx::Error> {
        let config = match pattern {
            DeploymentPattern::SingleProcess => PoolConfiguration::single_process(),
            DeploymentPattern::Distributed => PoolConfiguration::distributed(),
            DeploymentPattern::HighThroughput => PoolConfiguration::high_throughput(),
            DeploymentPattern::ResourceConstrained => PoolConfiguration::resource_constrained(),
            DeploymentPattern::Development => PoolConfiguration::development(),
        };

        Self::new_with_config(config).await
    }

    /// Create connection with custom configuration
    pub async fn new_with_config(config: PoolConfiguration) -> Result<Self, sqlx::Error> {
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://tasker:tasker@localhost/tasker_rust_development".to_string()
        });

        info!(
            "Initializing database pool for {:?} deployment with {} max connections",
            config.deployment_pattern, config.max_connections
        );

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .test_before_acquire(config.test_before_acquire)
            .connect(&database_url)
            .await?;

        info!(
            "Database pool initialized successfully: {} connections ({}..{} range), {}s timeout",
            pool.size(),
            config.min_connections,
            config.max_connections,
            config.acquire_timeout.as_secs()
        );

        Ok(Self { pool, config })
    }

    /// Get database pool reference
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get pool configuration
    pub fn config(&self) -> &PoolConfiguration {
        &self.config
    }

    /// Get detailed pool statistics for monitoring
    pub fn get_pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            size: self.pool.size(),
            num_idle: self.pool.num_idle() as u32,
            is_closed: self.pool.is_closed(),
            max_connections: self.config.max_connections,
            min_connections: self.config.min_connections,
            acquire_timeout_secs: self.config.acquire_timeout.as_secs(),
            deployment_pattern: self.config.deployment_pattern,
        }
    }

    /// Perform comprehensive health check including pool status
    pub async fn comprehensive_health_check(&self) -> Result<HealthCheckResult, sqlx::Error> {
        let start_time = std::time::Instant::now();
        
        // Basic connectivity test
        let connectivity_result = sqlx::query("SELECT 1 as health")
            .fetch_one(&self.pool)
            .await;

        let connectivity_duration = start_time.elapsed();
        
        match connectivity_result {
            Ok(_) => {
                let metrics = self.get_pool_metrics();
                Ok(HealthCheckResult {
                    healthy: true,
                    response_time_ms: connectivity_duration.as_millis() as u64,
                    pool_metrics: metrics,
                    error_message: None,
                })
            }
            Err(e) => {
                let metrics = self.get_pool_metrics();
                Ok(HealthCheckResult {
                    healthy: false,
                    response_time_ms: connectivity_duration.as_millis() as u64,
                    pool_metrics: metrics,
                    error_message: Some(e.to_string()),
                })
            }
        }
    }

    /// Gracefully close the connection pool
    pub async fn close(self) {
        info!(
            "Closing database pool for {:?} deployment",
            self.config.deployment_pattern
        );
        self.pool.close().await;
    }
}

/// Pool performance metrics for monitoring
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pub size: u32,
    pub num_idle: u32,
    pub is_closed: bool,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub deployment_pattern: DeploymentPattern,
}

impl PoolMetrics {
    /// Calculate pool utilization percentage
    pub fn utilization_percentage(&self) -> f64 {
        if self.max_connections == 0 {
            0.0
        } else {
            ((self.size - self.num_idle) as f64 / self.max_connections as f64) * 100.0
        }
    }

    /// Check if pool is under stress
    pub fn is_under_stress(&self) -> bool {
        self.utilization_percentage() > 80.0
    }

    /// Get pool efficiency score (0-100, higher is better)
    pub fn efficiency_score(&self) -> f64 {
        let utilization = self.utilization_percentage();
        let idle_ratio = self.num_idle as f64 / self.size as f64;
        
        // Optimal utilization is around 60-70%
        let utilization_score = if utilization < 60.0 {
            utilization / 60.0 * 100.0
        } else if utilization <= 70.0 {
            100.0
        } else {
            100.0 - ((utilization - 70.0) / 30.0 * 50.0)
        };

        // Balance utilization with having some idle connections
        (utilization_score * 0.8) + (idle_ratio * 20.0)
    }
}

/// Health check result with detailed pool information
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub response_time_ms: u64,
    pub pool_metrics: PoolMetrics,
    pub error_message: Option<String>,
}

impl HealthCheckResult {
    /// Check if system is performing optimally
    pub fn is_performing_optimally(&self) -> bool {
        self.healthy 
            && self.response_time_ms < 100 
            && !self.pool_metrics.is_under_stress()
    }

    /// Get performance status description
    pub fn performance_status(&self) -> String {
        if !self.healthy {
            "Unhealthy".to_string()
        } else if self.response_time_ms > 1000 {
            "Slow Response".to_string()
        } else if self.pool_metrics.is_under_stress() {
            "High Load".to_string()
        } else if self.pool_metrics.efficiency_score() > 80.0 {
            "Optimal".to_string()
        } else {
            "Good".to_string()
        }
    }
}
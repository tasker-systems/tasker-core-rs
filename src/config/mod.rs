//! # TaskerCore Configuration System
//!
//! This module provides comprehensive configuration management that mirrors the Ruby side's
//! YAML-based configuration approach. It eliminates hardcoded fallbacks and environment
//! variable dependencies in favor of explicit, validated configuration loading.
//!
//! ## Architecture
//!
//! - **Single Source of Truth**: All configuration comes from YAML files
//! - **Environment Awareness**: Supports development/test/production overrides
//! - **Explicit Validation**: No silent fallbacks or data corruption
//! - **Ruby Parity**: Mirrors Ruby side configuration structure exactly
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::config::ConfigManager;
//!
//! // Load configuration (environment auto-detected)
//! let config = ConfigManager::load()?;
//!
//! // Access configuration values
//! let database_url = config.database_url()?;
//! let pool_size = config.database.pool;
//! let timeout = config.execution.step_execution_timeout_seconds;
//! ```

pub mod error;
pub mod loader;
pub mod query_cache_config;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub use error::ConfigurationError;
pub use loader::ConfigManager;
pub use query_cache_config::{CacheTypeConfig, QueryCacheConfig, QueryCacheConfigLoader};

/// Root configuration structure mirroring tasker-config.yaml
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskerConfig {
    /// Authentication and authorization settings
    pub auth: AuthConfig,
    
    /// Database connection and pooling configuration
    pub database: DatabaseConfig,
    
    /// Telemetry and monitoring settings
    pub telemetry: TelemetryConfig,
    
    /// Task processing engine configuration
    pub engine: EngineConfig,
    
    /// TaskTemplate discovery configuration
    pub task_templates: TaskTemplatesConfig,
    
    /// Health monitoring configuration
    pub health: HealthConfig,
    
    /// Dependency graph processing settings
    pub dependency_graph: DependencyGraphConfig,
    
    /// System-wide settings
    pub system: SystemConfig,
    
    /// Backoff and retry configuration
    pub backoff: BackoffConfig,
    
    /// Task execution settings
    pub execution: ExecutionConfig,
    
    /// Task reenqueue configuration
    pub reenqueue: ReenqueueConfig,
    
    /// Event processing configuration
    pub events: EventsConfig,
    
    /// Caching configuration
    pub cache: CacheConfig,
    
    /// Query caching configuration - reuse existing QueryCacheConfig
    pub query_cache: QueryCacheConfig,
    
    /// PGMQ (PostgreSQL Message Queue) configuration
    pub pgmq: PgmqConfig,
    
    /// Orchestration system configuration
    pub orchestration: OrchestrationConfig,
}

/// Authentication and authorization configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub authentication_enabled: bool,
    pub strategy: String,
    pub current_user_method: String,
    pub authenticate_user_method: String,
    pub authorization_enabled: bool,
    pub authorization_coordinator_class: String,
}

/// Database connection and pooling configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub enable_secondary_database: bool,
    pub url: Option<String>,
    pub adapter: String,
    pub encoding: String,
    pub host: String,
    pub username: String,
    pub password: String,
    pub pool: u32,
    pub variables: DatabaseVariables,
    pub checkout_timeout: u64,
    pub reaping_frequency: u64,
    /// Environment-specific database name override
    pub database: Option<String>,
    /// Skip migration check on startup (useful for development/testing)
    #[serde(default)]
    pub skip_migration_check: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseVariables {
    pub statement_timeout: u64,
}

impl DatabaseConfig {
    /// Get database name for the current environment
    pub fn database_name(&self, environment: &str) -> String {
        // Use explicit database name if provided (from environment overrides)
        if let Some(db_name) = &self.database {
            return db_name.clone();
        }
        
        // Otherwise use environment-based naming convention
        match environment {
            "development" => "tasker_rust_development".to_string(),
            "test" => "tasker_rust_test".to_string(),
            "production" => std::env::var("POSTGRES_DB")
                .unwrap_or_else(|_| "tasker_production".to_string()),
            _ => format!("tasker_rust_{}", environment)
        }
    }
    
    /// Build complete database URL from configuration
    pub fn database_url(&self, environment: &str) -> String {
        // If URL is explicitly provided (with ${DATABASE_URL} expansion), use it
        if let Some(url) = &self.url {
            if url.starts_with("${DATABASE_URL}") {
                if let Ok(env_url) = std::env::var("DATABASE_URL") {
                    return env_url;
                }
            } else if !url.is_empty() && url != "${DATABASE_URL}" {
                return url.clone();
            }
        }
        
        // Build URL from components
        let port = std::env::var("DATABASE_PORT")
            .unwrap_or_else(|_| "5432".to_string());
            
        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username,
            self.password, 
            self.host,
            port,
            self.database_name(environment)
        )
    }
}

/// Telemetry and monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub service_name: String,
    pub sample_rate: f64,
}

/// Task processing engine configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EngineConfig {
    pub task_handler_directory: String,
    pub task_config_directory: String,
    pub identity_strategy: String,
    pub custom_events_directories: Vec<String>,
}

/// TaskTemplate discovery configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskTemplatesConfig {
    pub search_paths: Vec<String>,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthConfig {
    pub enabled: bool,
    pub check_interval_seconds: u64,
    pub alert_thresholds: AlertThresholds,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertThresholds {
    pub error_rate: f64,
    pub queue_depth: f64,
}

/// Dependency graph processing configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DependencyGraphConfig {
    pub max_depth: u32,
    pub cycle_detection_enabled: bool,
    pub optimization_enabled: bool,
}

/// System-wide configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemConfig {
    pub default_dependent_system: String,
    pub default_queue_name: String,
    pub version: String,                     // Updated to match Rust TASKER_CORE_VERSION
    // New constants unification fields
    pub max_recursion_depth: u32,           // Replaces hardcoded recursion limits
}

/// Backoff and retry configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackoffConfig {
    pub default_backoff_seconds: Vec<u64>,
    pub max_backoff_seconds: u64,
    pub backoff_multiplier: f64,
    pub jitter_enabled: bool,
    pub jitter_max_percentage: f64,
    pub reenqueue_delays: ReenqueueDelays,
    pub default_reenqueue_delay: u64,
    pub buffer_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReenqueueDelays {
    pub has_ready_steps: u64,
    pub waiting_for_dependencies: u64,
    pub processing: u64,
}

/// Task execution configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionConfig {
    pub processing_mode: String,
    pub max_concurrent_tasks: u32,
    pub max_concurrent_steps: u32,
    pub default_timeout_seconds: u64,
    pub step_execution_timeout_seconds: u64,
    pub environment: String,
    pub max_discovery_attempts: u32,
    pub step_batch_size: u32,
    // New constants unification fields
    pub max_retries: u32,                    // Replaces Ruby FALLBACK_MAX_RETRIES
    pub max_workflow_steps: u32,             // Replaces Rust constants::system::MAX_WORKFLOW_STEPS
    pub connection_timeout_seconds: u64,     // Replaces Ruby hardcoded API timeouts
}

impl ExecutionConfig {
    /// Get step execution timeout as Duration
    pub fn step_execution_timeout(&self) -> Duration {
        Duration::from_secs(self.step_execution_timeout_seconds)
    }
    
    /// Get default task timeout as Duration
    pub fn default_timeout(&self) -> Duration {
        Duration::from_secs(self.default_timeout_seconds)
    }
}

/// Task reenqueue configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReenqueueConfig {
    pub has_ready_steps: u64,
    pub waiting_for_dependencies: u64,
    pub processing: u64,
}

/// Event processing configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventsConfig {
    pub batch_size: u32,
    pub enabled: bool,
    pub batch_timeout_ms: u64,
}

/// Caching configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub ttl_seconds: u64,
    pub max_size: u32,
}

/// PGMQ (PostgreSQL Message Queue) configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PgmqConfig {
    pub poll_interval_ms: u64,
    pub visibility_timeout_seconds: u64,
    pub batch_size: u32,
    pub max_retries: u32,
    pub default_namespaces: Vec<String>,
    pub queue_naming_pattern: String,
    // New constants unification fields
    pub max_batch_size: u32,                // Replaces Ruby MAX_MESSAGE_COUNT constant
    pub shutdown_timeout_seconds: u64,      // Replaces Ruby FALLBACK_SHUTDOWN_TIMEOUT constant
}

impl PgmqConfig {
    /// Get poll interval as Duration
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }
    
    /// Get visibility timeout as Duration
    pub fn visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.visibility_timeout_seconds)
    }
    
    /// Generate queue name for a namespace
    pub fn queue_name_for_namespace(&self, namespace: &str) -> String {
        self.queue_naming_pattern.replace("{namespace}", namespace)
    }
}

/// Orchestration system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationConfig {
    pub mode: String,
    pub task_requests_queue_name: String,
    pub tasks_per_cycle: u32,
    pub cycle_interval_ms: u64,
    pub task_request_polling_interval_ms: u64,
    pub task_request_visibility_timeout_seconds: u64,
    pub task_request_batch_size: u32,
    pub active_namespaces: Vec<String>,
    pub max_concurrent_orchestrators: u32,
    pub enable_performance_logging: bool,
    pub default_claim_timeout_seconds: u64,
    pub queues: QueueConfig,
    pub embedded_orchestrator: EmbeddedOrchestratorConfig,
    pub enable_heartbeat: bool,
    pub heartbeat_interval_ms: u64,
}

impl OrchestrationConfig {
    /// Get cycle interval as Duration
    pub fn cycle_interval(&self) -> Duration {
        Duration::from_millis(self.cycle_interval_ms)
    }
    
    /// Get task request polling interval as Duration
    pub fn task_request_polling_interval(&self) -> Duration {
        Duration::from_millis(self.task_request_polling_interval_ms)
    }
    
    /// Get heartbeat interval as Duration
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }
    
    /// Get task request visibility timeout as Duration
    pub fn task_request_visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.task_request_visibility_timeout_seconds)
    }
    
    /// Get default claim timeout as Duration
    pub fn default_claim_timeout(&self) -> Duration {
        Duration::from_secs(self.default_claim_timeout_seconds)
    }
}

/// Queue configuration for orchestration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueConfig {
    pub task_requests: String,
    pub task_processing: String,
    pub batch_results: String,
    pub step_results: String,
    pub worker_queues: HashMap<String, String>,
    pub settings: QueueSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueSettings {
    pub visibility_timeout_seconds: u64,
    pub message_retention_seconds: u64,
    pub dead_letter_queue_enabled: bool,
    pub max_receive_count: u32,
}

/// Embedded orchestrator configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddedOrchestratorConfig {
    pub auto_start: bool,
    pub namespaces: Vec<String>,
    pub shutdown_timeout_seconds: u64,
}

impl EmbeddedOrchestratorConfig {
    /// Get shutdown timeout as Duration
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_seconds)
    }
}

impl TaskerConfig {
    /// Validate configuration for consistency and required fields
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        // Database configuration validation
        if self.database.host.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "database.host",
                "database configuration"
            ));
        }
        
        if self.database.username.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "database.username",
                "database configuration"
            ));
        }
        
        if self.database.pool == 0 {
            return Err(ConfigurationError::invalid_value(
                "database.pool",
                "0",
                "pool size must be greater than 0"
            ));
        }
        
        // Execution configuration validation
        if self.execution.environment.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "execution.environment",
                "execution configuration"
            ));
        }
        
        // PGMQ configuration validation
        if self.pgmq.batch_size == 0 {
            return Err(ConfigurationError::invalid_value(
                "pgmq.batch_size",
                "0",
                "batch size must be greater than 0"
            ));
        }
        
        // Orchestration configuration validation
        if self.orchestration.active_namespaces.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "orchestration.active_namespaces",
                "at least one active namespace must be configured"
            ));
        }
        
        Ok(())
    }
    
    /// Get database URL for the current environment
    pub fn database_url(&self) -> String {
        self.database.database_url(&self.execution.environment)
    }
    
    /// Check if running in test environment
    pub fn is_test_environment(&self) -> bool {
        self.execution.environment == "test"
    }
    
    /// Check if running in development environment
    pub fn is_development_environment(&self) -> bool {
        self.execution.environment == "development"
    }
    
    /// Check if running in production environment
    pub fn is_production_environment(&self) -> bool {
        self.execution.environment == "production"
    }
    
    // New accessor methods for constants unification
    
    /// Get maximum dependency depth
    pub fn max_dependency_depth(&self) -> usize {
        self.dependency_graph.max_depth as usize
    }
    
    /// Get maximum workflow steps
    pub fn max_workflow_steps(&self) -> usize {
        self.execution.max_workflow_steps as usize
    }
    
    /// Get system version string
    pub fn system_version(&self) -> &str {
        &self.system.version
    }
    
    /// Get connection timeout as Duration
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.execution.connection_timeout_seconds)
    }
    
    /// Get maximum retries
    pub fn max_retries(&self) -> u32 {
        self.execution.max_retries
    }
    
    /// Get maximum recursion depth
    pub fn max_recursion_depth(&self) -> usize {
        self.system.max_recursion_depth as usize
    }
    
    /// Get PGMQ shutdown timeout as Duration
    pub fn pgmq_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.pgmq.shutdown_timeout_seconds)
    }
    
    /// Get PGMQ maximum batch size
    pub fn pgmq_max_batch_size(&self) -> u32 {
        self.pgmq.max_batch_size
    }
    
    /// Validate cross-language configuration consistency between Rust and Ruby
    /// 
    /// This ensures that both Ruby and Rust sides use the same configuration values
    /// for critical system parameters that affect orchestration behavior.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if configuration is consistent, otherwise returns a `ConfigurationError`
    /// listing the inconsistencies found.
    pub fn validate_ruby_rust_consistency(&self) -> Result<(), ConfigurationError> {
        let mut inconsistencies = Vec::new();
        
        // Define expected Ruby-compatible defaults that should match
        let dependency_depth: u32 = 50;
        let max_steps: u32 = 1000;
        let max_retries: u32 = 3;
        let visibility_timeout: u64 = 30;
        let batch_size: u32 = 5;
        
        // Check dependency_graph.max_depth
        if self.dependency_graph.max_depth != dependency_depth {
            inconsistencies.push(format!(
                "dependency_graph.max_depth: Rust={}, Ruby Expected={}",
                self.dependency_graph.max_depth,
                dependency_depth
            ));
        }
        
        // Check execution.max_workflow_steps
        if self.execution.max_workflow_steps != max_steps {
            inconsistencies.push(format!(
                "execution.max_workflow_steps: Rust={}, Ruby Expected={}",
                self.execution.max_workflow_steps,
                max_steps
            ));
        }
        
        // Check execution.max_retries
        if self.execution.max_retries != max_retries {
            inconsistencies.push(format!(
                "execution.max_retries: Rust={}, Ruby Expected={}",
                self.execution.max_retries,
                max_retries
            ));
        }
        
        // Check pgmq.visibility_timeout_seconds
        if self.pgmq.visibility_timeout_seconds != visibility_timeout {
            inconsistencies.push(format!(
                "pgmq.visibility_timeout_seconds: Rust={}, Ruby Expected={}",
                self.pgmq.visibility_timeout_seconds,
                visibility_timeout
            ));
        }
        
        // Check pgmq.batch_size
        if self.pgmq.batch_size != batch_size {
            inconsistencies.push(format!(
                "pgmq.batch_size: Rust={}, Ruby Expected={}",
                self.pgmq.batch_size,
                batch_size
            ));
        }
        
        if !inconsistencies.is_empty() {
            return Err(ConfigurationError::invalid_value(
                "cross_language_consistency",
                &inconsistencies.join(", "),
                "Ruby and Rust configuration values must match for consistent behavior"
            ));
        }
        
        Ok(())
    }
    
    /// Get configuration warnings for potential cross-language issues
    /// 
    /// This provides non-fatal warnings about configuration that might cause
    /// issues in cross-language coordination between Ruby and Rust components.
    /// 
    /// # Returns
    /// 
    /// Vector of warning messages describing potential configuration issues.
    pub fn cross_language_configuration_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        
        // Check for environment-specific concerns
        match self.execution.environment.as_str() {
            "test" => {
                if self.dependency_graph.max_depth > 20 {
                    warnings.push(format!(
                        "dependency_graph.max_depth={} is high for test environment, may slow test execution",
                        self.dependency_graph.max_depth
                    ));
                }
                
                if self.execution.max_workflow_steps > 500 {
                    warnings.push(format!(
                        "execution.max_workflow_steps={} is high for test environment, may cause test timeouts",
                        self.execution.max_workflow_steps
                    ));
                }
                
                if self.pgmq.poll_interval_ms < 100 {
                    warnings.push(format!(
                        "pgmq.poll_interval_ms={}ms is very low, may cause high CPU usage in tests",
                        self.pgmq.poll_interval_ms
                    ));
                }
            },
            "production" => {
                if self.pgmq.poll_interval_ms < 200 {
                    warnings.push(format!(
                        "pgmq.poll_interval_ms={}ms is low for production, may cause high CPU usage",
                        self.pgmq.poll_interval_ms
                    ));
                }
                
                if self.execution.max_retries < 2 {
                    warnings.push(format!(
                        "execution.max_retries={} is low for production, may cause permanent failures for transient errors",
                        self.execution.max_retries
                    ));
                }
                
                if self.execution.max_workflow_steps > 10000 {
                    warnings.push(format!(
                        "execution.max_workflow_steps={} is very high, may cause memory issues",
                        self.execution.max_workflow_steps
                    ));
                }
            },
            "development" => {
                // Development warnings are generally less strict
                if self.pgmq.poll_interval_ms < 100 {
                    warnings.push(format!(
                        "pgmq.poll_interval_ms={}ms is low for development, may impact debugging experience",
                        self.pgmq.poll_interval_ms
                    ));
                }
            },
            _ => {
                warnings.push(format!(
                    "Unknown environment '{}', configuration validation may not be accurate",
                    self.execution.environment
                ));
            }
        }
        
        // Check for missing configuration that Ruby expects
        if self.system.version != "0.1.0" {
            warnings.push(format!(
                "system.version='{}' differs from expected Ruby version '0.1.0', may cause version mismatch issues",
                self.system.version
            ));
        }
        
        warnings
    }
}

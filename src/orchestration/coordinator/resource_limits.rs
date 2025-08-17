//! # System Resource Limits Detection and Validation
//!
//! Implements TAS-34 Phase 1: Resource Constraint Validation to prevent database pool exhaustion
//! and ensure system stability by validating that total executor configurations don't exceed
//! available system resources.

use crate::config::{ConfigManager, ExecutorInstanceConfig};
use crate::error::Result;
use crate::orchestration::executor::traits::ExecutorType;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use sysinfo::System;
use tracing::{debug, error, info, warn};

/// Configuration for resource validation enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceValidatorConfig {
    /// Whether to enforce minimum resource requirements
    pub enforce_minimum_resources: bool,
    /// Whether to enforce maximum resource requirements  
    pub enforce_maximum_resources: bool,
    /// Whether to warn on suboptimal configurations
    pub warn_on_suboptimal: bool,
    /// Failure mode when validation fails
    pub failure_mode: FailureMode,
}

impl Default for ResourceValidatorConfig {
    fn default() -> Self {
        Self {
            enforce_minimum_resources: false,
            enforce_maximum_resources: false,
            warn_on_suboptimal: true,
            failure_mode: FailureMode::BestEffort,
        }
    }
}

/// Failure mode for resource validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureMode {
    /// Stop immediately when validation fails
    FailFast,
    /// Run with reduced capacity when validation fails
    Degraded,
    /// Try to run anyway despite validation failures
    BestEffort,
}

/// System resource limits and availability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResourceLimits {
    /// Maximum database connections available
    pub max_database_connections: u32,
    /// Currently active database connections
    pub active_database_connections: u32,
    /// Available database connections for new executors
    pub available_database_connections: u32,
    /// Reserve connections for system operations (migrations, health checks, etc.)
    pub reserved_database_connections: u32,
    /// Maximum memory available (in MB)
    pub max_memory_mb: Option<u64>,
    /// Available memory for new executors (in MB)
    pub available_memory_mb: Option<u64>,
    /// CPU cores available (informational only - not used for executor limiting)
    ///
    /// Note: With Tokio's M:N threading model, CPU cores don't directly limit
    /// the number of async executors we can run. Tokio creates a small number
    /// of system threads (typically 1 per core) and schedules thousands of
    /// async tasks on them. Our executors are async tasks that yield at .await
    /// points, so the real constraint is database connections, not CPU cores.
    pub cpu_cores: Option<u32>,
    /// Detection timestamp
    pub detected_at: chrono::DateTime<chrono::Utc>,
    /// Warnings about resource constraints
    pub warnings: Vec<String>,
}

impl SystemResourceLimits {
    /// Analyze configured database pool size and system environment
    ///
    /// Simplified approach that trusts the configured database pool size rather than
    /// attempting unreliable runtime detection of connection availability.
    pub async fn detect(_database_pool: &PgPool, config_manager: &ConfigManager) -> Result<Self> {
        info!("INFO: RESOURCE_LIMITS: Analyzing configured database pool and system resources");

        let mut warnings = Vec::new();

        // Use configured database pool size (trust the configuration)
        let configured_pool_size = config_manager.config().database.pool;
        let max_database_connections = configured_pool_size;

        // No longer attempt to detect "active" connections - SQLx manages this
        let active_database_connections = 0; // Not reliably detectable, set to 0

        // Reserve connections for system operations (migrations, health checks, etc.)
        // Use dynamic reservation calculation based on pool size
        let reserved_database_connections =
            Self::calculate_reserved_connections(max_database_connections);

        // Available connections = total - reserved (trust SQLx to manage actual usage)
        let available_database_connections =
            max_database_connections.saturating_sub(reserved_database_connections);

        info!(
            "Database configuration - Max pool: {}, Reserved: {}, Available for executors: {}",
            max_database_connections, reserved_database_connections, available_database_connections
        );

        // Warn if configured database pool is small
        if max_database_connections < 10 {
            warnings.push(format!(
                "Configured database pool size ({max_database_connections}) is small - consider increasing for production workloads"
            ));
        }

        // Detect system memory (optional - best effort)
        let (max_memory_mb, available_memory_mb) = Self::detect_memory_limits();

        // Detect CPU cores (optional - best effort)
        let cpu_cores = Self::detect_cpu_cores();

        let resource_limits = Self {
            max_database_connections,
            active_database_connections,
            available_database_connections,
            reserved_database_connections,
            max_memory_mb,
            available_memory_mb,
            cpu_cores,
            detected_at: chrono::Utc::now(),
            warnings,
        };

        info!(
            "SUCCESS: RESOURCE_LIMITS: Detected limits - DB connections: {}/{}, Memory: {}/{}MB, CPUs: {}",
            resource_limits.available_database_connections,
            resource_limits.max_database_connections,
            resource_limits.available_memory_mb.unwrap_or(0),
            resource_limits.max_memory_mb.unwrap_or(0),
            resource_limits.cpu_cores.unwrap_or(0)
        );

        Ok(resource_limits)
    }

    /// Detect memory limits using sysinfo crate
    fn detect_memory_limits() -> (Option<u64>, Option<u64>) {
        let mut sys = System::new_all();
        sys.refresh_memory();

        // Get total memory in bytes, convert to MB
        let total_memory_mb = sys.total_memory() / (1024 * 1024);

        // Get available memory in bytes, convert to MB
        // Note: available_memory() might report very low values on macOS due to cache/buffer accounting
        let available_memory_mb = sys.available_memory() / (1024 * 1024);
        let free_memory_mb = sys.free_memory() / (1024 * 1024);
        let used_memory_mb = sys.used_memory() / (1024 * 1024);

        // On macOS and some Linux systems, "available" includes cache/buffers as unavailable
        // Calculate a more realistic available memory estimate:
        // 1. If available_memory is reported correctly (reasonable %), use it
        // 2. Otherwise, estimate as: total - used + reasonable buffer space
        let available_percentage = (available_memory_mb as f64 / total_memory_mb as f64) * 100.0;

        let effective_available_mb = if available_percentage > 20.0 {
            // available_memory seems reasonable, use it
            available_memory_mb
        } else {
            // available_memory seems too conservative, estimate more realistically
            // Assume we can use 70% of total memory, minus what's actually used by processes
            let conservative_total = (total_memory_mb as f64 * 0.7) as u64;
            let realistic_available = conservative_total.saturating_sub(used_memory_mb);

            debug!(
                "Adjusting memory calculation: available_memory ({} MB, {:.1}%) seems too low, \
                using conservative estimate ({} MB)",
                available_memory_mb, available_percentage, realistic_available
            );

            realistic_available
        };

        debug!(
            "Memory detection - Total: {} MB, Used: {} MB, Available: {} MB, Free: {} MB, \
            Effective Available: {} MB ({:.1}% of total)",
            total_memory_mb,
            used_memory_mb,
            available_memory_mb,
            free_memory_mb,
            effective_available_mb,
            (effective_available_mb as f64 / total_memory_mb as f64) * 100.0
        );

        (Some(total_memory_mb), Some(effective_available_mb))
    }

    /// Detect CPU cores (informational only)
    ///
    /// This information is used for monitoring and system characterization,
    /// but NOT for limiting the number of executors. In Tokio's async model,
    /// many async tasks (executors) can run efficiently on a few system threads.
    fn detect_cpu_cores() -> Option<u32> {
        std::thread::available_parallelism()
            .ok()
            .map(|p| p.get() as u32)
    }

    /// Calculate reserved connections based on pool size
    ///
    /// Implements dynamic reservation logic that scales appropriately for large pools.
    /// This fixes the issue where small pools (capped at 10) were insufficient for
    /// large database deployments.
    fn calculate_reserved_connections(max_connections: u32) -> u32 {
        let base_reserve = (max_connections as f32 * 0.2).round() as u32;

        match max_connections {
            0..=20 => base_reserve.max(2),                    // Small pools: min 2
            21..=50 => base_reserve.clamp(3, 10),             // Medium pools: 3-10
            51..=100 => base_reserve.clamp(5, 20),            // Large pools: 5-20
            _ => base_reserve.clamp(10, max_connections / 4), // XL pools: up to 25%
        }
    }

    /// Validate executor configuration against resource limits
    ///
    /// This implements the core validation logic from TAS-34 Phase 1.
    ///
    /// ## Resource Validation Philosophy
    ///
    /// This validation focuses on **real bottlenecks** that can cause system failures:
    ///
    /// ### Database Connections (Primary Constraint)
    /// - Each executor needs a database connection to operate
    /// - Database pool exhaustion causes immediate failures
    /// - We validate min/max executors against available connections
    ///
    /// ### Memory (Secondary Constraint)
    /// - Estimate memory usage based on executor count
    /// - Warn if memory pressure may occur under full load
    ///
    /// ### CPU Cores (Informational Only)
    /// - **NOT used for executor limiting** - this is critical!
    /// - Tokio uses M:N threading: many async tasks run on few system threads
    /// - Our executors are async tasks that yield at `.await` points
    /// - Tokio's scheduler handles the mapping of tasks to system threads
    /// - CPU core count is tracked only for monitoring/characterization
    ///
    /// This approach respects Tokio's design rather than trying to manage
    /// threading concerns that Tokio already handles efficiently.
    pub fn validate_executor_configuration(
        &self,
        config_manager: &ConfigManager,
    ) -> Result<ValidationResult> {
        info!(
            "INFO: RESOURCE_LIMITS: Analyzing executor configuration against detected resource limits"
        );
        info!("INFO: RESOURCE_LIMITS: Detection is best-effort - use for deployment guidance, not startup blocking");

        let mut validation_errors = Vec::new();
        let mut validation_warnings = Vec::new();

        // Calculate total executor requirements
        let executor_requirements = self.calculate_executor_requirements(config_manager)?;

        // Validate database connections
        let total_max_executors = executor_requirements.total_max_executors;
        let total_min_executors = executor_requirements.total_min_executors;

        info!(
            "Executor requirements - Min: {}, Max: {}, Available DB connections: {}",
            total_min_executors, total_max_executors, self.available_database_connections
        );

        // Configuration issue: minimum executors exceed available connections
        if total_min_executors > self.available_database_connections {
            validation_errors.push(format!(
                "CONFIG: Minimum executor count ({}) exceeds detected available database connections ({}). \
                Consider increasing database pool size or reducing min_executors.",
                total_min_executors,
                self.available_database_connections
            ));
        }

        // Configuration issue: maximum executors exceed total pool size
        if total_max_executors > self.max_database_connections {
            validation_errors.push(format!(
                "CONFIG: Maximum executor count ({}) exceeds detected database pool size ({}). \
                Consider increasing database pool size or reducing max_executors.",
                total_max_executors, self.max_database_connections
            ));
        }

        // Warning: maximum executors exceed available connections
        if total_max_executors > self.available_database_connections {
            validation_warnings.push(format!(
                "WARNING: Maximum executor count ({}) exceeds currently available database connections ({}). \
                System may experience connection pressure under full load.",
                total_max_executors,
                self.available_database_connections
            ));
        }

        // Warning: high resource utilization
        let max_utilization = total_max_executors as f32 / self.max_database_connections as f32;
        if max_utilization > 0.8 {
            validation_warnings.push(format!(
                "WARNING: Maximum executor configuration would use {:.1}% of database pool. \
                Consider reducing executor limits or increasing database pool size.",
                max_utilization * 100.0
            ));
        }

        // Memory-based warnings (if available)
        if let (Some(max_memory), Some(available_memory)) =
            (self.max_memory_mb, self.available_memory_mb)
        {
            // Estimate memory per executor (rough estimate: 50MB per executor)
            let estimated_memory_per_executor = 50;
            let estimated_total_memory = total_max_executors * estimated_memory_per_executor;

            if estimated_total_memory > available_memory as u32 {
                validation_warnings.push(format!(
                    "WARNING: Estimated memory usage ({estimated_total_memory} MB) \
                    exceeds available memory ({available_memory} MB). \
                    System may experience memory pressure under full load."
                ));
            }

            // Warn if available memory is low
            if available_memory < 500 {
                validation_warnings.push(format!(
                    "WARNING: Available memory is very low ({available_memory} MB). \
                    This may impact system performance."
                ));
            }

            // Log memory information
            info!(
                "Memory analysis - Total: {} MB, Available: {} MB, Estimated usage: {} MB",
                max_memory, available_memory, estimated_total_memory
            );
        }

        // Validate individual pool configurations
        for (executor_type, requirements) in &executor_requirements.per_type_requirements {
            if requirements.max_executors == 0 {
                validation_warnings.push(format!(
                    "WARNING: {} has max_executors = 0, no executors will be available",
                    executor_type.name()
                ));
            }

            if requirements.min_executors > requirements.max_executors {
                validation_errors.push(format!(
                    "CONFIG: {} min_executors ({}) exceeds max_executors ({}) - invalid configuration",
                    executor_type.name(),
                    requirements.min_executors,
                    requirements.max_executors
                ));
            }
        }

        let is_valid = validation_errors.is_empty();
        let validation_result = ValidationResult {
            is_valid,
            validation_errors,
            validation_warnings,
            executor_requirements,
            resource_limits: self.clone(),
        };

        if is_valid {
            info!("SUCCESS: RESOURCE_LIMITS: Configuration analysis complete - no issues detected");
        } else {
            info!(
                "INFO: RESOURCE_LIMITS: Configuration analysis complete - {} configuration issues detected",
                validation_result.validation_errors.len()
            );
            for error in &validation_result.validation_errors {
                info!("  - {}", error);
            }
        }

        if !validation_result.validation_warnings.is_empty() {
            info!(
                "INFO: RESOURCE_LIMITS: {} recommendations for optimization",
                validation_result.validation_warnings.len()
            );
            for warning in &validation_result.validation_warnings {
                info!("  - {}", warning);
            }
        }

        Ok(validation_result)
    }

    /// Calculate executor resource requirements from configuration
    fn calculate_executor_requirements(
        &self,
        config_manager: &ConfigManager,
    ) -> Result<ExecutorRequirements> {
        let mut per_type_requirements = HashMap::new();
        let mut total_min_executors = 0;
        let mut total_max_executors = 0;

        for executor_type in ExecutorType::all() {
            let executor_config = config_manager
                .config()
                .get_executor_instance_config(executor_type);

            let requirements = ExecutorTypeRequirements {
                executor_type,
                min_executors: executor_config.min_executors as u32,
                max_executors: executor_config.max_executors as u32,
                config: executor_config,
            };

            total_min_executors += requirements.min_executors;
            total_max_executors += requirements.max_executors;

            per_type_requirements.insert(executor_type, requirements);
        }

        Ok(ExecutorRequirements {
            total_min_executors,
            total_max_executors,
            per_type_requirements,
        })
    }
}

/// Executor resource requirements calculated from configuration
#[derive(Debug, Clone)]
pub struct ExecutorRequirements {
    pub total_min_executors: u32,
    pub total_max_executors: u32,
    pub per_type_requirements: HashMap<ExecutorType, ExecutorTypeRequirements>,
}

/// Resource requirements for a specific executor type
#[derive(Debug, Clone)]
pub struct ExecutorTypeRequirements {
    pub executor_type: ExecutorType,
    pub min_executors: u32,
    pub max_executors: u32,
    pub config: ExecutorInstanceConfig,
}

/// Result of resource validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the configuration passes validation
    pub is_valid: bool,
    /// Critical errors that prevent system startup
    pub validation_errors: Vec<String>,
    /// Warnings about resource constraints
    pub validation_warnings: Vec<String>,
    /// Calculated executor requirements
    pub executor_requirements: ExecutorRequirements,
    /// Detected resource limits
    pub resource_limits: SystemResourceLimits,
}

impl ValidationResult {
    /// Get a formatted summary of validation results
    pub fn summary(&self) -> String {
        let mut summary = vec![];

        summary.push(format!(
            "Configuration: {} (Min: {}, Max: {} executors)",
            if self.is_valid { "VALID" } else { "INVALID" },
            self.executor_requirements.total_min_executors,
            self.executor_requirements.total_max_executors
        ));

        summary.push(format!(
            "Resources: {} available DB connections ({} total, {} reserved)",
            self.resource_limits.available_database_connections,
            self.resource_limits.max_database_connections,
            self.resource_limits.reserved_database_connections
        ));

        if !self.validation_errors.is_empty() {
            summary.push(format!("ERRORS ({})", self.validation_errors.len()));
            for error in &self.validation_errors {
                summary.push(format!("  - {error}"));
            }
        }

        if !self.validation_warnings.is_empty() {
            summary.push(format!("WARNINGS ({})", self.validation_warnings.len()));
            for warning in &self.validation_warnings {
                summary.push(format!("  - {warning}"));
            }
        }

        summary.join("\n")
    }

    /// Check if configuration should fail startup based on validation config
    ///
    /// This method now supports configurable enforcement based on the provided
    /// resource validation configuration. Enforcement can be disabled, enabled
    /// for critical errors only, or enabled with different failure modes.
    pub fn should_fail_startup(&self, config: &ResourceValidatorConfig) -> bool {
        match config.failure_mode {
            FailureMode::FailFast => !self.validation_errors.is_empty(),
            FailureMode::Degraded => self.has_critical_errors(),
            FailureMode::BestEffort => false,
        }
    }

    /// Check if configuration should fail startup (legacy method for backwards compatibility)
    ///
    /// NOTE: Always returns false - resource validation is informational only by default.
    /// Use should_fail_startup(&config) for configurable enforcement.
    pub fn should_fail_startup_legacy(&self) -> bool {
        false // Always allow startup - validation is informational only
    }

    /// Check if the validation result contains critical errors
    ///
    /// Critical errors are those that would likely cause immediate system failure,
    /// such as minimum executor requirements exceeding available resources.
    pub fn has_critical_errors(&self) -> bool {
        self.validation_errors.iter().any(|error| {
            // Consider errors critical if they relate to minimum requirements
            error.contains("minimum executor count")
                || error.contains("exceeds detected database pool size")
                || error.contains("min_executors") && error.contains("exceeds max_executors")
        })
    }

    /// Get recommended database pool size for this configuration
    pub fn recommended_database_pool_size(&self) -> u32 {
        // Recommend max executors + 20% buffer + reserved connections
        let buffer = (self.executor_requirements.total_max_executors as f32 * 0.2).ceil() as u32;
        self.executor_requirements.total_max_executors
            + buffer
            + self.resource_limits.reserved_database_connections
    }
}

/// Resource limits validator for use in coordinator startup
pub struct ResourceValidator {
    resource_limits: SystemResourceLimits,
    config_manager: Arc<ConfigManager>,
    validation_config: ResourceValidatorConfig,
}

impl ResourceValidator {
    /// Create a new resource validator with default configuration
    pub async fn new(database_pool: &PgPool, config_manager: Arc<ConfigManager>) -> Result<Self> {
        let resource_limits = SystemResourceLimits::detect(database_pool, &config_manager).await?;

        Ok(Self {
            resource_limits,
            config_manager,
            validation_config: ResourceValidatorConfig::default(),
        })
    }

    /// Create a new resource validator with custom configuration
    pub async fn new_with_config(
        database_pool: &PgPool,
        config_manager: Arc<ConfigManager>,
        validation_config: ResourceValidatorConfig,
    ) -> Result<Self> {
        let resource_limits = SystemResourceLimits::detect(database_pool, &config_manager).await?;

        Ok(Self {
            resource_limits,
            config_manager,
            validation_config,
        })
    }

    /// Perform validation and log informational summary (does not block startup)
    ///
    /// Resource validation is now informational only. This provides visibility into
    /// resource configuration vs. system capacity without blocking startup.
    /// Resource tuning should be handled at deployment time based on these recommendations.
    pub async fn validate_and_log_info(&self) -> Result<ValidationResult> {
        let validation_result = self
            .resource_limits
            .validate_executor_configuration(&self.config_manager)?;

        info!(
            "INFO: RESOURCE_VALIDATOR: Configuration analysis completed - Config Issues: {}, Recommendations: {}",
            validation_result.validation_errors.len(),
            validation_result.validation_warnings.len()
        );

        // Add reliability warning about detection accuracy
        info!("INFO: RESOURCE_VALIDATOR: Resource detection is best-effort and may be inaccurate on some systems");
        info!("INFO: RESOURCE_VALIDATOR: Use this information for deployment tuning, not startup validation");

        // Log validation summary as informational
        for line in validation_result.summary().lines() {
            if validation_result.validation_errors.is_empty() {
                info!("DATA: RESOURCE_INFO: {}", line);
            } else {
                warn!("DATA: RESOURCE_INFO: {}", line);
            }
        }

        // Always succeed - validation is informational only
        Ok(validation_result)
    }

    /// Perform validation with configurable enforcement
    ///
    /// This method respects the validator's configuration for enforcement.
    /// Unlike validate_and_log_info(), this method can fail startup when
    /// configured to do so based on the validation results.
    pub async fn validate_with_enforcement(&self) -> Result<ValidationResult> {
        let validation_result = self
            .resource_limits
            .validate_executor_configuration(&self.config_manager)?;

        // Log validation results based on configuration
        let log_level = if validation_result.validation_errors.is_empty() {
            "INFO"
        } else {
            match self.validation_config.failure_mode {
                FailureMode::FailFast => "ERROR",
                FailureMode::Degraded => "WARN",
                FailureMode::BestEffort => "INFO",
            }
        };

        info!(
            "{}: RESOURCE_VALIDATOR: Configuration validation completed - Config Issues: {}, Recommendations: {}, Enforcement: {:?}",
            log_level,
            validation_result.validation_errors.len(),
            validation_result.validation_warnings.len(),
            self.validation_config.failure_mode
        );

        // Log validation summary with appropriate level
        for line in validation_result.summary().lines() {
            match log_level {
                "ERROR" => error!("RESOURCE_VALIDATION: {}", line),
                "WARN" => warn!("RESOURCE_VALIDATION: {}", line),
                _ => info!("RESOURCE_VALIDATION: {}", line),
            }
        }

        // Check if we should fail startup based on configuration
        if validation_result.should_fail_startup(&self.validation_config) {
            let error_msg = format!(
                "Resource validation failed with enforcement mode {:?}: {} configuration errors detected",
                self.validation_config.failure_mode,
                validation_result.validation_errors.len()
            );
            error!("STARTUP_FAILURE: {}", error_msg);
            return Err(crate::error::TaskerError::Configuration(error_msg));
        }

        Ok(validation_result)
    }

    /// Get the current validation configuration
    pub fn validation_config(&self) -> &ResourceValidatorConfig {
        &self.validation_config
    }

    /// Update the validation configuration
    pub fn set_validation_config(&mut self, config: ResourceValidatorConfig) {
        self.validation_config = config;
    }

    /// Get detected resource limits
    pub fn resource_limits(&self) -> &SystemResourceLimits {
        &self.resource_limits
    }

    /// Refresh resource limits (re-detect)
    pub async fn refresh(&mut self, database_pool: &PgPool) -> Result<()> {
        info!("REFRESH: RESOURCE_VALIDATOR: Refreshing resource limits detection");
        self.resource_limits =
            SystemResourceLimits::detect(database_pool, &self.config_manager).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Database connectivity tests removed as they test I/O rather than business logic.
    // These tests were creating temp directories and database connections just to test
    // configuration loading, which is already covered by config module tests.
    //
    // The actual business logic (validation calculations, resource limit detection)
    // is tested in the remaining tests without external dependencies.

    #[test]
    fn test_validation_result_summary() {
        let resource_limits = SystemResourceLimits {
            max_database_connections: 25,
            active_database_connections: 5,
            available_database_connections: 15,
            reserved_database_connections: 5,
            max_memory_mb: Some(1024),
            available_memory_mb: Some(512),
            cpu_cores: Some(4),
            detected_at: chrono::Utc::now(),
            warnings: vec![],
        };

        let executor_requirements = ExecutorRequirements {
            total_min_executors: 8,
            total_max_executors: 38,
            per_type_requirements: HashMap::new(),
        };

        let validation_result = ValidationResult {
            is_valid: false,
            validation_errors: vec![
                "Maximum executor count (38) exceeds total database pool size (25)".to_string(),
            ],
            validation_warnings: vec!["High resource utilization detected".to_string()],
            executor_requirements,
            resource_limits,
        };

        let summary = validation_result.summary();
        assert!(summary.contains("INVALID"));
        assert!(summary.contains("Min: 8, Max: 38"));
        assert!(summary.contains("ERRORS (1)"));
        assert!(summary.contains("WARNINGS (1)"));
        assert!(summary.contains("Maximum executor count"));
    }

    #[test]
    fn test_recommended_database_pool_size() {
        let resource_limits = SystemResourceLimits {
            max_database_connections: 25,
            active_database_connections: 5,
            available_database_connections: 15,
            reserved_database_connections: 5,
            max_memory_mb: None,
            available_memory_mb: None,
            cpu_cores: None,
            detected_at: chrono::Utc::now(),
            warnings: vec![],
        };

        let executor_requirements = ExecutorRequirements {
            total_min_executors: 8,
            total_max_executors: 38,
            per_type_requirements: HashMap::new(),
        };

        let validation_result = ValidationResult {
            is_valid: false,
            validation_errors: vec![],
            validation_warnings: vec![],
            executor_requirements,
            resource_limits,
        };

        let recommended_size = validation_result.recommended_database_pool_size();

        // Should be: 38 executors + 20% buffer (8) + 5 reserved = 51
        assert_eq!(recommended_size, 51);
    }

    #[test]
    fn test_dynamic_reserved_connections_calculation() {
        // Test small pools (0-20 connections)
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(5), 2); // min 2
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(10), 2); // 20% = 2
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(15), 3); // 20% = 3
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(20), 4); // 20% = 4

        // Test medium pools (21-50 connections)
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(25), 5); // 20% = 5
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(40), 8); // 20% = 8
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(50), 10); // 20% = 10, clamped to max 10

        // Test large pools (51-100 connections)
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(60), 12); // 20% = 12
        assert_eq!(SystemResourceLimits::calculate_reserved_connections(80), 16); // 20% = 16
        assert_eq!(
            SystemResourceLimits::calculate_reserved_connections(100),
            20
        ); // 20% = 20, clamped to max 20

        // Test extra large pools (>100 connections)
        assert_eq!(
            SystemResourceLimits::calculate_reserved_connections(120),
            24
        ); // 20% = 24
        assert_eq!(
            SystemResourceLimits::calculate_reserved_connections(200),
            40
        ); // 20% = 40
        assert_eq!(
            SystemResourceLimits::calculate_reserved_connections(400),
            80
        ); // 20% = 80, within 25% cap of 100
        assert_eq!(
            SystemResourceLimits::calculate_reserved_connections(800),
            160
        ); // 20% = 160, within 25% cap of 200

        // For very large pools, we cap at 25% of total connections
        assert_eq!(
            SystemResourceLimits::calculate_reserved_connections(1000),
            200
        ); // 20% = 200, within 25% cap of 250
        assert_eq!(
            SystemResourceLimits::calculate_reserved_connections(2000),
            400
        ); // 20% = 400, within 25% cap of 500
    }

    #[test]
    fn test_configurable_enforcement() {
        // Create validation result with errors
        let resource_limits = SystemResourceLimits {
            max_database_connections: 10,
            active_database_connections: 0,
            available_database_connections: 8,
            reserved_database_connections: 2,
            max_memory_mb: None,
            available_memory_mb: None,
            cpu_cores: None,
            detected_at: chrono::Utc::now(),
            warnings: vec![],
        };

        let executor_requirements = ExecutorRequirements {
            total_min_executors: 15, // Exceeds available connections (8)
            total_max_executors: 20,
            per_type_requirements: HashMap::new(),
        };

        let validation_result = ValidationResult {
            is_valid: false,
            validation_errors: vec![
                "minimum executor count (15) exceeds detected available database connections (8)"
                    .to_string(),
                "Maximum executor count (20) exceeds detected database pool size (10)".to_string(),
            ],
            validation_warnings: vec![],
            executor_requirements: executor_requirements.clone(),
            resource_limits: resource_limits.clone(),
        };

        // Test BestEffort mode - should never fail
        let best_effort_config = ResourceValidatorConfig {
            enforce_minimum_resources: false,
            enforce_maximum_resources: false,
            warn_on_suboptimal: true,
            failure_mode: FailureMode::BestEffort,
        };
        assert!(!validation_result.should_fail_startup(&best_effort_config));

        // Test FailFast mode - should fail when there are any errors
        let fail_fast_config = ResourceValidatorConfig {
            enforce_minimum_resources: true,
            enforce_maximum_resources: true,
            warn_on_suboptimal: true,
            failure_mode: FailureMode::FailFast,
        };
        assert!(validation_result.should_fail_startup(&fail_fast_config));

        // Test Degraded mode - should fail only on critical errors
        let degraded_config = ResourceValidatorConfig {
            enforce_minimum_resources: true,
            enforce_maximum_resources: false,
            warn_on_suboptimal: true,
            failure_mode: FailureMode::Degraded,
        };
        assert!(validation_result.should_fail_startup(&degraded_config)); // Should fail due to minimum executor count error

        // Test has_critical_errors
        assert!(validation_result.has_critical_errors()); // minimum executor count exceeds available connections

        // Test validation result without critical errors
        let non_critical_validation = ValidationResult {
            is_valid: false,
            validation_errors: vec!["Some non-critical configuration issue".to_string()],
            validation_warnings: vec![],
            executor_requirements: executor_requirements.clone(),
            resource_limits: resource_limits.clone(),
        };

        assert!(!non_critical_validation.has_critical_errors());
        assert!(!non_critical_validation.should_fail_startup(&degraded_config));
        // Should not fail on non-critical errors
    }
}

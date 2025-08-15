//! # System Resource Limits Detection and Validation
//!
//! Implements TAS-34 Phase 1: Resource Constraint Validation to prevent database pool exhaustion
//! and ensure system stability by validating that total executor configurations don't exceed
//! available system resources.

use crate::config::{ConfigManager, ExecutorInstanceConfig};
use crate::error::{Result, TaskerError};
use crate::orchestration::executor::traits::ExecutorType;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use sysinfo::System;
use tracing::{debug, error, info, warn};

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
    /// Detect system resource limits from database pool and system environment
    ///
    /// This is the core function implementing TAS-34 Phase 1 resource detection.
    /// It analyzes available database connections and system resources to prevent
    /// configuration errors that would cause pool exhaustion.
    pub async fn detect(database_pool: &PgPool) -> Result<Self> {
        info!("🔍 RESOURCE_LIMITS: Detecting system resource limits for validation");

        let mut warnings = Vec::new();

        // Detect database connection limits
        let max_database_connections = database_pool.size();
        let active_database_connections = database_pool.size() - database_pool.num_idle() as u32;

        // Reserve 20% of connections for system operations (migrations, health checks, etc.)
        // but ensure at least 2 connections are reserved and at most 10
        let reserved_database_connections =
            ((max_database_connections as f32 * 0.2).round() as u32).clamp(2, 10);

        let available_database_connections = max_database_connections
            .saturating_sub(active_database_connections)
            .saturating_sub(reserved_database_connections);

        debug!(
            "Database connections - Max: {}, Active: {}, Reserved: {}, Available: {}",
            max_database_connections,
            active_database_connections,
            reserved_database_connections,
            available_database_connections
        );

        // Warn if database pool is small
        if max_database_connections < 10 {
            warnings.push(format!(
                "Database pool size ({max_database_connections}) is very small and may cause connection exhaustion"
            ));
        }

        // Warn if database pool utilization is already high
        let utilization = active_database_connections as f32 / max_database_connections as f32;
        if utilization > 0.8 {
            warnings.push(format!(
                "Database pool utilization is high ({:.1}%) - may impact performance",
                utilization * 100.0
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
            "✅ RESOURCE_LIMITS: Detected limits - DB connections: {}/{}, Memory: {}/{}MB, CPUs: {}",
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
        // Note: available_memory() might report 0 on some systems
        // In that case, use free_memory() as a fallback
        let available_memory_mb = sys.available_memory() / (1024 * 1024);
        let free_memory_mb = sys.free_memory() / (1024 * 1024);

        // Use the larger of available or free memory (some systems report one as 0)
        let effective_available_mb = available_memory_mb.max(free_memory_mb);

        debug!(
            "Memory detection - Total: {} MB, Available: {} MB, Free: {} MB, Using: {} MB",
            total_memory_mb, available_memory_mb, free_memory_mb, effective_available_mb
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
        info!("🔍 RESOURCE_LIMITS: Validating executor configuration against resource limits");

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

        // Critical error: minimum executors exceed available connections
        if total_min_executors > self.available_database_connections {
            validation_errors.push(format!(
                "CRITICAL: Minimum executor count ({}) exceeds available database connections ({}). \
                System cannot start with this configuration.",
                total_min_executors,
                self.available_database_connections
            ));
        }

        // Critical error: maximum executors exceed total pool size
        if total_max_executors > self.max_database_connections {
            validation_errors.push(format!(
                "CRITICAL: Maximum executor count ({}) exceeds total database pool size ({}). \
                This will cause connection exhaustion under load.",
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
                    "CRITICAL: {} min_executors ({}) exceeds max_executors ({})",
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
            info!("✅ RESOURCE_LIMITS: Executor configuration validation passed");
        } else {
            error!(
                "❌ RESOURCE_LIMITS: Executor configuration validation failed with {} errors",
                validation_result.validation_errors.len()
            );
            for error in &validation_result.validation_errors {
                error!("  - {}", error);
            }
        }

        if !validation_result.validation_warnings.is_empty() {
            warn!(
                "⚠️ RESOURCE_LIMITS: {} validation warnings generated",
                validation_result.validation_warnings.len()
            );
            for warning in &validation_result.validation_warnings {
                warn!("  - {}", warning);
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

    /// Check if configuration should fail startup (has critical errors)
    pub fn should_fail_startup(&self) -> bool {
        !self.is_valid
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
}

impl ResourceValidator {
    /// Create a new resource validator
    pub async fn new(database_pool: &PgPool, config_manager: Arc<ConfigManager>) -> Result<Self> {
        let resource_limits = SystemResourceLimits::detect(database_pool).await?;

        Ok(Self {
            resource_limits,
            config_manager,
        })
    }

    /// Perform validation and optionally fail fast on critical errors
    pub async fn validate_and_fail_fast(&self) -> Result<ValidationResult> {
        let validation_result = self
            .resource_limits
            .validate_executor_configuration(&self.config_manager)?;

        info!(
            "🔍 RESOURCE_VALIDATOR: Validation completed - Valid: {}, Errors: {}, Warnings: {}",
            validation_result.is_valid,
            validation_result.validation_errors.len(),
            validation_result.validation_warnings.len()
        );

        // Log validation summary
        for line in validation_result.summary().lines() {
            if validation_result.is_valid {
                info!("📊 VALIDATION: {}", line);
            } else {
                error!("📊 VALIDATION: {}", line);
            }
        }

        // Fail fast on critical errors
        if validation_result.should_fail_startup() {
            return Err(TaskerError::InvalidConfiguration(format!(
                "Resource validation failed - system cannot start safely:\n{}",
                validation_result.summary()
            )));
        }

        Ok(validation_result)
    }

    /// Get detected resource limits
    pub fn resource_limits(&self) -> &SystemResourceLimits {
        &self.resource_limits
    }

    /// Refresh resource limits (re-detect)
    pub async fn refresh(&mut self, database_pool: &PgPool) -> Result<()> {
        info!("🔄 RESOURCE_VALIDATOR: Refreshing resource limits detection");
        self.resource_limits = SystemResourceLimits::detect(database_pool).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigManager;

    #[tokio::test]
    async fn test_system_resource_limits_creation() {
        // Setup test environment (respects existing DATABASE_URL in CI)
        crate::test_utils::setup_test_environment();

        let _config_manager = ConfigManager::load_from_directory_with_env(None, "test")
            .expect("Failed to load test configuration");

        // Create a test database pool
        let database_url = crate::test_utils::get_test_database_url();
        let pool = sqlx::postgres::PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let resource_limits = SystemResourceLimits::detect(&pool).await;
        assert!(resource_limits.is_ok());

        let limits = resource_limits.unwrap();
        assert!(limits.max_database_connections > 0);
        assert!(limits.detected_at <= chrono::Utc::now());
    }

    #[tokio::test]
    async fn test_resource_validation() {
        // Setup test environment (respects existing DATABASE_URL in CI)
        crate::test_utils::setup_test_environment();

        let config_manager = ConfigManager::load_from_directory_with_env(None, "test")
            .expect("Failed to load test configuration");

        // Create a test database pool
        let database_url = crate::test_utils::get_test_database_url();
        let pool = sqlx::postgres::PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        let resource_limits = SystemResourceLimits::detect(&pool).await.unwrap();
        let validation_result = resource_limits
            .validate_executor_configuration(&config_manager)
            .unwrap();

        // The test configuration should be reasonable
        assert!(!validation_result
            .executor_requirements
            .per_type_requirements
            .is_empty());

        // Check that totals are calculated
        assert!(validation_result.executor_requirements.total_min_executors > 0);
        assert!(validation_result.executor_requirements.total_max_executors > 0);

        // Verify validation logic
        assert_eq!(
            validation_result.is_valid,
            validation_result.validation_errors.is_empty()
        );
    }

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
}

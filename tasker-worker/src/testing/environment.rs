//! # Worker Test Environment Management
//!
//! Pure Rust test environment validation and safety checks, inspired by
//! tasker-orchestration/src/ffi/shared/test_database_management.rs patterns
//! but without FFI dependencies.
//!
//! ## Key Patterns
//!
//! - Environment safety validation
//! - Test-specific configuration verification
//! - Resource cleanup with proper ordering
//! - Test isolation guarantees

use std::collections::HashMap;
use std::env;
use tracing::{error, info, warn};

/// Test environment error types
#[derive(Debug, thiserror::Error)]
pub enum TestEnvironmentError {
    #[error("Environment validation failed: {0}")]
    ValidationError(String),

    #[error("Safety check failed: {0}")]
    SafetyError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Resource error: {0}")]
    ResourceError(String),
}

/// Worker test environment validator and manager
///
/// Ensures safe test execution with proper isolation and cleanup
#[derive(Debug, Clone)]
pub struct WorkerTestEnvironment {
    environment_vars: HashMap<String, String>,
    database_url: String,
    is_ci: bool,
    is_safe: bool,
}

impl WorkerTestEnvironment {
    /// Create and validate test environment
    pub fn new() -> Result<Self, TestEnvironmentError> {
        let mut env_vars = HashMap::new();

        // Collect relevant environment variables
        for (key, value) in env::vars() {
            if key.starts_with("TASKER_")
                || key.starts_with("TEST_")
                || key == "DATABASE_URL"
                || key == "RAILS_ENV"
                || key == "APP_ENV"
                || key == "CI"
            {
                env_vars.insert(key, value);
            }
        }

        // Get database URL
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
        });

        // Check CI environment
        let is_ci = env::var("CI").is_ok() || env::var("GITHUB_ACTIONS").is_ok();

        let mut env = Self {
            environment_vars: env_vars,
            database_url: database_url.clone(),
            is_ci,
            is_safe: false,
        };

        // Validate safety
        env.is_safe = env.validate_test_safety_internal()?;

        Ok(env)
    }

    /// Validate test environment safety
    pub fn validate_test_safety(&self) -> Result<(), TestEnvironmentError> {
        if !self.is_safe {
            return Err(TestEnvironmentError::SafetyError(
                "Test environment is not safe".to_string(),
            ));
        }

        Ok(())
    }

    /// Internal safety validation
    fn validate_test_safety_internal(&self) -> Result<bool, TestEnvironmentError> {
        // Check 1: Database URL must contain 'test'
        if !self.database_url.contains("test") {
            error!(
                "🚨 SAFETY: Database URL does not contain 'test': {}",
                self.database_url
            );
            return Err(TestEnvironmentError::SafetyError(format!(
                "Database URL must contain 'test' for safety. Found: {}",
                self.database_url
            )));
        }

        // Check 2: Database URL must NOT contain production keywords
        let danger_keywords = ["production", "prod", "live", "main", "primary"];
        for keyword in &danger_keywords {
            if self.database_url.to_lowercase().contains(keyword) {
                error!(
                    "🚨 SAFETY: Database URL contains dangerous keyword '{}': {}",
                    keyword, self.database_url
                );
                return Err(TestEnvironmentError::SafetyError(format!(
                    "Database URL contains dangerous keyword: {}",
                    keyword
                )));
            }
        }

        // Check 3: Environment must indicate test mode
        let is_test_env = self.get_env("TASKER_ENV") == Some("test".to_string())
            || self.get_env("TEST_ENV").is_some()
            || self.is_ci;

        if !is_test_env {
            warn!("⚠️ Environment does not indicate test mode");
            warn!("  Set TASKER_ENV=test for safety");

            // In CI, this is an error
            if self.is_ci {
                return Err(TestEnvironmentError::SafetyError(
                    "CI environment must have explicit test mode set".to_string(),
                ));
            }
        }

        // Check 4: Validate worker-specific test configuration
        if let Some(worker_env) = self.get_env("WORKER_ENV") {
            if worker_env != "test" {
                warn!("WORKER_ENV is set but not to 'test': {}", worker_env);
            }
        }

        info!("✅ Test environment safety validation passed");
        info!("  Database: {}", self.database_url);
        info!("  CI: {}", self.is_ci);
        info!("  Test mode: {}", is_test_env);

        Ok(true)
    }

    /// Get environment variable value
    pub fn get_env(&self, key: &str) -> Option<String> {
        self.environment_vars.get(key).cloned()
    }

    /// Check if running in CI
    pub fn is_ci(&self) -> bool {
        self.is_ci
    }

    /// Get database URL
    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    /// Create test-specific environment overrides
    pub fn create_test_overrides(&self) -> HashMap<String, String> {
        let mut overrides = HashMap::new();

        // Force test environment
        overrides.insert("TASKER_ENV".to_string(), "test".to_string());
        overrides.insert("WORKER_ENV".to_string(), "test".to_string());

        // Disable external integrations in tests
        overrides.insert("DISABLE_EXTERNAL_API".to_string(), "true".to_string());
        overrides.insert("DISABLE_NOTIFICATIONS".to_string(), "true".to_string());

        // Use test-specific configuration
        overrides.insert(
            "TASKER_CONFIG_ROOT".to_string(),
            "config/tasker/test".to_string(),
        );

        overrides
    }

    /// Apply test environment overrides
    pub fn apply_overrides(&self) {
        let overrides = self.create_test_overrides();
        for (key, value) in overrides {
            env::set_var(key, value);
        }
    }
}

/// Test safety validator for specific test scenarios
pub struct TestSafetyValidator {
    environment: WorkerTestEnvironment,
}

impl TestSafetyValidator {
    /// Create validator from current environment
    pub fn new() -> Result<Self, TestEnvironmentError> {
        Ok(Self {
            environment: WorkerTestEnvironment::new()?,
        })
    }

    /// Validate database safety for destructive operations
    pub fn validate_destructive_operations(&self) -> Result<(), TestEnvironmentError> {
        // Extra safety for operations that drop schemas or truncate tables
        self.environment.validate_test_safety()?;

        // Additional check: require explicit TEST_ALLOW_DESTRUCTIVE flag
        if !self.environment.is_ci()
            && self.environment.get_env("TEST_ALLOW_DESTRUCTIVE") != Some("true".to_string())
        {
            return Err(TestEnvironmentError::SafetyError(
                "Destructive operations require TEST_ALLOW_DESTRUCTIVE=true".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate worker test configuration
    pub fn validate_worker_config(&self) -> Result<(), TestEnvironmentError> {
        self.environment.validate_test_safety()?;

        // Check for worker-specific test configuration
        let config_root = self.environment.get_env("TASKER_CONFIG_ROOT");
        if let Some(root) = config_root {
            if !root.contains("test") && !self.environment.is_ci() {
                warn!("Config root does not indicate test configuration: {}", root);
            }
        }

        Ok(())
    }

    /// Get validated environment
    pub fn environment(&self) -> &WorkerTestEnvironment {
        &self.environment
    }
}

/// Test environment setup helper
pub struct TestEnvironmentSetup;

impl TestEnvironmentSetup {
    /// Setup complete test environment with safety checks
    pub async fn setup() -> Result<WorkerTestEnvironment, TestEnvironmentError> {
        let env = WorkerTestEnvironment::new()?;
        env.validate_test_safety()?;
        env.apply_overrides();

        info!("🧪 Test environment setup complete");

        Ok(env)
    }

    /// Setup with custom database URL
    pub async fn setup_with_database(
        database_url: &str,
    ) -> Result<WorkerTestEnvironment, TestEnvironmentError> {
        // Override DATABASE_URL
        env::set_var("DATABASE_URL", database_url);

        Self::setup().await
    }
}

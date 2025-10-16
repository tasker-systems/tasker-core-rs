// TAS-50: CommonConfig - Shared infrastructure configuration
//
// This configuration is shared by both orchestration and worker contexts.
// It contains infrastructure components that all systems need:
// - Database connection and pooling
// - Queue configuration (PGMQ)
// - Environment detection
// - Shared MPSC channel settings
//
// Phase 1 implementation: Non-breaking addition with conversion from TaskerConfig

use serde::{Deserialize, Serialize};

use super::ConfigurationContext;
use crate::config::circuit_breaker::CircuitBreakerConfig;
use crate::config::components::{DatabaseConfig, QueuesConfig, SharedChannelsConfig};
use crate::config::error::ConfigurationError;
use crate::config::TaskerConfig;

/// CommonConfig contains infrastructure configuration shared by all deployment contexts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonConfig {
    /// Database connection and pooling configuration
    pub database: DatabaseConfig,

    /// Queue system configuration (PGMQ)
    pub queues: QueuesConfig,

    /// Current environment (test/development/production)
    pub environment: String,

    /// Shared MPSC channel configuration
    pub shared_channels: SharedChannelsConfig,

    /// Circuit breaker configuration for resilience patterns
    /// Used by both orchestration and worker for database, queue, and API protection
    pub circuit_breakers: CircuitBreakerConfig,
}

impl CommonConfig {
    /// Get the database URL with environment variable override support
    ///
    /// Priority:
    /// 1. DATABASE_URL environment variable (if set)
    /// 2. database.url from TOML configuration (if set)
    /// 3. Build from components (host, username, password, database name)
    ///
    /// This allows K8s deployments to override database URLs without
    /// modifying TOML files, enabling secrets rotation and testing flexibility.
    pub fn database_url(&self) -> String {
        // Check for environment variable override first
        if let Ok(env_url) = std::env::var("DATABASE_URL") {
            if !env_url.is_empty() {
                return env_url;
            }
        }

        // Use DatabaseConfig's existing logic which handles TOML url and component building
        self.database.database_url(&self.environment)
    }
}

impl ConfigurationContext for CommonConfig {
    fn validate(&self) -> Result<(), Vec<ConfigurationError>> {
        let mut errors = Vec::new();

        // Database validation
        if self.database.host.is_empty() {
            errors.push(ConfigurationError::MissingRequiredField {
                field: "database.host".to_string(),
                context: "CommonConfig".to_string(),
            });
        }

        if self.database.username.is_empty() {
            errors.push(ConfigurationError::MissingRequiredField {
                field: "database.username".to_string(),
                context: "CommonConfig".to_string(),
            });
        }

        if self.database.pool.max_connections == 0 {
            errors.push(ConfigurationError::InvalidValue {
                field: "database.pool.max_connections".to_string(),
                value: "0".to_string(),
                context: "max_connections must be greater than 0".to_string(),
            });
        }

        // Environment validation
        if self.environment.is_empty() {
            errors.push(ConfigurationError::MissingRequiredField {
                field: "environment".to_string(),
                context: "CommonConfig".to_string(),
            });
        }

        // Queue validation
        if self.queues.default_batch_size == 0 {
            errors.push(ConfigurationError::InvalidValue {
                field: "queues.default_batch_size".to_string(),
                value: "0".to_string(),
                context: "batch_size must be greater than 0".to_string(),
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn environment(&self) -> &str {
        &self.environment
    }

    fn summary(&self) -> String {
        format!(
            "CommonConfig: environment={}, database={}, max_connections={}",
            self.environment, self.database.host, self.database.pool.max_connections
        )
    }
}

impl From<&TaskerConfig> for CommonConfig {
    /// Convert legacy TaskerConfig to CommonConfig
    ///
    /// This conversion extracts only the common/shared configuration fields
    /// from the monolithic TaskerConfig structure.
    fn from(config: &TaskerConfig) -> Self {
        Self {
            database: config.database.clone(),
            queues: config.queues.clone(),
            environment: config.execution.environment.clone(),
            shared_channels: config.mpsc_channels.shared.clone(),
            circuit_breakers: config.circuit_breakers.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_common_config_from_tasker_config() {
        // Create a test TaskerConfig
        let tasker_config = TaskerConfig::default();

        // Convert to CommonConfig
        let common_config = CommonConfig::from(&tasker_config);

        // Verify fields were correctly extracted
        assert_eq!(common_config.database.host, tasker_config.database.host);
        assert_eq!(
            common_config.database.pool.max_connections,
            tasker_config.database.pool.max_connections
        );
        assert_eq!(
            common_config.environment,
            tasker_config.execution.environment
        );
    }

    #[test]
    fn test_common_config_validation_success() {
        let tasker_config = TaskerConfig::default();
        let common_config = CommonConfig::from(&tasker_config);

        // Should pass validation
        assert!(common_config.validate().is_ok());
    }

    #[test]
    fn test_common_config_validation_missing_database_host() {
        let mut tasker_config = TaskerConfig::default();
        tasker_config.database.host = String::new();

        let common_config = CommonConfig::from(&tasker_config);

        // Should fail validation
        let result = common_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0],
            ConfigurationError::MissingRequiredField { .. }
        ));
    }

    #[test]
    fn test_common_config_validation_invalid_max_connections() {
        let mut tasker_config = TaskerConfig::default();
        tasker_config.database.pool.max_connections = 0;

        let common_config = CommonConfig::from(&tasker_config);

        // Should fail validation
        let result = common_config.validate();
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigurationError::InvalidValue { .. })));
    }

    #[test]
    #[serial]
    fn test_common_config_database_url_from_env() {
        // Clean up first to ensure no interference
        std::env::remove_var("DATABASE_URL");

        // Set environment variable
        std::env::set_var("DATABASE_URL", "postgresql://env:env@envhost:5432/envdb");

        let tasker_config = TaskerConfig::default();
        let common_config = CommonConfig::from(&tasker_config);

        // Should use environment variable override
        let db_url = common_config.database_url();
        assert_eq!(db_url, "postgresql://env:env@envhost:5432/envdb");

        // Clean up
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    #[serial]
    fn test_common_config_database_url_from_toml() {
        // Ensure no environment variable
        std::env::remove_var("DATABASE_URL");

        let tasker_config = TaskerConfig::default();
        let common_config = CommonConfig::from(&tasker_config);

        // Should use TOML configuration (built from components or url field)
        let db_url = common_config.database_url();
        assert!(!db_url.is_empty());
        assert!(db_url.starts_with("postgresql://"));
    }

    #[test]
    #[serial]
    fn test_common_config_database_url_env_overrides_toml() {
        // Clean up first to ensure no interference
        std::env::remove_var("DATABASE_URL");

        // Set environment variable
        std::env::set_var(
            "DATABASE_URL",
            "postgresql://override:override@override:5432/override",
        );

        let mut tasker_config = TaskerConfig::default();
        tasker_config.database.url = Some("postgresql://toml:toml@toml:5432/toml".to_string());

        let common_config = CommonConfig::from(&tasker_config);

        // Environment variable should override TOML
        let db_url = common_config.database_url();
        assert_eq!(
            db_url,
            "postgresql://override:override@override:5432/override"
        );

        // Clean up
        std::env::remove_var("DATABASE_URL");
    }
}

//! Configuration Error Types
//!
//! Comprehensive error handling for configuration loading and validation.
//! Provides specific, actionable error messages for different failure scenarios.
//!
//! This module provides error types that cover all aspects of orchestration:
//! - Task execution errors
//! - Step execution errors
//! - Registry errors
//! - Configuration errors
//! - State management errors
//! - Event publishing errors

use std::path::PathBuf;
use thiserror::Error;

/// Configuration-related errors with detailed context
#[derive(Debug, Error)]
pub enum ConfigurationError {
    /// Configuration file not found at expected locations
    #[error("Configuration file not found. Searched paths: {searched_paths:?}")]
    ConfigFileNotFound { searched_paths: Vec<PathBuf> },

    /// Invalid YAML syntax in configuration file
    #[error("Invalid YAML in configuration file '{file_path}': {error}")]
    InvalidYaml { file_path: String, error: String },

    /// Invalid TOML syntax in configuration file
    #[error("Invalid TOML syntax in '{file_path}': {error}")]
    InvalidToml { file_path: String, error: String },

    /// Resource constraint violation
    #[error("Resource constraint violation: requested {requested} executors but only {available} database connections available")]
    ResourceConstraintViolation { requested: usize, available: usize },

    /// Unknown configuration field
    #[error("Unknown configuration field: {field} in component {component}")]
    UnknownField { field: String, component: String },

    /// Type mismatch for configuration field
    #[error("Type mismatch for field {field}: expected {expected}, got {actual}")]
    TypeMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    /// Missing environment override file
    #[error("Environment override file missing: {path}")]
    MissingOverride { path: PathBuf },

    /// Missing required configuration field
    #[error("Missing required configuration field '{field}' in {context}")]
    MissingRequiredField { field: String, context: String },

    /// Invalid configuration value
    #[error("Invalid value '{value}' for field '{field}': {context}")]
    InvalidValue {
        field: String,
        value: String,
        context: String,
    },

    /// Environment-specific configuration issues
    #[error("Environment configuration error for '{environment}': {error}")]
    EnvironmentConfigError { environment: String, error: String },

    /// Configuration merging errors
    #[error("Failed to merge environment-specific configuration: {error}")]
    ConfigMergeError { error: String },

    /// File I/O errors during configuration loading
    #[error("Failed to read configuration file '{file_path}': {error}")]
    FileReadError { file_path: String, error: String },

    /// Environment variable expansion errors
    #[error("Failed to expand environment variable '{variable}' in configuration: {context}")]
    EnvironmentVariableError { variable: String, context: String },

    /// Configuration validation errors
    #[error("Configuration validation failed: {error}")]
    ValidationError { error: String },

    /// Step configuration parsing errors
    #[error("Invalid step configuration for '{step_name}': {error}")]
    InvalidStepConfig { step_name: String, error: String },

    /// Handler configuration parsing errors
    #[error("Invalid handler configuration for '{step_name}': {error}")]
    InvalidHandlerConfig { step_name: String, error: String },

    /// JSON serialization/deserialization errors
    #[error("JSON serialization error in {context}: {error}")]
    JsonSerializationError { context: String, error: String },

    /// Database configuration errors
    #[error("Database configuration error: {error}")]
    DatabaseConfigError { error: String },

    #[error("File file not found.")]
    FileNotFound(String),

    #[error("Parse Error for file {file_path}: {reason}")]
    ParseError { file_path: String, reason: String },

    #[error("Schema validation error for file {file_path}: {reason}")]
    SchemaValidationError { file_path: String, reason: String },

    #[error("Environment override error for key {key}: {reason}")]
    EnvironmentOverrideError { key: String, reason: String },
}

impl ConfigurationError {
    /// Create a configuration file not found error
    pub fn config_file_not_found(searched_paths: Vec<PathBuf>) -> Self {
        Self::ConfigFileNotFound { searched_paths }
    }

    /// Create an invalid YAML error
    pub fn invalid_yaml<P: Into<String>, E: std::fmt::Display>(file_path: P, error: E) -> Self {
        Self::InvalidYaml {
            file_path: file_path.into(),
            error: error.to_string(),
        }
    }

    /// Create a missing required field error
    pub fn missing_required_field<F: Into<String>, C: Into<String>>(field: F, context: C) -> Self {
        Self::MissingRequiredField {
            field: field.into(),
            context: context.into(),
        }
    }

    /// Create an invalid value error
    pub fn invalid_value<F: Into<String>, V: Into<String>, C: Into<String>>(
        field: F,
        value: V,
        context: C,
    ) -> Self {
        Self::InvalidValue {
            field: field.into(),
            value: value.into(),
            context: context.into(),
        }
    }

    /// Create an environment configuration error
    pub fn environment_config_error<E: Into<String>, R: std::fmt::Display>(
        environment: E,
        error: R,
    ) -> Self {
        Self::EnvironmentConfigError {
            environment: environment.into(),
            error: error.to_string(),
        }
    }

    /// Create a file read error
    pub fn file_read_error<P: Into<String>, E: std::fmt::Display>(file_path: P, error: E) -> Self {
        Self::FileReadError {
            file_path: file_path.into(),
            error: error.to_string(),
        }
    }

    /// Create an environment variable expansion error
    pub fn environment_variable_error<V: Into<String>, C: Into<String>>(
        variable: V,
        context: C,
    ) -> Self {
        Self::EnvironmentVariableError {
            variable: variable.into(),
            context: context.into(),
        }
    }

    /// Create a validation error
    pub fn validation_error<E: std::fmt::Display>(error: E) -> Self {
        Self::ValidationError {
            error: error.to_string(),
        }
    }

    /// Create an invalid step config error
    pub fn invalid_step_config<S: Into<String>, E: std::fmt::Display>(
        step_name: S,
        error: E,
    ) -> Self {
        Self::InvalidStepConfig {
            step_name: step_name.into(),
            error: error.to_string(),
        }
    }

    /// Create an invalid handler config error
    pub fn invalid_handler_config<S: Into<String>, E: std::fmt::Display>(
        step_name: S,
        error: E,
    ) -> Self {
        Self::InvalidHandlerConfig {
            step_name: step_name.into(),
            error: error.to_string(),
        }
    }

    /// Create a JSON serialization error
    pub fn json_serialization_error<C: Into<String>, E: std::fmt::Display>(
        context: C,
        error: E,
    ) -> Self {
        Self::JsonSerializationError {
            context: context.into(),
            error: error.to_string(),
        }
    }

    /// Create a database configuration error
    pub fn database_config_error<E: std::fmt::Display>(error: E) -> Self {
        Self::DatabaseConfigError {
            error: error.to_string(),
        }
    }

    /// Create an invalid TOML error
    pub fn invalid_toml<P: Into<String>, E: std::fmt::Display>(file_path: P, error: E) -> Self {
        Self::InvalidToml {
            file_path: file_path.into(),
            error: error.to_string(),
        }
    }

    /// Create a resource constraint violation error
    pub fn resource_constraint_violation(requested: usize, available: usize) -> Self {
        Self::ResourceConstraintViolation {
            requested,
            available,
        }
    }

    /// Create an unknown field error
    pub fn unknown_field<F: Into<String>, C: Into<String>>(field: F, component: C) -> Self {
        Self::UnknownField {
            field: field.into(),
            component: component.into(),
        }
    }

    /// Create a type mismatch error
    pub fn type_mismatch<F: Into<String>, E: Into<String>, A: Into<String>>(
        field: F,
        expected: E,
        actual: A,
    ) -> Self {
        Self::TypeMismatch {
            field: field.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    /// Create a missing override error
    pub fn missing_override(path: PathBuf) -> Self {
        Self::MissingOverride { path }
    }
}

/// Result type for configuration operations
pub type ConfigResult<T> = Result<T, ConfigurationError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_config_file_not_found_error() {
        let paths = vec![PathBuf::from("/path/1"), PathBuf::from("/path/2")];
        let error = ConfigurationError::config_file_not_found(paths);

        let error_string = error.to_string();
        assert!(error_string.contains("Configuration file not found"));
        assert!(error_string.contains("/path/1"));
        assert!(error_string.contains("/path/2"));
    }

    #[test]
    fn test_invalid_yaml_error() {
        let error =
            ConfigurationError::invalid_yaml("/path/to/config.yaml", "syntax error at line 5");

        let error_string = error.to_string();
        assert!(error_string.contains("Invalid YAML"));
        assert!(error_string.contains("/path/to/config.yaml"));
        assert!(error_string.contains("syntax error at line 5"));
    }

    #[test]
    fn test_missing_required_field_error() {
        let error =
            ConfigurationError::missing_required_field("database.host", "database configuration");

        let error_string = error.to_string();
        assert!(error_string.contains("Missing required configuration field 'database.host'"));
        assert!(error_string.contains("database configuration"));
    }

    #[test]
    fn test_invalid_value_error() {
        let error = ConfigurationError::invalid_value(
            "database.pool",
            "0",
            "pool size must be greater than 0",
        );

        let error_string = error.to_string();
        assert!(error_string.contains("Invalid value '0' for field 'database.pool'"));
        assert!(error_string.contains("pool size must be greater than 0"));
    }

    #[test]
    fn test_step_config_error() {
        let error =
            ConfigurationError::invalid_step_config("validate_order", "missing timeout_ms field");

        let error_string = error.to_string();
        assert!(error_string.contains("Invalid step configuration for 'validate_order'"));
        assert!(error_string.contains("missing timeout_ms field"));
    }

    #[test]
    fn test_handler_config_error() {
        let error =
            ConfigurationError::invalid_handler_config("process_payment", "invalid handler_class");

        let error_string = error.to_string();
        assert!(error_string.contains("Invalid handler configuration for 'process_payment'"));
        assert!(error_string.contains("invalid handler_class"));
    }
}

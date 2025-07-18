//! # Step Execution Error Classification
//!
//! Comprehensive error classification system for step execution failures.
//!
//! ## Overview
//!
//! This module provides a centralized error classification system that analyzes step
//! execution errors and determines appropriate handling strategies. It consolidates
//! the scattered error handling logic from StepExecutor and BaseStepHandler into
//! a unified, extensible classification framework.
//!
//! ## Key Features
//!
//! - **Error Categorization**: Classifies errors as retryable, permanent, timeout, etc.
//! - **Retry Strategy Determination**: Calculates appropriate retry delays and limits
//! - **Context-Aware Classification**: Uses execution context for intelligent decisions
//! - **Actionable Error Information**: Provides remediation suggestions and error codes
//! - **Framework Integration**: Easy integration with existing orchestration components
//!
//! ## Architecture
//!
//! The error classifier uses a strategy pattern with trait-based classification:
//!
//! ```text
//! ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
//! │ Error Instance  │────▶│ ErrorClassifier │────▶│ Classification  │
//! │ + Context       │     │ Strategy        │     │ Result          │
//! └─────────────────┘     └─────────────────┘     └─────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::error_classifier::{
//!     StandardErrorClassifier, ErrorClassifier, ErrorContext
//! };
//! use tasker_core::orchestration::errors::OrchestrationError;
//! use std::time::Duration;
//! use std::collections::HashMap;
//!
//! let classifier = StandardErrorClassifier::new();
//! let context = ErrorContext {
//!     step_id: 123,
//!     task_id: 456,
//!     attempt_number: 2,
//!     max_attempts: 5,
//!     execution_duration: Duration::from_secs(30),
//!     step_name: "process_payment".to_string(),
//!     error_source: "payment_gateway".to_string(),
//!     metadata: HashMap::new(),
//! };
//!
//! let error = OrchestrationError::TimeoutError {
//!     operation: "payment_processing".to_string(),
//!     timeout_duration: Duration::from_secs(30),
//! };
//!
//! let classification = classifier.classify_error(&error, &context);
//!
//! if classification.is_retryable {
//!     println!("Retrying in {}s", classification.retry_delay.unwrap().as_secs());
//! } else {
//!     println!("Permanent failure: {}", classification.error_category);
//! }
//! ```

use crate::orchestration::errors::{ExecutionError, OrchestrationError, StepExecutionError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Context information for error classification
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Step ID where the error occurred
    pub step_id: i64,

    /// Parent task ID
    pub task_id: i64,

    /// Current attempt number (1-based)
    pub attempt_number: u32,

    /// Maximum allowed attempts
    pub max_attempts: u32,

    /// How long the step ran before failing
    pub execution_duration: Duration,

    /// Name of the step that failed
    pub step_name: String,

    /// Source component where error originated
    pub error_source: String,

    /// Additional context metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Result of error classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorClassification {
    /// Primary error category
    pub error_category: ErrorCategory,

    /// Whether this error should be retried
    pub is_retryable: bool,

    /// Recommended delay before retry (if retryable)
    pub retry_delay: Option<Duration>,

    /// Specific error code for tracking
    pub error_code: String,

    /// Human-readable error message
    pub error_message: String,

    /// Suggested remediation actions
    pub remediation_suggestions: Vec<String>,

    /// Whether this is the final attempt
    pub is_final_attempt: bool,

    /// Classification confidence (0.0 - 1.0)
    pub confidence: f64,

    /// Additional classification metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Primary error categories for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Permanent error - will never succeed if retried
    Permanent,

    /// Transient error - may succeed on retry
    Transient,

    /// Timeout error - retry with different timeout
    Timeout,

    /// Rate limiting - retry with backoff
    RateLimit,

    /// Network error - retry with network-aware strategy
    Network,

    /// Configuration error - requires manual intervention
    Configuration,

    /// Resource exhaustion - retry when resources available
    ResourceExhaustion,

    /// Dependency failure - retry when dependencies recover
    DependencyFailure,

    /// State inconsistency - requires state resolution
    StateInconsistency,

    /// Unknown error - conservative retry approach
    Unknown,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::Permanent => write!(f, "Permanent"),
            ErrorCategory::Transient => write!(f, "Transient"),
            ErrorCategory::Timeout => write!(f, "Timeout"),
            ErrorCategory::RateLimit => write!(f, "Rate Limit"),
            ErrorCategory::Network => write!(f, "Network"),
            ErrorCategory::Configuration => write!(f, "Configuration"),
            ErrorCategory::ResourceExhaustion => write!(f, "Resource Exhaustion"),
            ErrorCategory::DependencyFailure => write!(f, "Dependency Failure"),
            ErrorCategory::StateInconsistency => write!(f, "State Inconsistency"),
            ErrorCategory::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Retry strategy recommendations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryStrategy {
    /// No retry recommended
    NoRetry,

    /// Immediate retry
    Immediate,

    /// Fixed delay retry
    FixedDelay,

    /// Exponential backoff
    ExponentialBackoff,

    /// Linear backoff
    LinearBackoff,

    /// Jittered exponential backoff
    JitteredExponentialBackoff,

    /// Custom delay (server-specified)
    CustomDelay,
}

/// Trait for error classification strategies
pub trait ErrorClassifier: Send + Sync {
    /// Classify an error and provide handling recommendations
    fn classify_error(
        &self,
        error: &OrchestrationError,
        context: &ErrorContext,
    ) -> ErrorClassification;

    /// Get the classifier name for identification
    fn classifier_name(&self) -> &'static str;

    /// Check if this classifier can handle the given error type
    fn can_classify(&self, error: &OrchestrationError) -> bool;
}

/// Standard error classifier with comprehensive classification logic
pub struct StandardErrorClassifier {
    /// Configuration for retry strategies
    config: ErrorClassifierConfig,
}

/// Configuration for error classification behavior
#[derive(Debug, Clone)]
pub struct ErrorClassifierConfig {
    /// Base retry delay for exponential backoff
    pub base_retry_delay: Duration,

    /// Maximum retry delay
    pub max_retry_delay: Duration,

    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,

    /// Jitter factor (0.0 - 1.0)
    pub jitter_factor: f64,

    /// Default timeout for timeout errors
    pub default_timeout: Duration,

    /// Maximum timeout allowed
    pub max_timeout: Duration,

    /// Rate limit retry delay
    pub rate_limit_delay: Duration,

    /// Network error retry delay
    pub network_error_delay: Duration,

    /// Resource exhaustion retry delay
    pub resource_exhaustion_delay: Duration,
}

impl Default for ErrorClassifierConfig {
    fn default() -> Self {
        Self {
            base_retry_delay: Duration::from_secs(1),
            max_retry_delay: Duration::from_secs(300), // 5 minutes
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
            default_timeout: Duration::from_secs(30),
            max_timeout: Duration::from_secs(3600), // 1 hour
            rate_limit_delay: Duration::from_secs(60),
            network_error_delay: Duration::from_secs(5),
            resource_exhaustion_delay: Duration::from_secs(30),
        }
    }
}

impl StandardErrorClassifier {
    /// Create a new standard error classifier with default configuration
    pub fn new() -> Self {
        Self {
            config: ErrorClassifierConfig::default(),
        }
    }

    /// Create a new standard error classifier with custom configuration
    pub fn with_config(config: ErrorClassifierConfig) -> Self {
        Self { config }
    }

    /// Classify timeout errors
    fn classify_timeout_error(
        &self,
        operation: &str,
        timeout_duration: Duration,
        context: &ErrorContext,
    ) -> ErrorClassification {
        let is_retryable = context.attempt_number < context.max_attempts;
        let retry_delay = if is_retryable {
            // For timeouts, use exponential backoff but cap at max timeout
            let base_delay = self.config.default_timeout;
            let multiplier = self
                .config
                .backoff_multiplier
                .powi(context.attempt_number as i32);
            let calculated_delay = base_delay.mul_f64(multiplier);
            Some(calculated_delay.min(self.config.max_retry_delay))
        } else {
            None
        };

        ErrorClassification {
            error_category: ErrorCategory::Timeout,
            is_retryable,
            retry_delay,
            error_code: "EXECUTION_TIMEOUT".to_string(),
            error_message: format!(
                "Step '{}' timed out after {:?} during {}",
                context.step_name, timeout_duration, operation
            ),
            remediation_suggestions: vec![
                "Consider increasing timeout configuration".to_string(),
                "Check for performance bottlenecks in step logic".to_string(),
                "Verify external dependencies are responsive".to_string(),
            ],
            is_final_attempt: context.attempt_number >= context.max_attempts,
            confidence: 0.95,
            metadata: HashMap::from([
                (
                    "timeout_duration".to_string(),
                    serde_json::json!(timeout_duration.as_secs()),
                ),
                ("operation".to_string(), serde_json::json!(operation)),
            ]),
        }
    }

    /// Classify database errors
    fn classify_database_error(
        &self,
        operation: &str,
        reason: &str,
        context: &ErrorContext,
    ) -> ErrorClassification {
        // Analyze database error message for classification
        let (category, is_retryable, error_code, suggestions) = if reason.contains("connection")
            || reason.contains("network")
            || reason.contains("timeout")
        {
            (
                ErrorCategory::Network,
                true,
                "DATABASE_CONNECTION_ERROR",
                vec![
                    "Check database connectivity".to_string(),
                    "Verify database server health".to_string(),
                    "Review connection pool configuration".to_string(),
                ],
            )
        } else if reason.contains("deadlock") || reason.contains("lock") {
            (
                ErrorCategory::StateInconsistency,
                true,
                "DATABASE_LOCK_ERROR",
                vec![
                    "Review transaction isolation levels".to_string(),
                    "Consider optimistic locking strategies".to_string(),
                    "Analyze query execution order".to_string(),
                ],
            )
        } else if reason.contains("constraint") || reason.contains("violation") {
            (
                ErrorCategory::Permanent,
                false,
                "DATABASE_CONSTRAINT_VIOLATION",
                vec![
                    "Review data validation rules".to_string(),
                    "Check for data consistency issues".to_string(),
                    "Verify referential integrity".to_string(),
                ],
            )
        } else {
            (
                ErrorCategory::Transient,
                true,
                "DATABASE_GENERAL_ERROR",
                vec![
                    "Check database logs for details".to_string(),
                    "Verify database schema compatibility".to_string(),
                    "Review SQL query syntax".to_string(),
                ],
            )
        };

        let retry_delay = if is_retryable && context.attempt_number < context.max_attempts {
            Some(self.calculate_exponential_backoff(context.attempt_number))
        } else {
            None
        };

        ErrorClassification {
            error_category: category,
            is_retryable: is_retryable && context.attempt_number < context.max_attempts,
            retry_delay,
            error_code: error_code.to_string(),
            error_message: format!(
                "Database operation '{}' failed for step '{}': {}",
                operation, context.step_name, reason
            ),
            remediation_suggestions: suggestions,
            is_final_attempt: context.attempt_number >= context.max_attempts,
            confidence: 0.85,
            metadata: HashMap::from([
                (
                    "database_operation".to_string(),
                    serde_json::json!(operation),
                ),
                ("error_reason".to_string(), serde_json::json!(reason)),
            ]),
        }
    }

    /// Classify state transition errors
    fn classify_state_error(
        &self,
        entity_type: &str,
        entity_id: i64,
        reason: &str,
        context: &ErrorContext,
    ) -> ErrorClassification {
        // State errors are usually due to concurrent modifications or invalid transitions
        let is_retryable = reason.contains("concurrent") || reason.contains("lock");
        let category = if is_retryable {
            ErrorCategory::StateInconsistency
        } else {
            ErrorCategory::Permanent
        };

        let retry_delay = if is_retryable && context.attempt_number < context.max_attempts {
            // Short delay for state conflicts to allow concurrent operations to complete
            Some(Duration::from_millis(500) * context.attempt_number)
        } else {
            None
        };

        ErrorClassification {
            error_category: category,
            is_retryable: is_retryable && context.attempt_number < context.max_attempts,
            retry_delay,
            error_code: "STATE_TRANSITION_ERROR".to_string(),
            error_message: format!(
                "State transition failed for {} {} in step '{}': {}",
                entity_type, entity_id, context.step_name, reason
            ),
            remediation_suggestions: vec![
                "Check for concurrent step executions".to_string(),
                "Verify state machine configuration".to_string(),
                "Review workflow step dependencies".to_string(),
            ],
            is_final_attempt: context.attempt_number >= context.max_attempts,
            confidence: 0.80,
            metadata: HashMap::from([
                ("entity_type".to_string(), serde_json::json!(entity_type)),
                ("entity_id".to_string(), serde_json::json!(entity_id)),
            ]),
        }
    }

    /// Classify configuration errors
    fn classify_configuration_error(
        &self,
        source: &str,
        reason: &str,
        context: &ErrorContext,
    ) -> ErrorClassification {
        // Configuration errors are permanent and require manual intervention
        ErrorClassification {
            error_category: ErrorCategory::Configuration,
            is_retryable: false,
            retry_delay: None,
            error_code: "CONFIGURATION_ERROR".to_string(),
            error_message: format!(
                "Configuration error in '{}' for step '{}': {}",
                source, context.step_name, reason
            ),
            remediation_suggestions: vec![
                "Review step configuration files".to_string(),
                "Verify environment variable settings".to_string(),
                "Check configuration schema compliance".to_string(),
                "Validate configuration file syntax".to_string(),
            ],
            is_final_attempt: true,
            confidence: 0.99,
            metadata: HashMap::from([
                (
                    "configuration_source".to_string(),
                    serde_json::json!(source),
                ),
                ("error_reason".to_string(), serde_json::json!(reason)),
            ]),
        }
    }

    /// Classify validation errors
    fn classify_validation_error(
        &self,
        field: &str,
        reason: &str,
        context: &ErrorContext,
    ) -> ErrorClassification {
        // Validation errors are permanent and indicate data issues
        ErrorClassification {
            error_category: ErrorCategory::Permanent,
            is_retryable: false,
            retry_delay: None,
            error_code: "VALIDATION_ERROR".to_string(),
            error_message: format!(
                "Validation failed for field '{}' in step '{}': {}",
                field, context.step_name, reason
            ),
            remediation_suggestions: vec![
                "Review input data format and values".to_string(),
                "Check field validation rules".to_string(),
                "Verify data type compatibility".to_string(),
                "Update step input data".to_string(),
            ],
            is_final_attempt: true,
            confidence: 0.95,
            metadata: HashMap::from([
                ("validation_field".to_string(), serde_json::json!(field)),
                ("validation_reason".to_string(), serde_json::json!(reason)),
            ]),
        }
    }

    /// Classify step execution errors from ExecutionError
    fn classify_execution_error(
        &self,
        execution_error: &ExecutionError,
        context: &ErrorContext,
    ) -> ErrorClassification {
        match execution_error {
            ExecutionError::StepExecutionFailed {
                step_id,
                reason,
                error_code,
            } => {
                // Analyze reason to determine category
                let (category, is_retryable) = if reason.contains("timeout") {
                    (ErrorCategory::Timeout, true)
                } else if reason.contains("network") || reason.contains("connection") {
                    (ErrorCategory::Network, true)
                } else if reason.contains("rate") || reason.contains("limit") {
                    (ErrorCategory::RateLimit, true)
                } else if reason.contains("resource")
                    || reason.contains("memory")
                    || reason.contains("disk")
                {
                    (ErrorCategory::ResourceExhaustion, true)
                } else if reason.contains("permission") || reason.contains("auth") {
                    (ErrorCategory::Configuration, false)
                } else {
                    (ErrorCategory::Transient, true)
                };

                let retry_delay = if is_retryable && context.attempt_number < context.max_attempts {
                    Some(self.calculate_category_specific_delay(category, context.attempt_number))
                } else {
                    None
                };

                ErrorClassification {
                    error_category: category,
                    is_retryable: is_retryable && context.attempt_number < context.max_attempts,
                    retry_delay,
                    error_code: error_code
                        .clone()
                        .unwrap_or_else(|| "STEP_EXECUTION_FAILED".to_string()),
                    error_message: format!("Step {step_id} execution failed: {reason}"),
                    remediation_suggestions: self.get_category_suggestions(category),
                    is_final_attempt: context.attempt_number >= context.max_attempts,
                    confidence: 0.75,
                    metadata: HashMap::from([
                        ("failed_step_id".to_string(), serde_json::json!(step_id)),
                        ("error_reason".to_string(), serde_json::json!(reason)),
                    ]),
                }
            }
            ExecutionError::ExecutionTimeout {
                step_id,
                timeout_duration,
            } => {
                self.classify_timeout_error(&format!("step_{step_id}"), *timeout_duration, context)
            }
            ExecutionError::RetryLimitExceeded {
                step_id,
                max_attempts,
            } => ErrorClassification {
                error_category: ErrorCategory::Permanent,
                is_retryable: false,
                retry_delay: None,
                error_code: "RETRY_LIMIT_EXCEEDED".to_string(),
                error_message: format!(
                    "Step {step_id} exceeded retry limit of {max_attempts} attempts"
                ),
                remediation_suggestions: vec![
                    "Review step implementation for persistent issues".to_string(),
                    "Consider increasing retry limit if appropriate".to_string(),
                    "Analyze error patterns to identify root cause".to_string(),
                ],
                is_final_attempt: true,
                confidence: 1.0,
                metadata: HashMap::from([
                    ("failed_step_id".to_string(), serde_json::json!(step_id)),
                    ("max_attempts".to_string(), serde_json::json!(max_attempts)),
                ]),
            },
            _ => {
                // Generic execution error classification
                ErrorClassification {
                    error_category: ErrorCategory::Transient,
                    is_retryable: context.attempt_number < context.max_attempts,
                    retry_delay: if context.attempt_number < context.max_attempts {
                        Some(self.calculate_exponential_backoff(context.attempt_number))
                    } else {
                        None
                    },
                    error_code: "EXECUTION_ERROR".to_string(),
                    error_message: format!("Step execution error: {execution_error}"),
                    remediation_suggestions: vec![
                        "Check step implementation for issues".to_string(),
                        "Review execution logs for details".to_string(),
                        "Verify step dependencies are available".to_string(),
                    ],
                    is_final_attempt: context.attempt_number >= context.max_attempts,
                    confidence: 0.60,
                    metadata: HashMap::from([(
                        "execution_error".to_string(),
                        serde_json::json!(execution_error.to_string()),
                    )]),
                }
            }
        }
    }

    /// Calculate exponential backoff delay
    fn calculate_exponential_backoff(&self, attempt_number: u32) -> Duration {
        let delay = self.config.base_retry_delay.mul_f64(
            self.config
                .backoff_multiplier
                .powi(attempt_number.saturating_sub(1) as i32),
        );

        let jittered_delay = if self.config.jitter_factor > 0.0 {
            let jitter = fastrand::f64() * self.config.jitter_factor;
            delay.mul_f64(1.0 + jitter)
        } else {
            delay
        };

        jittered_delay.min(self.config.max_retry_delay)
    }

    /// Calculate category-specific retry delay
    fn calculate_category_specific_delay(
        &self,
        category: ErrorCategory,
        attempt_number: u32,
    ) -> Duration {
        match category {
            ErrorCategory::RateLimit => self.config.rate_limit_delay,
            ErrorCategory::Network => self.config.network_error_delay * attempt_number,
            ErrorCategory::ResourceExhaustion => self.config.resource_exhaustion_delay,
            ErrorCategory::Timeout => self.config.default_timeout,
            _ => self.calculate_exponential_backoff(attempt_number),
        }
    }

    /// Get remediation suggestions for error category
    fn get_category_suggestions(&self, category: ErrorCategory) -> Vec<String> {
        match category {
            ErrorCategory::Network => vec![
                "Check network connectivity".to_string(),
                "Verify external service availability".to_string(),
                "Review network timeout configurations".to_string(),
            ],
            ErrorCategory::RateLimit => vec![
                "Implement request throttling".to_string(),
                "Review API rate limit policies".to_string(),
                "Consider using exponential backoff".to_string(),
            ],
            ErrorCategory::ResourceExhaustion => vec![
                "Monitor system resource usage".to_string(),
                "Scale resources if needed".to_string(),
                "Optimize resource consumption".to_string(),
            ],
            ErrorCategory::Timeout => vec![
                "Increase timeout configuration".to_string(),
                "Optimize step performance".to_string(),
                "Check for blocking operations".to_string(),
            ],
            ErrorCategory::Configuration => vec![
                "Review configuration files".to_string(),
                "Verify environment settings".to_string(),
                "Check permissions and access".to_string(),
            ],
            _ => vec![
                "Review error logs for details".to_string(),
                "Check step implementation".to_string(),
                "Verify dependencies are available".to_string(),
            ],
        }
    }
}

impl Default for StandardErrorClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorClassifier for StandardErrorClassifier {
    fn classify_error(
        &self,
        error: &OrchestrationError,
        context: &ErrorContext,
    ) -> ErrorClassification {
        match error {
            OrchestrationError::TimeoutError {
                operation,
                timeout_duration,
            } => self.classify_timeout_error(operation, *timeout_duration, context),
            OrchestrationError::DatabaseError { operation, reason } => {
                self.classify_database_error(operation, reason, context)
            }
            OrchestrationError::StateTransitionFailed {
                entity_type,
                entity_id,
                reason,
            } => self.classify_state_error(entity_type, *entity_id, reason, context),
            OrchestrationError::ConfigurationError { source, reason } => {
                self.classify_configuration_error(source, reason, context)
            }
            OrchestrationError::ValidationError { field, reason } => {
                self.classify_validation_error(field, reason, context)
            }
            OrchestrationError::ExecutionError(execution_error) => {
                self.classify_execution_error(execution_error, context)
            }
            _ => {
                // Generic classification for other error types
                let is_retryable = context.attempt_number < context.max_attempts;
                ErrorClassification {
                    error_category: ErrorCategory::Unknown,
                    is_retryable,
                    retry_delay: if is_retryable {
                        Some(self.calculate_exponential_backoff(context.attempt_number))
                    } else {
                        None
                    },
                    error_code: "UNKNOWN_ERROR".to_string(),
                    error_message: format!(
                        "Unknown error in step '{}': {}",
                        context.step_name, error
                    ),
                    remediation_suggestions: vec![
                        "Review error details and logs".to_string(),
                        "Check step implementation".to_string(),
                        "Verify system health".to_string(),
                    ],
                    is_final_attempt: context.attempt_number >= context.max_attempts,
                    confidence: 0.50,
                    metadata: HashMap::from([(
                        "error_type".to_string(),
                        serde_json::json!(format!("{:?}", error)),
                    )]),
                }
            }
        }
    }

    fn classifier_name(&self) -> &'static str {
        "StandardErrorClassifier"
    }

    fn can_classify(&self, _error: &OrchestrationError) -> bool {
        // Standard classifier can handle all error types
        true
    }
}

/// Convert ErrorClassification to StepExecutionError for framework compatibility
impl From<ErrorClassification> for StepExecutionError {
    fn from(classification: ErrorClassification) -> Self {
        match classification.error_category {
            ErrorCategory::Permanent | ErrorCategory::Configuration => {
                StepExecutionError::Permanent {
                    message: classification.error_message,
                    error_code: Some(classification.error_code),
                }
            }
            ErrorCategory::Timeout => StepExecutionError::Timeout {
                message: classification.error_message,
                timeout_duration: classification
                    .retry_delay
                    .unwrap_or(Duration::from_secs(30)),
            },
            ErrorCategory::Network => StepExecutionError::NetworkError {
                message: classification.error_message,
                status_code: None,
            },
            _ => StepExecutionError::Retryable {
                message: classification.error_message,
                retry_after: classification.retry_delay.map(|d| d.as_secs()),
                skip_retry: !classification.is_retryable,
                context: Some(classification.metadata),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_context() -> ErrorContext {
        ErrorContext {
            step_id: 123,
            task_id: 456,
            attempt_number: 2,
            max_attempts: 5,
            execution_duration: Duration::from_secs(30),
            step_name: "test_step".to_string(),
            error_source: "test_source".to_string(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_timeout_error_classification() {
        let classifier = StandardErrorClassifier::new();
        let context = create_test_context();
        let error = OrchestrationError::TimeoutError {
            operation: "test_operation".to_string(),
            timeout_duration: Duration::from_secs(30),
        };

        let classification = classifier.classify_error(&error, &context);

        assert_eq!(classification.error_category, ErrorCategory::Timeout);
        assert!(classification.is_retryable);
        assert!(classification.retry_delay.is_some());
        assert_eq!(classification.error_code, "EXECUTION_TIMEOUT");
        assert!(!classification.is_final_attempt);
    }

    #[test]
    fn test_database_error_classification() {
        let classifier = StandardErrorClassifier::new();
        let context = create_test_context();
        let error = OrchestrationError::DatabaseError {
            operation: "insert".to_string(),
            reason: "connection timeout".to_string(),
        };

        let classification = classifier.classify_error(&error, &context);

        assert_eq!(classification.error_category, ErrorCategory::Network);
        assert!(classification.is_retryable);
        assert_eq!(classification.error_code, "DATABASE_CONNECTION_ERROR");
    }

    #[test]
    fn test_configuration_error_classification() {
        let classifier = StandardErrorClassifier::new();
        let context = create_test_context();
        let error = OrchestrationError::ConfigurationError {
            source: "yaml_file".to_string(),
            reason: "invalid syntax".to_string(),
        };

        let classification = classifier.classify_error(&error, &context);

        assert_eq!(classification.error_category, ErrorCategory::Configuration);
        assert!(!classification.is_retryable);
        assert!(classification.retry_delay.is_none());
        assert_eq!(classification.error_code, "CONFIGURATION_ERROR");
        assert!(classification.is_final_attempt);
    }

    #[test]
    fn test_final_attempt_handling() {
        let classifier = StandardErrorClassifier::new();
        let mut context = create_test_context();
        context.attempt_number = 5; // Equal to max_attempts

        let error = OrchestrationError::DatabaseError {
            operation: "select".to_string(),
            reason: "temporary failure".to_string(),
        };

        let classification = classifier.classify_error(&error, &context);

        assert!(!classification.is_retryable); // Should not retry on final attempt
        assert!(classification.retry_delay.is_none());
        assert!(classification.is_final_attempt);
    }

    #[test]
    fn test_error_classification_to_step_execution_error() {
        let classification = ErrorClassification {
            error_category: ErrorCategory::Transient,
            is_retryable: true,
            retry_delay: Some(Duration::from_secs(5)),
            error_code: "TEST_ERROR".to_string(),
            error_message: "Test error message".to_string(),
            remediation_suggestions: vec![],
            is_final_attempt: false,
            confidence: 0.8,
            metadata: HashMap::new(),
        };

        let step_error: StepExecutionError = classification.into();

        match step_error {
            StepExecutionError::Retryable {
                message,
                retry_after,
                skip_retry,
                ..
            } => {
                assert_eq!(message, "Test error message");
                assert_eq!(retry_after, Some(5));
                assert!(!skip_retry);
            }
            _ => panic!("Expected Retryable error"),
        }
    }
}

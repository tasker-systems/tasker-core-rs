use crate::{TaskerError, TaskerResult};
use serde::{Deserialize, Serialize};

/// Type of orchestration executor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutorType {
    /// Processes incoming task requests from the task request queue
    TaskRequestProcessor,
    /// Runs orchestration cycles for task claiming and step enqueueing
    OrchestrationLoop,
    /// Processes step completion results from orchestration result queue
    StepResultProcessor,
}

impl ExecutorType {
    /// Get human-readable name for the executor type
    pub fn name(&self) -> &'static str {
        match self {
            ExecutorType::TaskRequestProcessor => "TaskRequestProcessor",
            ExecutorType::OrchestrationLoop => "OrchestrationLoop",
            ExecutorType::StepResultProcessor => "StepResultProcessor",
        }
    }

    /// Get default polling interval for this executor type (in milliseconds)
    pub fn default_polling_interval_ms(&self) -> u64 {
        match self {
            ExecutorType::TaskRequestProcessor => 250, // Task request polling interval
            ExecutorType::OrchestrationLoop => 100,    // High frequency for orchestration
            ExecutorType::StepResultProcessor => 100,  // High frequency for results
        }
    }

    /// Get default batch size for this executor type
    pub fn default_batch_size(&self) -> usize {
        match self {
            ExecutorType::TaskRequestProcessor => 10, // Task request batch size
            ExecutorType::OrchestrationLoop => 50,    // Tasks per orchestration cycle
            ExecutorType::StepResultProcessor => 20,  // Step results batch size
        }
    }

    /// Get all executor types
    pub fn all() -> Vec<ExecutorType> {
        vec![
            ExecutorType::TaskRequestProcessor,
            ExecutorType::OrchestrationLoop,
            ExecutorType::StepResultProcessor,
        ]
    }
}

/// Configuration parameters for an executor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    /// Polling interval in milliseconds
    pub polling_interval_ms: u64,
    /// Maximum batch size for processing
    pub batch_size: usize,
    /// Processing timeout in milliseconds
    pub processing_timeout_ms: u64,
    /// Maximum number of retries for failed operations
    pub max_retries: u32,
    /// Backpressure factor (0.0 - 1.0)
    pub backpressure_factor: f64,
    /// Whether circuit breaker is enabled
    pub circuit_breaker_enabled: bool,
    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,
}

impl ExecutorConfig {
    /// Create default configuration for an executor type
    pub fn default_for_type(executor_type: ExecutorType) -> Self {
        Self {
            polling_interval_ms: executor_type.default_polling_interval_ms(),
            batch_size: executor_type.default_batch_size(),
            processing_timeout_ms: 30000, // 30 seconds
            max_retries: 3,
            backpressure_factor: 1.0,
            circuit_breaker_enabled: true,
            circuit_breaker_threshold: 5,
        }
    }

    /// Apply backpressure factor to polling interval
    pub fn effective_polling_interval_ms(&self) -> u64 {
        let base_interval = self.polling_interval_ms as f64;
        let adjusted_interval = base_interval / self.backpressure_factor;
        // Ensure minimum interval of 10ms and maximum of 10 seconds
        (adjusted_interval as u64).clamp(10, 10000)
    }

    /// Apply backpressure factor to batch size
    pub fn effective_batch_size(&self) -> usize {
        let base_size = self.batch_size as f64;
        let adjusted_size = base_size * self.backpressure_factor;
        // Ensure minimum batch size of 1
        (adjusted_size as usize).max(1)
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> TaskerResult<()> {
        if self.polling_interval_ms == 0 {
            return Err(TaskerError::ConfigurationError(
                "polling_interval_ms must be greater than 0".to_string(),
            ));
        }

        if self.batch_size == 0 {
            return Err(TaskerError::ConfigurationError(
                "batch_size must be greater than 0".to_string(),
            ));
        }

        if self.processing_timeout_ms == 0 {
            return Err(TaskerError::ConfigurationError(
                "processing_timeout_ms must be greater than 0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.backpressure_factor) {
            return Err(TaskerError::ConfigurationError(
                "backpressure_factor must be between 0.0 and 1.0".to_string(),
            ));
        }

        Ok(())
    }
}

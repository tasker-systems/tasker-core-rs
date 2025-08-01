//! Result Aggregation Handler (pgmq-based)
//!
//! Simplified handler for pgmq architecture - delegates to orchestration result processor

use std::sync::Arc;
use tracing::{debug, error, info};

use crate::orchestration::{
    OrchestrationResultProcessor, StepError,
};

/// Result aggregation handler for pgmq architecture
pub struct ResultAggregationHandler {
    /// Orchestration result processor for handling orchestration logic
    result_processor: Arc<OrchestrationResultProcessor>,

    /// Configuration for result handling
    config: ResultHandlerConfig,
}

/// Configuration for result handling
#[derive(Debug, Clone)]
pub struct ResultHandlerConfig {
    /// Enable result tracking for monitoring
    pub enable_tracking: bool,

    /// Enable worker load management
    pub enable_load_management: bool,
}

impl Default for ResultHandlerConfig {
    fn default() -> Self {
        Self {
            enable_tracking: true,
            enable_load_management: true,
        }
    }
}

impl ResultAggregationHandler {
    /// Create a new result aggregation handler
    pub fn new(
        result_processor: Arc<OrchestrationResultProcessor>,
        config: ResultHandlerConfig,
    ) -> Self {
        Self {
            result_processor,
            config,
        }
    }

    /// Handler name for identification
    pub fn handler_name(&self) -> &str {
        "ResultAggregationHandler"
    }

    /// Handle partial result from worker (simplified for pgmq)
    pub async fn handle_partial_result(
        &self,
        step_id: i64,
        status: String,
        output: Option<serde_json::Value>,
        error: Option<StepError>,
        execution_time_ms: u64,
        worker_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Processing partial result for step {} from worker {}",
            step_id, worker_id
        );

        // Delegate to OrchestrationResultProcessor
        self.result_processor
            .handle_partial_result(
                0, // batch_id not needed in pgmq architecture
                step_id,
                status,
                output,
                error,
                execution_time_ms,
                worker_id,
            )
            .await
    }

    /// Check if result tracking is enabled
    pub fn is_tracking_enabled(&self) -> bool {
        self.config.enable_tracking
    }
}

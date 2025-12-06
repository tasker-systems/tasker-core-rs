//! # Completion Processor Service
//!
//! TAS-67: Processes step completion results from the completion channel.
//!
//! This service receives completed step results and routes them to the
//! orchestration queue via `FFICompletionService`.
//!
//! ## Architecture
//!
//! ```text
//! completion_receiver → CompletionProcessorService → FFICompletionService → Orchestration Queue
//! ```
//!
//! ## Domain Event Publishing
//!
//! Domain events are handled upstream by `PostHandlerCallback` (typically `DomainEventCallback`)
//! which is invoked by `FfiDispatchChannel` or `HandlerDispatchService` AFTER the result
//! is successfully sent to the completion channel. This ensures domain events only fire
//! after the result is committed to the internal pipeline.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, error, info};

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::system_context::SystemContext;

use crate::worker::services::FFICompletionService;

/// Configuration for the completion processor service
#[derive(Debug, Clone)]
pub struct CompletionProcessorConfig {
    /// Service identifier for logging
    pub service_id: String,
}

impl Default for CompletionProcessorConfig {
    fn default() -> Self {
        Self {
            service_id: "completion-processor".to_string(),
        }
    }
}

/// Completion processor service for routing step results to orchestration
///
/// TAS-67: Receives completed step results from the completion channel
/// and routes them to the orchestration queue. Domain events are handled
/// upstream by `PostHandlerCallback` before results reach this service.
pub struct CompletionProcessorService {
    /// Channel receiver for completion results
    completion_receiver: mpsc::Receiver<StepExecutionResult>,
    /// FFI completion service for sending to orchestration
    ffi_completion_service: FFICompletionService,
    /// Service identifier for logging
    service_id: String,
}

impl std::fmt::Debug for CompletionProcessorService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletionProcessorService")
            .field("service_id", &self.service_id)
            .finish()
    }
}

impl CompletionProcessorService {
    /// Create a new completion processor service
    ///
    /// # Arguments
    ///
    /// * `completion_receiver` - Channel to receive completion results
    /// * `context` - System context for dependencies
    /// * `worker_id` - Worker identifier
    /// * `config` - Service configuration
    pub fn new(
        completion_receiver: mpsc::Receiver<StepExecutionResult>,
        context: Arc<SystemContext>,
        worker_id: String,
        config: CompletionProcessorConfig,
    ) -> Self {
        let ffi_completion_service = FFICompletionService::new(worker_id, context);

        Self {
            completion_receiver,
            ffi_completion_service,
            service_id: config.service_id,
        }
    }

    /// Run the completion processor service
    ///
    /// This method runs until the completion channel is closed.
    pub async fn run(mut self) {
        info!(
            service_id = %self.service_id,
            "Completion processor service starting"
        );

        while let Some(result) = self.completion_receiver.recv().await {
            self.process_completion(result).await;
        }

        info!(
            service_id = %self.service_id,
            "Completion processor service stopped - channel closed"
        );
    }

    /// Process a single completion result
    async fn process_completion(&self, result: StepExecutionResult) {
        let step_uuid = result.step_uuid;
        let success = result.success;

        debug!(
            service_id = %self.service_id,
            step_uuid = %step_uuid,
            success = success,
            "Processing completion result"
        );

        // Send result to orchestration
        if let Err(e) = self
            .ffi_completion_service
            .send_step_result(result.clone())
            .await
        {
            error!(
                service_id = %self.service_id,
                step_uuid = %step_uuid,
                error = %e,
                "Failed to send step result to orchestration"
            );
            // Continue - don't fail the entire service
        } else {
            debug!(
                service_id = %self.service_id,
                step_uuid = %step_uuid,
                "Step result sent to orchestration"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_processor_config_default() {
        let config = CompletionProcessorConfig::default();
        assert_eq!(config.service_id, "completion-processor");
    }
}

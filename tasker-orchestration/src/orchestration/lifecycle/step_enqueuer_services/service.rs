//! Step Enqueuer Service
//!
//! Main service for orchestrating step enqueueing with batch and single task processing.

use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

use super::batch_processor::BatchProcessor;
use super::task_processor::TaskProcessor;
use super::types::StepEnqueuerServiceResult;
use crate::orchestration::lifecycle::step_enqueuer::StepEnqueuer;
use crate::orchestration::StepEnqueueResult;
use tasker_shared::config::orchestration::TaskClaimStepEnqueuerConfig;
use tasker_shared::database::sql_functions::ReadyTaskInfo;
use tasker_shared::{SystemContext, TaskerResult};

/// Main orchestration loop coordinator
pub struct StepEnqueuerService {
    batch_processor: BatchProcessor,
    task_processor: TaskProcessor,
    context: Arc<SystemContext>,
    config: TaskClaimStepEnqueuerConfig,
}

impl StepEnqueuerService {
    /// Create a new orchestration loop
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        let config: TaskClaimStepEnqueuerConfig = context.tasker_config.clone().into();
        let step_enqueuer = Arc::new(StepEnqueuer::new(context.clone()).await?);

        let batch_processor = BatchProcessor::new(context.clone(), step_enqueuer.clone(), config.clone());
        let task_processor = TaskProcessor::new(context.clone(), step_enqueuer);

        Ok(Self {
            batch_processor,
            task_processor,
            context,
            config,
        })
    }

    /// Run a single orchestration cycle using state machine approach
    #[instrument(skip(self), fields(system_id = %self.context.processor_uuid()))]
    pub async fn process_batch(&self) -> TaskerResult<StepEnqueuerServiceResult> {
        self.batch_processor.process_batch().await
    }

    /// Process a single task from ready info
    pub async fn process_single_task_from_ready_info(
        &self,
        task_info: &ReadyTaskInfo,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        self.task_processor.process_from_ready_info(task_info).await
    }

    /// Process a single task from UUID
    pub async fn process_single_task_from_uuid(
        &self,
        task_uuid: Uuid,
    ) -> TaskerResult<Option<StepEnqueueResult>> {
        self.task_processor.process_from_uuid(task_uuid).await
    }

    /// Get current configuration
    pub fn config(&self) -> &TaskClaimStepEnqueuerConfig {
        &self.config
    }

    /// Get processor ID (compatibility method)
    pub fn processor_uuid(&self) -> String {
        self.context.processor_uuid.to_string()
    }
}

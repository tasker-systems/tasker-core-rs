//! # FFI Completion Actor
//!
//! TAS-69: Actor for step completion processing.
//!
//! Handles step result processing and orchestration notification
//! by delegating to FFICompletionService.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info};

use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;

use super::messages::{ProcessStepCompletionMessage, SendStepResultMessage};
use super::traits::{Handler, Message, WorkerActor};
use crate::worker::services::FFICompletionService;

/// FFI Completion Actor
///
/// TAS-69: Wraps FFICompletionService with actor interface for message-based
/// step completion coordination.
pub struct FFICompletionActor {
    context: Arc<SystemContext>,
    service: FFICompletionService,
}

impl std::fmt::Debug for FFICompletionActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FFICompletionActor")
            .field("context", &"Arc<SystemContext>")
            .field("service", &self.service)
            .finish()
    }
}

impl FFICompletionActor {
    /// Create a new FFICompletionActor
    pub fn new(context: Arc<SystemContext>, worker_id: String) -> Self {
        let service = FFICompletionService::new(worker_id, context.clone());

        Self { context, service }
    }
}

impl WorkerActor for FFICompletionActor {
    fn name(&self) -> &'static str {
        "FFICompletionActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "FFICompletionActor started");
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "FFICompletionActor stopped");
        Ok(())
    }
}

#[async_trait]
impl Handler<SendStepResultMessage> for FFICompletionActor {
    async fn handle(&self, msg: SendStepResultMessage) -> TaskerResult<<SendStepResultMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            step_uuid = %msg.result.step_uuid,
            success = msg.result.success,
            "Handling SendStepResultMessage"
        );

        self.service.send_step_result(msg.result).await
    }
}

#[async_trait]
impl Handler<ProcessStepCompletionMessage> for FFICompletionActor {
    async fn handle(&self, msg: ProcessStepCompletionMessage) -> TaskerResult<<ProcessStepCompletionMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            step_uuid = %msg.step_result.step_uuid,
            correlation_id = ?msg.correlation_id,
            "Handling ProcessStepCompletionMessage"
        );

        // Process the step completion by sending result to orchestration
        self.service.send_step_result(msg.step_result).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_ffi_completion_actor_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let actor = FFICompletionActor::new(
            context.clone(),
            format!("worker_{}", uuid::Uuid::new_v4()),
        );

        assert_eq!(actor.name(), "FFICompletionActor");
    }

    #[test]
    fn test_ffi_completion_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FFICompletionActor>();
    }
}

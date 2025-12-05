//! # Template Cache Actor
//!
//! TAS-69: Actor for template cache management.
//!
//! Handles template cache operations by delegating to TaskTemplateManager.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info};

use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;

use super::messages::RefreshTemplateCacheMessage;
use super::traits::{Handler, Message, WorkerActor};
use crate::worker::task_template_manager::TaskTemplateManager;

/// Template Cache Actor
///
/// TAS-69: Wraps TaskTemplateManager with actor interface for message-based
/// cache management.
pub struct TemplateCacheActor {
    context: Arc<SystemContext>,
    task_template_manager: Arc<TaskTemplateManager>,
}

impl std::fmt::Debug for TemplateCacheActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateCacheActor")
            .field("context", &"Arc<SystemContext>")
            .field("task_template_manager", &"Arc<TaskTemplateManager>")
            .finish()
    }
}

impl TemplateCacheActor {
    /// Create a new TemplateCacheActor
    pub fn new(
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> Self {
        Self {
            context,
            task_template_manager,
        }
    }

    /// Get the task template manager reference
    pub fn task_template_manager(&self) -> &Arc<TaskTemplateManager> {
        &self.task_template_manager
    }
}

impl WorkerActor for TemplateCacheActor {
    fn name(&self) -> &'static str {
        "TemplateCacheActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "TemplateCacheActor started");
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "TemplateCacheActor stopped");
        Ok(())
    }
}

#[async_trait]
impl Handler<RefreshTemplateCacheMessage> for TemplateCacheActor {
    async fn handle(
        &self,
        msg: RefreshTemplateCacheMessage,
    ) -> TaskerResult<<RefreshTemplateCacheMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            namespace = ?msg.namespace,
            "Handling RefreshTemplateCacheMessage"
        );

        match msg.namespace {
            Some(ref ns) => {
                info!(
                    actor = self.name(),
                    namespace = ns,
                    "Refreshing template cache for namespace"
                );
                // TODO: Implement namespace-specific cache refresh
                // For now, just log the request
            }
            None => {
                info!(actor = self.name(), "Refreshing entire template cache");
                // TODO: Implement full cache refresh
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_template_cache_actor_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let actor = TemplateCacheActor::new(context.clone(), task_template_manager);

        assert_eq!(actor.name(), "TemplateCacheActor");
    }

    #[test]
    fn test_template_cache_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TemplateCacheActor>();
    }
}

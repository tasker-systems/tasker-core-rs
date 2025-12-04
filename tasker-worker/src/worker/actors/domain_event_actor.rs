//! # Domain Event Actor
//!
//! TAS-69: Actor for domain event dispatching.
//!
//! Wraps the existing DomainEventSystem with a thin actor layer
//! for consistent message-based coordination.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info};

use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;

use super::messages::DispatchEventsMessage;
use super::traits::{Handler, Message, WorkerActor};
use crate::worker::event_systems::domain_event_system::DomainEventSystemHandle;

/// Domain Event Actor
///
/// TAS-69: Wraps DomainEventSystemHandle with actor interface for message-based
/// event dispatching.
///
/// This is a thin wrapper that provides consistent actor-based access to
/// the existing DomainEventSystem. Domain events use fire-and-forget
/// semantics (try_send) - never blocking the workflow step execution.
pub struct DomainEventActor {
    context: Arc<SystemContext>,
    handle: Option<DomainEventSystemHandle>,
}

impl std::fmt::Debug for DomainEventActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainEventActor")
            .field("context", &"Arc<SystemContext>")
            .field("handle", &self.handle.is_some())
            .finish()
    }
}

impl DomainEventActor {
    /// Create a new DomainEventActor without a handle
    ///
    /// The handle can be set later via `set_handle()` once the
    /// DomainEventSystem is initialized.
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self {
            context,
            handle: None,
        }
    }

    /// Create a new DomainEventActor with an existing handle
    pub fn with_handle(context: Arc<SystemContext>, handle: DomainEventSystemHandle) -> Self {
        Self {
            context,
            handle: Some(handle),
        }
    }

    /// Set the domain event system handle
    pub fn set_handle(&mut self, handle: DomainEventSystemHandle) {
        self.handle = Some(handle);
    }

    /// Get the domain event system handle if available
    pub fn handle(&self) -> Option<&DomainEventSystemHandle> {
        self.handle.as_ref()
    }
}

impl WorkerActor for DomainEventActor {
    fn name(&self) -> &'static str {
        "DomainEventActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(
            actor = self.name(),
            handle_configured = self.handle.is_some(),
            "DomainEventActor started"
        );
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "DomainEventActor stopped");
        Ok(())
    }
}

#[async_trait]
impl Handler<DispatchEventsMessage> for DomainEventActor {
    async fn handle(&self, msg: DispatchEventsMessage) -> TaskerResult<<DispatchEventsMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            event_count = msg.events.len(),
            publisher = %msg.publisher_name,
            correlation_id = %msg.correlation_id,
            "Handling DispatchEventsMessage"
        );

        match &self.handle {
            Some(handle) => {
                let dispatched = handle.dispatch_events(
                    msg.events,
                    msg.publisher_name,
                    msg.correlation_id,
                );
                Ok(dispatched)
            }
            None => {
                debug!(
                    actor = self.name(),
                    "No domain event handle configured, skipping dispatch"
                );
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_domain_event_actor_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let actor = DomainEventActor::new(context.clone());

        assert_eq!(actor.name(), "DomainEventActor");
        assert!(actor.handle().is_none());
    }

    #[test]
    fn test_domain_event_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DomainEventActor>();
    }
}

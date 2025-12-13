//! # Core Worker Actor Traits
//!
//! TAS-69: Defines the foundational traits for the lightweight actor pattern used
//! throughout the worker system. These traits mirror the orchestration actor traits
//! (TAS-46) to provide consistent patterns across both systems.
//!
//! ## Architecture
//!
//! ```text
//! WorkerCommand ────→ WorkerActorRegistry ────→ Specific Actor
//!                            │                        │
//!                            │                        ├─→ Handler<M>
//!                            │                        │
//!                            └────────────────────────┴─→ Service Component
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use tasker_shared::{system_context::SystemContext, TaskerResult};

/// Base trait for all worker actors
///
/// Provides common functionality and lifecycle hooks for actors that manage
/// worker components. This trait establishes the foundation for the actor
/// pattern without requiring a full actor framework.
///
/// ## Lifecycle
///
/// Actors have optional lifecycle hooks:
/// - `started()`: Called when the actor is initialized (default: no-op)
/// - `stopped()`: Called when the actor is being shut down (default: no-op)
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_worker::worker::actors::WorkerActor;
/// use std::sync::Arc;
/// use tasker_shared::system_context::SystemContext;
///
/// pub struct MyActor {
///     context: Arc<SystemContext>,
/// }
///
/// impl WorkerActor for MyActor {
///     fn name(&self) -> &'static str {
///         "MyActor"
///     }
///
///     fn context(&self) -> &Arc<SystemContext> {
///         &self.context
///     }
/// }
/// ```
pub trait WorkerActor: Send + Sync + 'static {
    /// Actor name for logging and metrics
    ///
    /// Should be a static string that identifies this actor type.
    /// Used in logging, metrics, and debugging.
    fn name(&self) -> &'static str;

    /// Access to shared system context
    ///
    /// Provides access to database pools, configuration, message clients,
    /// and other shared resources needed by the actor.
    fn context(&self) -> &Arc<SystemContext>;

    /// Called when the actor is started (optional)
    ///
    /// Override this method to initialize resources, subscribe to events,
    /// or perform other startup tasks. Default implementation is a no-op.
    ///
    /// # Errors
    ///
    /// Return an error if actor startup fails. The error will be propagated
    /// to the caller, preventing the actor from being registered.
    fn started(&mut self) -> TaskerResult<()> {
        Ok(())
    }

    /// Called when the actor is being stopped (optional)
    ///
    /// Override this method to clean up resources, unsubscribe from events,
    /// or perform other shutdown tasks. Default implementation is a no-op.
    ///
    /// # Errors
    ///
    /// Return an error if actor shutdown fails. The error will be logged
    /// but not propagated, allowing other actors to shut down cleanly.
    fn stopped(&mut self) -> TaskerResult<()> {
        Ok(())
    }
}

/// Message handler trait for specific message types
///
/// Actors implement this trait for each message type they can handle.
/// This allows type-safe message routing and clear separation of concerns.
///
/// ## Type Parameters
///
/// - `M`: The message type this handler processes (must implement `Message`)
///
/// ## Example
///
/// ```rust,ignore
/// use crate::worker::actors::{Handler, Message, WorkerActor};
/// use async_trait::async_trait;
/// use tasker_shared::TaskerResult;
///
/// // Define a message
/// pub struct ExecuteStepMessage {
///     pub step_uuid: Uuid,
/// }
///
/// impl Message for ExecuteStepMessage {
///     type Response = ();
/// }
///
/// // Implement handler
/// #[async_trait]
/// impl Handler<ExecuteStepMessage> for StepExecutorActor {
///     async fn handle(&self, msg: ExecuteStepMessage)
///         -> TaskerResult<()> {
///         self.service.execute_step(msg.step_uuid).await
///     }
/// }
/// ```
#[async_trait]
pub trait Handler<M: Message>: WorkerActor {
    /// Handle a message and return a response
    ///
    /// This method processes the incoming message and returns a result.
    /// Actors delegate to their underlying service components to perform
    /// the actual work.
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to process
    ///
    /// # Returns
    ///
    /// A `TaskerResult` containing either the response or an error.
    ///
    /// # Errors
    ///
    /// Returns an error if message processing fails. The error should
    /// provide enough context for debugging and observability.
    async fn handle(&self, msg: M) -> TaskerResult<M::Response>;
}

/// Marker trait for command messages
///
/// All messages sent to actors must implement this trait. It defines the
/// associated response type and ensures messages are Send-able across threads.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_worker::worker::actors::Message;
/// use uuid::Uuid;
///
/// pub struct ExecuteStepMessage {
///     pub step_uuid: Uuid,
///     pub queue_name: String,
/// }
///
/// impl Message for ExecuteStepMessage {
///     type Response = ();
/// }
/// ```
pub trait Message: Send + 'static {
    /// The response type for this message
    ///
    /// This defines what type will be returned when an actor handles
    /// this message. Must be Send to allow cross-thread communication.
    type Response: Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test message type
    struct TestMessage {
        value: i32,
    }

    impl Message for TestMessage {
        type Response = String;
    }

    // Test actor
    #[derive(Clone)]
    struct TestActor {
        context: Arc<SystemContext>,
    }

    impl WorkerActor for TestActor {
        fn name(&self) -> &'static str {
            "TestActor"
        }

        fn context(&self) -> &Arc<SystemContext> {
            &self.context
        }
    }

    #[async_trait]
    impl Handler<TestMessage> for TestActor {
        async fn handle(&self, msg: TestMessage) -> TaskerResult<String> {
            Ok(format!("Received: {}", msg.value))
        }
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_actor_trait_compilation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );
        let actor = TestActor { context };
        let response = actor.handle(TestMessage { value: 42 }).await.unwrap();
        assert_eq!(response, "Received: 42");
    }

    #[test]
    fn test_message_trait_bounds() {
        // Verify that Message trait requires Send
        fn assert_send<T: Send>() {}
        assert_send::<TestMessage>();
    }

    #[test]
    fn test_worker_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        // This compiles only if TestActor satisfies WorkerActor bounds
        fn assert_worker_actor<T: WorkerActor>() {
            assert_send_sync::<T>();
        }
        assert_worker_actor::<TestActor>();
    }
}

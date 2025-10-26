//! # Core Actor Traits
//!
//! Defines the foundational traits for the lightweight actor pattern used
//! throughout the orchestration system.

use async_trait::async_trait;
use std::sync::Arc;
use tasker_shared::{system_context::SystemContext, TaskerResult};

/// Base trait for all orchestration actors
///
/// Provides common functionality and lifecycle hooks for actors that manage
/// orchestration components. This trait establishes the foundation for the
/// actor pattern without requiring a full actor framework.
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
/// use tasker_orchestration::actors::OrchestrationActor;
/// use std::sync::Arc;
/// use tasker_shared::system_context::SystemContext;
///
/// pub struct MyActor {
///     context: Arc<SystemContext>,
/// }
///
/// impl OrchestrationActor for MyActor {
///     fn name(&self) -> &'static str {
///         "MyActor"
///     }
///
///     fn context(&self) -> &Arc<SystemContext> {
///         &self.context
///     }
///
///     // Optional: implement started/stopped for resource management
/// }
/// ```
pub trait OrchestrationActor: Send + Sync + 'static {
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
    #[allow(unused_variables)]
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
    #[allow(unused_variables)]
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
/// ```rust,no_run
/// use tasker_orchestration::actors::{Handler, Message, OrchestrationActor};
/// use async_trait::async_trait;
/// use tasker_shared::TaskerResult;
/// use tasker_shared::system_context::SystemContext;
/// use std::sync::Arc;
/// use uuid::Uuid;
///
/// // Define a message
/// pub struct InitializeTaskMessage {
///     pub task_name: String,
/// }
///
/// impl Message for InitializeTaskMessage {
///     type Response = Uuid;
/// }
///
/// // Define an actor
/// pub struct MyActor {
///     context: Arc<SystemContext>,
/// }
///
/// impl OrchestrationActor for MyActor {
///     fn name(&self) -> &'static str { "MyActor" }
///     fn context(&self) -> &Arc<SystemContext> { &self.context }
/// }
///
/// // Implement handler
/// #[async_trait]
/// impl Handler<InitializeTaskMessage> for MyActor {
///     type Response = Uuid;
///
///     async fn handle(&self, msg: InitializeTaskMessage)
///         -> TaskerResult<Uuid> {
///         // Process message and return result
///         Ok(Uuid::new_v4())
///     }
/// }
/// ```
#[async_trait]
pub trait Handler<M: Message>: OrchestrationActor {
    /// The response type for this handler
    ///
    /// Must match `M::Response` from the message trait implementation.
    type Response: Send;

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
    async fn handle(&self, msg: M) -> TaskerResult<Self::Response>;
}

/// Marker trait for command messages
///
/// All messages sent to actors must implement this trait. It defines the
/// associated response type and ensures messages are Send-able across threads.
///
/// ## Type Parameters
///
/// - `Response`: The type returned when this message is handled
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::actors::Message;
/// use uuid::Uuid;
///
/// pub struct InitializeTaskMessage {
///     pub task_name: String,
/// }
///
/// impl Message for InitializeTaskMessage {
///     type Response = Uuid;
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

    impl OrchestrationActor for TestActor {
        fn name(&self) -> &'static str {
            "TestActor"
        }

        fn context(&self) -> &Arc<SystemContext> {
            &self.context
        }
    }

    impl TestActor {
        async fn start(&self) -> TaskerResult<Arc<TestActor>> {
            Ok(Arc::new(self.clone()))
        }

        async fn stop(&self) -> TaskerResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl Handler<TestMessage> for TestActor {
        type Response = String;

        async fn handle(&self, msg: TestMessage) -> TaskerResult<String> {
            Ok(format!("Received: {}", msg.value))
        }
    }

    #[tokio::test]
    async fn test_actor_trait_compilation() {
        // SystemContext will automatically use complete-test.toml in test builds
        let context = SystemContext::new_for_orchestration().await.unwrap();
        let actor = TestActor {
            context: Arc::new(context),
        };
        let actor_ref = actor.start().await.unwrap();
        let response = actor_ref.handle(TestMessage { value: 42 }).await.unwrap();
        assert_eq!(response, "Received: 42");
        actor.stop().await.unwrap();
    }

    #[test]
    fn test_message_trait_bounds() {
        // Verify that Message trait requires Send
        fn assert_send<T: Send>() {}
        assert_send::<TestMessage>();
    }
}

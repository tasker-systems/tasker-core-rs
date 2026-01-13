#![expect(
    dead_code,
    reason = "Test module with helpers for actor-based testing patterns"
)]
//! # Actor Test Harness
//!
//! Test infrastructure for actor-based testing patterns (TAS-46).
//!
//! ## Purpose
//!
//! Provides standardized setup and helper methods for testing actors in isolation
//! and in coordination. This harness simplifies actor-based testing by providing
//! type-safe message sending and actor lifecycle management.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use common::actor_test_harness::ActorTestHarness;
//!
//! #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
//! async fn test_actor_coordination(pool: PgPool) -> Result<()> {
//!     let harness = ActorTestHarness::new(pool).await?;
//!
//!     // Send messages to actors
//!     let msg = InitializeTaskMessage { request };
//!     let result = harness.send(&harness.task_initializer_actor, msg).await?;
//!
//!     assert!(result.task_uuid.is_some());
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use tasker_orchestration::actors::ActorRegistry;
use tasker_shared::system_context::SystemContext;

/// Test harness for actor-based testing
///
/// Provides:
/// - Initialized ActorRegistry with all actors
/// - Helper methods for sending messages to actors
/// - Lifecycle management for test setup/teardown
pub struct ActorTestHarness {
    /// Database connection pool
    pub pool: PgPool,

    /// System context for framework operations
    pub context: Arc<SystemContext>,

    /// Actor registry with all orchestration actors
    pub actors: ActorRegistry,
}

impl ActorTestHarness {
    /// Create a new ActorTestHarness with all actors initialized
    ///
    /// This initializes:
    /// 1. SystemContext from the database pool
    /// 2. ActorRegistry with all actors
    /// 3. Calls started() on all actors
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool (usually from #[sqlx::test])
    ///
    /// # Returns
    ///
    /// A configured ActorTestHarness ready for testing
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - SystemContext initialization fails
    /// - ActorRegistry build fails
    /// - Any actor's started() hook fails
    pub async fn new(pool: PgPool) -> Result<Self> {
        tracing::info!("🔧 Initializing ActorTestHarness");

        // Create SystemContext from pool
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

        // Build ActorRegistry with all actors
        let actors = ActorRegistry::build(context.clone()).await?;

        tracing::info!("✅ ActorTestHarness initialized successfully");

        Ok(Self {
            pool,
            context,
            actors,
        })
    }

    /// Get a reference to the actor registry
    ///
    /// Provides direct access to the ActorRegistry for accessing
    /// specific actors.
    pub fn registry(&self) -> &ActorRegistry {
        &self.actors
    }

    /// Get a reference to the system context
    ///
    /// Provides access to the underlying SystemContext for operations
    /// that require database pool, configuration, etc.
    pub fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    /// Shutdown all actors gracefully
    ///
    /// Calls stopped() on all actors. Should be called at the end
    /// of tests that modify actor state.
    pub async fn shutdown(&mut self) {
        tracing::info!("🛑 Shutting down ActorTestHarness");
        self.actors.shutdown().await;
        tracing::info!("✅ ActorTestHarness shutdown complete");
    }
}

// Phase 2: Add helper methods for sending messages to specific actors
// Example (future):
// impl ActorTestHarness {
//     /// Send a message to TaskInitializerActor
//     pub async fn initialize_task(
//         &self,
//         request: TaskRequest,
//     ) -> Result<TaskInitializationResult> {
//         let msg = InitializeTaskMessage { request };
//         self.actors.task_initializer_actor.handle(msg).await
//     }
//
//     /// Send a message to StepEnqueuerActor
//     pub async fn enqueue_steps(
//         &self,
//         task_info: &ReadyTaskInfo,
//     ) -> Result<StepEnqueueResult> {
//         let msg = EnqueueStepsMessage { task_info: task_info.clone() };
//         self.actors.step_enqueuer_actor.handle(msg).await
//     }
// }

#[cfg(test)]
mod tests {
    #[test]
    fn test_actor_test_harness_compiles() {
        // Verify the test harness structure compiles
        // Integration tests will be added in Phase 2
    }
}

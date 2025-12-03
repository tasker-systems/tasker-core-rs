#![allow(dead_code)]
//! # Lifecycle Test Manager
//!
//! Access test helpers via `use crate::common::*` instead of `tasker_core::test_helpers`
//!
//! Unified test infrastructure for framework lifecycle integration testing (TAS-42).
//!
//! ## Purpose
//!
//! Provides standardized setup and helper methods for testing SQL function integration
//! with actual framework state machine transitions and orchestration lifecycle.
//!
//! ## Architecture
//!
//! Similar to IntegrationTestManager but focused on lifecycle testing:
//! - TaskTemplate registration from YAML fixtures
//! - TaskRequest creation helpers
//! - SystemContext and service initialization
//! - SQL function validation helpers
//!
//! ## Usage
//!
//! ```rust,no_run
//! use common::lifecycle_test_manager::LifecycleTestManager;
//!
//! #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
//! async fn test_example(pool: PgPool) -> Result<()> {
//!     let manager = LifecycleTestManager::new(pool).await?;
//!
//!     // Create task from diamond pattern template
//!     let task_request = manager.create_task_request_for_template(
//!         "diamond_pattern",
//!         "diamond_workflow",
//!         serde_json::json!({"even_number": 6})
//!     );
//!
//!     // Initialize task using framework
//!     let init_result = manager.initialize_task(task_request).await?;
//!
//!     // Validate with SQL functions
//!     manager.validate_task_execution_context(init_result.task_uuid, 4, 0, 1).await?;
//!
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_orchestration::orchestration::lifecycle::{
    step_enqueuer_services::StepEnqueuerService,
    task_initialization::{TaskInitializationResult, TaskInitializer},
};
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::models::WorkflowStep;
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::state_machine::{
    events::StepEvent, states::WorkflowStepState, step_state_machine::StepStateMachine,
};
use tasker_shared::system_context::SystemContext;
use uuid::Uuid;

// TAS-46: Import actor testing infrastructure
use crate::common::actor_test_harness::ActorTestHarness;

/// Manager for lifecycle integration testing
///
/// ## TAS-46 Actor Migration
///
/// This manager supports both legacy direct service access and new actor-based testing.
/// Use the `_via_actor()` methods for new tests. Legacy methods are maintained for
/// backward compatibility and will be deprecated in a future release.
pub struct LifecycleTestManager {
    /// Database connection pool
    pub pool: PgPool,
    /// System context for framework operations
    pub system_context: Arc<SystemContext>,
    /// Task handler registry with loaded templates
    #[allow(dead_code)]
    pub registry: TaskHandlerRegistry,

    // ========== Legacy Fields (Backward Compatibility) ==========
    /// Task initializer for creating tasks (legacy - prefer actor_harness)
    ///
    /// **Deprecated**: Use `actor_harness` for new tests. This field will be
    /// removed in a future release after all tests are migrated.
    pub task_initializer: TaskInitializer,

    // ========== Actor-Based Fields (TAS-46) ==========
    /// Actor test harness for actor-based testing (TAS-46)
    ///
    /// Provides access to all orchestration actors for type-safe message-based testing.
    /// This is the preferred approach for new tests.
    pub actor_harness: ActorTestHarness,

    /// Path to task template fixtures
    #[allow(dead_code)]
    pub template_path: String,
}

impl LifecycleTestManager {
    /// Create a new LifecycleTestManager with all services initialized
    pub async fn new(pool: PgPool) -> Result<Self> {
        Self::with_template_path(pool, "tests/fixtures/task_templates/rust").await
    }

    /// Create a LifecycleTestManager with custom template path
    pub async fn with_template_path(pool: PgPool, template_path: &str) -> Result<Self> {
        tracing::info!("🔧 Initializing LifecycleTestManager");

        // Create TaskHandlerRegistry and load templates
        let registry = TaskHandlerRegistry::new(pool.clone());
        let discovery_result = registry
            .discover_and_register_templates(template_path)
            .await?;

        tracing::info!(
            total_files = discovery_result.total_files,
            successful = discovery_result.successful_registrations,
            failed = discovery_result.failed_registrations,
            "✅ Templates registered"
        );

        if discovery_result.failed_registrations > 0 {
            anyhow::bail!(
                "Failed to register {} templates: {:?}",
                discovery_result.failed_registrations,
                discovery_result.errors
            );
        }

        // Create SystemContext
        let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        tracing::info!(processor_uuid = %system_context.processor_uuid, "✅ SystemContext created");

        // Create StepEnqueuerService and TaskInitializer (legacy support)
        let step_enqueuer_service =
            Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
        let task_initializer = TaskInitializer::new(system_context.clone(), step_enqueuer_service);

        // TAS-46: Create ActorTestHarness for actor-based testing
        let actor_harness = ActorTestHarness::new(pool.clone()).await?;
        tracing::info!("✅ ActorTestHarness initialized");

        tracing::info!("✅ LifecycleTestManager ready (hybrid: legacy + actor support)");

        Ok(Self {
            pool,
            system_context,
            registry,
            task_initializer,
            actor_harness,
            template_path: template_path.to_string(),
        })
    }

    /// Create a TaskRequest for a specific template with context
    pub fn create_task_request_for_template(
        &self,
        template_name: &str,
        namespace: &str,
        context: serde_json::Value,
    ) -> TaskRequest {
        TaskRequest {
            name: template_name.to_string(),
            namespace: namespace.to_string(),
            version: "1.0.0".to_string(),
            context,
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
            initiator: "lifecycle_test".to_string(),
            source_system: "integration_test".to_string(),
            reason: format!("Testing {} lifecycle", template_name),
            tags: vec!["test".to_string(), "lifecycle".to_string()],
            bypass_steps: vec![],
            requested_at: chrono::Utc::now().naive_utc(),
            options: None,
            priority: Some(5),
        }
    }

    /// Create a TaskRequest with custom options
    #[allow(dead_code)]
    pub fn create_task_request_with_options(
        &self,
        template_name: &str,
        namespace: &str,
        context: serde_json::Value,
        options: HashMap<String, serde_json::Value>,
    ) -> TaskRequest {
        let mut request = self.create_task_request_for_template(template_name, namespace, context);
        request.options = Some(options);
        request
    }

    // ========== Task Initialization Methods ==========

    /// Initialize a task using TaskInitializer (legacy method)
    ///
    /// **Note**: This is the legacy direct service access method. For new tests,
    /// consider using `initialize_task_via_actor()` once actors are implemented (Phase 2).
    pub async fn initialize_task(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult> {
        tracing::info!(
            name = %task_request.name,
            namespace = %task_request.namespace,
            "🚀 Initializing task (legacy method)"
        );

        let result = self
            .task_initializer
            .create_task_from_request(task_request)
            .await?;

        tracing::info!(
            task_uuid = %result.task_uuid,
            step_count = result.step_count,
            "✅ Task initialized (legacy method)"
        );

        Ok(result)
    }

    /// Initialize a task using actor-based approach (TAS-46 Phase 2)
    ///
    /// **Coming in Phase 2**: This method will use the TaskInitializerActor
    /// for message-based task initialization.
    ///
    /// # Example (Phase 2)
    ///
    /// ```ignore
    /// let result = manager.initialize_task_via_actor(task_request).await?;
    /// ```
    #[allow(dead_code)]
    pub async fn initialize_task_via_actor(
        &self,
        _task_request: TaskRequest,
    ) -> Result<TaskInitializationResult> {
        // Phase 2: Implement using TaskInitializerActor
        // let msg = InitializeTaskMessage { request: task_request };
        // self.actor_harness.task_initializer_actor.handle(msg).await
        todo!("TAS-46 Phase 2: Implement with TaskInitializerActor")
    }

    // ========== SQL Validation Methods ==========

    /// Validate task execution context matches expected values
    pub async fn validate_task_execution_context(
        &self,
        task_uuid: Uuid,
        expected_total_steps: i64,
        expected_completed_steps: i64,
        expected_ready_steps: i64,
    ) -> Result<()> {
        let task_context = sqlx::query!(
            "SELECT total_steps, completed_steps, ready_steps, execution_status
             FROM get_task_execution_context($1)",
            task_uuid
        )
        .fetch_one(&self.pool)
        .await?;

        tracing::info!(
            task_uuid = %task_uuid,
            total_steps = task_context.total_steps,
            completed_steps = task_context.completed_steps,
            ready_steps = task_context.ready_steps,
            execution_status = ?task_context.execution_status,
            "✅ SQL VALIDATION: get_task_execution_context"
        );

        if task_context.total_steps != Some(expected_total_steps) {
            anyhow::bail!(
                "Total steps mismatch: expected {}, got {:?}",
                expected_total_steps,
                task_context.total_steps
            );
        }

        if task_context.completed_steps != Some(expected_completed_steps) {
            anyhow::bail!(
                "Completed steps mismatch: expected {}, got {:?}",
                expected_completed_steps,
                task_context.completed_steps
            );
        }

        if task_context.ready_steps != Some(expected_ready_steps) {
            anyhow::bail!(
                "Ready steps mismatch: expected {}, got {:?}",
                expected_ready_steps,
                task_context.ready_steps
            );
        }

        Ok(())
    }

    /// Get step readiness status for a task
    /// Get the current state of a task from the most recent transition
    #[allow(dead_code)]
    pub async fn get_current_task_state(&self, task_uuid: Uuid) -> Result<Option<String>> {
        let state = sqlx::query_scalar!(
            "SELECT to_state FROM tasker_task_transitions
             WHERE task_uuid = $1 AND most_recent = true",
            task_uuid
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(state)
    }

    pub async fn get_step_readiness_status(&self, task_uuid: Uuid) -> Result<Vec<StepReadiness>> {
        let step_readiness = sqlx::query!(
            "SELECT workflow_step_uuid, name, current_state, dependencies_satisfied,
                    ready_for_execution, total_parents, completed_parents,
                    retry_eligible, attempts, max_attempts, next_retry_at
             FROM get_step_readiness_status($1)",
            task_uuid
        )
        .fetch_all(&self.pool)
        .await?;

        let results: Vec<StepReadiness> = step_readiness
            .into_iter()
            .map(|row| StepReadiness {
                workflow_step_uuid: row.workflow_step_uuid.unwrap(),
                name: row.name.unwrap(),
                current_state: row.current_state,
                dependencies_satisfied: row.dependencies_satisfied.unwrap_or(false),
                ready_for_execution: row.ready_for_execution.unwrap_or(false),
                total_parents: row.total_parents.unwrap_or(0),
                completed_parents: row.completed_parents.unwrap_or(0),
                retry_eligible: row.retry_eligible.unwrap_or(false),
                attempts: row.attempts.unwrap_or(0),
                max_attempts: row.max_attempts.unwrap_or(3),
                next_retry_at: row.next_retry_at,
            })
            .collect();

        tracing::info!(
            task_uuid = %task_uuid,
            steps_analyzed = results.len(),
            "✅ SQL VALIDATION: get_step_readiness_status"
        );

        Ok(results)
    }

    /// Validate step readiness for a specific step by name
    pub async fn validate_step_readiness(
        &self,
        task_uuid: Uuid,
        step_name: &str,
        expected_ready: bool,
        expected_total_parents: i32,
        expected_completed_parents: i32,
    ) -> Result<()> {
        let step_readiness = self.get_step_readiness_status(task_uuid).await?;

        let step = step_readiness
            .iter()
            .find(|s| s.name == step_name)
            .ok_or_else(|| anyhow::anyhow!("Step {} not found", step_name))?;

        tracing::info!(
            step = %step_name,
            ready = step.ready_for_execution,
            total_parents = step.total_parents,
            completed_parents = step.completed_parents,
            "📊 Step readiness validation"
        );

        if step.ready_for_execution != expected_ready {
            anyhow::bail!(
                "Step {} ready mismatch: expected {}, got {}",
                step_name,
                expected_ready,
                step.ready_for_execution
            );
        }

        if step.total_parents != expected_total_parents {
            anyhow::bail!(
                "Step {} total_parents mismatch: expected {}, got {}",
                step_name,
                expected_total_parents,
                step.total_parents
            );
        }

        if step.completed_parents != expected_completed_parents {
            anyhow::bail!(
                "Step {} completed_parents mismatch: expected {}, got {}",
                step_name,
                expected_completed_parents,
                step.completed_parents
            );
        }

        Ok(())
    }

    /// Count ready steps for a task
    pub async fn count_ready_steps(&self, task_uuid: Uuid) -> Result<usize> {
        let step_readiness = self.get_step_readiness_status(task_uuid).await?;
        let ready_count = step_readiness
            .iter()
            .filter(|s| s.ready_for_execution)
            .count();

        tracing::info!(
            task_uuid = %task_uuid,
            ready_steps = ready_count,
            total_steps = step_readiness.len(),
            "📊 Ready step count"
        );

        Ok(ready_count)
    }

    /// Get a workflow step by name for a task
    pub async fn get_step_by_name(&self, task_uuid: Uuid, step_name: &str) -> Result<WorkflowStep> {
        let step = sqlx::query_as::<_, WorkflowStep>(
            r#"
            SELECT ws.*
            FROM tasker_workflow_steps ws
            JOIN tasker_named_steps ns ON ws.named_step_uuid = ns.named_step_uuid
            WHERE ws.task_uuid = $1 AND ns.name = $2
            "#,
        )
        .bind(task_uuid)
        .bind(step_name)
        .fetch_one(&self.pool)
        .await?;

        Ok(step)
    }

    /// Transition a step through its state machine
    /// This also handles attempts increment when transitioning to in_progress (matching worker behavior)
    pub async fn transition_step(&self, step: &WorkflowStep, event: StepEvent) -> Result<()> {
        let mut state_machine = StepStateMachine::new(step.clone(), self.system_context.clone());
        let new_state = state_machine.transition(event.clone()).await?;

        // Increment attempts when transitioning to InProgress (matching worker claim behavior)
        // From distributed systems perspective: claiming a step = attempt begins
        // This ensures worker crashes/timeouts count against retry limits
        if matches!(new_state, WorkflowStepState::InProgress) && matches!(event, StepEvent::Start) {
            let now = chrono::Utc::now().naive_utc();
            sqlx::query!(
                "UPDATE tasker_workflow_steps
                 SET attempts = COALESCE(attempts, 0) + 1,
                     last_attempted_at = $2,
                     updated_at = NOW()
                 WHERE workflow_step_uuid = $1",
                step.workflow_step_uuid,
                now
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Complete a step with a successful result
    pub async fn complete_step(
        &self,
        task_uuid: Uuid,
        step_name: &str,
        result_data: serde_json::Value,
    ) -> Result<()> {
        let step = self.get_step_by_name(task_uuid, step_name).await?;

        // Transition to InProgress first if needed
        let current_state = sqlx::query_scalar!(
            "SELECT to_state FROM tasker_workflow_step_transitions
             WHERE workflow_step_uuid = $1 AND most_recent = true",
            step.workflow_step_uuid
        )
        .fetch_optional(&self.pool)
        .await?;

        let current = current_state.unwrap_or_else(|| "pending".to_string());

        match current.as_str() {
            "pending" => {
                self.transition_step(&step, StepEvent::Start).await?;
                self.transition_step(
                    &step,
                    StepEvent::EnqueueForOrchestration(Some(result_data.clone())),
                )
                .await?;
            }
            "enqueued" => {
                self.transition_step(&step, StepEvent::Start).await?;
                self.transition_step(
                    &step,
                    StepEvent::EnqueueForOrchestration(Some(result_data.clone())),
                )
                .await?;
            }
            "in_progress" => {
                self.transition_step(
                    &step,
                    StepEvent::EnqueueForOrchestration(Some(result_data.clone())),
                )
                .await?;
            }
            "enqueued_for_orchestration" => {
                // Already enqueued, just complete
            }
            "error" => {
                // Step is in error state - recovery by starting over
                // In real system, this would be a retry after backoff
                // The Retry event transitions error → pending, then we proceed normally
                self.transition_step(&step, StepEvent::Retry).await?;
                self.transition_step(&step, StepEvent::Start).await?;
                self.transition_step(
                    &step,
                    StepEvent::EnqueueForOrchestration(Some(result_data.clone())),
                )
                .await?;

                // Note: attempts were already incremented during the initial failure.
                // We do NOT increment here on successful recovery. If this retry also fails,
                // THAT failure will increment attempts again.
            }
            _ => {
                anyhow::bail!("Cannot complete step from state: {}", current);
            }
        }

        // Final transition to Complete
        self.transition_step(&step, StepEvent::Complete(Some(result_data.clone())))
            .await?;

        // Update step results in database
        sqlx::query!(
            "UPDATE tasker_workflow_steps SET processed = true, results = $2
             WHERE workflow_step_uuid = $1",
            step.workflow_step_uuid,
            result_data
        )
        .execute(&self.pool)
        .await?;

        tracing::info!(
            step_name = %step_name,
            step_uuid = %step.workflow_step_uuid,
            "✅ Step completed successfully"
        );

        Ok(())
    }

    /// Fail a step with an error
    pub async fn fail_step(
        &self,
        task_uuid: Uuid,
        step_name: &str,
        error_message: &str,
    ) -> Result<()> {
        let step = self.get_step_by_name(task_uuid, step_name).await?;

        // DEBUG: Check attempts at the very start of fail_step
        let error_data = serde_json::json!({
            "error": error_message,
            "error_type": "TestError"
        });

        // Check current state
        let current_state = sqlx::query_scalar!(
            "SELECT to_state FROM tasker_workflow_step_transitions
             WHERE workflow_step_uuid = $1 AND most_recent = true",
            step.workflow_step_uuid
        )
        .fetch_optional(&self.pool)
        .await?;

        let current = current_state.unwrap_or_else(|| "pending".to_string());

        tracing::info!(
            step_name = %step_name,
            current_state = %current,
            "🔍 fail_step called on step in state"
        );

        match current.as_str() {
            "error" => {
                // Step already in error state - to simulate a retry that also fails,
                // we need to transition through the retry flow: error → pending → in_progress → error
                // The Start transition will automatically increment attempts (matching worker claim)
                tracing::info!(
                    step_name = %step_name,
                    step_uuid = %step.workflow_step_uuid,
                    error = %error_message,
                    "❌ Step in error state, simulating retry attempt that also fails"
                );

                // Retry flow: error → pending → in_progress (increments attempts) → error
                self.transition_step(&step, StepEvent::Retry).await?;
                self.transition_step(&step, StepEvent::Start).await?; // Increments attempts here!
                self.transition_step(
                    &step,
                    StepEvent::EnqueueAsErrorForOrchestration(Some(error_data)),
                )
                .await?;
                self.transition_step(&step, StepEvent::Fail(error_message.to_string()))
                    .await?;

                tracing::info!(
                    step_name = %step_name,
                    "❌ Retry failed (attempts incremented during Start transition)"
                );
            }
            "pending" | "enqueued" => {
                // Transition to in_progress first, then to error
                // Note: Start transition automatically increments attempts (matching worker claim)
                self.transition_step(&step, StepEvent::Start).await?;
                self.transition_step(
                    &step,
                    StepEvent::EnqueueAsErrorForOrchestration(Some(error_data)),
                )
                .await?;
                self.transition_step(&step, StepEvent::Fail(error_message.to_string()))
                    .await?;

                tracing::info!(
                    step_name = %step_name,
                    step_uuid = %step.workflow_step_uuid,
                    error = %error_message,
                    "❌ Step failed (attempts incremented during Start transition)"
                );
            }
            "in_progress" => {
                // Already in progress, transition to error
                // Note: attempts were already incremented when step transitioned to in_progress
                self.transition_step(
                    &step,
                    StepEvent::EnqueueAsErrorForOrchestration(Some(error_data)),
                )
                .await?;
                self.transition_step(&step, StepEvent::Fail(error_message.to_string()))
                    .await?;

                tracing::info!(
                    step_name = %step_name,
                    step_uuid = %step.workflow_step_uuid,
                    error = %error_message,
                    "❌ Step failed (attempts already incremented when claimed)"
                );
            }
            "complete" => {
                anyhow::bail!("Cannot fail a step that is already complete");
            }
            _ => {
                anyhow::bail!("Cannot fail step from state: {}", current);
            }
        }

        Ok(())
    }

    /// Transition a task to Complete state for testing purposes
    ///
    /// This method creates a terminal task transition to 'complete' state, which is useful
    /// for testing scenarios that require tasks in terminal states, such as:
    /// - Task finalization logic
    /// - Analytics queries on completed tasks
    /// - Dead Letter Queue (DLQ) processing
    /// - Any testing scenarios requiring terminal state tasks
    ///
    /// # Note
    ///
    /// In production, task completion is handled automatically by the orchestration system
    /// after all steps complete. For tests that don't run full orchestration, we need to
    /// manually create the terminal transition.
    ///
    /// This bypasses the state machine guards for testing convenience.
    /// The transition is created properly with sort_key ordering and most_recent flags
    /// to match production behavior.
    pub async fn complete_task(&self, task_uuid: Uuid) -> Result<()> {
        // Get current state
        let current_state: Option<String> = sqlx::query_scalar(
            r#"
            SELECT to_state
            FROM tasker_task_transitions
            WHERE task_uuid = $1 AND most_recent = true
            "#,
        )
        .bind(task_uuid)
        .fetch_optional(&self.pool)
        .await?;

        tracing::info!(
            task_uuid = %task_uuid,
            current_state = ?current_state,
            "🔄 Creating terminal transition to Complete state for testing"
        );

        // Mark existing transitions as not most_recent
        sqlx::query(
            r#"
            UPDATE tasker_task_transitions
            SET most_recent = false
            WHERE task_uuid = $1
            "#,
        )
        .bind(task_uuid)
        .execute(&self.pool)
        .await?;

        // Get max sort_key
        let max_sort_key: Option<i32> = sqlx::query_scalar(
            r#"
            SELECT MAX(sort_key)
            FROM tasker_task_transitions
            WHERE task_uuid = $1
            "#,
        )
        .bind(task_uuid)
        .fetch_optional(&self.pool)
        .await?;

        let next_sort_key = max_sort_key.unwrap_or(0) + 1;

        // Insert transition to 'complete' state
        sqlx::query(
            r#"
            INSERT INTO tasker_task_transitions (
                task_transition_uuid,
                task_uuid,
                to_state,
                from_state,
                sort_key,
                most_recent,
                processor_uuid,
                metadata,
                created_at,
                updated_at
            ) VALUES ($1, $2, 'complete', $3, $4, true, $5, $6, NOW(), NOW())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(task_uuid)
        .bind(current_state)
        .bind(next_sort_key)
        .bind(self.system_context.processor_uuid())
        .bind(serde_json::json!({"source": "test_helper", "event": "complete"}))
        .execute(&self.pool)
        .await?;

        // Update task complete flag
        sqlx::query(
            r#"
            UPDATE tasker_tasks
            SET complete = true, updated_at = NOW()
            WHERE task_uuid = $1
            "#,
        )
        .bind(task_uuid)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            task_uuid = %task_uuid,
            "✅ Task transitioned to Complete state for testing"
        );

        Ok(())
    }

    // ========== Domain Event Observability (TAS-65) ==========

    /// Create a durable event capture for observing domain events
    ///
    /// ## Usage
    ///
    /// ```rust,no_run
    /// let manager = LifecycleTestManager::new(pool).await?;
    /// let mut capture = manager.create_event_capture();
    ///
    /// // ... run workflow that publishes events ...
    ///
    /// // Verify events were published
    /// let events = capture.get_events_in_namespace("payments").await?;
    /// assert!(!events.is_empty());
    ///
    /// // Cleanup only our tracked events
    /// capture.cleanup().await?;
    /// ```
    pub fn create_event_capture(
        &self,
    ) -> crate::common::domain_event_test_helpers::DurableEventCapture {
        crate::common::domain_event_test_helpers::DurableEventCapture::new(self.pool.clone())
    }
}

/// Step readiness information from SQL function
#[derive(Debug, Clone)]
pub struct StepReadiness {
    #[allow(dead_code)]
    pub workflow_step_uuid: Uuid,
    pub name: String,
    pub current_state: Option<String>,
    pub dependencies_satisfied: bool,
    pub ready_for_execution: bool,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub retry_eligible: bool,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_retry_at: Option<chrono::NaiveDateTime>,
}

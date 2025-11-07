//! # Decision Point Actor
//!
//! Actor implementation for processing decision point outcomes and dynamically
//! creating workflow steps based on handler decisions.

use crate::actors::{Handler, Message, OrchestrationActor};
use crate::orchestration::lifecycle::DecisionPointService;
use async_trait::async_trait;
use sqlx::types::Uuid;
use std::sync::Arc;
use tasker_shared::messaging::DecisionPointOutcome;
use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;
use tracing::{debug, info};

/// Message for processing a decision point outcome
///
/// Wraps the decision point execution result for actor-based processing.
/// The actor will validate the outcome and dynamically create workflow steps
/// as specified by the handler's decision.
#[derive(Debug, Clone)]
pub struct ProcessDecisionPointMessage {
    /// UUID of the decision workflow step that was executed
    pub workflow_step_uuid: Uuid,

    /// UUID of the parent task
    pub task_uuid: Uuid,

    /// The decision outcome from the handler
    pub outcome: DecisionPointOutcome,
}

impl Message for ProcessDecisionPointMessage {
    type Response = DecisionPointProcessingResult;
}

/// Result of decision point processing
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionPointProcessingResult {
    /// No steps were created (NoBranches outcome)
    NoStepsCreated,

    /// Steps were successfully created
    StepsCreated {
        /// Names of the steps that were created
        step_names: Vec<String>,
        /// Number of workflow steps created
        count: usize,
    },
}

/// Actor for processing decision point outcomes
///
/// This actor coordinates the dynamic creation of workflow steps based on
/// decision point handler outcomes. It handles:
///
/// 1. Validation of decision outcomes against task templates
/// 2. Dynamic workflow step creation using WorkflowStepCreator
/// 3. Dependency wiring between decision parent and new steps
/// 4. DAG integrity maintenance with cycle detection
/// 5. Idempotency for retry scenarios
/// 6. Event emission for dynamic step creation
///
/// # Example
///
/// ```rust,no_run
/// use tasker_orchestration::actors::{Handler, decision_point_actor::*};
/// use tasker_shared::messaging::DecisionPointOutcome;
/// use uuid::Uuid;
///
/// # async fn example(actor: DecisionPointActor, task_uuid: Uuid, step_uuid: Uuid) -> Result<(), Box<dyn std::error::Error>> {
/// let outcome = DecisionPointOutcome::create_steps(vec!["branch_a".to_string()]);
/// let msg = ProcessDecisionPointMessage {
///     workflow_step_uuid: step_uuid,
///     task_uuid,
///     outcome,
/// };
/// let result = actor.handle(msg).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct DecisionPointActor {
    /// System context for framework operations
    context: Arc<SystemContext>,

    /// Underlying service that performs the actual work
    service: Arc<DecisionPointService>,
}

impl DecisionPointActor {
    /// Create a new DecisionPointActor
    ///
    /// # Arguments
    ///
    /// * `context` - System context for framework operations
    /// * `service` - DecisionPointService to delegate work to
    pub fn new(context: Arc<SystemContext>, service: Arc<DecisionPointService>) -> Self {
        Self { context, service }
    }
}

impl OrchestrationActor for DecisionPointActor {
    fn name(&self) -> &'static str {
        "DecisionPointActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "DecisionPointActor started - ready to process decision outcomes"
        );
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(
            actor = %self.name(),
            "DecisionPointActor stopped"
        );
        Ok(())
    }
}

#[async_trait]
impl Handler<ProcessDecisionPointMessage> for DecisionPointActor {
    type Response = DecisionPointProcessingResult;

    async fn handle(&self, msg: ProcessDecisionPointMessage) -> TaskerResult<Self::Response> {
        debug!(
            actor = %self.name(),
            workflow_step_uuid = %msg.workflow_step_uuid,
            task_uuid = %msg.task_uuid,
            requires_creation = msg.outcome.requires_step_creation(),
            step_count = msg.outcome.step_names().len(),
            "Processing decision point outcome"
        );

        // Check if we need to create steps
        if !msg.outcome.requires_step_creation() {
            debug!(
                actor = %self.name(),
                workflow_step_uuid = %msg.workflow_step_uuid,
                "No steps to create (NoBranches outcome)"
            );
            return Ok(DecisionPointProcessingResult::NoStepsCreated);
        }

        // Delegate to underlying service for step creation
        let step_mapping = self
            .service
            .process_decision_outcome(msg.workflow_step_uuid, msg.task_uuid, msg.outcome.clone())
            .await
            .map_err(|e| tasker_shared::TaskerError::OrchestrationError(e.to_string()))?;

        let step_names: Vec<String> = step_mapping.keys().cloned().collect();
        let count = step_names.len();

        debug!(
            actor = %self.name(),
            workflow_step_uuid = %msg.workflow_step_uuid,
            task_uuid = %msg.task_uuid,
            steps_created = count,
            "Decision point processing complete"
        );

        Ok(DecisionPointProcessingResult::StepsCreated { step_names, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_point_actor_implements_traits() {
        // Verify trait implementations compile
        fn assert_orchestration_actor<T: OrchestrationActor>() {}
        fn assert_handler<T: Handler<ProcessDecisionPointMessage>>() {}

        assert_orchestration_actor::<DecisionPointActor>();
        assert_handler::<DecisionPointActor>();
    }

    #[test]
    fn test_process_decision_point_message_implements_message() {
        fn assert_message<T: Message>() {}
        assert_message::<ProcessDecisionPointMessage>();
    }

    #[test]
    fn test_decision_point_processing_result_equality() {
        let result1 = DecisionPointProcessingResult::NoStepsCreated;
        let result2 = DecisionPointProcessingResult::NoStepsCreated;
        assert_eq!(result1, result2);

        let result3 = DecisionPointProcessingResult::StepsCreated {
            step_names: vec!["branch_a".to_string()],
            count: 1,
        };
        let result4 = DecisionPointProcessingResult::StepsCreated {
            step_names: vec!["branch_a".to_string()],
            count: 1,
        };
        assert_eq!(result3, result4);
    }

    #[tokio::test]
    async fn test_no_branches_outcome_processing() {
        // ConfigLoader automatically loads .env file
        let context = Arc::new(SystemContext::new_for_orchestration().await.unwrap());
        let service = Arc::new(DecisionPointService::new(context.clone()));
        let mut actor = DecisionPointActor::new(context, service);

        // Start the actor
        actor.started().unwrap();

        let msg = ProcessDecisionPointMessage {
            workflow_step_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            outcome: DecisionPointOutcome::no_branches(),
        };

        let result = actor.handle(msg).await.unwrap();
        assert_eq!(result, DecisionPointProcessingResult::NoStepsCreated);

        // Stop the actor
        actor.stopped().unwrap();
    }
}

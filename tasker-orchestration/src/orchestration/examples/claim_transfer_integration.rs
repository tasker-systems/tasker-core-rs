//! # Claim Transfer Integration Examples
//!
//! This module provides examples of how to use the claim transfer mechanism
//! to enable complex multi-phase operations between task_finalizer.rs and
//! task_claim_step_enqueuer.rs without race conditions.
//!
//! ## Primary Use Case (TAS-41)
//!
//! The main scenario this solves:
//! 1. TaskFinalizer claims a task for finalization processing
//! 2. During finalization, it determines more steps need to be enqueued
//! 3. Instead of releasing and re-claiming (which could create race conditions),
//!    it transfers the claim to TaskClaimStepEnqueuer for step processing
//! 4. The same processor maintains control throughout the operation

use std::sync::Arc;
use uuid::Uuid;

use crate::orchestration::{
    lifecycle::{
        task_finalizer::TaskFinalizer,
        task_claim_step_enqueuer::TaskClaimStepEnqueuer,
    },
    task_claim::{
        ClaimPurpose,
        TaskClaimManager, 
        ClaimTransferable,
        FinalizationClaimer,
        TaskClaimer,
    },
};

use tasker_shared::{TaskerResult, SystemContext};
use tracing::{info, warn, error};

/// Example integration showing claim transfer from finalization to step enqueuing
///
/// This is the primary pattern for TAS-41 where a task needs both finalization
/// and step enqueuing operations performed by the same processor.
pub struct ClaimTransferExample {
    finalization_claimer: FinalizationClaimer,
    task_claimer: TaskClaimer,
    task_claim_step_enqueuer: Arc<TaskClaimStepEnqueuer>,
    processor_id: String,
}

impl ClaimTransferExample {
    /// Create a new claim transfer example with shared processor ID
    pub fn new(
        context: Arc<SystemContext>,
        task_claim_step_enqueuer: Arc<TaskClaimStepEnqueuer>,
        processor_id: String,
    ) -> Self {
        let finalization_claimer = FinalizationClaimer::new(context.clone(), processor_id.clone());
        let task_claimer = TaskClaimer::new(context.database_pool().clone(), processor_id.clone());

        Self {
            finalization_claimer,
            task_claimer,
            task_claim_step_enqueuer,
            processor_id,
        }
    }

    /// Example 1: Basic claim transfer pattern
    ///
    /// This shows the fundamental claim transfer from finalization to step enqueuing
    pub async fn basic_claim_transfer_example(&self, task_uuid: Uuid) -> TaskerResult<()> {
        info!(
            task_uuid = task_uuid.to_string(),
            processor_id = %self.processor_id,
            "Starting basic claim transfer example"
        );

        // Step 1: Claim the task for finalization
        let finalization_state = self.finalization_claimer
            .claim_task(task_uuid, ClaimPurpose::Finalization, Some(300))
            .await?;

        info!(
            task_uuid = task_uuid.to_string(),
            state = ?finalization_state,
            "Successfully claimed task for finalization"
        );

        // Step 2: Perform finalization logic (simulated)
        self.simulate_finalization_work(task_uuid).await?;

        // Step 3: Transfer the claim to step enqueuing
        let step_enqueuing_state = self.finalization_claimer
            .transfer_finalization_to_step_enqueuing(task_uuid, Some(300))
            .await?;

        info!(
            task_uuid = task_uuid.to_string(),
            state = ?step_enqueuing_state,
            "Successfully transferred claim from finalization to step enqueuing"
        );

        // Step 4: Use the TaskClaimStepEnqueuer to process steps
        // Note: The task_claim_step_enqueuer would need to accept the existing claim
        match self.task_claim_step_enqueuer.process_single_task(task_uuid).await? {
            Some(enqueue_result) => {
                info!(
                    task_uuid = task_uuid.to_string(),
                    steps_enqueued = enqueue_result.steps_enqueued,
                    "Successfully processed task with transferred claim"
                );
            }
            None => {
                warn!(
                    task_uuid = task_uuid.to_string(),
                    "Task had no ready steps for enqueuing"
                );
            }
        }

        // Step 5: Release the claim
        let released = self.task_claimer
            .release_claim(task_uuid, ClaimPurpose::StepEnqueueing)
            .await?;

        info!(
            task_uuid = task_uuid.to_string(),
            released = released,
            "Claim transfer example completed"
        );

        Ok(())
    }

    /// Example 2: Chain transfer pattern
    ///
    /// This shows transferring through multiple purposes in sequence
    pub async fn chain_transfer_example(&self, task_uuid: Uuid) -> TaskerResult<()> {
        info!(
            task_uuid = task_uuid.to_string(),
            processor_id = %self.processor_id,
            "Starting chain transfer example"
        );

        // Create a chain of purposes to transfer through
        let purposes = vec![
            ClaimPurpose::Finalization,
            ClaimPurpose::StepEnqueueing,
            ClaimPurpose::Processing,
        ];

        let states = self.finalization_claimer
            .claim_and_transfer_chain(task_uuid, purposes, Some(300))
            .await?;

        info!(
            task_uuid = task_uuid.to_string(),
            chain_length = states.len(),
            "Successfully executed claim transfer chain"
        );

        // Process with each state in the chain
        for (i, state) in states.iter().enumerate() {
            info!(
                task_uuid = task_uuid.to_string(),
                step = i,
                state = ?state,
                "Processing with transferred state"
            );

            match i {
                0 => self.simulate_finalization_work(task_uuid).await?,
                1 => self.simulate_step_enqueuing_work(task_uuid).await?,
                2 => self.simulate_processing_work(task_uuid).await?,
                _ => {}
            }
        }

        // Release the final claim
        let released = self.task_claimer
            .release_claim(task_uuid, ClaimPurpose::Processing)
            .await?;

        info!(
            task_uuid = task_uuid.to_string(),
            released = released,
            "Chain transfer example completed"
        );

        Ok(())
    }

    /// Example 3: Error handling with claim transfer
    ///
    /// This shows how to handle errors during claim transfer operations
    pub async fn error_handling_example(&self, task_uuid: Uuid) -> TaskerResult<()> {
        info!(
            task_uuid = task_uuid.to_string(),
            processor_id = %self.processor_id,
            "Starting error handling example"
        );

        // Step 1: Claim the task for finalization
        let _finalization_state = match self.finalization_claimer
            .claim_task(task_uuid, ClaimPurpose::Finalization, Some(300))
            .await
        {
            Ok(state) => state,
            Err(e) => {
                error!(
                    task_uuid = task_uuid.to_string(),
                    error = %e,
                    "Failed to claim task for finalization"
                );
                return Err(e);
            }
        };

        // Step 2: Simulate an error during processing
        let transfer_result = self.finalization_claimer
            .transfer_finalization_to_step_enqueuing(task_uuid, Some(300))
            .await;

        match transfer_result {
            Ok(state) => {
                info!(
                    task_uuid = task_uuid.to_string(),
                    state = ?state,
                    "Transfer successful, proceeding with step enqueuing"
                );

                // Proceed with step enqueuing...
                // If an error occurs here, we should still release the claim
                let process_result = self.simulate_step_enqueuing_work(task_uuid).await;

                // Always attempt to release the claim, regardless of processing success
                let released = self.task_claimer
                    .release_claim(task_uuid, ClaimPurpose::StepEnqueueing)
                    .await?;

                info!(
                    task_uuid = task_uuid.to_string(),
                    released = released,
                    "Released claim after processing"
                );

                // Return the processing result
                process_result
            }
            Err(e) => {
                error!(
                    task_uuid = task_uuid.to_string(),
                    error = %e,
                    "Failed to transfer claim, attempting to release finalization claim"
                );

                // If transfer fails, release the original finalization claim
                let _released = self.finalization_claimer
                    .release_claim(task_uuid, ClaimPurpose::Finalization)
                    .await?;

                Err(e)
            }
        }
    }

    /// Example 4: Conditional claim transfer based on task state
    ///
    /// This shows how to make transfer decisions based on task execution context
    pub async fn conditional_transfer_example(&self, task_uuid: Uuid) -> TaskerResult<()> {
        info!(
            task_uuid = task_uuid.to_string(),
            processor_id = %self.processor_id,
            "Starting conditional transfer example"
        );

        // Step 1: Claim the task for finalization
        let _finalization_state = self.finalization_claimer
            .claim_task(task_uuid, ClaimPurpose::Finalization, Some(300))
            .await?;

        // Step 2: Check task execution context to decide on transfer
        let needs_step_enqueuing = self.check_if_needs_step_enqueuing(task_uuid).await?;

        if needs_step_enqueuing {
            info!(
                task_uuid = task_uuid.to_string(),
                "Task needs step enqueuing, transferring claim"
            );

            // Transfer the claim
            let _step_enqueuing_state = self.finalization_claimer
                .transfer_finalization_to_step_enqueuing(task_uuid, Some(300))
                .await?;

            // Process steps
            self.simulate_step_enqueuing_work(task_uuid).await?;

            // Release step enqueuing claim
            self.task_claimer
                .release_claim(task_uuid, ClaimPurpose::StepEnqueueing)
                .await?;
        } else {
            info!(
                task_uuid = task_uuid.to_string(),
                "Task doesn't need step enqueuing, releasing finalization claim"
            );

            // Just release the finalization claim
            self.finalization_claimer
                .release_claim(task_uuid, ClaimPurpose::Finalization)
                .await?;
        }

        Ok(())
    }

    // Simulation methods for different types of work

    async fn simulate_finalization_work(&self, task_uuid: Uuid) -> TaskerResult<()> {
        info!(
            task_uuid = task_uuid.to_string(),
            "Simulating finalization work"
        );
        // Simulate some finalization processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        Ok(())
    }

    async fn simulate_step_enqueuing_work(&self, task_uuid: Uuid) -> TaskerResult<()> {
        info!(
            task_uuid = task_uuid.to_string(),
            "Simulating step enqueuing work"
        );
        // Simulate some step enqueuing processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        Ok(())
    }

    async fn simulate_processing_work(&self, task_uuid: Uuid) -> TaskerResult<()> {
        info!(
            task_uuid = task_uuid.to_string(),
            "Simulating processing work"
        );
        // Simulate some general processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(75)).await;
        Ok(())
    }

    async fn check_if_needs_step_enqueuing(&self, _task_uuid: Uuid) -> TaskerResult<bool> {
        // This would contain actual logic to check if step enqueuing is needed
        // For example, checking the task execution context
        Ok(true) // Simulate that step enqueuing is needed
    }
}

/// Helper function to demonstrate the integration pattern in TaskFinalizer
///
/// This shows how the TaskFinalizer could be modified to use claim transfers
pub async fn integrate_with_task_finalizer_example(
    finalization_claimer: &FinalizationClaimer,
    task_claim_step_enqueuer: Arc<TaskClaimStepEnqueuer>,
    task_uuid: Uuid,
) -> TaskerResult<()> {
    info!(
        task_uuid = task_uuid.to_string(),
        "Demonstrating TaskFinalizer integration with claim transfer"
    );

    // This is how TaskFinalizer could be modified to use claim transfers:

    // 1. TaskFinalizer claims the task for finalization
    let _finalization_state = finalization_claimer
        .claim_task(task_uuid, ClaimPurpose::Finalization, Some(300))
        .await?;

    // 2. TaskFinalizer performs finalization logic
    // ... finalization work ...

    // 3. If finalization determines that steps need to be enqueued,
    //    transfer the claim instead of releasing and hoping another
    //    processor doesn't claim it first
    let _step_enqueuing_state = finalization_claimer
        .transfer_finalization_to_step_enqueuing(task_uuid, Some(300))
        .await?;

    // 4. Delegate to TaskClaimStepEnqueuer while maintaining claim ownership
    //    The step enqueuer would need to be modified to accept an existing claim
    match task_claim_step_enqueuer.process_single_task(task_uuid).await? {
        Some(result) => {
            info!(
                task_uuid = task_uuid.to_string(),
                steps_enqueued = result.steps_enqueued,
                "Successfully enqueued steps after finalization"
            );
        }
        None => {
            info!(
                task_uuid = task_uuid.to_string(),
                "No steps were ready for enqueuing after finalization"
            );
        }
    }

    // 5. The claim is released by the TaskClaimStepEnqueuer or we release it here
    // (This would be handled in the actual integration)

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Note: These would be integration tests that require a database
    // For now, they serve as documentation of the expected usage patterns

    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_basic_claim_transfer_pattern() {
        // This test would verify the basic claim transfer pattern
        // It would require:
        // 1. Database setup with test data
        // 2. SystemContext with database connection
        // 3. Real task and step data
        
        // let context = setup_test_context().await;
        // let task_claim_step_enqueuer = setup_task_claim_step_enqueuer(&context).await;
        // let processor_id = "test_processor_123".to_string();
        // 
        // let example = ClaimTransferExample::new(
        //     context.clone(), 
        //     task_claim_step_enqueuer, 
        //     processor_id
        // );
        // 
        // let task_uuid = create_test_task(&context).await;
        // example.basic_claim_transfer_example(task_uuid).await.unwrap();
        
        // Verify the claim was transferred and released properly
    }

    #[tokio::test]
    #[ignore] // Requires database setup
    async fn test_error_handling_during_transfer() {
        // This test would verify proper error handling and cleanup
        // during claim transfer operations
    }
}

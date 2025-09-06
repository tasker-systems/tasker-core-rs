//! # Task Claim Manager Trait
//!
//! Provides a unified interface for task claim operations across different claim purposes.
//! This trait enables claim transfers between different operational contexts (finalization,
//! step enqueuing, etc.) when held by the same processor.
//!
//! ## Design Philosophy
//!
//! The trait abstracts over the specific claim implementations while providing a consistent
//! interface for claim lifecycle management. This enables complex multi-phase operations
//! that require claim transfers while maintaining distributed safety guarantees.

use async_trait::async_trait;
use uuid::Uuid;
use tasker_shared::{TaskerResult, TaskerError};
use super::task_claim_state::{TaskClaimState, ClaimPurpose, ClaimError};

/// Unified interface for task claim operations
#[async_trait]
pub trait TaskClaimManager {
    /// Get the processor ID for this claim manager
    fn processor_id(&self) -> &str;

    /// Attempt to claim a task for the specified purpose
    async fn claim_task(
        &self,
        task_uuid: Uuid,
        purpose: ClaimPurpose,
        timeout_seconds: Option<i32>,
    ) -> TaskerResult<TaskClaimState>;

    /// Transfer an existing claim to a new purpose (same processor only)
    async fn transfer_claim(
        &self,
        task_uuid: Uuid,
        from_purpose: ClaimPurpose,
        to_purpose: ClaimPurpose,
        timeout_seconds: Option<i32>,
    ) -> TaskerResult<TaskClaimState>;

    /// Release a claim held by this processor
    async fn release_claim(
        &self,
        task_uuid: Uuid,
        purpose: ClaimPurpose,
    ) -> TaskerResult<bool>;

    /// Extend a claim to prevent timeout
    async fn extend_claim(
        &self,
        task_uuid: Uuid,
        purpose: ClaimPurpose,
        additional_seconds: Option<i32>,
    ) -> TaskerResult<bool>;

    /// Get the current claim state for a task
    async fn get_claim_state(
        &self,
        task_uuid: Uuid,
    ) -> TaskerResult<TaskClaimState>;

    /// Check if this processor can claim or transfer the task
    async fn can_claim_or_transfer(
        &self,
        task_uuid: Uuid,
        purpose: ClaimPurpose,
    ) -> TaskerResult<bool> {
        let current_state = self.get_claim_state(task_uuid).await?;
        Ok(current_state.can_claim(self.processor_id()))
    }
}

/// Extension trait for claim transfer operations
#[async_trait]
pub trait ClaimTransferable: TaskClaimManager {
    /// Transfer a claim from finalization to step enqueuing
    /// This is the primary use case for TAS-41
    async fn transfer_finalization_to_step_enqueuing(
        &self,
        task_uuid: Uuid,
        timeout_seconds: Option<i32>,
    ) -> TaskerResult<TaskClaimState> {
        self.transfer_claim(
            task_uuid,
            ClaimPurpose::Finalization,
            ClaimPurpose::StepEnqueueing,
            timeout_seconds,
        ).await
    }

    /// Transfer a claim from step enqueuing to processing
    async fn transfer_step_enqueuing_to_processing(
        &self,
        task_uuid: Uuid,
        timeout_seconds: Option<i32>,
    ) -> TaskerResult<TaskClaimState> {
        self.transfer_claim(
            task_uuid,
            ClaimPurpose::StepEnqueueing,
            ClaimPurpose::Processing,
            timeout_seconds,
        ).await
    }

    /// Chain multiple claim operations in sequence with automatic transfer
    /// This is useful for complex multi-phase operations
    async fn claim_and_transfer_chain(
        &self,
        task_uuid: Uuid,
        purposes: Vec<ClaimPurpose>,
        timeout_seconds: Option<i32>,
    ) -> TaskerResult<Vec<TaskClaimState>> {
        if purposes.is_empty() {
            return Err(TaskerError::InvalidOperation("Empty purposes chain".to_string()));
        }

        let mut states = Vec::new();
        let mut current_purpose = purposes[0].clone();

        // Initial claim
        let initial_state = self.claim_task(task_uuid, current_purpose.clone(), timeout_seconds).await?;
        states.push(initial_state);

        // Transfer through the chain
        for next_purpose in purposes.into_iter().skip(1) {
            let transferred_state = self.transfer_claim(
                task_uuid,
                current_purpose,
                next_purpose.clone(),
                timeout_seconds,
            ).await?;
            
            states.push(transferred_state);
            current_purpose = next_purpose;
        }

        Ok(states)
    }
}

/// Errors specific to claim management operations
#[derive(Debug, thiserror::Error)]
pub enum ClaimManagerError {
    #[error("Claim transfer not supported between {from:?} and {to:?}")]
    UnsupportedTransfer {
        from: ClaimPurpose,
        to: ClaimPurpose,
    },

    #[error("Processor {processor_id} cannot transfer claim held by {current_holder}")]
    TransferNotAuthorized {
        processor_id: String,
        current_holder: String,
    },

    #[error("Claim has expired and cannot be transferred")]
    ClaimExpired,

    #[error("Database operation failed: {message}")]
    DatabaseError { message: String },
}

impl From<ClaimError> for ClaimManagerError {
    fn from(error: ClaimError) -> Self {
        match error {
            ClaimError::AlreadyClaimed { current_holder, .. } => {
                ClaimManagerError::TransferNotAuthorized {
                    processor_id: "unknown".to_string(),
                    current_holder: current_holder.unwrap_or_default(),
                }
            }
            ClaimError::NotClaimHolder { actual_holder, .. } => {
                ClaimManagerError::TransferNotAuthorized {
                    processor_id: "unknown".to_string(),
                    current_holder: actual_holder,
                }
            }
            _ => ClaimManagerError::DatabaseError {
                message: error.to_string(),
            },
        }
    }
}

impl From<TaskerError> for ClaimManagerError {
    fn from(error: TaskerError) -> Self {
        ClaimManagerError::DatabaseError {
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::task_claim::task_claim_state::{ClaimHolder, TaskClaimState};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock implementation for testing
    struct MockClaimManager {
        processor_id: String,
        claims: Mutex<HashMap<Uuid, TaskClaimState>>,
    }

    impl MockClaimManager {
        fn new(processor_id: String) -> Self {
            Self {
                processor_id,
                claims: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl TaskClaimManager for MockClaimManager {
        fn processor_id(&self) -> &str {
            &self.processor_id
        }

        async fn claim_task(
            &self,
            task_uuid: Uuid,
            purpose: ClaimPurpose,
            timeout_seconds: Option<i32>,
        ) -> TaskerResult<TaskClaimState> {
            let mut claims = self.claims.lock().unwrap();
            let current_state = claims.get(&task_uuid)
                .cloned()
                .unwrap_or_else(|| TaskClaimState::unclaimed(task_uuid));

            let new_state = current_state.try_claim(
                self.processor_id.clone(),
                purpose,
                timeout_seconds.unwrap_or(30),
            ).map_err(|e| TaskerError::InvalidOperation(e.to_string()))?;

            claims.insert(task_uuid, new_state.clone());
            Ok(new_state)
        }

        async fn transfer_claim(
            &self,
            task_uuid: Uuid,
            _from_purpose: ClaimPurpose,
            to_purpose: ClaimPurpose,
            timeout_seconds: Option<i32>,
        ) -> TaskerResult<TaskClaimState> {
            // For mock, just re-claim with new purpose
            self.claim_task(task_uuid, to_purpose, timeout_seconds).await
        }

        async fn release_claim(
            &self,
            task_uuid: Uuid,
            _purpose: ClaimPurpose,
        ) -> TaskerResult<bool> {
            let mut claims = self.claims.lock().unwrap();
            if let Some(current_state) = claims.get(&task_uuid).cloned() {
                let released_state = current_state.try_release(&self.processor_id)
                    .map_err(|e| TaskerError::InvalidOperation(e.to_string()))?;
                claims.insert(task_uuid, released_state);
                Ok(true)
            } else {
                Ok(false)
            }
        }

        async fn extend_claim(
            &self,
            _task_uuid: Uuid,
            _purpose: ClaimPurpose,
            _additional_seconds: Option<i32>,
        ) -> TaskerResult<bool> {
            // Mock implementation - always successful
            Ok(true)
        }

        async fn get_claim_state(
            &self,
            task_uuid: Uuid,
        ) -> TaskerResult<TaskClaimState> {
            let claims = self.claims.lock().unwrap();
            Ok(claims.get(&task_uuid)
                .cloned()
                .unwrap_or_else(|| TaskClaimState::unclaimed(task_uuid)))
        }
    }

    impl ClaimTransferable for MockClaimManager {}

    #[tokio::test]
    async fn test_claim_and_transfer_chain() {
        let manager = MockClaimManager::new("test_processor".to_string());
        let task_uuid = Uuid::new_v4();

        let purposes = vec![
            ClaimPurpose::Finalization,
            ClaimPurpose::StepEnqueueing,
            ClaimPurpose::Processing,
        ];

        let states = manager.claim_and_transfer_chain(task_uuid, purposes, Some(30)).await.unwrap();

        assert_eq!(states.len(), 3);
        assert!(states.iter().all(|state| state.is_claimed()));
    }

    #[tokio::test]
    async fn test_claim_transfer_shortcuts() {
        let manager = MockClaimManager::new("test_processor".to_string());
        let task_uuid = Uuid::new_v4();

        // First claim for finalization
        let _initial_state = manager.claim_task(task_uuid, ClaimPurpose::Finalization, Some(30)).await.unwrap();

        // Transfer to step enqueuing
        let transferred_state = manager.transfer_finalization_to_step_enqueuing(task_uuid, Some(30)).await.unwrap();

        assert!(transferred_state.is_claimed());
    }
}

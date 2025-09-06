//! Task Claim State Machine
//!
//! This module provides a Rust-idiomatic representation of the task claim state machine
//! that underlies our distributed task orchestration system. While the authoritative
//! state lives in the database (for ACID compliance), this enum provides a clear
//! conceptual model for reasoning about claim states and transitions.
//!
//! ## Design Philosophy
//!
//! The task claim mechanism prevents race conditions in our distributed system by
//! ensuring only one processor can work on a task at a time. The claim serves
//! multiple purposes (finalization, step enqueueing) and can be "transferred"
//! between these purposes when held by the same processor.
//!
//! ## Claim Transfer Architecture (TAS-41)
//!
//! A key innovation is that claims can be seamlessly transferred between different
//! operational contexts when held by the same processor. For example:
//! 1. Processor claims task for finalization
//! 2. Same processor can "re-claim" for step enqueueing without releasing
//! 3. This avoids race conditions while allowing complex multi-phase operations

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// The purpose or context of a task claim
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimPurpose {
    /// Claim for finalizing task state and determining completion
    Finalization,
    /// Claim for enqueueing ready steps to queues
    StepEnqueueing,
    /// Claim for general task processing
    Processing,
    /// Claim transferred from one purpose to another (same processor)
    Transferred {
        from: Box<ClaimPurpose>,
        to: Box<ClaimPurpose>,
    },
}

impl fmt::Display for ClaimPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClaimPurpose::Finalization => write!(f, "Finalization"),
            ClaimPurpose::StepEnqueueing => write!(f, "StepEnqueueing"),
            ClaimPurpose::Processing => write!(f, "Processing"),
            ClaimPurpose::Transferred { from, to } => {
                write!(f, "Transferred({} -> {})", from, to)
            }
        }
    }
}

/// Information about who holds a claim
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimHolder {
    /// The processor/orchestrator ID holding the claim
    pub processor_id: String,
    /// When the claim was acquired
    pub claimed_at: DateTime<Utc>,
    /// How long the claim is valid for
    pub timeout_seconds: i32,
    /// The purpose of this claim
    pub purpose: ClaimPurpose,
}

impl ClaimHolder {
    /// Check if the claim has expired
    pub fn is_expired(&self) -> bool {
        let expiry = self.claimed_at + Duration::seconds(self.timeout_seconds as i64);
        Utc::now() > expiry
    }

    /// Check if this holder can transfer the claim to a new purpose
    pub fn can_transfer_to(&self, new_purpose: &ClaimPurpose) -> bool {
        !self.is_expired() && self.purpose != *new_purpose
    }

    /// Get the remaining time on this claim
    pub fn remaining_seconds(&self) -> Option<i64> {
        if self.is_expired() {
            None
        } else {
            let expiry = self.claimed_at + Duration::seconds(self.timeout_seconds as i64);
            Some((expiry - Utc::now()).num_seconds())
        }
    }
}

/// Represents the state of a task claim in our distributed system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskClaimState {
    /// Task is not claimed - available for any processor to claim
    Unclaimed {
        task_uuid: Uuid,
        last_released_at: Option<DateTime<Utc>>,
        last_released_by: Option<String>,
    },

    /// Task is actively claimed by a processor
    Claimed {
        task_uuid: Uuid,
        holder: ClaimHolder,
    },

    /// Task claim has expired but not yet reclaimed
    /// This is a transient state - the database will treat this as Unclaimed
    /// but we track it separately for observability
    Expired {
        task_uuid: Uuid,
        previous_holder: ClaimHolder,
        expired_at: DateTime<Utc>,
    },

    /// Task claim was extended by the same processor
    Extended {
        task_uuid: Uuid,
        holder: ClaimHolder,
        previous_timeout: i32,
        extension_count: u32,
    },

    /// Task claim was transferred to a different purpose by the same processor
    /// This is the key state for our claim transfer architecture
    Transferred {
        task_uuid: Uuid,
        holder: ClaimHolder,
        transfer_count: u32,
    },

    /// Task is complete and cannot be claimed
    Complete {
        task_uuid: Uuid,
        completed_at: DateTime<Utc>,
        last_holder: Option<ClaimHolder>,
    },
}

impl TaskClaimState {
    /// Create a new unclaimed state
    pub fn unclaimed(task_uuid: Uuid) -> Self {
        TaskClaimState::Unclaimed {
            task_uuid,
            last_released_at: None,
            last_released_by: None,
        }
    }

    /// Get the task UUID for this state
    pub fn task_uuid(&self) -> Uuid {
        match self {
            TaskClaimState::Unclaimed { task_uuid, .. } => *task_uuid,
            TaskClaimState::Claimed { task_uuid, .. } => *task_uuid,
            TaskClaimState::Expired { task_uuid, .. } => *task_uuid,
            TaskClaimState::Extended { task_uuid, .. } => *task_uuid,
            TaskClaimState::Transferred { task_uuid, .. } => *task_uuid,
            TaskClaimState::Complete { task_uuid, .. } => *task_uuid,
        }
    }

    /// Check if this state allows claiming
    pub fn can_claim(&self, processor_id: &str) -> bool {
        match self {
            TaskClaimState::Unclaimed { .. } => true,
            TaskClaimState::Expired { .. } => true,
            TaskClaimState::Claimed { holder, .. } => {
                // Same processor can re-claim (for transfer or extension)
                holder.processor_id == processor_id || holder.is_expired()
            }
            TaskClaimState::Extended { holder, .. } => {
                // Same processor can re-claim
                holder.processor_id == processor_id || holder.is_expired()
            }
            TaskClaimState::Transferred { holder, .. } => {
                // Same processor can continue transferring
                holder.processor_id == processor_id || holder.is_expired()
            }
            TaskClaimState::Complete { .. } => false,
        }
    }

    /// Attempt to claim the task
    pub fn try_claim(
        self,
        processor_id: String,
        purpose: ClaimPurpose,
        timeout_seconds: i32,
    ) -> Result<TaskClaimState, ClaimError> {
        if !self.can_claim(&processor_id) {
            return Err(ClaimError::AlreadyClaimed {
                task_uuid: self.task_uuid(),
                current_holder: self.current_holder().map(|h| h.processor_id.clone()),
            });
        }

        let task_uuid = self.task_uuid();
        let now = Utc::now();

        match self {
            TaskClaimState::Unclaimed { .. } | TaskClaimState::Expired { .. } => {
                // Fresh claim
                Ok(TaskClaimState::Claimed {
                    task_uuid,
                    holder: ClaimHolder {
                        processor_id,
                        claimed_at: now,
                        timeout_seconds,
                        purpose,
                    },
                })
            }
            TaskClaimState::Claimed { ref holder, .. }
            | TaskClaimState::Extended { ref holder, .. }
            | TaskClaimState::Transferred { ref holder, .. } => {
                if holder.processor_id == processor_id {
                    if holder.purpose == purpose {
                        // Extension
                        Ok(TaskClaimState::Extended {
                            task_uuid,
                            previous_timeout: holder.timeout_seconds,
                            holder: ClaimHolder {
                                processor_id,
                                claimed_at: now,
                                timeout_seconds,
                                purpose,
                            },
                            extension_count: self.extension_count() + 1,
                        })
                    } else {
                        // Transfer to new purpose
                        Ok(TaskClaimState::Transferred {
                            task_uuid,
                            holder: ClaimHolder {
                                processor_id,
                                claimed_at: now,
                                timeout_seconds,
                                purpose: ClaimPurpose::Transferred {
                                    from: Box::new(holder.purpose),
                                    to: Box::new(purpose),
                                },
                            },
                            transfer_count: self.transfer_count() + 1,
                        })
                    }
                } else if holder.is_expired() {
                    // Claim after expiry
                    Ok(TaskClaimState::Claimed {
                        task_uuid,
                        holder: ClaimHolder {
                            processor_id,
                            claimed_at: now,
                            timeout_seconds,
                            purpose,
                        },
                    })
                } else {
                    Err(ClaimError::AlreadyClaimed {
                        task_uuid,
                        current_holder: Some(holder.processor_id),
                    })
                }
            }
            TaskClaimState::Complete { .. } => Err(ClaimError::TaskComplete { task_uuid }),
        }
    }

    /// Release the claim
    pub fn try_release(self, processor_id: &str) -> Result<TaskClaimState, ClaimError> {
        let task_uuid = self.task_uuid();

        match self {
            TaskClaimState::Claimed { holder, .. }
            | TaskClaimState::Extended { holder, .. }
            | TaskClaimState::Transferred { holder, .. } => {
                if holder.processor_id == processor_id {
                    Ok(TaskClaimState::Unclaimed {
                        task_uuid,
                        last_released_at: Some(Utc::now()),
                        last_released_by: Some(processor_id.to_string()),
                    })
                } else {
                    Err(ClaimError::NotClaimHolder {
                        task_uuid,
                        processor_id: processor_id.to_string(),
                        actual_holder: holder.processor_id,
                    })
                }
            }
            _ => Err(ClaimError::NotClaimed { task_uuid }),
        }
    }

    /// Mark the task as complete
    pub fn complete(self) -> TaskClaimState {
        TaskClaimState::Complete {
            task_uuid: self.task_uuid(),
            completed_at: Utc::now(),
            last_holder: self.current_holder(),
        }
    }

    /// Get the current claim holder if any
    pub fn current_holder(&self) -> Option<ClaimHolder> {
        match self {
            TaskClaimState::Claimed { holder, .. } => Some(holder.clone()),
            TaskClaimState::Extended { holder, .. } => Some(holder.clone()),
            TaskClaimState::Transferred { holder, .. } => Some(holder.clone()),
            TaskClaimState::Complete { last_holder, .. } => last_holder.clone(),
            _ => None,
        }
    }

    /// Get the extension count for this state
    fn extension_count(&self) -> u32 {
        match self {
            TaskClaimState::Extended {
                extension_count, ..
            } => *extension_count,
            _ => 0,
        }
    }

    /// Get the transfer count for this state
    fn transfer_count(&self) -> u32 {
        match self {
            TaskClaimState::Transferred { transfer_count, .. } => *transfer_count,
            _ => 0,
        }
    }

    /// Check if the task is claimed
    pub fn is_claimed(&self) -> bool {
        matches!(
            self,
            TaskClaimState::Claimed { .. }
                | TaskClaimState::Extended { .. }
                | TaskClaimState::Transferred { .. }
        )
    }

    /// Check if the task is available for claiming
    pub fn is_available(&self) -> bool {
        matches!(
            self,
            TaskClaimState::Unclaimed { .. } | TaskClaimState::Expired { .. }
        )
    }
}

/// Errors that can occur during claim operations
#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    #[error("Task {task_uuid} is already claimed by {current_holder:?}")]
    AlreadyClaimed {
        task_uuid: Uuid,
        current_holder: Option<String>,
    },

    #[error("Task {task_uuid} is not claimed")]
    NotClaimed { task_uuid: Uuid },

    #[error("Task {task_uuid} is complete and cannot be claimed")]
    TaskComplete { task_uuid: Uuid },

    #[error("Processor {processor_id} is not the claim holder for task {task_uuid} (held by {actual_holder})")]
    NotClaimHolder {
        task_uuid: Uuid,
        processor_id: String,
        actual_holder: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_transitions() {
        let task_uuid = Uuid::new_v4();
        let processor1 = "processor1".to_string();
        let processor2 = "processor2".to_string();

        // Start unclaimed
        let state = TaskClaimState::unclaimed(task_uuid);
        assert!(state.is_available());
        assert!(state.can_claim(&processor1));

        // Claim for finalization
        let state = state
            .try_claim(processor1.clone(), ClaimPurpose::Finalization, 30)
            .unwrap();
        assert!(state.is_claimed());
        assert!(state.can_claim(&processor1)); // Same processor can re-claim
        assert!(!state.can_claim(&processor2)); // Different processor cannot

        // Transfer to step enqueueing (same processor)
        let state = state
            .try_claim(processor1.clone(), ClaimPurpose::StepEnqueueing, 30)
            .unwrap();
        assert!(matches!(state, TaskClaimState::Transferred { .. }));

        // Release the claim
        let state = state.try_release(&processor1).unwrap();
        assert!(state.is_available());

        // Now processor2 can claim
        let state = state
            .try_claim(processor2.clone(), ClaimPurpose::Processing, 60)
            .unwrap();
        assert!(state.is_claimed());
    }

    #[test]
    fn test_claim_extension() {
        let task_uuid = Uuid::new_v4();
        let processor = "processor".to_string();

        let state = TaskClaimState::unclaimed(task_uuid);

        // Initial claim
        let state = state
            .try_claim(processor.clone(), ClaimPurpose::Processing, 30)
            .unwrap();

        // Extend the claim (same processor, same purpose)
        let state = state
            .try_claim(processor.clone(), ClaimPurpose::Processing, 60)
            .unwrap();

        assert!(matches!(state, TaskClaimState::Extended { .. }));
        if let TaskClaimState::Extended {
            extension_count,
            previous_timeout,
            ..
        } = state
        {
            assert_eq!(extension_count, 1);
            assert_eq!(previous_timeout, 30);
        }
    }

    #[test]
    fn test_complete_state() {
        let task_uuid = Uuid::new_v4();
        let processor = "processor".to_string();

        let state = TaskClaimState::unclaimed(task_uuid);
        let state = state
            .try_claim(processor.clone(), ClaimPurpose::Finalization, 30)
            .unwrap();

        // Complete the task
        let state = state.complete();
        assert!(matches!(state, TaskClaimState::Complete { .. }));

        // Cannot claim a complete task
        assert!(!state.can_claim(&processor));
        let result = state.try_claim(processor, ClaimPurpose::Processing, 30);
        assert!(matches!(result, Err(ClaimError::TaskComplete { .. })));
    }
}

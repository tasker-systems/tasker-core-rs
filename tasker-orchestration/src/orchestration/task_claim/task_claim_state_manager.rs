//! # Task Claim State Manager
//!
//! Bridges between the database representation of task claims and the TaskClaimState enum.
//! This provides a clean abstraction layer that converts database rows to strongly-typed
//! enum states while maintaining the database as the single source of truth.
//!
//! ## Architecture
//!
//! - Database Layer: ACID-compliant storage with SQL functions for atomic operations
//! - TaskClaimState Enum: Type-safe representation of claim states and transitions
//! - This Manager: Conversion layer that synchronizes between the two representations
//!
//! The database remains authoritative, but the enum provides better reasoning about
//! state transitions and business logic.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::task_claim_state::{TaskClaimState, ClaimHolder, ClaimPurpose};
use tasker_shared::{TaskerResult, TaskerError};
use tracing::{debug, warn, error};

/// Database row structure for task claim information
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskClaimDbRow {
    pub task_uuid: Uuid,
    pub complete: bool,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_timeout_seconds: Option<i32>,
    pub is_expired: bool,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Result structure for claim transfer operations
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimTransferResult {
    pub transferred: bool,
    pub task_uuid: Uuid,
    pub processor_id: String,
    pub new_purpose: String,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_timeout_seconds: i32,
    pub message: String,
}

/// Result structure for purpose-aware claiming
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimWithPurposeResult {
    pub claimed: bool,
    pub task_uuid: Uuid,
    pub processor_id: String,
    pub purpose: String,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_timeout_seconds: i32,
    pub message: String,
}

/// Manages conversion between database state and TaskClaimState enum
pub struct TaskClaimStateManager {
    pool: PgPool,
}

impl TaskClaimStateManager {
    /// Create a new TaskClaimStateManager
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get the current claim state for a task from the database
    pub async fn get_claim_state(
        &self,
        task_uuid: Uuid,
    ) -> TaskerResult<TaskClaimState> {
        debug!(
            task_uuid = task_uuid.to_string(),
            "Fetching claim state from database"
        );

        let query = "SELECT * FROM get_task_claim_info($1::UUID)";
        
        let row: Option<TaskClaimDbRow> = sqlx::query_as(query)
            .bind(task_uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to fetch claim info for task {}: {}", task_uuid, e);
                TaskerError::DatabaseError(format!("Failed to fetch claim info: {e}"))
            })?;

        let row = match row {
            Some(row) => row,
            None => {
                debug!(task_uuid = task_uuid.to_string(), "Task not found");
                return Ok(TaskClaimState::unclaimed(task_uuid));
            }
        };

        let state = self.db_row_to_claim_state(row);
        
        debug!(
            task_uuid = task_uuid.to_string(),
            state = ?state,
            "Converted database row to claim state"
        );

        Ok(state)
    }

    /// Transfer a claim between purposes using the database function
    pub async fn transfer_claim(
        &self,
        task_uuid: Uuid,
        processor_id: &str,
        from_purpose: &ClaimPurpose,
        to_purpose: &ClaimPurpose,
        new_timeout_seconds: Option<i32>,
    ) -> TaskerResult<TaskClaimState> {
        debug!(
            task_uuid = task_uuid.to_string(),
            processor_id = processor_id,
            from = ?from_purpose,
            to = ?to_purpose,
            "Transferring claim in database"
        );

        let query = r#"
            SELECT * FROM transfer_task_claim($1::UUID, $2::VARCHAR, $3::VARCHAR, $4::VARCHAR, $5::INTEGER)
        "#;

        let result: ClaimTransferResult = sqlx::query_as(query)
            .bind(task_uuid)
            .bind(processor_id)
            .bind(from_purpose.to_string())
            .bind(to_purpose.to_string())
            .bind(new_timeout_seconds)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!(
                    "Failed to transfer claim for task {} from {:?} to {:?}: {}",
                    task_uuid, from_purpose, to_purpose, e
                );
                TaskerError::DatabaseError(format!("Claim transfer failed: {e}"))
            })?;

        if result.transferred {
            debug!(
                task_uuid = task_uuid.to_string(),
                message = %result.message,
                "Claim transfer successful"
            );
            
            // Return the new state after transfer
            self.get_claim_state(task_uuid).await
        } else {
            warn!(
                task_uuid = task_uuid.to_string(),
                message = %result.message,
                "Claim transfer failed"
            );
            
            Err(TaskerError::InvalidOperation(format!(
                "Claim transfer failed: {}", result.message
            )))
        }
    }

    /// Claim a task with a specific purpose
    pub async fn claim_with_purpose(
        &self,
        task_uuid: Uuid,
        processor_id: &str,
        purpose: &ClaimPurpose,
        timeout_seconds: Option<i32>,
    ) -> TaskerResult<TaskClaimState> {
        debug!(
            task_uuid = task_uuid.to_string(),
            processor_id = processor_id,
            purpose = ?purpose,
            timeout = timeout_seconds,
            "Claiming task with purpose in database"
        );

        let query = r#"
            SELECT * FROM claim_task_with_purpose($1::UUID, $2::VARCHAR, $3::VARCHAR, $4::INTEGER)
        "#;

        let result: ClaimWithPurposeResult = sqlx::query_as(query)
            .bind(task_uuid)
            .bind(processor_id)
            .bind(purpose.to_string())
            .bind(timeout_seconds.unwrap_or(300))
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!(
                    "Failed to claim task {} with purpose {:?}: {}",
                    task_uuid, purpose, e
                );
                TaskerError::DatabaseError(format!("Claim with purpose failed: {e}"))
            })?;

        if result.claimed {
            debug!(
                task_uuid = task_uuid.to_string(),
                message = %result.message,
                "Task claimed successfully"
            );
            
            // Return the new state after claiming
            self.get_claim_state(task_uuid).await
        } else {
            debug!(
                task_uuid = task_uuid.to_string(),
                message = %result.message,
                "Task claim failed"
            );
            
            Err(TaskerError::InvalidOperation(format!(
                "Task claim failed: {}", result.message
            )))
        }
    }

    /// Convert a database row to a TaskClaimState enum
    fn db_row_to_claim_state(&self, row: TaskClaimDbRow) -> TaskClaimState {
        // Task is complete
        if row.complete {
            return TaskClaimState::Complete {
                task_uuid: row.task_uuid,
                completed_at: row.updated_at, // Use updated_at as completion time
                last_holder: self.create_claim_holder_from_row(&row),
            };
        }

        // Task is not claimed
        if row.claimed_by.is_none() || row.claimed_at.is_none() {
            return TaskClaimState::Unclaimed {
                task_uuid: row.task_uuid,
                last_released_at: Some(row.updated_at),
                last_released_by: row.claimed_by,
            };
        }

        // Task has an active or expired claim
        let claim_holder = self.create_claim_holder_from_row(&row)
            .expect("Should have claim holder data if claimed_by is set");

        if row.is_expired {
            TaskClaimState::Expired {
                task_uuid: row.task_uuid,
                previous_holder: claim_holder,
                expired_at: row.claimed_at.unwrap() + 
                    chrono::Duration::seconds(row.claim_timeout_seconds.unwrap_or(300) as i64),
            }
        } else {
            // Active claim - we default to Claimed state since we don't track
            // transfer history in the database (that's maintained at the application level)
            TaskClaimState::Claimed {
                task_uuid: row.task_uuid,
                holder: claim_holder,
            }
        }
    }

    /// Create a ClaimHolder from database row data
    fn create_claim_holder_from_row(&self, row: &TaskClaimDbRow) -> Option<ClaimHolder> {
        if let (Some(processor_id), Some(claimed_at), Some(timeout_seconds)) = (
            &row.claimed_by,
            row.claimed_at,
            row.claim_timeout_seconds,
        ) {
            Some(ClaimHolder {
                processor_id: processor_id.clone(),
                claimed_at,
                timeout_seconds,
                // We don't store purpose in DB, so we use a generic purpose
                // The actual purpose tracking happens at the application level
                purpose: ClaimPurpose::Processing,
            })
        } else {
            None
        }
    }

    /// Validate that a claim state is consistent with database state
    pub async fn validate_claim_state(
        &self,
        expected_state: &TaskClaimState,
    ) -> TaskerResult<bool> {
        let current_state = self.get_claim_state(expected_state.task_uuid()).await?;
        
        // Compare the essential aspects of the states
        let is_consistent = match (expected_state, &current_state) {
            (TaskClaimState::Unclaimed { .. }, TaskClaimState::Unclaimed { .. }) => true,
            (TaskClaimState::Complete { .. }, TaskClaimState::Complete { .. }) => true,
            (TaskClaimState::Claimed { holder: expected, .. }, TaskClaimState::Claimed { holder: actual, .. }) => {
                expected.processor_id == actual.processor_id
            },
            (TaskClaimState::Expired { .. }, TaskClaimState::Expired { .. }) => true,
            _ => false,
        };

        if !is_consistent {
            warn!(
                task_uuid = expected_state.task_uuid().to_string(),
                expected = ?expected_state,
                actual = ?current_state,
                "Claim state inconsistency detected"
            );
        }

        Ok(is_consistent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_db_row_to_claim_state_unclaimed() {
        let manager = TaskClaimStateManager::new(PgPool::connect("").await.unwrap());
        let task_uuid = Uuid::new_v4();
        
        let row = TaskClaimDbRow {
            task_uuid,
            complete: false,
            claimed_by: None,
            claimed_at: None,
            claim_timeout_seconds: None,
            is_expired: false,
            updated_at: Utc::now(),
            created_at: Utc::now(),
        };

        let state = manager.db_row_to_claim_state(row);
        
        match state {
            TaskClaimState::Unclaimed { task_uuid: uuid, .. } => {
                assert_eq!(uuid, task_uuid);
            }
            _ => panic!("Expected Unclaimed state"),
        }
    }

    #[test]
    fn test_db_row_to_claim_state_claimed() {
        let manager = TaskClaimStateManager::new(PgPool::connect("").await.unwrap());
        let task_uuid = Uuid::new_v4();
        let now = Utc::now();
        
        let row = TaskClaimDbRow {
            task_uuid,
            complete: false,
            claimed_by: Some("test_processor".to_string()),
            claimed_at: Some(now),
            claim_timeout_seconds: Some(300),
            is_expired: false,
            updated_at: now,
            created_at: now,
        };

        let state = manager.db_row_to_claim_state(row);
        
        match state {
            TaskClaimState::Claimed { task_uuid: uuid, holder } => {
                assert_eq!(uuid, task_uuid);
                assert_eq!(holder.processor_id, "test_processor");
                assert_eq!(holder.timeout_seconds, 300);
            }
            _ => panic!("Expected Claimed state, got {:?}", state),
        }
    }

    #[test]
    fn test_db_row_to_claim_state_complete() {
        let manager = TaskClaimStateManager::new(PgPool::connect("").await.unwrap());
        let task_uuid = Uuid::new_v4();
        let now = Utc::now();
        
        let row = TaskClaimDbRow {
            task_uuid,
            complete: true,
            claimed_by: Some("test_processor".to_string()),
            claimed_at: Some(now),
            claim_timeout_seconds: Some(300),
            is_expired: false,
            updated_at: now,
            created_at: now,
        };

        let state = manager.db_row_to_claim_state(row);
        
        match state {
            TaskClaimState::Complete { task_uuid: uuid, .. } => {
                assert_eq!(uuid, task_uuid);
            }
            _ => panic!("Expected Complete state, got {:?}", state),
        }
    }
}

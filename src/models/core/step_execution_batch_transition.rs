//! # Step Execution Batch Transition Model
//!
//! State machine transitions for batch execution lifecycle tracking.
//!
//! ## Overview
//!
//! The `StepExecutionBatchTransition` model tracks all state changes for
//! execution batches, providing a complete audit trail of the batch lifecycle
//! from creation through completion or failure.
//!
//! ## Key Features
//!
//! - **State Machine Tracking**: Records all state transitions
//! - **Most Recent Flag**: Efficient current state lookups
//! - **Metadata Support**: Transition-specific context storage
//! - **Trigger Recording**: Captures what initiated the transition
//! - **Immutable History**: Complete audit trail preservation
//!
//! ## Database Schema
//!
//! Maps to `tasker_step_execution_batch_transitions` table:
//! ```sql
//! CREATE TABLE tasker_step_execution_batch_transitions (
//!   id BIGSERIAL PRIMARY KEY,
//!   batch_id BIGINT NOT NULL,
//!   from_state VARCHAR,
//!   to_state VARCHAR NOT NULL,
//!   triggered_by VARCHAR,
//!   metadata JSONB,
//!   most_recent BOOLEAN DEFAULT true,
//!   -- ... timestamps
//! );
//! ```
//!
//! ## State Machine
//!
//! Valid states and transitions:
//! - `created` → `published`: Batch sent to ZeroMQ
//! - `published` → `executing`: First worker picks up batch
//! - `executing` → `completed`: All steps finished successfully
//! - `executing` → `failed`: One or more steps failed
//! - `executing` → `timeout`: Batch exceeded timeout threshold
//! - Any state → `cancelled`: Manual cancellation
//!
//! ## Most Recent Pattern
//!
//! Only one transition per batch has `most_recent = true`, enabling
//! efficient current state queries without aggregation.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Represents a state transition in the batch execution lifecycle.
///
/// Each record captures a single state change with full context,
/// building an immutable audit trail of the batch's progression.
///
/// # State Machine Integrity
///
/// The system enforces valid state transitions:
/// - Initial state must be `created`
/// - Terminal states: `completed`, `failed`, `timeout`, `cancelled`
/// - No transitions allowed from terminal states
///
/// # Most Recent Flag
///
/// The `most_recent` flag enables O(1) current state lookups:
/// ```sql
/// SELECT * FROM transitions
/// WHERE batch_id = ? AND most_recent = true
/// ```
///
/// # Metadata Examples
///
/// Transition metadata can include:
/// ```json
/// {
///   "reason": "All steps completed successfully",
///   "completed_steps": 10,
///   "failed_steps": 0,
///   "execution_time_ms": 5432
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepExecutionBatchTransition {
    pub id: i64,
    pub batch_id: i64,
    pub from_state: Option<String>,
    pub to_state: String,
    pub event_name: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub sort_key: i32,
    pub most_recent: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New transition for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepExecutionBatchTransition {
    pub batch_id: i64,
    pub from_state: Option<String>,
    pub to_state: String,
    pub event_name: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub sort_key: i32,
}

/// Valid batch execution states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BatchState {
    Created,
    Published,
    Executing,
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

impl std::str::FromStr for BatchState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "created" => Ok(Self::Created),
            "published" => Ok(Self::Published),
            "executing" => Ok(Self::Executing),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "timeout" => Ok(Self::Timeout),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid batch state: {s}")),
        }
    }
}

impl BatchState {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Published => "published",
            Self::Executing => "executing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
        }
    }

    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Timeout | Self::Cancelled
        )
    }

    /// Check if transition is valid
    pub fn can_transition_to(&self, to: &BatchState) -> bool {
        if self.is_terminal() {
            return false; // No transitions from terminal states
        }

        match (self, to) {
            (BatchState::Created, BatchState::Published) => true,
            (BatchState::Published, BatchState::Executing) => true,
            (BatchState::Executing, BatchState::Completed) => true,
            (BatchState::Executing, BatchState::Failed) => true,
            (BatchState::Executing, BatchState::Timeout) => true,
            (_, BatchState::Cancelled) => !self.is_terminal(), // Can cancel from any non-terminal
            _ => false,
        }
    }
}

impl StepExecutionBatchTransition {
    /// Find by primary key
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, from_state, to_state, event_name,
                   metadata, sort_key, most_recent, created_at, updated_at
            FROM tasker_step_execution_batch_transitions
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    /// Find the most recent transition for a batch
    pub async fn find_most_recent(
        pool: &PgPool,
        batch_id: i64,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, from_state, to_state, event_name,
                   metadata, sort_key, most_recent, created_at, updated_at
            FROM tasker_step_execution_batch_transitions
            WHERE batch_id = $1 AND most_recent = true
            "#,
            batch_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Create a new transition (handles most_recent flag)
    pub async fn create(
        pool: &PgPool,
        new_transition: NewStepExecutionBatchTransition,
    ) -> Result<Self, sqlx::Error> {
        let mut tx = pool.begin().await?;

        // First, unset most_recent on all existing transitions for this batch
        sqlx::query!(
            r#"
            UPDATE tasker_step_execution_batch_transitions
            SET most_recent = false, updated_at = NOW()
            WHERE batch_id = $1 AND most_recent = true
            "#,
            new_transition.batch_id
        )
        .execute(&mut *tx)
        .await?;

        // Create the new transition with most_recent = true
        let created = sqlx::query_as!(
            Self,
            r#"
            INSERT INTO tasker_step_execution_batch_transitions
            (batch_id, from_state, to_state, event_name, metadata, sort_key, most_recent)
            VALUES ($1, $2, $3, $4, $5, $6, true)
            RETURNING id, batch_id, from_state, to_state, event_name,
                      metadata, sort_key, most_recent, created_at, updated_at
            "#,
            new_transition.batch_id,
            new_transition.from_state,
            new_transition.to_state,
            new_transition.event_name,
            new_transition.metadata,
            new_transition.sort_key
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(created)
    }

    /// Find all transitions for a batch (complete history)
    pub async fn find_by_batch(pool: &PgPool, batch_id: i64) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, from_state, to_state, event_name,
                   metadata, sort_key, most_recent, created_at, updated_at
            FROM tasker_step_execution_batch_transitions
            WHERE batch_id = $1
            ORDER BY sort_key ASC
            "#,
            batch_id
        )
        .fetch_all(pool)
        .await
    }

    /// Find batches in a specific state
    pub async fn find_batches_in_state(
        pool: &PgPool,
        state: &str,
        limit: i64,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let records = sqlx::query!(
            r#"
            SELECT DISTINCT batch_id
            FROM tasker_step_execution_batch_transitions
            WHERE to_state = $1 AND most_recent = true
            ORDER BY batch_id DESC
            LIMIT $2
            "#,
            state,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(records.into_iter().map(|r| r.batch_id).collect())
    }

    /// Count batches by current state
    pub async fn count_by_state(pool: &PgPool) -> Result<Vec<StateCount>, sqlx::Error> {
        sqlx::query_as!(
            StateCount,
            r#"
            SELECT to_state as state, COUNT(*) as count
            FROM tasker_step_execution_batch_transitions
            WHERE most_recent = true
            GROUP BY to_state
            ORDER BY count DESC
            "#
        )
        .fetch_all(pool)
        .await
    }

    /// Find batches that transitioned to a state within a time window
    pub async fn find_recently_transitioned(
        pool: &PgPool,
        to_state: &str,
        minutes_ago: i32,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Self,
            r#"
            SELECT id, batch_id, from_state, to_state, event_name,
                   metadata, sort_key, most_recent, created_at, updated_at
            FROM tasker_step_execution_batch_transitions
            WHERE to_state = $1
              AND created_at > NOW() - INTERVAL '1 minute' * $2
            ORDER BY created_at DESC
            "#,
            to_state,
            minutes_ago as f64
        )
        .fetch_all(pool)
        .await
    }
}

/// Helper struct for state counts
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StateCount {
    pub state: String,
    pub count: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        // Valid transitions
        assert!(BatchState::Created.can_transition_to(&BatchState::Published));
        assert!(BatchState::Published.can_transition_to(&BatchState::Executing));
        assert!(BatchState::Executing.can_transition_to(&BatchState::Completed));
        assert!(BatchState::Executing.can_transition_to(&BatchState::Failed));

        // Can cancel from non-terminal states
        assert!(BatchState::Created.can_transition_to(&BatchState::Cancelled));
        assert!(BatchState::Executing.can_transition_to(&BatchState::Cancelled));

        // Cannot transition from terminal states
        assert!(!BatchState::Completed.can_transition_to(&BatchState::Failed));
        assert!(!BatchState::Failed.can_transition_to(&BatchState::Completed));
        assert!(!BatchState::Cancelled.can_transition_to(&BatchState::Published));

        // Invalid transitions
        assert!(!BatchState::Created.can_transition_to(&BatchState::Completed));
        assert!(!BatchState::Published.can_transition_to(&BatchState::Failed));
    }

    #[test]
    fn test_terminal_states() {
        assert!(!BatchState::Created.is_terminal());
        assert!(!BatchState::Published.is_terminal());
        assert!(!BatchState::Executing.is_terminal());

        assert!(BatchState::Completed.is_terminal());
        assert!(BatchState::Failed.is_terminal());
        assert!(BatchState::Timeout.is_terminal());
        assert!(BatchState::Cancelled.is_terminal());
    }
}

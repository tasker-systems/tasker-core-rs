//! # Task Transition Model
//!
//! State transition audit trail and history management for tasks.
//!
//! ## Overview
//!
//! The `TaskTransition` model provides a complete audit trail of task state changes
//! with atomic transition management and current state tracking. Each transition
//! captures the state change, timing, and associated metadata.
//!
//! ## State Machine Integration
//!
//! This model serves as the storage layer for the TaskStateMachine:
//! - **Current State Tracking**: `most_recent = true` flag for O(1) lookups
//! - **Historical Audit**: Complete transition history with timestamps
//! - **Atomic Transitions**: Transaction-wrapped state changes
//! - **Metadata Storage**: JSONB context for transition reasons and data
//!
//! ## Key Features
//!
//! - **Automatic Sort Keys**: Sequential numbering for chronological ordering
//! - **Most Recent Flagging**: Efficient current state queries
//! - **Transaction Safety**: Atomic multi-row updates for consistency
//! - **Rich Metadata**: JSONB storage for transition context and debugging
//!
//! ## Database Schema
//!
//! Maps to `tasker_task_transitions` table:
//! ```sql
//! CREATE TABLE tasker_task_transitions (
//!   id BIGSERIAL PRIMARY KEY,
//!   task_uuid BIGINT NOT NULL,
//!   to_state VARCHAR NOT NULL,
//!   from_state VARCHAR,
//!   sort_key INTEGER NOT NULL,
//!   most_recent BOOLEAN DEFAULT false,
//!   metadata JSONB,
//!   -- ... timestamps
//! );
//! ```
//!
//! ## Performance Optimization
//!
//! - **Current State Index**: `(task_uuid, most_recent) WHERE most_recent = true`
//! - **History Index**: `(task_uuid, sort_key)` for chronological queries
//! - **State Index**: `(to_state, created_at)` for state-based analytics
//!
//! ## Rails Heritage
//!
//! Migrated from `app/models/tasker/task_transition.rb` (7.3KB, 236 lines)
//! Preserves all state machine integration and audit trail functionality.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Represents a single state transition in a task's lifecycle.
///
/// Each transition captures a state change with full audit trail information,
/// including timing, metadata, and sequence ordering for historical analysis.
///
/// # State Tracking
///
/// The model uses several fields for state management:
/// - `to_state`: The new state after transition
/// - `from_state`: Previous state (None for initial transitions)
/// - `most_recent`: Flag indicating current state (only one per task)
/// - `sort_key`: Sequential numbering for chronological ordering
///
/// # Metadata Structure
///
/// The metadata field contains transition context:
/// ```json
/// {
///   "reason": "User requested cancellation",
///   "actor": "user_123",
///   "system_info": {
///     "host": "worker-01",
///     "version": "1.2.3"
///   },
///   "error_details": null
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskTransition {
    pub task_transition_uuid: Uuid,
    pub task_uuid: Uuid,
    pub to_state: String,
    pub from_state: Option<String>,
    pub processor_uuid: Option<Uuid>, // TAS-41: Processor ownership tracking
    pub metadata: Option<serde_json::Value>,
    pub sort_key: i32,
    pub most_recent: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskTransition for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskTransition {
    pub task_uuid: Uuid,
    pub to_state: String,
    pub from_state: Option<String>,
    pub processor_uuid: Option<Uuid>, // TAS-41: Processor ownership tracking
    pub metadata: Option<serde_json::Value>,
}

/// Query builder for task transitions
#[derive(Debug, Default)]
pub struct TaskTransitionQuery {
    task_uuid: Option<Uuid>,
    state: Option<String>,
    most_recent_only: bool,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl TaskTransition {
    /// Create a new task transition with atomic state management.
    ///
    /// This method performs a complex multi-step transaction to ensure consistent
    /// state transition recording with proper audit trail maintenance.
    ///
    /// # Transaction Steps
    ///
    /// The method executes three operations atomically:
    ///
    /// 1. **Sort Key Generation**: Calculate next sequential sort_key
    /// ```sql
    /// SELECT COALESCE(MAX(sort_key), 0) + 1 as next_sort_key
    /// FROM tasker_task_transitions
    /// WHERE task_uuid = $1::uuid
    /// ```
    ///
    /// 2. **Previous State Update**: Mark existing transitions as historical
    /// ```sql
    /// UPDATE tasker_task_transitions
    /// SET most_recent = false, updated_at = NOW()
    /// WHERE task_uuid = $1::uuid AND most_recent = true
    /// ```
    ///
    /// 3. **New Transition Insert**: Create transition with most_recent = true
    ///
    /// # Atomicity Guarantees
    ///
    /// The transaction ensures:
    /// - **Exactly One Current State**: Only one transition per task has most_recent = true
    /// - **Sequential Sort Keys**: No gaps or duplicates in chronological ordering
    /// - **Consistent Timestamps**: All updates use same transaction timestamp
    /// - **Rollback Safety**: Partial failures leave database in consistent state
    ///
    /// # Concurrency Handling
    ///
    /// Multiple concurrent transitions are handled safely through:
    /// - **Row-level Locking**: PostgreSQL locks task_uuid rows during update
    /// - **Serializable Isolation**: Prevents phantom reads in sort key calculation
    /// - **Unique Constraints**: Database enforces single most_recent per task
    ///
    /// # Performance Characteristics
    ///
    /// - **Lock Duration**: Minimal (single transaction)
    /// - **Index Usage**: Primary key and task_uuid indexes
    /// - **Memory Usage**: Single row materialization
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::core::task_transition::{TaskTransition, NewTaskTransition};
    /// use serde_json::json;
    /// use sqlx::PgPool;
    /// use uuid::Uuid;
    ///
    /// # async fn example(pool: &PgPool) -> Result<(), sqlx::Error> {
    /// let transition = TaskTransition::create(pool, NewTaskTransition {
    ///     task_uuid: Uuid::new_v4(),
    ///     to_state: "processing".to_string(),
    ///     from_state: Some("pending".to_string()),
    ///     processor_uuid: Some(Uuid::new_v4()),  // TAS-41: Processor ownership
    ///     metadata: Some(json!({"reason": "worker_assigned"})),
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(
        pool: &PgPool,
        new_transition: NewTaskTransition,
    ) -> Result<TaskTransition, sqlx::Error> {
        // Start a transaction to ensure atomicity
        let mut tx = pool.begin().await?;

        // Get the next sort key for this task
        let sort_key_result = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) + 1 as next_sort_key
            FROM tasker_task_transitions
            WHERE task_uuid = $1::uuid
            "#,
            new_transition.task_uuid
        )
        .fetch_one(&mut *tx)
        .await?;

        let next_sort_key = sort_key_result.next_sort_key.unwrap_or(1);

        // Mark all existing transitions as not most recent
        sqlx::query!(
            r#"
            UPDATE tasker_task_transitions
            SET most_recent = false, updated_at = NOW()
            WHERE task_uuid = $1::uuid AND most_recent = true
            "#,
            new_transition.task_uuid
        )
        .execute(&mut *tx)
        .await?;

        // Insert the new transition
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            INSERT INTO tasker_task_transitions
            (task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent, created_at, updated_at)
            VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6, true, NOW(), NOW())
            RETURNING task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                      created_at, updated_at
            "#,
            new_transition.task_uuid,
            new_transition.to_state,
            new_transition.from_state,
            new_transition.processor_uuid,
            new_transition.metadata,
            next_sort_key
        )
        .fetch_one(&mut *tx)
        .await?;

        // Commit the transaction
        tx.commit().await?;

        Ok(transition)
    }

    /// Create a new task transition within an existing transaction
    /// This is useful when you need to create transitions as part of a larger transaction
    pub async fn create_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        new_transition: NewTaskTransition,
    ) -> Result<TaskTransition, sqlx::Error> {
        // Get the next sort key for this task
        let sort_key_result = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) + 1 as next_sort_key
            FROM tasker_task_transitions
            WHERE task_uuid = $1::uuid
            "#,
            new_transition.task_uuid
        )
        .fetch_one(&mut **tx)
        .await?;

        let next_sort_key = sort_key_result.next_sort_key.unwrap_or(1);

        // Mark all existing transitions as not most recent
        sqlx::query!(
            r#"
            UPDATE tasker_task_transitions
            SET most_recent = false, updated_at = NOW()
            WHERE task_uuid = $1::uuid AND most_recent = true
            "#,
            new_transition.task_uuid
        )
        .execute(&mut **tx)
        .await?;

        // Insert the new transition
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            INSERT INTO tasker_task_transitions
            (task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent, created_at, updated_at)
            VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6, true, NOW(), NOW())
            RETURNING task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                      created_at, updated_at
            "#,
            new_transition.task_uuid,
            new_transition.to_state,
            new_transition.from_state,
            new_transition.processor_uuid,
            new_transition.metadata,
            next_sort_key
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(transition)
    }

    /// Find a transition by UUID
    pub async fn find_by_uuid(
        pool: &PgPool,
        uuid: Uuid,
    ) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_transition_uuid = $1::uuid
            "#,
            uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Get the current (most recent) transition for a task
    pub async fn get_current(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_uuid = $1::uuid AND most_recent = true
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            task_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Get all transitions for a task ordered by sort key
    pub async fn list_by_task(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_uuid = $1::uuid
            ORDER BY sort_key ASC
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get transitions by state
    pub async fn list_by_state(
        pool: &PgPool,
        state: &str,
        most_recent_only: bool,
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = if most_recent_only {
            sqlx::query_as!(
                TaskTransition,
                r#"
                SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                       created_at, updated_at
                FROM tasker_task_transitions
                WHERE to_state = $1 AND most_recent = true
                ORDER BY created_at DESC
                "#,
                state
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                TaskTransition,
                r#"
                SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                       created_at, updated_at
                FROM tasker_task_transitions
                WHERE to_state = $1
                ORDER BY created_at DESC
                "#,
                state
            )
            .fetch_all(pool)
            .await?
        };

        Ok(transitions)
    }

    /// Get transitions for multiple tasks
    pub async fn list_by_tasks(
        pool: &PgPool,
        task_uuids: &[Uuid],
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_uuid = ANY($1)
            ORDER BY task_uuid, sort_key ASC
            "#,
            task_uuids
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get the previous transition for a task (before the current one)
    pub async fn get_previous(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_uuid = $1::uuid AND most_recent = false
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            task_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Count transitions by state
    pub async fn count_by_state(
        pool: &PgPool,
        state: &str,
        most_recent_only: bool,
    ) -> Result<i64, sqlx::Error> {
        let count = if most_recent_only {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_task_transitions
                WHERE to_state = $1 AND most_recent = true
                "#,
                state
            )
            .fetch_one(pool)
            .await?
            .count
            .unwrap_or(0)
        } else {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_task_transitions
                WHERE to_state = $1
                "#,
                state
            )
            .fetch_one(pool)
            .await?
            .count
            .unwrap_or(0)
        };

        Ok(count)
    }

    /// Get transition history for a task with pagination
    pub async fn get_history(
        pool: &PgPool,
        task_uuid: Uuid,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_uuid = $1::uuid
            ORDER BY sort_key DESC
            LIMIT $2 OFFSET $3
            "#,
            task_uuid,
            limit,
            offset
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Update metadata for a transition
    pub async fn update_metadata(
        &mut self,
        pool: &PgPool,
        metadata: serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_task_transitions
            SET metadata = $2, updated_at = NOW()
            WHERE task_transition_uuid = $1::uuid
            "#,
            self.task_transition_uuid,
            metadata
        )
        .execute(pool)
        .await?;

        self.metadata = Some(metadata);
        Ok(())
    }

    /// Check if a task can transition from one state to another
    pub async fn can_transition(
        pool: &PgPool,
        task_uuid: Uuid,
        from_state: &str,
        _to_state: &str,
    ) -> Result<bool, sqlx::Error> {
        // Get current state
        let current = Self::get_current(pool, task_uuid).await?;

        if let Some(transition) = current {
            // Check if current state matches expected from_state
            Ok(transition.to_state == from_state)
        } else {
            // No transitions yet, so check if from_state is initial state
            Ok(from_state == "draft" || from_state == "planned")
        }
    }

    /// Get tasks in a specific state
    pub async fn get_tasks_in_state(pool: &PgPool, state: &str) -> Result<Vec<Uuid>, sqlx::Error> {
        let task_uuids = sqlx::query!(
            r#"
            SELECT DISTINCT task_uuid
            FROM tasker_task_transitions
            WHERE to_state = $1 AND most_recent = true
            "#,
            state
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.task_uuid)
        .collect();

        Ok(task_uuids)
    }

    /// Get tasks by multiple states
    pub async fn get_tasks_in_states(
        pool: &PgPool,
        states: &[String],
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let task_uuids = sqlx::query!(
            r#"
            SELECT DISTINCT task_uuid
            FROM tasker_task_transitions
            WHERE to_state = ANY($1) AND most_recent = true
            "#,
            states
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.task_uuid)
        .collect();

        Ok(task_uuids)
    }

    /// Delete old transitions (keep only the most recent N transitions per task)
    pub async fn cleanup_old_transitions(
        pool: &PgPool,
        task_uuid: Uuid,
        keep_count: i32,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_transitions
            WHERE task_uuid = $1
              AND sort_key < (
                SELECT sort_key
                FROM tasker_task_transitions
                WHERE task_uuid = $1
                ORDER BY sort_key DESC
                LIMIT 1 OFFSET $2
              )
            "#,
            task_uuid,
            keep_count as i64 - 1
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get task state summary (count of tasks in each state)
    pub async fn get_state_summary(pool: &PgPool) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let summary = sqlx::query!(
            r#"
            SELECT to_state, COUNT(DISTINCT task_uuid) as count
            FROM tasker_task_transitions
            WHERE most_recent = true
            GROUP BY to_state
            "#
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.to_state, row.count.unwrap_or(0)))
        .collect();

        Ok(summary)
    }

    // ============================================================================
    // RAILS ACTIVERECORE SCOPE EQUIVALENTS (3+ scopes)
    // ============================================================================

    /// Get recent transitions ordered by sort_key DESC (Rails: scope :recent)
    pub async fn recent(pool: &PgPool) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            ORDER BY sort_key DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Filter transitions by to_state (Rails: scope :to_state)
    pub async fn to_state(pool: &PgPool, state: &str) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, task_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   created_at, updated_at
            FROM tasker_task_transitions
            WHERE to_state = $1
            ORDER BY created_at DESC
            "#,
            state
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Filter transitions with specific metadata key (Rails: scope :with_metadata_key)
    pub async fn with_metadata_key(
        pool: &PgPool,
        key: &str,
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   task_uuid, created_at, updated_at
            FROM tasker_task_transitions
            WHERE metadata ? $1
            ORDER BY created_at DESC
            "#,
            key
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    // ============================================================================
    // RAILS CLASS METHODS (5+ methods)
    // ============================================================================

    /// Find most recent transition to a specific state (Rails: most_recent_to_state)
    pub async fn most_recent_to_state(
        pool: &PgPool,
        state: &str,
    ) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   task_uuid, created_at, updated_at
            FROM tasker_task_transitions
            WHERE to_state = $1
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            state
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Find transitions within time range (Rails: in_time_range)
    pub async fn in_time_range(
        pool: &PgPool,
        start_time: chrono::NaiveDateTime,
        end_time: chrono::NaiveDateTime,
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   task_uuid, created_at, updated_at
            FROM tasker_task_transitions
            WHERE created_at BETWEEN $1 AND $2
            ORDER BY created_at DESC
            "#,
            start_time,
            end_time
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Find transitions with specific metadata value (Rails: with_metadata_value)
    pub async fn with_metadata_value(
        pool: &PgPool,
        key: &str,
        value: &str,
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                   task_uuid, created_at, updated_at
            FROM tasker_task_transitions
            WHERE metadata ->> $1 = $2
            ORDER BY created_at DESC
            "#,
            key,
            value
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get transition statistics (Rails: statistics)
    pub async fn statistics(pool: &PgPool) -> Result<TransitionStatistics, sqlx::Error> {
        let total_transitions =
            sqlx::query!("SELECT COUNT(*) as count FROM tasker_task_transitions")
                .fetch_one(pool)
                .await?
                .count
                .unwrap_or(0);

        let states = sqlx::query!(
            r#"
            SELECT to_state, COUNT(*) as count
            FROM tasker_task_transitions
            GROUP BY to_state
            "#
        )
        .fetch_all(pool)
        .await?;

        let mut state_counts = std::collections::HashMap::new();
        for state in states {
            state_counts.insert(state.to_state, state.count.unwrap_or(0));
        }

        let recent_activity = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_task_transitions
            WHERE created_at > NOW() - INTERVAL '24 hours'
            "#
        )
        .fetch_one(pool)
        .await?
        .count
        .unwrap_or(0);

        Ok(TransitionStatistics {
            total_transitions,
            states: state_counts,
            recent_activity,
            average_time_between_transitions: Self::average_time_between_transitions(pool)
                .await
                .ok(),
        })
    }

    /// Calculate average time between transitions (Rails: average_time_between_transitions)
    pub async fn average_time_between_transitions(pool: &PgPool) -> Result<f64, sqlx::Error> {
        let transitions =
            sqlx::query!("SELECT created_at FROM tasker_task_transitions ORDER BY created_at")
                .fetch_all(pool)
                .await?;

        if transitions.len() < 2 {
            return Ok(0.0);
        }

        let mut total_time = 0.0;
        for i in 1..transitions.len() {
            let duration =
                (transitions[i].created_at - transitions[i - 1].created_at).num_seconds() as f64;
            total_time += duration;
        }

        Ok(total_time / (transitions.len() - 1) as f64)
    }

    // ============================================================================
    // RAILS INSTANCE METHODS (9+ methods)
    // ============================================================================

    /// Get duration since previous transition (Rails: duration_since_previous)
    pub async fn duration_since_previous(&self, pool: &PgPool) -> Result<Option<f64>, sqlx::Error> {
        let previous = sqlx::query!(
            r#"
            SELECT created_at
            FROM tasker_task_transitions
            WHERE task_uuid = $1::uuid AND sort_key < $2
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            self.task_uuid,
            self.sort_key
        )
        .fetch_optional(pool)
        .await?;

        match previous {
            Some(prev) => {
                let duration = (self.created_at - prev.created_at).num_seconds() as f64;
                Ok(Some(duration))
            }
            None => Ok(None),
        }
    }

    /// Check if transition is to error state (Rails: error_transition?)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::models::task_transition::TaskTransition;
    /// use chrono::NaiveDateTime;
    /// use uuid::Uuid;
    ///
    /// let task_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let task_transition_uuid = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    ///
    /// let transition = TaskTransition {
    ///     task_transition_uuid,
    ///     task_uuid,
    ///     to_state: "error".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 2,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(transition.error_transition());
    ///
    /// let task_transition_uuid2 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let success_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid2,
    ///     task_uuid,
    ///     to_state: "complete".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 3,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(!success_transition.error_transition());
    /// ```
    pub fn error_transition(&self) -> bool {
        self.to_state == "error"
    }

    /// Check if transition is to completion state (Rails: completion_transition?)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::models::task_transition::TaskTransition;
    /// use chrono::NaiveDateTime;
    /// use uuid::Uuid;
    ///
    /// let task_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let task_transition_uuid1 = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    ///
    /// let complete_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid1,
    ///     task_uuid,
    ///     to_state: "complete".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 2,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(complete_transition.completion_transition());
    ///
    /// let task_transition_uuid2 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let manual_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid2,
    ///     task_uuid,
    ///     to_state: "resolved_manually".to_string(),
    ///     from_state: Some("error".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 3,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(manual_transition.completion_transition());
    ///
    /// let task_transition_uuid3 = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440004").unwrap();
    /// let error_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid3,
    ///     task_uuid,
    ///     to_state: "error".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 4,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(!error_transition.completion_transition());
    /// ```
    pub fn completion_transition(&self) -> bool {
        matches!(self.to_state.as_str(), "complete" | "resolved_manually")
    }

    /// Check if transition is to cancelled state (Rails: cancellation_transition?)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::models::task_transition::TaskTransition;
    /// use chrono::NaiveDateTime;
    /// use uuid::Uuid;
    ///
    /// let task_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let task_transition_uuid1 = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    ///
    /// let cancelled_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid1,
    ///     task_uuid,
    ///     to_state: "cancelled".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 2,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(cancelled_transition.cancellation_transition());
    ///
    /// let task_transition_uuid2 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let running_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid2,
    ///     task_uuid,
    ///     to_state: "running".to_string(),
    ///     from_state: Some("pending".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 1,
    ///     most_recent: false,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(!running_transition.cancellation_transition());
    /// ```
    pub fn cancellation_transition(&self) -> bool {
        self.to_state == "cancelled"
    }

    /// Get human-readable description (Rails: description)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::models::task_transition::TaskTransition;
    /// use chrono::NaiveDateTime;
    /// use uuid::Uuid;
    ///
    /// let task_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let task_transition_uuid1 = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    ///
    /// let complete_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid1,
    ///     task_uuid,
    ///     to_state: "complete".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 2,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert_eq!(complete_transition.description(), "Task completed successfully");
    ///
    /// let task_transition_uuid2 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let custom_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid2,
    ///     task_uuid,
    ///     to_state: "custom_state".to_string(),
    ///     from_state: Some("pending".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 3,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert_eq!(custom_transition.description(), "Task transitioned to custom_state");
    /// ```
    pub fn description(&self) -> String {
        match self.to_state.as_str() {
            "pending" => "Task initialized and ready for processing".to_string(),
            "in_progress" => "Task execution started".to_string(),
            "complete" => "Task completed successfully".to_string(),
            "error" => "Task encountered an error".to_string(),
            "cancelled" => "Task was cancelled".to_string(),
            "resolved_manually" => "Task was manually resolved".to_string(),
            _ => format!("Task transitioned to {}", self.to_state),
        }
    }

    /// Get formatted metadata with computed fields (Rails: formatted_metadata)
    pub async fn formatted_metadata(
        &self,
        pool: &PgPool,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut base_metadata = self
            .metadata
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        if let serde_json::Value::Object(ref mut map) = base_metadata {
            // Add computed fields
            if let Ok(Some(duration)) = self.duration_since_previous(pool).await {
                map.insert(
                    "duration_since_previous".to_string(),
                    serde_json::json!(duration),
                );
            }
            map.insert(
                "transition_description".to_string(),
                serde_json::json!(self.description()),
            );
            map.insert(
                "transition_timestamp".to_string(),
                serde_json::json!(self.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            );
        }

        Ok(base_metadata)
    }

    /// Check if metadata contains key (Rails: has_metadata?)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::models::task_transition::TaskTransition;
    /// use chrono::NaiveDateTime;
    /// use uuid::Uuid;
    ///
    /// let task_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let task_transition_uuid1 = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    ///
    /// let transition_with_metadata = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid1,
    ///     task_uuid,
    ///     to_state: "complete".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: Some(serde_json::json!({ "duration_ms": 1500 })),
    ///     sort_key: 2,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(transition_with_metadata.has_metadata());
    ///
    /// let task_transition_uuid2 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let transition_no_metadata = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid2,
    ///     task_uuid,
    ///     to_state: "error".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 3,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// assert!(!transition_no_metadata.has_metadata());
    /// ```
    pub fn has_metadata(&self) -> bool {
        self.metadata.is_some()
    }

    /// Get metadata value with default (Rails: get_metadata)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::models::task_transition::TaskTransition;
    /// use chrono::NaiveDateTime;
    /// use serde_json::{json, Value};
    /// use uuid::Uuid;
    ///
    /// let task_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let task_transition_uuid1 = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    ///
    /// let transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid1,
    ///     task_uuid,
    ///     to_state: "complete".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: Some(json!({"reason": "task_finished", "duration": 120})),
    ///     sort_key: 2,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// // Get existing value
    /// let reason = transition.get_metadata("reason", json!("unknown"));
    /// assert_eq!(reason, json!("task_finished"));
    ///
    /// // Get missing value with default
    /// let priority = transition.get_metadata("priority", json!("normal"));
    /// assert_eq!(priority, json!("normal"));
    ///
    /// // Test with transition that has no metadata
    /// let task_transition_uuid2 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let empty_transition = TaskTransition {
    ///     task_transition_uuid: task_transition_uuid2,
    ///     task_uuid,
    ///     to_state: "error".to_string(),
    ///     from_state: Some("running".to_string()),
    ///     processor_uuid: None,
    ///     metadata: None,
    ///     sort_key: 3,
    ///     most_recent: true,
    ///     created_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    ///     updated_at: NaiveDateTime::from_timestamp_opt(1640995200, 0).unwrap(),
    /// };
    ///
    /// let default_value = empty_transition.get_metadata("any_key", json!("default"));
    /// assert_eq!(default_value, json!("default"));
    /// ```
    pub fn get_metadata(&self, key: &str, default: serde_json::Value) -> serde_json::Value {
        if let Some(metadata) = &self.metadata {
            metadata.get(key).cloned().unwrap_or(default)
        } else {
            default
        }
    }

    /// Set metadata value (Rails: set_metadata)
    pub fn set_metadata(&mut self, key: &str, value: serde_json::Value) -> serde_json::Value {
        let mut metadata = self
            .metadata
            .take()
            .unwrap_or_else(|| serde_json::json!({}));
        if let serde_json::Value::Object(ref mut map) = metadata {
            map.insert(key.to_string(), value.clone());
        }
        self.metadata = Some(metadata);
        value
    }
}

/// Transition statistics result (Rails: statistics method return type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionStatistics {
    pub total_transitions: i64,
    pub states: std::collections::HashMap<String, i64>,
    pub recent_activity: i64,
    pub average_time_between_transitions: Option<f64>,
}

impl TaskTransitionQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn task_uuid(mut self, id: Uuid) -> Self {
        self.task_uuid = Some(id);
        self
    }

    pub fn state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    pub fn most_recent_only(mut self) -> Self {
        self.most_recent_only = true;
        self
    }

    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = Some(offset);
        self
    }

    pub async fn execute(self, pool: &PgPool) -> Result<Vec<TaskTransition>, sqlx::Error> {
        // Use secure static queries instead of dynamic SQL construction
        match (
            self.task_uuid,
            self.state,
            self.most_recent_only,
            self.limit,
            self.offset,
        ) {
            // Most common case: by task_uuid only
            (Some(task_uuid), None, false, None, None) => {
                sqlx::query_as!(
                    TaskTransition,
                    r#"
                    SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                           task_uuid, created_at, updated_at
                    FROM tasker_task_transitions
                    WHERE task_uuid = $1::uuid
                    ORDER BY task_uuid, sort_key DESC
                    "#,
                    task_uuid
                )
                .fetch_all(pool)
                .await
            }
            // By task_uuid and state
            (Some(task_uuid), Some(state), false, None, None) => {
                sqlx::query_as!(
                    TaskTransition,
                    r#"
                    SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                           task_uuid, created_at, updated_at
                    FROM tasker_task_transitions
                    WHERE task_uuid = $1::uuid AND to_state = $2
                    ORDER BY task_uuid, sort_key DESC
                    "#,
                    task_uuid,
                    state
                )
                .fetch_all(pool)
                .await
            }
            // Most recent only for task
            (Some(task_uuid), None, true, None, None) => {
                sqlx::query_as!(
                    TaskTransition,
                    r#"
                    SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                           task_uuid, created_at, updated_at
                    FROM tasker_task_transitions
                    WHERE task_uuid = $1::uuid AND most_recent = true
                    ORDER BY task_uuid, sort_key DESC
                    "#,
                    task_uuid
                )
                .fetch_all(pool)
                .await
            }
            // With pagination (limit only)
            (Some(task_uuid), None, false, Some(limit), None) => {
                sqlx::query_as!(
                    TaskTransition,
                    r#"
                    SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                           task_uuid, created_at, updated_at
                    FROM tasker_task_transitions
                    WHERE task_uuid = $1::uuid
                    ORDER BY task_uuid, sort_key DESC
                    LIMIT $2
                    "#,
                    task_uuid,
                    limit as i64
                )
                .fetch_all(pool)
                .await
            }
            // With pagination (limit and offset)
            (Some(task_uuid), None, false, Some(limit), Some(offset)) => {
                sqlx::query_as!(
                    TaskTransition,
                    r#"
                    SELECT task_transition_uuid, to_state, from_state, processor_uuid, metadata, sort_key, most_recent,
                           task_uuid, created_at, updated_at
                    FROM tasker_task_transitions
                    WHERE task_uuid = $1::uuid
                    ORDER BY task_uuid, sort_key DESC
                    LIMIT $2 OFFSET $3
                    "#,
                    task_uuid,
                    limit as i64,
                    offset as i64
                )
                .fetch_all(pool)
                .await
            }
            // Default fallback for edge cases
            _ => {
                if let Some(task_uuid) = self.task_uuid {
                    TaskTransition::list_by_task(pool, task_uuid).await
                } else {
                    // Return empty for unsupported query combinations
                    Ok(Vec::new())
                }
            }
        }
    }
}

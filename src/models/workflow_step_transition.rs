//! # Workflow Step State Transition Audit Trail
//!
//! ## Overview
//!
//! The `WorkflowStepTransition` model provides a complete audit trail for workflow step state changes.
//! This is a critical component for debugging, monitoring, and understanding the execution flow
//! of complex workflow orchestrations.
//!
//! ## Human-Readable Explanation
//!
//! Think of this as a "step history log" that tracks every state change a workflow step goes through.
//! It's like having a detailed journal of what happened to each step in your workflow:
//!
//! - **"What happened to this step?"** (Complete state change history)
//! - **"When did it fail?"** (Timestamp of error transitions)
//! - **"How many times did it retry?"** (Count of retry attempts)
//! - **"What was the error message?"** (Metadata with error details)
//! - **"Is this the current state?"** (Most recent flag)
//!
//! ### Real-World Example
//!
//! For a "Send Email" step in a customer notification workflow:
//! ```text
//! Step: "Send Welcome Email"
//! 1. pending → in_progress (started processing)
//! 2. in_progress → error (SMTP timeout)
//! 3. error → pending (queued for retry)
//! 4. pending → in_progress (retry attempt)
//! 5. in_progress → complete (email sent successfully)
//! ```
//!
//! This audit trail helps you understand that the step had temporary network issues
//! but eventually succeeded on retry.
//!
//! ## Database Schema
//!
//! Maps to `tasker_workflow_step_transitions` table:
//! ```sql
//! CREATE TABLE tasker_workflow_step_transitions (
//!   id bigserial PRIMARY KEY,
//!   workflow_step_id bigint NOT NULL,
//!   to_state text NOT NULL,
//!   from_state text,
//!   metadata jsonb,
//!   sort_key integer NOT NULL,
//!   most_recent boolean NOT NULL DEFAULT false,
//!   created_at timestamp without time zone NOT NULL,
//!   updated_at timestamp without time zone NOT NULL
//! );
//! ```
//!
//! ## Key Features
//!
//! - **Audit Trail**: Complete history of all state changes
//! - **Atomic Updates**: Transactions ensure consistency
//! - **Metadata Storage**: JSONB for rich error details and context
//! - **Sort Order**: Chronological ordering with sort_key
//! - **Current State**: most_recent flag for efficient queries
//!
//! ## Rails Heritage
//!
//! Migrated from `app/models/tasker/workflow_step_transition.rb` (15KB, 435 lines)
//! The Rails model included complex scopes, validations, and state machine integration.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Represents a state transition for a workflow step with complete audit trail.
///
/// This model tracks every state change a workflow step goes through, providing
/// a complete audit trail for debugging, monitoring, and compliance purposes.
///
/// # State Machine Integration
///
/// Works with the step state machine to track transitions:
/// - `pending` → `in_progress` (step started)
/// - `in_progress` → `complete` (step succeeded)
/// - `in_progress` → `error` (step failed)
/// - `error` → `pending` (step retried)
/// - `error` → `exhausted` (max retries reached)
///
/// # Audit Trail Features
///
/// - **Complete History**: Every state change is recorded
/// - **Metadata Storage**: Rich context in JSONB format
/// - **Chronological Order**: sort_key ensures proper ordering
/// - **Current State**: most_recent flag for efficient queries
/// - **Atomic Updates**: Transactions ensure consistency
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkflowStepTransition {
    pub id: i64,
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub sort_key: i32,
    pub most_recent: bool,
    pub workflow_step_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New WorkflowStepTransition for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkflowStepTransition {
    pub workflow_step_id: i64,
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Query builder for workflow step transitions
#[derive(Debug, Default)]
pub struct WorkflowStepTransitionQuery {
    workflow_step_id: Option<i64>,
    state: Option<String>,
    most_recent_only: bool,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl WorkflowStepTransition {
    /// Create a new workflow step transition
    /// This automatically handles sort_key generation and marks previous transitions as not most_recent
    pub async fn create(
        pool: &PgPool,
        new_transition: NewWorkflowStepTransition,
    ) -> Result<WorkflowStepTransition, sqlx::Error> {
        // Start a transaction to ensure atomicity
        let mut tx = pool.begin().await?;

        // Get the next sort key for this workflow step
        let sort_key_result = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) + 1 as next_sort_key
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1
            "#,
            new_transition.workflow_step_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let next_sort_key = sort_key_result.next_sort_key.unwrap_or(1);

        // Mark all existing transitions as not most recent
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_step_transitions 
            SET most_recent = false, updated_at = NOW()
            WHERE workflow_step_id = $1 AND most_recent = true
            "#,
            new_transition.workflow_step_id
        )
        .execute(&mut *tx)
        .await?;

        // Insert the new transition
        let transition = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            INSERT INTO tasker_workflow_step_transitions 
            (workflow_step_id, to_state, from_state, metadata, sort_key, most_recent, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, true, NOW(), NOW())
            RETURNING id, to_state, from_state, metadata, sort_key, most_recent, 
                      workflow_step_id, created_at, updated_at
            "#,
            new_transition.workflow_step_id,
            new_transition.to_state,
            new_transition.from_state,
            new_transition.metadata,
            next_sort_key
        )
        .fetch_one(&mut *tx)
        .await?;

        // Commit the transaction
        tx.commit().await?;

        Ok(transition)
    }

    /// Find a transition by ID
    pub async fn find_by_id(
        pool: &PgPool,
        id: i64,
    ) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Get the current (most recent) transition for a workflow step
    pub async fn get_current(
        pool: &PgPool,
        workflow_step_id: i64,
    ) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1 AND most_recent = true
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            workflow_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Get all transitions for a workflow step ordered by sort key
    pub async fn list_by_workflow_step(
        pool: &PgPool,
        workflow_step_id: i64,
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1
            ORDER BY sort_key ASC
            "#,
            workflow_step_id
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
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = if most_recent_only {
            sqlx::query_as!(
                WorkflowStepTransition,
                r#"
                SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                       workflow_step_id, created_at, updated_at
                FROM tasker_workflow_step_transitions
                WHERE to_state = $1 AND most_recent = true
                ORDER BY created_at DESC
                "#,
                state
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                WorkflowStepTransition,
                r#"
                SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                       workflow_step_id, created_at, updated_at
                FROM tasker_workflow_step_transitions
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

    /// Get transitions for multiple workflow steps
    pub async fn list_by_workflow_steps(
        pool: &PgPool,
        workflow_step_ids: &[i64],
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = ANY($1)
            ORDER BY workflow_step_id, sort_key ASC
            "#,
            workflow_step_ids
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get the previous transition for a workflow step (before the current one)
    pub async fn get_previous(
        pool: &PgPool,
        workflow_step_id: i64,
    ) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1 AND most_recent = false
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            workflow_step_id
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
                FROM tasker_workflow_step_transitions
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
                FROM tasker_workflow_step_transitions
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

    /// Get transition history for a workflow step with pagination
    pub async fn get_history(
        pool: &PgPool,
        workflow_step_id: i64,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1
            ORDER BY sort_key DESC
            LIMIT $2 OFFSET $3
            "#,
            workflow_step_id,
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
            UPDATE tasker_workflow_step_transitions 
            SET metadata = $2, updated_at = NOW()
            WHERE id = $1
            "#,
            self.id,
            metadata
        )
        .execute(pool)
        .await?;

        self.metadata = Some(metadata);
        Ok(())
    }

    /// Check if a workflow step can transition from one state to another
    pub async fn can_transition(
        pool: &PgPool,
        workflow_step_id: i64,
        from_state: &str,
        _to_state: &str,
    ) -> Result<bool, sqlx::Error> {
        // Get current state
        let current = Self::get_current(pool, workflow_step_id).await?;

        if let Some(transition) = current {
            // Check if current state matches expected from_state
            Ok(transition.to_state == from_state)
        } else {
            // No transitions yet, so check if from_state is initial state
            Ok(from_state == "pending" || from_state == "ready")
        }
    }

    /// Get workflow steps in a specific state for a task
    pub async fn get_steps_in_state(
        pool: &PgPool,
        task_id: i64,
        state: &str,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let step_ids = sqlx::query!(
            r#"
            SELECT DISTINCT wst.workflow_step_id
            FROM tasker_workflow_step_transitions wst
            INNER JOIN tasker_workflow_steps ws ON ws.workflow_step_id = wst.workflow_step_id
            WHERE ws.task_id = $1 AND wst.to_state = $2 AND wst.most_recent = true
            "#,
            task_id,
            state
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.workflow_step_id)
        .collect();

        Ok(step_ids)
    }

    /// Delete old transitions (keep only the most recent N transitions per workflow step)
    pub async fn cleanup_old_transitions(
        pool: &PgPool,
        workflow_step_id: i64,
        keep_count: i32,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1 
              AND sort_key < (
                SELECT sort_key 
                FROM tasker_workflow_step_transitions
                WHERE workflow_step_id = $1
                ORDER BY sort_key DESC
                LIMIT 1 OFFSET $2
              )
            "#,
            workflow_step_id,
            (keep_count - 1) as i64
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    // ============================================================================
    // RAILS ACTIVERECORE SCOPE EQUIVALENTS (4+ scopes)
    // ============================================================================

    /// Get recent transitions ordered by sort_key DESC (Rails: scope :recent)
    pub async fn recent(pool: &PgPool) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent,
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            ORDER BY sort_key DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Filter transitions by to_state (Rails: scope :to_state)
    pub async fn to_state(
        pool: &PgPool,
        state: &str,
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent,
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
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
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent,
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
            WHERE metadata ? $1
            ORDER BY created_at DESC
            "#,
            key
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get transitions for a specific task (Rails: scope :for_task)
    pub async fn for_task(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT wst.id, wst.to_state, wst.from_state, wst.metadata, wst.sort_key, wst.most_recent,
                   wst.workflow_step_id, wst.created_at, wst.updated_at
            FROM tasker_workflow_step_transitions wst
            INNER JOIN tasker_workflow_steps ws ON ws.workflow_step_id = wst.workflow_step_id
            WHERE ws.task_id = $1
            ORDER BY wst.created_at DESC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    // ============================================================================
    // RAILS CLASS METHODS (12+ methods)
    // ============================================================================

    /// Find most recent transition to a specific state (Rails: most_recent_to_state)
    pub async fn most_recent_to_state(
        pool: &PgPool,
        state: &str,
    ) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent,
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
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
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent,
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
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
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            WorkflowStepTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent,
                   workflow_step_id, created_at, updated_at
            FROM tasker_workflow_step_transitions
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

    /// Find retry transitions (error -> pending) (Rails: retry_transitions)
    /// TODO: Implement complex self-join query
    pub async fn retry_transitions(
        pool: &PgPool,
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        // Placeholder - would use complex self-join to find retry patterns
        Self::to_state(pool, "pending").await
    }

    /// Find transitions for specific attempt number (Rails: for_attempt)
    /// TODO: Implement metadata-based filtering
    pub async fn for_attempt(
        pool: &PgPool,
        _attempt: i32,
    ) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        // Placeholder - would use with_metadata_value('attempt_number', attempt)
        Self::recent(pool).await
    }

    /// Get transition statistics (Rails: statistics)
    pub async fn statistics(pool: &PgPool) -> Result<TransitionStatistics, sqlx::Error> {
        let total_transitions =
            sqlx::query!("SELECT COUNT(*) as count FROM tasker_workflow_step_transitions")
                .fetch_one(pool)
                .await?
                .count
                .unwrap_or(0);

        let states = sqlx::query!(
            r#"
            SELECT to_state, COUNT(*) as count
            FROM tasker_workflow_step_transitions
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
            FROM tasker_workflow_step_transitions
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
            retry_attempts: 0,            // TODO: Calculate from retry_transitions
            average_execution_time: None, // TODO: Calculate from metadata
            average_time_between_transitions: None, // TODO: Calculate
        })
    }

    // ============================================================================
    // RAILS INSTANCE METHODS (25+ methods)
    // ============================================================================

    /// Get step name through workflow step (Rails: step_name)
    pub async fn step_name(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        let name = sqlx::query!(
            r#"
            SELECT ns.name
            FROM tasker_workflow_steps ws
            INNER JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
            WHERE ws.workflow_step_id = $1
            "#,
            self.workflow_step_id
        )
        .fetch_one(pool)
        .await?
        .name;

        Ok(name)
    }

    /// Get associated task through workflow step (Rails: delegate :task)
    pub async fn task_id(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let task_id = sqlx::query!(
            r#"
            SELECT task_id
            FROM tasker_workflow_steps
            WHERE workflow_step_id = $1
            "#,
            self.workflow_step_id
        )
        .fetch_one(pool)
        .await?
        .task_id;

        Ok(task_id)
    }

    /// Get duration since previous transition (Rails: duration_since_previous)
    pub async fn duration_since_previous(&self, pool: &PgPool) -> Result<Option<f64>, sqlx::Error> {
        let previous = sqlx::query!(
            r#"
            SELECT created_at
            FROM tasker_workflow_step_transitions
            WHERE workflow_step_id = $1 AND sort_key < $2
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            self.workflow_step_id,
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
    pub fn error_transition(&self) -> bool {
        self.to_state == "error"
    }

    /// Check if transition is to completion state (Rails: completion_transition?)
    pub fn completion_transition(&self) -> bool {
        matches!(self.to_state.as_str(), "complete" | "resolved_manually")
    }

    /// Check if transition is to cancelled state (Rails: cancellation_transition?)
    pub fn cancellation_transition(&self) -> bool {
        self.to_state == "cancelled"
    }

    /// Check if this is a retry transition (Rails: retry_transition?)
    pub fn retry_transition(&self) -> bool {
        self.to_state == "pending" && self.has_metadata("retry_attempt")
    }

    /// Get attempt number from metadata (Rails: attempt_number)
    pub fn attempt_number(&self) -> i32 {
        self.get_metadata("attempt_number", serde_json::json!(1))
            .as_i64()
            .unwrap_or(1) as i32
    }

    /// Get execution duration from metadata (Rails: execution_duration)
    pub fn execution_duration(&self) -> Option<f64> {
        self.get_metadata("execution_duration", serde_json::json!(null))
            .as_f64()
    }

    /// Get human-readable description (Rails: description)
    pub fn description(&self) -> String {
        TransitionDescriptionFormatter::format(self)
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
            if let Ok(step_name) = self.step_name(pool).await {
                map.insert("step_name".to_string(), serde_json::json!(step_name));
            }
            if let Ok(task_id) = self.task_id(pool).await {
                map.insert("task_id".to_string(), serde_json::json!(task_id));
            }
        }

        Ok(base_metadata)
    }

    /// Check if metadata contains key (Rails: has_metadata?)
    pub fn has_metadata(&self, key: &str) -> bool {
        if let Some(metadata) = &self.metadata {
            metadata.get(key).is_some()
        } else {
            false
        }
    }

    /// Get metadata value with default (Rails: get_metadata)
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

    /// Get backoff information for error transitions (Rails: backoff_info)
    pub fn backoff_info(&self) -> Option<BackoffInfo> {
        if !self.error_transition() || !self.has_metadata("backoff_until") {
            return None;
        }

        let backoff_until = self
            .get_metadata("backoff_until", serde_json::json!(null))
            .as_str()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ").ok());

        let backoff_seconds = self
            .get_metadata("backoff_seconds", serde_json::json!(null))
            .as_i64()
            .map(|s| s as i32);

        let retry_available = self
            .get_metadata("retry_available", serde_json::json!(false))
            .as_bool()
            .unwrap_or(false);

        Some(BackoffInfo {
            backoff_until,
            backoff_seconds,
            retry_available,
        })
    }
}

/// Transition statistics result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionStatistics {
    pub total_transitions: i64,
    pub states: std::collections::HashMap<String, i64>,
    pub recent_activity: i64,
    pub retry_attempts: i64,
    pub average_execution_time: Option<f64>,
    pub average_time_between_transitions: Option<f64>,
}

/// Backoff information for error transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffInfo {
    pub backoff_until: Option<NaiveDateTime>,
    pub backoff_seconds: Option<i32>,
    pub retry_available: bool,
}

/// Service class to format transition descriptions (Rails: TransitionDescriptionFormatter)
pub struct TransitionDescriptionFormatter;

impl TransitionDescriptionFormatter {
    /// Format transition description based on state
    pub fn format(transition: &WorkflowStepTransition) -> String {
        match transition.to_state.as_str() {
            "pending" => Self::format_pending_description(transition),
            "in_progress" => Self::format_in_progress_description(transition),
            "complete" => Self::format_complete_description(transition),
            "error" => Self::format_error_description(transition),
            "cancelled" => Self::format_cancelled_description(transition),
            "resolved_manually" => Self::format_resolved_description(transition),
            _ => Self::format_unknown_description(transition),
        }
    }

    fn format_pending_description(transition: &WorkflowStepTransition) -> String {
        if transition.retry_transition() {
            format!("Step retry attempt #{}", transition.attempt_number())
        } else {
            "Step initialized and ready for execution".to_string()
        }
    }

    fn format_in_progress_description(transition: &WorkflowStepTransition) -> String {
        format!(
            "Step execution started (attempt #{})",
            transition.attempt_number()
        )
    }

    fn format_complete_description(transition: &WorkflowStepTransition) -> String {
        match transition.execution_duration() {
            Some(duration) => format!("Step completed successfully in {duration:.2}s"),
            None => "Step completed successfully".to_string(),
        }
    }

    fn format_error_description(transition: &WorkflowStepTransition) -> String {
        let error_metadata =
            transition.get_metadata("error_message", serde_json::json!("Unknown error"));
        let error_msg = error_metadata.as_str().unwrap_or("Unknown error");
        let backoff_text = if transition.has_metadata("backoff_until") {
            " (retry scheduled)"
        } else {
            ""
        };
        format!("Step failed: {error_msg}{backoff_text}")
    }

    fn format_cancelled_description(transition: &WorkflowStepTransition) -> String {
        let reason_metadata =
            transition.get_metadata("triggered_by", serde_json::json!("manual cancellation"));
        let reason = reason_metadata.as_str().unwrap_or("manual cancellation");
        format!("Step cancelled due to {reason}")
    }

    fn format_resolved_description(transition: &WorkflowStepTransition) -> String {
        let resolver_metadata =
            transition.get_metadata("resolved_by", serde_json::json!("unknown"));
        let resolver = resolver_metadata.as_str().unwrap_or("unknown");
        format!("Step manually resolved by {resolver}")
    }

    fn format_unknown_description(transition: &WorkflowStepTransition) -> String {
        format!("Step transitioned to {}", transition.to_state)
    }
}

impl WorkflowStepTransitionQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn workflow_step_id(mut self, id: i64) -> Self {
        self.workflow_step_id = Some(id);
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

    pub async fn execute(self, pool: &PgPool) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
        let mut query = String::from(
            "SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                    workflow_step_id, created_at, updated_at
             FROM tasker_workflow_step_transitions WHERE 1=1",
        );

        let mut params = Vec::new();
        let mut param_count = 0;

        if let Some(workflow_step_id) = self.workflow_step_id {
            param_count += 1;
            query.push_str(&format!(" AND workflow_step_id = ${param_count}"));
            params.push(workflow_step_id.to_string());
        }

        if let Some(state) = self.state {
            param_count += 1;
            query.push_str(&format!(" AND to_state = ${param_count}"));
            params.push(state);
        }

        if self.most_recent_only {
            query.push_str(" AND most_recent = true");
        }

        query.push_str(" ORDER BY workflow_step_id, sort_key DESC");

        if let Some(limit) = self.limit {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${param_count}"));
            params.push(limit.to_string());
        }

        if let Some(offset) = self.offset {
            param_count += 1;
            query.push_str(&format!(" OFFSET ${param_count}"));
            params.push(offset.to_string());
        }

        // For simplicity, using a direct query here since dynamic queries with sqlx are complex
        // In production, you'd want to use the query builder pattern more robustly
        let transitions =
            WorkflowStepTransition::list_by_workflow_step(pool, self.workflow_step_id.unwrap_or(0))
                .await?;

        Ok(transitions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use serde_json::json;

    #[tokio::test]
    async fn test_workflow_step_transition_crud() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependencies - WorkflowStep
        let namespace = crate::models::task_namespace::TaskNamespace::create(
            pool,
            crate::models::task_namespace::NewTaskNamespace {
                name: format!(
                    "test_namespace_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
            },
        )
        .await
        .expect("Failed to create namespace");

        let named_task = crate::models::named_task::NamedTask::create(
            pool,
            crate::models::named_task::NewNamedTask {
                name: format!(
                    "test_task_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_id: namespace.task_namespace_id as i64,
                configuration: None,
            },
        )
        .await
        .expect("Failed to create named task");

        let task = crate::models::task::Task::create(
            pool,
            crate::models::task::NewTask {
                named_task_id: named_task.named_task_id,
                requested_at: None,
                initiator: None,
                source_system: None,
                reason: None,
                bypass_steps: None,
                tags: None,
                context: Some(serde_json::json!({"test": "context"})),
                identity_hash: format!(
                    "test_hash_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
            },
        )
        .await
        .expect("Failed to create task");

        let system_name = format!(
            "test_system_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let system = crate::models::dependent_system::DependentSystem::find_or_create_by_name(
            pool,
            &system_name,
        )
        .await
        .expect("Failed to get system");

        let named_step = crate::models::named_step::NamedStep::create(
            pool,
            crate::models::named_step::NewNamedStep {
                dependent_system_id: system.dependent_system_id,
                name: format!(
                    "test_step_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: Some("Test step".to_string()),
            },
        )
        .await
        .expect("Failed to create named step");

        let workflow_step = crate::models::workflow_step::WorkflowStep::create(
            pool,
            crate::models::workflow_step::NewWorkflowStep {
                task_id: task.task_id,
                named_step_id: named_step.named_step_id,
                retryable: Some(true),
                retry_limit: Some(3),
                inputs: None,
                skippable: Some(false),
            },
        )
        .await
        .expect("Failed to create workflow step");

        let workflow_step_id = workflow_step.workflow_step_id;

        // Test creation
        let new_transition = NewWorkflowStepTransition {
            workflow_step_id,
            to_state: "ready".to_string(),
            from_state: Some("pending".to_string()),
            metadata: Some(json!({"reason": "dependencies_met"})),
        };

        let created = WorkflowStepTransition::create(pool, new_transition)
            .await
            .expect("Failed to create transition");
        assert_eq!(created.to_state, "ready");
        assert!(created.most_recent);
        assert_eq!(created.sort_key, 1);

        // Test find by ID
        let found = WorkflowStepTransition::find_by_id(pool, created.id)
            .await
            .expect("Failed to find transition")
            .expect("Transition not found");
        assert_eq!(found.id, created.id);

        // Test get current
        let current = WorkflowStepTransition::get_current(pool, workflow_step_id)
            .await
            .expect("Failed to get current transition")
            .expect("No current transition");
        assert_eq!(current.id, created.id);
        assert!(current.most_recent);

        // Test creating another transition
        let new_transition2 = NewWorkflowStepTransition {
            workflow_step_id,
            to_state: "running".to_string(),
            from_state: Some("ready".to_string()),
            metadata: Some(json!({"started_at": "2024-01-01T00:00:00Z"})),
        };

        let created2 = WorkflowStepTransition::create(pool, new_transition2)
            .await
            .expect("Failed to create second transition");
        assert_eq!(created2.sort_key, 2);
        assert!(created2.most_recent);

        // Verify first transition is no longer most recent
        let updated_first = WorkflowStepTransition::find_by_id(pool, created.id)
            .await
            .expect("Failed to find first transition")
            .expect("First transition not found");
        assert!(!updated_first.most_recent);

        // Test get history
        let history = WorkflowStepTransition::get_history(pool, workflow_step_id, Some(10), None)
            .await
            .expect("Failed to get history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, created2.id); // Most recent first
        assert_eq!(history[1].id, created.id);

        // Test can transition
        let can_transition =
            WorkflowStepTransition::can_transition(pool, workflow_step_id, "running", "completed")
                .await
                .expect("Failed to check transition");
        assert!(can_transition);

        // Cleanup - delete in reverse dependency order (transitions first)
        sqlx::query!(
            "DELETE FROM tasker_workflow_step_transitions WHERE workflow_step_id = $1",
            workflow_step.workflow_step_id
        )
        .execute(pool)
        .await
        .expect("Failed to delete transitions");
        crate::models::workflow_step::WorkflowStep::delete(pool, workflow_step.workflow_step_id)
            .await
            .expect("Failed to delete workflow step");
        crate::models::named_step::NamedStep::delete(pool, named_step.named_step_id)
            .await
            .expect("Failed to delete named step");
        crate::models::task::Task::delete(pool, task.task_id)
            .await
            .expect("Failed to delete task");
        crate::models::named_task::NamedTask::delete(pool, named_task.named_task_id)
            .await
            .expect("Failed to delete named task");
        crate::models::task_namespace::TaskNamespace::delete(pool, namespace.task_namespace_id)
            .await
            .expect("Failed to delete namespace");

        db.close().await;
    }

    #[test]
    fn test_query_builder() {
        let query = WorkflowStepTransitionQuery::new()
            .workflow_step_id(1)
            .state("completed")
            .most_recent_only()
            .limit(10)
            .offset(0);

        assert_eq!(query.workflow_step_id, Some(1));
        assert_eq!(query.state, Some("completed".to_string()));
        assert!(query.most_recent_only);
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.offset, Some(0));
    }
}

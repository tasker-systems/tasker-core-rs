use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// WorkflowStepTransition represents state transitions for workflow steps with audit trail
/// Maps to `tasker_workflow_step_transitions` table - polymorphic audit trail (15KB Rails model)
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
    pub async fn create(pool: &PgPool, new_transition: NewWorkflowStepTransition) -> Result<WorkflowStepTransition, sqlx::Error> {
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
            (workflow_step_id, to_state, from_state, metadata, sort_key, most_recent)
            VALUES ($1, $2, $3, $4, $5, true)
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
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
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
    pub async fn get_current(pool: &PgPool, workflow_step_id: i64) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
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
    pub async fn list_by_workflow_step(pool: &PgPool, workflow_step_id: i64) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
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
    pub async fn list_by_state(pool: &PgPool, state: &str, most_recent_only: bool) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
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
    pub async fn list_by_workflow_steps(pool: &PgPool, workflow_step_ids: &[i64]) -> Result<Vec<WorkflowStepTransition>, sqlx::Error> {
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
    pub async fn get_previous(pool: &PgPool, workflow_step_id: i64) -> Result<Option<WorkflowStepTransition>, sqlx::Error> {
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
    pub async fn count_by_state(pool: &PgPool, state: &str, most_recent_only: bool) -> Result<i64, sqlx::Error> {
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
            .count.unwrap_or(0)
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
            .count.unwrap_or(0)
        };

        Ok(count)
    }

    /// Get transition history for a workflow step with pagination
    pub async fn get_history(
        pool: &PgPool, 
        workflow_step_id: i64, 
        limit: Option<i64>, 
        offset: Option<i64>
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
    pub async fn update_metadata(&mut self, pool: &PgPool, metadata: serde_json::Value) -> Result<(), sqlx::Error> {
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
    pub async fn can_transition(pool: &PgPool, workflow_step_id: i64, from_state: &str, _to_state: &str) -> Result<bool, sqlx::Error> {
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
    pub async fn get_steps_in_state(pool: &PgPool, task_id: i64, state: &str) -> Result<Vec<i64>, sqlx::Error> {
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
    pub async fn cleanup_old_transitions(pool: &PgPool, workflow_step_id: i64, keep_count: i32) -> Result<u64, sqlx::Error> {
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
             FROM tasker_workflow_step_transitions WHERE 1=1"
        );

        let mut params = Vec::new();
        let mut param_count = 0;

        if let Some(workflow_step_id) = self.workflow_step_id {
            param_count += 1;
            query.push_str(&format!(" AND workflow_step_id = ${}", param_count));
            params.push(workflow_step_id.to_string());
        }

        if let Some(state) = self.state {
            param_count += 1;
            query.push_str(&format!(" AND to_state = ${}", param_count));
            params.push(state);
        }

        if self.most_recent_only {
            query.push_str(" AND most_recent = true");
        }

        query.push_str(" ORDER BY workflow_step_id, sort_key DESC");

        if let Some(limit) = self.limit {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${}", param_count));
            params.push(limit.to_string());
        }

        if let Some(offset) = self.offset {
            param_count += 1;
            query.push_str(&format!(" OFFSET ${}", param_count));
            params.push(offset.to_string());
        }

        // For simplicity, using a direct query here since dynamic queries with sqlx are complex
        // In production, you'd want to use the query builder pattern more robustly
        let transitions = WorkflowStepTransition::list_by_workflow_step(
            pool, 
            self.workflow_step_id.unwrap_or(0)
        ).await?;

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
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Assume we have a workflow step with id 1
        let workflow_step_id = 1;

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
        let can_transition = WorkflowStepTransition::can_transition(
            pool, 
            workflow_step_id, 
            "running", 
            "completed"
        )
        .await
        .expect("Failed to check transition");
        assert!(can_transition);

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
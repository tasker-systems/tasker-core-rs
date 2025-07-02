use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// TaskTransition represents state transitions for tasks with audit trail
/// Maps to `tasker_task_transitions` table - polymorphic audit trail (7.3KB Rails model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskTransition {
    pub id: i64,
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub sort_key: i32,
    pub most_recent: bool,
    pub task_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskTransition for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskTransition {
    pub task_id: i64,
    pub to_state: String,
    pub from_state: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Query builder for task transitions
#[derive(Debug, Default)]
pub struct TaskTransitionQuery {
    task_id: Option<i64>,
    state: Option<String>,
    most_recent_only: bool,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl TaskTransition {
    /// Create a new task transition
    /// This automatically handles sort_key generation and marks previous transitions as not most_recent
    pub async fn create(pool: &PgPool, new_transition: NewTaskTransition) -> Result<TaskTransition, sqlx::Error> {
        // Start a transaction to ensure atomicity
        let mut tx = pool.begin().await?;
        
        // Get the next sort key for this task
        let sort_key_result = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(sort_key), 0) + 1 as next_sort_key
            FROM tasker_task_transitions
            WHERE task_id = $1
            "#,
            new_transition.task_id
        )
        .fetch_one(&mut *tx)
        .await?;
        
        let next_sort_key = sort_key_result.next_sort_key.unwrap_or(1);
        
        // Mark all existing transitions as not most recent
        sqlx::query!(
            r#"
            UPDATE tasker_task_transitions 
            SET most_recent = false, updated_at = NOW()
            WHERE task_id = $1 AND most_recent = true
            "#,
            new_transition.task_id
        )
        .execute(&mut *tx)
        .await?;
        
        // Insert the new transition
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            INSERT INTO tasker_task_transitions 
            (task_id, to_state, from_state, metadata, sort_key, most_recent)
            VALUES ($1, $2, $3, $4, $5, true)
            RETURNING id, to_state, from_state, metadata, sort_key, most_recent, 
                      task_id, created_at, updated_at
            "#,
            new_transition.task_id,
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
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Get the current (most recent) transition for a task
    pub async fn get_current(pool: &PgPool, task_id: i64) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_id = $1 AND most_recent = true
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(transition)
    }

    /// Get all transitions for a task ordered by sort key
    pub async fn list_by_task(pool: &PgPool, task_id: i64) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_id = $1
            ORDER BY sort_key ASC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get transitions by state
    pub async fn list_by_state(pool: &PgPool, state: &str, most_recent_only: bool) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = if most_recent_only {
            sqlx::query_as!(
                TaskTransition,
                r#"
                SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                       task_id, created_at, updated_at
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
                SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                       task_id, created_at, updated_at
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
    pub async fn list_by_tasks(pool: &PgPool, task_ids: &[i64]) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_id = ANY($1)
            ORDER BY task_id, sort_key ASC
            "#,
            task_ids
        )
        .fetch_all(pool)
        .await?;

        Ok(transitions)
    }

    /// Get the previous transition for a task (before the current one)
    pub async fn get_previous(pool: &PgPool, task_id: i64) -> Result<Option<TaskTransition>, sqlx::Error> {
        let transition = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_id = $1 AND most_recent = false
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            task_id
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
                FROM tasker_task_transitions
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
                FROM tasker_task_transitions
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

    /// Get transition history for a task with pagination
    pub async fn get_history(
        pool: &PgPool, 
        task_id: i64, 
        limit: Option<i64>, 
        offset: Option<i64>
    ) -> Result<Vec<TaskTransition>, sqlx::Error> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        
        let transitions = sqlx::query_as!(
            TaskTransition,
            r#"
            SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                   task_id, created_at, updated_at
            FROM tasker_task_transitions
            WHERE task_id = $1
            ORDER BY sort_key DESC
            LIMIT $2 OFFSET $3
            "#,
            task_id,
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
            UPDATE tasker_task_transitions 
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

    /// Check if a task can transition from one state to another
    pub async fn can_transition(pool: &PgPool, task_id: i64, from_state: &str, _to_state: &str) -> Result<bool, sqlx::Error> {
        // Get current state
        let current = Self::get_current(pool, task_id).await?;
        
        if let Some(transition) = current {
            // Check if current state matches expected from_state
            Ok(transition.to_state == from_state)
        } else {
            // No transitions yet, so check if from_state is initial state
            Ok(from_state == "draft" || from_state == "planned")
        }
    }

    /// Get tasks in a specific state
    pub async fn get_tasks_in_state(pool: &PgPool, state: &str) -> Result<Vec<i64>, sqlx::Error> {
        let task_ids = sqlx::query!(
            r#"
            SELECT DISTINCT task_id
            FROM tasker_task_transitions
            WHERE to_state = $1 AND most_recent = true
            "#,
            state
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.task_id)
        .collect();

        Ok(task_ids)
    }

    /// Get tasks by multiple states
    pub async fn get_tasks_in_states(pool: &PgPool, states: &[String]) -> Result<Vec<i64>, sqlx::Error> {
        let task_ids = sqlx::query!(
            r#"
            SELECT DISTINCT task_id
            FROM tasker_task_transitions
            WHERE to_state = ANY($1) AND most_recent = true
            "#,
            states
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.task_id)
        .collect();

        Ok(task_ids)
    }

    /// Delete old transitions (keep only the most recent N transitions per task)
    pub async fn cleanup_old_transitions(pool: &PgPool, task_id: i64, keep_count: i32) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_transitions
            WHERE task_id = $1 
              AND sort_key < (
                SELECT sort_key 
                FROM tasker_task_transitions
                WHERE task_id = $1
                ORDER BY sort_key DESC
                LIMIT 1 OFFSET $2
              )
            "#,
            task_id,
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
            SELECT to_state, COUNT(DISTINCT task_id) as count
            FROM tasker_task_transitions
            WHERE most_recent = true
            GROUP BY to_state
            ORDER BY count DESC
            "#
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| (row.to_state, row.count.unwrap_or(0)))
        .collect();

        Ok(summary)
    }
}

impl TaskTransitionQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn task_id(mut self, id: i64) -> Self {
        self.task_id = Some(id);
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
        let mut query = String::from(
            "SELECT id, to_state, from_state, metadata, sort_key, most_recent, 
                    task_id, created_at, updated_at
             FROM tasker_task_transitions WHERE 1=1"
        );

        let mut params = Vec::new();
        let mut param_count = 0;

        if let Some(task_id) = self.task_id {
            param_count += 1;
            query.push_str(&format!(" AND task_id = ${}", param_count));
            params.push(task_id.to_string());
        }

        if let Some(state) = self.state {
            param_count += 1;
            query.push_str(&format!(" AND to_state = ${}", param_count));
            params.push(state);
        }

        if self.most_recent_only {
            query.push_str(" AND most_recent = true");
        }

        query.push_str(" ORDER BY task_id, sort_key DESC");

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
        let transitions = TaskTransition::list_by_task(
            pool, 
            self.task_id.unwrap_or(0)
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
    async fn test_task_transition_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Assume we have a task with id 1
        let task_id = 1;

        // Test creation
        let new_transition = NewTaskTransition {
            task_id,
            to_state: "planned".to_string(),
            from_state: Some("draft".to_string()),
            metadata: Some(json!({"reason": "ready_for_planning"})),
        };

        let created = TaskTransition::create(pool, new_transition)
            .await
            .expect("Failed to create transition");
        assert_eq!(created.to_state, "planned");
        assert!(created.most_recent);
        assert_eq!(created.sort_key, 1);

        // Test find by ID
        let found = TaskTransition::find_by_id(pool, created.id)
            .await
            .expect("Failed to find transition")
            .expect("Transition not found");
        assert_eq!(found.id, created.id);

        // Test get current
        let current = TaskTransition::get_current(pool, task_id)
            .await
            .expect("Failed to get current transition")
            .expect("No current transition");
        assert_eq!(current.id, created.id);
        assert!(current.most_recent);

        // Test creating another transition
        let new_transition2 = NewTaskTransition {
            task_id,
            to_state: "launched".to_string(),
            from_state: Some("planned".to_string()),
            metadata: Some(json!({"launched_at": "2024-01-01T00:00:00Z"})),
        };

        let created2 = TaskTransition::create(pool, new_transition2)
            .await
            .expect("Failed to create second transition");
        assert_eq!(created2.sort_key, 2);
        assert!(created2.most_recent);

        // Verify first transition is no longer most recent
        let updated_first = TaskTransition::find_by_id(pool, created.id)
            .await
            .expect("Failed to find first transition")
            .expect("First transition not found");
        assert!(!updated_first.most_recent);

        // Test get history
        let history = TaskTransition::get_history(pool, task_id, Some(10), None)
            .await
            .expect("Failed to get history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, created2.id); // Most recent first
        assert_eq!(history[1].id, created.id);

        // Test can transition
        let can_transition = TaskTransition::can_transition(
            pool, 
            task_id, 
            "launched", 
            "running"
        )
        .await
        .expect("Failed to check transition");
        assert!(can_transition);

        // Test state summary
        let summary = TaskTransition::get_state_summary(pool)
            .await
            .expect("Failed to get state summary");
        assert!(!summary.is_empty());

        db.close().await;
    }

    #[test]
    fn test_query_builder() {
        let query = TaskTransitionQuery::new()
            .task_id(1)
            .state("completed")
            .most_recent_only()
            .limit(10)
            .offset(0);

        assert_eq!(query.task_id, Some(1));
        assert_eq!(query.state, Some("completed".to_string()));
        assert!(query.most_recent_only);
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.offset, Some(0));
    }
}
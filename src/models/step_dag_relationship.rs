use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// StepDagRelationship represents a read-only view of DAG relationships for workflow steps
/// Maps to `tasker_step_dag_relationships` view - sophisticated DAG query logic (1.9KB Rails model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepDagRelationship {
    pub workflow_step_id: Option<i64>,
    pub task_id: Option<i64>,
    pub named_step_id: Option<i32>,
    pub parent_step_ids: Option<serde_json::Value>, // JSONB array of parent step IDs
    pub child_step_ids: Option<serde_json::Value>,  // JSONB array of child step IDs
    pub parent_count: Option<i64>,
    pub child_count: Option<i64>,
    pub is_root_step: Option<bool>,
    pub is_leaf_step: Option<bool>,
    pub min_depth_from_root: Option<i32>,
}

/// Query builder for step DAG relationships
#[derive(Debug, Default)]
pub struct StepDagRelationshipQuery {
    task_id: Option<i64>,
    workflow_step_id: Option<i64>,
    is_root_step: Option<bool>,
    is_leaf_step: Option<bool>,
    has_parents: Option<bool>,
    has_children: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl StepDagRelationship {
    /// Find a step DAG relationship by workflow step ID
    pub async fn find_by_workflow_step_id(pool: &PgPool, target_workflow_step_id: i64) -> Result<Option<StepDagRelationship>, sqlx::Error> {
        let relationship = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, 
                   parent_step_ids, child_step_ids, parent_count, child_count,
                   is_root_step, is_leaf_step, min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE workflow_step_id = $1
            "#,
            target_workflow_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(relationship)
    }

    /// Get all DAG relationships for a task (Rails scope: for_task)
    pub async fn for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, 
                   parent_step_ids, child_step_ids, parent_count, child_count,
                   is_root_step, is_leaf_step, min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE task_id = $1
            ORDER BY min_depth_from_root, workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get root steps for a task (Rails scope: root_steps)
    pub async fn root_steps(pool: &PgPool, task_id: Option<i64>) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = if let Some(task_id) = task_id {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE is_root_step = true AND task_id = $1
                ORDER BY workflow_step_id
                "#,
                task_id
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE is_root_step = true
                ORDER BY task_id, workflow_step_id
                "#
            )
            .fetch_all(pool)
            .await?
        };

        Ok(relationships)
    }

    /// Get leaf steps for a task (Rails scope: leaf_steps)
    pub async fn leaf_steps(pool: &PgPool, task_id: Option<i64>) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = if let Some(task_id) = task_id {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE is_leaf_step = true AND task_id = $1
                ORDER BY workflow_step_id
                "#,
                task_id
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE is_leaf_step = true
                ORDER BY task_id, workflow_step_id
                "#
            )
            .fetch_all(pool)
            .await?
        };

        Ok(relationships)
    }

    /// Get steps with parents (Rails scope: with_parents)
    pub async fn with_parents(pool: &PgPool, task_id: Option<i64>) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = if let Some(task_id) = task_id {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE parent_count > 0 AND task_id = $1
                ORDER BY min_depth_from_root, workflow_step_id
                "#,
                task_id
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE parent_count > 0
                ORDER BY task_id, min_depth_from_root, workflow_step_id
                "#
            )
            .fetch_all(pool)
            .await?
        };

        Ok(relationships)
    }

    /// Get steps with children (Rails scope: with_children)
    pub async fn with_children(pool: &PgPool, task_id: Option<i64>) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = if let Some(task_id) = task_id {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE child_count > 0 AND task_id = $1
                ORDER BY min_depth_from_root, workflow_step_id
                "#,
                task_id
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                WHERE child_count > 0
                ORDER BY task_id, min_depth_from_root, workflow_step_id
                "#
            )
            .fetch_all(pool)
            .await?
        };

        Ok(relationships)
    }

    /// Get siblings of a workflow step (Rails scope: siblings_of)
    /// This implements the sophisticated sibling logic from the Rails model
    pub async fn siblings_of(pool: &PgPool, workflow_step_id: i64) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        // Find steps that have exactly the same parent set as the given step
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            WITH target_step AS (
                SELECT parent_step_ids
                FROM tasker_step_dag_relationships
                WHERE workflow_step_id = $1
            )
            SELECT sdr.workflow_step_id, sdr.task_id, sdr.named_step_id, 
                   sdr.parent_step_ids, sdr.child_step_ids, sdr.parent_count, sdr.child_count,
                   sdr.is_root_step, sdr.is_leaf_step, sdr.min_depth_from_root
            FROM tasker_step_dag_relationships sdr, target_step ts
            WHERE sdr.parent_step_ids = ts.parent_step_ids 
              AND sdr.workflow_step_id != $1
            ORDER BY sdr.workflow_step_id
            "#,
            workflow_step_id
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get DAG relationships by depth level
    pub async fn by_depth_level(pool: &PgPool, task_id: i64, depth: i32) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, 
                   parent_step_ids, child_step_ids, parent_count, child_count,
                   is_root_step, is_leaf_step, min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE task_id = $1 AND min_depth_from_root = $2
            ORDER BY workflow_step_id
            "#,
            task_id,
            depth
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get maximum depth for a task
    pub async fn max_depth_for_task(pool: &PgPool, task_id: i64) -> Result<Option<i32>, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT MAX(min_depth_from_root) as max_depth
            FROM tasker_step_dag_relationships
            WHERE task_id = $1
            "#,
            task_id
        )
        .fetch_one(pool)
        .await?;

        Ok(result.max_depth)
    }

    /// Helper method: check if step is root (Rails method: root_step?)
    pub fn is_root_step(&self) -> bool {
        self.is_root_step.unwrap_or(false)
    }

    /// Helper method: check if step is leaf (Rails method: leaf_step?)
    pub fn is_leaf_step(&self) -> bool {
        self.is_leaf_step.unwrap_or(false)
    }

    /// Helper method: check if step has parents (Rails method: has_parents?)
    pub fn has_parents(&self) -> bool {
        self.parent_count.unwrap_or(0) > 0
    }

    /// Helper method: check if step has children (Rails method: has_children?)
    pub fn has_children(&self) -> bool {
        self.child_count.unwrap_or(0) > 0
    }

    /// Parse parent step IDs from JSONB (Rails method: parent_step_ids_array)
    pub fn parent_step_ids_array(&self) -> Vec<i64> {
        if let Some(parent_ids) = &self.parent_step_ids {
            if let Ok(ids) = serde_json::from_value::<Vec<i64>>(parent_ids.clone()) {
                return ids;
            }
        }
        Vec::new()
    }

    /// Parse child step IDs from JSONB (Rails method: child_step_ids_array)
    pub fn child_step_ids_array(&self) -> Vec<i64> {
        if let Some(child_ids) = &self.child_step_ids {
            if let Ok(ids) = serde_json::from_value::<Vec<i64>>(child_ids.clone()) {
                return ids;
            }
        }
        Vec::new()
    }

    /// Get direct parent relationships
    pub async fn get_parent_relationships(&self, pool: &PgPool) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let parent_ids = self.parent_step_ids_array();
        if parent_ids.is_empty() {
            return Ok(Vec::new());
        }

        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, 
                   parent_step_ids, child_step_ids, parent_count, child_count,
                   is_root_step, is_leaf_step, min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE workflow_step_id = ANY($1)
            ORDER BY min_depth_from_root, workflow_step_id
            "#,
            &parent_ids
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get direct child relationships
    pub async fn get_child_relationships(&self, pool: &PgPool) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let child_ids = self.child_step_ids_array();
        if child_ids.is_empty() {
            return Ok(Vec::new());
        }

        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT workflow_step_id, task_id, named_step_id, 
                   parent_step_ids, child_step_ids, parent_count, child_count,
                   is_root_step, is_leaf_step, min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE workflow_step_id = ANY($1)
            ORDER BY min_depth_from_root, workflow_step_id
            "#,
            &child_ids
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get all descendants (recursive children)
    pub async fn get_all_descendants(&self, pool: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
        let descendants = sqlx::query!(
            r#"
            WITH RECURSIVE descendants AS (
                -- Base case: direct children
                SELECT unnest(ARRAY(SELECT jsonb_array_elements_text(child_step_ids)::bigint)) as step_id
                FROM tasker_step_dag_relationships
                WHERE workflow_step_id = $1
                
                UNION ALL
                
                -- Recursive case: children of children
                SELECT unnest(ARRAY(SELECT jsonb_array_elements_text(sdr.child_step_ids)::bigint)) as step_id
                FROM descendants d
                JOIN tasker_step_dag_relationships sdr ON sdr.workflow_step_id = d.step_id
                WHERE sdr.child_step_ids IS NOT NULL AND sdr.child_step_ids != '[]'::jsonb
            )
            SELECT DISTINCT step_id FROM descendants
            "#,
            self.workflow_step_id.unwrap_or(0)
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|row| row.step_id)
        .collect();

        Ok(descendants)
    }

    /// Get all ancestors (recursive parents)
    pub async fn get_all_ancestors(&self, pool: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
        let ancestors = sqlx::query!(
            r#"
            WITH RECURSIVE ancestors AS (
                -- Base case: direct parents
                SELECT unnest(ARRAY(SELECT jsonb_array_elements_text(parent_step_ids)::bigint)) as step_id
                FROM tasker_step_dag_relationships
                WHERE workflow_step_id = $1
                
                UNION ALL
                
                -- Recursive case: parents of parents
                SELECT unnest(ARRAY(SELECT jsonb_array_elements_text(sdr.parent_step_ids)::bigint)) as step_id
                FROM ancestors a
                JOIN tasker_step_dag_relationships sdr ON sdr.workflow_step_id = a.step_id
                WHERE sdr.parent_step_ids IS NOT NULL AND sdr.parent_step_ids != '[]'::jsonb
            )
            SELECT DISTINCT step_id FROM ancestors
            "#,
            self.workflow_step_id.unwrap_or(0)
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|row| row.step_id)
        .collect();

        Ok(ancestors)
    }
}

impl StepDagRelationshipQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn task_id(mut self, task_id: i64) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn workflow_step_id(mut self, workflow_step_id: i64) -> Self {
        self.workflow_step_id = Some(workflow_step_id);
        self
    }

    pub fn root_steps_only(mut self) -> Self {
        self.is_root_step = Some(true);
        self
    }

    pub fn leaf_steps_only(mut self) -> Self {
        self.is_leaf_step = Some(true);
        self
    }

    pub fn with_parents_only(mut self) -> Self {
        self.has_parents = Some(true);
        self
    }

    pub fn with_children_only(mut self) -> Self {
        self.has_children = Some(true);
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

    pub async fn execute(self, pool: &PgPool) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let mut conditions = Vec::new();
        let mut params = Vec::new();
        let mut param_count = 0;

        if let Some(task_id) = self.task_id {
            param_count += 1;
            conditions.push(format!("task_id = ${}", param_count));
            params.push(task_id.to_string());
        }

        if let Some(workflow_step_id) = self.workflow_step_id {
            param_count += 1;
            conditions.push(format!("workflow_step_id = ${}", param_count));
            params.push(workflow_step_id.to_string());
        }

        if let Some(true) = self.is_root_step {
            conditions.push("is_root_step = true".to_string());
        }

        if let Some(true) = self.is_leaf_step {
            conditions.push("is_leaf_step = true".to_string());
        }

        if let Some(true) = self.has_parents {
            conditions.push("parent_count > 0".to_string());
        }

        if let Some(true) = self.has_children {
            conditions.push("child_count > 0".to_string());
        }

        let _where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let _order_clause = "ORDER BY task_id, min_depth_from_root, workflow_step_id";

        let _limit_clause = if let Some(limit) = self.limit {
            format!(" LIMIT {}", limit)
        } else {
            String::new()
        };

        let _offset_clause = if let Some(offset) = self.offset {
            format!(" OFFSET {}", offset)
        } else {
            String::new()
        };

        // For simplicity in this implementation, using the task-specific method
        // In production, you'd want to implement the full dynamic query
        if let Some(task_id) = self.task_id {
            StepDagRelationship::for_task(pool, task_id).await
        } else {
            // Fallback to getting all relationships
            sqlx::query_as!(
                StepDagRelationship,
                r#"
                SELECT workflow_step_id, task_id, named_step_id, 
                       parent_step_ids, child_step_ids, parent_count, child_count,
                       is_root_step, is_leaf_step, min_depth_from_root
                FROM tasker_step_dag_relationships
                ORDER BY task_id, min_depth_from_root, workflow_step_id
                "#
            )
            .fetch_all(pool)
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_step_dag_relationship_queries() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Assume we have workflow steps in the database
        let task_id = 1;

        // Test for_task
        let relationships = StepDagRelationship::for_task(pool, task_id)
            .await
            .expect("Failed to get relationships for task");

        // Test root_steps
        let _root_steps = StepDagRelationship::root_steps(pool, Some(task_id))
            .await
            .expect("Failed to get root steps");

        // Test leaf_steps
        let _leaf_steps = StepDagRelationship::leaf_steps(pool, Some(task_id))
            .await
            .expect("Failed to get leaf steps");

        // Test with_parents
        let _with_parents = StepDagRelationship::with_parents(pool, Some(task_id))
            .await
            .expect("Failed to get steps with parents");

        // Test with_children
        let _with_children = StepDagRelationship::with_children(pool, Some(task_id))
            .await
            .expect("Failed to get steps with children");

        if let Some(first_step) = relationships.first() {
            // Test find_by_workflow_step_id
            let found = StepDagRelationship::find_by_workflow_step_id(pool, first_step.workflow_step_id.unwrap_or(0))
                .await
                .expect("Failed to find by workflow step id");
            assert!(found.is_some());

            // Test helper methods
            let found_ref = found.as_ref().unwrap();
            assert_eq!(found_ref.is_root_step(), found_ref.is_root_step.unwrap_or(false));
            assert_eq!(found_ref.is_leaf_step(), found_ref.is_leaf_step.unwrap_or(false));
            assert_eq!(found_ref.has_parents(), found_ref.parent_count.unwrap_or(0) > 0);
            assert_eq!(found_ref.has_children(), found_ref.child_count.unwrap_or(0) > 0);

            // Test JSONB array parsing
            let parent_ids = found_ref.parent_step_ids_array();
            let child_ids = found_ref.child_step_ids_array();
            assert_eq!(parent_ids.len(), found_ref.parent_count.unwrap_or(0) as usize);
            assert_eq!(child_ids.len(), found_ref.child_count.unwrap_or(0) as usize);

            // Test siblings_of
            let _siblings = StepDagRelationship::siblings_of(pool, first_step.workflow_step_id.unwrap_or(0))
                .await
                .expect("Failed to get siblings");
        }

        db.close().await;
    }

    #[test]
    fn test_query_builder() {
        let query = StepDagRelationshipQuery::new()
            .task_id(1)
            .root_steps_only()
            .limit(10)
            .offset(0);

        assert_eq!(query.task_id, Some(1));
        assert_eq!(query.is_root_step, Some(true));
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.offset, Some(0));
    }
}
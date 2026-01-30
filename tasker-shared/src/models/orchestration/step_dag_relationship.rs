//! # Step DAG Relationship Analysis
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL VIEW.
//!
//! ## Overview
//!
//! The `StepDagRelationship` represents dynamically computed DAG (Directed Acyclic Graph)
//! relationship analysis for workflow steps. This data is **never stored** - it's calculated
//! on-demand using a sophisticated SQL VIEW that analyzes step dependencies and hierarchy.
//!
//! ## Human-Readable Explanation
//!
//! Think of this as a "relationship map" for workflow steps. In a complex workflow with multiple
//! steps that depend on each other, this model helps answer questions like:
//!
//! - **"Which steps can run first?"** (Root steps with no parents)
//! - **"Which steps are waiting for others?"** (Steps with unsatisfied dependencies)
//! - **"Which steps finish the workflow?"** (Leaf steps with no children)
//! - **"How deep is this step in the workflow?"** (Depth from root steps)
//! - **"What would happen if this step fails?"** (Impact on child steps)
//!
//! ### Real-World Example
//!
//! Imagine a software deployment workflow:
//! ```text
//! [Build Code] → [Run Tests] → [Deploy to Staging] → [Deploy to Prod]
//!                     ↓              ↓
//!              [Security Scan] → [Approve Deploy]
//! ```
//!
//! This model would show:
//! - **Root step**: "Build Code" (can start immediately)
//! - **Leaf steps**: "Deploy to Prod" (final step)
//! - **Dependencies**: "Deploy to Staging" depends on "Run Tests" AND "Security Scan"
//! - **Depth levels**: Build(0) → Tests(1) → Staging(2) → Prod(3)
//!
//! ## SQL VIEW Integration
//!
//! This module integrates with the PostgreSQL VIEW:
//!
//! ### `tasker.step_dag_relationships`
//! - Computes DAG relationships and hierarchy for workflow steps
//! - Provides parent/child step relationships with JSONB arrays
//! - Calculates step depth and identifies root/leaf steps
//! - Uses recursive CTEs for efficient depth calculation
//!
//! ## VIEW Return Schema
//!
//! The VIEW returns:
//! ```sql
//! SELECT
//!   workflow_step_uuid bigint,
//!   task_uuid bigint,
//!   named_step_uuid integer,
//!   parent_step_uuids jsonb,     -- Array of parent workflow_step_uuids
//!   child_step_uuids jsonb,      -- Array of child workflow_step_uuids
//!   parent_count bigint,       -- Count of parent steps
//!   child_count bigint,        -- Count of child steps
//!   is_root_step boolean,      -- True if no parents (entry point)
//!   is_leaf_step boolean,      -- True if no children (exit point)
//!   min_depth_from_root integer -- Minimum depth from root steps
//! ```
//!
//! ## Performance Characteristics
//!
//! - **No Storage Overhead**: Computed view with no table maintenance
//! - **Always Current**: Real-time calculation ensures data is never stale
//! - **Efficient Computation**: Leverages indexes on workflow_step_edges
//! - **Recursive Analysis**: Uses CTEs for depth calculation with cycle protection

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{types::Uuid, FromRow, PgPool};

/// Represents computed DAG relationship analysis for workflow steps.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of querying
/// the `tasker.step_dag_relationships` SQL VIEW.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Step dependencies from `tasker.workflow_step_edges`
/// - Parent/child relationships and counts
/// - Step hierarchy depth using recursive traversal
/// - Root/leaf identification for workflow entry/exit points
///
/// # No CRUD Operations
///
/// Unlike other models, this struct does NOT support:
/// - `create()` - Cannot insert computed data
/// - `update()` - Cannot modify computed data
/// - `delete()` - Cannot delete computed data
///
/// Only read operations are available by querying the VIEW.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepDagRelationship {
    pub workflow_step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub named_step_uuid: Uuid,
    pub parent_step_uuids: JsonValue, // JSONB array of parent workflow_step_uuids
    pub child_step_uuids: JsonValue,  // JSONB array of child workflow_step_uuids
    pub parent_count: i64,
    pub child_count: i64,
    pub is_root_step: bool,               // No parents - workflow entry point
    pub is_leaf_step: bool,               // No children - workflow exit point
    pub min_depth_from_root: Option<i32>, // Minimum depth from root (can be null for orphaned steps)
}

/// Query parameters for DAG relationship analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDagRelationshipQuery {
    pub task_uuid: Option<Uuid>,
    pub workflow_step_uuid: Option<Uuid>,
    pub min_depth: Option<i32>,
    pub max_depth: Option<i32>,
    pub is_root_step: Option<bool>,
    pub is_leaf_step: Option<bool>,
}

impl StepDagRelationship {
    /// Get DAG relationships for all steps.
    pub async fn get_all(pool: &PgPool) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
                parent_step_uuids as "parent_step_uuids!: JsonValue",
                child_step_uuids as "child_step_uuids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker.step_dag_relationships
            ORDER BY task_uuid, min_depth_from_root NULLS LAST, workflow_step_uuid
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get DAG relationships for a specific task.
    pub async fn get_for_task(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
                parent_step_uuids as "parent_step_uuids!: JsonValue",
                child_step_uuids as "child_step_uuids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker.step_dag_relationships
            WHERE task_uuid = $1::uuid
            ORDER BY min_depth_from_root NULLS LAST, workflow_step_uuid
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get DAG relationship for a specific workflow step.
    pub async fn get_for_step(
        pool: &PgPool,
        workflow_step_uuid: Uuid,
    ) -> Result<Option<StepDagRelationship>, sqlx::Error> {
        let relationship = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
                parent_step_uuids as "parent_step_uuids!: JsonValue",
                child_step_uuids as "child_step_uuids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker.step_dag_relationships
            WHERE workflow_step_uuid = $1::uuid
            "#,
            workflow_step_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(relationship)
    }

    /// Get all root steps (entry points) for a task.
    pub async fn get_root_steps(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
                parent_step_uuids as "parent_step_uuids!: JsonValue",
                child_step_uuids as "child_step_uuids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker.step_dag_relationships
            WHERE task_uuid = $1::uuid AND is_root_step = true
            ORDER BY workflow_step_uuid
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get all leaf steps (exit points) for a task.
    pub async fn get_leaf_steps(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
                parent_step_uuids as "parent_step_uuids!: JsonValue",
                child_step_uuids as "child_step_uuids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker.step_dag_relationships
            WHERE task_uuid = $1::uuid AND is_leaf_step = true
            ORDER BY workflow_step_uuid
            "#,
            task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get steps at a specific depth level.
    pub async fn get_steps_at_depth(
        pool: &PgPool,
        task_uuid: Uuid,
        depth: i32,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_uuid as "workflow_step_uuid!: Uuid",
                task_uuid as "task_uuid!: Uuid",
                named_step_uuid as "named_step_uuid!: Uuid",
                parent_step_uuids as "parent_step_uuids!: JsonValue",
                child_step_uuids as "child_step_uuids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker.step_dag_relationships
            WHERE task_uuid = $1::uuid AND min_depth_from_root = $2
            ORDER BY workflow_step_uuid
            "#,
            task_uuid,
            depth
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get parent step IDs as a `Vec<i64>`.
    ///
    /// Returns the workflow step IDs that this step depends on.
    /// These steps must complete successfully before this step can run.
    ///
    /// # Example
    /// ```rust
    /// use tasker_shared::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    /// use uuid::Uuid;
    ///
    /// let parent_uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let parent_uuid2 = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap();
    /// let parent_step_uuids = vec![parent_uuid1.to_string(), parent_uuid2.to_string()];
    ///
    /// let deploy_step = StepDagRelationship {
    ///     workflow_step_uuid: Uuid::new_v4(),
    ///     task_uuid: Uuid::new_v4(),
    ///     named_step_uuid: Uuid::new_v4(),
    ///     parent_step_uuids: json!(parent_step_uuids), // Build and Test step IDs
    ///     child_step_uuids: json!([]),
    ///     parent_count: 2,
    ///     child_count: 0,
    ///     is_root_step: false,
    ///     is_leaf_step: true,
    ///     min_depth_from_root: Some(2),
    /// };
    ///
    /// let parents = deploy_step.parent_ids();
    /// assert_eq!(parents, vec![parent_uuid1, parent_uuid2]);
    /// println!("Deploy step waits for steps: {:?}", parents);
    /// ```
    pub fn parent_ids(&self) -> Vec<Uuid> {
        self.parent_step_uuids
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| Uuid::parse_str(s).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get child step IDs as a `Vec<i64>`.
    ///
    /// Returns the workflow step IDs that depend on this step.
    /// These steps will be blocked until this step completes successfully.
    ///
    /// # Example
    /// ```rust
    /// use tasker_shared::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    /// use uuid::Uuid;
    ///
    /// let child_uuid1 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let child_uuid2 = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440004").unwrap();
    /// let child_step_uuids = vec![child_uuid1.to_string(), child_uuid2.to_string()];
    ///
    /// let build_step = StepDagRelationship {
    ///     workflow_step_uuid: Uuid::new_v4(),
    ///     task_uuid: Uuid::new_v4(),
    ///     named_step_uuid: Uuid::new_v4(),
    ///     parent_step_uuids: json!([]), // No dependencies - can start immediately
    ///     child_step_uuids: json!(child_step_uuids), // Test and Package step IDs
    ///     parent_count: 0,
    ///     child_count: 2,
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(0),
    /// };
    ///
    /// let children = build_step.child_ids();
    /// assert_eq!(children, vec![child_uuid1, child_uuid2]);
    /// println!("Build completion will unblock: {:?}", children);
    /// ```
    pub fn child_ids(&self) -> Vec<Uuid> {
        self.child_step_uuids
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| Uuid::parse_str(s).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if this step has no dependencies (can execute immediately).
    ///
    /// Root steps are workflow entry points that can start as soon as the task begins.
    ///
    /// # Example
    /// ```rust
    /// use tasker_shared::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    /// use uuid::Uuid;
    ///
    /// let workflow_uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let workflow_uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();
    /// let workflow_uuid3 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let task_uuid = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440004").unwrap();
    /// let named_uuid1 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440005").unwrap();
    /// let named_uuid2 = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440006").unwrap();
    ///
    /// let root_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid1,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid1,
    ///     parent_step_uuids: json!([]), // No dependencies
    ///     child_step_uuids: json!([workflow_uuid2.to_string(), workflow_uuid3.to_string()]),
    ///     parent_count: 0,
    ///     child_count: 2,
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(0),
    /// };
    ///
    /// let dependent_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid2,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid2,
    ///     parent_step_uuids: json!([workflow_uuid1.to_string()]), // Depends on step 1
    ///     child_step_uuids: json!([]),
    ///     parent_count: 1,
    ///     child_count: 0,
    ///     is_root_step: false,
    ///     is_leaf_step: true,
    ///     min_depth_from_root: Some(1),
    /// };
    ///
    /// assert!(root_step.can_execute_immediately());
    /// assert!(!dependent_step.can_execute_immediately());
    ///
    /// if root_step.can_execute_immediately() {
    ///     println!("This step can start right away!");
    /// }
    /// ```
    pub fn can_execute_immediately(&self) -> bool {
        self.is_root_step
    }

    /// Check if this step is a workflow exit point.
    ///
    /// Leaf steps are the final steps in a workflow. When all leaf steps complete,
    /// the entire task is considered finished.
    ///
    /// # Example
    /// ```rust
    /// use tasker_shared::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    /// use uuid::Uuid;
    ///
    /// let workflow_uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let workflow_uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();
    /// let workflow_uuid3 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let task_uuid = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440004").unwrap();
    /// let named_uuid2 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440005").unwrap();
    /// let named_uuid3 = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440006").unwrap();
    ///
    /// let final_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid3,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid3,
    ///     parent_step_uuids: json!([workflow_uuid1.to_string(), workflow_uuid2.to_string()]),
    ///     child_step_uuids: json!([]), // No children - this is a leaf step
    ///     parent_count: 2,
    ///     child_count: 0,
    ///     is_root_step: false,
    ///     is_leaf_step: true,
    ///     min_depth_from_root: Some(2),
    /// };
    ///
    /// let middle_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid2,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid2,
    ///     parent_step_uuids: json!([workflow_uuid1.to_string()]),
    ///     child_step_uuids: json!([workflow_uuid3.to_string()]), // Has children - not a leaf
    ///     parent_count: 1,
    ///     child_count: 1,
    ///     is_root_step: false,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(1),
    /// };
    ///
    /// // Note: This depends on is_leaf_step field being set correctly
    /// // For this example, we'll show the concept with assertions
    /// println!("Step 3 is a workflow exit: {}", final_step.child_ids().is_empty());
    /// println!("Step 2 is not a workflow exit: {}", !middle_step.child_ids().is_empty());
    /// ```
    pub fn is_workflow_exit(&self) -> bool {
        self.is_leaf_step
    }

    /// Get the execution level (depth from root).
    ///
    /// Returns the minimum number of steps from any root step to this step.
    /// This helps determine execution order and identify the "critical path".
    ///
    /// # Example
    /// ```rust
    /// use tasker_shared::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    /// use uuid::Uuid;
    ///
    /// let workflow_uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let workflow_uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();
    /// let workflow_uuid3 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let workflow_uuid4 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440004").unwrap();
    /// let workflow_uuid99 = Uuid::parse_str("990e8400-e29b-41d4-a716-446655440099").unwrap();
    /// let task_uuid = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440005").unwrap();
    /// let named_uuid1 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440006").unwrap();
    /// let named_uuid2 = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440007").unwrap();
    /// let named_uuid99 = Uuid::parse_str("aa0e8400-e29b-41d4-a716-446655440099").unwrap();
    ///
    /// let root_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid1,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid1,
    ///     parent_step_uuids: json!([]),
    ///     child_step_uuids: json!([workflow_uuid2.to_string(), workflow_uuid3.to_string()]),
    ///     parent_count: 0,
    ///     child_count: 2,
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(0),
    /// };
    ///
    /// let second_level_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid2,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid2,
    ///     parent_step_uuids: json!([workflow_uuid1.to_string()]),
    ///     child_step_uuids: json!([workflow_uuid4.to_string()]),
    ///     parent_count: 1,
    ///     child_count: 1,
    ///     is_root_step: false,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(1),
    /// };
    ///
    /// let orphaned_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid99,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid99,
    ///     parent_step_uuids: json!([]),
    ///     child_step_uuids: json!([]),
    ///     parent_count: 0,
    ///     child_count: 0,
    ///     min_depth_from_root: None, // No depth calculated
    ///     is_root_step: false,
    ///     is_leaf_step: false,
    /// };
    ///
    /// match root_step.execution_level() {
    ///     Some(0) => println!("This is a root step (starts immediately)"),
    ///     Some(depth) => println!("This step runs at depth level {}", depth),
    ///     None => println!("This step may be orphaned or in a cycle"),
    /// }
    ///
    /// assert_eq!(root_step.execution_level(), Some(0));
    /// assert_eq!(second_level_step.execution_level(), Some(1));
    /// assert_eq!(orphaned_step.execution_level(), None);
    /// ```
    pub fn execution_level(&self) -> Option<i32> {
        self.min_depth_from_root
    }

    /// Check if this step is orphaned (no depth calculated - potential cycle).
    ///
    /// Orphaned steps may indicate circular dependencies or disconnected workflow fragments.
    /// These require special attention as they may never execute.
    ///
    /// # Example
    /// ```rust
    /// use tasker_shared::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    /// use uuid::Uuid;
    /// use tracing::warn;
    ///
    /// let workflow_uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let workflow_uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();
    /// let workflow_uuid99 = Uuid::parse_str("990e8400-e29b-41d4-a716-446655440099").unwrap();
    /// let task_uuid = Uuid::parse_str("660e8400-e29b-41d4-a716-446655440003").unwrap();
    /// let named_uuid1 = Uuid::parse_str("770e8400-e29b-41d4-a716-446655440004").unwrap();
    /// let named_uuid99 = Uuid::parse_str("aa0e8400-e29b-41d4-a716-446655440099").unwrap();
    ///
    /// let normal_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid1,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid1,
    ///     parent_step_uuids: json!([]),
    ///     child_step_uuids: json!([workflow_uuid2.to_string()]),
    ///     parent_count: 0,
    ///     child_count: 1,
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(0),
    /// };
    ///
    /// let orphaned_step = StepDagRelationship {
    ///     workflow_step_uuid: workflow_uuid99,
    ///     task_uuid,
    ///     named_step_uuid: named_uuid99,
    ///     parent_step_uuids: json!([]),
    ///     child_step_uuids: json!([]),
    ///     parent_count: 0,
    ///     child_count: 0,
    ///     min_depth_from_root: None, // No depth - possibly orphaned
    ///     is_root_step: false,
    ///     is_leaf_step: false,
    /// };
    ///
    /// assert!(!normal_step.is_orphaned());
    /// assert!(orphaned_step.is_orphaned());
    ///
    /// if orphaned_step.is_orphaned() {
    ///     warn!(step_uuid = %orphaned_step.workflow_step_uuid, "WARNING: Step may be in a dependency cycle!");
    ///     // Log for investigation or skip execution
    /// }
    /// ```
    pub fn is_orphaned(&self) -> bool {
        self.min_depth_from_root.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_dag_relationship(
        parent_uuids: serde_json::Value,
        child_uuids: serde_json::Value,
        is_root: bool,
        is_leaf: bool,
        depth: Option<i32>,
    ) -> StepDagRelationship {
        StepDagRelationship {
            workflow_step_uuid: Uuid::now_v7(),
            task_uuid: Uuid::now_v7(),
            named_step_uuid: Uuid::now_v7(),
            parent_step_uuids: parent_uuids,
            child_step_uuids: child_uuids,
            parent_count: 0,
            child_count: 0,
            is_root_step: is_root,
            is_leaf_step: is_leaf,
            min_depth_from_root: depth,
        }
    }

    #[test]
    fn test_parent_ids_with_uuid_array() {
        let rel = make_dag_relationship(
            json!([
                "550e8400-e29b-41d4-a716-446655440001",
                "660e8400-e29b-41d4-a716-446655440002"
            ]),
            json!([]),
            false,
            false,
            Some(1),
        );
        let parents = rel.parent_ids();
        assert_eq!(parents.len(), 2);
        assert_eq!(
            parents[0],
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap()
        );
        assert_eq!(
            parents[1],
            Uuid::parse_str("660e8400-e29b-41d4-a716-446655440002").unwrap()
        );
    }

    #[test]
    fn test_parent_ids_with_empty_array() {
        let rel = make_dag_relationship(json!([]), json!([]), true, false, Some(0));
        let parents = rel.parent_ids();
        assert!(parents.is_empty());
    }

    #[test]
    fn test_parent_ids_with_null() {
        let rel = make_dag_relationship(json!(null), json!([]), false, false, None);
        let parents = rel.parent_ids();
        assert!(parents.is_empty());
    }

    #[test]
    fn test_child_ids_with_uuid_array() {
        let rel = make_dag_relationship(
            json!([]),
            json!([
                "770e8400-e29b-41d4-a716-446655440003",
                "880e8400-e29b-41d4-a716-446655440004"
            ]),
            true,
            false,
            Some(0),
        );
        let children = rel.child_ids();
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0],
            Uuid::parse_str("770e8400-e29b-41d4-a716-446655440003").unwrap()
        );
        assert_eq!(
            children[1],
            Uuid::parse_str("880e8400-e29b-41d4-a716-446655440004").unwrap()
        );
    }

    #[test]
    fn test_child_ids_with_empty_array() {
        let rel = make_dag_relationship(json!([]), json!([]), false, true, Some(2));
        let children = rel.child_ids();
        assert!(children.is_empty());
    }

    #[test]
    fn test_can_execute_immediately_root_step() {
        let rel = make_dag_relationship(json!([]), json!([]), true, false, Some(0));
        assert!(rel.can_execute_immediately());
    }

    #[test]
    fn test_can_execute_immediately_non_root_step() {
        let rel = make_dag_relationship(
            json!(["550e8400-e29b-41d4-a716-446655440001"]),
            json!([]),
            false,
            true,
            Some(1),
        );
        assert!(!rel.can_execute_immediately());
    }

    #[test]
    fn test_is_workflow_exit_leaf_step() {
        let rel = make_dag_relationship(json!([]), json!([]), false, true, Some(3));
        assert!(rel.is_workflow_exit());
    }

    #[test]
    fn test_is_workflow_exit_non_leaf_step() {
        let rel = make_dag_relationship(
            json!([]),
            json!(["550e8400-e29b-41d4-a716-446655440001"]),
            true,
            false,
            Some(0),
        );
        assert!(!rel.is_workflow_exit());
    }

    #[test]
    fn test_execution_level_with_some_depth() {
        let rel = make_dag_relationship(json!([]), json!([]), false, false, Some(2));
        assert_eq!(rel.execution_level(), Some(2));
    }

    #[test]
    fn test_execution_level_with_none() {
        let rel = make_dag_relationship(json!([]), json!([]), false, false, None);
        assert_eq!(rel.execution_level(), None);
    }

    #[test]
    fn test_is_orphaned_with_none_depth() {
        let rel = make_dag_relationship(json!([]), json!([]), false, false, None);
        assert!(rel.is_orphaned());
    }

    #[test]
    fn test_is_orphaned_with_some_depth() {
        let rel = make_dag_relationship(json!([]), json!([]), true, false, Some(0));
        assert!(!rel.is_orphaned());
    }

    #[test]
    fn test_step_dag_relationship_query_construction() {
        let task_uuid = Uuid::now_v7();
        let query = StepDagRelationshipQuery {
            task_uuid: Some(task_uuid),
            workflow_step_uuid: None,
            min_depth: Some(0),
            max_depth: Some(5),
            is_root_step: Some(true),
            is_leaf_step: None,
        };
        assert_eq!(query.task_uuid, Some(task_uuid));
        assert!(query.workflow_step_uuid.is_none());
        assert_eq!(query.min_depth, Some(0));
        assert_eq!(query.max_depth, Some(5));
        assert_eq!(query.is_root_step, Some(true));
        assert!(query.is_leaf_step.is_none());
    }
}

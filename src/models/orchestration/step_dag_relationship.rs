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
//! ### `tasker_step_dag_relationships`
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
//!   workflow_step_id bigint,
//!   task_id bigint,
//!   named_step_id integer,
//!   parent_step_ids jsonb,     -- Array of parent workflow_step_ids
//!   child_step_ids jsonb,      -- Array of child workflow_step_ids
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
use sqlx::{FromRow, PgPool};

/// Represents computed DAG relationship analysis for workflow steps.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of querying
/// the `tasker_step_dag_relationships` SQL VIEW.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Step dependencies from `tasker_workflow_step_edges`
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct StepDagRelationship {
    pub workflow_step_id: i64,
    pub task_id: i64,
    pub named_step_id: i32,
    pub parent_step_ids: JsonValue, // JSONB array of parent workflow_step_ids
    pub child_step_ids: JsonValue,  // JSONB array of child workflow_step_ids
    pub parent_count: i64,
    pub child_count: i64,
    pub is_root_step: bool,               // No parents - workflow entry point
    pub is_leaf_step: bool,               // No children - workflow exit point
    pub min_depth_from_root: Option<i32>, // Minimum depth from root (can be null for orphaned steps)
}

/// Query parameters for DAG relationship analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDagRelationshipQuery {
    pub task_id: Option<i64>,
    pub workflow_step_id: Option<i64>,
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
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                parent_step_ids as "parent_step_ids!: JsonValue",
                child_step_ids as "child_step_ids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker_step_dag_relationships
            ORDER BY task_id, min_depth_from_root NULLS LAST, workflow_step_id
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get DAG relationships for a specific task.
    pub async fn get_by_task(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                parent_step_ids as "parent_step_ids!: JsonValue",
                child_step_ids as "child_step_ids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE task_id = $1
            ORDER BY min_depth_from_root NULLS LAST, workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get DAG relationship for a specific workflow step.
    pub async fn get_by_step(
        pool: &PgPool,
        workflow_step_id: i64,
    ) -> Result<Option<StepDagRelationship>, sqlx::Error> {
        let relationship = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                parent_step_ids as "parent_step_ids!: JsonValue",
                child_step_ids as "child_step_ids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE workflow_step_id = $1
            "#,
            workflow_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(relationship)
    }

    /// Get all root steps (entry points) for a task.
    pub async fn get_root_steps(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                parent_step_ids as "parent_step_ids!: JsonValue",
                child_step_ids as "child_step_ids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE task_id = $1 AND is_root_step = true
            ORDER BY workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get all leaf steps (exit points) for a task.
    pub async fn get_leaf_steps(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                parent_step_ids as "parent_step_ids!: JsonValue",
                child_step_ids as "child_step_ids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
            FROM tasker_step_dag_relationships
            WHERE task_id = $1 AND is_leaf_step = true
            ORDER BY workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(relationships)
    }

    /// Get steps at a specific depth level.
    pub async fn get_by_depth(
        pool: &PgPool,
        task_id: i64,
        depth: i32,
    ) -> Result<Vec<StepDagRelationship>, sqlx::Error> {
        let relationships = sqlx::query_as!(
            StepDagRelationship,
            r#"
            SELECT
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                named_step_id as "named_step_id!: i32",
                parent_step_ids as "parent_step_ids!: JsonValue",
                child_step_ids as "child_step_ids!: JsonValue",
                parent_count as "parent_count!: i64",
                child_count as "child_count!: i64",
                is_root_step as "is_root_step!: bool",
                is_leaf_step as "is_leaf_step!: bool",
                min_depth_from_root
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

    /// Get parent step IDs as a `Vec<i64>`.
    ///
    /// Returns the workflow step IDs that this step depends on.
    /// These steps must complete successfully before this step can run.
    ///
    /// # Example
    /// ```rust
    /// use tasker_core::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    ///
    /// let deploy_step = StepDagRelationship {
    ///     workflow_step_id: 3,
    ///     task_id: 100,
    ///     named_step_id: 1003,
    ///     parent_step_ids: json!([1, 2]), // Build and Test step IDs
    ///     child_step_ids: json!([]),
    ///     parent_count: 2,
    ///     child_count: 0,
    ///     is_root_step: false,
    ///     is_leaf_step: true,
    ///     min_depth_from_root: Some(2),
    /// };
    ///
    /// let parents = deploy_step.parent_ids();
    /// assert_eq!(parents, vec![1, 2]);
    /// println!("Deploy step waits for steps: {:?}", parents); // [1, 2]
    /// ```
    pub fn parent_ids(&self) -> Vec<i64> {
        self.parent_step_ids
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default()
    }

    /// Get child step IDs as a `Vec<i64>`.
    ///
    /// Returns the workflow step IDs that depend on this step.
    /// These steps will be blocked until this step completes successfully.
    ///
    /// # Example
    /// ```rust
    /// use tasker_core::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    ///
    /// let build_step = StepDagRelationship {
    ///     workflow_step_id: 1,
    ///     task_id: 100,
    ///     named_step_id: 1001,
    ///     parent_step_ids: json!([]), // No dependencies - can start immediately
    ///     child_step_ids: json!([2, 3]), // Test and Package step IDs
    ///     parent_count: 0,
    ///     child_count: 2,
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(0),
    /// };
    ///
    /// let children = build_step.child_ids();
    /// assert_eq!(children, vec![2, 3]);
    /// println!("Build completion will unblock: {:?}", children); // [2, 3]
    /// ```
    pub fn child_ids(&self) -> Vec<i64> {
        self.child_step_ids
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default()
    }

    /// Check if this step has no dependencies (can execute immediately).
    ///
    /// Root steps are workflow entry points that can start as soon as the task begins.
    ///
    /// # Example
    /// ```rust
    /// use tasker_core::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    ///
    /// let root_step = StepDagRelationship {
    ///     workflow_step_id: 1,
    ///     task_id: 100,
    ///     named_step_id: 1001,
    ///     parent_step_ids: json!([]), // No dependencies
    ///     child_step_ids: json!([2, 3]),
    ///     parent_count: 0,
    ///     child_count: 2,
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    ///     min_depth_from_root: Some(0),
    /// };
    ///
    /// let dependent_step = StepDagRelationship {
    ///     workflow_step_id: 2,
    ///     task_id: 100,
    ///     named_step_id: 1002,
    ///     parent_step_ids: json!([1]), // Depends on step 1
    ///     child_step_ids: json!([]),
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
    /// use tasker_core::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    ///
    /// let final_step = StepDagRelationship {
    ///     workflow_step_id: 3,
    ///     task_id: 100,
    ///     named_step_id: 1003,
    ///     parent_step_ids: json!([1, 2]),
    ///     child_step_ids: json!([]), // No children - this is a leaf step
    ///     parent_count: 2,
    ///     child_count: 0,
    ///     is_root_step: false,
    ///     is_leaf_step: true,
    ///     min_depth_from_root: Some(2),
    /// };
    ///
    /// let middle_step = StepDagRelationship {
    ///     workflow_step_id: 2,
    ///     task_id: 100,
    ///     named_step_id: 1002,
    ///     parent_step_ids: json!([1]),
    ///     child_step_ids: json!([3]), // Has children - not a leaf
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
    /// use tasker_core::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    ///
    /// let root_step = StepDagRelationship {
    ///     workflow_step_id: 1,
    ///     task_id: 100,
    ///     named_step_id: 1001,
    ///     parent_step_ids: json!([]),
    ///     child_step_ids: json!([2, 3]),
    ///     parent_count: 0,
    ///     child_count: 2,
    ///     min_depth_from_root: Some(0), // Root step has depth 0
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    /// };
    ///
    /// let second_level_step = StepDagRelationship {
    ///     workflow_step_id: 2,
    ///     task_id: 100,
    ///     named_step_id: 1002,
    ///     parent_step_ids: json!([1]),
    ///     child_step_ids: json!([4]),
    ///     parent_count: 1,
    ///     child_count: 1,
    ///     min_depth_from_root: Some(1), // One step from root
    ///     is_root_step: false,
    ///     is_leaf_step: false,
    /// };
    ///
    /// let orphaned_step = StepDagRelationship {
    ///     workflow_step_id: 99,
    ///     task_id: 100,
    ///     named_step_id: 1099,
    ///     parent_step_ids: json!([]),
    ///     child_step_ids: json!([]),
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
    /// use tasker_core::models::orchestration::step_dag_relationship::StepDagRelationship;
    /// use serde_json::json;
    ///
    /// let normal_step = StepDagRelationship {
    ///     workflow_step_id: 1,
    ///     task_id: 100,
    ///     named_step_id: 1001,
    ///     parent_step_ids: json!([]),
    ///     child_step_ids: json!([2]),
    ///     parent_count: 0,
    ///     child_count: 1,
    ///     min_depth_from_root: Some(0), // Has calculated depth
    ///     is_root_step: true,
    ///     is_leaf_step: false,
    /// };
    ///
    /// let orphaned_step = StepDagRelationship {
    ///     workflow_step_id: 99,
    ///     task_id: 100,
    ///     named_step_id: 1099,
    ///     parent_step_ids: json!([]),
    ///     child_step_ids: json!([]),
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
    ///     eprintln!("WARNING: Step {} may be in a dependency cycle!", orphaned_step.workflow_step_id);
    ///     // Log for investigation or skip execution
    /// }
    /// ```
    pub fn is_orphaned(&self) -> bool {
        self.min_depth_from_root.is_none()
    }
}

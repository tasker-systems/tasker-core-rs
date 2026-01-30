//! # Step Transitive Dependencies Model
//!
//! This module provides models for querying and working with transitive step dependencies.
//! It supports efficient DAG traversal and dependency resolution for workflow orchestration.
//!
//! ## Overview
//!
//! The `StepTransitiveDependencies` struct represents the result of the
//! `get_step_transitive_dependencies` SQL function, which recursively finds all
//! ancestor steps (transitive dependencies) for a given step.
//!
//! ## SQL Function Integration
//!
//! This model corresponds to the `get_step_transitive_dependencies(target_step_uuid)`
//! SQL function that uses recursive CTEs to traverse the DAG and return all
//! ancestor steps with their results and processing status.
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use tasker_shared::models::orchestration::StepTransitiveDependencies;
//! use tasker_shared::database::sql_functions::SqlFunctionExecutor;
//! use sqlx::PgPool;
//! use uuid::Uuid;
//!
//! # async fn example(pool: PgPool, step_uuid: Uuid) -> Result<(), sqlx::Error> {
//! let executor = SqlFunctionExecutor::new(pool);
//! let dependencies = executor.get_step_transitive_dependencies(step_uuid).await?;
//!
//! for dep in dependencies {
//!     println!("Dependency: {} (distance: {})", dep.step_name, dep.distance);
//!     if dep.processed {
//!         println!("  Results: {:?}", dep.results);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::messaging::StepExecutionResult;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;
use uuid::Uuid;

pub type StepDependencyResultMap = HashMap<String, StepExecutionResult>;
pub type StepExecutionResultAsJson = sqlx::types::Json<StepExecutionResult>;

impl From<StepExecutionResultAsJson> for StepExecutionResult {
    fn from(json: StepExecutionResultAsJson) -> Self {
        json.0
    }
}

/// Result structure for get_step_transitive_dependencies SQL function
///
/// Represents a single ancestor step in the transitive dependency chain,
/// including its processing status and results.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepTransitiveDependencies {
    /// Primary key of the workflow step
    pub workflow_step_uuid: Uuid,
    /// Task that this step belongs to
    pub task_uuid: Uuid,
    /// Named step template ID
    pub named_step_uuid: Uuid,
    /// Human-readable step name
    pub step_name: String,
    /// Step execution results (JSON)
    pub results: Option<StepExecutionResultAsJson>,
    /// Whether this step has been processed successfully
    pub processed: bool,
    /// Distance from the target step (1 = direct parent, 2 = grandparent, etc.)
    pub distance: i32,
}

impl StepTransitiveDependencies {
    /// Create a new step transitive dependency entry
    pub fn new(
        workflow_step_uuid: Uuid,
        task_uuid: Uuid,
        named_step_uuid: Uuid,
        step_name: String,
        results: Option<StepExecutionResultAsJson>,
        processed: bool,
        distance: i32,
    ) -> Self {
        Self {
            workflow_step_uuid,
            task_uuid,
            named_step_uuid,
            step_name,
            results,
            processed,
            distance,
        }
    }

    /// Check if this dependency is a direct parent (distance = 1)
    pub fn is_direct_parent(&self) -> bool {
        self.distance == 1
    }

    /// Check if this dependency has completed processing
    pub fn is_completed(&self) -> bool {
        self.processed
    }

    /// Get the step results as a structured value
    pub fn get_results(&self) -> Option<&StepExecutionResultAsJson> {
        self.results.as_ref()
    }

    /// Check if this dependency is ready (has results available)
    pub fn has_results(&self) -> bool {
        self.processed && self.results.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Build a default StepExecutionResult for use in tests.
    fn default_execution_result() -> StepExecutionResult {
        StepExecutionResult {
            step_uuid: Uuid::nil(),
            success: true,
            result: Value::Object(serde_json::Map::new()),
            status: "completed".to_string(),
            ..Default::default()
        }
    }

    // ---- new ----

    #[test]
    fn test_new_sets_all_fields() {
        let ws_uuid = Uuid::new_v4();
        let t_uuid = Uuid::new_v4();
        let ns_uuid = Uuid::new_v4();
        let name = "validate_payment".to_string();
        let exec_result = default_execution_result();
        let results = Some(sqlx::types::Json(exec_result.clone()));

        let dep = StepTransitiveDependencies::new(
            ws_uuid,
            t_uuid,
            ns_uuid,
            name.clone(),
            results,
            true,
            2,
        );

        assert_eq!(dep.workflow_step_uuid, ws_uuid);
        assert_eq!(dep.task_uuid, t_uuid);
        assert_eq!(dep.named_step_uuid, ns_uuid);
        assert_eq!(dep.step_name, name);
        assert!(dep.processed);
        assert_eq!(dep.distance, 2);
        assert!(dep.results.is_some());
        assert_eq!(dep.results.as_ref().unwrap().0.success, exec_result.success);
    }

    #[test]
    fn test_new_without_results() {
        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "step_a".to_string(),
            None,
            false,
            1,
        );

        assert!(dep.results.is_none());
        assert!(!dep.processed);
    }

    // ---- is_direct_parent ----

    #[test]
    fn test_is_direct_parent_distance_one() {
        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "parent_step".to_string(),
            None,
            true,
            1,
        );
        assert!(
            dep.is_direct_parent(),
            "Distance 1 should be a direct parent"
        );
    }

    #[test]
    fn test_is_direct_parent_distance_two() {
        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "grandparent_step".to_string(),
            None,
            true,
            2,
        );
        assert!(
            !dep.is_direct_parent(),
            "Distance 2 should not be a direct parent"
        );
    }

    // ---- is_completed ----

    #[test]
    fn test_is_completed_true() {
        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "done_step".to_string(),
            None,
            true,
            1,
        );
        assert!(dep.is_completed(), "processed=true should mean completed");
    }

    #[test]
    fn test_is_completed_false() {
        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "pending_step".to_string(),
            None,
            false,
            1,
        );
        assert!(
            !dep.is_completed(),
            "processed=false should mean not completed"
        );
    }

    // ---- get_results ----

    #[test]
    fn test_get_results_some() {
        let exec_result = default_execution_result();
        let results = Some(sqlx::types::Json(exec_result));

        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "step_with_results".to_string(),
            results,
            true,
            1,
        );

        let got = dep.get_results();
        assert!(got.is_some(), "Should return Some when results exist");
        assert!(got.unwrap().0.success, "Result should indicate success");
    }

    #[test]
    fn test_get_results_none() {
        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "step_no_results".to_string(),
            None,
            false,
            1,
        );

        assert!(
            dep.get_results().is_none(),
            "Should return None when no results"
        );
    }

    // ---- has_results ----

    #[test]
    fn test_has_results_processed_with_results() {
        let exec_result = default_execution_result();
        let results = Some(sqlx::types::Json(exec_result));

        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "completed_step".to_string(),
            results,
            true,
            1,
        );

        assert!(
            dep.has_results(),
            "processed=true with results should have results"
        );
    }

    #[test]
    fn test_has_results_processed_without_results() {
        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "completed_no_data".to_string(),
            None,
            true,
            1,
        );

        assert!(
            !dep.has_results(),
            "processed=true without results should not have results"
        );
    }

    #[test]
    fn test_has_results_not_processed() {
        let exec_result = default_execution_result();
        let results = Some(sqlx::types::Json(exec_result));

        let dep = StepTransitiveDependencies::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "not_processed".to_string(),
            results,
            false,
            1,
        );

        assert!(
            !dep.has_results(),
            "processed=false should not have results even if results field is Some"
        );
    }
}

/// Query builder for step transitive dependencies operations
pub struct StepTransitiveDependenciesQuery {
    pool: PgPool,
}

crate::debug_with_pgpool!(StepTransitiveDependenciesQuery { pool: PgPool });

impl StepTransitiveDependenciesQuery {
    /// Create a new query builder
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get all transitive dependencies for a step
    ///
    /// This method calls the `get_step_transitive_dependencies` SQL function
    /// to recursively find all ancestor steps.
    pub async fn get_for_step(
        &self,
        step_uuid: Uuid,
    ) -> Result<Vec<StepTransitiveDependencies>, sqlx::Error> {
        let sql = "SELECT * FROM get_step_transitive_dependencies($1)";
        sqlx::query_as::<_, StepTransitiveDependencies>(sql)
            .bind(step_uuid)
            .fetch_all(&self.pool)
            .await
    }

    /// Get transitive dependencies as a HashMap for efficient lookup by step name
    ///
    /// This is useful when step handlers need to look up specific dependency results
    /// using the step name.
    pub async fn get_as_map(
        &self,
        step_uuid: Uuid,
    ) -> Result<HashMap<String, StepTransitiveDependencies>, sqlx::Error> {
        let dependencies = self.get_for_step(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .map(|dep| (dep.step_name.clone(), dep))
            .collect())
    }

    /// Get only completed transitive dependencies (those with results)
    ///
    /// This filters the results to only include dependencies that have been
    /// processed and have results available.
    pub async fn get_completed_for_step(
        &self,
        step_uuid: Uuid,
    ) -> Result<Vec<StepTransitiveDependencies>, sqlx::Error> {
        let dependencies = self.get_for_step(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .filter(|dep| dep.has_results())
            .collect())
    }

    /// Get direct parents only (distance = 1)
    ///
    /// This is equivalent to the existing immediate dependency queries
    /// but uses the transitive function for consistency.
    pub async fn get_direct_parents(
        &self,
        step_uuid: Uuid,
    ) -> Result<Vec<StepTransitiveDependencies>, sqlx::Error> {
        let dependencies = self.get_for_step(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .filter(|dep| dep.is_direct_parent())
            .collect())
    }

    /// Get dependency results as a HashMap for step handler consumption
    ///
    /// This creates a map of step_name -> results for easy lookup in step handlers,
    /// similar to the existing `sequence.get_results()` pattern.
    pub async fn get_results_map(
        &self,
        step_uuid: Uuid,
    ) -> Result<StepDependencyResultMap, sqlx::Error> {
        let dependencies = self.get_completed_for_step(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .filter_map(|dep| dep.results.map(|results| (dep.step_name, results.into())))
            .collect())
    }
}

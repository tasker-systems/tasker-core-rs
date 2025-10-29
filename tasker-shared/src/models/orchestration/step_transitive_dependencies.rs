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

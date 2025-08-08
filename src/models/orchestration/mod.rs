//! # Orchestration Models
//!
//! This module contains models for workflow orchestration and execution management.
//! These models provide computed views and analysis for understanding workflow state,
//! readiness, and dependency relationships.
//!
//! ## Orchestration Models
//!
//! Most models in this module are **computed views** - they represent the output of SQL functions
//! or views that analyze the underlying task and step data to provide orchestration insights.
//! These models typically do not support create, update, or delete operations since they
//! represent computed analysis.
//!
//! ### Execution Context
//! - `TaskExecutionContext`: Comprehensive task execution state and statistics
//! - `StepReadinessStatus`: Step readiness analysis for orchestration decisions
//!
//! ### DAG Analysis
//! - `StepDagRelationship`: DAG relationship analysis and hierarchy computation
//!
//! ## Usage Patterns
//!
//! These models are typically used for:
//! - Workflow orchestration and scheduling decisions
//! - Step readiness and dependency resolution
//! - DAG traversal and execution planning
//! - Real-time execution state monitoring
//! - Orchestrator health and progress tracking

pub mod step_dag_relationship;
pub mod step_readiness_status;
pub mod step_transitive_dependencies;
pub mod task_execution_context;

// Re-export for easy access
pub use step_dag_relationship::{StepDagRelationship, StepDagRelationshipQuery};
pub use step_readiness_status::{
    NewStepReadinessStatus, StepReadinessResult, StepReadinessStatus, StepReadinessWithStep,
};
pub use step_transitive_dependencies::{StepTransitiveDependencies, StepTransitiveDependenciesQuery};
pub use task_execution_context::TaskExecutionContext;

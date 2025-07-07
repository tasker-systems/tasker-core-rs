//! # Data Models
//!
//! Complete data layer with all Rails models migrated to Rust with 100% schema parity.
//!
//! ## Overview
//!
//! This module contains all 18+ Rails models fully migrated to Rust with:
//! - **100% Schema Accuracy**: Exact match with Rails production schema
//! - **Type Safety**: Full SQLx compile-time verification
//! - **Rails Scope Equivalents**: High-performance query builders
//! - **Comprehensive Testing**: SQLx native testing with database isolation
//!
//! ## Model Organization
//!
//! ### Core Table-Based Models
//!
//! Primary workflow orchestration entities:
//! - [`Task`] - Core task instances with JSONB context and identity hashing
//! - [`TaskNamespace`] - Organizational hierarchy for tasks
//! - [`NamedTask`] - Task templates with versioning and configuration
//! - [`NamedStep`] - Step definitions linked to dependent systems
//! - [`WorkflowStep`] - Individual step instances with retry state management
//! - [`WorkflowStepEdge`] - DAG dependency relationships between steps
//! - [`WorkflowStepTransition`] - Step state change audit trail with retry tracking
//! - [`TaskTransition`] - Task state change audit trail
//!
//! ### Supporting Models
//!
//! Configuration and metadata:
//! - [`NamedTasksNamedStep`] - Junction table with step configuration
//! - [`DependentSystem`] - External system references for step handlers
//! - [`DependentSystemObjectMap`] - Bidirectional system object mappings
//! - [`AnnotationType`] - Annotation categorization and metadata
//! - [`TaskAnnotation`] - Task metadata storage with JSONB annotations
//!
//! ### Specialized Model Modules
//!
//! - [`orchestration`] - Workflow execution and DAG analysis models
//!   - [`orchestration::TaskExecutionContext`] - Real-time task execution status
//!   - [`orchestration::StepReadinessStatus`] - Step dependency analysis
//!   - [`orchestration::StepDagRelationship`] - DAG relationship analysis
//!
//! - [`insights`] - Analytics and performance monitoring models
//!   - [`insights::AnalyticsMetrics`] - System-wide performance metrics
//!   - [`insights::SystemHealthCounts`] - Real-time health monitoring
//!   - [`insights::SlowestSteps`] - Step performance analysis
//!   - [`insights::SlowestTasks`] - Task optimization insights
//!
//! ## Key Features
//!
//! - **JSONB Operations**: Full PostgreSQL JSONB support with containment/path queries
//! - **DAG Analysis**: Complete dependency resolution with cycle detection
//! - **Retry Logic**: Exponential backoff and retry limit enforcement
//! - **State Machines**: Proper state tracking with transition audit trails
//! - **Performance Monitoring**: Real-time analytics and bottleneck identification
//! - **Complex Scoping**: Rails-equivalent scopes for filtering and advanced queries
//!
//! ## Rails Heritage
//!
//! All models are migrated from `/Users/petetaylor/projects/tasker/app/models/tasker/`
//! with complete preservation of:
//! - ActiveRecord associations and validations
//! - Complex scopes and query methods
//! - Business logic and computed properties
//! - Database constraints and indexes
//!
//! ## Usage Examples
//!
//! ```rust,no_run
//! use tasker_core::models::core::task::{Task, NewTask};
//! use tasker_core::models::core::workflow_step::WorkflowStep;
//! use tasker_core::models::core::task_namespace::TaskNamespace;
//! use serde_json::json;
//! use sqlx::PgPool;
//!
//! # async fn example(pool: &PgPool, named_task_id: i32) -> Result<(), sqlx::Error> {
//! // Find active tasks (this would need to be implemented)
//! // let active_tasks = Task::find_active(pool).await?;
//!
//! // Create task with context
//! let task = Task::create(pool, NewTask {
//!     named_task_id,
//!     requested_at: None,
//!     initiator: Some("system".to_string()),
//!     source_system: None,
//!     reason: None,
//!     bypass_steps: None,
//!     tags: None,
//!     context: Some(json!({"input": "data"})),
//!     identity_hash: "example_hash".to_string(),
//! }).await?;
//! # Ok(())
//! # }
//! ```

// Deprecated - kept for backwards compatibility

// Organized model modules
pub mod core;
pub mod insights;
pub mod orchestration;

// Re-export core models for easy access
pub use core::annotation_type::{AnnotationType, AnnotationTypeWithStats, NewAnnotationType};
pub use core::dependent_system::{DependentSystem, NewDependentSystem};
pub use core::dependent_system_object_map::{
    DependentSystemObjectMap, DependentSystemObjectMapWithSystems, MappingStats,
    NewDependentSystemObjectMap,
};
pub use core::named_step::NamedStep;
pub use core::named_task::NamedTask;
pub use core::named_tasks_named_step::{NamedTasksNamedStep, NewNamedTasksNamedStep};
pub use core::task::Task;
pub use core::task_annotation::{
    AnnotationTypeCount, NewTaskAnnotation, TaskAnnotation, TaskAnnotationWithType,
};
pub use core::task_namespace::TaskNamespace;
pub use core::task_transition::{NewTaskTransition, TaskTransition};
pub use core::workflow_step::WorkflowStep;
pub use core::workflow_step_edge::WorkflowStepEdge;
pub use core::workflow_step_transition::{NewWorkflowStepTransition, WorkflowStepTransition};

// Re-export organized model modules
pub use core::*;
pub use insights::*;
pub use orchestration::*;

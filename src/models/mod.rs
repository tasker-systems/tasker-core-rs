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
//! ```rust,ignore
//! use tasker_core::models::{Task, WorkflowStep, TaskNamespace};
//!
//! // Find active tasks
//! let active_tasks = Task::find_active(&pool).await?;
//!
//! // Create task with context
//! let task = Task::create(&pool, NewTask {
//!     named_task_id: template.named_task_id,
//!     context: json!({"input": "data"}),
//!     ..Default::default()
//! }).await?;
//! ```

pub mod annotation_type;
pub mod dependent_system;
pub mod dependent_system_object_map;
pub mod named_step;
pub mod named_task;
pub mod named_tasks_named_step;
pub mod task;
pub mod task_annotation;
pub mod task_namespace;
pub mod task_transition;
pub mod transitions;
pub mod workflow_step;
pub mod workflow_step_edge;
pub mod workflow_step_transition; // Deprecated - kept for backwards compatibility

// Organized model modules
pub mod insights;
pub mod orchestration;

// Re-export core models for easy access
pub use annotation_type::{AnnotationType, AnnotationTypeWithStats, NewAnnotationType};
pub use dependent_system::{DependentSystem, NewDependentSystem};
pub use dependent_system_object_map::{
    DependentSystemObjectMap, DependentSystemObjectMapWithSystems, MappingStats,
    NewDependentSystemObjectMap,
};
pub use named_step::NamedStep;
pub use named_task::NamedTask;
pub use named_tasks_named_step::{NamedTasksNamedStep, NewNamedTasksNamedStep};
pub use task::Task;
pub use task_annotation::{
    AnnotationTypeCount, NewTaskAnnotation, TaskAnnotation, TaskAnnotationWithType,
};
pub use task_namespace::TaskNamespace;
pub use task_transition::{NewTaskTransition, TaskTransition};
pub use workflow_step::WorkflowStep;
pub use workflow_step_edge::WorkflowStepEdge;
pub use workflow_step_transition::{NewWorkflowStepTransition, WorkflowStepTransition};

// Re-export organized model modules
pub use insights::*;
pub use orchestration::*;

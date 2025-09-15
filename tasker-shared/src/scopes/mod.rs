//! # Query Scopes Module

pub mod common;
pub mod edges;
pub mod named_task;
pub mod task;
pub mod transitions;
pub mod workflow_step;

// Re-export common functionality
pub use common::{ScopeBuilder, SpecialQuery};

// Re-export all scope builders
pub use edges::WorkflowStepEdgeScope;
pub use named_task::{NamedTaskScope, TaskNamespaceScope};
pub use task::TaskScope;
pub use transitions::{TaskTransitionScope, WorkflowStepTransitionScope};
pub use workflow_step::WorkflowStepScope;

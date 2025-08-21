//! # Orchestration Types
//!
//! Core types and data structures used throughout the orchestration system.
//!
//! This module provides the fundamental types that are shared across all orchestration
//! components, including task results, step results, handler metadata, and configuration
//! structures.

use std::time::Duration;
use uuid::Uuid;

use tasker_shared::types::TaskContext;
use tasker_shared::OrchestrationError;

pub type OrchestrationResult<T> = Result<T, OrchestrationError>;

/// Framework integration trait for delegation
///
/// This trait defines the interface for framework-specific step execution.
/// The orchestration core handles concurrency, DAG traversal, and viable step
/// discovery, while frameworks only need to implement individual step execution.
#[async_trait::async_trait]
pub trait FrameworkIntegration: Send + Sync {
    /// Framework name for logging/metrics
    fn framework_name(&self) -> &'static str;

    /// Get task context for execution
    async fn get_task_context(&self, task_uuid: Uuid) -> Result<TaskContext, OrchestrationError>;

    /// Enqueue task back to framework's queue
    async fn enqueue_task(
        &self,
        task_uuid: Uuid,
        delay: Option<Duration>,
    ) -> Result<(), OrchestrationError>;

    /// Check if this framework supports native batch execution
    ///
    /// All frameworks now support batch execution since individual step execution
    /// has been removed. This method can be used to distinguish between frameworks
    /// that implement true parallelism vs sequential batch processing.
    fn supports_batch_execution(&self) -> bool {
        true // All frameworks must support batch execution
    }
}

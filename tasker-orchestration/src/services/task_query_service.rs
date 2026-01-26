//! # Task Query Service
//!
//! TAS-76: Database queries for task operations, extracted from task handlers.
//! This service handles all database interactions for tasks, providing a clean
//! separation between query logic and business logic.
//!
//! ## Design
//!
//! The query service is responsible for:
//! - Fetching tasks with execution context
//! - Batch queries for task lists with pagination
//! - Composing related data (steps, metadata, execution status)
//!
//! It delegates to `SqlFunctionExecutor` for complex PostgreSQL functions and
//! uses the `Task` model for standard CRUD operations.

use std::collections::HashMap;

use bigdecimal::ToPrimitive;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use tracing::error;
use uuid::Uuid;

use tasker_shared::database::sql_functions::{
    SqlFunctionExecutor, StepReadinessStatus, TaskExecutionContext,
};
use tasker_shared::models::core::task::{PaginationInfo, Task, TaskListQuery};
use tasker_shared::models::orchestration::{ExecutionStatus, RecommendedAction};
use tasker_shared::types::api::orchestration::TaskResponse;

/// Errors that can occur during task query operations.
#[derive(Error, Debug)]
pub enum TaskQueryError {
    #[error("Task not found: {0}")]
    NotFound(Uuid),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Failed to fetch task metadata: {0}")]
    MetadataError(String),
}

/// Result type for task query operations.
pub type TaskQueryResult<T> = Result<T, TaskQueryError>;

/// Task with all related context data for API responses.
#[derive(Debug, Clone)]
pub struct TaskWithContext {
    pub task: Task,
    pub task_name: String,
    pub namespace: String,
    pub version: String,
    pub status: String,
    pub execution_context: TaskExecutionContext,
    pub steps: Vec<StepReadinessStatus>,
}

/// Paginated list of tasks with execution context.
#[derive(Debug)]
pub struct PaginatedTasksWithContext {
    pub tasks: Vec<TaskWithContext>,
    pub pagination: PaginationInfo,
}

/// Service for task-related database queries.
///
/// Provides methods for fetching tasks with their execution context,
/// step readiness status, and metadata. Uses connection pooling and
/// the SQL function executor for complex aggregations.
#[derive(Debug, Clone)]
pub struct TaskQueryService {
    pool: PgPool,
}

impl TaskQueryService {
    /// Create a new task query service.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a task by UUID with full execution context and step information.
    ///
    /// This performs multiple queries:
    /// 1. Fetch the task
    /// 2. Fetch execution context via SQL function
    /// 3. Fetch step readiness status
    /// 4. Fetch task metadata (name, namespace, version)
    pub async fn get_task_with_context(&self, uuid: Uuid) -> TaskQueryResult<TaskWithContext> {
        let sql_executor = SqlFunctionExecutor::new(self.pool.clone());

        // Get basic task information
        let task = Task::find_by_id(&self.pool, uuid)
            .await?
            .ok_or(TaskQueryError::NotFound(uuid))?;

        // Get execution context using SQL function
        let execution_context = sql_executor
            .get_task_execution_context(uuid)
            .await?
            .ok_or(TaskQueryError::NotFound(uuid))?;

        // Get step readiness status
        let steps = sql_executor.get_step_readiness_status(uuid, None).await?;

        // Get task metadata using JOIN query
        let task_metadata = task.for_orchestration(&self.pool).await.map_err(|e| {
            error!(error = %e, "Failed to get task metadata");
            TaskQueryError::MetadataError(e.to_string())
        })?;

        Ok(TaskWithContext {
            task,
            task_name: task_metadata.task_name,
            namespace: task_metadata.namespace_name,
            version: task_metadata.task_version,
            status: execution_context.execution_status.as_str().to_string(),
            execution_context,
            steps,
        })
    }

    /// List tasks with pagination and execution context.
    ///
    /// Uses batch SQL functions to efficiently fetch execution contexts
    /// for all tasks in a single query, avoiding N+1 problems.
    pub async fn list_tasks_with_context(
        &self,
        query: &TaskListQuery,
    ) -> TaskQueryResult<PaginatedTasksWithContext> {
        let sql_executor = SqlFunctionExecutor::new(self.pool.clone());

        // Get paginated tasks
        let paginated_result = Task::list_with_pagination(&self.pool, query).await?;

        // Extract task UUIDs for batch execution context lookup
        let task_uuids: Vec<Uuid> = paginated_result
            .tasks
            .iter()
            .map(|t| t.task.task_uuid)
            .collect();

        // Batch fetch execution contexts
        let execution_contexts = sql_executor
            .get_task_execution_contexts_batch(task_uuids)
            .await?;

        // Create lookup map
        let context_map: HashMap<Uuid, TaskExecutionContext> = execution_contexts
            .into_iter()
            .map(|ctx| (ctx.task_uuid, ctx))
            .collect();

        // Combine tasks with their execution contexts
        let tasks: Vec<TaskWithContext> = paginated_result
            .tasks
            .into_iter()
            .map(|twm| {
                let execution_context = context_map
                    .get(&twm.task.task_uuid)
                    .cloned()
                    .unwrap_or_else(|| {
                        Self::default_execution_context(
                            twm.task.task_uuid,
                            twm.task.named_task_uuid,
                        )
                    });

                TaskWithContext {
                    task: twm.task,
                    task_name: twm.task_name,
                    namespace: twm.namespace_name,
                    version: twm.task_version,
                    status: twm.status,
                    execution_context,
                    steps: Vec::new(), // Steps not included in list view
                }
            })
            .collect();

        Ok(PaginatedTasksWithContext {
            tasks,
            pagination: paginated_result.pagination,
        })
    }

    /// Convert a TaskWithContext to a TaskResponse for API responses.
    ///
    /// Handles all the type conversions (BigDecimal to f64, JSONB tags, etc.)
    pub fn to_task_response(twc: &TaskWithContext) -> TaskResponse {
        let task = &twc.task;
        let ec = &twc.execution_context;

        // Convert tags from JSONB to Vec<String>
        let tags = match &task.tags {
            Some(serde_json::Value::Array(arr)) => Some(
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
            ),
            _ => None,
        };

        // Convert BigDecimal to f64
        let completion_percentage = ec.completion_percentage.to_f64().unwrap_or(0.0);

        TaskResponse {
            task_uuid: task.task_uuid.to_string(),
            name: twc.task_name.clone(),
            namespace: twc.namespace.clone(),
            version: twc.version.clone(),
            status: twc.status.clone(),
            created_at: DateTime::from_naive_utc_and_offset(task.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(task.updated_at, Utc),
            completed_at: None,
            context: task
                .context
                .clone()
                .unwrap_or_else(|| serde_json::json!({})),
            initiator: task
                .initiator
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            source_system: task
                .source_system
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            reason: task.reason.clone().unwrap_or_else(|| "unknown".to_string()),
            priority: Some(task.priority),
            tags,

            // Execution context fields
            total_steps: ec.total_steps,
            pending_steps: ec.pending_steps,
            in_progress_steps: ec.in_progress_steps,
            completed_steps: ec.completed_steps,
            failed_steps: ec.failed_steps,
            ready_steps: ec.ready_steps,
            execution_status: ec.execution_status.as_str().to_string(),
            recommended_action: ec
                .recommended_action
                .map(|ra| ra.into())
                .unwrap_or_else(|| "none".to_string()),
            completion_percentage,
            health_status: ec.health_status.clone(),

            // Step information
            steps: twc.steps.clone(),
            correlation_id: task.correlation_id,
            parent_correlation_id: task.parent_correlation_id,
        }
    }

    /// Create a default execution context for tasks without one.
    fn default_execution_context(task_uuid: Uuid, named_task_uuid: Uuid) -> TaskExecutionContext {
        TaskExecutionContext {
            task_uuid,
            named_task_uuid,
            status: "unknown".to_string(),
            total_steps: 0,
            pending_steps: 0,
            in_progress_steps: 0,
            completed_steps: 0,
            failed_steps: 0,
            ready_steps: 0,
            execution_status: ExecutionStatus::WaitingForDependencies,
            recommended_action: Some(RecommendedAction::WaitForDependencies),
            completion_percentage: sqlx::types::BigDecimal::from(0),
            health_status: "unknown".to_string(),
            enqueued_steps: 0,
        }
    }
}

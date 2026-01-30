//! # Task Service
//!
//! TAS-76: Business logic for task operations, extracted from task handlers.
//! This service handles validation, error classification, and delegates to
//! `TaskQueryService` for database operations.
//!
//! ## Design
//!
//! Following the TAS-168 analytics pattern:
//! ```text
//! Handler -> TaskService (business logic) -> TaskQueryService (DB queries)
//! ```
//!
//! The service is responsible for:
//! - Input validation
//! - Error classification (client vs server errors)
//! - Response building
//!
//! Note: Circuit breaker and backpressure handling remain in the handler layer
//! via `execute_with_circuit_breaker` / `execute_with_backpressure_check`.

use std::sync::Arc;

use sqlx::PgPool;
use thiserror::Error;
use tracing::{error, info};
use uuid::Uuid;

use crate::orchestration::lifecycle::task_initialization::{
    TaskInitializationError, TaskInitializer,
};
use crate::services::task_query_service::{TaskQueryError, TaskQueryService, TaskWithContext};
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::models::core::task::{Task, TaskListQuery};
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::api::orchestration::{TaskListResponse, TaskResponse};

/// Errors that can occur during task service operations.
#[derive(Error, Debug)]
pub enum TaskServiceError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Task not found: {0}")]
    NotFound(Uuid),

    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Duplicate task: {0}")]
    DuplicateTask(String),

    #[error("Task cannot be cancelled: {0}")]
    CannotCancel(String),

    #[error("Backpressure active: {reason}")]
    Backpressure {
        reason: String,
        retry_after_seconds: u64,
    },

    #[error("Circuit breaker open")]
    CircuitBreakerOpen,

    #[error("Database error: {0}")]
    Database(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl TaskServiceError {
    /// Check if this is a client error (4xx) vs server error (5xx).
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::Validation(_)
                | Self::NotFound(_)
                | Self::TemplateNotFound(_)
                | Self::InvalidConfiguration(_)
                | Self::DuplicateTask(_)
                | Self::CannotCancel(_)
        )
    }
}

/// Result type for task service operations.
pub type TaskServiceResult<T> = Result<T, TaskServiceError>;

/// Service for task business logic.
///
/// Handles validation, error classification, and coordinates between
/// handlers and the database layer. Circuit breaker protection is handled
/// at the handler layer via `execute_with_circuit_breaker`.
#[derive(Clone)]
pub struct TaskService {
    query_service: TaskQueryService,
    write_pool: PgPool,
    task_initializer: Arc<TaskInitializer>,
    system_context: Arc<SystemContext>,
}

impl std::fmt::Debug for TaskService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskService")
            .field("write_pool", &"PgPool")
            .finish()
    }
}

impl TaskService {
    /// Create a new task service.
    pub fn new(
        read_pool: PgPool,
        write_pool: PgPool,
        task_initializer: Arc<TaskInitializer>,
        system_context: Arc<SystemContext>,
    ) -> Self {
        Self {
            query_service: TaskQueryService::new(read_pool),
            write_pool,
            task_initializer,
            system_context,
        }
    }

    /// Create a new task from a request.
    ///
    /// Performs:
    /// 1. Input validation
    /// 2. Task initialization via TaskInitializer
    /// 3. Fetch full task context for response
    /// 4. Error classification
    ///
    /// Returns the same `TaskResponse` shape as `get_task`, following REST best practices
    /// where POST/create returns the same representation as GET/read.
    ///
    /// Note: Backpressure and circuit breaker checks should be done at the handler layer.
    pub async fn create_task(&self, request: TaskRequest) -> TaskServiceResult<TaskResponse> {
        // Input validation
        if request.name.is_empty() {
            return Err(TaskServiceError::Validation(
                "Task name cannot be empty".to_string(),
            ));
        }
        if request.namespace.is_empty() {
            return Err(TaskServiceError::Validation(
                "Namespace cannot be empty".to_string(),
            ));
        }
        if request.version.is_empty() {
            return Err(TaskServiceError::Validation(
                "Version cannot be empty".to_string(),
            ));
        }

        info!(
            namespace = %request.namespace,
            task_name = %request.name,
            version = %request.version,
            initiator = %request.initiator,
            source_system = %request.source_system,
            "Creating new task via TaskService"
        );

        // Set default values
        let mut task_request = request.clone();
        task_request.requested_at = chrono::Utc::now().naive_utc();

        // Initialize task
        let result = self
            .task_initializer
            .create_and_enqueue_task_from_request(task_request)
            .await;

        match result {
            Ok(task_result) => {
                info!(
                    task_uuid = %task_result.task_uuid,
                    step_count = task_result.step_count,
                    handler_config = ?task_result.handler_config_name,
                    "Task created successfully via TaskService"
                );

                // Fetch full task context for response (same shape as get_task)
                self.get_task(task_result.task_uuid).await
            }
            Err(init_error) => {
                // Check for duplicate task (unique constraint violation)
                let error_string = init_error.to_string();
                if error_string.contains("duplicate key value violates unique constraint")
                    && error_string.contains("idx_tasks_identity_hash")
                {
                    info!(
                        namespace = %request.namespace,
                        task_name = %request.name,
                        "Task creation rejected - duplicate identity hash"
                    );
                    return Err(TaskServiceError::DuplicateTask(
                        "A task with this identity already exists".to_string(),
                    ));
                }

                // Classify error
                if init_error.is_client_error() {
                    error!(
                        namespace = %request.namespace,
                        task_name = %request.name,
                        error = %init_error,
                        "Task creation failed - client error"
                    );

                    match init_error {
                        TaskInitializationError::ConfigurationNotFound(msg) => {
                            Err(TaskServiceError::TemplateNotFound(msg))
                        }
                        TaskInitializationError::InvalidConfiguration(msg) => {
                            Err(TaskServiceError::InvalidConfiguration(msg))
                        }
                        _ => Err(TaskServiceError::Internal(init_error.to_string())),
                    }
                } else {
                    error!(
                        namespace = %request.namespace,
                        task_name = %request.name,
                        error = %init_error,
                        "Task creation failed - server error"
                    );
                    Err(TaskServiceError::Database(init_error.to_string()))
                }
            }
        }
    }

    /// Get a task by UUID with full execution context.
    pub async fn get_task(&self, uuid: Uuid) -> TaskServiceResult<TaskResponse> {
        let twc = self
            .query_service
            .get_task_with_context(uuid)
            .await
            .map_err(|e| match e {
                TaskQueryError::NotFound(id) => TaskServiceError::NotFound(id),
                TaskQueryError::Database(e) => TaskServiceError::Database(e.to_string()),
                TaskQueryError::MetadataError(msg) => TaskServiceError::Database(msg),
            })?;

        Ok(TaskQueryService::to_task_response(&twc))
    }

    /// List tasks with pagination and execution context.
    pub async fn list_tasks(&self, query: TaskListQuery) -> TaskServiceResult<TaskListResponse> {
        // Validate pagination
        if query.per_page == 0 || query.per_page > 100 {
            return Err(TaskServiceError::Validation(
                "per_page must be between 1 and 100".to_string(),
            ));
        }
        if query.page == 0 {
            return Err(TaskServiceError::Validation(
                "page must be >= 1".to_string(),
            ));
        }

        let result = self
            .query_service
            .list_tasks_with_context(&query)
            .await
            .map_err(|e| TaskServiceError::Database(e.to_string()))?;

        let tasks: Vec<TaskResponse> = result
            .tasks
            .iter()
            .map(TaskQueryService::to_task_response)
            .collect();

        Ok(TaskListResponse {
            tasks,
            pagination: result.pagination,
        })
    }

    /// Cancel a task.
    pub async fn cancel_task(&self, uuid: Uuid) -> TaskServiceResult<TaskResponse> {
        let sql_executor = SqlFunctionExecutor::new(self.write_pool.clone());

        // Find the task
        let task = Task::find_by_id(&self.write_pool, uuid)
            .await
            .map_err(|e| TaskServiceError::Database(e.to_string()))?
            .ok_or(TaskServiceError::NotFound(uuid))?;

        // Check if task can be cancelled
        let can_cancel = task
            .can_be_cancelled(self.system_context.clone())
            .await
            .map_err(|e| TaskServiceError::Database(e.to_string()))?;

        if !can_cancel {
            return Err(TaskServiceError::CannotCancel(
                "Cannot cancel a completed task".to_string(),
            ));
        }

        // Cancel the task
        let mut task = task;
        task.cancel_task(self.system_context.clone())
            .await
            .map_err(|e| TaskServiceError::Database(e.to_string()))?;

        info!(task_uuid = %uuid, "Task cancelled via TaskService");

        // Build response with updated context
        let twc = self.build_task_with_context(task, &sql_executor).await?;
        Ok(TaskQueryService::to_task_response(&twc))
    }

    /// Build a TaskWithContext from a task and SQL executor.
    async fn build_task_with_context(
        &self,
        task: Task,
        sql_executor: &SqlFunctionExecutor,
    ) -> TaskServiceResult<TaskWithContext> {
        let uuid = task.task_uuid;

        // Get task metadata
        let (name, namespace, version, status) = task
            .get_task_metadata(&self.write_pool)
            .await
            .map_err(|e| TaskServiceError::Database(e.to_string()))?;

        // Get execution context
        let execution_context = sql_executor
            .get_task_execution_context(uuid)
            .await
            .map_err(|e| TaskServiceError::Database(e.to_string()))?
            .ok_or(TaskServiceError::NotFound(uuid))?;

        // Get step readiness status
        let steps = sql_executor
            .get_step_readiness_status(uuid, None)
            .await
            .map_err(|e| TaskServiceError::Database(e.to_string()))?;

        Ok(TaskWithContext {
            task,
            task_name: name,
            namespace,
            version,
            status,
            execution_context,
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- TaskServiceError Display messages ---

    #[test]
    fn test_error_display_validation() {
        let err = TaskServiceError::Validation("missing name".to_string());
        assert_eq!(err.to_string(), "Validation error: missing name");
    }

    #[test]
    fn test_error_display_not_found() {
        let uuid = Uuid::now_v7();
        let err = TaskServiceError::NotFound(uuid);
        assert!(err.to_string().contains(&uuid.to_string()));
    }

    #[test]
    fn test_error_display_template_not_found() {
        let err = TaskServiceError::TemplateNotFound("process_order".to_string());
        assert_eq!(err.to_string(), "Template not found: process_order");
    }

    #[test]
    fn test_error_display_invalid_configuration() {
        let err = TaskServiceError::InvalidConfiguration("bad version".to_string());
        assert_eq!(err.to_string(), "Invalid configuration: bad version");
    }

    #[test]
    fn test_error_display_duplicate_task() {
        let err = TaskServiceError::DuplicateTask("task-123".to_string());
        assert_eq!(err.to_string(), "Duplicate task: task-123");
    }

    #[test]
    fn test_error_display_cannot_cancel() {
        let err = TaskServiceError::CannotCancel("already complete".to_string());
        assert_eq!(
            err.to_string(),
            "Task cannot be cancelled: already complete"
        );
    }

    #[test]
    fn test_error_display_backpressure() {
        let err = TaskServiceError::Backpressure {
            reason: "queue full".to_string(),
            retry_after_seconds: 30,
        };
        assert!(err.to_string().contains("queue full"));
    }

    #[test]
    fn test_error_display_circuit_breaker_open() {
        let err = TaskServiceError::CircuitBreakerOpen;
        assert_eq!(err.to_string(), "Circuit breaker open");
    }

    #[test]
    fn test_error_display_database() {
        let err = TaskServiceError::Database("connection refused".to_string());
        assert_eq!(err.to_string(), "Database error: connection refused");
    }

    #[test]
    fn test_error_display_internal() {
        let err = TaskServiceError::Internal("unexpected".to_string());
        assert_eq!(err.to_string(), "Internal error: unexpected");
    }

    // --- is_client_error classification ---

    #[test]
    fn test_client_errors() {
        assert!(TaskServiceError::Validation("x".to_string()).is_client_error());
        assert!(TaskServiceError::NotFound(Uuid::now_v7()).is_client_error());
        assert!(TaskServiceError::TemplateNotFound("x".to_string()).is_client_error());
        assert!(TaskServiceError::InvalidConfiguration("x".to_string()).is_client_error());
        assert!(TaskServiceError::DuplicateTask("x".to_string()).is_client_error());
        assert!(TaskServiceError::CannotCancel("x".to_string()).is_client_error());
    }

    #[test]
    fn test_server_errors() {
        assert!(!TaskServiceError::Backpressure {
            reason: "x".to_string(),
            retry_after_seconds: 1,
        }
        .is_client_error());
        assert!(!TaskServiceError::CircuitBreakerOpen.is_client_error());
        assert!(!TaskServiceError::Database("x".to_string()).is_client_error());
        assert!(!TaskServiceError::Internal("x".to_string()).is_client_error());
    }

    // --- Debug output ---

    #[test]
    fn test_error_debug_format() {
        let err = TaskServiceError::Validation("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Validation"));
    }
}

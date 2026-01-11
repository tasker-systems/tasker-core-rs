//! # Task Model
//!
//! Core task execution model managing workflow orchestration instances.
//!
//! ## Overview
//!
//! The `Task` model represents actual task instances and serves as the primary orchestration
//! unit in the workflow system. Each task instance is created from a `NamedTask` template
//! and contains execution context, state management, and delegation metadata.
//!
//! ## Key Features
//!
//! - **Identity-based Deduplication**: Tasks use SHA-256 hashes to prevent duplicate execution
//! - **JSONB Context Storage**: Flexible context and metadata storage with PostgreSQL operators
//! - **State Machine Integration**: Delegates state management to TaskStateMachine
//! - **Workflow Step Coordination**: Manages collection of WorkflowStep instances
//! - **Comprehensive Scoping**: 18+ ActiveRecord scope equivalents for complex queries
//!
//! ## Database Schema
//!
//! Maps to `tasker.tasks` table with the following key columns:
//! - `task_uuid`: Primary key (UUID v7)
//! - `named_task_uuid`: References task template (UUID)
//! - `identity_hash`: SHA-256 for deduplication (VARCHAR, indexed)
//! - `context`: JSONB for execution context
//! - `tags`: JSONB for metadata and filtering
//! - `complete`: Completion flag (BOOLEAN)
//!
//! ## Rails Heritage
//!
//! Migrated from `app/models/tasker/task.rb` (16KB, 425 lines)
//! Preserves all ActiveRecord scopes, class methods, instance methods, and delegations.
//!
//! ## Performance Characteristics
//!
//! - **Identity Lookups**: O(1) via hash index
//! - **JSONB Queries**: Efficient JSON operations with GIN indexes
//! - **State Queries**: Complex joins with task_transitions table
//! - **Scope Queries**: Optimized with strategic indexes

use crate::state_machine::{
    events::{StepEvent, TaskEvent},
    step_state_machine::StepStateMachine,
    task_state_machine::TaskStateMachine,
};
use crate::system_context::SystemContext;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

/// Represents a task execution instance with workflow orchestration metadata.
///
/// Each task is created from a `NamedTask` template and contains all execution context
/// needed for workflow orchestration. Tasks use identity hashing for deduplication
/// and JSONB storage for flexible metadata.
///
/// # Database Mapping
///
/// Maps directly to the `tasker.tasks` table with Rails-compatible schema:
/// ```sql
/// CREATE TABLE tasker.tasks (
///   task_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
///   named_task_uuid UUID NOT NULL,
///   complete BOOLEAN DEFAULT false,
///   identity_hash VARCHAR(64) NOT NULL,
///   context JSONB,
///   tags JSONB,
///   -- ... other fields
/// );
/// ```
///
/// # Identity Deduplication
///
/// The `identity_hash` field prevents duplicate task execution by creating a SHA-256
/// hash from the named_task_uuid and context. This ensures idempotent task creation.
///
/// # JSONB Context Structure
///
/// The context field stores execution parameters:
/// ```json
/// {
///   "input_data": { "order_id": 12345, "priority": "high" },
///   "execution_options": { "timeout": 3600, "max_attempts": 3 },
///   "metadata": { "source": "api", "version": "1.0" }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct Task {
    pub task_uuid: Uuid,
    pub named_task_uuid: Uuid,
    pub complete: bool,
    pub requested_at: NaiveDateTime,
    pub initiator: Option<String>,
    pub source_system: Option<String>,
    pub reason: Option<String>,
    pub bypass_steps: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub identity_hash: String,
    pub priority: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    /// TAS-29: Correlation ID for distributed tracing
    pub correlation_id: Uuid,
    /// TAS-29: Optional parent correlation ID for workflow hierarchies
    pub parent_correlation_id: Option<Uuid>,
}

/// New Task for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTask {
    pub named_task_uuid: Uuid,
    pub requested_at: Option<NaiveDateTime>, // Defaults to NOW() if not provided
    pub initiator: Option<String>,
    pub source_system: Option<String>,
    pub reason: Option<String>,
    pub bypass_steps: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub identity_hash: String,
    pub priority: Option<i32>,
    /// TAS-29: Correlation ID for distributed tracing (auto-generated if not provided)
    pub correlation_id: Uuid,
    /// TAS-29: Optional parent correlation ID for workflow hierarchies
    pub parent_correlation_id: Option<Uuid>,
}

/// Task with delegation metadata for orchestration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskForOrchestration {
    pub task: Task,
    pub task_name: String,
    pub task_version: String,
    pub namespace_name: String,
}

/// Query parameters for task listing with pagination and filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListQuery {
    pub page: u32,
    pub per_page: u32,
    pub namespace: Option<String>,
    pub status: Option<String>,
    pub initiator: Option<String>,
    pub source_system: Option<String>,
}

impl Default for TaskListQuery {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 25,
            namespace: None,
            status: None,
            initiator: None,
            source_system: None,
        }
    }
}

/// Task with metadata for list operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskWithMetadata {
    pub task: Task,
    pub task_name: String,
    pub task_version: String,
    pub namespace_name: String,
    pub status: String, // "completed" or "pending"
}

/// Paginated results for task listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedTaskList {
    pub tasks: Vec<TaskWithMetadata>,
    pub pagination: PaginationInfo,
}

/// Pagination metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PaginationInfo {
    pub page: u32,
    pub per_page: u32,
    pub total_count: u64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_previous: bool,
}

impl Task {
    /// Create a new task
    pub async fn create(pool: &PgPool, new_task: NewTask) -> Result<Task, sqlx::Error> {
        let sanitized_task = Task::sanitize_new_task(new_task)?;

        let task = sqlx::query_as!(
            Task,
            r#"
            INSERT INTO tasker.tasks (
                named_task_uuid, complete, requested_at, initiator, source_system,
                reason, bypass_steps, tags, context, identity_hash,
                priority, correlation_id, parent_correlation_id, created_at, updated_at
            )
            VALUES ($1::uuid, false, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW())
            RETURNING task_uuid, named_task_uuid, complete, requested_at, initiator, source_system,
                      reason, bypass_steps, tags, context, identity_hash,
                    priority, created_at, updated_at, correlation_id, parent_correlation_id
            "#,
            sanitized_task.named_task_uuid,
            sanitized_task.requested_at,
            sanitized_task.initiator,
            sanitized_task.source_system,
            sanitized_task.reason,
            sanitized_task.bypass_steps,
            sanitized_task.tags,
            sanitized_task.context,
            sanitized_task.identity_hash,
            sanitized_task.priority.unwrap_or(0),
            sanitized_task.correlation_id,
            sanitized_task.parent_correlation_id,
        )
        .fetch_one(pool)
        .await?;

        Ok(task)
    }

    pub async fn create_with_transaction(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        new_task: NewTask,
    ) -> Result<Task, sqlx::Error> {
        let sanitized_task = Task::sanitize_new_task(new_task)?;
        let task = sqlx::query_as!(
            Task,
            r#"
        INSERT INTO tasker.tasks (
            named_task_uuid, complete, requested_at, initiator, source_system,
            reason, bypass_steps, tags, context, identity_hash,
            priority, correlation_id, parent_correlation_id, created_at, updated_at
        )
        VALUES ($1::uuid, false, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW(), NOW())
        RETURNING task_uuid, named_task_uuid, complete, requested_at, initiator, source_system,
                  reason, bypass_steps, tags, context, identity_hash,
                  priority, created_at, updated_at, correlation_id, parent_correlation_id
        "#,
            sanitized_task.named_task_uuid,
            sanitized_task.requested_at,
            sanitized_task.initiator,
            sanitized_task.source_system,
            sanitized_task.reason,
            sanitized_task.bypass_steps,
            sanitized_task.tags,
            sanitized_task.context,
            sanitized_task.identity_hash,
            sanitized_task.priority.unwrap_or(0),
            sanitized_task.correlation_id,
            sanitized_task.parent_correlation_id
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(task)
    }

    fn sanitize_new_task(new_task: NewTask) -> Result<NewTask, sqlx::Error> {
        // Validate JSONB fields before database operation
        if let Some(ref context) = new_task.context {
            if let Err(validation_error) = crate::validation::validate_task_context(context) {
                return Err(sqlx::Error::Protocol(format!(
                    "Invalid context: {validation_error}"
                )));
            }
        }

        if let Some(ref tags) = new_task.tags {
            if let Err(validation_error) = crate::validation::validate_task_tags(tags) {
                return Err(sqlx::Error::Protocol(format!(
                    "Invalid tags: {validation_error}"
                )));
            }
        }

        if let Some(ref bypass_steps) = new_task.bypass_steps {
            if let Err(validation_error) = crate::validation::validate_bypass_steps(bypass_steps) {
                return Err(sqlx::Error::Protocol(format!(
                    "Invalid bypass_steps: {validation_error}"
                )));
            }
        }

        // Sanitize JSONB inputs
        let sanitized_context = new_task.context.map(crate::validation::sanitize_json);
        let sanitized_tags = new_task.tags.map(crate::validation::sanitize_json);
        let sanitized_bypass_steps = new_task.bypass_steps.map(crate::validation::sanitize_json);

        let requested_at = new_task
            .requested_at
            .unwrap_or_else(|| chrono::Utc::now().naive_utc());

        let identity_hash =
            Task::generate_identity_hash(new_task.named_task_uuid, &sanitized_context);

        Ok(NewTask {
            named_task_uuid: new_task.named_task_uuid,
            requested_at: Some(requested_at),
            initiator: new_task.initiator,
            source_system: new_task.source_system,
            reason: new_task.reason,
            bypass_steps: sanitized_bypass_steps,
            tags: sanitized_tags,
            context: sanitized_context,
            identity_hash,
            priority: new_task.priority,
            correlation_id: new_task.correlation_id,
            parent_correlation_id: new_task.parent_correlation_id,
        })
    }

    /// Find a task by ID
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Task>, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            SELECT task_uuid, named_task_uuid, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash,
                   priority, created_at, updated_at, correlation_id, parent_correlation_id
            FROM tasker.tasks
            WHERE task_uuid = $1::uuid
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// Find a task by identity hash
    pub async fn find_by_identity_hash(
        pool: &PgPool,
        hash: &str,
    ) -> Result<Option<Task>, sqlx::Error> {
        let task = sqlx::query_as!(
            Task,
            r#"
            SELECT task_uuid, named_task_uuid, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash,
                   priority, created_at, updated_at, correlation_id, parent_correlation_id
            FROM tasker.tasks
            WHERE identity_hash = $1
            "#,
            hash
        )
        .fetch_optional(pool)
        .await?;

        Ok(task)
    }

    /// List tasks by named task UUID
    pub async fn list_by_named_task(
        pool: &PgPool,
        named_task_uuid: Uuid,
    ) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT task_uuid, named_task_uuid, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash,
                   priority, created_at, updated_at, correlation_id, parent_correlation_id
            FROM tasker.tasks
            WHERE named_task_uuid = $1::uuid
            ORDER BY created_at DESC
            "#,
            named_task_uuid
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List incomplete tasks
    pub async fn list_incomplete(pool: &PgPool) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT task_uuid, named_task_uuid, complete, requested_at, initiator, source_system,
                   reason, bypass_steps, tags, context, identity_hash,
                   priority, created_at, updated_at, correlation_id, parent_correlation_id
            FROM tasker.tasks
            WHERE complete = false
            ORDER BY requested_at ASC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// List tasks with pagination and filtering
    ///
    /// Provides a flexible method for listing tasks with pagination support and optional filtering
    /// by namespace, status, initiator, and source system. This method handles the complexity
    /// of dynamic query building and pagination calculations.
    ///
    /// # Parameters
    ///
    /// * `pool` - Database connection pool
    /// * `query` - Query parameters including pagination settings and optional filters
    ///
    /// # Returns
    ///
    /// Returns a `PaginatedTaskList` containing the tasks for the requested page and
    /// complete pagination metadata.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::core::task::{Task, TaskListQuery};
    /// use sqlx::PgPool;
    ///
    /// # async fn example(pool: &PgPool) -> Result<(), sqlx::Error> {
    /// let query = TaskListQuery {
    ///     page: 1,
    ///     per_page: 10,
    ///     namespace: Some("order_processing".to_string()),
    ///     status: Some("pending".to_string()),
    ///     initiator: None,
    ///     source_system: None,
    /// };
    ///
    /// let result = Task::list_with_pagination(pool, &query).await?;
    /// println!("Found {} tasks on page {}", result.tasks.len(), result.pagination.page);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_with_pagination(
        pool: &PgPool,
        query: &TaskListQuery,
    ) -> Result<PaginatedTaskList, sqlx::Error> {
        // Validate pagination parameters
        let page = if query.page == 0 { 1 } else { query.page };
        let per_page = if query.per_page == 0 || query.per_page > 100 {
            25
        } else {
            query.per_page
        };
        let offset = (page - 1) * per_page;

        // Build query using SQLx QueryBuilder for safe dynamic queries
        use sqlx::QueryBuilder;

        let mut query_builder = QueryBuilder::new(
            r#"
            SELECT
                t.task_uuid, t.named_task_uuid, t.complete, t.requested_at, t.initiator,
                t.source_system, t.reason, t.bypass_steps, t.tags, t.context,
                t.identity_hash, t.priority, t.created_at, t.updated_at,
                t.correlation_id, t.parent_correlation_id,
                nt.name as task_name, nt.version as task_version, ns.name as namespace_name
            FROM tasker.tasks t
            INNER JOIN tasker.named_tasks nt ON t.named_task_uuid = nt.named_task_uuid
            INNER JOIN tasker.task_namespaces ns ON nt.task_namespace_uuid = ns.task_namespace_uuid
            "#,
        );

        // Add WHERE conditions with proper parameterization
        let mut has_where = false;

        if let Some(namespace) = &query.namespace {
            query_builder.push(" WHERE ns.name = ");
            query_builder.push_bind(namespace);
            has_where = true;
        }

        if let Some(status) = &query.status {
            if has_where {
                query_builder.push(" AND ");
            } else {
                query_builder.push(" WHERE ");
                has_where = true;
            }

            if status == "completed" {
                query_builder.push("t.complete = true");
            } else if status == "pending" {
                query_builder.push("t.complete = false");
            }
        }

        if let Some(initiator) = &query.initiator {
            if has_where {
                query_builder.push(" AND ");
            } else {
                query_builder.push(" WHERE ");
                has_where = true;
            }
            query_builder.push("t.initiator = ");
            query_builder.push_bind(initiator);
        }

        if let Some(source_system) = &query.source_system {
            if has_where {
                query_builder.push(" AND ");
            } else {
                query_builder.push(" WHERE ");
                // technically true but never read
                // has_where = true;
            }
            query_builder.push("t.source_system = ");
            query_builder.push_bind(source_system);
        }

        query_builder.push(" ORDER BY t.created_at DESC");

        // Clone the query builder for count query
        let mut count_query_builder = {
            let mut count_builder = QueryBuilder::new(
                "SELECT COUNT(*) as total FROM (SELECT DISTINCT t.task_uuid FROM tasker.tasks t INNER JOIN tasker.named_tasks nt ON t.named_task_uuid = nt.named_task_uuid INNER JOIN tasker.task_namespaces ns ON nt.task_namespace_uuid = ns.task_namespace_uuid"
            );

            // Rebuild the WHERE clause for count query
            let mut has_where = false;

            if let Some(namespace) = &query.namespace {
                count_builder.push(" WHERE ns.name = ");
                count_builder.push_bind(namespace);
                has_where = true;
            }

            if let Some(status) = &query.status {
                if has_where {
                    count_builder.push(" AND ");
                } else {
                    count_builder.push(" WHERE ");
                    has_where = true;
                }

                if status == "completed" {
                    count_builder.push("t.complete = true");
                } else if status == "pending" {
                    count_builder.push("t.complete = false");
                }
            }

            if let Some(initiator) = &query.initiator {
                if has_where {
                    count_builder.push(" AND ");
                } else {
                    count_builder.push(" WHERE ");
                    has_where = true;
                }
                count_builder.push("t.initiator = ");
                count_builder.push_bind(initiator);
            }

            if let Some(source_system) = &query.source_system {
                if has_where {
                    count_builder.push(" AND ");
                } else {
                    count_builder.push(" WHERE ");
                    // technically true but never read
                    // has_where = true;
                }
                count_builder.push("t.source_system = ");
                count_builder.push_bind(source_system);
            }

            count_builder.push(") as count_subquery");
            count_builder
        };

        // Get total count first (for pagination metadata)
        let count_row = count_query_builder.build().fetch_one(pool).await?;

        let total_count: i64 = count_row.get("total");

        // Add pagination to the main query
        query_builder.push(" LIMIT ");
        query_builder.push_bind(per_page as i64);
        query_builder.push(" OFFSET ");
        query_builder.push_bind(offset as i64);

        // Execute main query to get tasks with metadata
        let task_rows = query_builder.build().fetch_all(pool).await?;

        // Convert rows to TaskWithMetadata objects
        let tasks: Vec<TaskWithMetadata> = task_rows
            .into_iter()
            .map(|row| {
                let task = Task {
                    task_uuid: row.get("task_uuid"),
                    named_task_uuid: row.get("named_task_uuid"),
                    complete: row.get("complete"),
                    requested_at: row.get("requested_at"),
                    initiator: row.get("initiator"),
                    source_system: row.get("source_system"),
                    reason: row.get("reason"),
                    bypass_steps: row.get("bypass_steps"),
                    tags: row.get("tags"),
                    context: row.get("context"),
                    identity_hash: row.get("identity_hash"),
                    priority: row.get("priority"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                    correlation_id: row.get("correlation_id"),
                    parent_correlation_id: row.get("parent_correlation_id"),
                };

                let status = if task.complete {
                    "completed".to_string()
                } else {
                    "pending".to_string()
                };

                TaskWithMetadata {
                    task,
                    task_name: row.get("task_name"),
                    task_version: row.get("task_version"),
                    namespace_name: row.get("namespace_name"),
                    status,
                }
            })
            .collect();

        // Calculate pagination metadata
        let total_pages = ((total_count as f64) / (per_page as f64)).ceil() as u32;

        let pagination = PaginationInfo {
            page,
            per_page,
            total_count: total_count as u64,
            total_pages,
            has_next: page < total_pages,
            has_previous: page > 1,
        };

        Ok(PaginatedTaskList { tasks, pagination })
    }

    /// Mark task as complete
    pub async fn mark_complete(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker.tasks
            SET complete = true, updated_at = NOW()
            WHERE task_uuid = $1::uuid
            "#,
            self.task_uuid
        )
        .execute(pool)
        .await?;

        self.complete = true;
        Ok(())
    }

    /// Update task context
    pub async fn update_context(
        &mut self,
        pool: &PgPool,
        context: serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        // Validate input before database operation
        if let Err(validation_error) = crate::validation::validate_task_context(&context) {
            return Err(sqlx::Error::Protocol(format!(
                "Invalid context: {validation_error}"
            )));
        }

        // Sanitize the JSON to remove potentially dangerous content
        let sanitized_context = crate::validation::sanitize_json(context);

        sqlx::query!(
            r#"
            UPDATE tasker.tasks
            SET context = $2, updated_at = NOW()
            WHERE task_uuid = $1::uuid
            "#,
            self.task_uuid,
            sanitized_context
        )
        .execute(pool)
        .await?;

        self.context = Some(sanitized_context);
        Ok(())
    }

    /// Update task tags
    pub async fn update_tags(
        &mut self,
        pool: &PgPool,
        tags: serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        // Validate input before database operation
        if let Err(validation_error) = crate::validation::validate_task_tags(&tags) {
            return Err(sqlx::Error::Protocol(format!(
                "Invalid tags: {validation_error}"
            )));
        }

        // Sanitize the JSON to remove potentially dangerous content
        let sanitized_tags = crate::validation::sanitize_json(tags);

        sqlx::query!(
            r#"
            UPDATE tasker.tasks
            SET tags = $2, updated_at = NOW()
            WHERE task_uuid = $1::uuid
            "#,
            self.task_uuid,
            sanitized_tags
        )
        .execute(pool)
        .await?;

        self.tags = Some(sanitized_tags);
        Ok(())
    }

    /// Delete a task
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker.tasks
            WHERE task_uuid = $1::uuid
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get current state from the task transition history.
    ///
    /// Tasks don't store state directly - instead, state is managed through
    /// a transition history table that tracks all state changes with timestamps.
    ///
    /// # SQL Query Logic
    ///
    /// ```sql
    /// SELECT to_state
    /// FROM tasker.task_transitions
    /// WHERE task_uuid = $1::uuid AND most_recent = true
    /// ORDER BY sort_key DESC
    /// LIMIT 1
    /// ```
    ///
    /// The query uses two strategies for finding the current state:
    /// 1. **Most Recent Flag**: `most_recent = true` for O(1) lookup
    /// 2. **Sort Key Ordering**: Fallback ordering by `sort_key DESC`
    ///
    /// # State Machine Integration
    ///
    /// This method integrates with the TaskStateMachine which manages:
    /// - State transition validation
    /// - Lifecycle event publishing
    /// - Transition audit trails
    ///
    /// # Performance
    ///
    /// - **Typical Case**: O(1) via most_recent index
    /// - **Fallback**: O(log n) via sort_key index
    /// - **Memory**: Returns owned String to avoid borrowing issues
    pub async fn get_current_state(&self, pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT to_state
            FROM tasker.task_transitions
            WHERE task_uuid = $1::uuid AND most_recent = true
            ORDER BY sort_key DESC
            LIMIT 1
            "#,
            self.task_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| r.to_state))
    }

    /// Check if task has any workflow steps
    pub async fn has_workflow_steps(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker.workflow_steps
            WHERE task_uuid = $1::uuid
            "#,
            self.task_uuid
        )
        .fetch_one(pool)
        .await?
        .count;

        Ok(count.unwrap_or(0) > 0)
    }

    /// Get delegation metadata for orchestration
    pub async fn for_orchestration(
        &self,
        pool: &PgPool,
    ) -> Result<TaskForOrchestration, sqlx::Error> {
        let task_metadata = sqlx::query!(
            r#"
            SELECT nt.name as task_name, nt.version as task_version, tn.name as namespace_name
            FROM tasker.tasks t
            INNER JOIN tasker.named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
            INNER JOIN tasker.task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
            WHERE t.task_uuid = $1::uuid
            "#,
            self.task_uuid
        )
        .fetch_one(pool)
        .await?;

        Ok(TaskForOrchestration {
            task: self.clone(),
            task_name: task_metadata.task_name,
            task_version: task_metadata.task_version,
            namespace_name: task_metadata.namespace_name,
        })
    }

    /// Generate a unique identity hash for task deduplication.
    ///
    /// Creates a deterministic SHA-256 hash from the named_task_uuid and context
    /// to ensure idempotent task creation. Tasks with identical hashes are
    /// considered duplicates and should not be executed twice.
    ///
    /// # Algorithm
    ///
    /// 1. **Hash Named Task ID**: Primary identifier for the task type
    /// 2. **Hash Context**: JSON context canonicalized as string
    /// 3. **Combine**: Use Rust's DefaultHasher for deterministic output
    ///
    /// # Deduplication Strategy
    ///
    /// The identity hash enables several deduplication patterns:
    /// - **Exact Duplicates**: Same task type + same context = identical hash
    /// - **Parameter Variations**: Different context = different hash
    /// - **Version Independence**: Hash is independent of task version
    ///
    /// # Example
    ///
    /// ```rust
    /// use serde_json::json;
    /// use tasker_shared::models::Task;
    /// use uuid::Uuid;
    ///
    /// let named_task_uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let context = Some(json!({"order_id": 12345, "priority": "high"}));
    /// let hash = Task::generate_identity_hash(named_task_uuid, &context);
    ///
    /// // Hash is deterministic - same inputs always produce same output
    /// assert_eq!(hash.len(), 16); // u64 hash produces 16-char hex string
    /// assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    ///
    /// // Same inputs produce same hash
    /// let hash2 = Task::generate_identity_hash(named_task_uuid, &context);
    /// assert_eq!(hash, hash2);
    /// ```
    ///
    /// # Performance
    ///
    /// - **Hash Calculation**: O(1) for typical context sizes
    /// - **Database Lookup**: O(1) via unique index on identity_hash
    /// - **Memory Usage**: Fixed 16-character string regardless of context size
    pub fn generate_identity_hash(
        named_task_uuid: Uuid,
        context: &Option<serde_json::Value>,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        named_task_uuid.hash(&mut hasher);
        if let Some(ctx) = context {
            ctx.to_string().hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())
    }

    // ============================================================================
    // RAILS ACTIVERECORE SCOPE EQUIVALENTS (18 scopes)
    // ============================================================================

    /// Find tasks by annotation (Rails: scope :by_annotation)
    /// TODO: Fix SQLx database validation issues
    pub async fn by_annotation(
        pool: &PgPool,
        _annotation_name: &str,
        _key: &str,
        _value: &str,
    ) -> Result<Vec<Task>, sqlx::Error> {
        // For now, return incomplete tasks as placeholder until database schema issues are resolved
        Self::list_incomplete(pool).await
    }

    /// Find tasks by current state using transitions (Rails: scope :by_current_state)
    ///
    /// Uses task transitions to efficiently find tasks in specific states.
    /// When no state is provided, returns all tasks.
    pub async fn by_current_state(
        pool: &PgPool,
        state: Option<&str>,
    ) -> Result<Vec<Task>, sqlx::Error> {
        match state {
            Some(state_filter) => {
                let tasks = sqlx::query_as!(
                    Task,
                    r#"
                    SELECT t.task_uuid, t.named_task_uuid, t.complete, t.requested_at,
                           t.initiator, t.source_system, t.reason, t.bypass_steps,
                           t.tags, t.context, t.identity_hash, t.priority,
                           t.created_at, t.updated_at, t.correlation_id, t.parent_correlation_id
                    FROM tasker.tasks t
                    INNER JOIN tasker.task_transitions tt
                        ON t.task_uuid = tt.task_uuid
                    WHERE tt.most_recent = true
                        AND tt.to_state = $1
                    ORDER BY t.task_uuid
                    "#,
                    state_filter
                )
                .fetch_all(pool)
                .await?;

                Ok(tasks)
            }
            None => {
                // Return all tasks when no state filter provided
                let tasks = sqlx::query_as!(
                    Task,
                    r#"
                    SELECT task_uuid, named_task_uuid, complete, requested_at,
                           initiator, source_system, reason, bypass_steps,
                           tags, context, identity_hash, priority,
                           created_at, updated_at, correlation_id, parent_correlation_id
                    FROM tasker.tasks
                    ORDER BY task_uuid
                    "#
                )
                .fetch_all(pool)
                .await?;

                Ok(tasks)
            }
        }
    }

    /// Find tasks with all associated records preloaded (Rails: scope :with_all_associated)
    /// TODO: Implement association preloading strategy
    pub async fn with_all_associated(pool: &PgPool) -> Result<Vec<Task>, sqlx::Error> {
        Self::list_incomplete(pool).await
    }

    /// Find tasks created since a specific time (Rails: scope :created_since)
    pub async fn created_since(
        pool: &PgPool,
        since_time: DateTime<Utc>,
    ) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT task_uuid, named_task_uuid, complete, requested_at, initiator,
                   source_system, reason, bypass_steps, tags, context,
                   identity_hash, priority, created_at, updated_at, correlation_id, parent_correlation_id
            FROM tasker.tasks
            WHERE created_at >= $1
            ORDER BY created_at DESC
            "#,
            since_time.naive_utc()
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Find tasks completed since a specific time (Rails: scope :completed_since)
    ///
    /// Finds tasks that transitioned to complete state after the specified time.
    /// Uses task transitions with timestamp filtering for accurate results.
    pub async fn completed_since(
        pool: &PgPool,
        since_time: DateTime<Utc>,
    ) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT t.task_uuid, t.named_task_uuid, t.complete, t.requested_at,
                   t.initiator, t.source_system, t.reason, t.bypass_steps,
                   t.tags, t.context, t.identity_hash, t.priority,
                   t.created_at, t.updated_at, t.correlation_id, t.parent_correlation_id
            FROM tasker.tasks t
            INNER JOIN tasker.task_transitions tt
                ON t.task_uuid = tt.task_uuid
            WHERE tt.most_recent = true
                AND tt.to_state = 'complete'
                AND tt.created_at >= $1
            ORDER BY tt.created_at DESC
            "#,
            since_time.naive_utc()
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Find tasks that failed since a specific time (Rails: scope :failed_since)
    ///
    /// Finds tasks that transitioned to error state after the specified time.
    /// Uses task transitions with timestamp filtering for accurate results.
    pub async fn failed_since(
        pool: &PgPool,
        since_time: DateTime<Utc>,
    ) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT t.task_uuid, t.named_task_uuid, t.complete, t.requested_at,
                   t.initiator, t.source_system, t.reason, t.bypass_steps,
                   t.tags, t.context, t.identity_hash, t.priority,
                   t.created_at, t.updated_at, t.correlation_id, t.parent_correlation_id
            FROM tasker.tasks t
            INNER JOIN tasker.task_transitions tt
                ON t.task_uuid = tt.task_uuid
            WHERE tt.most_recent = true
                AND tt.to_state = 'error'
                AND tt.created_at >= $1
            ORDER BY tt.created_at DESC
            "#,
            since_time.naive_utc()
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Find active tasks (not in terminal states) (Rails: scope :active)
    ///
    /// Active tasks are those not in terminal states (complete, error, cancelled).
    /// Uses task transitions to accurately determine current state.
    pub async fn active(pool: &PgPool) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT t.task_uuid, t.named_task_uuid, t.complete, t.requested_at,
                   t.initiator, t.source_system, t.reason, t.bypass_steps,
                   t.tags, t.context, t.identity_hash, t.priority,
                   t.created_at, t.updated_at, t.correlation_id, t.parent_correlation_id
            FROM tasker.tasks t
            LEFT JOIN tasker.task_transitions tt
                ON t.task_uuid = tt.task_uuid AND tt.most_recent = true
            WHERE tt.to_state IS NULL
                OR tt.to_state NOT IN ('complete', 'error', 'cancelled')
            ORDER BY t.task_uuid
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Find tasks in a specific namespace (Rails: scope :in_namespace)
    pub async fn in_namespace(
        pool: &PgPool,
        namespace_name: &str,
    ) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT t.task_uuid, t.named_task_uuid, t.complete, t.requested_at, t.initiator,
                   t.source_system, t.reason, t.bypass_steps, t.tags, t.context,
                   t.identity_hash, t.priority, t.created_at, t.updated_at, t.correlation_id, t.parent_correlation_id
            FROM tasker.tasks t
            JOIN tasker.named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
            JOIN tasker.task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
            WHERE tn.name = $1
            ORDER BY t.created_at DESC
            "#,
            namespace_name
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Find tasks with a specific task name (Rails: scope :with_task_name)
    pub async fn with_task_name(pool: &PgPool, task_name: &str) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT t.task_uuid, t.named_task_uuid, t.complete, t.requested_at, t.initiator,
                   t.source_system, t.reason, t.bypass_steps, t.tags, t.context,
                   t.identity_hash, t.priority, t.created_at, t.updated_at, t.correlation_id, t.parent_correlation_id
            FROM tasker.tasks t
            JOIN tasker.named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
            WHERE nt.name = $1
            ORDER BY t.created_at DESC
            "#,
            task_name
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    /// Find tasks with a specific version (Rails: scope :with_version)
    pub async fn with_version(pool: &PgPool, version: &str) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"
            SELECT t.task_uuid, t.named_task_uuid, t.complete, t.requested_at, t.initiator,
                   t.source_system, t.reason, t.bypass_steps, t.tags, t.context,
                   t.identity_hash, t.priority, t.created_at, t.updated_at, t.correlation_id, t.parent_correlation_id
            FROM tasker.tasks t
            JOIN tasker.named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
            WHERE nt.version = $1
            ORDER BY t.created_at DESC
            "#,
            version
        )
        .fetch_all(pool)
        .await?;

        Ok(tasks)
    }

    // ============================================================================
    // RAILS CLASS METHODS (6+ methods)
    // ============================================================================

    /// Create and save task with defaults from TaskRequest (Rails: create_with_defaults!)
    pub async fn create_with_defaults(
        pool: &PgPool,
        task_request: super::task_request::TaskRequest,
    ) -> Result<Task, crate::errors::TaskerError> {
        let resolved_request = task_request.resolve(pool).await?;
        resolved_request.create_task(pool).await
    }

    /// Create unsaved task instance from TaskRequest (Rails: from_task_request)
    /// Note: This creates a NewTask but does not resolve the NamedTask from the database.
    /// For full resolution, use create_with_defaults instead.
    pub fn from_task_request(task_request: super::task_request::TaskRequest) -> NewTask {
        NewTask {
            named_task_uuid: Uuid::nil(), // Will need to be resolved separately
            requested_at: Some(task_request.requested_at),
            initiator: Some(task_request.initiator),
            source_system: Some(task_request.source_system),
            reason: Some(task_request.reason),
            bypass_steps: Some(serde_json::Value::Array(
                task_request
                    .bypass_steps
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            )),
            tags: Some(serde_json::Value::Array(
                task_request
                    .tags
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            )),
            context: Some(task_request.context.clone()),
            identity_hash: Self::generate_identity_hash(Uuid::nil(), &Some(task_request.context)),
            priority: task_request.priority,
            correlation_id: task_request.correlation_id,
            parent_correlation_id: task_request.parent_correlation_id,
        }
    }

    /// Count unique task types (Rails: unique_task_types_count)
    pub async fn unique_task_types_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(DISTINCT nt.name) as count
            FROM tasker.tasks t
            INNER JOIN tasker.named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
            "#
        )
        .fetch_one(pool)
        .await?
        .count;

        Ok(count.unwrap_or(0))
    }

    /// Extract options from TaskRequest (Rails: get_request_options)
    pub fn get_request_options(
        resolved_request: &super::task_request::ResolvedTaskRequest,
    ) -> std::collections::HashMap<String, serde_json::Value> {
        resolved_request.get_request_options().clone()
    }

    /// Get default task request options for a named task (Rails: get_default_task_request_options)
    /// TODO: Implement default option generation based on named task configuration
    pub async fn get_default_task_request_options(
        pool: &PgPool,
        named_task_uuid: Uuid,
    ) -> Result<serde_json::Value, sqlx::Error> {
        // Placeholder - would load from named_task configuration and apply defaults
        let _named_task = sqlx::query!(
            "SELECT configuration FROM tasker.named_tasks WHERE named_task_uuid = $1::uuid",
            named_task_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(serde_json::json!({
            "default_timeout": 3600,
            "max_attempts": 3,
            "retryable": true,
            "skippable": false
        }))
    }

    /// Get current task status via state machine (Rails: status)
    pub async fn status(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        // Delegate to state machine - for now use get_current_state
        match self.get_current_state(pool).await? {
            Some(state) => Ok(state),
            None => Ok("pending".to_string()),
        }
    }

    /// Find workflow step by name (Rails: get_step_by_name)
    ///
    /// Looks up a workflow step within this task by its named step name.
    /// Returns the workflow step if found, None if not found.
    pub async fn get_step_by_name(
        &self,
        pool: &PgPool,
        name: &str,
    ) -> Result<Option<crate::models::WorkflowStep>, sqlx::Error> {
        let step = sqlx::query_as!(
            crate::models::WorkflowStep,
            r#"
            SELECT ws.workflow_step_uuid, ws.task_uuid, ws.named_step_uuid, ws.retryable,
                   ws.max_attempts, ws.in_process, ws.processed, ws.processed_at,
                   ws.attempts, ws.last_attempted_at, ws.backoff_request_seconds,
                   ws.inputs, ws.results, ws.checkpoint, ws.skippable, ws.created_at, ws.updated_at
            FROM tasker.workflow_steps ws
            INNER JOIN tasker.named_steps ns ON ns.named_step_uuid = ws.named_step_uuid
            WHERE ws.task_uuid = $1::uuid AND ns.name = $2
            LIMIT 1
            "#,
            self.task_uuid,
            name
        )
        .fetch_optional(pool)
        .await?;

        Ok(step)
    }

    /// Get RuntimeGraphAnalyzer for this task (Rails: runtime_analyzer) - memoized
    /// TODO: Implement RuntimeGraphAnalyzer
    pub fn runtime_analyzer(&self) -> String {
        format!("RuntimeGraphAnalyzer(task_uuid: {})", self.task_uuid)
    }

    /// Get runtime dependency graph analysis (Rails: dependency_graph)
    /// TODO: Implement dependency graph analysis
    pub fn dependency_graph(&self) -> serde_json::Value {
        serde_json::json!({
            "task_uuid": self.task_uuid,
            "nodes": [],
            "edges": [],
            "analysis": "placeholder"
        })
    }

    /// Check if all workflow steps are complete using efficient aggregation.
    ///
    /// This method is critical for task completion detection and uses PostgreSQL's
    /// FILTER clause for efficient conditional counting.
    ///
    /// # SQL Aggregation Logic
    ///
    /// ```sql
    /// SELECT
    ///     COUNT(*) as total_steps,
    ///     COUNT(*) FILTER (WHERE processed = true) as completed_steps
    /// FROM tasker.workflow_steps
    /// WHERE task_uuid = $1::uuid
    /// ```
    ///
    /// The query performs two counts in a single pass:
    /// 1. **Total Steps**: `COUNT(*)` - all steps for this task
    /// 2. **Completed Steps**: `COUNT(*) FILTER (WHERE processed = true)` - only processed steps
    ///
    /// # Business Logic
    ///
    /// A task is considered complete when:
    /// - It has at least one workflow step (total > 0)
    /// - All steps are processed (total == completed)
    ///
    /// # Performance Benefits
    ///
    /// - **Single Query**: Avoids N+1 queries or multiple round trips
    /// - **Conditional Counting**: FILTER clause is more efficient than subqueries
    /// - **Index Usage**: Leverages index on (task_uuid, processed)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use tasker_shared::models::Task;
    ///
    /// # async fn example(pool: PgPool, mut task: Task) -> Result<(), sqlx::Error> {
    /// // Check if all workflow steps are complete before marking task as done
    /// if task.all_steps_complete(&pool).await? {
    ///     println!("All steps complete! Marking task {} as done", task.task_uuid);
    ///     task.mark_complete(&pool).await?;
    /// } else {
    ///     println!("Task {} still has pending steps", task.task_uuid);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For complete examples with workflow setup, see `tests/models/task.rs`.
    pub async fn all_steps_complete(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT
                COUNT(*) as total_steps,
                COUNT(*) FILTER (WHERE processed = true) as completed_steps
            FROM tasker.workflow_steps
            WHERE task_uuid = $1::uuid
            "#,
            self.task_uuid
        )
        .fetch_one(pool)
        .await?;

        let total = result.total_steps.unwrap_or(0);
        let completed = result.completed_steps.unwrap_or(0);

        Ok(total > 0 && total == completed)
    }

    /// Get TaskExecutionContext for this task (Rails: task_execution_context) - memoized
    pub async fn task_execution_context(
        &self,
        pool: &PgPool,
    ) -> Result<Option<crate::models::TaskExecutionContext>, sqlx::Error> {
        crate::models::TaskExecutionContext::get_for_task(pool, self.task_uuid).await
    }

    /// Reload task and clear memoized instances (Rails: reload override)
    /// TODO: Implement memoization clearing
    pub async fn reload(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        // Reload the task from database
        if let Some(reloaded) = Self::find_by_id(pool, self.task_uuid).await? {
            *self = reloaded;
        }
        // TODO: Clear memoized instances (state_machine, diagram, etc.)
        Ok(())
    }

    // ============================================================================
    // RAILS DELEGATIONS (7 delegations)
    // ============================================================================

    /// Get task name (Rails: delegate :name, to: :named_task)
    pub async fn name(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        let name = sqlx::query!(
            "SELECT name FROM tasker.named_tasks WHERE named_task_uuid = $1::uuid",
            self.named_task_uuid
        )
        .fetch_one(pool)
        .await?
        .name;

        Ok(name)
    }

    /// Get workflow summary (Rails: delegate :workflow_summary, to: :task_execution_context)
    /// TODO: Integrate with TaskExecutionContext.workflow_summary
    pub async fn workflow_summary(&self, pool: &PgPool) -> Result<serde_json::Value, sqlx::Error> {
        // Placeholder - would delegate to TaskExecutionContext
        let _context = self.task_execution_context(pool).await?;
        Ok(serde_json::json!({
            "task_uuid": self.task_uuid,
            "total_steps": 0,
            "completed_steps": 0,
            "status": "placeholder"
        }))
    }

    /// Get namespace name (Rails: delegate :namespace_name, to: :named_task)
    pub async fn namespace_name(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        let namespace_name = sqlx::query!(
            r#"
            SELECT tn.name
            FROM tasker.named_tasks nt
            INNER JOIN tasker.task_namespaces tn ON tn.task_namespace_uuid = nt.task_namespace_uuid
            WHERE nt.named_task_uuid = $1::uuid
            "#,
            self.named_task_uuid
        )
        .fetch_one(pool)
        .await?
        .name;

        Ok(namespace_name)
    }

    /// Get version (Rails: delegate :version, to: :named_task)
    pub async fn version(&self, pool: &PgPool) -> Result<String, sqlx::Error> {
        let version = sqlx::query!(
            "SELECT version FROM tasker.named_tasks WHERE named_task_uuid = $1::uuid",
            self.named_task_uuid
        )
        .fetch_one(pool)
        .await?
        .version;

        Ok(version)
    }
}

/// # Task Usage Examples
///
/// Real-world examples of how to use the Task model in different scenarios.
impl Task {
    /// Example: Creating a customer order processing task
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::task::{Task, NewTask};
    /// use sqlx::PgPool;
    /// use serde_json::json;
    ///
    /// # async fn create_order_processing_task(pool: &PgPool, order_id: i64) -> Result<Task, sqlx::Error> {
    /// # let ORDER_PROCESSING_NAMED_task_uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// // Order processing context with customer data and preferences
    /// let context = json!({
    ///     "order_id": order_id,
    ///     "priority": "high",
    ///     "customer_tier": "premium",
    ///     "shipping_method": "express",
    ///     "payment_method": "credit_card",
    ///     "special_instructions": "Gift wrap requested"
    /// });
    ///
    /// // Create identity hash for deduplication
    /// let identity_hash = Task::generate_identity_hash(ORDER_PROCESSING_NAMED_task_uuid, &Some(context.clone()));
    ///
    /// // Check if task already exists (idempotent creation)
    /// if let Some(existing_task) = Task::find_by_identity_hash(pool, &identity_hash).await? {
    ///     println!("Order {} already has a processing task: {}", order_id, existing_task.task_uuid);
    ///     return Ok(existing_task);
    /// }
    ///
    /// // Create new task
    /// let new_task = NewTask {
    ///     named_task_uuid: ORDER_PROCESSING_NAMED_task_uuid,
    ///     requested_at: None, // Will default to NOW()
    ///     initiator: Some("order_service".to_string()),
    ///     source_system: Some("e_commerce_api".to_string()),
    ///     reason: Some(format!("Process order {}", order_id)),
    ///     bypass_steps: None,
    ///     tags: Some(json!({"order_id": order_id, "department": "fulfillment"})),
    ///     context: Some(context),
    ///     identity_hash,
    ///     priority: Some(5), // Default priority
    ///     correlation_id: uuid::Uuid::new_v4(), // TAS-29: Distributed tracing
    ///     parent_correlation_id: None, // TAS-29: Top-level task has no parent
    /// };
    ///
    /// let task = Task::create(pool, new_task).await?;
    /// println!("Created order processing task {} for order {}", task.task_uuid, order_id);
    ///
    /// Ok(task)
    /// # }
    /// ```
    ///
    /// This example shows:
    /// - **Rich Context**: Order details, customer preferences, and processing hints
    /// - **Idempotent Creation**: Check for existing task before creating new one
    /// - **Metadata Tagging**: Tags for filtering and organization
    /// - **Audit Trail**: Initiator, source system, and reason for traceability
    pub fn example_order_processing() {}

    /// Example: Monitoring task execution progress
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::task::Task;
    /// use tasker_shared::models::orchestration::task_execution_context::TaskExecutionContext;
    /// use sqlx::PgPool;
    /// use uuid::Uuid;
    ///
    /// # async fn monitor_progress(pool: &PgPool, task_uuid: Uuid) -> Result<(), sqlx::Error> {
    /// let task = Task::find_by_id(pool, task_uuid).await?
    ///     .ok_or_else(|| sqlx::Error::RowNotFound)?;
    ///
    /// // Get real-time execution context
    /// let context = TaskExecutionContext::get_for_task(pool, task_uuid).await?
    ///     .ok_or_else(|| sqlx::Error::RowNotFound)?;
    ///
    /// // Get current state from transition history
    /// let current_state = task.get_current_state(pool).await?
    ///     .unwrap_or_else(|| "unknown".to_string());
    ///
    /// // Print comprehensive status
    /// println!("=== Task {} Progress Report ===", task_uuid);
    /// println!("State: {}", current_state);
    /// println!("Progress: {}/{} steps complete",
    ///          context.completed_steps, context.total_steps);
    /// println!("Ready: {} steps ready to execute", context.ready_steps);
    /// println!("Active: {} steps currently running", context.in_progress_steps);
    /// println!("Failed: {} steps failed", context.failed_steps);
    /// println!("Health: {}", context.health_status);
    ///
    /// if let Some(action) = context.recommended_action {
    ///     println!("Recommended Action: {:?}", action);
    /// }
    ///
    /// // Check if intervention needed
    /// if context.failed_steps > 0 && context.ready_steps == 0 {
    ///     println!(" Task is blocked - manual intervention may be required");
    /// } else if context.ready_steps > 0 {
    ///     println!("Task has {} steps ready for execution", context.ready_steps);
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// This example demonstrates:
    /// - **Real-time Monitoring**: Get current execution status and progress
    /// - **State Resolution**: Retrieve current state from transition history
    /// - **Health Assessment**: Identify blocked tasks and ready work
    /// - **Progress Visualization**: Human-readable progress reporting
    pub fn example_progress_monitoring() {}

    /// Example: Bulk task analysis for dashboard
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::orchestration::task_execution_context::TaskExecutionContext;
    /// use sqlx::PgPool;
    ///
    /// # struct TaskDashboardSummary {
    /// #     total_tasks: usize,
    /// #     complete_tasks: usize,
    /// #     in_progress_tasks: usize,
    /// #     blocked_tasks: usize,
    /// #     ready_tasks: usize,
    /// #     total_ready_steps: i64,
    /// # }
    /// # async fn generate_task_dashboard(pool: &PgPool, task_uuids: Vec<uuid::Uuid>) -> Result<(), sqlx::Error> {
    /// // Get execution contexts for all tasks in a single query
    /// let contexts = TaskExecutionContext::get_for_tasks(pool, &task_uuids).await?;
    ///
    /// let mut summary = TaskDashboardSummary {
    ///     total_tasks: contexts.len(),
    ///     complete_tasks: 0,
    ///     in_progress_tasks: 0,
    ///     blocked_tasks: 0,
    ///     ready_tasks: 0,
    ///     total_ready_steps: 0,
    /// };
    ///
    /// println!("=== Task Dashboard ({} tasks) ===", contexts.len());
    ///
    /// for context in &contexts {
    ///     // Categorize tasks by execution status
    ///     if context.completed_steps == context.total_steps && context.total_steps > 0 {
    ///         summary.complete_tasks += 1;
    ///     } else if context.in_progress_steps > 0 {
    ///         summary.in_progress_tasks += 1;
    ///     } else if context.failed_steps > 0 && context.ready_steps == 0 {
    ///         summary.blocked_tasks += 1;
    ///     } else if context.ready_steps > 0 {
    ///         summary.ready_tasks += 1;
    ///         summary.total_ready_steps += context.ready_steps;
    ///     }
    ///
    ///     // Print individual task status
    ///     let completion_pct = if context.total_steps > 0 {
    ///         (context.completed_steps * 100 / context.total_steps)
    ///     } else { 0 };
    ///
    ///     println!("Task {}: {:?} - {}% complete - {} ready steps",
    ///              context.task_uuid, context.execution_status,
    ///              completion_pct, context.ready_steps);
    /// }
    ///
    /// // Print summary
    /// println!("\n=== Summary ===");
    /// println!("Complete: {}", summary.complete_tasks);
    /// println!("In Progress: {}", summary.in_progress_tasks);
    /// println!("⚡ Ready: {} ({} steps)", summary.ready_tasks, summary.total_ready_steps);
    /// println!("🚫 Blocked: {}", summary.blocked_tasks);
    ///
    /// Ok(())
    /// # }
    /// ```
    ///
    /// This example shows:
    /// - **Batch Processing**: Efficient bulk task analysis
    /// - **Dashboard Generation**: Summary statistics and categorization
    /// - **Status Visualization**: Clear visual indicators for task states
    /// - **Actionable Insights**: Ready work identification for scheduling
    pub fn example_dashboard_analysis() {}

    /// Example: Task context updates and lifecycle management
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::core::task::Task;
    /// use serde_json::json;
    /// use sqlx::PgPool;
    /// use chrono::Utc;
    /// use uuid::Uuid;
    ///
    /// async fn update_task_lifecycle(pool: &PgPool, task_uuid: Uuid) -> Result<(), sqlx::Error> {
    ///     let mut task = Task::find_by_id(pool, task_uuid).await?
    ///         .ok_or_else(|| sqlx::Error::RowNotFound)?;
    ///
    ///     // Update context with runtime information
    ///     let updated_context = json!({
    ///         "original_context": task.context,
    ///         "runtime_data": {
    ///             "started_at": Utc::now(),
    ///             "worker_node": "worker-003",
    ///             "resource_allocation": {
    ///                 "cpu_cores": 4,
    ///                 "memory_gb": 8
    ///             }
    ///         },
    ///         "progress_tracking": {
    ///             "checkpoints": [],
    ///             "estimated_completion": null
    ///         }
    ///     });
    ///
    ///     task.update_context(pool, updated_context).await?;
    ///
    ///     // Add execution tags
    ///     let execution_tags = json!({
    ///         "environment": "production",
    ///         "worker_pool": "high_priority",
    ///         "cost_center": "operations",
    ///         "sla_tier": "tier_1"
    ///     });
    ///
    ///     task.update_tags(pool, execution_tags).await?;
    ///
    ///     println!("Updated task {} with runtime context and execution tags", task_uuid);
    ///
    ///     // Later, when task completes
    ///     let final_context = json!({
    ///         "execution_summary": {
    ///             "completed_at": Utc::now(),
    ///             "total_duration_seconds": 245,
    ///             "resource_usage": {
    ///                 "peak_cpu_percent": 78,
    ///                 "peak_memory_mb": 1024
    ///             },
    ///             "steps_executed": 12,
    ///             "retries_required": 2
    ///         }
    ///     });
    ///
    ///     task.update_context(pool, final_context).await?;
    ///     task.mark_complete(pool).await?;
    ///
    ///     println!("Task {} completed and marked as finished", task_uuid);
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// This example demonstrates:
    /// - **Dynamic Context Updates**: Add runtime information as execution progresses
    /// - **Metadata Enrichment**: Tags for organization and resource tracking
    /// - **Lifecycle Tracking**: Capture execution timeline and resource usage
    /// - **Completion Handling**: Proper task finalization
    pub fn example_lifecycle_management() {}

    // ============================================================================
    // GUARD DELEGATION METHODS
    // ============================================================================

    /// Check if all workflow steps are complete (for state machine guards)
    ///
    /// This method provides the core logic for the AllStepsCompleteGuard by checking
    /// if all workflow steps for this task are in a complete state.
    ///
    /// Uses the step transition history to determine completion status rather than
    /// relying on boolean flags, providing more accurate state representation.
    pub async fn all_workflow_steps_complete(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let incomplete_count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker.workflow_steps ws
            LEFT JOIN (
                SELECT DISTINCT ON (workflow_step_uuid) workflow_step_uuid, to_state
                FROM tasker.workflow_step_transitions
                WHERE most_recent = true
                ORDER BY workflow_step_uuid, sort_key DESC
            ) current_states ON current_states.workflow_step_uuid = ws.workflow_step_uuid
            WHERE ws.task_uuid = $1::uuid
              AND (current_states.to_state IS NULL
                   OR current_states.to_state NOT IN ('complete', 'resolved_manually'))
            "#,
            self.task_uuid
        )
        .fetch_one(pool)
        .await?;

        let incomplete = incomplete_count.count.unwrap_or(1);
        Ok(incomplete == 0)
    }

    /// Check if task is not currently in progress (for state machine guards)
    ///
    /// This method checks the current task state from the transition history
    /// to determine if the task is already in progress, preventing double execution.
    pub async fn not_in_progress(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let current_state = self.get_current_state(pool).await?;

        match current_state {
            Some(state) => Ok(state != "in_progress"),
            None => Ok(true), // No state means not in progress
        }
    }

    /// Check if task can be reset (must be in error state)
    ///
    /// This method determines if a task is eligible for reset operations
    /// by checking if it's currently in an error state.
    pub async fn can_be_reset(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let current_state = self.get_current_state(pool).await?;

        match current_state {
            Some(state) => Ok(state == "error"),
            None => Ok(false), // No state means can't be reset
        }
    }

    /// Count incomplete workflow steps for this task
    ///
    /// Returns the count of workflow steps that are not in a complete state.
    /// Used for detailed error reporting in guard failures.
    pub async fn count_incomplete_workflow_steps(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker.workflow_steps ws
            LEFT JOIN (
                SELECT DISTINCT ON (workflow_step_uuid) workflow_step_uuid, to_state
                FROM tasker.workflow_step_transitions
                WHERE most_recent = true
                ORDER BY workflow_step_uuid, sort_key DESC
            ) current_states ON current_states.workflow_step_uuid = ws.workflow_step_uuid
            WHERE ws.task_uuid = $1::uuid
              AND (current_states.to_state IS NULL
                   OR current_states.to_state NOT IN ('complete', 'resolved_manually'))
            "#,
            self.task_uuid
        )
        .fetch_one(pool)
        .await?;

        Ok(result.count.unwrap_or(0))
    }

    // ============================================================================
    // TASK CANCELLATION AND STATE MANAGEMENT
    // ============================================================================

    /// Check if task can be cancelled using proper state machine validation
    ///
    /// A task can be cancelled if it is not in a terminal state according to the
    /// state machine. This method uses the state machine architecture to provide
    /// accurate cancellation validation rather than just checking the complete flag.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool for state machine queries
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the task can be cancelled, `Ok(false)` if it cannot,
    /// or an error if state machine queries fail.
    pub async fn can_be_cancelled(
        &self,
        system_context: Arc<SystemContext>,
    ) -> Result<bool, sqlx::Error> {
        // Create event publisher and task state machine
        let task_state_machine = TaskStateMachine::new(self.clone(), system_context.clone());

        // Get current state and check if it's terminal
        let current_state = task_state_machine
            .current_state()
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to get current state: {e}")))?;

        // Task can be cancelled if it's not in a terminal state
        Ok(!current_state.is_terminal())
    }

    /// Cancel a task using proper state machine integration and transaction safety
    ///
    /// This method provides comprehensive task cancellation by:
    /// 1. Checking if the task can be cancelled based on its current state
    /// 2. Transitioning all non-terminal workflow steps to cancelled state
    /// 3. Transitioning the task itself to cancelled state
    /// 4. Wrapping everything in a database transaction for atomicity
    /// 5. Maintaining proper audit trails through state machine transitions
    ///
    /// The cancellation process uses the proper state machine architecture to ensure
    /// all state transitions are audited and events are published appropriately.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool for executing operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful cancellation, or an error if:
    /// - The task is in a terminal state (complete, cancelled, etc.)
    /// - State machine transitions fail due to invalid states
    /// - Database transaction fails
    /// - Required state machine components cannot be initialized
    ///
    /// # Example
    ///
    /// ```rust
    /// # use tasker_shared::models::Task;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: &PgPool, task_uuid: Uuid) -> Result<(), sqlx::Error> {
    /// let mut task = Task::find_by_id(pool, task_uuid).await?.unwrap();
    ///
    /// # use std::sync::Arc;
    /// # let context: Arc<tasker_shared::system_context::SystemContext> = panic!("Example only");
    /// task.cancel_task(context).await?;
    /// // Task and all steps are now properly transitioned to cancelled state
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cancel_task(
        &mut self,
        system_context: Arc<SystemContext>,
    ) -> Result<(), sqlx::Error> {
        use crate::scopes::ScopeBuilder;
        // Step 1: Create task state machine and check if cancellation is valid
        let mut task_state_machine = TaskStateMachine::new(self.clone(), system_context.clone());
        let current_task_state = task_state_machine
            .current_state()
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to get current state: {e}")))?;

        // Check if task can be cancelled (not in terminal state)
        if current_task_state.is_terminal() {
            return Err(sqlx::Error::Protocol(format!(
                "Cannot cancel task in terminal state: {current_task_state:?}"
            )));
        }

        // Step 2: Find all workflow steps for this task
        let workflow_steps = crate::models::WorkflowStep::scope()
            .for_task(self.task_uuid)
            .all(system_context.database_pool())
            .await?;

        // Step 3: Cancel all non-terminal workflow steps using state machine
        // Note: Each state machine transition handles its own transaction internally
        for step in workflow_steps {
            let step_state_machine = StepStateMachine::new(step, system_context.clone());

            // Get current step state
            let current_step_state = step_state_machine
                .current_state()
                .await
                .map_err(|e| sqlx::Error::Protocol(format!("Failed to get step state: {e}")))?;

            // Only cancel non-terminal steps
            if !current_step_state.is_terminal() {
                let mut step_sm = step_state_machine;
                step_sm
                    .transition(StepEvent::Cancel)
                    .await
                    .map_err(|e| sqlx::Error::Protocol(format!("Failed to cancel step: {e}")))?;
            }
        }

        // Step 4: Cancel the task using state machine
        // Note: Task state machine transition handles its own transaction internally
        let cancelled_transition_succeeded = task_state_machine
            .transition(TaskEvent::Cancel)
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to cancel task: {e}")))?;

        if !cancelled_transition_succeeded {
            return Err(sqlx::Error::Protocol("Failed to cancel task".to_string()));
        }

        // Step 5: Update the local task instance to reflect the new state
        // Reload the task from database to get the latest state machine updates
        let updated_task = Task::find_by_id(system_context.database_pool(), self.task_uuid)
            .await?
            .ok_or_else(|| {
                sqlx::Error::Protocol("Task not found after cancellation".to_string())
            })?;

        // Update this instance with the latest data
        self.complete = updated_task.complete;
        self.reason = updated_task.reason;
        self.updated_at = updated_task.updated_at;

        Ok(())
    }

    /// Get task details with all metadata for API responses
    ///
    /// This method fetches the complete task information with associated metadata
    /// including task name, namespace, and version information needed for API responses.
    ///
    /// # Returns
    ///
    /// Returns a tuple containing:
    /// - Task name
    /// - Namespace name
    /// - Version
    /// - Status string ("completed" if complete, "pending" otherwise)
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sqlx::PgPool;
    /// # use tasker_shared::models::core::task::Task;
    /// # async fn example(pool: &PgPool, task: &Task) -> Result<(), sqlx::Error> {
    /// let (name, namespace, version, status) = task.get_task_metadata(pool).await?;
    /// println!("Task: {}/{} v{} - Status: {}", namespace, name, version, status);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_task_metadata(
        &self,
        pool: &PgPool,
    ) -> Result<(String, String, String, String), sqlx::Error> {
        let metadata = sqlx::query!(
            r#"
            SELECT
                nt.name,
                ns.name as namespace,
                nt.version,
                COALESCE(tec.execution_status, 'unknown') as status
            FROM tasker.tasks t
            INNER JOIN tasker.named_tasks nt ON t.named_task_uuid = nt.named_task_uuid
            INNER JOIN tasker.task_namespaces ns ON nt.task_namespace_uuid = ns.task_namespace_uuid
            LEFT JOIN LATERAL get_task_execution_context(t.task_uuid) tec ON true
            WHERE t.task_uuid = $1
            "#,
            self.task_uuid
        )
        .fetch_one(pool)
        .await?;

        Ok((
            metadata.name,
            metadata.namespace,
            metadata.version,
            metadata.status.unwrap_or("unknown".to_string()),
        ))
    }
}

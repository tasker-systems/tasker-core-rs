use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// WorkerNamedTask represents explicit associations between workers and named tasks they support
/// Maps to `tasker_worker_named_tasks` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkerNamedTask {
    pub id: i32,
    pub worker_id: i32,
    pub named_task_id: i32,
    pub configuration: serde_json::Value,
    pub priority: Option<i32>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New WorkerNamedTask for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkerNamedTask {
    pub worker_id: i32,
    pub named_task_id: i32,
    pub configuration: Option<serde_json::Value>,
    pub priority: Option<i32>,
}

/// WorkerNamedTask with joined data for worker selection
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkerNamedTaskWithDetails {
    pub id: i32,
    pub worker_id: i32,
    pub named_task_id: i32,
    pub configuration: serde_json::Value,
    pub priority: i32,
    pub worker_name: String,
    pub task_name: String,
    pub task_version: String,
    pub namespace_name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl WorkerNamedTask {
    /// Create a new worker-named task association
    pub async fn create(
        pool: &PgPool,
        new_association: NewWorkerNamedTask,
    ) -> Result<WorkerNamedTask, sqlx::Error> {
        let configuration = new_association
            .configuration
            .unwrap_or_else(|| serde_json::json!({}));
        let priority = new_association.priority.unwrap_or(100);

        let association = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            INSERT INTO tasker_worker_named_tasks (worker_id, named_task_id, configuration, priority, created_at, updated_at)
            VALUES ($1, $2, $3, $4, NOW(), NOW())
            RETURNING id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            "#,
            new_association.worker_id,
            new_association.named_task_id,
            configuration,
            priority
        )
        .fetch_one(pool)
        .await?;

        Ok(association)
    }

    /// Create or update worker-named task association (upsert pattern)
    pub async fn create_or_update(
        pool: &PgPool,
        worker_id: i32,
        named_task_id: i32,
        configuration: serde_json::Value,
        priority: Option<i32>,
    ) -> Result<WorkerNamedTask, sqlx::Error> {
        let association = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            INSERT INTO tasker_worker_named_tasks (worker_id, named_task_id, configuration, priority, created_at, updated_at)
            VALUES ($1, $2, $3, COALESCE($4, 100), NOW(), NOW())
            ON CONFLICT (worker_id, named_task_id) DO UPDATE SET
                configuration = EXCLUDED.configuration,
                priority = COALESCE(EXCLUDED.priority, 100),
                updated_at = NOW()
            RETURNING id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            "#,
            worker_id,
            named_task_id,
            configuration,
            priority
        )
        .fetch_one(pool)
        .await?;

        Ok(association)
    }

    /// Find association by ID
    pub async fn find_by_id(
        pool: &PgPool,
        id: i32,
    ) -> Result<Option<WorkerNamedTask>, sqlx::Error> {
        let association = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            SELECT id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            FROM tasker_worker_named_tasks
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(association)
    }

    /// Find association by worker and named task
    pub async fn find_by_worker_and_task(
        pool: &PgPool,
        worker_id: i32,
        named_task_id: i32,
    ) -> Result<Option<WorkerNamedTask>, sqlx::Error> {
        let association = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            SELECT id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            FROM tasker_worker_named_tasks
            WHERE worker_id = $1 AND named_task_id = $2
            "#,
            worker_id,
            named_task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(association)
    }

    /// List all associations for a worker
    pub async fn list_by_worker(
        pool: &PgPool,
        worker_id: i32,
    ) -> Result<Vec<WorkerNamedTask>, sqlx::Error> {
        let associations = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            SELECT id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            FROM tasker_worker_named_tasks
            WHERE worker_id = $1
            ORDER BY priority DESC, created_at
            "#,
            worker_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// List all associations for a named task
    pub async fn list_by_named_task(
        pool: &PgPool,
        named_task_id: i32,
    ) -> Result<Vec<WorkerNamedTask>, sqlx::Error> {
        let associations = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            SELECT id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            FROM tasker_worker_named_tasks
            WHERE named_task_id = $1
            ORDER BY priority DESC, created_at
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// List associations with full details for worker selection
    pub async fn list_with_details_by_task_info(
        pool: &PgPool,
        namespace: &str,
        task_name: &str,
        task_version: &str,
    ) -> Result<Vec<WorkerNamedTaskWithDetails>, sqlx::Error> {
        let associations = sqlx::query_as!(
            WorkerNamedTaskWithDetails,
            r#"
            SELECT 
                wnt.id,
                wnt.worker_id,
                wnt.named_task_id,
                wnt.configuration,
                COALESCE(wnt.priority, 100) as "priority!",
                wnt.created_at,
                wnt.updated_at,
                w.worker_name,
                nt.name as task_name,
                nt.version as task_version,
                tn.name as namespace_name
            FROM tasker_worker_named_tasks wnt
            INNER JOIN tasker_workers w ON wnt.worker_id = w.worker_id
            INNER JOIN tasker_named_tasks nt ON wnt.named_task_id = nt.named_task_id
            INNER JOIN tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
            WHERE tn.name = $1 AND nt.name = $2 AND nt.version = $3
            ORDER BY wnt.priority DESC, wnt.created_at
            "#,
            namespace,
            task_name,
            task_version
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Update association configuration
    pub async fn update_configuration(
        pool: &PgPool,
        id: i32,
        configuration: serde_json::Value,
    ) -> Result<WorkerNamedTask, sqlx::Error> {
        let association = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            UPDATE tasker_worker_named_tasks
            SET 
                configuration = $2,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            "#,
            id,
            configuration
        )
        .fetch_one(pool)
        .await?;

        Ok(association)
    }

    /// Update association priority
    pub async fn update_priority(
        pool: &PgPool,
        id: i32,
        priority: i32,
    ) -> Result<WorkerNamedTask, sqlx::Error> {
        let association = sqlx::query_as!(
            WorkerNamedTask,
            r#"
            UPDATE tasker_worker_named_tasks
            SET 
                priority = $2,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, worker_id, named_task_id, configuration, priority, created_at, updated_at
            "#,
            id,
            priority
        )
        .fetch_one(pool)
        .await?;

        Ok(association)
    }

    /// Delete association
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_worker_named_tasks
            WHERE id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete association by worker and task
    pub async fn delete_by_worker_and_task(
        pool: &PgPool,
        worker_id: i32,
        named_task_id: i32,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_worker_named_tasks
            WHERE worker_id = $1 AND named_task_id = $2
            "#,
            worker_id,
            named_task_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all associations for a worker
    pub async fn delete_all_by_worker(pool: &PgPool, worker_id: i32) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_worker_named_tasks
            WHERE worker_id = $1
            "#,
            worker_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Find active workers that can handle a specific named task
    /// Filters by workers that have active registrations and are healthy
    pub async fn find_active_workers_for_task(
        pool: &PgPool,
        named_task_id: i32,
    ) -> Result<Vec<WorkerNamedTaskWithDetails>, sqlx::Error> {
        let active_workers = sqlx::query_as!(
            WorkerNamedTaskWithDetails,
            r#"
            SELECT 
                wnt.id,
                wnt.worker_id,
                wnt.named_task_id,
                wnt.configuration,
                COALESCE(wnt.priority, 100) as "priority!",
                wnt.created_at,
                wnt.updated_at,
                w.worker_name,
                nt.name as task_name,
                nt.version as task_version,
                tn.name as namespace_name
            FROM tasker_worker_named_tasks wnt
            INNER JOIN tasker_workers w ON wnt.worker_id = w.worker_id
            INNER JOIN tasker_named_tasks nt ON wnt.named_task_id = nt.named_task_id
            INNER JOIN tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
            INNER JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id
            WHERE wnt.named_task_id = $1
              AND wr.unregistered_at IS NULL
              AND wr.status IN ('registered', 'healthy')
              AND (wr.last_heartbeat_at IS NULL OR wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes')
            ORDER BY wnt.priority DESC, wnt.created_at
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(active_workers)
    }

    /// Get task coverage statistics
    pub async fn get_task_coverage_stats(
        pool: &PgPool,
    ) -> Result<Vec<TaskCoverageStats>, sqlx::Error> {
        let stats = sqlx::query_as!(
            TaskCoverageStats,
            r#"
            SELECT 
                nt.named_task_id,
                nt.name as task_name,
                nt.version as task_version,
                tn.name as namespace_name,
                COUNT(wnt.id) as worker_count,
                ARRAY_AGG(w.worker_name ORDER BY wnt.priority DESC) as worker_names
            FROM tasker_named_tasks nt
            INNER JOIN tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
            LEFT JOIN tasker_worker_named_tasks wnt ON nt.named_task_id = wnt.named_task_id
            LEFT JOIN tasker_workers w ON wnt.worker_id = w.worker_id
            GROUP BY nt.named_task_id, nt.name, nt.version, tn.name
            ORDER BY worker_count ASC, namespace_name, task_name, task_version
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(stats)
    }
}

/// Task coverage statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskCoverageStats {
    pub named_task_id: i32,
    pub task_name: String,
    pub task_version: String,
    pub namespace_name: String,
    pub worker_count: Option<i64>,
    pub worker_names: Option<Vec<String>>,
}

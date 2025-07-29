use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Worker represents a worker instance that can execute tasks in distributed architecture
/// Maps to `tasker_workers` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct Worker {
    pub worker_id: i32,
    pub worker_name: String,
    pub metadata: serde_json::Value,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New Worker for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorker {
    pub worker_name: String,
    pub metadata: Option<serde_json::Value>,
}

/// Worker metadata structure for type safety
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerMetadata {
    pub version: Option<String>,
    pub ruby_version: Option<String>,
    pub hostname: Option<String>,
    pub pid: Option<i32>,
    pub started_at: Option<String>,
    #[serde(flatten)]
    pub custom: serde_json::Map<String, serde_json::Value>,
}

impl Worker {
    /// Create a new worker
    pub async fn create(pool: &PgPool, new_worker: NewWorker) -> Result<Worker, sqlx::Error> {
        let metadata = new_worker.metadata.unwrap_or_else(|| serde_json::json!({}));

        let worker = sqlx::query_as!(
            Worker,
            r#"
            INSERT INTO tasker_workers (worker_name, metadata, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            RETURNING worker_id, worker_name, metadata, created_at, updated_at
            "#,
            new_worker.worker_name,
            metadata
        )
        .fetch_one(pool)
        .await?;

        Ok(worker)
    }

    /// Find a worker by ID
    pub async fn find_by_id(pool: &PgPool, id: i32) -> Result<Option<Worker>, sqlx::Error> {
        let worker = sqlx::query_as!(
            Worker,
            r#"
            SELECT worker_id, worker_name, metadata, created_at, updated_at
            FROM tasker_workers
            WHERE worker_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(worker)
    }

    /// Find a worker by worker name
    pub async fn find_by_worker_name(
        pool: &PgPool,
        worker_name: &str,
    ) -> Result<Option<Worker>, sqlx::Error> {
        let worker = sqlx::query_as!(
            Worker,
            r#"
            SELECT worker_id, worker_name, metadata, created_at, updated_at
            FROM tasker_workers
            WHERE worker_name = $1
            "#,
            worker_name
        )
        .fetch_optional(pool)
        .await?;

        Ok(worker)
    }

    /// Create or update worker by worker name (upsert pattern)
    pub async fn create_or_update_by_worker_name(
        pool: &PgPool,
        worker_name: &str,
        metadata: serde_json::Value,
    ) -> Result<Worker, sqlx::Error> {
        let worker = sqlx::query_as!(
            Worker,
            r#"
            INSERT INTO tasker_workers (worker_name, metadata, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            ON CONFLICT (worker_name) DO UPDATE SET
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            RETURNING worker_id, worker_name, metadata, created_at, updated_at
            "#,
            worker_name,
            metadata
        )
        .fetch_one(pool)
        .await?;

        Ok(worker)
    }

    /// List all active workers (those with recent registrations)
    pub async fn list_active(pool: &PgPool) -> Result<Vec<Worker>, sqlx::Error> {
        let workers = sqlx::query_as!(
            Worker,
            r#"
            SELECT DISTINCT w.worker_id, w.worker_name, w.metadata, w.created_at, w.updated_at
            FROM tasker_workers w
            INNER JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id
            WHERE wr.status IN ('registered', 'healthy')
              AND wr.unregistered_at IS NULL
              AND wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes'
            ORDER BY w.worker_name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(workers)
    }

    /// List all workers
    pub async fn list_all(pool: &PgPool) -> Result<Vec<Worker>, sqlx::Error> {
        let workers = sqlx::query_as!(
            Worker,
            r#"
            SELECT worker_id, worker_name, metadata, created_at, updated_at
            FROM tasker_workers
            ORDER BY worker_name
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(workers)
    }

    /// Update worker metadata
    pub async fn update_metadata(
        pool: &PgPool,
        id: i32,
        metadata: serde_json::Value,
    ) -> Result<Worker, sqlx::Error> {
        let worker = sqlx::query_as!(
            Worker,
            r#"
            UPDATE tasker_workers
            SET 
                metadata = $2,
                updated_at = NOW()
            WHERE worker_id = $1
            RETURNING worker_id, worker_name, metadata, created_at, updated_at
            "#,
            id,
            metadata
        )
        .fetch_one(pool)
        .await?;

        Ok(worker)
    }

    /// Delete a worker (cascade deletes registrations and task associations)
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_workers
            WHERE worker_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete worker by name
    pub async fn delete_by_worker_name(
        pool: &PgPool,
        worker_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_workers
            WHERE worker_name = $1
            "#,
            worker_name
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if worker name is unique
    pub async fn is_worker_name_unique(
        pool: &PgPool,
        worker_name: &str,
        exclude_id: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let count = if let Some(id) = exclude_id {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_workers
                WHERE worker_name = $1 AND worker_id != $2
                "#,
                worker_name,
                id
            )
            .fetch_one(pool)
            .await?
            .count
        } else {
            sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM tasker_workers
                WHERE worker_name = $1
                "#,
                worker_name
            )
            .fetch_one(pool)
            .await?
            .count
        };

        Ok(count.unwrap_or(0) == 0)
    }

    /// Get worker statistics
    pub async fn get_statistics(
        pool: &PgPool,
        id: i32,
    ) -> Result<Option<WorkerStatistics>, sqlx::Error> {
        let stats = sqlx::query_as!(
            WorkerStatistics,
            r#"
            SELECT 
                w.worker_id,
                w.worker_name,
                COUNT(wnt.id) as supported_tasks_count,
                COUNT(wr.id) FILTER (WHERE wr.status = 'healthy' AND wr.unregistered_at IS NULL) as active_registrations_count,
                MAX(wr.last_heartbeat_at) as last_heartbeat_at,
                MAX(wr.registered_at) as last_registered_at
            FROM tasker_workers w
            LEFT JOIN tasker_worker_named_tasks wnt ON w.worker_id = wnt.worker_id
            LEFT JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id
            WHERE w.worker_id = $1
            GROUP BY w.worker_id, w.worker_name
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(stats)
    }

    /// Parse metadata as WorkerMetadata for type safety
    pub fn parse_metadata(&self) -> Result<WorkerMetadata, serde_json::Error> {
        serde_json::from_value(self.metadata.clone())
    }
}

/// Worker statistics aggregation
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkerStatistics {
    pub worker_id: i32,
    pub worker_name: String,
    pub supported_tasks_count: Option<i64>,
    pub active_registrations_count: Option<i64>,
    pub last_heartbeat_at: Option<NaiveDateTime>,
    pub last_registered_at: Option<NaiveDateTime>,
}

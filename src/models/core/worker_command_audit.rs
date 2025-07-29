use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

/// Command direction enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text")]
pub enum CommandDirection {
    #[sqlx(rename = "sent")]
    Sent,
    #[sqlx(rename = "received")]
    Received,
}

impl CommandDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandDirection::Sent => "sent",
            CommandDirection::Received => "received",
        }
    }
}

impl std::fmt::Display for CommandDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// WorkerCommandAudit represents audit trail of all worker commands for observability
/// Maps to `tasker_worker_command_audit` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkerCommandAudit {
    pub id: i64,
    pub worker_id: i32,
    pub command_type: String,
    pub command_direction: String, // Store as string to match database
    pub correlation_id: Option<String>,
    pub batch_id: Option<i64>,
    pub task_id: Option<i64>,
    pub step_count: Option<i32>,
    pub execution_time_ms: Option<i64>,
    pub response_status: Option<String>,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: NaiveDateTime,
}

/// New WorkerCommandAudit for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkerCommandAudit {
    pub worker_id: i32,
    pub command_type: String,
    pub command_direction: CommandDirection,
    pub correlation_id: Option<String>,
    pub batch_id: Option<i64>,
    pub task_id: Option<i64>,
    pub step_count: Option<i32>,
    pub execution_time_ms: Option<i64>,
    pub response_status: Option<String>,
    pub error_message: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Command statistics aggregation for reporting
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CommandStatistics {
    pub worker_id: i32,
    pub worker_name: String,
    pub command_type: String,
    pub total_commands: Option<i64>,
    pub successful_commands: Option<i64>,
    pub failed_commands: Option<i64>,
    pub avg_execution_time_ms: Option<f64>,
    pub total_steps_processed: Option<i64>,
    pub first_command_at: Option<NaiveDateTime>,
    pub last_command_at: Option<NaiveDateTime>,
}

/// Worker performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkerPerformanceMetrics {
    pub worker_id: i32,
    pub worker_name: String,
    pub total_batches_processed: Option<i64>,
    pub total_steps_processed: Option<i64>,
    pub avg_batch_processing_time_ms: Option<f64>,
    pub avg_steps_per_batch: Option<f64>,
    pub success_rate_percent: Option<f64>,
    pub active_period_start: Option<NaiveDateTime>,
    pub active_period_end: Option<NaiveDateTime>,
}

impl WorkerCommandAudit {
    /// Create a new command audit record
    pub async fn create(
        pool: &PgPool,
        new_audit: NewWorkerCommandAudit,
    ) -> Result<WorkerCommandAudit, sqlx::Error> {
        let metadata = new_audit.metadata.unwrap_or_else(|| serde_json::json!({}));

        let audit = sqlx::query_as!(
            WorkerCommandAudit,
            r#"
            INSERT INTO tasker_worker_command_audit 
                (worker_id, command_type, command_direction, correlation_id, batch_id, task_id,
                 step_count, execution_time_ms, response_status, error_message, metadata, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW())
            RETURNING id, worker_id, command_type, command_direction, correlation_id, batch_id, task_id,
                      step_count, execution_time_ms, response_status, error_message, metadata, created_at
            "#,
            new_audit.worker_id,
            new_audit.command_type,
            new_audit.command_direction.as_str(),
            new_audit.correlation_id,
            new_audit.batch_id,
            new_audit.task_id,
            new_audit.step_count,
            new_audit.execution_time_ms,
            new_audit.response_status,
            new_audit.error_message,
            metadata
        )
        .fetch_one(pool)
        .await?;

        Ok(audit)
    }

    /// Find audit record by ID
    pub async fn find_by_id(
        pool: &PgPool,
        id: i64,
    ) -> Result<Option<WorkerCommandAudit>, sqlx::Error> {
        let audit = sqlx::query_as!(
            WorkerCommandAudit,
            r#"
            SELECT id, worker_id, command_type, command_direction, correlation_id, batch_id, task_id,
                   step_count, execution_time_ms, response_status, error_message, metadata, created_at
            FROM tasker_worker_command_audit
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(audit)
    }

    /// List audit records by worker
    pub async fn list_by_worker(
        pool: &PgPool,
        worker_id: i32,
        limit: Option<i64>,
    ) -> Result<Vec<WorkerCommandAudit>, sqlx::Error> {
        let limit = limit.unwrap_or(100);

        let audits = sqlx::query_as!(
            WorkerCommandAudit,
            r#"
            SELECT id, worker_id, command_type, command_direction, correlation_id, batch_id, task_id,
                   step_count, execution_time_ms, response_status, error_message, metadata, created_at
            FROM tasker_worker_command_audit
            WHERE worker_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            worker_id,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(audits)
    }

    /// List audit records by command type
    pub async fn list_by_command_type(
        pool: &PgPool,
        command_type: &str,
        limit: Option<i64>,
    ) -> Result<Vec<WorkerCommandAudit>, sqlx::Error> {
        let limit = limit.unwrap_or(100);

        let audits = sqlx::query_as!(
            WorkerCommandAudit,
            r#"
            SELECT id, worker_id, command_type, command_direction, correlation_id, batch_id, task_id,
                   step_count, execution_time_ms, response_status, error_message, metadata, created_at
            FROM tasker_worker_command_audit
            WHERE command_type = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
            command_type,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(audits)
    }

    /// List audit records by correlation ID (for request-response tracking)
    pub async fn list_by_correlation_id(
        pool: &PgPool,
        correlation_id: &str,
    ) -> Result<Vec<WorkerCommandAudit>, sqlx::Error> {
        let audits = sqlx::query_as!(
            WorkerCommandAudit,
            r#"
            SELECT id, worker_id, command_type, command_direction, correlation_id, batch_id, task_id,
                   step_count, execution_time_ms, response_status, error_message, metadata, created_at
            FROM tasker_worker_command_audit
            WHERE correlation_id = $1
            ORDER BY created_at ASC
            "#,
            correlation_id
        )
        .fetch_all(pool)
        .await?;

        Ok(audits)
    }

    /// List audit records by batch ID
    pub async fn list_by_batch_id(
        pool: &PgPool,
        batch_id: i64,
    ) -> Result<Vec<WorkerCommandAudit>, sqlx::Error> {
        let audits = sqlx::query_as!(
            WorkerCommandAudit,
            r#"
            SELECT id, worker_id, command_type, command_direction, correlation_id, batch_id, task_id,
                   step_count, execution_time_ms, response_status, error_message, metadata, created_at
            FROM tasker_worker_command_audit
            WHERE batch_id = $1
            ORDER BY created_at ASC
            "#,
            batch_id
        )
        .fetch_all(pool)
        .await?;

        Ok(audits)
    }

    /// Get command type statistics for a worker
    pub async fn get_worker_command_statistics(
        pool: &PgPool,
        worker_id: i32,
        since: Option<NaiveDateTime>,
    ) -> Result<Vec<CommandStatistics>, sqlx::Error> {
        let since =
            since.unwrap_or_else(|| chrono::Utc::now().naive_utc() - chrono::Duration::days(30));

        let stats = sqlx::query_as!(
            CommandStatistics,
            r#"
            SELECT 
                wca.worker_id,
                w.worker_name,
                wca.command_type,
                COUNT(*) as total_commands,
                COUNT(*) FILTER (WHERE wca.response_status IN ('success', 'completed')) as successful_commands,
                COUNT(*) FILTER (WHERE wca.response_status IN ('error', 'failed', 'timeout')) as failed_commands,
                AVG(wca.execution_time_ms)::DOUBLE PRECISION as avg_execution_time_ms,
                SUM(wca.step_count) as total_steps_processed,
                MIN(wca.created_at) as first_command_at,
                MAX(wca.created_at) as last_command_at
            FROM tasker_worker_command_audit wca
            INNER JOIN tasker_workers w ON wca.worker_id = w.worker_id
            WHERE wca.worker_id = $1 AND wca.created_at >= $2
            GROUP BY wca.worker_id, w.worker_name, wca.command_type
            ORDER BY total_commands DESC
            "#,
            worker_id,
            since
        )
        .fetch_all(pool)
        .await?;

        Ok(stats)
    }

    /// Get worker performance metrics
    pub async fn get_worker_performance_metrics(
        pool: &PgPool,
        worker_id: i32,
        since: Option<NaiveDateTime>,
    ) -> Result<Option<WorkerPerformanceMetrics>, sqlx::Error> {
        let since =
            since.unwrap_or_else(|| chrono::Utc::now().naive_utc() - chrono::Duration::days(30));

        let metrics = sqlx::query_as!(
            WorkerPerformanceMetrics,
            r#"
            SELECT 
                wca.worker_id,
                w.worker_name,
                COUNT(DISTINCT wca.batch_id) FILTER (WHERE wca.batch_id IS NOT NULL) as total_batches_processed,
                SUM(wca.step_count) as total_steps_processed,
                (AVG(wca.execution_time_ms) FILTER (WHERE wca.command_type = 'execute_batch'))::DOUBLE PRECISION as avg_batch_processing_time_ms,
                (AVG(wca.step_count) FILTER (WHERE wca.command_type = 'execute_batch' AND wca.step_count IS NOT NULL))::DOUBLE PRECISION as avg_steps_per_batch,
                (COUNT(*) FILTER (WHERE wca.response_status IN ('success', 'completed'))::DOUBLE PRECISION / 
                 COUNT(*)::DOUBLE PRECISION * 100) as success_rate_percent,
                MIN(wca.created_at) as active_period_start,
                MAX(wca.created_at) as active_period_end
            FROM tasker_worker_command_audit wca
            INNER JOIN tasker_workers w ON wca.worker_id = w.worker_id
            WHERE wca.worker_id = $1 AND wca.created_at >= $2
            GROUP BY wca.worker_id, w.worker_name
            "#,
            worker_id,
            since
        )
        .fetch_optional(pool)
        .await?;

        Ok(metrics)
    }

    /// Get system-wide command type distribution
    pub async fn get_command_type_distribution(
        pool: &PgPool,
        since: Option<NaiveDateTime>,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let since =
            since.unwrap_or_else(|| chrono::Utc::now().naive_utc() - chrono::Duration::hours(24));

        let distribution = sqlx::query!(
            r#"
            SELECT command_type, COUNT(*) as count
            FROM tasker_worker_command_audit
            WHERE created_at >= $1
            GROUP BY command_type
            ORDER BY count DESC
            "#,
            since
        )
        .fetch_all(pool)
        .await?;

        Ok(distribution
            .into_iter()
            .map(|row| (row.command_type, row.count.unwrap_or(0)))
            .collect())
    }

    /// Clean up old audit records (housekeeping)
    pub async fn cleanup_old_records(
        pool: &PgPool,
        older_than_days: i32,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_worker_command_audit
            WHERE created_at < NOW() - INTERVAL '1 day' * $1
            "#,
            older_than_days as f64
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Parse command direction as enum
    pub fn parse_command_direction(&self) -> Result<CommandDirection, String> {
        match self.command_direction.as_str() {
            "sent" => Ok(CommandDirection::Sent),
            "received" => Ok(CommandDirection::Received),
            _ => Err(format!(
                "Unknown command direction: {}",
                self.command_direction
            )),
        }
    }

    /// Check if command was successful based on response status
    pub fn is_successful(&self) -> bool {
        match &self.response_status {
            Some(status) => matches!(status.as_str(), "success" | "completed" | "acknowledged"),
            None => false,
        }
    }

    /// Check if command failed based on response status
    pub fn is_failed(&self) -> bool {
        match &self.response_status {
            Some(status) => matches!(status.as_str(), "error" | "failed" | "timeout"),
            None => false,
        }
    }

    /// Get execution duration as std::time::Duration if available
    pub fn get_execution_duration(&self) -> Option<std::time::Duration> {
        self.execution_time_ms
            .map(|ms| std::time::Duration::from_millis(ms as u64))
    }
}

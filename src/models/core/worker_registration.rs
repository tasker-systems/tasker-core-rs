use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

/// Worker status enumeration matching database constraint
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text")]
pub enum WorkerStatus {
    #[sqlx(rename = "registered")]
    Registered,
    #[sqlx(rename = "healthy")]
    Healthy,
    #[sqlx(rename = "unhealthy")]
    Unhealthy,
    #[sqlx(rename = "disconnected")]
    Disconnected,
}

impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkerStatus::Registered => "registered",
            WorkerStatus::Healthy => "healthy",
            WorkerStatus::Unhealthy => "unhealthy",
            WorkerStatus::Disconnected => "disconnected",
        }
    }
}

impl std::fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// WorkerRegistration represents worker connection status and health tracking
/// Maps to `tasker_worker_registrations` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkerRegistration {
    pub id: i32,
    pub worker_id: i32,
    pub status: String, // Store as string to match database
    pub connection_type: String,
    pub connection_details: serde_json::Value,
    pub last_heartbeat_at: Option<NaiveDateTime>,
    pub registered_at: NaiveDateTime,
    pub unregistered_at: Option<NaiveDateTime>,
    pub failure_reason: Option<String>,
}

/// New WorkerRegistration for creation (without generated fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkerRegistration {
    pub worker_id: i32,
    pub status: WorkerStatus,
    pub connection_type: String,
    pub connection_details: serde_json::Value,
}

/// Connection information structure for type safety
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub listener_port: Option<u16>,
    pub host: String,
    pub protocol_version: Option<String>,
    pub supported_commands: Option<Vec<String>>,
    #[serde(flatten)]
    pub custom: serde_json::Map<String, serde_json::Value>,
}

impl WorkerRegistration {
    /// Create a new worker registration
    pub async fn register(
        pool: &PgPool,
        new_registration: NewWorkerRegistration,
    ) -> Result<WorkerRegistration, sqlx::Error> {
        let registration = sqlx::query_as!(
            WorkerRegistration,
            r#"
            INSERT INTO tasker_worker_registrations 
                (worker_id, status, connection_type, connection_details, registered_at)
            VALUES ($1, $2, $3, $4, NOW())
            RETURNING id, worker_id, status, connection_type, connection_details, 
                      last_heartbeat_at, registered_at, unregistered_at, failure_reason
            "#,
            new_registration.worker_id,
            new_registration.status.as_str(),
            new_registration.connection_type,
            new_registration.connection_details
        )
        .fetch_one(pool)
        .await?;

        Ok(registration)
    }

    /// Find registration by ID
    pub async fn find_by_id(
        pool: &PgPool,
        id: i32,
    ) -> Result<Option<WorkerRegistration>, sqlx::Error> {
        let registration = sqlx::query_as!(
            WorkerRegistration,
            r#"
            SELECT id, worker_id, status, connection_type, connection_details,
                   last_heartbeat_at, registered_at, unregistered_at, failure_reason
            FROM tasker_worker_registrations
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(registration)
    }

    /// Find active registration for a worker
    pub async fn find_active_for_worker(
        pool: &PgPool,
        worker_id: i32,
    ) -> Result<Option<WorkerRegistration>, sqlx::Error> {
        let registration = sqlx::query_as!(
            WorkerRegistration,
            r#"
            SELECT id, worker_id, status, connection_type, connection_details,
                   last_heartbeat_at, registered_at, unregistered_at, failure_reason
            FROM tasker_worker_registrations
            WHERE worker_id = $1 
              AND unregistered_at IS NULL
              AND status IN ('registered', 'healthy')
            ORDER BY registered_at DESC
            LIMIT 1
            "#,
            worker_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(registration)
    }

    /// Update heartbeat timestamp
    pub async fn update_heartbeat(
        pool: &PgPool,
        registration_id: i32,
    ) -> Result<WorkerRegistration, sqlx::Error> {
        let registration = sqlx::query_as!(
            WorkerRegistration,
            r#"
            UPDATE tasker_worker_registrations
            SET 
                last_heartbeat_at = NOW(),
                status = CASE 
                    WHEN status = 'registered' THEN 'healthy'
                    ELSE status
                END
            WHERE id = $1
            RETURNING id, worker_id, status, connection_type, connection_details,
                      last_heartbeat_at, registered_at, unregistered_at, failure_reason
            "#,
            registration_id
        )
        .fetch_one(pool)
        .await?;

        Ok(registration)
    }

    /// Update heartbeat by worker ID (for active registration)
    pub async fn update_heartbeat_by_worker(
        pool: &PgPool,
        worker_id: i32,
    ) -> Result<Option<WorkerRegistration>, sqlx::Error> {
        let registration = sqlx::query_as!(
            WorkerRegistration,
            r#"
            UPDATE tasker_worker_registrations
            SET 
                last_heartbeat_at = NOW(),
                status = CASE 
                    WHEN status = 'registered' THEN 'healthy'
                    ELSE status
                END
            WHERE worker_id = $1 
              AND unregistered_at IS NULL
              AND status IN ('registered', 'healthy')
            RETURNING id, worker_id, status, connection_type, connection_details,
                      last_heartbeat_at, registered_at, unregistered_at, failure_reason
            "#,
            worker_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(registration)
    }

    /// Mark registration as disconnected/unregistered
    pub async fn mark_disconnected(
        pool: &PgPool,
        registration_id: i32,
        reason: &str,
    ) -> Result<WorkerRegistration, sqlx::Error> {
        let registration = sqlx::query_as!(
            WorkerRegistration,
            r#"
            UPDATE tasker_worker_registrations
            SET 
                status = 'disconnected',
                unregistered_at = NOW(),
                failure_reason = $2
            WHERE id = $1
            RETURNING id, worker_id, status, connection_type, connection_details,
                      last_heartbeat_at, registered_at, unregistered_at, failure_reason
            "#,
            registration_id,
            reason
        )
        .fetch_one(pool)
        .await?;

        Ok(registration)
    }

    /// Mark worker as unhealthy
    pub async fn mark_unhealthy(
        pool: &PgPool,
        registration_id: i32,
        reason: Option<&str>,
    ) -> Result<WorkerRegistration, sqlx::Error> {
        let registration = sqlx::query_as!(
            WorkerRegistration,
            r#"
            UPDATE tasker_worker_registrations
            SET 
                status = 'unhealthy',
                failure_reason = $2
            WHERE id = $1
            RETURNING id, worker_id, status, connection_type, connection_details,
                      last_heartbeat_at, registered_at, unregistered_at, failure_reason
            "#,
            registration_id,
            reason
        )
        .fetch_one(pool)
        .await?;

        Ok(registration)
    }

    /// List all active registrations
    pub async fn list_active(pool: &PgPool) -> Result<Vec<WorkerRegistration>, sqlx::Error> {
        let registrations = sqlx::query_as!(
            WorkerRegistration,
            r#"
            SELECT id, worker_id, status, connection_type, connection_details,
                   last_heartbeat_at, registered_at, unregistered_at, failure_reason
            FROM tasker_worker_registrations
            WHERE unregistered_at IS NULL
              AND status IN ('registered', 'healthy')
              AND (last_heartbeat_at IS NULL OR last_heartbeat_at > NOW() - INTERVAL '2 minutes')
            ORDER BY registered_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(registrations)
    }

    /// List registrations by connection type
    pub async fn list_by_connection_type(
        pool: &PgPool,
        connection_type: &str,
    ) -> Result<Vec<WorkerRegistration>, sqlx::Error> {
        let registrations = sqlx::query_as!(
            WorkerRegistration,
            r#"
            SELECT id, worker_id, status, connection_type, connection_details,
                   last_heartbeat_at, registered_at, unregistered_at, failure_reason
            FROM tasker_worker_registrations
            WHERE connection_type = $1
            ORDER BY registered_at DESC
            "#,
            connection_type
        )
        .fetch_all(pool)
        .await?;

        Ok(registrations)
    }

    /// List all registrations for a worker (including historical)
    pub async fn list_by_worker(
        pool: &PgPool,
        worker_id: i32,
    ) -> Result<Vec<WorkerRegistration>, sqlx::Error> {
        let registrations = sqlx::query_as!(
            WorkerRegistration,
            r#"
            SELECT id, worker_id, status, connection_type, connection_details,
                   last_heartbeat_at, registered_at, unregistered_at, failure_reason
            FROM tasker_worker_registrations
            WHERE worker_id = $1
            ORDER BY registered_at DESC
            "#,
            worker_id
        )
        .fetch_all(pool)
        .await?;

        Ok(registrations)
    }

    /// Get registration statistics
    pub async fn get_statistics(pool: &PgPool) -> Result<RegistrationStatistics, sqlx::Error> {
        let stats = sqlx::query_as!(
            RegistrationStatistics,
            r#"
            SELECT 
                COUNT(*) FILTER (WHERE status = 'registered') as registered_count,
                COUNT(*) FILTER (WHERE status = 'healthy') as healthy_count,
                COUNT(*) FILTER (WHERE status = 'unhealthy') as unhealthy_count,
                COUNT(*) FILTER (WHERE status = 'disconnected') as disconnected_count,
                COUNT(*) FILTER (WHERE unregistered_at IS NULL) as active_count,
                COUNT(DISTINCT worker_id) as unique_workers_count,
                COUNT(DISTINCT connection_type) as connection_types_count,
                MIN(registered_at) as earliest_registration,
                MAX(last_heartbeat_at) as latest_heartbeat
            FROM tasker_worker_registrations
            "#
        )
        .fetch_one(pool)
        .await?;

        Ok(stats)
    }

    /// Clean up old disconnected registrations (housekeeping)
    pub async fn cleanup_old_registrations(
        pool: &PgPool,
        older_than_days: i32,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_worker_registrations
            WHERE status = 'disconnected'
              AND unregistered_at < NOW() - INTERVAL '1 day' * $1
            "#,
            older_than_days as f64
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Parse status as WorkerStatus enum
    pub fn parse_status(&self) -> Result<WorkerStatus, String> {
        match self.status.as_str() {
            "registered" => Ok(WorkerStatus::Registered),
            "healthy" => Ok(WorkerStatus::Healthy),
            "unhealthy" => Ok(WorkerStatus::Unhealthy),
            "disconnected" => Ok(WorkerStatus::Disconnected),
            _ => Err(format!("Unknown worker status: {}", self.status)),
        }
    }

    /// Parse connection details as ConnectionInfo for type safety
    pub fn parse_connection_details(&self) -> Result<ConnectionInfo, serde_json::Error> {
        serde_json::from_value(self.connection_details.clone())
    }

    /// Check if registration is considered active
    pub fn is_active(&self) -> bool {
        self.unregistered_at.is_none()
            && matches!(self.status.as_str(), "registered" | "healthy")
            && self.is_heartbeat_recent()
    }

    /// Check if last heartbeat is recent (within 2 minutes)
    pub fn is_heartbeat_recent(&self) -> bool {
        match self.last_heartbeat_at {
            Some(heartbeat) => {
                let now = chrono::Utc::now().naive_utc();
                let threshold = now - chrono::Duration::minutes(2);
                heartbeat > threshold
            }
            None => {
                // If no heartbeat yet, check if registration is recent (within 30 seconds)
                let now = chrono::Utc::now().naive_utc();
                let threshold = now - chrono::Duration::seconds(30);
                self.registered_at > threshold
            }
        }
    }
}

/// Registration statistics aggregation
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegistrationStatistics {
    pub registered_count: Option<i64>,
    pub healthy_count: Option<i64>,
    pub unhealthy_count: Option<i64>,
    pub disconnected_count: Option<i64>,
    pub active_count: Option<i64>,
    pub unique_workers_count: Option<i64>,
    pub connection_types_count: Option<i64>,
    pub earliest_registration: Option<NaiveDateTime>,
    pub latest_heartbeat: Option<NaiveDateTime>,
}

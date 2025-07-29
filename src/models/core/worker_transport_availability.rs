use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// Tracks which core instances can reach which workers via which transports
/// Maps to `tasker_worker_transport_availability` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct WorkerTransportAvailability {
    pub id: i32,
    pub worker_id: i32,
    pub core_instance_id: String,
    pub transport_type: String,
    pub connection_details: serde_json::Value,
    pub is_reachable: bool,
    pub last_verified_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New transport availability for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkerTransportAvailability {
    pub worker_id: i32,
    pub core_instance_id: String,
    pub transport_type: String,
    pub connection_details: serde_json::Value,
    pub is_reachable: bool,
}

/// Transport types enum for type safety
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Tcp,
    Unix,
}

impl TransportType {
    pub fn as_str(&self) -> &str {
        match self {
            TransportType::Tcp => "tcp",
            TransportType::Unix => "unix",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tcp" => Some(TransportType::Tcp),
            "unix" => Some(TransportType::Unix),
            _ => None,
        }
    }
}

/// Connection details for TCP transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpConnectionDetails {
    pub host: String,
    pub port: u16,
}

/// Connection details for Unix socket transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnixConnectionDetails {
    pub socket_path: String,
}

impl WorkerTransportAvailability {
    /// Create or update transport availability
    pub async fn upsert(
        pool: &PgPool,
        new_availability: NewWorkerTransportAvailability,
    ) -> Result<WorkerTransportAvailability, sqlx::Error> {
        let availability = sqlx::query_as!(
            WorkerTransportAvailability,
            r#"
            INSERT INTO tasker_worker_transport_availability 
                (worker_id, core_instance_id, transport_type, connection_details, is_reachable, last_verified_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())
            ON CONFLICT (worker_id, core_instance_id, transport_type) DO UPDATE SET
                connection_details = EXCLUDED.connection_details,
                is_reachable = EXCLUDED.is_reachable,
                last_verified_at = NOW(),
                updated_at = NOW()
            RETURNING id, worker_id, core_instance_id, transport_type, connection_details, is_reachable, last_verified_at, created_at, updated_at
            "#,
            new_availability.worker_id,
            new_availability.core_instance_id,
            new_availability.transport_type,
            new_availability.connection_details,
            new_availability.is_reachable
        )
        .fetch_one(pool)
        .await?;

        Ok(availability)
    }

    /// Find transport availability for a specific worker and core instance
    pub async fn find_for_worker_and_core(
        pool: &PgPool,
        worker_id: i32,
        core_instance_id: &str,
    ) -> Result<Vec<WorkerTransportAvailability>, sqlx::Error> {
        let availabilities = sqlx::query_as!(
            WorkerTransportAvailability,
            r#"
            SELECT id, worker_id, core_instance_id, transport_type, connection_details, 
                   is_reachable, last_verified_at, created_at, updated_at
            FROM tasker_worker_transport_availability
            WHERE worker_id = $1 AND core_instance_id = $2
            ORDER BY transport_type
            "#,
            worker_id,
            core_instance_id
        )
        .fetch_all(pool)
        .await?;

        Ok(availabilities)
    }

    /// Find all reachable workers for a core instance
    pub async fn find_reachable_for_core(
        pool: &PgPool,
        core_instance_id: &str,
        transport_type: Option<&str>,
    ) -> Result<Vec<WorkerTransportAvailability>, sqlx::Error> {
        let availabilities = if let Some(transport) = transport_type {
            sqlx::query_as!(
                WorkerTransportAvailability,
                r#"
                SELECT id, worker_id, core_instance_id, transport_type, connection_details, 
                       is_reachable, last_verified_at, created_at, updated_at
                FROM tasker_worker_transport_availability
                WHERE core_instance_id = $1 
                  AND transport_type = $2
                  AND is_reachable = true
                  AND last_verified_at > NOW() - INTERVAL '5 minutes'
                ORDER BY worker_id, transport_type
                "#,
                core_instance_id,
                transport
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                WorkerTransportAvailability,
                r#"
                SELECT id, worker_id, core_instance_id, transport_type, connection_details, 
                       is_reachable, last_verified_at, created_at, updated_at
                FROM tasker_worker_transport_availability
                WHERE core_instance_id = $1 
                  AND is_reachable = true
                  AND last_verified_at > NOW() - INTERVAL '5 minutes'
                ORDER BY worker_id, transport_type
                "#,
                core_instance_id
            )
            .fetch_all(pool)
            .await?
        };

        Ok(availabilities)
    }

    /// Update reachability status
    pub async fn update_reachability(
        pool: &PgPool,
        worker_id: i32,
        core_instance_id: &str,
        transport_type: &str,
        is_reachable: bool,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE tasker_worker_transport_availability
            SET is_reachable = $4,
                last_verified_at = NOW(),
                updated_at = NOW()
            WHERE worker_id = $1 
              AND core_instance_id = $2
              AND transport_type = $3
            "#,
            worker_id,
            core_instance_id,
            transport_type,
            is_reachable
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Clean up stale entries older than specified hours
    pub async fn cleanup_stale_entries(pool: &PgPool, hours_old: i32) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_worker_transport_availability
            WHERE last_verified_at < NOW() - INTERVAL '1 hour' * $1
            "#,
            hours_old as f64
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Check if a worker is reachable by a core instance
    pub async fn is_worker_reachable(
        pool: &PgPool,
        worker_id: i32,
        core_instance_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_worker_transport_availability
            WHERE worker_id = $1 
              AND core_instance_id = $2
              AND is_reachable = true
              AND last_verified_at > NOW() - INTERVAL '5 minutes'
            "#,
            worker_id,
            core_instance_id
        )
        .fetch_one(pool)
        .await?
        .count;

        Ok(count.unwrap_or(0) > 0)
    }

    /// Get connection details as TCP
    pub fn as_tcp_connection(&self) -> Result<TcpConnectionDetails, serde_json::Error> {
        serde_json::from_value(self.connection_details.clone())
    }

    /// Get connection details as Unix
    pub fn as_unix_connection(&self) -> Result<UnixConnectionDetails, serde_json::Error> {
        serde_json::from_value(self.connection_details.clone())
    }
}

/// Statistics about transport availability for a core instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportAvailabilityStats {
    pub core_instance_id: String,
    pub total_workers: i64,
    pub reachable_workers: i64,
    pub tcp_workers: i64,
    pub unix_workers: i64,
    pub recently_verified: i64,
    pub stale_entries: i64,
}

impl TransportAvailabilityStats {
    /// Get statistics for a core instance
    pub async fn for_core_instance(
        pool: &PgPool,
        core_instance_id: &str,
    ) -> Result<TransportAvailabilityStats, sqlx::Error> {
        let stats = sqlx::query!(
            r#"
            SELECT 
                $1 as core_instance_id,
                COUNT(DISTINCT worker_id) as total_workers,
                COUNT(DISTINCT worker_id) FILTER (WHERE is_reachable = true) as reachable_workers,
                COUNT(*) FILTER (WHERE transport_type = 'tcp') as tcp_workers,
                COUNT(*) FILTER (WHERE transport_type = 'unix') as unix_workers,
                COUNT(*) FILTER (WHERE last_verified_at > NOW() - INTERVAL '5 minutes') as recently_verified,
                COUNT(*) FILTER (WHERE last_verified_at <= NOW() - INTERVAL '5 minutes') as stale_entries
            FROM tasker_worker_transport_availability
            WHERE core_instance_id = $1
            "#,
            core_instance_id
        )
        .fetch_one(pool)
        .await?;

        Ok(TransportAvailabilityStats {
            core_instance_id: core_instance_id.to_string(),
            total_workers: stats.total_workers.unwrap_or(0),
            reachable_workers: stats.reachable_workers.unwrap_or(0),
            tcp_workers: stats.tcp_workers.unwrap_or(0),
            unix_workers: stats.unix_workers.unwrap_or(0),
            recently_verified: stats.recently_verified.unwrap_or(0),
            stale_entries: stats.stale_entries.unwrap_or(0),
        })
    }
}

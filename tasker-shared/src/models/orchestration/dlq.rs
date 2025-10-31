// TAS-49: Dead Letter Queue (DLQ) Domain Models
//
// Domain types for DLQ investigation tracking system. These types map to
// PostgreSQL enums and tables created in TAS-49 migrations.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::Type;
use uuid::Uuid;

/// DLQ Resolution Status
///
/// Tracks the lifecycle of a DLQ investigation (NOT task state).
/// This enum maps to the PostgreSQL `dlq_resolution_status` enum type.
///
/// State Machine:
/// - `pending` → `manually_resolved` (operator fixed problem via step APIs)
/// - `pending` → `permanently_failed` (unfixable issue, task stays in Error state)
/// - `pending` → `cancelled` (investigation no longer needed)
///
/// A task can have multiple DLQ entries over time (investigation history),
/// but only one pending entry at a time (enforced by unique index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "dlq_resolution_status", rename_all = "snake_case")]
pub enum DlqResolutionStatus {
    /// Investigation in progress
    Pending,

    /// Operator fixed problem steps, task progressed
    ManuallyResolved,

    /// Unfixable issue (e.g., bad template, data corruption)
    PermanentlyFailed,

    /// Investigation cancelled (duplicate, false positive, etc.)
    Cancelled,
}

impl DlqResolutionStatus {
    /// Check if this status represents an active investigation
    #[must_use]
    pub const fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Check if this status represents a resolved investigation
    #[must_use]
    pub const fn is_resolved(self) -> bool {
        matches!(
            self,
            Self::ManuallyResolved | Self::PermanentlyFailed | Self::Cancelled
        )
    }

    /// Get human-readable description
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::ManuallyResolved => "manually_resolved",
            Self::PermanentlyFailed => "permanently_failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// DLQ Reason
///
/// Why was a task sent to DLQ? Determines investigation priority and remediation approach.
/// This enum maps to the PostgreSQL `dlq_reason` enum type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "dlq_reason", rename_all = "snake_case")]
pub enum DlqReason {
    /// Task exceeded state timeout threshold (TAS-48 staleness detection)
    StalenessTimeout,

    /// TAS-42 retry limit hit
    MaxRetriesExceeded,

    /// Circular dependency discovered
    DependencyCycleDetected,

    /// No worker available for extended period
    WorkerUnavailable,

    /// Operator manually sent to DLQ
    ManualDlq,
}

impl DlqReason {
    /// Get investigation priority (lower number = higher priority)
    ///
    /// Used by `v_dlq_investigation_queue` view to prioritize investigations.
    #[must_use]
    pub const fn investigation_priority(self) -> u8 {
        match self {
            Self::DependencyCycleDetected => 1, // Highest priority - structural issue
            Self::MaxRetriesExceeded => 2,      // High - systematic failure
            Self::WorkerUnavailable => 3,       // Medium - infrastructure issue
            Self::StalenessTimeout => 4,        // Normal - timeout exceeded
            Self::ManualDlq => 5,               // Lowest - operator initiated
        }
    }

    /// Get human-readable description
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StalenessTimeout => "staleness_timeout",
            Self::MaxRetriesExceeded => "max_retries_exceeded",
            Self::DependencyCycleDetected => "dependency_cycle_detected",
            Self::WorkerUnavailable => "worker_unavailable",
            Self::ManualDlq => "manual_dlq",
        }
    }

    /// Check if this reason indicates a systemic issue
    #[must_use]
    pub const fn is_systemic(self) -> bool {
        matches!(
            self,
            Self::DependencyCycleDetected | Self::MaxRetriesExceeded | Self::WorkerUnavailable
        )
    }
}

/// DLQ Entry
///
/// Complete DLQ investigation record. Maps to `tasker_tasks_dlq` table.
///
/// Architecture:
/// - DLQ is an INVESTIGATION TRACKER (not task manipulation layer)
/// - Tasks remain in tasker_tasks table (typically in Error state)
/// - DLQ entries track "why stuck" and "what operator did"
/// - Multiple DLQ entries per task allowed (historical trail)
/// - Only one "pending" investigation per task at a time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEntry {
    /// Unique identifier for this DLQ entry
    pub dlq_entry_uuid: Uuid,

    /// Task under investigation
    pub task_uuid: Uuid,

    /// Task state when sent to DLQ
    pub original_state: String,

    /// Why task was sent to DLQ
    pub dlq_reason: DlqReason,

    /// When task was added to DLQ
    pub dlq_timestamp: NaiveDateTime,

    /// Current investigation status
    pub resolution_status: DlqResolutionStatus,

    /// When investigation was resolved (if resolved)
    pub resolution_timestamp: Option<NaiveDateTime>,

    /// Investigation notes and resolution details
    pub resolution_notes: Option<String>,

    /// Who resolved the investigation (user or system)
    pub resolved_by: Option<String>,

    /// Complete task snapshot at DLQ time (JSONB)
    /// Contains: task details, steps, transitions, execution context
    pub task_snapshot: serde_json::Value,

    /// Additional metadata (JSONB)
    /// Examples: error stack trace, detection method, related tasks
    pub metadata: Option<serde_json::Value>,

    /// Entry creation timestamp
    pub created_at: NaiveDateTime,

    /// Entry last update timestamp
    pub updated_at: NaiveDateTime,
}

/// DLQ list query parameters
#[derive(Debug, Clone)]
pub struct DlqListParams {
    /// Filter by resolution status (optional)
    pub resolution_status: Option<DlqResolutionStatus>,
    /// Maximum number of entries to return (default: 50)
    pub limit: i64,
    /// Offset for pagination (default: 0)
    pub offset: i64,
}

impl Default for DlqListParams {
    fn default() -> Self {
        Self {
            resolution_status: None,
            limit: 50,
            offset: 0,
        }
    }
}

/// DLQ statistics by reason
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqStats {
    pub dlq_reason: DlqReason,
    pub total_entries: i64,
    pub pending: i64,
    pub manually_resolved: i64,
    pub permanent_failures: i64,
    pub oldest_entry: Option<NaiveDateTime>,
    pub newest_entry: Option<NaiveDateTime>,
}

/// DLQ investigation update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqInvestigationUpdate {
    /// New resolution status (optional)
    pub resolution_status: Option<DlqResolutionStatus>,
    /// Investigation notes (optional)
    pub resolution_notes: Option<String>,
    /// Who resolved the investigation (optional)
    pub resolved_by: Option<String>,
    /// Additional metadata (optional)
    pub metadata: Option<serde_json::Value>,
}

impl DlqEntry {
    /// Calculate how long this task has been in DLQ (minutes)
    #[must_use]
    pub fn time_in_dlq_minutes(&self, now: NaiveDateTime) -> i64 {
        (now - self.dlq_timestamp).num_minutes()
    }

    /// Check if this investigation has exceeded max pending age
    #[must_use]
    pub fn is_stale(&self, max_pending_age_hours: i32, now: NaiveDateTime) -> bool {
        if !self.resolution_status.is_pending() {
            return false;
        }

        let hours_in_dlq = (now - self.dlq_timestamp).num_hours();
        hours_in_dlq > i64::from(max_pending_age_hours)
    }

    /// List DLQ entries with optional filtering and pagination
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `params` - Query parameters (status filter, pagination)
    ///
    /// # Returns
    ///
    /// Vector of DLQ entries ordered by dlq_timestamp (most recent first)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::orchestration::dlq::{DlqEntry, DlqListParams, DlqResolutionStatus};
    /// use sqlx::PgPool;
    ///
    /// async fn list_pending_investigations(pool: &PgPool) -> Result<Vec<DlqEntry>, sqlx::Error> {
    ///     let params = DlqListParams {
    ///         resolution_status: Some(DlqResolutionStatus::Pending),
    ///         limit: 20,
    ///         offset: 0,
    ///     };
    ///     DlqEntry::list(pool, params).await
    /// }
    /// ```
    pub async fn list(
        pool: &sqlx::PgPool,
        params: DlqListParams,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Convert enum to text for SQL parameter binding
        let resolution_status_text = params.resolution_status.map(|s| s.as_str());

        let entries = sqlx::query_as!(
            DlqEntry,
            r#"
            SELECT
                dlq_entry_uuid,
                task_uuid,
                original_state,
                dlq_reason as "dlq_reason: DlqReason",
                dlq_timestamp,
                resolution_status as "resolution_status: DlqResolutionStatus",
                resolution_timestamp,
                resolution_notes,
                resolved_by,
                task_snapshot,
                metadata,
                created_at,
                updated_at
            FROM tasker_tasks_dlq
            WHERE ($1::text IS NULL OR resolution_status::text = $1)
            ORDER BY dlq_timestamp DESC
            LIMIT $2
            OFFSET $3
            "#,
            resolution_status_text,
            params.limit,
            params.offset
        )
        .fetch_all(pool)
        .await?;

        Ok(entries)
    }

    /// Find the most recent DLQ entry for a specific task
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `task_uuid` - UUID of the task
    ///
    /// # Returns
    ///
    /// Most recent DLQ entry for the task, or None if not found
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::orchestration::dlq::DlqEntry;
    /// use sqlx::PgPool;
    /// use uuid::Uuid;
    ///
    /// async fn get_task_investigation(pool: &PgPool, task_uuid: Uuid) -> Result<Option<DlqEntry>, sqlx::Error> {
    ///     DlqEntry::find_by_task(pool, task_uuid).await
    /// }
    /// ```
    pub async fn find_by_task(
        pool: &sqlx::PgPool,
        task_uuid: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        let entry = sqlx::query_as!(
            DlqEntry,
            r#"
            SELECT
                dlq_entry_uuid,
                task_uuid,
                original_state,
                dlq_reason as "dlq_reason: DlqReason",
                dlq_timestamp,
                resolution_status as "resolution_status: DlqResolutionStatus",
                resolution_timestamp,
                resolution_notes,
                resolved_by,
                task_snapshot,
                metadata,
                created_at,
                updated_at
            FROM tasker_tasks_dlq
            WHERE task_uuid = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            task_uuid
        )
        .fetch_optional(pool)
        .await?;

        Ok(entry)
    }

    /// Update DLQ investigation status and notes
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `dlq_entry_uuid` - UUID of the DLQ entry to update
    /// * `update` - Update data (status, notes, resolved_by, metadata)
    ///
    /// # Returns
    ///
    /// True if entry was found and updated, false if not found
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::orchestration::dlq::{DlqEntry, DlqInvestigationUpdate, DlqResolutionStatus};
    /// use sqlx::PgPool;
    /// use uuid::Uuid;
    ///
    /// async fn resolve_investigation(pool: &PgPool, entry_uuid: Uuid) -> Result<bool, sqlx::Error> {
    ///     let update = DlqInvestigationUpdate {
    ///         resolution_status: Some(DlqResolutionStatus::ManuallyResolved),
    ///         resolution_notes: Some("Fixed by resetting retry count".to_string()),
    ///         resolved_by: Some("operator@example.com".to_string()),
    ///         metadata: None,
    ///     };
    ///     DlqEntry::update_investigation(pool, entry_uuid, update).await
    /// }
    /// ```
    pub async fn update_investigation(
        pool: &sqlx::PgPool,
        dlq_entry_uuid: Uuid,
        update: DlqInvestigationUpdate,
    ) -> Result<bool, sqlx::Error> {
        // Convert enum to text for SQL parameter binding
        let resolution_status_text = update.resolution_status.map(|s| s.as_str());

        let result = sqlx::query!(
            r#"
            UPDATE tasker_tasks_dlq
            SET resolution_status = COALESCE($2::text::dlq_resolution_status, resolution_status),
                resolution_timestamp = CASE WHEN $2 IS NOT NULL THEN NOW() ELSE resolution_timestamp END,
                resolution_notes = COALESCE($3, resolution_notes),
                resolved_by = COALESCE($4, resolved_by),
                metadata = COALESCE($5, metadata),
                updated_at = NOW()
            WHERE dlq_entry_uuid = $1
            "#,
            dlq_entry_uuid,
            resolution_status_text,
            update.resolution_notes,
            update.resolved_by,
            update.metadata
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get aggregated DLQ statistics by reason
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    ///
    /// # Returns
    ///
    /// Vector of statistics grouped by DLQ reason
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::orchestration::dlq::{DlqEntry, DlqStats};
    /// use sqlx::PgPool;
    ///
    /// async fn get_system_health(pool: &PgPool) -> Result<Vec<DlqStats>, sqlx::Error> {
    ///     DlqEntry::get_stats(pool).await
    /// }
    /// ```
    pub async fn get_stats(pool: &sqlx::PgPool) -> Result<Vec<DlqStats>, sqlx::Error> {
        let stats = sqlx::query_as!(
            DlqStats,
            r#"
            SELECT
                dlq_reason as "dlq_reason: DlqReason",
                COUNT(*) as "total_entries!",
                COUNT(*) FILTER (WHERE resolution_status = 'pending') as "pending!",
                COUNT(*) FILTER (WHERE resolution_status = 'manually_resolved') as "manually_resolved!",
                COUNT(*) FILTER (WHERE resolution_status = 'permanently_failed') as "permanent_failures!",
                MIN(dlq_timestamp) as oldest_entry,
                MAX(dlq_timestamp) as newest_entry
            FROM tasker_tasks_dlq
            GROUP BY dlq_reason
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlq_resolution_status_predicates() {
        assert!(DlqResolutionStatus::Pending.is_pending());
        assert!(!DlqResolutionStatus::Pending.is_resolved());

        assert!(!DlqResolutionStatus::ManuallyResolved.is_pending());
        assert!(DlqResolutionStatus::ManuallyResolved.is_resolved());

        assert!(!DlqResolutionStatus::PermanentlyFailed.is_pending());
        assert!(DlqResolutionStatus::PermanentlyFailed.is_resolved());

        assert!(!DlqResolutionStatus::Cancelled.is_pending());
        assert!(DlqResolutionStatus::Cancelled.is_resolved());
    }

    #[test]
    fn test_dlq_resolution_status_as_str() {
        assert_eq!(DlqResolutionStatus::Pending.as_str(), "pending");
        assert_eq!(
            DlqResolutionStatus::ManuallyResolved.as_str(),
            "manually_resolved"
        );
        assert_eq!(
            DlqResolutionStatus::PermanentlyFailed.as_str(),
            "permanently_failed"
        );
        assert_eq!(DlqResolutionStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_dlq_reason_investigation_priority() {
        // Lower number = higher priority
        assert_eq!(
            DlqReason::DependencyCycleDetected.investigation_priority(),
            1
        );
        assert_eq!(DlqReason::MaxRetriesExceeded.investigation_priority(), 2);
        assert_eq!(DlqReason::WorkerUnavailable.investigation_priority(), 3);
        assert_eq!(DlqReason::StalenessTimeout.investigation_priority(), 4);
        assert_eq!(DlqReason::ManualDlq.investigation_priority(), 5);
    }

    #[test]
    fn test_dlq_reason_is_systemic() {
        assert!(DlqReason::DependencyCycleDetected.is_systemic());
        assert!(DlqReason::MaxRetriesExceeded.is_systemic());
        assert!(DlqReason::WorkerUnavailable.is_systemic());
        assert!(!DlqReason::StalenessTimeout.is_systemic());
        assert!(!DlqReason::ManualDlq.is_systemic());
    }

    #[test]
    fn test_dlq_reason_as_str() {
        assert_eq!(DlqReason::StalenessTimeout.as_str(), "staleness_timeout");
        assert_eq!(
            DlqReason::MaxRetriesExceeded.as_str(),
            "max_retries_exceeded"
        );
        assert_eq!(
            DlqReason::DependencyCycleDetected.as_str(),
            "dependency_cycle_detected"
        );
        assert_eq!(DlqReason::WorkerUnavailable.as_str(), "worker_unavailable");
        assert_eq!(DlqReason::ManualDlq.as_str(), "manual_dlq");
    }

    #[test]
    fn test_dlq_entry_time_in_dlq_minutes() {
        let dlq_timestamp =
            NaiveDateTime::parse_from_str("2025-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let now =
            NaiveDateTime::parse_from_str("2025-01-01 12:30:00", "%Y-%m-%d %H:%M:%S").unwrap();

        let entry = DlqEntry {
            dlq_entry_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            original_state: "waiting_for_dependencies".to_string(),
            dlq_reason: DlqReason::StalenessTimeout,
            dlq_timestamp,
            resolution_status: DlqResolutionStatus::Pending,
            resolution_timestamp: None,
            resolution_notes: None,
            resolved_by: None,
            task_snapshot: serde_json::json!({}),
            metadata: None,
            created_at: dlq_timestamp,
            updated_at: dlq_timestamp,
        };

        assert_eq!(entry.time_in_dlq_minutes(now), 30);
    }

    #[test]
    fn test_dlq_entry_is_stale() {
        let dlq_timestamp =
            NaiveDateTime::parse_from_str("2025-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let now =
            NaiveDateTime::parse_from_str("2025-01-08 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(); // 7 days later

        let mut entry = DlqEntry {
            dlq_entry_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            original_state: "waiting_for_dependencies".to_string(),
            dlq_reason: DlqReason::StalenessTimeout,
            dlq_timestamp,
            resolution_status: DlqResolutionStatus::Pending,
            resolution_timestamp: None,
            resolution_notes: None,
            resolved_by: None,
            task_snapshot: serde_json::json!({}),
            metadata: None,
            created_at: dlq_timestamp,
            updated_at: dlq_timestamp,
        };

        // 7 days = 168 hours, max_pending_age_hours = 168
        assert!(!entry.is_stale(168, now)); // Exactly at threshold - not stale

        // 7 days > 24 hours
        assert!(entry.is_stale(24, now)); // Exceeds threshold - stale

        // Resolved entries are never stale
        entry.resolution_status = DlqResolutionStatus::ManuallyResolved;
        assert!(!entry.is_stale(24, now));
    }
}

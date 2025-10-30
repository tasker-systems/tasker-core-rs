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

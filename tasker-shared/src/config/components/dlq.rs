// TAS-49: Dead Letter Queue (DLQ) and Lifecycle Configuration Components
//
// Configuration types for DLQ investigation tracking, staleness detection,
// and archival operations.

use serde::{Deserialize, Serialize};

/// Staleness Detection Configuration
///
/// Controls automatic detection and transition of stale tasks to DLQ.
/// Consolidates TAS-48 hardcoded thresholds into configurable system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalenessDetectionConfig {
    /// Enable staleness detection background service
    pub enabled: bool,

    /// Interval between staleness detection runs (seconds)
    pub detection_interval_seconds: u64,

    /// Maximum number of stale tasks to process per detection run
    pub batch_size: i32,

    /// Dry-run mode: detect but don't transition (for validation)
    pub dry_run: bool,

    /// Staleness thresholds by task state
    pub thresholds: StalenessThresholds,

    /// Actions to take when staleness detected
    pub actions: StalenessActions,
}

impl Default for StalenessDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            detection_interval_seconds: 300, // 5 minutes
            batch_size: 100,
            dry_run: false,
            thresholds: StalenessThresholds::default(),
            actions: StalenessActions::default(),
        }
    }
}

/// Staleness Thresholds
///
/// Time limits before tasks are considered stale. Per-template lifecycle
/// configuration in TaskTemplate YAML takes precedence over these defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalenessThresholds {
    /// Max time in waiting_for_dependencies state (minutes)
    /// TAS-48 consolidation: was hardcoded as 60 in SQL
    pub waiting_for_dependencies_minutes: i32,

    /// Max time in waiting_for_retry state (minutes)
    /// TAS-48 consolidation: was hardcoded as 30 in SQL
    pub waiting_for_retry_minutes: i32,

    /// Max time in steps_in_process state (minutes)
    pub steps_in_process_minutes: i32,

    /// Max total task lifetime (hours)
    pub task_max_lifetime_hours: i32,
}

impl Default for StalenessThresholds {
    fn default() -> Self {
        Self {
            waiting_for_dependencies_minutes: 60, // TAS-48 default
            waiting_for_retry_minutes: 30,        // TAS-48 default
            steps_in_process_minutes: 30,
            task_max_lifetime_hours: 24,
        }
    }
}

/// Staleness Actions
///
/// Controls what happens when staleness is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalenessActions {
    /// Automatically transition stale tasks to Error state
    pub auto_transition_to_error: bool,

    /// Automatically create DLQ investigation entry
    pub auto_move_to_dlq: bool,

    /// Emit staleness events for monitoring
    pub emit_events: bool,

    /// Event channel name for staleness notifications
    pub event_channel: String,
}

impl Default for StalenessActions {
    fn default() -> Self {
        Self {
            auto_transition_to_error: true,
            auto_move_to_dlq: true,
            emit_events: true,
            event_channel: "task_staleness_detected".to_string(),
        }
    }
}

/// DLQ Operations Configuration
///
/// Controls Dead Letter Queue investigation tracking behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqConfig {
    /// Enable DLQ investigation tracking
    pub enabled: bool,

    /// Automatically create DLQ entries when staleness detected
    pub auto_dlq_on_staleness: bool,

    /// Include full task+steps state in DLQ snapshot JSONB
    pub include_full_task_snapshot: bool,

    /// Alert if investigation pending longer than this (hours)
    pub max_pending_age_hours: i32,

    /// DLQ reasons configuration
    pub reasons: DlqReasons,
}

impl Default for DlqConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_dlq_on_staleness: true,
            include_full_task_snapshot: true,
            max_pending_age_hours: 168, // 1 week
            reasons: DlqReasons::default(),
        }
    }
}

/// DLQ Reasons Configuration
///
/// Controls which conditions trigger DLQ entry creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqReasons {
    /// Tasks exceeding state timeout thresholds
    pub staleness_timeout: bool,

    /// TAS-42 retry limit hit
    pub max_retries_exceeded: bool,

    /// No worker available for extended period
    pub worker_unavailable: bool,

    /// Circular dependency discovered
    pub dependency_cycle_detected: bool,

    /// Operator manually sent to DLQ
    pub manual_dlq: bool,
}

impl Default for DlqReasons {
    fn default() -> Self {
        Self {
            staleness_timeout: true,
            max_retries_exceeded: true,
            worker_unavailable: true,
            dependency_cycle_detected: true,
            manual_dlq: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staleness_detection_config_default() {
        let config = StalenessDetectionConfig::default();
        assert!(config.enabled);
        assert_eq!(config.detection_interval_seconds, 300);
        assert_eq!(config.batch_size, 100);
        assert!(!config.dry_run);
    }

    #[test]
    fn test_staleness_thresholds_default() {
        let thresholds = StalenessThresholds::default();
        // TAS-48 consolidation values
        assert_eq!(thresholds.waiting_for_dependencies_minutes, 60);
        assert_eq!(thresholds.waiting_for_retry_minutes, 30);
        assert_eq!(thresholds.steps_in_process_minutes, 30);
        assert_eq!(thresholds.task_max_lifetime_hours, 24);
    }

    #[test]
    fn test_staleness_actions_default() {
        let actions = StalenessActions::default();
        assert!(actions.auto_transition_to_error);
        assert!(actions.auto_move_to_dlq);
        assert!(actions.emit_events);
        assert_eq!(actions.event_channel, "task_staleness_detected");
    }

    #[test]
    fn test_dlq_config_default() {
        let config = DlqConfig::default();
        assert!(config.enabled);
        assert!(config.auto_dlq_on_staleness);
        assert!(config.include_full_task_snapshot);
        assert_eq!(config.max_pending_age_hours, 168); // 1 week
    }

    #[test]
    fn test_dlq_reasons_default() {
        let reasons = DlqReasons::default();
        assert!(reasons.staleness_timeout);
        assert!(reasons.max_retries_exceeded);
        assert!(reasons.worker_unavailable);
        assert!(reasons.dependency_cycle_detected);
        assert!(reasons.manual_dlq);
    }
}

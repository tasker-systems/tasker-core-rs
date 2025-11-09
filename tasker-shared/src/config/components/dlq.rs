// TAS-49: Dead Letter Queue (DLQ) and Lifecycle Configuration Components
//
// Re-exports V2 canonical DLQ configuration types for backward compatibility.
// New code should import directly from tasker_v2.

pub use crate::config::tasker::tasker_v2::{
    DlqOperationsConfig, DlqReasons, StalenessActions, StalenessDetectionConfig,
    StalenessThresholds,
};

// Type alias for backward compatibility
// Old code using `DlqConfig` will now use `DlqOperationsConfig`
pub type DlqConfig = DlqOperationsConfig;

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
    fn test_dlq_operations_config_default() {
        // Test using the new canonical name
        let config = DlqOperationsConfig::default();
        assert!(config.enabled);
        assert!(config.auto_dlq_on_staleness);
        assert!(config.include_full_task_snapshot);
        assert_eq!(config.max_pending_age_hours, 168);

        // Verify staleness_detection is nested
        assert!(config.staleness_detection.enabled);
        assert_eq!(config.staleness_detection.detection_interval_seconds, 300);
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

    #[test]
    fn test_type_alias_compatibility() {
        // Verify that DlqConfig (old name) and DlqOperationsConfig (new name) are the same type
        let config1: DlqConfig = DlqConfig::default();
        let config2: DlqOperationsConfig = DlqOperationsConfig::default();

        // Should be able to assign between them
        let _: DlqConfig = config2.clone();
        let _: DlqOperationsConfig = config1;
    }
}

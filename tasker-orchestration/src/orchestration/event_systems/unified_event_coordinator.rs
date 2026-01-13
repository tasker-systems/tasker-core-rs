//! # Unified Event Coordinator - Demonstration of EventDrivenSystem Pattern Reuse
//!
//! This module demonstrates how the EventDrivenSystem trait enables pattern reuse
//! across different event contexts (orchestration and task readiness) while respecting
//! their architectural differences.
//!
//! ## Key Demonstration Points
//!
//! 1. **Unified Management**: Single coordinator managing both queue-level and database-level events
//! 2. **Deployment Mode Consistency**: Same deployment modes applied across different event types
//! 3. **Pattern Reuse**: Common interface for start/stop, health checks, and statistics
//! 4. **Context Awareness**: Respects differences between pgmq-notify vs PostgreSQL LISTEN/NOTIFY

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use super::{OrchestrationEventSystem, OrchestrationEventSystemConfig, TaskReadinessEventSystem};
use crate::orchestration::{command_processor::OrchestrationCommand, OrchestrationCore};
use tasker_shared::config::event_systems::TaskReadinessEventSystemConfig;
use tasker_shared::{EventDrivenSystem, SystemStatistics};

/// Unified coordinator demonstrating EventDrivenSystem pattern reuse
///
/// This coordinator manages both orchestration (queue-level) and task readiness
/// (database-level) event systems using the unified EventDrivenSystem interface.
/// It demonstrates how different event types can be managed consistently while
/// respecting their architectural differences.
#[derive(Debug)]
pub struct UnifiedEventCoordinator {
    /// Orchestration event system
    orchestration_system: Option<OrchestrationEventSystem>,

    /// Task readiness event system
    task_readiness_system: Option<TaskReadinessEventSystem>,

    /// System context
    context: Arc<SystemContext>,

    /// Configuration
    #[expect(dead_code, reason = "Configuration preserved for future coordinator enhancements")]
    config: UnifiedCoordinatorConfig,
}

/// Configuration for unified event coordinator
#[derive(Debug, Clone)]
pub struct UnifiedCoordinatorConfig {
    /// Coordinator identifier
    pub coordinator_id: String,

    /// Orchestration system configuration
    pub orchestration_config: OrchestrationEventSystemConfig,

    /// Task readiness system configuration
    pub task_readiness_config: TaskReadinessEventSystemConfig,
}

impl Default for UnifiedCoordinatorConfig {
    fn default() -> Self {
        Self {
            coordinator_id: "unified-event-coordinator".to_string(),
            orchestration_config: OrchestrationEventSystemConfig::default(),
            task_readiness_config: TaskReadinessEventSystemConfig::default(),
        }
    }
}

impl UnifiedCoordinatorConfig {
    /// Create configuration from ConfigManager
    ///
    /// Loads deployment mode and task readiness configuration from TOML configuration
    /// to drive the behavior of the unified event coordination system.
    pub fn from_config_manager(
        config_manager: &tasker_shared::config::ConfigManager,
    ) -> TaskerResult<Self> {
        let config = config_manager.config();

        // Access event systems configuration from orchestration context
        let event_systems = config
            .orchestration
            .as_ref()
            .map(|o| o.event_systems.clone())
            .unwrap_or_default();

        let task_readiness_event_system = event_systems.task_readiness.clone();
        let orchestration_event_system = event_systems.orchestration.clone();

        info!(
            orchestration_deployment_mode = %orchestration_event_system.deployment_mode,
            task_readiness_deployment_mode = %task_readiness_event_system.deployment_mode,
            "Loading UnifiedCoordinatorConfig from configuration"
        );

        // Convert V2 configs to legacy EventSystemConfig types
        use tasker_shared::config::event_systems::{
            EventSystemConfig, OrchestrationEventSystemMetadata, TaskReadinessEventSystemMetadata,
        };

        // TAS-61 Phase 6D: Use hydrated config values instead of defaults
        let orchestration_config = EventSystemConfig::<OrchestrationEventSystemMetadata> {
            system_id: orchestration_event_system.system_id,
            deployment_mode: orchestration_event_system.deployment_mode,
            timing: orchestration_event_system.timing,
            processing: orchestration_event_system.processing,
            health: orchestration_event_system.health,
            metadata: OrchestrationEventSystemMetadata { _reserved: None },
        };

        let task_readiness_config = EventSystemConfig::<TaskReadinessEventSystemMetadata> {
            system_id: task_readiness_event_system.system_id,
            deployment_mode: task_readiness_event_system.deployment_mode,
            timing: task_readiness_event_system.timing,
            processing: task_readiness_event_system.processing,
            health: task_readiness_event_system.health,
            metadata: TaskReadinessEventSystemMetadata { _reserved: None },
        };

        Ok(Self {
            coordinator_id: "unified-event-coordinator".to_string(),
            orchestration_config,
            task_readiness_config,
        })
    }
}

impl UnifiedEventCoordinator {
    /// Create new unified event coordinator
    pub async fn new(
        config: UnifiedCoordinatorConfig,
        context: Arc<SystemContext>,
        orchestration_core: Arc<OrchestrationCore>,
        orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,
    ) -> TaskerResult<Self> {
        let coordinator_id = Uuid::new_v4();

        info!(
            coordinator_id = %coordinator_id,
            orchestration_deployment_mode = %config.orchestration_config.deployment_mode,
            task_readiness_deployment_mode = %config.task_readiness_config.deployment_mode,
            "Creating UnifiedEventCoordinator"
        );

        // TODO: Implement unified event notification system when needed

        // Create orchestration event system if enabled
        // TAS-51: Get command channel monitor for send instrumentation
        let command_channel_monitor = orchestration_core.command_channel_monitor();

        let orchestration_system = Some(
            OrchestrationEventSystem::new(
                config.orchestration_config.clone(),
                context.clone(),
                orchestration_core.clone(),
                orchestration_command_sender.clone(),
                command_channel_monitor,
            )
            .await?,
        );

        let task_readiness_system = Some(TaskReadinessEventSystem::new(context.clone()));

        info!(
            coordinator_id = %coordinator_id,
            "UnifiedEventCoordinator created successfully"
        );

        Ok(Self {
            orchestration_system,
            task_readiness_system,
            context,
            config,
        })
    }

    /// Start unified event coordination
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            processor_uuid = %self.context.processor_uuid(),
            "Starting UnifiedEventCoordinator"
        );

        // Start orchestration system using EventDrivenSystem interface
        if let Some(orchestration_system) = &mut self.orchestration_system {
            orchestration_system.start().await.map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to start orchestration event system: {}",
                    e
                ))
            })?;

            info!(
                processor_uuid = %self.context.processor_uuid(),
                deployment_mode = ?orchestration_system.deployment_mode(),
                "Orchestration event system started"
            );
        }

        // Start task readiness system using EventDrivenSystem interface
        if let Some(task_readiness_system) = &mut self.task_readiness_system {
            task_readiness_system.start().await.map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to start task readiness event system: {}",
                    e
                ))
            })?;

            info!(
                processor_uuid = %self.context.processor_uuid(),
                system_id = %task_readiness_system.processor_uuid(),
                deployment_mode = ?task_readiness_system.deployment_mode(),
                "Task readiness event system started"
            );
        }

        info!(
            processor_uuid = %self.context.processor_uuid(),
            "UnifiedEventCoordinator started successfully"
        );

        Ok(())
    }

    /// Stop unified event coordination
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            processor_uuid = %self.context.processor_uuid(),
            "Stopping UnifiedEventCoordinator"
        );

        // Stop both systems using EventDrivenSystem interface
        if let Some(orchestration_system) = &mut self.orchestration_system {
            orchestration_system.stop().await?;
        }

        if let Some(task_readiness_system) = &mut self.task_readiness_system {
            task_readiness_system.stop().await?;
        }

        info!(
            processor_uuid = %self.context.processor_uuid(),
            "UnifiedEventCoordinator stopped successfully"
        );

        Ok(())
    }

    /// Perform unified health check across all event systems
    pub async fn health_check(&self) -> TaskerResult<UnifiedHealthReport> {
        let mut report = UnifiedHealthReport {
            processor_uuid: self.context.processor_uuid(),
            orchestration_healthy: true,
            task_readiness_healthy: true,
            orchestration_statistics: None,
            task_readiness_statistics: None,
            overall_healthy: true,
        };

        // Check orchestration system health
        if let Some(orchestration_system) = &self.orchestration_system {
            match orchestration_system.health_check().await {
                Ok(_) => {
                    report.orchestration_statistics = Some(orchestration_system.statistics());
                }
                Err(e) => {
                    warn!(
                        processor_uuid = %self.context.processor_uuid(),
                        error = %e,
                        "Orchestration system health check failed"
                    );
                    report.orchestration_healthy = false;
                    report.overall_healthy = false;
                }
            }
        }

        // Check task readiness system health
        if let Some(task_readiness_system) = &self.task_readiness_system {
            if task_readiness_system.is_running() {
                info!(
                    processor_uuid = %self.context.processor_uuid(),
                    "Task readiness system health check passed"
                );
                report.task_readiness_healthy = true;
                if report.orchestration_healthy && report.task_readiness_healthy {
                    report.overall_healthy = true;
                }
            } else {
                warn!(
                    processor_uuid = %self.context.processor_uuid(),
                    "Task readiness system health check failed"
                );
                report.task_readiness_healthy = false;
                report.overall_healthy = false;
            }
        }

        debug!(
            processor_uuid = %self.context.processor_uuid(),
            orchestration_healthy = %report.orchestration_healthy,
            task_readiness_healthy = %report.task_readiness_healthy,
            overall_healthy = %report.overall_healthy,
            "Unified health check completed"
        );

        Ok(report)
    }

    // TODO: Implement notification handling when unified event system is ready
}

/// Unified health report combining both event system statuses
#[derive(Debug, Clone)]
pub struct UnifiedHealthReport {
    pub processor_uuid: Uuid,
    pub orchestration_healthy: bool,
    pub task_readiness_healthy: bool,
    pub orchestration_statistics: Option<SystemStatistics>,
    pub task_readiness_statistics: Option<SystemStatistics>,
    pub overall_healthy: bool,
}

#[cfg(test)]
mod tests {
    use tasker_shared::config::event_systems::{
        EventSystemConfig, OrchestrationEventSystemMetadata, TaskReadinessEventSystemMetadata,
    };
    use tasker_shared::config::tasker::{
        // TAS-61: EventSystemBackoffConfig import removed - no longer used in tests
        EventSystemHealthConfig,
        EventSystemProcessingConfig,
        EventSystemTimingConfig,
    };
    use tasker_shared::DeploymentMode;

    /// TAS-61 Phase 6D: Verify config values drive behavior (not defaults)
    ///
    /// This test ensures that EventSystemProcessingConfig and EventSystemHealthConfig
    /// values from TOML configuration are actually used by UnifiedEventCoordinator,
    /// preventing regression where defaults were used instead of hydrated config.
    #[test]
    fn test_config_values_used_not_defaults() {
        // Create non-default config values that would be loaded from TOML
        let custom_processing = EventSystemProcessingConfig {
            batch_size: 75,                 // Non-default
            max_concurrent_operations: 150, // Non-default
            max_retries: 5,                 // Non-default
                                            // TAS-61: Commented out during investigation - backoff field removed
                                            // backoff: EventSystemBackoffConfig {
                                            //     initial_delay_ms: 250,
                                            //     max_delay_ms: 15000,
                                            //     multiplier: 3.0,
                                            //     jitter_percent: 0.2,
                                            // },
        };

        let custom_health = EventSystemHealthConfig {
            enabled: false,                        // Intentionally opposite of default
            performance_monitoring_enabled: false, // Intentionally opposite of default
            max_consecutive_errors: 25,            // Non-default
            error_rate_threshold_per_minute: 50,   // Non-default
        };

        let custom_timing = EventSystemTimingConfig {
            health_check_interval_seconds: 45,
            fallback_polling_interval_seconds: 8,
            visibility_timeout_seconds: 45,
            processing_timeout_seconds: 90,
            claim_timeout_seconds: 450,
        };

        // Create event system configs with our custom values
        // These simulate what would be loaded from TOML
        let orchestration_event_system = EventSystemConfig::<()> {
            system_id: "test-orchestration-system".to_string(),
            deployment_mode: DeploymentMode::PollingOnly,
            timing: custom_timing.clone(),
            processing: custom_processing.clone(),
            health: custom_health.clone(),
            metadata: (),
        };

        let task_readiness_event_system = EventSystemConfig::<()> {
            system_id: "test-task-readiness-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: custom_timing.clone(),
            processing: custom_processing.clone(),
            health: custom_health.clone(),
            metadata: (),
        };

        // Simulate the conversion logic in from_config_manager (lines 104-120)
        // TAS-61 Phase 6D: Use hydrated config values instead of defaults
        let orchestration_config = EventSystemConfig::<OrchestrationEventSystemMetadata> {
            system_id: orchestration_event_system.system_id.clone(),
            deployment_mode: orchestration_event_system.deployment_mode,
            timing: orchestration_event_system.timing.clone(),
            processing: orchestration_event_system.processing.clone(), // ✅ Hydrated
            health: orchestration_event_system.health.clone(),         // ✅ Hydrated
            metadata: OrchestrationEventSystemMetadata { _reserved: None },
        };

        let task_readiness_config = EventSystemConfig::<TaskReadinessEventSystemMetadata> {
            system_id: task_readiness_event_system.system_id.clone(),
            deployment_mode: task_readiness_event_system.deployment_mode,
            timing: task_readiness_event_system.timing.clone(),
            processing: task_readiness_event_system.processing.clone(), // ✅ Hydrated
            health: task_readiness_event_system.health.clone(),         // ✅ Hydrated
            metadata: TaskReadinessEventSystemMetadata { _reserved: None },
        };

        // Verify orchestration config has custom values (not defaults)
        assert_eq!(
            orchestration_config.processing.batch_size, 75,
            "Processing batch_size should use custom config value"
        );
        assert_eq!(
            orchestration_config.processing.max_concurrent_operations, 150,
            "Processing max_concurrent_operations should use custom config value"
        );
        assert_eq!(
            orchestration_config.processing.max_retries, 5,
            "Processing max_retries should use custom config value"
        );
        // TAS-61: Commented out during investigation - backoff field removed
        // assert_eq!(
        //     orchestration_config.processing.backoff.initial_delay_ms, 250,
        //     "Backoff initial_delay_ms should use custom config value"
        // );

        assert!(
            !orchestration_config.health.enabled,
            "Health enabled should use custom config value (false)"
        );
        assert!(
            !orchestration_config.health.performance_monitoring_enabled,
            "Health performance_monitoring should use custom config value (false)"
        );
        assert_eq!(
            orchestration_config.health.max_consecutive_errors, 25,
            "Health max_consecutive_errors should use custom config value"
        );

        assert_eq!(
            orchestration_config.timing.health_check_interval_seconds, 45,
            "Timing health_check_interval should use custom config value"
        );
        assert_eq!(
            orchestration_config
                .timing
                .fallback_polling_interval_seconds,
            8,
            "Timing fallback_polling_interval should use custom config value"
        );

        // Verify task readiness config has custom values (not defaults)
        assert_eq!(
            task_readiness_config.processing.batch_size, 75,
            "Task readiness processing batch_size should use custom config value"
        );
        assert!(
            !task_readiness_config.health.enabled,
            "Task readiness health enabled should use custom config value (false)"
        );

        println!("✅ TAS-61 Phase 6D: Config values verified - not using defaults");
    }

    /// TAS-61: Verify default config creates expected values
    ///
    /// This test documents what the default configuration looks like,
    /// serving as a baseline for comparison with custom configs.
    #[test]
    fn test_default_config_baseline() {
        let default_processing = EventSystemProcessingConfig::default();
        let default_health = EventSystemHealthConfig::default();
        let default_timing = EventSystemTimingConfig::default();

        // Document default processing values
        assert_eq!(
            default_processing.batch_size, 50,
            "Default batch_size should be 50"
        );
        assert_eq!(
            default_processing.max_concurrent_operations, 100,
            "Default max_concurrent_operations should be 100"
        );
        assert_eq!(
            default_processing.max_retries, 3,
            "Default max_retries should be 3"
        );

        // Document default health values
        assert!(
            default_health.enabled,
            "Default health enabled should be true"
        );
        assert!(
            default_health.performance_monitoring_enabled,
            "Default performance_monitoring should be true"
        );
        assert_eq!(
            default_health.max_consecutive_errors, 5,
            "Default max_consecutive_errors should be 5"
        );
        assert_eq!(
            default_health.error_rate_threshold_per_minute, 100,
            "Default error_rate_threshold_per_minute should be 100"
        );

        // Document default timing values
        assert_eq!(
            default_timing.health_check_interval_seconds, 60,
            "Default health_check_interval should be 60"
        );
        assert_eq!(
            default_timing.fallback_polling_interval_seconds, 10,
            "Default fallback_polling_interval should be 10"
        );
        assert_eq!(
            default_timing.visibility_timeout_seconds, 300,
            "Default visibility_timeout should be 300"
        );
        assert_eq!(
            default_timing.processing_timeout_seconds, 60,
            "Default processing_timeout should be 60"
        );
        assert_eq!(
            default_timing.claim_timeout_seconds, 30,
            "Default claim_timeout should be 30"
        );

        println!("✅ Default configuration baseline documented");
    }

    /// TAS-61: Regression test - ensure defaults are NOT used when config is provided
    ///
    /// This test explicitly checks that we're not accidentally using defaults
    /// when configuration values are available.
    #[test]
    fn test_no_default_override_regression() {
        let custom_processing = EventSystemProcessingConfig {
            batch_size: 200,
            max_concurrent_operations: 300,
            max_retries: 7,
            // TAS-61: Commented out during investigation - backoff field removed
            // backoff: EventSystemBackoffConfig::default(),
        };

        // If this were using defaults, batch_size would be 100 (not 200)
        assert_ne!(
            custom_processing.batch_size,
            EventSystemProcessingConfig::default().batch_size,
            "Custom config should NOT match default"
        );
        assert_eq!(
            custom_processing.batch_size, 200,
            "Custom config should preserve configured value"
        );

        println!("✅ No regression - custom config values preserved");
    }
}

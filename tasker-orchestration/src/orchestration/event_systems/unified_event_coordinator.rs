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
pub struct UnifiedEventCoordinator {
    /// Orchestration event system
    orchestration_system: Option<OrchestrationEventSystem>,

    /// Task readiness event system
    task_readiness_system: Option<TaskReadinessEventSystem>,

    /// System context
    context: Arc<SystemContext>,

    /// Configuration
    #[allow(dead_code)]
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

        // Access task readiness configuration from unified event systems configuration
        let task_readiness_config = config.event_systems.task_readiness.clone();

        // Access orchestration configuration from unified event systems configuration
        let orchestration_config = config.event_systems.orchestration.clone();

        info!(
            orchestration_deployment_mode = %orchestration_config.deployment_mode,
            task_readiness_deployment_mode = %task_readiness_config.deployment_mode,
            "Loading UnifiedCoordinatorConfig from configuration"
        );

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

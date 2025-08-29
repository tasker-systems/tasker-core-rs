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
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use tasker_shared::DeploymentMode;
use crate::orchestration::{
    command_processor::OrchestrationCommand,
    OrchestrationCore,
};
use tasker_shared::{EventDrivenSystem, EventSystemStatistics, SystemStatistics};
use super::{
    OrchestrationEventSystem, OrchestrationEventSystemConfig,
    TaskReadinessEventSystem, TaskReadinessEventSystemConfig,
};

/// Unified coordinator demonstrating EventDrivenSystem pattern reuse
///
/// This coordinator manages both orchestration (queue-level) and task readiness
/// (database-level) event systems using the unified EventDrivenSystem interface.
/// It demonstrates how different event types can be managed consistently while
/// respecting their architectural differences.
pub struct UnifiedEventCoordinator {
    /// Coordinator identifier
    coordinator_id: Uuid,
    
    /// Orchestration event system
    orchestration_system: Option<OrchestrationEventSystem>,
    
    /// Task readiness event system  
    task_readiness_system: Option<TaskReadinessEventSystem>,
    
    /// System context
    context: Arc<SystemContext>,
    
    /// Configuration
    config: UnifiedCoordinatorConfig,
}

/// Configuration for unified event coordinator
#[derive(Debug, Clone)]
pub struct UnifiedCoordinatorConfig {
    /// Coordinator identifier
    pub coordinator_id: String,
    
    /// Deployment mode for both systems (unified approach)
    pub deployment_mode: DeploymentMode,
    
    /// Enable orchestration event coordination
    pub orchestration_enabled: bool,
    
    /// Enable task readiness event coordination
    pub task_readiness_enabled: bool,
    
    /// Orchestration system configuration
    pub orchestration_config: OrchestrationEventSystemConfig,
    
    /// Task readiness system configuration  
    pub task_readiness_config: TaskReadinessEventSystemConfig,
}

impl Default for UnifiedCoordinatorConfig {
    fn default() -> Self {
        Self {
            coordinator_id: "unified-event-coordinator".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            orchestration_enabled: true,
            task_readiness_enabled: true,
            orchestration_config: OrchestrationEventSystemConfig::default(),
            task_readiness_config: TaskReadinessEventSystemConfig::default(),
        }
    }
}

impl UnifiedCoordinatorConfig {
    /// Create configuration from ConfigManager (TAS-43)
    /// 
    /// Loads deployment mode and task readiness configuration from TOML configuration
    /// to drive the behavior of the unified event coordination system.
    pub fn from_config_manager(
        config_manager: &tasker_shared::config::ConfigManager,
    ) -> TaskerResult<Self> {
        let config = config_manager.config();
        
        // Access task readiness configuration directly from TaskerConfig struct
        let task_readiness_config = &config.task_readiness;

        // Use deployment mode directly from configuration
        let deployment_mode = match task_readiness_config.deployment_mode {
            tasker_shared::config::TaskReadinessDeploymentMode::PollingOnly => DeploymentMode::PollingOnly,
            tasker_shared::config::TaskReadinessDeploymentMode::Hybrid => DeploymentMode::Hybrid,
            tasker_shared::config::TaskReadinessDeploymentMode::EventDrivenOnly => DeploymentMode::EventDrivenOnly,
        };

        // Determine what systems to enable based on deployment mode
        let (orchestration_enabled, task_readiness_enabled) = match deployment_mode {
            DeploymentMode::PollingOnly => (false, false), // No event coordination
            DeploymentMode::Hybrid => (true, true),        // Both systems active
            DeploymentMode::EventDrivenOnly => (true, true), // Full event-driven
        };

        info!(
            deployment_mode = ?deployment_mode,
            orchestration_enabled = %orchestration_enabled,
            task_readiness_enabled = %task_readiness_enabled,
            "Loading UnifiedCoordinatorConfig from configuration"
        );

        Ok(Self {
            coordinator_id: "unified-event-coordinator".to_string(),
            deployment_mode,
            orchestration_enabled,
            task_readiness_enabled,
            orchestration_config: OrchestrationEventSystemConfig::default(),
            task_readiness_config: TaskReadinessEventSystemConfig::default(),
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
            deployment_mode = ?config.deployment_mode,
            orchestration_enabled = %config.orchestration_enabled,
            task_readiness_enabled = %config.task_readiness_enabled,
            "Creating UnifiedEventCoordinator"
        );

        // TODO: Implement unified event notification system when needed

        // Create orchestration event system if enabled
        let orchestration_system = if config.orchestration_enabled {
            let mut orchestration_config = config.orchestration_config.clone();
            orchestration_config.deployment_mode = config.deployment_mode.clone();
            orchestration_config.system_id = format!("{}-orchestration", config.coordinator_id);
            
            Some(OrchestrationEventSystem::new(
                orchestration_config,
                context.clone(),
                orchestration_core.clone(),
                orchestration_command_sender,
            ).await?)
        } else {
            None
        };

        // Create task readiness event system if enabled
        let task_readiness_system = if config.task_readiness_enabled {
            let mut task_readiness_config = config.task_readiness_config.clone();
            task_readiness_config.deployment_mode = config.deployment_mode.clone();
            task_readiness_config.system_id = format!("{}-task-readiness", config.coordinator_id);
            
            // TODO TAS-43: TaskReadinessEventSystem now requires command_sender parameter
            // This will be fixed when UnifiedEventCoordinator is wired into bootstrap
            // with access to the CommandProcessor's command_sender channel
            
            // Some(TaskReadinessEventSystem::new(
            //     task_readiness_config,
            //     context.clone(),
            //     orchestration_core.clone(),
            //     command_sender,  // Need to pass this from bootstrap
            // ).await?)
            
            None  // Temporarily disabled until bootstrap integration
        } else {
            None
        };

        // TODO: Implement notification handler when unified event system is ready

        info!(
            coordinator_id = %coordinator_id,
            "UnifiedEventCoordinator created successfully"
        );

        Ok(Self {
            coordinator_id,
            orchestration_system,
            task_readiness_system,
            context,
            config,
        })
    }

    /// Start unified event coordination
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            coordinator_id = %self.coordinator_id,
            "Starting UnifiedEventCoordinator"
        );

        // Start orchestration system using EventDrivenSystem interface
        if let Some(orchestration_system) = &mut self.orchestration_system {
            orchestration_system.start().await.map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to start orchestration event system: {}", e
                ))
            })?;
            
            info!(
                coordinator_id = %self.coordinator_id,
                system_id = %orchestration_system.system_id(),
                deployment_mode = ?orchestration_system.deployment_mode(),
                "Orchestration event system started"
            );
        }

        // Start task readiness system using EventDrivenSystem interface
        if let Some(task_readiness_system) = &mut self.task_readiness_system {
            task_readiness_system.start().await.map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to start task readiness event system: {}", e
                ))
            })?;
            
            info!(
                coordinator_id = %self.coordinator_id,
                system_id = %task_readiness_system.system_id(),
                deployment_mode = ?task_readiness_system.deployment_mode(),
                "Task readiness event system started"
            );
        }

        info!(
            coordinator_id = %self.coordinator_id,
            "UnifiedEventCoordinator started successfully"
        );

        Ok(())
    }

    /// Stop unified event coordination
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            coordinator_id = %self.coordinator_id,
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
            coordinator_id = %self.coordinator_id,
            "UnifiedEventCoordinator stopped successfully"
        );

        Ok(())
    }

    /// Perform unified health check across all event systems
    pub async fn health_check(&self) -> TaskerResult<UnifiedHealthReport> {
        let mut report = UnifiedHealthReport {
            coordinator_id: self.coordinator_id,
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
                },
                Err(e) => {
                    warn!(
                        coordinator_id = %self.coordinator_id,
                        error = %e,
                        "Orchestration system health check failed"
                    );
                    report.orchestration_healthy = false;
                    report.overall_healthy = false;
                },
            }
        }

        // Check task readiness system health
        if let Some(task_readiness_system) = &self.task_readiness_system {
            match task_readiness_system.health_check().await {
                Ok(_) => {
                    report.task_readiness_statistics = Some(task_readiness_system.statistics());
                },
                Err(e) => {
                    warn!(
                        coordinator_id = %self.coordinator_id,
                        error = %e,
                        "Task readiness system health check failed"
                    );
                    report.task_readiness_healthy = false;
                    report.overall_healthy = false;
                },
            }
        }

        debug!(
            coordinator_id = %self.coordinator_id,
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
    pub coordinator_id: Uuid,
    pub orchestration_healthy: bool,
    pub task_readiness_healthy: bool,
    pub orchestration_statistics: Option<SystemStatistics>,
    pub task_readiness_statistics: Option<SystemStatistics>,
    pub overall_healthy: bool,
}
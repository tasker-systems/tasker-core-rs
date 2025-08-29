//! # TAS-43 Event-Driven Message Processor Bridge (LEGACY COMPATIBILITY)
//!
//! This module provides a compatibility bridge for the previous event-driven architecture,
//! now delegating to the new modular WorkerEventSystem architecture implemented in
//! `event_systems/worker_event_system.rs`.
//!
//! ## Migration Strategy
//!
//! - **New Architecture**: Delegates to WorkerEventSystem with unified deployment modes
//! - **Legacy API Compatibility**: Maintains existing API surface for gradual migration
//! - **Configuration Bridging**: Maps old EventDrivenConfig to new WorkerEventSystemConfig
//! - **Statistics Mapping**: Bridges old stats to new comprehensive metrics

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;
use uuid::Uuid;

use tasker_shared::{
    event_system::{deployment::DeploymentMode, event_driven::EventDrivenSystem},
    system_context::SystemContext,
    TaskerError, TaskerResult,
};

use super::command_processor::WorkerCommand;
use super::event_systems::worker_event_system::{WorkerEventSystem, WorkerEventSystemConfig};
use super::task_template_manager::TaskTemplateManager;
use super::worker_queues::{fallback_poller::WorkerPollerConfig, listener::WorkerListenerConfig};

/// Configuration for event-driven message processing
#[derive(Debug, Clone)]
pub struct EventDrivenConfig {
    /// Enable event-driven processing (vs polling-only)
    pub event_driven_enabled: bool,
    /// Fallback polling interval (for reliability)
    pub fallback_polling_interval: Duration,
    /// Message batch size for processing
    pub batch_size: u32,
    /// Visibility timeout for message processing
    pub visibility_timeout: Duration,
}

impl Default for EventDrivenConfig {
    fn default() -> Self {
        Self {
            event_driven_enabled: true,
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
        }
    }
}

/// Event-driven message processor bridge that delegates to WorkerEventSystem
pub struct EventDrivenMessageProcessor {
    /// Legacy configuration for API compatibility
    config: EventDrivenConfig,

    /// System context (maintained for API compatibility)
    #[allow(dead_code)]
    context: Arc<SystemContext>,

    /// Task template manager (maintained for API compatibility)
    #[allow(dead_code)]
    task_template_manager: Arc<TaskTemplateManager>,

    /// The new WorkerEventSystem that handles all functionality
    worker_event_system: Option<WorkerEventSystem>,

    /// Command sender bridge (maintained for API compatibility)
    #[allow(dead_code)]
    worker_command_sender: mpsc::Sender<WorkerCommand>,

    /// Processor ID for logging and compatibility
    processor_id: Uuid,

    /// Running state
    is_running: bool,

    /// Cached supported namespaces for API compatibility
    supported_namespaces: Vec<String>,
}

impl EventDrivenMessageProcessor {
    /// Create new event-driven message processor (delegates to WorkerEventSystem)
    pub async fn new(
        config: EventDrivenConfig,
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
        worker_command_sender: mpsc::Sender<WorkerCommand>,
    ) -> TaskerResult<Self> {
        // Build supported namespaces list from task template manager + "default"
        let mut supported_namespaces = task_template_manager.supported_namespaces().to_vec();
        if !supported_namespaces.contains(&"default".to_string()) {
            supported_namespaces.push("default".to_string());
        }

        info!(
            supported_namespaces = ?supported_namespaces,
            "Creating EventDrivenMessageProcessor bridge with delegation to WorkerEventSystem"
        );

        let processor_id = Uuid::now_v7();

        // Create the new WorkerEventSystem with mapped configuration
        let worker_event_config =
            Self::map_config_to_new_architecture(&config, &supported_namespaces, processor_id);
        let pgmq_client = context.message_client().clone();
        let worker_event_system = WorkerEventSystem::new(
            worker_event_config,
            context.clone(),
            worker_command_sender.clone(),
            pgmq_client,
        )
        .await?;

        Ok(Self {
            config,
            context,
            task_template_manager,
            worker_event_system: Some(worker_event_system),
            worker_command_sender,
            processor_id,
            is_running: false,
            supported_namespaces,
        })
    }

    /// Map legacy EventDrivenConfig to new WorkerEventSystemConfig
    fn map_config_to_new_architecture(
        legacy_config: &EventDrivenConfig,
        supported_namespaces: &[String],
        processor_id: Uuid,
    ) -> WorkerEventSystemConfig {
        let deployment_mode = if legacy_config.event_driven_enabled {
            DeploymentMode::Hybrid
        } else {
            DeploymentMode::PollingOnly
        };

        WorkerEventSystemConfig {
            system_id: format!("legacy-event-driven-{}", processor_id),
            deployment_mode,
            supported_namespaces: supported_namespaces.to_vec(),
            health_monitoring_enabled: true,
            health_check_interval: Duration::from_secs(60),
            max_concurrent_processors: legacy_config.batch_size as usize,
            processing_timeout: legacy_config.visibility_timeout,
            listener: WorkerListenerConfig {
                supported_namespaces: supported_namespaces.to_vec(),
                retry_interval: Duration::from_secs(5),
                max_retry_attempts: 3,
                event_timeout: legacy_config.visibility_timeout,
                health_check_interval: Duration::from_secs(60),
                batch_processing: true,
                connection_timeout: Duration::from_secs(10),
            },
            fallback_poller: WorkerPollerConfig {
                enabled: true,
                polling_interval: legacy_config.fallback_polling_interval,
                batch_size: legacy_config.batch_size,
                age_threshold: Duration::from_secs(2),
                max_age: Duration::from_secs(12 * 60 * 60),
                supported_namespaces: supported_namespaces.to_vec(),
                visibility_timeout: legacy_config.visibility_timeout,
            },
        }
    }

    /// Start event-driven message processing (delegates to WorkerEventSystem)
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            processor_id = %self.processor_id,
            supported_namespaces = ?self.supported_namespaces,
            deployment_mode = ?self.config.event_driven_enabled,
            "Starting EventDrivenMessageProcessor bridge - delegating to WorkerEventSystem"
        );

        self.is_running = true;

        if let Some(ref mut worker_event_system) = self.worker_event_system {
            worker_event_system.start().await?;
        } else {
            return Err(TaskerError::WorkerError(
                "WorkerEventSystem not initialized".to_string(),
            ));
        }

        info!(
            processor_id = %self.processor_id,
            "EventDrivenMessageProcessor bridge started successfully - delegated to WorkerEventSystem"
        );

        Ok(())
    }

    /// Stop event-driven message processing (delegates to WorkerEventSystem)
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            processor_id = %self.processor_id,
            "Stopping EventDrivenMessageProcessor bridge - delegating to WorkerEventSystem"
        );

        self.is_running = false;

        if let Some(ref mut worker_event_system) = self.worker_event_system {
            worker_event_system.stop().await?;
        }

        info!(
            processor_id = %self.processor_id,
            "EventDrivenMessageProcessor bridge stopped successfully"
        );

        Ok(())
    }

    /// Check if processor is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Get processor statistics (delegates to WorkerEventSystem)
    pub async fn get_stats(&self) -> EventDrivenStats {
        if let Some(ref worker_event_system) = self.worker_event_system {
            let new_stats = worker_event_system.statistics();

            // Map new statistics to legacy format for API compatibility
            EventDrivenStats {
                processor_id: self.processor_id,
                supported_namespaces: self.supported_namespaces.clone(),
                event_driven_enabled: self.config.event_driven_enabled,
                listener_connected: self.is_running, // Use running state as health indicator
                messages_processed: new_stats.events_processed,
                events_received: new_stats.events_processed, // Map to same field for simplicity
            }
        } else {
            // Fallback stats if WorkerEventSystem is not initialized
            EventDrivenStats {
                processor_id: self.processor_id,
                supported_namespaces: self.supported_namespaces.clone(),
                event_driven_enabled: self.config.event_driven_enabled,
                listener_connected: false,
                messages_processed: 0,
                events_received: 0,
            }
        }
    }
}

// NOTE: Old MessageEventHandler implementation removed - now delegated to
// WorkerEventSystem architecture in event_systems/worker_event_system.rs

/// Statistics for event-driven processing
#[derive(Debug, Clone)]
pub struct EventDrivenStats {
    pub processor_id: Uuid,
    pub supported_namespaces: Vec<String>,
    pub event_driven_enabled: bool,
    pub listener_connected: bool,
    pub messages_processed: u64,
    pub events_received: u64,
}

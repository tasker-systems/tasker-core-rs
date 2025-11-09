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
    config::event_systems::{
        BackoffConfig, EventSystemHealthConfig, EventSystemProcessingConfig,
        EventSystemTimingConfig, FallbackPollerConfig, InProcessEventsConfig,
        ListenerConfig as UnifiedWorkerListenerConfig, ResourceLimitsConfig,
        WorkerEventSystemMetadata,
    },
    event_system::{deployment::DeploymentMode, event_driven::EventDrivenSystem},
    system_context::SystemContext,
    TaskerError, TaskerResult,
};

use super::command_processor::WorkerCommand;
use super::event_systems::worker_event_system::{WorkerEventSystem, WorkerEventSystemConfig};
use super::task_template_manager::TaskTemplateManager;

/// Configuration for event-driven message processing
#[derive(Debug, Clone)]
pub struct EventDrivenConfig {
    /// Fallback polling interval (for reliability)
    pub fallback_polling_interval: Duration,
    /// Message batch size for processing
    pub batch_size: u32,
    /// Visibility timeout for message processing
    pub visibility_timeout: Duration,

    /// Deployment mode for event-driven processing
    pub deployment_mode: DeploymentMode,
}

impl Default for EventDrivenConfig {
    fn default() -> Self {
        Self {
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
            deployment_mode: DeploymentMode::Hybrid,
        }
    }
}

/// Event-driven message processor bridge that delegates to WorkerEventSystem
pub(crate) struct EventDrivenMessageProcessor {
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
        let supported_namespaces = task_template_manager.supported_namespaces().await.to_vec();

        info!(
            supported_namespaces = ?supported_namespaces,
            "Creating EventDrivenMessageProcessor bridge with delegation to WorkerEventSystem"
        );

        let processor_id = Uuid::now_v7();

        // Create the new WorkerEventSystem with mapped configuration
        let worker_event_config =
            Self::map_config_to_new_architecture(&config, &supported_namespaces, processor_id);
        let worker_event_system = WorkerEventSystem::new(
            worker_event_config,
            worker_command_sender.clone(),
            context.clone(),
            supported_namespaces.clone(),
        );

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
        edd_config: &EventDrivenConfig,
        supported_namespaces: &[String],
        processor_id: Uuid,
    ) -> WorkerEventSystemConfig {
        let deployment_mode = edd_config.deployment_mode.clone();

        WorkerEventSystemConfig {
            system_id: format!("legacy-event-driven-{}", processor_id),
            deployment_mode,
            timing: EventSystemTimingConfig {
                health_check_interval_seconds: 60,
                fallback_polling_interval_seconds: edd_config.fallback_polling_interval.as_secs()
                    as u32,
                visibility_timeout_seconds: edd_config.visibility_timeout.as_secs() as u32,
                processing_timeout_seconds: edd_config.visibility_timeout.as_secs() as u32,
                claim_timeout_seconds: 30,
            },
            processing: EventSystemProcessingConfig {
                max_concurrent_operations: edd_config.batch_size,
                batch_size: edd_config.batch_size,
                max_retries: 3,
                backoff: BackoffConfig {
                    initial_delay_ms: 1000,
                    max_delay_ms: 60000,
                    multiplier: 2.0,
                    jitter_percent: 0.1,
                },
            },
            health: EventSystemHealthConfig {
                enabled: true,
                performance_monitoring_enabled: true,
                max_consecutive_errors: 10,
                error_rate_threshold_per_minute: 60,
            },
            metadata: WorkerEventSystemMetadata {
                in_process_events: InProcessEventsConfig {
                    ffi_integration_enabled: true,
                    deduplication_cache_size: 10000,
                },
                listener: UnifiedWorkerListenerConfig {
                    retry_interval_seconds: 5,
                    max_retry_attempts: 3,
                    event_timeout_seconds: edd_config.visibility_timeout.as_secs() as u32,
                    batch_processing: true,
                    connection_timeout_seconds: 10,
                },
                fallback_poller: FallbackPollerConfig {
                    enabled: true,
                    polling_interval_ms: edd_config.fallback_polling_interval.as_millis() as u32,
                    batch_size: edd_config.batch_size,
                    age_threshold_seconds: 60,
                    max_age_hours: 24,
                    visibility_timeout_seconds: edd_config.visibility_timeout.as_secs() as u32,
                    supported_namespaces: supported_namespaces.into(),
                },
                resource_limits: ResourceLimitsConfig {
                    max_memory_mb: 1024,
                    max_cpu_percent: 80.0,
                    max_database_connections: 10,
                    max_queue_connections: 5,
                },
            },
        }
    }

    /// Start event-driven message processing (delegates to WorkerEventSystem)
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            processor_id = %self.processor_id,
            supported_namespaces = ?self.supported_namespaces,
            deployment_mode = ?self.config.deployment_mode,
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
                deployment_mode: self.config.deployment_mode.clone(),
                listener_connected: self.is_running, // Use running state as health indicator
                messages_processed: new_stats.events_processed,
                events_received: new_stats.events_processed, // Map to same field for simplicity
            }
        } else {
            // Fallback stats if WorkerEventSystem is not initialized
            EventDrivenStats {
                processor_id: self.processor_id,
                supported_namespaces: self.supported_namespaces.clone(),
                deployment_mode: self.config.deployment_mode.clone(),
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
    pub deployment_mode: DeploymentMode,
    pub listener_connected: bool,
    pub messages_processed: u64,
    pub events_received: u64,
}

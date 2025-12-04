//! # TAS-69 WorkerCore with Actor-Based Architecture
//! # TAS-43 Event-Driven Message Processing Integration
//!
//! This module provides the main worker system core using the actor-based
//! command-actor-service architecture (TAS-69).
//!
//! ## Key Features
//!
//! - **Actor-Based Command Processing**: Pure routing via ActorCommandProcessor
//! - **Event-Driven Processing**: Real-time message processing via PostgreSQL LISTEN/NOTIFY
//! - **Hybrid Reliability**: Event-driven with fallback polling for guaranteed processing
//! - **Step Execution**: Manages step message processing and result publishing
//! - **TaskTemplate Management**: Local template caching and validation
//! - **Orchestration API**: HTTP client for orchestration service communication
//! - **Health Monitoring**: Worker-specific health checks and status reporting

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use uuid::Uuid;

use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use super::actor_command_processor::ActorCommandProcessor;
use super::command_processor::{WorkerCommand, WorkerStatus};
use super::event_driven_processor::{
    EventDrivenConfig, EventDrivenMessageProcessor, EventDrivenStats,
};
use super::event_router::EventRouter;
use super::event_systems::domain_event_system::{
    DomainEventSystem, DomainEventSystemConfig, DomainEventSystemHandle,
};
use super::in_process_event_bus::{InProcessEventBus, InProcessEventBusConfig};
use super::event_publisher::WorkerEventPublisher;
use super::step_event_publisher::{PublishResult, StepEventContext, StepEventPublisher};
use super::step_event_publisher_registry::StepEventPublisherRegistry;
use super::task_template_manager::TaskTemplateManager;
use crate::health::WorkerHealthStatus;
use std::sync::RwLock;
use tasker_client::api_clients::orchestration_client::{
    OrchestrationApiClient, OrchestrationApiConfig,
};
use tasker_shared::events::domain_events::DomainEventPublisher;
use tasker_shared::messaging::execution_types::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tokio::sync::RwLock as TokioRwLock;

/// TAS-69 Actor-Based WorkerCore with TAS-43 Event-Driven Integration
///
/// Focused on worker-specific operations with real-time event processing:
/// - Actor-based command processing via ActorCommandProcessor
/// - Event-driven step message processing via PostgreSQL LISTEN/NOTIFY
/// - Hybrid reliability with fallback polling for guaranteed processing
/// - Publishing results back to orchestration
/// - Managing local task template cache
/// - Health monitoring and metrics
pub struct WorkerCore {
    /// System context for dependency injection
    pub context: Arc<SystemContext>,

    /// Command sender for worker operations
    command_sender: mpsc::Sender<WorkerCommand>,

    /// TAS-69: Actor-based command processor
    /// Uses actor pattern for pure routing and typed message handling
    #[allow(dead_code)]
    processor: Option<ActorCommandProcessor>,

    /// Event-driven message processor for real-time message handling
    event_driven_processor: Option<EventDrivenMessageProcessor>,

    /// Task template manager for local caching
    pub task_template_manager: Arc<TaskTemplateManager>,

    /// Orchestration API client for service communication
    pub orchestration_client: Arc<OrchestrationApiClient>,

    /// TAS-65 Phase 3: Step event publisher registry
    /// Registry of custom event publishers keyed by name (from YAML `publisher:` field)
    step_event_publisher_registry: Arc<RwLock<StepEventPublisherRegistry>>,

    /// TAS-65 Phase 3: Domain event publisher for PGMQ
    /// Used to create StepEventPublisherContext when invoking publishers
    domain_event_publisher: Arc<DomainEventPublisher>,

    /// TAS-65/TAS-69 Phase 8: Domain event system for background event processing
    /// Stored here so it can be spawned in start() and kept alive
    domain_event_system: Option<DomainEventSystem>,

    /// TAS-65/TAS-69 Phase 8: Domain event system handle for shutdown
    /// Kept for graceful shutdown coordination
    domain_event_handle: Option<DomainEventSystemHandle>,

    /// TAS-65/TAS-69 Phase 8: In-process event bus for fast event subscribers
    /// Exposed for registering Rust subscribers
    pub in_process_bus: Arc<TokioRwLock<InProcessEventBus>>,

    /// TAS-65: Event router for domain event statistics
    /// Kept for debug/observability endpoints
    event_router: Option<Arc<EventRouter>>,

    /// JoinHandle for the processor background task
    /// Stored to await clean shutdown and detect panics
    processor_task_handle: Option<JoinHandle<()>>,

    /// JoinHandle for the domain event system background task
    /// Stored to await clean shutdown and detect panics
    domain_event_task_handle: Option<JoinHandle<()>>,

    /// System status
    pub status: WorkerCoreStatus,

    /// Core configuration
    pub core_id: Uuid,
}

impl std::fmt::Debug for WorkerCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let registry_count = self
            .step_event_publisher_registry
            .read()
            .map(|r| r.len())
            .unwrap_or(0);
        f.debug_struct("WorkerCore")
            .field("core_id", &self.core_id)
            .field("status", &self.status)
            .field("has_processor", &self.processor.is_some())
            .field("has_event_driven", &self.event_driven_processor.is_some())
            .field(
                "has_domain_event_system",
                &self.domain_event_system.is_some(),
            )
            .field(
                "processor_task_running",
                &self.processor_task_handle.is_some(),
            )
            .field(
                "domain_event_task_running",
                &self.domain_event_task_handle.is_some(),
            )
            .field("registered_event_publishers", &registry_count)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkerCoreStatus {
    Created,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

impl WorkerCore {
    /// Create new WorkerCore with TAS-69 actor-based architecture
    pub async fn new(
        context: Arc<SystemContext>,
        orchestration_config: OrchestrationApiConfig,
    ) -> TaskerResult<Self> {
        Self::new_with_event_system(context, orchestration_config, None).await
    }

    /// Create new WorkerCore with external event system
    ///
    /// TAS-69: Uses ActorCommandProcessor for pure routing and typed message handling
    pub async fn new_with_event_system(
        context: Arc<SystemContext>,
        orchestration_config: OrchestrationApiConfig,
        event_system: Option<Arc<tasker_shared::events::WorkerEventSystem>>,
    ) -> TaskerResult<Self> {
        info!("Creating WorkerCore with TAS-69 Actor-Based Architecture");

        // Create worker-specific components
        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let discovery_result = task_template_manager.ensure_templates_in_database().await?;

        if !discovery_result.discovered_namespaces.is_empty() {
            let namespace_refs: Vec<&str> = discovery_result
                .discovered_namespaces
                .iter()
                .map(|ns| ns.as_str())
                .collect();

            // Initialize worker queues for step execution
            context.initialize_queues(&namespace_refs).await?;

            // TAS-65 Phase 2: Initialize domain event queues for event publishing
            context
                .initialize_domain_event_queues(&namespace_refs)
                .await?;

            task_template_manager
                .set_supported_namespaces(discovery_result.discovered_namespaces.clone())
                .await;
        }

        let orchestration_client = Arc::new(
            OrchestrationApiClient::new(orchestration_config).map_err(|e| {
                TaskerError::ConfigurationError(format!(
                    "Failed to create orchestration client: {}",
                    e
                ))
            })?,
        );

        // TAS-65 Phase 3: Create domain event publisher for step events
        let domain_event_publisher = Arc::new(DomainEventPublisher::new(context.message_client()));

        // TAS-65 Phase 3: Initialize step event publisher registry with domain publisher
        let step_event_publisher_registry = Arc::new(RwLock::new(StepEventPublisherRegistry::new(
            domain_event_publisher.clone(),
        )));

        // TAS-65/TAS-69: Load worker config for domain event system
        let worker_config = context
            .tasker_config
            .worker
            .as_ref()
            .expect("Worker configuration required for domain event system");

        // TAS-65/TAS-69: Create in-process event bus
        let in_process_channels = &worker_config.mpsc_channels.in_process_events;
        let in_process_bus_config = InProcessEventBusConfig {
            ffi_channel_buffer_size: in_process_channels.broadcast_buffer_size as usize,
            log_subscriber_errors: in_process_channels.log_subscriber_errors,
            dispatch_timeout_ms: in_process_channels.dispatch_timeout_ms as u64,
        };
        let in_process_bus = Arc::new(TokioRwLock::new(InProcessEventBus::new(
            in_process_bus_config,
        )));

        // TAS-65/TAS-69: Create event router for dual-path delivery
        let event_router = Arc::new(EventRouter::new(
            domain_event_publisher.clone(),
            in_process_bus.clone(),
        ));

        // TAS-65/TAS-69: Create domain event system and handle
        let domain_event_channels = &worker_config.mpsc_channels.domain_events;
        let domain_event_system_config = DomainEventSystemConfig {
            channel_buffer_size: domain_event_channels.command_buffer_size as usize,
            shutdown_drain_timeout_ms: domain_event_channels.shutdown_drain_timeout_ms as u64,
            log_dropped_events: domain_event_channels.log_dropped_events,
        };
        let (domain_event_system, domain_event_handle) =
            DomainEventSystem::new(event_router.clone(), domain_event_system_config);

        // TAS-69: Create shared event system for FFI handlers
        // CRITICAL: Both event_publisher and event_subscriber MUST share the same event system
        // Otherwise FFI handlers won't receive step execution events
        let worker_id = format!("worker_{}", uuid::Uuid::now_v7());
        let shared_event_system = event_system
            .unwrap_or_else(|| Arc::new(tasker_shared::events::WorkerEventSystem::new()));
        let event_publisher =
            WorkerEventPublisher::with_event_system(worker_id.clone(), shared_event_system.clone());

        // TAS-51: Use configured buffer size for worker command processor
        let command_buffer_size = worker_config
            .mpsc_channels
            .command_processor
            .command_buffer_size as usize;

        // TAS-51: Initialize channel monitor for observability
        let command_channel_monitor = tasker_shared::monitoring::ChannelMonitor::new(
            "worker_command_channel",
            command_buffer_size,
        );

        // TAS-69: Create ActorCommandProcessor with all dependencies upfront
        // This eliminates two-phase initialization complexity
        let (mut processor, command_sender) = ActorCommandProcessor::new(
            context.clone(),
            worker_id,
            task_template_manager.clone(),
            event_publisher,
            domain_event_handle.clone(),
            command_buffer_size,
            command_channel_monitor,
        )
        .await?;

        // Enable event subscriber for completion events using shared event system
        processor
            .enable_event_subscriber(Some(shared_event_system))
            .await;

        // Create EventDrivenMessageProcessor for TAS-43 integration
        let event_driven_config = EventDrivenConfig {
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
            deployment_mode: context
                .tasker_config
                .worker
                .as_ref()
                .map(|w| w.event_systems.worker.deployment_mode)
                .unwrap_or_else(|| {
                    panic!("Worker configuration required for event-driven processing")
                }),
        };

        let event_driven_processor = EventDrivenMessageProcessor::new(
            event_driven_config,
            context.clone(),
            task_template_manager.clone(),
            command_sender.clone(),
        )
        .await
        .map_err(|e| {
            TaskerError::WorkerError(format!("Failed to create event-driven processor: {e}"))
        })?;

        let core_id = Uuid::now_v7();

        info!(
            core_id = %core_id,
            "WorkerCore initialized with TAS-69 actor-based processor"
        );

        Ok(Self {
            context,
            command_sender,
            processor: Some(processor),
            event_driven_processor: Some(event_driven_processor),
            task_template_manager,
            orchestration_client,
            step_event_publisher_registry,
            domain_event_publisher,
            domain_event_system: Some(domain_event_system),
            domain_event_handle: Some(domain_event_handle),
            in_process_bus,
            event_router: Some(event_router),
            processor_task_handle: None,
            domain_event_task_handle: None,
            status: WorkerCoreStatus::Created,
            core_id,
        })
    }

    /// Start the worker core with event-driven processing
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!("Starting WorkerCore with TAS-69 actor-based architecture");

        self.status = WorkerCoreStatus::Starting;

        // Start event-driven message processor for real-time processing
        if let Some(ref mut event_driven_processor) = self.event_driven_processor {
            event_driven_processor.start().await.map_err(|e| {
                TaskerError::WorkerError(format!("Failed to start event-driven processor: {e}"))
            })?;

            info!(
                core_id = %self.core_id,
                "Event-driven message processor started successfully"
            );
        }

        // TAS-69: Start actor-based processor
        if let Some(mut processor) = self.processor.take() {
            info!(
                core_id = %self.core_id,
                "Starting ActorCommandProcessor"
            );
            let handle = tokio::spawn(async move {
                if let Err(e) = processor.start_with_events().await {
                    tracing::error!("ActorCommandProcessor error: {}", e);
                }
            });
            self.processor_task_handle = Some(handle);
        }

        // TAS-65/TAS-69: Start domain event system background processing
        if let Some(domain_event_system) = self.domain_event_system.take() {
            info!(
                core_id = %self.core_id,
                "Starting DomainEventSystem background processing loop"
            );
            // Store the JoinHandle so we can await clean shutdown
            let handle = tokio::spawn(async move {
                domain_event_system.run().await;
            });
            self.domain_event_task_handle = Some(handle);
        }

        self.status = WorkerCoreStatus::Running;

        info!(
            core_id = %self.core_id,
            "WorkerCore started successfully with TAS-40 command pattern, TAS-43 event-driven integration, and TAS-65 domain events"
        );

        Ok(())
    }

    /// Stop the worker core and event-driven processing
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!("Stopping WorkerCore with event-driven processing");

        self.status = WorkerCoreStatus::Stopping;

        // Stop event-driven message processor first
        if let Some(ref mut event_driven_processor) = self.event_driven_processor {
            if let Err(e) = event_driven_processor.stop().await {
                warn!(
                    core_id = %self.core_id,
                    error = %e,
                    "Failed to stop event-driven processor cleanly"
                );
            } else {
                info!(
                    core_id = %self.core_id,
                    "Event-driven message processor stopped successfully"
                );
            }
        }

        // Send shutdown command to worker processor
        let (resp_tx, resp_rx) = oneshot::channel();
        if let Err(e) = self
            .command_sender
            .send(WorkerCommand::Shutdown { resp: resp_tx })
            .await
        {
            warn!("Failed to send shutdown command: {e}");
        } else {
            // Wait for shutdown acknowledgment
            if let Err(e) = tokio::time::timeout(Duration::from_secs(10), resp_rx).await {
                warn!("Shutdown acknowledgment timeout: {e}");
            }
        }

        // Await processor task completion to ensure clean shutdown
        if let Some(handle) = self.processor_task_handle.take() {
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(Ok(())) => {
                    info!(
                        core_id = %self.core_id,
                        "Processor task completed successfully"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        core_id = %self.core_id,
                        error = %e,
                        "Processor task panicked during shutdown"
                    );
                }
                Err(_) => {
                    warn!(
                        core_id = %self.core_id,
                        "Processor task did not complete within timeout"
                    );
                }
            }
        }

        // TAS-65/TAS-69 Phase 8: Gracefully shutdown domain event system
        if let Some(ref handle) = self.domain_event_handle {
            info!(
                core_id = %self.core_id,
                "Initiating domain event system shutdown"
            );
            let shutdown_result = handle.shutdown().await;
            info!(
                core_id = %self.core_id,
                events_drained = shutdown_result.events_drained,
                duration_ms = shutdown_result.duration_ms,
                success = shutdown_result.success,
                "Domain event system shutdown complete"
            );
        }

        // Await domain event task completion to ensure clean shutdown
        if let Some(handle) = self.domain_event_task_handle.take() {
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(Ok(())) => {
                    info!(
                        core_id = %self.core_id,
                        "Domain event task completed successfully"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        core_id = %self.core_id,
                        error = %e,
                        "Domain event task panicked during shutdown"
                    );
                }
                Err(_) => {
                    warn!(
                        core_id = %self.core_id,
                        "Domain event task did not complete within timeout"
                    );
                }
            }
        }

        self.status = WorkerCoreStatus::Stopped;

        info!(
            core_id = %self.core_id,
            "WorkerCore stopped successfully"
        );

        Ok(())
    }

    /// Get worker health status including event-driven processor status
    pub async fn get_health(&self) -> TaskerResult<WorkerHealthStatus> {
        // Use GetWorkerStatus to get worker information, then convert to health status
        let worker_status = self.get_processing_stats().await?;

        // Get event-driven processor stats if available
        let event_driven_connected = if let Some(ref processor) = self.event_driven_processor {
            let stats = processor.get_stats().await;
            stats.listener_connected && processor.is_running()
        } else {
            false
        };

        // Convert WorkerStatus to WorkerHealthStatus
        Ok(WorkerHealthStatus {
            status: worker_status.status.clone(),
            database_connected: event_driven_connected, // Event-driven processor includes DB connectivity
            orchestration_api_reachable: true,          // TODO: Could check actual API connectivity
            supported_namespaces: self.task_template_manager.supported_namespaces().await,
            template_cache_stats: Some(self.task_template_manager.cache_stats().await),
            total_messages_processed: worker_status.steps_executed,
            successful_executions: worker_status.steps_succeeded,
            failed_executions: worker_status.steps_failed,
        })
    }

    /// Get processing statistics via command pattern
    pub async fn get_processing_stats(&self) -> TaskerResult<WorkerStatus> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(WorkerCommand::GetWorkerStatus { resp: resp_tx })
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Failed to send stats command: {e}")))?;

        resp_rx
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Stats response error: {e}")))?
    }

    /// Process step message via command pattern
    pub async fn process_step_message(
        &self,
        _message: tasker_shared::messaging::message::SimpleStepMessage,
    ) -> TaskerResult<tasker_shared::messaging::StepExecutionResult> {
        let (_resp_tx, _resp_rx) =
            oneshot::channel::<TaskerResult<tasker_shared::messaging::StepExecutionResult>>();

        // TODO: This method needs to be updated to use TaskSequenceStep instead of SimpleStepMessage
        // For now, we'll return an error to indicate this needs to be implemented properly
        Err(TaskerError::WorkerError(
            "process_step_message needs to be updated to use TaskSequenceStep pattern".to_string(),
        ))
    }

    /// Refresh task template cache via command pattern
    pub async fn refresh_template_cache(&self, namespace: Option<String>) -> TaskerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(WorkerCommand::RefreshTemplateCache {
                namespace,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                TaskerError::WorkerError(format!("Failed to send cache refresh command: {e}"))
            })?;

        resp_rx
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Cache refresh response error: {e}")))?
    }

    /// Get current worker core status
    pub fn status(&self) -> &WorkerCoreStatus {
        &self.status
    }

    /// Get core ID
    pub fn core_id(&self) -> &Uuid {
        &self.core_id
    }

    /// Get supported namespaces
    pub async fn supported_namespaces(&self) -> Vec<String> {
        self.task_template_manager.supported_namespaces().await
    }

    /// Check if a namespace is supported
    pub async fn is_namespace_supported(&self, namespace: &str) -> bool {
        self.task_template_manager
            .is_namespace_supported(namespace)
            .await
    }

    /// Get the command sender for communicating with the worker processor
    pub fn command_sender(&self) -> &mpsc::Sender<WorkerCommand> {
        &self.command_sender
    }

    /// Get event-driven processor statistics
    pub async fn get_event_driven_stats(&self) -> Option<EventDrivenStats> {
        if let Some(ref processor) = self.event_driven_processor {
            Some(processor.get_stats().await)
        } else {
            None
        }
    }

    /// Check if event-driven processing is enabled and running
    pub fn is_event_driven_enabled(&self) -> bool {
        if let Some(ref processor) = self.event_driven_processor {
            processor.is_running()
        } else {
            false
        }
    }

    /// Get event router reference for direct stats access (avoids mutex lock)
    ///
    /// Used by WorkerWebState to cache the event router during initialization.
    pub fn event_router(&self) -> Option<Arc<EventRouter>> {
        self.event_router.clone()
    }

    /// Get in-process event bus reference for direct stats access (avoids mutex lock)
    ///
    /// Used by WorkerWebState to cache the in-process bus during initialization.
    pub fn in_process_event_bus(&self) -> Arc<TokioRwLock<InProcessEventBus>> {
        self.in_process_bus.clone()
    }

    /// TAS-65: Get domain event statistics for observability
    ///
    /// Returns combined statistics from the EventRouter and InProcessEventBus.
    /// Used by the `/debug/events` endpoint for E2E test verification.
    ///
    /// # Returns
    ///
    /// `DomainEventStats` containing:
    /// - Router stats (durable_routed, fast_routed, broadcast_routed)
    /// - In-process bus stats (total_events_dispatched, handler counts)
    pub async fn get_domain_event_stats(&self) -> tasker_shared::types::web::DomainEventStats {
        use tasker_shared::types::web::{
            DomainEventStats, EventRouterStats as WebEventRouterStats,
            InProcessEventBusStats as WebInProcessEventBusStats,
        };

        // Get router stats
        let router_stats = if let Some(ref router) = self.event_router {
            let stats = router.get_statistics();
            WebEventRouterStats {
                total_routed: stats.total_routed,
                durable_routed: stats.durable_routed,
                fast_routed: stats.fast_routed,
                broadcast_routed: stats.broadcast_routed,
                fast_delivery_errors: stats.fast_delivery_errors,
                routing_errors: stats.routing_errors,
            }
        } else {
            WebEventRouterStats::default()
        };

        // Get in-process bus stats
        let bus_stats = {
            let bus = self.in_process_bus.read().await;
            let stats = bus.get_statistics();
            WebInProcessEventBusStats {
                total_events_dispatched: stats.total_events_dispatched,
                rust_handler_dispatches: stats.rust_handler_dispatches,
                ffi_channel_dispatches: stats.ffi_channel_dispatches,
                rust_handler_errors: stats.rust_handler_errors,
                ffi_channel_drops: stats.ffi_channel_drops,
                rust_subscriber_patterns: stats.rust_subscriber_patterns,
                rust_handler_count: stats.rust_handler_count,
                ffi_subscriber_count: stats.ffi_subscriber_count,
            }
        };

        DomainEventStats {
            router: router_stats,
            in_process_bus: bus_stats,
            captured_at: chrono::Utc::now(),
            worker_id: format!("worker-{}", self.core_id),
        }
    }

    // =========================================================================
    // TAS-65 Phase 3: Step Event Publisher Methods
    // =========================================================================

    /// Register a custom step event publisher
    ///
    /// The publisher's `name()` is used as the lookup key. This name should
    /// match the `publisher:` field in YAML step definitions.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// worker_core.register_step_event_publisher(PaymentEventPublisher::new());
    /// ```
    pub fn register_step_event_publisher<P: StepEventPublisher + 'static>(&self, publisher: P) {
        let name = publisher.name().to_string();
        info!(publisher_name = %name, "Registering custom step event publisher");
        if let Ok(mut registry) = self.step_event_publisher_registry.write() {
            registry.register(publisher);
        } else {
            warn!(publisher_name = %name, "Failed to acquire write lock for publisher registration");
        }
    }

    /// Get a step event publisher by name
    ///
    /// Returns the publisher if registered, or None if not found.
    pub fn get_step_event_publisher(&self, name: &str) -> Option<Arc<dyn StepEventPublisher>> {
        self.step_event_publisher_registry
            .read()
            .ok()
            .and_then(|r| r.get(name))
    }

    /// Get the appropriate publisher for a step
    ///
    /// Looks up the publisher by name from YAML config. Returns the generic
    /// publisher if no custom publisher is specified or found.
    pub fn get_step_publisher_or_default(
        &self,
        publisher_name: Option<&str>,
    ) -> Arc<dyn StepEventPublisher> {
        self.step_event_publisher_registry
            .read()
            .map(|r| r.get_or_default(publisher_name))
            .unwrap_or_else(|_| {
                // Fallback: create a new generic publisher (should rarely happen)
                Arc::new(
                    super::step_event_publisher::DefaultDomainEventPublisher::new(
                        self.domain_event_publisher.clone(),
                    ),
                )
            })
    }

    /// Publish domain events for a completed step
    ///
    /// This is the main entry point for post-execution event publishing.
    /// Creates a lightweight context DTO and invokes the appropriate publisher.
    ///
    /// # Arguments
    ///
    /// * `task_sequence_step` - The completed step with full context
    /// * `execution_result` - The step execution result
    /// * `publisher_name` - Optional custom publisher name from YAML
    ///
    /// # Returns
    ///
    /// Result containing published event IDs, skipped events, and any errors.
    /// Event publishing failures are logged but do NOT fail the step.
    ///
    /// # Performance
    ///
    /// The context is a pure DTO (no Arc cloning). Publishers own their
    /// `Arc<DomainEventPublisher>` so no per-invocation Arc operations occur.
    pub async fn publish_step_events(
        &self,
        task_sequence_step: TaskSequenceStep,
        execution_result: StepExecutionResult,
        publisher_name: Option<&str>,
    ) -> PublishResult {
        let publisher = self.get_step_publisher_or_default(publisher_name);

        // Create lightweight DTO context (no Arc, no behavior)
        let ctx = StepEventContext::new(task_sequence_step, execution_result);

        let result = publisher.publish(&ctx).await;

        // Log results
        if !result.published.is_empty() {
            info!(
                step_name = %ctx.step_name(),
                publisher = %publisher.name(),
                published_count = result.published_count(),
                "Step events published successfully"
            );
        }

        if !result.errors.is_empty() {
            warn!(
                step_name = %ctx.step_name(),
                publisher = %publisher.name(),
                error_count = result.error_count(),
                errors = ?result.errors,
                "Some step events failed to publish"
            );
        }

        result
    }

    /// Get the count of registered custom publishers
    pub fn registered_publisher_count(&self) -> usize {
        self.step_event_publisher_registry
            .read()
            .map(|r| r.len())
            .unwrap_or(0)
    }

    /// Get names of all registered custom publishers
    pub fn registered_publisher_names(&self) -> Vec<String> {
        self.step_event_publisher_registry
            .read()
            .map(|r| r.registered_names().iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    /// Get a reference to the domain event publisher
    ///
    /// Useful for advanced scenarios where direct access is needed.
    pub fn domain_event_publisher(&self) -> Arc<DomainEventPublisher> {
        self.domain_event_publisher.clone()
    }
}

impl std::fmt::Display for WorkerCoreStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerCoreStatus::Created => write!(f, "Created"),
            WorkerCoreStatus::Starting => write!(f, "Starting"),
            WorkerCoreStatus::Running => write!(f, "Running"),
            WorkerCoreStatus::Stopping => write!(f, "Stopping"),
            WorkerCoreStatus::Stopped => write!(f, "Stopped"),
            WorkerCoreStatus::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

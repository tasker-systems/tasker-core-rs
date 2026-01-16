//! # Worker Core System
//!
//! Command pattern worker implementation that mirrors OrchestrationCore architecture

pub mod actor_command_processor; // TAS-69: Actor-based command processor (pure routing)
pub mod channels; // TAS-133: Semantic NewType channel wrappers
pub mod actors; // TAS-69: Actor-based architecture for worker system
pub mod command_processor;
pub mod core;
pub mod domain_event_commands; // TAS-65/TAS-69: Domain event command types
pub mod event_driven_processor;
pub mod event_publisher;
pub mod event_router; // TAS-65 Dual-Path: Routes events by delivery mode
pub mod event_subscriber;
pub mod event_systems;
pub mod handlers; // TAS-67: Language-agnostic handler traits and dispatch
pub mod hydration; // TAS-69: Message hydration layer
pub mod in_process_event_bus; // TAS-65 Dual-Path: Fast in-memory event delivery
pub mod orchestration_result_sender;
pub mod services; // TAS-69: Decomposed service layer
pub mod step_claim;
pub mod step_event_publisher; // TAS-65 Phase 3: Custom event publisher trait
pub mod step_event_publisher_registry; // TAS-65 Phase 3: Publisher registry
pub mod task_template_manager;
pub mod traits; // TAS-65: Worker traits including DomainEventPublishable
pub mod worker_queues;

pub use core::{DispatchHandles, WorkerCore, WorkerCoreStatus};

// TAS-133: Semantic channel wrappers for type-safe channel usage
pub use channels::{
    ChannelFactory, WorkerCommandReceiver, WorkerCommandSender, WorkerNotificationReceiver,
    WorkerNotificationSender,
};

pub use command_processor::{
    EventIntegrationStatus, StepExecutionStats, WorkerCommand, WorkerStatus,
};
pub use event_driven_processor::{EventDrivenConfig, EventDrivenStats};
pub use event_publisher::{WorkerEventError, WorkerEventPublisher, WorkerEventPublisherStats};
pub use event_router::{EventRouteOutcome, EventRouter, EventRouterBuilder, EventRouterError};
pub use event_subscriber::{
    CorrelatedCompletionListener, CorrelatedStepResult, WorkerEventSubscriber,
    WorkerEventSubscriberError, WorkerEventSubscriberStats,
};
pub use in_process_event_bus::{
    InProcessEventBus, InProcessEventBusBuilder, InProcessEventBusConfig,
}; // TAS-65 Dual-Path
pub use step_event_publisher::{
    DefaultDomainEventPublisher, PublishResult, StepEventContext, StepEventPublisher,
    StepEventPublisherError,
}; // TAS-65 Phase 3
pub use step_event_publisher_registry::{
    PublisherNotFoundError, StepEventPublisherRegistry, StepEventPublisherRegistryBuilder,
    ValidationErrors,
}; // TAS-65 Phase 3 + Phase 4 validation
pub use task_template_manager::{
    CachedTemplate, TaskTemplateManagerConfig, WorkerTaskTemplateOperations,
};
pub use traits::DomainEventPublishable; // TAS-65 // TAS-65 Dual-Path
                                        // Re-export stats types from canonical location in tasker-shared
pub use domain_event_commands::{
    DomainEventCommand, DomainEventSystemStats, DomainEventToPublish, ShutdownResult,
}; // TAS-65/TAS-69
pub use event_systems::domain_event_system::{
    DomainEventSystem, DomainEventSystemConfig, DomainEventSystemHandle,
};
pub use tasker_shared::metrics::worker::{EventRouterStats, InProcessEventBusStats}; // TAS-65/TAS-69

// TAS-69: Actor-based worker architecture
pub use actor_command_processor::ActorCommandProcessor;
pub use actors::{Handler, Message, WorkerActor, WorkerActorRegistry};
pub use hydration::StepMessageHydrator;

// TAS-67: Handler dispatch abstractions
// TAS-75: Added CapacityChecker and LoadSheddingConfig for worker load shedding
pub use handlers::{
    create_dispatch_channels, CapacityChecker, CompletionProcessorConfig,
    CompletionProcessorService, DomainEventCallback, FfiDispatchChannel, FfiDispatchChannelConfig,
    FfiDispatchMetrics, FfiStepEvent, HandlerDispatchConfig, HandlerDispatchService,
    LoadSheddingConfig, NoOpCallback, PostHandlerCallback, StepHandler, StepHandlerRegistry,
};

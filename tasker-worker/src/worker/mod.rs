//! # Worker Core System
//!
//! Command pattern worker implementation that mirrors OrchestrationCore architecture

pub mod command_processor;
pub mod core;
pub mod event_consumer; // TAS-65 Phase 2.3b: Event consumer service
pub mod event_driven_processor;
pub mod event_publisher;
pub mod event_subscriber;
pub mod event_systems;
pub mod orchestration_result_sender;
pub mod step_claim;
pub mod task_template_manager;
pub mod traits; // TAS-65: Worker traits including DomainEventPublishable
pub mod worker_queues;

pub use core::{WorkerCore, WorkerCoreStatus};

pub use command_processor::{
    EventIntegrationStatus, StepExecutionStats, WorkerCommand, WorkerStatus,
};
pub use event_consumer::{EventConsumer, EventConsumerConfig, EventConsumerStats}; // TAS-65 Phase 2.3b
pub use event_driven_processor::{EventDrivenConfig, EventDrivenStats};
pub use event_publisher::{WorkerEventError, WorkerEventPublisher, WorkerEventPublisherStats};
pub use event_subscriber::{
    CorrelatedCompletionListener, CorrelatedStepResult, WorkerEventSubscriber,
    WorkerEventSubscriberError, WorkerEventSubscriberStats,
};
pub use task_template_manager::{
    CachedTemplate, TaskTemplateManagerConfig, WorkerTaskTemplateOperations,
};
pub use traits::DomainEventPublishable; // TAS-65

pub mod domain_events;
pub mod publisher;
pub mod types;
pub mod validation; // TAS-65 Phase 2.2: Event schema validation
pub mod worker_events;

// Re-export key types for convenience
pub use publisher::{EventPublisher, EventPublisherConfig, EventPublisherStats, PublishError};
pub use types::{
    constants, Event, GenericEvent, OrchestrationEvent, StepResult, TaskResult, ViableStep,
};

// Re-export TAS-40 worker event types
pub use worker_events::{
    WorkerEventError, WorkerEventPublisher, WorkerEventSubscriber, WorkerEventSystem,
};

// Re-export TAS-65 Phase 2.1 domain event types
pub use domain_events::{DomainEvent, DomainEventError, DomainEventPublisher, EventMetadata};

// Re-export TAS-65 Phase 2.2 validation types
pub use validation::{EventSchemaValidator, EventValidationError};

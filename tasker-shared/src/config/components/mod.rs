// TAS-50: Reusable configuration components
//
// This module contains individual configuration components that are
// composed into the context-specific configurations.
//
// Phase 1 implementation: Non-breaking addition of component structs

pub mod backoff;
pub mod circuit_breakers;
pub mod database;
pub mod dlq;
pub mod event_systems;
pub mod mpsc_channels;
pub mod orchestration_system;
pub mod queues;
pub mod web;
pub mod worker_system;

// Re-export component types
pub use backoff::BackoffConfig;
pub use circuit_breakers::CircuitBreakersConfig;
pub use database::{DatabaseConfig, DatabasePoolConfig};
pub use dlq::{
    DlqConfig, DlqReasons, StalenessActions, StalenessDetectionConfig, StalenessThresholds,
};
// TAS-61 Phase 6C/6D: EventSystemsConfig removed - use V2 configs directly
pub use event_systems::{
    OrchestrationEventSystemConfig, TaskReadinessEventSystemConfig, WorkerEventSystemConfig,
};
pub use mpsc_channels::{
    MpscChannelsConfig, OrchestrationChannelsConfig, SharedChannelsConfig, WorkerChannelsConfig,
};
pub use orchestration_system::OrchestrationSystemConfig;
pub use queues::{PgmqConfig, QueuesConfig};
pub use web::WebConfig;
pub use worker_system::WorkerSystemConfig;

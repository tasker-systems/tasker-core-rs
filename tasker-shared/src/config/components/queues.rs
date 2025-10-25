// TAS-50: Queues configuration component
//
// Phase 1: Re-export existing types for backward compatibility

pub use crate::config::queues::{
    OrchestrationQueuesConfig, PgmqBackendConfig, QueuesConfig, RabbitMqBackendConfig,
};
pub use crate::config::tasker::QueuesConfig as PgmqConfig;

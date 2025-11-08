// TAS-50: Queues configuration component
//
// TAS-61 Phase 6C/6D: Updated to use V2 configuration

pub use crate::config::queues::{
    OrchestrationQueuesConfig, PgmqBackendConfig, QueuesConfig, RabbitMqBackendConfig,
};
pub use crate::config::tasker::tasker_v2::QueuesConfig as PgmqConfig;

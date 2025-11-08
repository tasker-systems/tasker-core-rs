//! # TaskerCore Configuration System (TAS-61 Phase 6C/6D V2 Canonical)
//!
//! Dead-simple configuration loading:
//! 1. Read pre-merged TOML from TASKER_CONFIG_PATH
//! 2. Deserialize to TaskerConfigV2
//! 3. Validate
//! 4. Return V2 directly (no bridge conversion - V2 is canonical)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::config::ConfigLoader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // TAS-61 Phase 6C/6D: ConfigLoader returns V2 configuration directly
//! // Load configuration from TASKER_CONFIG_PATH
//! let config = ConfigLoader::load_from_env()?;
//!
//! // Access configuration values via common config
//! let database_url = &config.common.database.url;
//! # Ok(())
//! # }
//! ```

pub mod circuit_breaker;
pub mod config_loader;
pub mod documentation;
pub mod error;
pub mod event_systems;
pub mod executor;
pub mod merge;
pub mod merger;
pub mod mpsc_channels;
pub mod orchestration;
pub mod queues;
pub mod tasker;
pub mod web;
pub mod worker;

// Primary exports - TAS-61 Simple V2 Configuration System
pub use config_loader::{ConfigLoader, ConfigManager};

// TAS-50: CLI configuration merger and documentation
pub use documentation::{ConfigDocumentation, EnvironmentRecommendation, ParameterDocumentation};
pub use merger::ConfigMerger;

// Re-export types and errors
pub use circuit_breaker::{
    CircuitBreakerComponentConfig, CircuitBreakerConfig, CircuitBreakerGlobalSettings,
};
pub use error::{ConfigResult, ConfigurationError};
pub use event_systems::{
    EventSystemConfig, EventSystemHealthConfig, EventSystemProcessingConfig,
    EventSystemTimingConfig,
    OrchestrationEventSystemConfig as UnifiedOrchestrationEventSystemConfig,
    TaskReadinessEventSystemConfig as UnifiedTaskReadinessEventSystemConfig,
    WorkerEventSystemConfig as UnifiedWorkerEventSystemConfig,
};
pub use orchestration::{
    event_systems::OrchestrationEventSystemConfig, ExecutorConfig,
    ExecutorType, OrchestrationConfig, OrchestrationSystemConfig,
};
// TAS-61 Phase 6C/6D: DecisionPointsConfig now in V2
pub use tasker::tasker_v2::DecisionPointsConfig;
pub use queues::{
    OrchestrationQueuesConfig, PgmqBackendConfig, QueuesConfig, RabbitMqBackendConfig,
};

pub mod queue_classification;
pub use queue_classification::{ConfigDrivenMessageEvent, QueueClassifier, QueueType};
pub use worker::{
    EventSystemConfig as WorkerLegacyEventSystemConfig, HealthMonitoringConfig,
    StepProcessingConfig, WorkerConfig,
};

pub use web::*;

// TAS-43 Task Readiness System exports

// TAS-51 MPSC Channels Configuration exports
pub use mpsc_channels::{
    DropPolicy, MpscChannelsConfig, OrchestrationChannelsConfig,
    OrchestrationCommandProcessorConfig, OrchestrationEventListenersConfig,
    OrchestrationEventSystemsConfig, OverflowMetricsConfig, OverflowPolicyConfig,
    SharedChannelsConfig, SharedEventPublisherConfig, SharedFfiConfig, TaskReadinessChannelsConfig,
    TaskReadinessEventChannelConfig, WorkerChannelsConfig, WorkerCommandProcessorConfig,
    WorkerEventListenersConfig, WorkerEventSubscribersConfig, WorkerEventSystemsConfig,
    WorkerInProcessEventsConfig,
};

// TAS-61 Phase 6C/6D: V2 is the canonical configuration
// All legacy TaskerConfig references removed - use TaskerConfigV2
pub use tasker::TaskerConfigV2;

// TAS-50: Context-specific configuration system (Phase 1)
// Component-based configuration for V2 architecture
pub mod components;
// TAS-61 Phase 6C/6D: contexts/ layer deleted - use V2 config directly

//! # TaskerCore Configuration System (TAS-61 V2 Simple Loader)
//!
//! Dead-simple configuration loading:
//! 1. Read pre-merged TOML from TASKER_CONFIG_PATH
//! 2. Deserialize to TaskerConfigV2
//! 3. Validate
//! 4. Convert to legacy TaskerConfig via bridge
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::config::ConfigLoader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration from TASKER_CONFIG_PATH
//! let config = ConfigLoader::load_from_env()?;
//!
//! // Access configuration values
//! let database_url = config.database_url();
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
    event_systems::OrchestrationEventSystemConfig, DecisionPointsConfig, ExecutorConfig,
    ExecutorType, OrchestrationConfig, OrchestrationSystemConfig,
};
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

// ConfigManager now exported from config_loader above
pub use tasker::{
    BackoffConfig, DatabaseConfig, DatabasePoolConfig, EngineConfig, EventSystemsConfig,
    ExecutionConfig, ReenqueueDelays, SystemConfig, TaskTemplatesConfig, TaskerConfig,
    TelemetryConfig,
};

// TAS-50: Context-specific configuration system (Phase 1)
// Non-breaking addition of context-specific configuration structs
pub mod components;
pub mod contexts;

// Phase 1: New modules for context-specific configuration
// These are additive and maintain 100% backward compatibility
pub use contexts::{
    CommonConfig, ConfigContext, ConfigurationContext,
    OrchestrationConfig as ContextOrchestrationConfig, WorkerConfig as ContextWorkerConfig,
};

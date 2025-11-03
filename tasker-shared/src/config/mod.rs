//! # TaskerCore Configuration System (TAS-34 Unified TOML)
//!
//! This module provides unified TOML-based configuration management with strict validation
//! and fail-fast behavior. All configuration loading is handled by UnifiedConfigLoader.
//!
//! ## Architecture
//!
//! - **Single Source of Truth**: UnifiedConfigLoader handles all configuration loading
//! - **TOML Only**: Component-based TOML configuration with environment overrides
//! - **Fail-Fast Validation**: No silent fallbacks or defaults
//! - **Strict Type Safety**: ValidatedConfig provides type-safe access to all components
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::config::UnifiedConfigLoader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration with environment detection
//! let mut loader = UnifiedConfigLoader::new_from_env()?;
//! let config = loader.load_tasker_config()?;
//!
//! // Access configuration values
//! let database_url = config.database_url();
//! let pool_size = config.database.pool;
//! let timeout = config.execution.step_execution_timeout_seconds;
//! # Ok(())
//! # }
//! ```

pub mod circuit_breaker;
pub mod documentation;
pub mod error;
pub mod event_systems;
pub mod executor;
pub mod manager;
pub mod merger;
pub mod mpsc_channels;
pub mod orchestration;
pub mod queues;
pub mod tasker;
pub mod unified_loader;
pub mod web;
pub mod worker;

// Primary exports - TAS-34 Unified Configuration System
pub use unified_loader::{UnifiedConfigLoader, ValidatedConfig};

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

// Compatibility wrapper (thin wrapper around UnifiedConfigLoader)
pub use manager::{ConfigManager, ContextConfigManager};
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

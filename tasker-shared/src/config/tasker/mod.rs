// Primary context-based configuration system (TAS-61 Phase 5.5)
pub mod tasker_v2;

// Bridge for context-based → legacy conversion (TAS-61 Phase 2)
pub mod bridge;

// Legacy monolithic config (to be removed in Phase 6-8 of TAS-61)
pub mod tasker;

// Re-export all legacy config types for backward compatibility during Phase 6-8
// These will be fully removed once all consumers migrate to context-based configuration
pub use tasker::{
    // Core structs defined in tasker.rs
    AlertThresholds,
    BackoffConfig,
    // Re-exported types from other modules
    CircuitBreakerComponentConfig,
    CircuitBreakerConfig,
    CircuitBreakerGlobalSettings,
    ConfigDrivenMessageEvent,
    ConfigResult,
    ConfigurationError,
    DatabaseConfig,
    DatabasePoolConfig,
    DatabaseVariables,
    DecisionPointsConfig,
    EngineConfig,
    EventSystemConfig,
    EventSystemHealthConfig,
    EventSystemProcessingConfig,
    EventSystemTimingConfig,
    EventSystemsConfig,
    ExecutionConfig,
    ExecutorConfig,
    ExecutorType,
    HealthConfig,
    HealthMonitoringConfig,
    OrchestrationConfig,
    OrchestrationEventSystemConfig,
    OrchestrationQueuesConfig,
    OrchestrationSystemConfig,
    PgmqBackendConfig,
    QueueClassifier,
    QueueType,
    QueuesConfig,
    RabbitMqBackendConfig,
    ReenqueueDelays,
    StepProcessingConfig,
    SystemConfig,
    TaskTemplatesConfig,
    TaskerConfig,
    TelemetryConfig,
    WorkerConfig,
};

// Primary context-based configuration (use this for all new code)
pub use tasker_v2::TaskerConfig as TaskerConfigV2;

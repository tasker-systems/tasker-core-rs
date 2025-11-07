// Legacy config (will be deprecated in Phase 6-8 of TAS-61)
pub mod tasker;

// V2 config (new baseline - TAS-61)
pub mod tasker_v2;

// Bridge for v2 → legacy conversion (TAS-61 Phase 2)
pub mod bridge;

// Re-export all legacy config types for backward compatibility
// This maintains backward compatibility during TAS-61 migration
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

// Re-export v2 config with alias for clarity
pub use tasker_v2::TaskerConfig as TaskerConfigV2;

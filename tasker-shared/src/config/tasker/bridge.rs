//! Bridge Module for TaskerConfigV2 → Legacy TaskerConfig Conversion (TAS-61)
//!
//! This module provides temporary conversion during the TAS-61 migration from legacy
//! flat configuration to v2 context-based configuration. Once all systems are migrated
//! to use TaskerConfigV2 directly, this bridge will be removed.
//!
//! ## Conversion Strategy
//!
//! The bridge implements `From<TaskerConfigV2> for TaskerConfig` to allow seamless
//! conversion from v2's context-based structure to the legacy flat structure.
//!
//! ### Field Mapping
//!
//! **Common Fields** (always present):
//! - `common.system` → `system`
//! - `common.database` → `database`
//! - `common.queues` → `queues`
//! - `common.circuit_breakers` → `circuit_breakers`
//! - `common.execution` → `execution`
//! - `common.backoff` → `backoff`
//! - `common.task_templates` → `task_templates`
//! - `common.telemetry` → `telemetry` (with default if None)
//!
//! **Orchestration Fields** (if present):
//! - `orchestration.*` → `orchestration`
//! - `orchestration.event_systems.*` → `event_systems.orchestration`
//! - `orchestration.decision_points` → `decision_points`
//!
//! **Worker Fields** (if present):
//! - `worker.*` → `worker`
//! - `worker.event_systems.*` → `event_systems.worker`
//!
//! **MPSC Channels** (merged from all contexts):
//! - `common.mpsc_channels.*` + `orchestration.mpsc_channels.*` + `worker.mpsc_channels.*` → `mpsc_channels`
//!
//! ## Example
//!
//! ```rust,ignore
//! use tasker_shared::config::{UnifiedConfigLoader, TaskerConfig};
//!
//! let mut loader = UnifiedConfigLoader::new("test")?;
//! let config_v2 = loader.load_tasker_config_v2()?;
//!
//! // Bridge conversion
//! let legacy_config: TaskerConfig = config_v2.into();
//!
//! // Now use with existing systems
//! let orchestration_core = OrchestrationCore::new(legacy_config)?;
//! ```

use super::tasker::{
    BackoffConfig, CircuitBreakerConfig, DatabaseConfig, DatabasePoolConfig, DatabaseVariables,
    DecisionPointsConfig, EventSystemsConfig, ExecutionConfig, OrchestrationConfig, QueuesConfig,
    ReenqueueDelays, SystemConfig, TaskTemplatesConfig, TaskerConfig, TelemetryConfig,
    WorkerConfig,
};
use super::tasker_v2::{
    CircuitBreakerComponentConfig as V2CircuitBreakerComponentConfig, CircuitBreakerDefaultConfig,
    CommonConfig, ComponentCircuitBreakerConfigs,
    DatabaseVariablesConfig as V2DatabaseVariablesConfig,
    GlobalCircuitBreakerSettings as V2GlobalCircuitBreakerSettings,
    OrchestrationConfig as V2OrchestrationConfig,
    OrchestrationQueuesConfig as V2OrchestrationQueuesConfig, OrchestrationWebConfig,
    PgmqConfig as V2PgmqConfig, PoolConfig as V2PoolConfig, RabbitmqConfig as V2RabbitmqConfig,
    ReenqueueDelaysConfig as V2ReenqueueDelaysConfig, TaskerConfig as TaskerConfigV2,
    WorkerConfig as V2WorkerConfig, WorkerWebConfig,
};
use crate::config::circuit_breaker::{CircuitBreakerComponentConfig, CircuitBreakerGlobalSettings};
use crate::config::mpsc_channels::MpscChannelsConfig;
use crate::config::queues::{OrchestrationQueuesConfig, PgmqBackendConfig, RabbitMqBackendConfig};
use crate::config::web::WebConfig;
use std::collections::HashMap;

// ============================================================================
// From Trait Implementations for Type Conversion
// ============================================================================

impl From<CircuitBreakerDefaultConfig> for CircuitBreakerComponentConfig {
    fn from(v2: CircuitBreakerDefaultConfig) -> Self {
        Self {
            failure_threshold: v2.failure_threshold,
            timeout_seconds: v2.timeout_seconds as u64,
            success_threshold: v2.success_threshold,
        }
    }
}

impl From<V2CircuitBreakerComponentConfig> for CircuitBreakerComponentConfig {
    fn from(v2: V2CircuitBreakerComponentConfig) -> Self {
        Self {
            failure_threshold: v2.failure_threshold,
            timeout_seconds: v2.timeout_seconds as u64,
            success_threshold: v2.success_threshold,
        }
    }
}

impl From<ComponentCircuitBreakerConfigs> for HashMap<String, CircuitBreakerComponentConfig> {
    fn from(v2: ComponentCircuitBreakerConfigs) -> Self {
        let mut map = HashMap::new();
        map.insert("task_readiness".to_string(), v2.task_readiness.into());
        map.insert("pgmq".to_string(), v2.pgmq.into());
        map
    }
}

impl From<V2GlobalCircuitBreakerSettings> for CircuitBreakerGlobalSettings {
    fn from(v2: V2GlobalCircuitBreakerSettings) -> Self {
        Self {
            max_circuit_breakers: v2.max_circuit_breakers as usize,
            metrics_collection_interval_seconds: v2.metrics_collection_interval_seconds as u64,
            min_state_transition_interval_seconds: v2.min_state_transition_interval_seconds,
        }
    }
}

impl From<V2PoolConfig> for DatabasePoolConfig {
    fn from(v2: V2PoolConfig) -> Self {
        Self {
            max_connections: v2.max_connections,
            min_connections: v2.min_connections,
            acquire_timeout_seconds: v2.acquire_timeout_seconds as u64,
            idle_timeout_seconds: v2.idle_timeout_seconds as u64,
            max_lifetime_seconds: v2.max_lifetime_seconds as u64,
        }
    }
}

impl From<V2DatabaseVariablesConfig> for DatabaseVariables {
    fn from(v2: V2DatabaseVariablesConfig) -> Self {
        Self {
            statement_timeout: v2.statement_timeout as u64,
        }
    }
}

impl From<V2ReenqueueDelaysConfig> for ReenqueueDelays {
    fn from(v2: V2ReenqueueDelaysConfig) -> Self {
        Self {
            initializing: v2.initializing as u64,
            enqueuing_steps: v2.enqueuing_steps as u64,
            steps_in_process: v2.steps_in_process as u64,
            evaluating_results: v2.evaluating_results as u64,
            waiting_for_dependencies: v2.waiting_for_dependencies as u64,
            waiting_for_retry: v2.waiting_for_retry as u64,
            blocked_by_failures: v2.blocked_by_failures as u64,
        }
    }
}

impl From<V2OrchestrationQueuesConfig> for OrchestrationQueuesConfig {
    fn from(v2: V2OrchestrationQueuesConfig) -> Self {
        Self {
            task_requests: v2.task_requests,
            task_finalizations: v2.task_finalizations,
            step_results: v2.step_results,
        }
    }
}

impl From<V2PgmqConfig> for PgmqBackendConfig {
    fn from(v2: V2PgmqConfig) -> Self {
        Self {
            poll_interval_ms: v2.poll_interval_ms as u64,
            shutdown_timeout_seconds: v2.shutdown_timeout_seconds as u64,
            max_retries: v2.max_retries,
        }
    }
}

impl From<V2RabbitmqConfig> for RabbitMqBackendConfig {
    fn from(v2: V2RabbitmqConfig) -> Self {
        Self {
            connection_timeout_seconds: v2.connection_timeout_seconds as u64,
        }
    }
}

impl From<OrchestrationWebConfig> for WebConfig {
    fn from(v2: OrchestrationWebConfig) -> Self {
        use crate::config::web::*;

        WebConfig {
            enabled: v2.enabled,
            bind_address: v2.bind_address,
            request_timeout_ms: v2.request_timeout_ms as u64,
            tls: v2
                .tls
                .map(|t| WebTlsConfig {
                    enabled: t.enabled,
                    cert_path: t.cert_path,
                    key_path: t.key_path,
                })
                .unwrap_or_default(),
            database_pools: WebDatabasePoolsConfig {
                web_api_pool_size: v2.database_pools.web_api_pool_size,
                web_api_max_connections: v2.database_pools.web_api_max_connections,
                web_api_connection_timeout_seconds: v2
                    .database_pools
                    .web_api_connection_timeout_seconds
                    as u64,
                web_api_idle_timeout_seconds: v2.database_pools.web_api_idle_timeout_seconds as u64,
                max_total_connections_hint: v2.database_pools.max_total_connections_hint,
            },
            cors: v2
                .cors
                .map(|c| WebCorsConfig {
                    enabled: c.enabled,
                    allowed_origins: c.allowed_origins,
                    allowed_methods: c.allowed_methods,
                    allowed_headers: c.allowed_headers,
                    max_age_seconds: c.max_age_seconds as u64,
                })
                .unwrap_or_else(|| WebCorsConfig {
                    enabled: false,
                    allowed_origins: vec![],
                    allowed_methods: vec![],
                    allowed_headers: vec![],
                    max_age_seconds: 86400,
                }),
            auth: v2
                .auth
                .map(|a| {
                    // Convert v2 routes_map() HashMap<String, RouteAuthConfig> to legacy format
                    let protected_routes = a
                        .routes_map()
                        .into_iter()
                        .map(|(k, v)| {
                            (
                                k,
                                RouteAuthConfig {
                                    auth_type: v.auth_type,
                                    required: v.required,
                                },
                            )
                        })
                        .collect();

                    WebAuthConfig {
                        enabled: a.enabled,
                        jwt_issuer: a.jwt_issuer,
                        jwt_audience: a.jwt_audience,
                        jwt_token_expiry_hours: a.jwt_token_expiry_hours as u64,
                        jwt_private_key: a.jwt_private_key,
                        jwt_public_key: a.jwt_public_key,
                        api_key: a.api_key,
                        api_key_header: a.api_key_header,
                        protected_routes,
                    }
                })
                .unwrap_or_default(),
            rate_limiting: v2
                .rate_limiting
                .map(|r| WebRateLimitConfig {
                    enabled: r.enabled,
                    requests_per_minute: r.requests_per_minute,
                    burst_size: r.burst_size,
                    per_client_limit: r.per_client_limit,
                })
                .unwrap_or_default(),
            resilience: v2
                .resilience
                .map(|r| WebResilienceConfig {
                    circuit_breaker_enabled: r.circuit_breaker_enabled,
                    request_timeout_seconds: r.request_timeout_seconds as u64,
                    max_concurrent_requests: r.max_concurrent_requests,
                })
                .unwrap_or_default(),
        }
    }
}

impl From<WorkerWebConfig> for WebConfig {
    fn from(v2: WorkerWebConfig) -> Self {
        use crate::config::web::*;

        WebConfig {
            enabled: v2.enabled,
            bind_address: v2.bind_address,
            request_timeout_ms: v2.request_timeout_ms as u64,
            tls: v2
                .tls
                .map(|t| WebTlsConfig {
                    enabled: t.enabled,
                    cert_path: t.cert_path,
                    key_path: t.key_path,
                })
                .unwrap_or_default(),
            database_pools: WebDatabasePoolsConfig {
                web_api_pool_size: v2.database_pools.web_api_pool_size,
                web_api_max_connections: v2.database_pools.web_api_max_connections,
                web_api_connection_timeout_seconds: v2
                    .database_pools
                    .web_api_connection_timeout_seconds
                    as u64,
                web_api_idle_timeout_seconds: v2.database_pools.web_api_idle_timeout_seconds as u64,
                max_total_connections_hint: v2.database_pools.max_total_connections_hint,
            },
            cors: v2
                .cors
                .map(|c| WebCorsConfig {
                    enabled: c.enabled,
                    allowed_origins: c.allowed_origins,
                    allowed_methods: c.allowed_methods,
                    allowed_headers: c.allowed_headers,
                    max_age_seconds: c.max_age_seconds as u64,
                })
                .unwrap_or_else(|| WebCorsConfig {
                    enabled: false,
                    allowed_origins: vec![],
                    allowed_methods: vec![],
                    allowed_headers: vec![],
                    max_age_seconds: 86400,
                }),
            auth: v2
                .auth
                .map(|a| {
                    // Convert v2 routes_map() HashMap<String, RouteAuthConfig> to legacy format
                    let protected_routes = a
                        .routes_map()
                        .into_iter()
                        .map(|(k, v)| {
                            (
                                k,
                                RouteAuthConfig {
                                    auth_type: v.auth_type,
                                    required: v.required,
                                },
                            )
                        })
                        .collect();

                    WebAuthConfig {
                        enabled: a.enabled,
                        jwt_issuer: a.jwt_issuer,
                        jwt_audience: a.jwt_audience,
                        jwt_token_expiry_hours: a.jwt_token_expiry_hours as u64,
                        jwt_private_key: a.jwt_private_key,
                        jwt_public_key: a.jwt_public_key,
                        api_key: a.api_key,
                        api_key_header: a.api_key_header,
                        protected_routes,
                    }
                })
                .unwrap_or_default(),
            rate_limiting: v2
                .rate_limiting
                .map(|r| WebRateLimitConfig {
                    enabled: r.enabled,
                    requests_per_minute: r.requests_per_minute,
                    burst_size: r.burst_size,
                    per_client_limit: r.per_client_limit,
                })
                .unwrap_or_default(),
            resilience: v2
                .resilience
                .map(|r| WebResilienceConfig {
                    circuit_breaker_enabled: r.circuit_breaker_enabled,
                    request_timeout_seconds: r.request_timeout_seconds as u64,
                    max_concurrent_requests: r.max_concurrent_requests,
                })
                .unwrap_or_default(),
        }
    }
}

// ============================================================================
// Main Conversion Implementation
// ============================================================================

impl From<TaskerConfigV2> for TaskerConfig {
    fn from(v2: TaskerConfigV2) -> Self {
        // Extract common configuration (always present)
        let common = v2.common;

        // Build event_systems from context-specific event systems
        let event_systems =
            build_event_systems_config(v2.orchestration.as_ref(), v2.worker.as_ref());

        // Build merged MPSC channels configuration
        let mpsc_channels =
            build_mpsc_channels_config(&common, v2.orchestration.as_ref(), v2.worker.as_ref());

        // Extract orchestration configuration or build default
        let orchestration = v2
            .orchestration
            .as_ref()
            .map(|orch| convert_orchestration_config(orch))
            .unwrap_or_default();

        // Extract decision points from orchestration or use default
        let decision_points = v2
            .orchestration
            .map(|orch| convert_decision_points_config(orch.decision_points))
            .unwrap_or_default();

        // Extract worker configuration (optional)
        let worker = v2.worker.map(|w| convert_worker_config(&w));

        // Build legacy config
        TaskerConfig {
            database: convert_database_config(common.database),
            telemetry: common
                .telemetry
                .map(|t| TelemetryConfig {
                    enabled: t.enabled,
                    service_name: t.service_name,
                    sample_rate: t.sample_rate,
                })
                .unwrap_or_default(),
            task_templates: convert_task_templates_config(common.task_templates),
            system: convert_system_config(common.system),
            backoff: convert_backoff_config(common.backoff),
            execution: convert_execution_config(common.execution),
            queues: convert_queues_config(common.queues),
            orchestration,
            circuit_breakers: convert_circuit_breaker_config(common.circuit_breakers),
            event_systems,
            mpsc_channels,
            decision_points,
            worker,
        }
    }
}

// ============================================================================
// Helper Functions for Type Conversion
// ============================================================================

fn build_event_systems_config(
    orchestration: Option<&V2OrchestrationConfig>,
    worker: Option<&V2WorkerConfig>,
) -> EventSystemsConfig {
    // Convert orchestration event system
    let orchestration_config = orchestration
        .map(|orch| convert_event_system_config(orch.event_systems.orchestration.clone()))
        .unwrap_or_default();

    // Convert task readiness event system
    let task_readiness_config = orchestration
        .map(|orch| convert_event_system_config(orch.event_systems.task_readiness.clone()))
        .unwrap_or_default();

    // Convert worker event system - need to convert v2 WorkerEventSystemConfig to legacy WorkerEventSystemConfig
    let worker_config = worker
        .map(|w| convert_worker_event_system_config(w.event_systems.worker.clone()))
        .unwrap_or_default();

    EventSystemsConfig {
        orchestration: orchestration_config,
        task_readiness: task_readiness_config,
        worker: worker_config,
    }
}

fn convert_event_system_config<T: Default>(
    v2: super::tasker_v2::EventSystemConfig,
) -> crate::config::event_systems::EventSystemConfig<T> {
    use crate::config::event_systems::{
        BackoffConfig, EventSystemHealthConfig, EventSystemProcessingConfig,
        EventSystemTimingConfig,
    };

    crate::config::event_systems::EventSystemConfig {
        system_id: v2.system_id,
        deployment_mode: convert_deployment_mode(v2.deployment_mode),
        timing: EventSystemTimingConfig {
            health_check_interval_seconds: v2.timing.health_check_interval_seconds as u64,
            fallback_polling_interval_seconds: v2.timing.fallback_polling_interval_seconds as u64,
            visibility_timeout_seconds: v2.timing.visibility_timeout_seconds as u64,
            processing_timeout_seconds: v2.timing.processing_timeout_seconds as u64,
            claim_timeout_seconds: v2.timing.claim_timeout_seconds as u64,
        },
        processing: EventSystemProcessingConfig {
            max_concurrent_operations: v2.processing.max_concurrent_operations as usize,
            batch_size: v2.processing.batch_size,
            max_retries: v2.processing.max_retries,
            backoff: BackoffConfig {
                initial_delay_ms: v2.processing.backoff.initial_delay_ms as u64,
                max_delay_ms: v2.processing.backoff.max_delay_ms as u64,
                multiplier: v2.processing.backoff.multiplier,
                jitter_percent: v2.processing.backoff.jitter_percent,
            },
        },
        health: EventSystemHealthConfig {
            enabled: v2.health.enabled,
            performance_monitoring_enabled: v2.health.performance_monitoring_enabled,
            max_consecutive_errors: v2.health.max_consecutive_errors,
            error_rate_threshold_per_minute: v2.health.error_rate_threshold_per_minute,
        },
        metadata: T::default(),
    }
}

fn convert_worker_event_system_config(
    v2: super::tasker_v2::WorkerEventSystemConfig,
) -> crate::config::event_systems::WorkerEventSystemConfig {
    use crate::config::event_systems::{
        BackoffConfig, EventSystemHealthConfig, EventSystemProcessingConfig,
        EventSystemTimingConfig, InProcessEventConfig, WorkerEventSystemMetadata,
        WorkerFallbackPollerConfig, WorkerListenerConfig, WorkerResourceLimits,
    };

    crate::config::event_systems::WorkerEventSystemConfig {
        system_id: v2.system_id,
        deployment_mode: convert_deployment_mode(v2.deployment_mode),
        timing: EventSystemTimingConfig {
            health_check_interval_seconds: v2.timing.health_check_interval_seconds as u64,
            fallback_polling_interval_seconds: v2.timing.fallback_polling_interval_seconds as u64,
            visibility_timeout_seconds: v2.timing.visibility_timeout_seconds as u64,
            processing_timeout_seconds: v2.timing.processing_timeout_seconds as u64,
            claim_timeout_seconds: v2.timing.claim_timeout_seconds as u64,
        },
        processing: EventSystemProcessingConfig {
            max_concurrent_operations: v2.processing.max_concurrent_operations as usize,
            batch_size: v2.processing.batch_size,
            max_retries: v2.processing.max_retries,
            backoff: BackoffConfig {
                initial_delay_ms: v2.processing.backoff.initial_delay_ms as u64,
                max_delay_ms: v2.processing.backoff.max_delay_ms as u64,
                multiplier: v2.processing.backoff.multiplier,
                jitter_percent: v2.processing.backoff.jitter_percent,
            },
        },
        health: EventSystemHealthConfig {
            enabled: v2.health.enabled,
            performance_monitoring_enabled: v2.health.performance_monitoring_enabled,
            max_consecutive_errors: v2.health.max_consecutive_errors,
            error_rate_threshold_per_minute: v2.health.error_rate_threshold_per_minute,
        },
        metadata: WorkerEventSystemMetadata {
            in_process_events: InProcessEventConfig {
                ffi_integration_enabled: v2.metadata.in_process_events.ffi_integration_enabled,
                deduplication_cache_size: v2.metadata.in_process_events.deduplication_cache_size
                    as usize,
            },
            listener: WorkerListenerConfig {
                retry_interval_seconds: v2.metadata.listener.retry_interval_seconds as u64,
                max_retry_attempts: v2.metadata.listener.max_retry_attempts,
                event_timeout_seconds: v2.metadata.listener.event_timeout_seconds as u64,
                batch_processing: v2.metadata.listener.batch_processing,
                connection_timeout_seconds: v2.metadata.listener.connection_timeout_seconds as u64,
            },
            fallback_poller: WorkerFallbackPollerConfig {
                enabled: v2.metadata.fallback_poller.enabled,
                polling_interval_ms: v2.metadata.fallback_poller.polling_interval_ms as u64,
                batch_size: v2.metadata.fallback_poller.batch_size,
                age_threshold_seconds: v2.metadata.fallback_poller.age_threshold_seconds as u64,
                max_age_hours: v2.metadata.fallback_poller.max_age_hours as u64,
                visibility_timeout_seconds: v2.metadata.fallback_poller.visibility_timeout_seconds
                    as u64,
                supported_namespaces: v2.metadata.fallback_poller.supported_namespaces,
            },
            resource_limits: WorkerResourceLimits {
                max_memory_mb: v2.metadata.resource_limits.max_memory_mb as u64,
                max_cpu_percent: v2.metadata.resource_limits.max_cpu_percent,
                max_database_connections: v2.metadata.resource_limits.max_database_connections,
                max_queue_connections: v2.metadata.resource_limits.max_queue_connections,
            },
        },
    }
}

fn convert_deployment_mode(
    v2: super::tasker_v2::DeploymentMode,
) -> crate::event_system::DeploymentMode {
    use super::tasker_v2::DeploymentMode as V2DeploymentMode;
    use crate::event_system::DeploymentMode as LegacyDeploymentMode;

    match v2 {
        V2DeploymentMode::EventDrivenOnly => LegacyDeploymentMode::EventDrivenOnly,
        V2DeploymentMode::PollingOnly => LegacyDeploymentMode::PollingOnly,
        V2DeploymentMode::Hybrid => LegacyDeploymentMode::Hybrid,
    }
}

fn convert_drop_policy(v2: &str) -> crate::config::mpsc_channels::DropPolicy {
    use crate::config::mpsc_channels::DropPolicy;

    match v2.to_lowercase().as_str() {
        "dropoldest" | "drop_oldest" => DropPolicy::DropOldest,
        "dropnewest" | "drop_newest" => DropPolicy::DropNewest,
        "block" => DropPolicy::Block,
        _ => DropPolicy::Block, // Default to Block for unknown values
    }
}

fn convert_decision_points_config(
    v2: super::tasker_v2::DecisionPointsConfig,
) -> DecisionPointsConfig {
    DecisionPointsConfig {
        enabled: v2.enabled,
        max_steps_per_decision: v2.max_steps_per_decision as usize,
        max_decision_depth: v2.max_decision_depth as usize,
        warn_threshold_steps: v2.warn_threshold_steps as usize,
        warn_threshold_depth: v2.warn_threshold_depth as usize,
        enable_detailed_logging: v2.enable_detailed_logging,
        enable_metrics: v2.enable_metrics,
    }
}

fn build_mpsc_channels_config(
    common: &CommonConfig,
    orchestration: Option<&V2OrchestrationConfig>,
    worker: Option<&V2WorkerConfig>,
) -> MpscChannelsConfig {
    use crate::config::mpsc_channels::*;

    // Start with shared channels from common
    let shared = &common.mpsc_channels;

    // Build orchestration channels if present
    let orchestration_channels = orchestration.map(|orch| OrchestrationChannelsConfig {
        command_processor: OrchestrationCommandProcessorConfig {
            command_buffer_size: orch.mpsc_channels.command_processor.command_buffer_size as usize,
        },
        event_systems: OrchestrationEventSystemsConfig {
            event_channel_buffer_size: orch.mpsc_channels.event_systems.event_channel_buffer_size
                as usize,
        },
        event_listeners: OrchestrationEventListenersConfig {
            pgmq_event_buffer_size: orch.mpsc_channels.event_listeners.pgmq_event_buffer_size
                as usize,
        },
    });

    // Build task readiness channels - use default if orchestration not present
    let task_readiness_channels = TaskReadinessChannelsConfig {
        event_channel: TaskReadinessEventChannelConfig {
            buffer_size: orchestration
                .map(|orch| orch.mpsc_channels.event_systems.event_channel_buffer_size as usize)
                .unwrap_or(5000),
            send_timeout_ms: 100,
        },
    };

    // Build worker channels if present
    let worker_channels = worker.map(|w| WorkerChannelsConfig {
        command_processor: WorkerCommandProcessorConfig {
            command_buffer_size: w.mpsc_channels.command_processor.command_buffer_size as usize,
        },
        event_systems: WorkerEventSystemsConfig {
            event_channel_buffer_size: w.mpsc_channels.event_systems.event_channel_buffer_size
                as usize,
        },
        event_subscribers: WorkerEventSubscribersConfig {
            completion_buffer_size: w.mpsc_channels.event_subscribers.completion_buffer_size
                as usize,
            result_buffer_size: w.mpsc_channels.event_subscribers.result_buffer_size as usize,
        },
        in_process_events: WorkerInProcessEventsConfig {
            broadcast_buffer_size: w.mpsc_channels.in_process_events.broadcast_buffer_size as usize,
        },
        event_listeners: WorkerEventListenersConfig {
            pgmq_event_buffer_size: w.mpsc_channels.event_listeners.pgmq_event_buffer_size as usize,
        },
    });

    MpscChannelsConfig {
        shared: SharedChannelsConfig {
            event_publisher: SharedEventPublisherConfig {
                event_queue_buffer_size: shared.event_publisher.event_queue_buffer_size as usize,
            },
            ffi: SharedFfiConfig {
                ruby_event_buffer_size: shared.ffi.ruby_event_buffer_size as usize,
            },
        },
        orchestration: orchestration_channels.unwrap_or_else(|| OrchestrationChannelsConfig {
            command_processor: OrchestrationCommandProcessorConfig {
                command_buffer_size: 5000,
            },
            event_systems: OrchestrationEventSystemsConfig {
                event_channel_buffer_size: 10000,
            },
            event_listeners: OrchestrationEventListenersConfig {
                pgmq_event_buffer_size: 50000,
            },
        }),
        task_readiness: task_readiness_channels,
        worker: worker_channels.unwrap_or_else(|| WorkerChannelsConfig {
            command_processor: WorkerCommandProcessorConfig {
                command_buffer_size: 2000,
            },
            event_systems: WorkerEventSystemsConfig {
                event_channel_buffer_size: 2000,
            },
            event_subscribers: WorkerEventSubscribersConfig {
                completion_buffer_size: 1000,
                result_buffer_size: 1000,
            },
            in_process_events: WorkerInProcessEventsConfig {
                broadcast_buffer_size: 2000,
            },
            event_listeners: WorkerEventListenersConfig {
                pgmq_event_buffer_size: 10000,
            },
        }),
        overflow_policy: OverflowPolicyConfig {
            log_warning_threshold: shared.overflow_policy.log_warning_threshold,
            drop_policy: convert_drop_policy(&shared.overflow_policy.drop_policy),
            metrics: OverflowMetricsConfig {
                enabled: shared.overflow_policy.metrics.enabled,
                saturation_check_interval_seconds: shared
                    .overflow_policy
                    .metrics
                    .saturation_check_interval_seconds
                    as u64,
            },
        },
    }
}

fn convert_orchestration_config(v2_orch: &V2OrchestrationConfig) -> OrchestrationConfig {
    OrchestrationConfig {
        mode: v2_orch.mode.clone(),
        enable_performance_logging: v2_orch.enable_performance_logging,
        web: v2_orch.web.clone().map(|w| w.into()).unwrap_or_default(),
    }
}

fn convert_worker_config(v2_worker: &V2WorkerConfig) -> WorkerConfig {
    use crate::config::worker::{HealthMonitoringConfig, StepProcessingConfig};

    WorkerConfig {
        worker_id: v2_worker.worker_id.clone(),
        worker_type: v2_worker.worker_type.clone(),
        step_processing: StepProcessingConfig {
            claim_timeout_seconds: v2_worker.step_processing.claim_timeout_seconds as u64,
            max_retries: v2_worker.step_processing.max_retries,
            max_concurrent_steps: v2_worker.step_processing.max_concurrent_steps as usize,
        },
        health_monitoring: HealthMonitoringConfig {
            health_check_interval_seconds: v2_worker.health_monitoring.health_check_interval_seconds
                as u64,
            performance_monitoring_enabled: v2_worker
                .health_monitoring
                .performance_monitoring_enabled,
            error_rate_threshold: v2_worker.health_monitoring.error_rate_threshold,
        },
        queues: QueuesConfig::default(), // Will be populated from main config
        web: v2_worker.web.clone().map(|w| w.into()).unwrap_or_default(),
    }
}

// Simple conversion functions for types that are identical or have trivial conversions

fn convert_database_config(v2_db: super::tasker_v2::DatabaseConfig) -> DatabaseConfig {
    DatabaseConfig {
        url: Some(v2_db.url),
        database: Some(v2_db.database),
        skip_migration_check: v2_db.skip_migration_check,
        pool: v2_db.pool.into(),
        variables: v2_db.variables.into(),
    }
}

fn convert_task_templates_config(
    v2_templates: super::tasker_v2::TaskTemplatesConfig,
) -> TaskTemplatesConfig {
    TaskTemplatesConfig {
        search_paths: v2_templates.search_paths,
    }
}

fn convert_system_config(v2_system: super::tasker_v2::SystemConfig) -> SystemConfig {
    SystemConfig {
        version: v2_system.version,
        default_dependent_system: v2_system.default_dependent_system,
        max_recursion_depth: v2_system.max_recursion_depth,
    }
}

fn convert_backoff_config(v2_backoff: super::tasker_v2::BackoffConfig) -> BackoffConfig {
    BackoffConfig {
        default_backoff_seconds: v2_backoff
            .default_backoff_seconds
            .into_iter()
            .map(|v| v as u64)
            .collect(),
        max_backoff_seconds: v2_backoff.max_backoff_seconds as u64,
        backoff_multiplier: v2_backoff.backoff_multiplier,
        jitter_enabled: v2_backoff.jitter_enabled,
        jitter_max_percentage: v2_backoff.jitter_max_percentage,
        reenqueue_delays: v2_backoff.reenqueue_delays.into(),
    }
}

fn convert_execution_config(v2_exec: super::tasker_v2::ExecutionConfig) -> ExecutionConfig {
    ExecutionConfig {
        max_concurrent_tasks: v2_exec.max_concurrent_tasks,
        max_concurrent_steps: v2_exec.max_concurrent_steps,
        default_timeout_seconds: v2_exec.default_timeout_seconds as u64,
        step_execution_timeout_seconds: v2_exec.step_execution_timeout_seconds as u64,
        max_discovery_attempts: v2_exec.max_discovery_attempts,
        step_batch_size: v2_exec.step_batch_size,
        max_retries: v2_exec.max_retries,
        max_workflow_steps: v2_exec.max_workflow_steps,
        connection_timeout_seconds: v2_exec.connection_timeout_seconds as u64,
        environment: v2_exec.environment,
    }
}

fn convert_queues_config(v2_queues: super::tasker_v2::QueuesConfig) -> QueuesConfig {
    QueuesConfig {
        backend: v2_queues.backend,
        orchestration_namespace: v2_queues.orchestration_namespace,
        worker_namespace: v2_queues.worker_namespace,
        default_visibility_timeout_seconds: v2_queues.default_visibility_timeout_seconds,
        default_batch_size: v2_queues.default_batch_size,
        max_batch_size: v2_queues.max_batch_size,
        naming_pattern: v2_queues.naming_pattern,
        health_check_interval: v2_queues.health_check_interval as u64,
        orchestration_queues: v2_queues.orchestration_queues.into(),
        pgmq: v2_queues.pgmq.into(),
        rabbitmq: v2_queues.rabbitmq.map(|r| r.into()),
    }
}

fn convert_circuit_breaker_config(
    v2_cb: super::tasker_v2::CircuitBreakerConfig,
) -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        enabled: v2_cb.enabled,
        global_settings: v2_cb.global_settings.into(),
        default_config: v2_cb.default_config.into(),
        component_configs: v2_cb.component_configs.into(),
    }
}

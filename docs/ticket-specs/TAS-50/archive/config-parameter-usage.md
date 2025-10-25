# Configuration Parameter Usage Analysis

**Analysis Date**:
**Purpose**: Comprehensive mapping of configuration parameter usage across the codebase

---

## Table of Contents

- [Common Configuration](#common-configuration)
- [Orchestration Configuration](#orchestration-configuration)
- [Worker Configuration](#worker-configuration)
- [Summary Statistics](#summary-statistics)

---


## Ucommon Configuration

| Parameter Path | Type | Usage Locations | Notes |
|----------------|------|-----------------|-------|
| `environment` | String | Not found | Potentially unused |
| `database.enable_secondary_database` | bool | Not found | Potentially unused |
| `database.url` | String | Not found | Potentially unused |
| `database.adapter` | String | Not found | Potentially unused |
| `database.encoding` | String | Not found | Potentially unused |
| `database.host` | String | Not found | Potentially unused |
| `database.username` | String | Not found | Potentially unused |
| `database.password` | String | Not found | Potentially unused |
| `database.checkout_timeout` | u32/u64/i32 | Not found | Potentially unused |
| `database.reaping_frequency` | u32/u64/i32 | Not found | Potentially unused |
| `database.skip_migration_check` | bool | Not found | Potentially unused |
| `database.pool.max_connections` | u32/u64/i32 | Not found | Potentially unused |
| `database.pool.min_connections` | u32/u64/i32 | Not found | Potentially unused |
| `database.pool.acquire_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `database.pool.idle_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `database.pool.max_lifetime_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `database.variables.statement_timeout` | u32/u64/i32 | Not found | Potentially unused |
| `queues.backend` | String | Not found | Potentially unused |
| `queues.orchestration_namespace` | String | Not found | Potentially unused |
| `queues.worker_namespace` | String | Not found | Potentially unused |
| `queues.default_namespace` | String | Not found | Potentially unused |
| `queues.naming_pattern` | String | Not found | Potentially unused |
| `queues.default_visibility_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `queues.health_check_interval` | u32/u64/i32 | Not found | Potentially unused |
| `queues.default_batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `queues.max_batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `queues.orchestration_queues.task_requests` | String | Not found | Potentially unused |
| `queues.orchestration_queues.task_finalizations` | String | Not found | Potentially unused |
| `queues.orchestration_queues.step_results` | String | Not found | Potentially unused |
| `queues.pgmq.poll_interval_ms` | u32/u64/i32 | Not found | Potentially unused |
| `queues.pgmq.shutdown_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `queues.pgmq.max_retries` | u32/u64/i32 | Not found | Potentially unused |
| `queues.rabbitmq.connection_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `queues.rabbitmq.heartbeat_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `queues.rabbitmq.channel_pool_size` | u32/u64/i32 | Not found | Potentially unused |
| `shared_channels.event_publisher.event_queue_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `shared_channels.ffi.ruby_event_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.enabled` | bool | Not found | Potentially unused |
| `circuit_breakers.global_settings.max_circuit_breakers` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.global_settings.metrics_collection_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.global_settings.auto_create_enabled` | bool | Not found | Potentially unused |
| `circuit_breakers.global_settings.min_state_transition_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.default_config.failure_threshold` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.default_config.timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.default_config.success_threshold` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.component_configs.pgmq.failure_threshold` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.component_configs.pgmq.timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.component_configs.pgmq.success_threshold` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.component_configs.task_readiness.failure_threshold` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.component_configs.task_readiness.timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `circuit_breakers.component_configs.task_readiness.success_threshold` | u32/u64/i32 | Not found | Potentially unused |

## Uorchestration Configuration

| Parameter Path | Type | Usage Locations | Notes |
|----------------|------|-----------------|-------|
| `environment` | String | Not found | Potentially unused |
| `backoff.default_backoff_seconds` | Array | Not found | Potentially unused |
| `backoff.max_backoff_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.backoff_multiplier` | f64 | Not found | Potentially unused |
| `backoff.jitter_enabled` | bool | Not found | Potentially unused |
| `backoff.jitter_max_percentage` | f64 | Not found | Potentially unused |
| `backoff.default_reenqueue_delay` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.buffer_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.reenqueue_delays.initializing` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.reenqueue_delays.enqueuing_steps` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.reenqueue_delays.steps_in_process` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.reenqueue_delays.evaluating_results` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.reenqueue_delays.waiting_for_dependencies` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.reenqueue_delays.waiting_for_retry` | u32/u64/i32 | Not found | Potentially unused |
| `backoff.reenqueue_delays.blocked_by_failures` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.mode` | String | Not found | Potentially unused |
| `orchestration_system.max_concurrent_orchestrators` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.enable_performance_logging` | bool | Not found | Potentially unused |
| `orchestration_system.use_unified_state_machine` | bool | Not found | Potentially unused |
| `orchestration_system.operational_state.enable_shutdown_aware_monitoring` | bool | Not found | Potentially unused |
| `orchestration_system.operational_state.suppress_alerts_during_shutdown` | bool | Not found | Potentially unused |
| `orchestration_system.operational_state.startup_health_threshold_multiplier` | f64 | Not found | Potentially unused |
| `orchestration_system.operational_state.shutdown_health_threshold_multiplier` | f64 | Not found | Potentially unused |
| `orchestration_system.operational_state.graceful_shutdown_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.operational_state.emergency_shutdown_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.operational_state.enable_transition_logging` | bool | Not found | Potentially unused |
| `orchestration_system.operational_state.transition_log_level` | String | Not found | Potentially unused |
| `orchestration_system.web.enabled` | bool | Not found | Potentially unused |
| `orchestration_system.web.bind_address` | String | Not found | Potentially unused |
| `orchestration_system.web.request_timeout_ms` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.max_request_size_mb` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.tls.enabled` | bool | Not found | Potentially unused |
| `orchestration_system.web.tls.cert_path` | String | Not found | Potentially unused |
| `orchestration_system.web.tls.key_path` | String | Not found | Potentially unused |
| `orchestration_system.web.database_pools.web_api_pool_size` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.database_pools.web_api_max_connections` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.database_pools.web_api_connection_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.database_pools.web_api_idle_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.database_pools.max_total_connections_hint` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.cors.enabled` | bool | Not found | Potentially unused |
| `orchestration_system.web.cors.allowed_origins` | Array | Not found | Potentially unused |
| `orchestration_system.web.cors.allowed_methods` | Array | Not found | Potentially unused |
| `orchestration_system.web.cors.allowed_headers` | Array | Not found | Potentially unused |
| `orchestration_system.web.cors.max_age_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.auth.enabled` | bool | Not found | Potentially unused |
| `orchestration_system.web.auth.jwt_issuer` | String | Not found | Potentially unused |
| `orchestration_system.web.auth.jwt_audience` | String | Not found | Potentially unused |
| `orchestration_system.web.auth.jwt_token_expiry_hours` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.auth.jwt_private_key` | String | Not found | Potentially unused |
| `orchestration_system.web.auth.jwt_public_key` | String | Not found | Potentially unused |
| `orchestration_system.web.auth.api_key` | String | Not found | Potentially unused |
| `orchestration_system.web.auth.api_key_header` | String | Not found | Potentially unused |
| `orchestration_system.web.rate_limiting.enabled` | bool | Not found | Potentially unused |
| `orchestration_system.web.rate_limiting.requests_per_minute` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.rate_limiting.burst_size` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.rate_limiting.per_client_limit` | bool | Not found | Potentially unused |
| `orchestration_system.web.resilience.circuit_breaker_enabled` | bool | Not found | Potentially unused |
| `orchestration_system.web.resilience.request_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.resilience.max_concurrent_requests` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_system.web.resource_monitoring.report_pool_usage_to_health_monitor` | bool | Not found | Potentially unused |
| `orchestration_system.web.resource_monitoring.pool_usage_warning_threshold` | f64 | Not found | Potentially unused |
| `orchestration_system.web.resource_monitoring.pool_usage_critical_threshold` | f64 | Not found | Potentially unused |
| `orchestration_events.system_id` | String | Not found | Potentially unused |
| `orchestration_events.deployment_mode` | String | Not found | Potentially unused |
| `orchestration_events.timing.health_check_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.timing.fallback_polling_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.timing.visibility_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.timing.processing_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.timing.claim_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.processing.max_concurrent_operations` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.processing.batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.processing.max_retries` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.processing.backoff.initial_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.processing.backoff.max_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.processing.backoff.multiplier` | f64 | Not found | Potentially unused |
| `orchestration_events.processing.backoff.jitter_percent` | f64 | Not found | Potentially unused |
| `orchestration_events.health.enabled` | bool | Not found | Potentially unused |
| `orchestration_events.health.performance_monitoring_enabled` | bool | Not found | Potentially unused |
| `orchestration_events.health.max_consecutive_errors` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.health.error_rate_threshold_per_minute` | u32/u64/i32 | Not found | Potentially unused |
| `orchestration_events.metadata.queues_populated_at_runtime` | bool | Not found | Potentially unused |
| `task_readiness_events.system_id` | String | Not found | Potentially unused |
| `task_readiness_events.deployment_mode` | String | Not found | Potentially unused |
| `task_readiness_events.timing.health_check_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.timing.fallback_polling_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.timing.visibility_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.timing.processing_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.timing.claim_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.processing.max_concurrent_operations` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.processing.batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.processing.max_retries` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.processing.backoff.initial_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.processing.backoff.max_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.processing.backoff.multiplier` | f64 | Not found | Potentially unused |
| `task_readiness_events.processing.backoff.jitter_percent` | f64 | Not found | Potentially unused |
| `task_readiness_events.health.enabled` | bool | Not found | Potentially unused |
| `task_readiness_events.health.performance_monitoring_enabled` | bool | Not found | Potentially unused |
| `task_readiness_events.health.max_consecutive_errors` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.health.error_rate_threshold_per_minute` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.enhanced_settings.startup_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.enhanced_settings.shutdown_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.enhanced_settings.metrics_enabled` | bool | Not found | Potentially unused |
| `task_readiness_events.metadata.enhanced_settings.rollback_threshold_percent` | f64 | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.global_channels` | Array | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.max_payload_size_bytes` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.parse_timeout_ms` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.namespace_patterns.task_ready` | String | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.namespace_patterns.task_state_change` | String | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.connection.max_connection_retries` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.connection.connection_retry_delay_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.notification.connection.auto_reconnect` | bool | Not found | Potentially unused |
| `task_readiness_events.metadata.event_channel.max_retries` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.event_channel.backoff.initial_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.event_channel.backoff.max_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.event_channel.backoff.multiplier` | f64 | Not found | Potentially unused |
| `task_readiness_events.metadata.event_channel.backoff.jitter_percent` | f64 | Not found | Potentially unused |
| `task_readiness_events.metadata.coordinator.instance_id_prefix` | String | Not found | Potentially unused |
| `task_readiness_events.metadata.coordinator.operation_timeout_ms` | u32/u64/i32 | Not found | Potentially unused |
| `task_readiness_events.metadata.coordinator.stats_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.command_processor.command_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.event_systems.event_channel_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.event_listeners.pgmq_event_buffer_size` | u32/u64/i32 | Not found | Potentially unused |

## Uworker Configuration

| Parameter Path | Type | Usage Locations | Notes |
|----------------|------|-----------------|-------|
| `environment` | String | Not found | Potentially unused |
| `worker_system.worker_id` | String | Not found | Potentially unused |
| `worker_system.worker_type` | String | Not found | Potentially unused |
| `worker_system.event_driven.enabled` | bool | Not found | Potentially unused |
| `worker_system.event_driven.deployment_mode` | String | Not found | Potentially unused |
| `worker_system.event_driven.health_monitoring_enabled` | bool | Not found | Potentially unused |
| `worker_system.event_driven.health_check_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.step_processing.claim_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.step_processing.max_retries` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.step_processing.retry_backoff_multiplier` | f64 | Not found | Potentially unused |
| `worker_system.step_processing.heartbeat_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.step_processing.max_concurrent_steps` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.listener.retry_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.listener.max_retry_attempts` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.listener.event_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.listener.health_check_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.listener.batch_processing` | bool | Not found | Potentially unused |
| `worker_system.listener.connection_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.fallback_poller.enabled` | bool | Not found | Potentially unused |
| `worker_system.fallback_poller.polling_interval_ms` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.fallback_poller.batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.fallback_poller.age_threshold_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.fallback_poller.max_age_hours` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.fallback_poller.visibility_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.fallback_poller.processable_states` | Array | Not found | Potentially unused |
| `worker_system.health_monitoring.health_check_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.health_monitoring.metrics_collection_enabled` | bool | Not found | Potentially unused |
| `worker_system.health_monitoring.performance_monitoring_enabled` | bool | Not found | Potentially unused |
| `worker_system.health_monitoring.step_processing_rate_threshold` | f64 | Not found | Potentially unused |
| `worker_system.health_monitoring.error_rate_threshold` | f64 | Not found | Potentially unused |
| `worker_system.health_monitoring.memory_usage_threshold_mb` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.resource_limits.max_memory_mb` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.resource_limits.max_cpu_percent` | f64 | Not found | Potentially unused |
| `worker_system.resource_limits.max_database_connections` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.resource_limits.max_queue_connections` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.queue_config.visibility_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.queue_config.batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.queue_config.polling_interval_ms` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.enabled` | bool | Not found | Potentially unused |
| `worker_system.web.bind_address` | String | Not found | Potentially unused |
| `worker_system.web.request_timeout_ms` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.max_request_size_mb` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.tls.enabled` | bool | Not found | Potentially unused |
| `worker_system.web.tls.cert_path` | String | Not found | Potentially unused |
| `worker_system.web.tls.key_path` | String | Not found | Potentially unused |
| `worker_system.web.database_pools.web_api_pool_size` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.database_pools.web_api_max_connections` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.database_pools.web_api_connection_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.database_pools.web_api_idle_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.database_pools.max_total_connections_hint` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.cors.enabled` | bool | Not found | Potentially unused |
| `worker_system.web.cors.allowed_origins` | Array | Not found | Potentially unused |
| `worker_system.web.cors.allowed_methods` | Array | Not found | Potentially unused |
| `worker_system.web.cors.allowed_headers` | Array | Not found | Potentially unused |
| `worker_system.web.cors.max_age_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.auth.enabled` | bool | Not found | Potentially unused |
| `worker_system.web.auth.jwt_issuer` | String | Not found | Potentially unused |
| `worker_system.web.auth.jwt_audience` | String | Not found | Potentially unused |
| `worker_system.web.auth.jwt_token_expiry_hours` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.auth.jwt_private_key` | String | Not found | Potentially unused |
| `worker_system.web.auth.jwt_public_key` | String | Not found | Potentially unused |
| `worker_system.web.auth.api_key` | String | Not found | Potentially unused |
| `worker_system.web.auth.api_key_header` | String | Not found | Potentially unused |
| `worker_system.web.rate_limiting.enabled` | bool | Not found | Potentially unused |
| `worker_system.web.rate_limiting.requests_per_minute` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.rate_limiting.burst_size` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.rate_limiting.per_client_limit` | bool | Not found | Potentially unused |
| `worker_system.web.resilience.circuit_breaker_enabled` | bool | Not found | Potentially unused |
| `worker_system.web.resilience.request_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.resilience.max_concurrent_requests` | u32/u64/i32 | Not found | Potentially unused |
| `worker_system.web.resource_monitoring.report_pool_usage_to_health_monitor` | bool | Not found | Potentially unused |
| `worker_system.web.resource_monitoring.pool_usage_warning_threshold` | f64 | Not found | Potentially unused |
| `worker_system.web.resource_monitoring.pool_usage_critical_threshold` | f64 | Not found | Potentially unused |
| `worker_system.web.endpoints.health_enabled` | bool | Not found | Potentially unused |
| `worker_system.web.endpoints.health_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.readiness_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.liveness_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.metrics_enabled` | bool | Not found | Potentially unused |
| `worker_system.web.endpoints.prometheus_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.worker_metrics_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.status_enabled` | bool | Not found | Potentially unused |
| `worker_system.web.endpoints.basic_status_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.detailed_status_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.namespace_health_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.registered_handlers_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.templates_enabled` | bool | Not found | Potentially unused |
| `worker_system.web.endpoints.templates_base_path` | String | Not found | Potentially unused |
| `worker_system.web.endpoints.template_cache_enabled` | bool | Not found | Potentially unused |
| `worker_events.system_id` | String | Not found | Potentially unused |
| `worker_events.deployment_mode` | String | Not found | Potentially unused |
| `worker_events.timing.health_check_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.timing.fallback_polling_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.timing.visibility_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.timing.processing_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.timing.claim_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.processing.max_concurrent_operations` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.processing.batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.processing.max_retries` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.processing.backoff.initial_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.processing.backoff.max_delay_ms` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.processing.backoff.multiplier` | f64 | Not found | Potentially unused |
| `worker_events.processing.backoff.jitter_percent` | f64 | Not found | Potentially unused |
| `worker_events.health.enabled` | bool | Not found | Potentially unused |
| `worker_events.health.performance_monitoring_enabled` | bool | Not found | Potentially unused |
| `worker_events.health.max_consecutive_errors` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.health.error_rate_threshold_per_minute` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.in_process_events.ffi_integration_enabled` | bool | Not found | Potentially unused |
| `worker_events.metadata.in_process_events.deduplication_cache_size` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.listener.retry_interval_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.listener.max_retry_attempts` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.listener.event_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.listener.batch_processing` | bool | Not found | Potentially unused |
| `worker_events.metadata.listener.connection_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.fallback_poller.enabled` | bool | Not found | Potentially unused |
| `worker_events.metadata.fallback_poller.polling_interval_ms` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.fallback_poller.batch_size` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.fallback_poller.age_threshold_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.fallback_poller.max_age_hours` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.fallback_poller.visibility_timeout_seconds` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.resource_limits.max_memory_mb` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.resource_limits.max_cpu_percent` | f64 | Not found | Potentially unused |
| `worker_events.metadata.resource_limits.max_database_connections` | u32/u64/i32 | Not found | Potentially unused |
| `worker_events.metadata.resource_limits.max_queue_connections` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.command_processor.command_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.event_systems.event_channel_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.event_subscribers.completion_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.event_subscribers.result_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.in_process_events.broadcast_buffer_size` | u32/u64/i32 | Not found | Potentially unused |
| `mpsc_channels.event_listeners.pgmq_event_buffer_size` | u32/u64/i32 | Not found | Potentially unused |

---

## Summary Statistics

### Parameter Counts by Context

| Context | Parameter Count |
|---------|----------------|
| Ucommon | 51 |
| Uorchestration | 122 |
| Uworker | 129 |

### Analysis Notes

- **Type Detection**: Inferred from TOML value format
- **Usage Locations**: Limited to first 10 matches per parameter
- **Search Scope**: Rust files (*.rs) and TOML files
- **Excluded**: target/, .git/, node_modules/

### Next Steps

1. Review "Not found" parameters for removal
2. Add documentation metadata to active parameters
3. Generate environment-specific recommendations
4. Create parameter relationship mappings

---

**Analysis Complete**

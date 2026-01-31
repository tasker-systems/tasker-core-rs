# Configuration Reference: worker



> 19/101 parameters documented
> Generated: 2026-01-31T02:17:55.519096738+00:00

---


## worker

Root-level worker parameters

**Path:** `worker`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `worker_id` | `String` | `"worker-default-001"` | Unique identifier for this worker instance |
| `worker_type` | `String` | `"general"` | Worker type classification for routing and reporting |


#### `worker.worker_id`

Unique identifier for this worker instance

- **Type:** `String`
- **Default:** `"worker-default-001"`
- **Valid Range:** non-empty string
- **System Impact:** Used in logging, metrics, and step claim attribution; must be unique across all worker instances in a cluster


#### `worker.worker_type`

Worker type classification for routing and reporting

- **Type:** `String`
- **Default:** `"general"`
- **Valid Range:** non-empty string
- **System Impact:** Used to match worker capabilities with step handler requirements; 'general' handles all step types


## circuit_breakers

**Path:** `worker.circuit_breakers`

### ffi_completion_send

**Path:** `worker.circuit_breakers.ffi_completion_send`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `integer` | `5` |  |
| `recovery_timeout_seconds` | `integer` | `5` |  |
| `slow_send_threshold_ms` | `u32` | `100` | Threshold in milliseconds above which FFI completion channel sends are logged as slow |
| `success_threshold` | `integer` | `2` |  |


#### `worker.circuit_breakers.ffi_completion_send.slow_send_threshold_ms`

Threshold in milliseconds above which FFI completion channel sends are logged as slow

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 10-10000
- **System Impact:** Observability: identifies when the FFI completion channel is under pressure from slow consumers


## event_systems

**Path:** `worker.event_systems`

### worker

**Path:** `worker.event_systems.worker`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `deployment_mode` | `DeploymentMode` | `"Hybrid"` | Event delivery mode: 'Hybrid' (LISTEN/NOTIFY + polling fallback), 'EventDrivenOnly', or 'PollingOnly' |
| `system_id` | `String` | `"worker-event-system"` | Unique identifier for the worker event system instance |


#### `worker.event_systems.worker.deployment_mode`

Event delivery mode: 'Hybrid' (LISTEN/NOTIFY + polling fallback), 'EventDrivenOnly', or 'PollingOnly'

- **Type:** `DeploymentMode`
- **Default:** `"Hybrid"`
- **Valid Range:** Hybrid | EventDrivenOnly | PollingOnly
- **System Impact:** Hybrid is recommended; EventDrivenOnly has lowest latency but no fallback; PollingOnly has highest latency but no LISTEN/NOTIFY dependency


#### `worker.event_systems.worker.system_id`

Unique identifier for the worker event system instance

- **Type:** `String`
- **Default:** `"worker-event-system"`
- **Valid Range:** non-empty string
- **System Impact:** Used in logging and metrics to distinguish this event system from others


#### health

**Path:** `worker.event_systems.worker.health`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` |  |
| `error_rate_threshold_per_minute` | `integer` | `20` |  |
| `max_consecutive_errors` | `integer` | `10` |  |
| `performance_monitoring_enabled` | `bool` | `true` |  |


#### metadata

**Path:** `worker.event_systems.worker.metadata`

##### fallback_poller

**Path:** `worker.event_systems.worker.metadata.fallback_poller`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `age_threshold_seconds` | `integer` | `5` |  |
| `batch_size` | `integer` | `20` |  |
| `enabled` | `bool` | `true` |  |
| `max_age_hours` | `integer` | `24` |  |
| `polling_interval_ms` | `integer` | `1000` |  |
| `supported_namespaces` | `array` | `[]` |  |
| `visibility_timeout_seconds` | `integer` | `30` |  |

##### in_process_events

**Path:** `worker.event_systems.worker.metadata.in_process_events`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `deduplication_cache_size` | `integer` | `10000` |  |
| `ffi_integration_enabled` | `bool` | `true` |  |

##### listener

**Path:** `worker.event_systems.worker.metadata.listener`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_processing` | `bool` | `true` |  |
| `connection_timeout_seconds` | `integer` | `30` |  |
| `event_timeout_seconds` | `integer` | `60` |  |
| `max_retry_attempts` | `integer` | `5` |  |
| `retry_interval_seconds` | `integer` | `5` |  |

##### resource_limits

**Path:** `worker.event_systems.worker.metadata.resource_limits`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_cpu_percent` | `float` | `80.0` |  |
| `max_database_connections` | `integer` | `100` |  |
| `max_memory_mb` | `integer` | `4096` |  |
| `max_queue_connections` | `integer` | `50` |  |

#### processing

**Path:** `worker.event_systems.worker.processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `integer` | `20` |  |
| `max_concurrent_operations` | `integer` | `100` |  |
| `max_retries` | `integer` | `3` |  |


##### backoff

**Path:** `worker.event_systems.worker.processing.backoff`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_delay_ms` | `integer` | `100` |  |
| `jitter_percent` | `float` | `0.1` |  |
| `max_delay_ms` | `integer` | `10000` |  |
| `multiplier` | `float` | `2.0` |  |

#### timing

**Path:** `worker.event_systems.worker.timing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `claim_timeout_seconds` | `integer` | `300` |  |
| `fallback_polling_interval_seconds` | `integer` | `2` |  |
| `health_check_interval_seconds` | `integer` | `30` |  |
| `processing_timeout_seconds` | `integer` | `60` |  |
| `visibility_timeout_seconds` | `integer` | `30` |  |


## grpc

**Path:** `worker.grpc`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `bind_address` | `String` | `"${TASKER_WORKER_GRPC_BIND_ADDRESS:-0.0.0.0:9191}"` | Socket address for the worker gRPC server |
| `enable_health_service` | `bool` | `true` |  |
| `enable_reflection` | `bool` | `true` |  |
| `enabled` | `bool` | `true` | Enable the gRPC API server for the worker service |
| `keepalive_interval_seconds` | `integer` | `30` |  |
| `keepalive_timeout_seconds` | `integer` | `20` |  |
| `max_concurrent_streams` | `u32` | `1000` | Maximum number of concurrent gRPC streams per connection |
| `max_frame_size` | `integer` | `16384` |  |
| `tls_enabled` | `bool` | `false` |  |


#### `worker.grpc.bind_address`

Socket address for the worker gRPC server

- **Type:** `String`
- **Default:** `"${TASKER_WORKER_GRPC_BIND_ADDRESS:-0.0.0.0:9191}"`
- **Valid Range:** host:port
- **System Impact:** Must not conflict with the REST API or orchestration gRPC ports; default 9191


#### `worker.grpc.enabled`

Enable the gRPC API server for the worker service

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no gRPC endpoints are available; clients must use REST


#### `worker.grpc.max_concurrent_streams`

Maximum number of concurrent gRPC streams per connection

- **Type:** `u32`
- **Default:** `1000`
- **Valid Range:** 1-10000
- **System Impact:** Workers typically handle more concurrent streams than orchestration; default 1000 reflects this


## health_monitoring

**Path:** `worker.health_monitoring`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `error_rate_threshold` | `f64` | `0.05` | Error rate threshold (0.0-1.0) above which the worker reports as unhealthy |
| `health_check_interval_seconds` | `u32` | `30` | Interval in seconds between worker health self-checks |
| `performance_monitoring_enabled` | `bool` | `true` |  |


#### `worker.health_monitoring.error_rate_threshold`

Error rate threshold (0.0-1.0) above which the worker reports as unhealthy

- **Type:** `f64`
- **Default:** `0.05`
- **Valid Range:** 0.0-1.0
- **System Impact:** A value of 0.05 means the worker becomes unhealthy if more than 5% of recent step executions fail


#### `worker.health_monitoring.health_check_interval_seconds`

Interval in seconds between worker health self-checks

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Controls how frequently the worker evaluates its own health status for readiness probes


## mpsc_channels

**Path:** `worker.mpsc_channels`

### command_processor

**Path:** `worker.mpsc_channels.command_processor`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `command_buffer_size` | `integer` | `2000` |  |


### domain_events

**Path:** `worker.mpsc_channels.domain_events`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `command_buffer_size` | `integer` | `1000` |  |
| `log_dropped_events` | `bool` | `true` |  |
| `shutdown_drain_timeout_ms` | `integer` | `5000` |  |


### event_listeners

**Path:** `worker.mpsc_channels.event_listeners`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `pgmq_event_buffer_size` | `integer` | `10000` |  |


### event_subscribers

**Path:** `worker.mpsc_channels.event_subscribers`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `completion_buffer_size` | `integer` | `1000` |  |
| `result_buffer_size` | `integer` | `1000` |  |


### event_systems

**Path:** `worker.mpsc_channels.event_systems`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `event_channel_buffer_size` | `integer` | `2000` |  |


### ffi_dispatch

**Path:** `worker.mpsc_channels.ffi_dispatch`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `callback_timeout_ms` | `integer` | `5000` |  |
| `completion_send_timeout_ms` | `integer` | `10000` |  |
| `completion_timeout_ms` | `integer` | `30000` |  |
| `dispatch_buffer_size` | `integer` | `1000` |  |
| `starvation_warning_threshold_ms` | `integer` | `10000` |  |


### handler_dispatch

**Path:** `worker.mpsc_channels.handler_dispatch`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `completion_buffer_size` | `integer` | `1000` |  |
| `dispatch_buffer_size` | `integer` | `1000` |  |
| `handler_timeout_ms` | `integer` | `30000` |  |
| `max_concurrent_handlers` | `integer` | `10` |  |


#### load_shedding

**Path:** `worker.mpsc_channels.handler_dispatch.load_shedding`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `capacity_threshold_percent` | `float` | `80.0` |  |
| `enabled` | `bool` | `true` |  |
| `warning_threshold_percent` | `float` | `70.0` |  |


### in_process_events

**Path:** `worker.mpsc_channels.in_process_events`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `broadcast_buffer_size` | `integer` | `2000` |  |
| `dispatch_timeout_ms` | `integer` | `5000` |  |
| `log_subscriber_errors` | `bool` | `true` |  |


## orchestration_client

**Path:** `worker.orchestration_client`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `base_url` | `String` | `"http://localhost:8080"` | Base URL of the orchestration REST API that this worker reports to |
| `max_retries` | `u32` | `3` | Maximum retry attempts for failed orchestration API calls |
| `timeout_ms` | `u32` | `30000` | HTTP request timeout in milliseconds for orchestration API calls |


#### `worker.orchestration_client.base_url`

Base URL of the orchestration REST API that this worker reports to

- **Type:** `String`
- **Default:** `"http://localhost:8080"`
- **Valid Range:** valid HTTP(S) URL
- **System Impact:** Workers send step completion results and health reports to this endpoint

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
| test | http://localhost:8080 | Local orchestration for testing |
| production | http://orchestration:8080 | Container-internal DNS in Kubernetes/Docker |

**Related:** `orchestration.web.bind_address`


#### `worker.orchestration_client.max_retries`

Maximum retry attempts for failed orchestration API calls

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-10
- **System Impact:** Retries use backoff; higher values improve resilience to transient network issues


#### `worker.orchestration_client.timeout_ms`

HTTP request timeout in milliseconds for orchestration API calls

- **Type:** `u32`
- **Default:** `30000`
- **Valid Range:** 100-300000
- **System Impact:** Worker-to-orchestration calls exceeding this timeout fail and may be retried


## step_processing

**Path:** `worker.step_processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `claim_timeout_seconds` | `u32` | `300` | Maximum time in seconds a step claim remains valid before expiring |
| `max_concurrent_steps` | `u32` | `50` | Maximum number of steps this worker processes simultaneously |
| `max_retries` | `u32` | `3` | Maximum number of retry attempts for a failed step at the worker level |


#### `worker.step_processing.claim_timeout_seconds`

Maximum time in seconds a step claim remains valid before expiring

- **Type:** `u32`
- **Default:** `300`
- **Valid Range:** 1-3600
- **System Impact:** If a worker fails to complete a step within this window, the claim expires and the step becomes available for retry


#### `worker.step_processing.max_concurrent_steps`

Maximum number of steps this worker processes simultaneously

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-100000
- **System Impact:** Primary worker concurrency control; bounded by semaphore in HandlerDispatchService


#### `worker.step_processing.max_retries`

Maximum number of retry attempts for a failed step at the worker level

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Worker-level retry cap; interacts with the orchestration-level execution.max_retries


## web

**Path:** `worker.web`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `bind_address` | `String` | `"${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8081}"` | Socket address for the worker REST API server |
| `enabled` | `bool` | `true` | Enable the REST API server for the worker service |
| `request_timeout_ms` | `integer` | `30000` |  |


#### `worker.web.bind_address`

Socket address for the worker REST API server

- **Type:** `String`
- **Default:** `"${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8081}"`
- **Valid Range:** host:port
- **System Impact:** Must not conflict with orchestration.web.bind_address when co-located; default 8081

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
| production | 0.0.0.0:8081 | Standard worker port; use TASKER_WEB_BIND_ADDRESS env var to override |
| test | 0.0.0.0:8081 | Default port offset from orchestration (8080) |


#### `worker.web.enabled`

Enable the REST API server for the worker service

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no HTTP endpoints are available; the worker operates via messaging only


### auth

**Path:** `worker.web.auth`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `api_key` | `String` | `""` |  |
| `api_key_header` | `String` | `"X-API-Key"` |  |
| `enabled` | `bool` | `false` | Enable authentication for the worker REST API |
| `jwt_audience` | `String` | `"worker-api"` |  |
| `jwt_issuer` | `String` | `"tasker-worker"` |  |
| `jwt_private_key` | `String` | `""` |  |
| `jwt_public_key` | `String` | `"${TASKER_JWT_PUBLIC_KEY:-}"` |  |
| `jwt_public_key_path` | `String` | `"${TASKER_JWT_PUBLIC_KEY_PATH:-}"` |  |
| `jwt_token_expiry_hours` | `integer` | `24` |  |


#### `worker.web.auth.enabled`

Enable authentication for the worker REST API

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When false, all worker API endpoints are unauthenticated


### database_pools

**Path:** `worker.web.database_pools`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_total_connections_hint` | `integer` | `25` |  |
| `web_api_connection_timeout_seconds` | `integer` | `30` |  |
| `web_api_idle_timeout_seconds` | `integer` | `600` |  |
| `web_api_max_connections` | `integer` | `15` |  |
| `web_api_pool_size` | `integer` | `10` |  |


### resilience

**Path:** `worker.web.resilience`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `circuit_breaker_enabled` | `bool` | `true` |  |



---

*Generated by `tasker-cli docs` — [Tasker Configuration System](https://github.com/tasker-systems/tasker-core)*
# Configuration Reference: orchestration



> 23/100 parameters documented
> Generated: 2026-01-31T02:17:53.342235702+00:00

---


## orchestration

Root-level orchestration parameters

**Path:** `orchestration`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enable_performance_logging` | `bool` | `true` | Enable detailed performance logging for orchestration actors |
| `mode` | `String` | `"standalone"` | Orchestration deployment mode |


#### `orchestration.enable_performance_logging`

Enable detailed performance logging for orchestration actors

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Emits timing metrics for task processing, step enqueueing, and result evaluation; disable in production if log volume is a concern


#### `orchestration.mode`

Orchestration deployment mode

- **Type:** `String`
- **Default:** `"standalone"`
- **Valid Range:** standalone
- **System Impact:** Reserved for future multi-node orchestration; standalone is the only supported mode


## batch_processing

**Path:** `orchestration.batch_processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `checkpoint_stall_minutes` | `integer` | `15` |  |
| `default_batch_size` | `integer` | `1000` |  |
| `enabled` | `bool` | `true` |  |
| `max_parallel_batches` | `integer` | `50` |  |


## decision_points

**Path:** `orchestration.decision_points`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enable_detailed_logging` | `bool` | `false` |  |
| `enable_metrics` | `bool` | `true` |  |
| `enabled` | `bool` | `true` |  |
| `max_decision_depth` | `integer` | `20` |  |
| `max_steps_per_decision` | `integer` | `100` |  |
| `warn_threshold_depth` | `integer` | `10` |  |
| `warn_threshold_steps` | `integer` | `50` |  |


## dlq

**Path:** `orchestration.dlq`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `auto_dlq_on_staleness` | `bool` | `true` | Automatically move stale tasks to the DLQ when staleness detection identifies them |
| `enabled` | `bool` | `true` | Enable the Dead Letter Queue subsystem for handling unrecoverable tasks |
| `include_full_task_snapshot` | `bool` | `true` |  |
| `max_pending_age_hours` | `u32` | `168` | Maximum age in hours a task can remain in a pending-like state before being considered stale |


#### `orchestration.dlq.auto_dlq_on_staleness`

Automatically move stale tasks to the DLQ when staleness detection identifies them

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, the staleness detector can autonomously DLQ tasks without manual intervention


#### `orchestration.dlq.enabled`

Enable the Dead Letter Queue subsystem for handling unrecoverable tasks

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, stale or failed tasks remain in their error state without DLQ routing


#### `orchestration.dlq.max_pending_age_hours`

Maximum age in hours a task can remain in a pending-like state before being considered stale

- **Type:** `u32`
- **Default:** `168`
- **Valid Range:** 1-720
- **System Impact:** Safety net for tasks that fall through other staleness thresholds; 168 = 1 week


### reasons

**Path:** `orchestration.dlq.reasons`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `dependency_cycle_detected` | `bool` | `true` |  |
| `manual_dlq` | `bool` | `true` |  |
| `max_retries_exceeded` | `bool` | `true` |  |
| `staleness_timeout` | `bool` | `true` |  |
| `worker_unavailable` | `bool` | `true` |  |


### staleness_detection

**Path:** `orchestration.dlq.staleness_detection`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `integer` | `100` |  |
| `detection_interval_seconds` | `u32` | `300` | Interval in seconds between staleness detection sweeps |
| `dry_run` | `bool` | `false` | Run staleness detection in observation-only mode without taking action |
| `enabled` | `bool` | `true` | Enable periodic scanning for stale tasks |


#### `orchestration.dlq.staleness_detection.detection_interval_seconds`

Interval in seconds between staleness detection sweeps

- **Type:** `u32`
- **Default:** `300`
- **Valid Range:** 30-3600
- **System Impact:** Lower values detect stale tasks faster but increase database query frequency


#### `orchestration.dlq.staleness_detection.dry_run`

Run staleness detection in observation-only mode without taking action

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** Logs what would be DLQ'd without actually transitioning tasks; useful for tuning thresholds


#### `orchestration.dlq.staleness_detection.enabled`

Enable periodic scanning for stale tasks

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no automatic staleness detection runs; tasks must be manually DLQ'd


#### actions

**Path:** `orchestration.dlq.staleness_detection.actions`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `auto_move_to_dlq` | `bool` | `true` |  |
| `auto_transition_to_error` | `bool` | `true` |  |
| `emit_events` | `bool` | `true` |  |
| `event_channel` | `String` | `"task_staleness_detected"` | PGMQ channel name for staleness detection events |


#### `orchestration.dlq.staleness_detection.actions.event_channel`

PGMQ channel name for staleness detection events

- **Type:** `String`
- **Default:** `"task_staleness_detected"`
- **Valid Range:** 1-255 characters
- **System Impact:** Consumers can subscribe to this channel for alerting or custom staleness handling


#### thresholds

**Path:** `orchestration.dlq.staleness_detection.thresholds`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `steps_in_process_minutes` | `integer` | `30` |  |
| `task_max_lifetime_hours` | `u32` | `24` | Absolute maximum lifetime for any task regardless of state |
| `waiting_for_dependencies_minutes` | `integer` | `60` |  |
| `waiting_for_retry_minutes` | `integer` | `30` |  |


#### `orchestration.dlq.staleness_detection.thresholds.task_max_lifetime_hours`

Absolute maximum lifetime for any task regardless of state

- **Type:** `u32`
- **Default:** `24`
- **Valid Range:** 1-168
- **System Impact:** Hard cap; tasks exceeding this age are considered stale even if actively processing


## event_systems

**Path:** `orchestration.event_systems`

### orchestration

**Path:** `orchestration.event_systems.orchestration`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `deployment_mode` | `DeploymentMode` | `"Hybrid"` | Event delivery mode: 'Hybrid' (LISTEN/NOTIFY + polling fallback), 'EventDrivenOnly', or 'PollingOnly' |
| `system_id` | `String` | `"orchestration-event-system"` | Unique identifier for the orchestration event system instance |


#### `orchestration.event_systems.orchestration.deployment_mode`

Event delivery mode: 'Hybrid' (LISTEN/NOTIFY + polling fallback), 'EventDrivenOnly', or 'PollingOnly'

- **Type:** `DeploymentMode`
- **Default:** `"Hybrid"`
- **Valid Range:** Hybrid | EventDrivenOnly | PollingOnly
- **System Impact:** Hybrid is recommended; EventDrivenOnly has lowest latency but no fallback; PollingOnly has highest latency but no LISTEN/NOTIFY dependency


#### `orchestration.event_systems.orchestration.system_id`

Unique identifier for the orchestration event system instance

- **Type:** `String`
- **Default:** `"orchestration-event-system"`
- **Valid Range:** non-empty string
- **System Impact:** Used in logging and metrics to distinguish this event system from others


#### health

**Path:** `orchestration.event_systems.orchestration.health`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` |  |
| `error_rate_threshold_per_minute` | `integer` | `20` |  |
| `max_consecutive_errors` | `integer` | `10` |  |
| `performance_monitoring_enabled` | `bool` | `true` |  |


#### processing

**Path:** `orchestration.event_systems.orchestration.processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `integer` | `20` |  |
| `max_concurrent_operations` | `integer` | `50` |  |
| `max_retries` | `integer` | `3` |  |


##### backoff

**Path:** `orchestration.event_systems.orchestration.processing.backoff`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_delay_ms` | `integer` | `100` |  |
| `jitter_percent` | `float` | `0.1` |  |
| `max_delay_ms` | `integer` | `10000` |  |
| `multiplier` | `float` | `2.0` |  |

#### timing

**Path:** `orchestration.event_systems.orchestration.timing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `claim_timeout_seconds` | `integer` | `300` |  |
| `fallback_polling_interval_seconds` | `integer` | `5` |  |
| `health_check_interval_seconds` | `integer` | `30` |  |
| `processing_timeout_seconds` | `integer` | `60` |  |
| `visibility_timeout_seconds` | `integer` | `30` |  |


### task_readiness

**Path:** `orchestration.event_systems.task_readiness`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `deployment_mode` | `String` | `"Hybrid"` |  |
| `system_id` | `String` | `"task-readiness-event-system"` |  |


#### health

**Path:** `orchestration.event_systems.task_readiness.health`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` |  |
| `error_rate_threshold_per_minute` | `integer` | `20` |  |
| `max_consecutive_errors` | `integer` | `10` |  |
| `performance_monitoring_enabled` | `bool` | `true` |  |


#### processing

**Path:** `orchestration.event_systems.task_readiness.processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `integer` | `50` |  |
| `max_concurrent_operations` | `integer` | `100` |  |
| `max_retries` | `integer` | `3` |  |


##### backoff

**Path:** `orchestration.event_systems.task_readiness.processing.backoff`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_delay_ms` | `integer` | `100` |  |
| `jitter_percent` | `float` | `0.1` |  |
| `max_delay_ms` | `integer` | `10000` |  |
| `multiplier` | `float` | `2.0` |  |

#### timing

**Path:** `orchestration.event_systems.task_readiness.timing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `claim_timeout_seconds` | `integer` | `300` |  |
| `fallback_polling_interval_seconds` | `integer` | `5` |  |
| `health_check_interval_seconds` | `integer` | `30` |  |
| `processing_timeout_seconds` | `integer` | `60` |  |
| `visibility_timeout_seconds` | `integer` | `30` |  |


## grpc

**Path:** `orchestration.grpc`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `bind_address` | `String` | `"${TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS:-0.0.0.0:9190}"` | Socket address for the gRPC server |
| `enable_health_service` | `bool` | `true` | Enable the gRPC health checking service (grpc.health.v1) |
| `enable_reflection` | `bool` | `true` | Enable gRPC server reflection for service discovery |
| `enabled` | `bool` | `true` | Enable the gRPC API server alongside the REST API |
| `keepalive_interval_seconds` | `integer` | `30` |  |
| `keepalive_timeout_seconds` | `integer` | `20` |  |
| `max_concurrent_streams` | `integer` | `200` |  |
| `max_frame_size` | `integer` | `16384` |  |
| `tls_enabled` | `bool` | `false` | Enable TLS encryption for gRPC connections |


#### `orchestration.grpc.bind_address`

Socket address for the gRPC server

- **Type:** `String`
- **Default:** `"${TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS:-0.0.0.0:9190}"`
- **Valid Range:** host:port
- **System Impact:** Must not conflict with the REST API bind_address; default 9190 avoids Prometheus port conflict


#### `orchestration.grpc.enable_health_service`

Enable the gRPC health checking service (grpc.health.v1)

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Required for gRPC-native health checks used by load balancers and container orchestrators


#### `orchestration.grpc.enable_reflection`

Enable gRPC server reflection for service discovery

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Allows tools like grpcurl to list and inspect services; safe to enable in development, consider disabling in production


#### `orchestration.grpc.enabled`

Enable the gRPC API server alongside the REST API

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no gRPC endpoints are available; clients must use REST


#### `orchestration.grpc.tls_enabled`

Enable TLS encryption for gRPC connections

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When true, tls_cert_path and tls_key_path must be provided; required for production gRPC deployments


## mpsc_channels

**Path:** `orchestration.mpsc_channels`

### command_processor

**Path:** `orchestration.mpsc_channels.command_processor`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `command_buffer_size` | `integer` | `5000` |  |


### event_listeners

**Path:** `orchestration.mpsc_channels.event_listeners`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `pgmq_event_buffer_size` | `integer` | `50000` |  |


### event_systems

**Path:** `orchestration.mpsc_channels.event_systems`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `event_channel_buffer_size` | `integer` | `10000` |  |


## web

**Path:** `orchestration.web`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `bind_address` | `String` | `"${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8080}"` | Socket address for the REST API server |
| `enabled` | `bool` | `true` | Enable the REST API server for the orchestration service |
| `request_timeout_ms` | `u32` | `30000` | Maximum time in milliseconds for an HTTP request to complete before timeout |


#### `orchestration.web.bind_address`

Socket address for the REST API server

- **Type:** `String`
- **Default:** `"${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8080}"`
- **Valid Range:** host:port
- **System Impact:** Determines where the orchestration REST API listens; use 0.0.0.0 for container deployments

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
| production | 0.0.0.0:8080 | Standard port; use TASKER_WEB_BIND_ADDRESS env var to override in CI |
| test | 0.0.0.0:8080 | Default port for test fixtures |


#### `orchestration.web.enabled`

Enable the REST API server for the orchestration service

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no HTTP endpoints are available; the service operates via messaging only


#### `orchestration.web.request_timeout_ms`

Maximum time in milliseconds for an HTTP request to complete before timeout

- **Type:** `u32`
- **Default:** `30000`
- **Valid Range:** 100-300000
- **System Impact:** Requests exceeding this timeout return HTTP 408; protects against slow client connections


### auth

**Path:** `orchestration.web.auth`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `api_key` | `String` | `""` |  |
| `api_key_header` | `String` | `"X-API-Key"` | HTTP header name for API key authentication |
| `enabled` | `bool` | `false` | Enable authentication for the REST API |
| `jwt_audience` | `String` | `"tasker-api"` |  |
| `jwt_issuer` | `String` | `"tasker-core"` | Expected 'iss' claim in JWT tokens |
| `jwt_private_key` | `String` | `""` |  |
| `jwt_public_key` | `String` | `"${TASKER_JWT_PUBLIC_KEY:-}"` |  |
| `jwt_public_key_path` | `String` | `"${TASKER_JWT_PUBLIC_KEY_PATH:-}"` |  |
| `jwt_token_expiry_hours` | `integer` | `24` |  |


#### `orchestration.web.auth.api_key_header`

HTTP header name for API key authentication

- **Type:** `String`
- **Default:** `"X-API-Key"`
- **Valid Range:** valid HTTP header name
- **System Impact:** Clients send their API key in this header; default is X-API-Key


#### `orchestration.web.auth.enabled`

Enable authentication for the REST API

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When false, all API endpoints are unauthenticated; enable in production with JWT or API key auth


#### `orchestration.web.auth.jwt_issuer`

Expected 'iss' claim in JWT tokens

- **Type:** `String`
- **Default:** `"tasker-core"`
- **Valid Range:** non-empty string
- **System Impact:** Tokens with a different issuer are rejected during validation


### database_pools

**Path:** `orchestration.web.database_pools`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_total_connections_hint` | `integer` | `50` |  |
| `web_api_connection_timeout_seconds` | `integer` | `30` |  |
| `web_api_idle_timeout_seconds` | `integer` | `600` |  |
| `web_api_max_connections` | `integer` | `30` |  |
| `web_api_pool_size` | `integer` | `20` |  |


### resilience

**Path:** `orchestration.web.resilience`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `circuit_breaker_enabled` | `bool` | `true` |  |



---

*Generated by `tasker-cli docs` — [Tasker Configuration System](https://github.com/tasker-systems/tasker-core)*
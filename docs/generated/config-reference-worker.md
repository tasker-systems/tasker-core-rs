# Configuration Reference: worker



> 101/101 parameters documented
> Generated: 2026-01-31T02:40:25.282081985+00:00

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
| `failure_threshold` | `u32` | `5` | Number of consecutive FFI completion send failures before the circuit breaker trips |
| `recovery_timeout_seconds` | `u32` | `5` | Time the FFI completion breaker stays Open before probing with a test send |
| `slow_send_threshold_ms` | `u32` | `100` | Threshold in milliseconds above which FFI completion channel sends are logged as slow |
| `success_threshold` | `u32` | `2` | Consecutive successful sends in Half-Open required to close the breaker |


#### `worker.circuit_breakers.ffi_completion_send.failure_threshold`

Number of consecutive FFI completion send failures before the circuit breaker trips

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-100
- **System Impact:** Protects the FFI completion channel from cascading failures; when tripped, sends are short-circuited


#### `worker.circuit_breakers.ffi_completion_send.recovery_timeout_seconds`

Time the FFI completion breaker stays Open before probing with a test send

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-300
- **System Impact:** Short timeout (5s) because FFI channel issues are typically transient


#### `worker.circuit_breakers.ffi_completion_send.slow_send_threshold_ms`

Threshold in milliseconds above which FFI completion channel sends are logged as slow

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 10-10000
- **System Impact:** Observability: identifies when the FFI completion channel is under pressure from slow consumers


#### `worker.circuit_breakers.ffi_completion_send.success_threshold`

Consecutive successful sends in Half-Open required to close the breaker

- **Type:** `u32`
- **Default:** `2`
- **Valid Range:** 1-100
- **System Impact:** Low threshold (2) allows fast recovery since FFI send failures are usually transient


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
| `enabled` | `bool` | `true` | Enable health monitoring for the worker event system |
| `error_rate_threshold_per_minute` | `u32` | `20` | Error rate per minute above which the worker event system reports as unhealthy |
| `max_consecutive_errors` | `u32` | `10` | Number of consecutive errors before the worker event system reports as unhealthy |
| `performance_monitoring_enabled` | `bool` | `true` | Enable detailed performance metrics for worker event processing |


#### `worker.event_systems.worker.health.enabled`

Enable health monitoring for the worker event system

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no health checks or error tracking run for the worker event system


#### `worker.event_systems.worker.health.error_rate_threshold_per_minute`

Error rate per minute above which the worker event system reports as unhealthy

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-10000
- **System Impact:** Rate-based health signal complementing max_consecutive_errors


#### `worker.event_systems.worker.health.max_consecutive_errors`

Number of consecutive errors before the worker event system reports as unhealthy

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-1000
- **System Impact:** Triggers health status degradation; resets on any successful event processing


#### `worker.event_systems.worker.health.performance_monitoring_enabled`

Enable detailed performance metrics for worker event processing

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Tracks step dispatch latency and throughput; useful for tuning concurrency settings


#### metadata

**Path:** `worker.event_systems.worker.metadata`

##### fallback_poller

**Path:** `worker.event_systems.worker.metadata.fallback_poller`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `age_threshold_seconds` | `u32` | `5` | Minimum age in seconds of a message before the fallback poller will pick it up |
| `batch_size` | `u32` | `20` | Number of messages to dequeue in a single fallback poll |
| `enabled` | `bool` | `true` | Enable the fallback polling mechanism for step dispatch |
| `max_age_hours` | `u32` | `24` | Maximum age in hours of messages the fallback poller will process |
| `polling_interval_ms` | `u32` | `1000` | Interval in milliseconds between fallback polling cycles |
| `supported_namespaces` | `Vec<String>` | `[]` | List of queue namespaces the fallback poller monitors; empty means all namespaces |
| `visibility_timeout_seconds` | `u32` | `30` | Time in seconds a message polled by the fallback mechanism remains invisible |

##### in_process_events

**Path:** `worker.event_systems.worker.metadata.in_process_events`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `deduplication_cache_size` | `usize` | `10000` | Number of event IDs to cache for deduplication of in-process events |
| `ffi_integration_enabled` | `bool` | `true` | Enable FFI integration for in-process event delivery to Ruby/Python workers |

##### listener

**Path:** `worker.event_systems.worker.metadata.listener`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_processing` | `bool` | `true` | Enable batch processing of accumulated LISTEN/NOTIFY events |
| `connection_timeout_seconds` | `u32` | `30` | Maximum time to wait when establishing the LISTEN/NOTIFY PostgreSQL connection |
| `event_timeout_seconds` | `u32` | `60` | Maximum time to wait for a LISTEN/NOTIFY event before yielding |
| `max_retry_attempts` | `u32` | `5` | Maximum number of listener reconnection attempts before falling back to polling |
| `retry_interval_seconds` | `u32` | `5` | Interval in seconds between LISTEN/NOTIFY listener reconnection attempts |

##### resource_limits

**Path:** `worker.event_systems.worker.metadata.resource_limits`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_cpu_percent` | `f64` | `80.0` | Maximum CPU utilization percentage the worker event system should target |
| `max_database_connections` | `u32` | `100` | Maximum number of database connections the worker event system can use |
| `max_memory_mb` | `u32` | `4096` | Maximum memory in megabytes the worker event system is expected to use |
| `max_queue_connections` | `u32` | `50` | Maximum number of queue connections the worker event system can use |

#### processing

**Path:** `worker.event_systems.worker.processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `u32` | `20` | Number of events dequeued in a single batch read by the worker |
| `max_concurrent_operations` | `u32` | `100` | Maximum number of events processed concurrently by the worker event system |
| `max_retries` | `u32` | `3` | Maximum retry attempts for a failed worker event processing operation |


#### `worker.event_systems.worker.processing.batch_size`

Number of events dequeued in a single batch read by the worker

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-1000
- **System Impact:** Larger batches improve throughput but increase per-batch processing time


#### `worker.event_systems.worker.processing.max_concurrent_operations`

Maximum number of events processed concurrently by the worker event system

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 1-10000
- **System Impact:** Controls parallelism for step dispatch and completion processing


#### `worker.event_systems.worker.processing.max_retries`

Maximum retry attempts for a failed worker event processing operation

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Events exceeding this retry count are dropped or sent to the DLQ


##### backoff

**Path:** `worker.event_systems.worker.processing.backoff`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_delay_ms` | `u64` | `100` | Initial backoff delay in milliseconds after first worker event processing failure |
| `jitter_percent` | `f64` | `0.1` | Maximum jitter as a fraction of the computed backoff delay |
| `max_delay_ms` | `u64` | `10000` | Maximum backoff delay in milliseconds between worker event retries |
| `multiplier` | `f64` | `2.0` | Multiplier applied to the backoff delay after each consecutive failure |

#### timing

**Path:** `worker.event_systems.worker.timing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `claim_timeout_seconds` | `u32` | `300` | Maximum time in seconds a worker event claim remains valid |
| `fallback_polling_interval_seconds` | `u32` | `2` | Interval in seconds between fallback polling cycles for step dispatch |
| `health_check_interval_seconds` | `u32` | `30` | Interval in seconds between health check probes for the worker event system |
| `processing_timeout_seconds` | `u32` | `60` | Maximum time in seconds allowed for processing a single worker event |
| `visibility_timeout_seconds` | `u32` | `30` | Time in seconds a dequeued step dispatch message remains invisible to other workers |


#### `worker.event_systems.worker.timing.claim_timeout_seconds`

Maximum time in seconds a worker event claim remains valid

- **Type:** `u32`
- **Default:** `300`
- **Valid Range:** 1-3600
- **System Impact:** Prevents abandoned claims from blocking step processing indefinitely


#### `worker.event_systems.worker.timing.fallback_polling_interval_seconds`

Interval in seconds between fallback polling cycles for step dispatch

- **Type:** `u32`
- **Default:** `2`
- **Valid Range:** 1-60
- **System Impact:** Shorter than orchestration (2s vs 5s) because workers need fast step pickup for low latency


#### `worker.event_systems.worker.timing.health_check_interval_seconds`

Interval in seconds between health check probes for the worker event system

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Controls how frequently the worker event system verifies its own connectivity


#### `worker.event_systems.worker.timing.processing_timeout_seconds`

Maximum time in seconds allowed for processing a single worker event

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 1-3600
- **System Impact:** Events exceeding this timeout are considered failed and may be retried


#### `worker.event_systems.worker.timing.visibility_timeout_seconds`

Time in seconds a dequeued step dispatch message remains invisible to other workers

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Prevents duplicate step execution; must be longer than typical step processing time


## grpc

**Path:** `worker.grpc`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `bind_address` | `String` | `"${TASKER_WORKER_GRPC_BIND_ADDRESS:-0.0.0.0:9191}"` | Socket address for the worker gRPC server |
| `enable_health_service` | `bool` | `true` | Enable the gRPC health checking service on the worker |
| `enable_reflection` | `bool` | `true` | Enable gRPC server reflection for the worker service |
| `enabled` | `bool` | `true` | Enable the gRPC API server for the worker service |
| `keepalive_interval_seconds` | `u32` | `30` | Interval in seconds between gRPC keepalive ping frames on the worker |
| `keepalive_timeout_seconds` | `u32` | `20` | Time in seconds to wait for a keepalive ping acknowledgment before closing the connection |
| `max_concurrent_streams` | `u32` | `1000` | Maximum number of concurrent gRPC streams per connection |
| `max_frame_size` | `u32` | `16384` | Maximum size in bytes of a single HTTP/2 frame for the worker gRPC server |
| `tls_enabled` | `bool` | `false` | Enable TLS encryption for worker gRPC connections |


#### `worker.grpc.bind_address`

Socket address for the worker gRPC server

- **Type:** `String`
- **Default:** `"${TASKER_WORKER_GRPC_BIND_ADDRESS:-0.0.0.0:9191}"`
- **Valid Range:** host:port
- **System Impact:** Must not conflict with the REST API or orchestration gRPC ports; default 9191


#### `worker.grpc.enable_health_service`

Enable the gRPC health checking service on the worker

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Required for gRPC-native health checks used by load balancers and container orchestrators


#### `worker.grpc.enable_reflection`

Enable gRPC server reflection for the worker service

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Allows tools like grpcurl to list and inspect worker services; consider disabling in production


#### `worker.grpc.enabled`

Enable the gRPC API server for the worker service

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no gRPC endpoints are available; clients must use REST


#### `worker.grpc.keepalive_interval_seconds`

Interval in seconds between gRPC keepalive ping frames on the worker

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Detects dead connections; lower values detect failures faster but increase network overhead


#### `worker.grpc.keepalive_timeout_seconds`

Time in seconds to wait for a keepalive ping acknowledgment before closing the connection

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-300
- **System Impact:** Connections that fail to acknowledge within this window are considered dead and closed


#### `worker.grpc.max_concurrent_streams`

Maximum number of concurrent gRPC streams per connection

- **Type:** `u32`
- **Default:** `1000`
- **Valid Range:** 1-10000
- **System Impact:** Workers typically handle more concurrent streams than orchestration; default 1000 reflects this


#### `worker.grpc.max_frame_size`

Maximum size in bytes of a single HTTP/2 frame for the worker gRPC server

- **Type:** `u32`
- **Default:** `16384`
- **Valid Range:** 16384-16777215
- **System Impact:** Larger frames reduce framing overhead for large messages but increase memory per-stream


#### `worker.grpc.tls_enabled`

Enable TLS encryption for worker gRPC connections

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When true, TLS cert and key paths must be provided; required for production gRPC deployments


## health_monitoring

**Path:** `worker.health_monitoring`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `error_rate_threshold` | `f64` | `0.05` | Error rate threshold (0.0-1.0) above which the worker reports as unhealthy |
| `health_check_interval_seconds` | `u32` | `30` | Interval in seconds between worker health self-checks |
| `performance_monitoring_enabled` | `bool` | `true` | Enable detailed performance metrics collection for worker health monitoring |


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


#### `worker.health_monitoring.performance_monitoring_enabled`

Enable detailed performance metrics collection for worker health monitoring

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Tracks step execution latency, throughput, and success rates for health assessment


## mpsc_channels

**Path:** `worker.mpsc_channels`

### command_processor

**Path:** `worker.mpsc_channels.command_processor`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `command_buffer_size` | `usize` | `2000` | Bounded channel capacity for the worker command processor |


#### `worker.mpsc_channels.command_processor.command_buffer_size`

Bounded channel capacity for the worker command processor

- **Type:** `usize`
- **Default:** `2000`
- **Valid Range:** 100-100000
- **System Impact:** Buffers incoming worker commands; smaller than orchestration (2000 vs 5000) since workers process fewer command types


### domain_events

**Path:** `worker.mpsc_channels.domain_events`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `command_buffer_size` | `usize` | `1000` | Bounded channel capacity for domain event system commands |
| `log_dropped_events` | `bool` | `true` | Log a warning when domain events are dropped due to channel saturation |
| `shutdown_drain_timeout_ms` | `u32` | `5000` | Maximum time in milliseconds to drain pending domain events during shutdown |


#### `worker.mpsc_channels.domain_events.command_buffer_size`

Bounded channel capacity for domain event system commands

- **Type:** `usize`
- **Default:** `1000`
- **Valid Range:** 100-50000
- **System Impact:** Buffers domain event system control commands such as publish, subscribe, and shutdown


#### `worker.mpsc_channels.domain_events.log_dropped_events`

Log a warning when domain events are dropped due to channel saturation

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Observability: helps detect when event volume exceeds channel capacity


#### `worker.mpsc_channels.domain_events.shutdown_drain_timeout_ms`

Maximum time in milliseconds to drain pending domain events during shutdown

- **Type:** `u32`
- **Default:** `5000`
- **Valid Range:** 100-60000
- **System Impact:** Ensures in-flight domain events are delivered before the worker exits; prevents event loss


### event_listeners

**Path:** `worker.mpsc_channels.event_listeners`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `pgmq_event_buffer_size` | `usize` | `10000` | Bounded channel capacity for PGMQ event listener notifications on the worker |


#### `worker.mpsc_channels.event_listeners.pgmq_event_buffer_size`

Bounded channel capacity for PGMQ event listener notifications on the worker

- **Type:** `usize`
- **Default:** `10000`
- **Valid Range:** 1000-1000000
- **System Impact:** Buffers PGMQ LISTEN/NOTIFY events; smaller than orchestration (10000 vs 50000) since workers handle fewer event types


### event_subscribers

**Path:** `worker.mpsc_channels.event_subscribers`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `completion_buffer_size` | `usize` | `1000` | Bounded channel capacity for step completion event subscribers |
| `result_buffer_size` | `usize` | `1000` | Bounded channel capacity for step result event subscribers |


#### `worker.mpsc_channels.event_subscribers.completion_buffer_size`

Bounded channel capacity for step completion event subscribers

- **Type:** `usize`
- **Default:** `1000`
- **Valid Range:** 100-50000
- **System Impact:** Buffers step completion notifications before they are forwarded to the orchestration service


#### `worker.mpsc_channels.event_subscribers.result_buffer_size`

Bounded channel capacity for step result event subscribers

- **Type:** `usize`
- **Default:** `1000`
- **Valid Range:** 100-50000
- **System Impact:** Buffers step execution results before they are published to the result queue


### event_systems

**Path:** `worker.mpsc_channels.event_systems`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `event_channel_buffer_size` | `usize` | `2000` | Bounded channel capacity for the worker event system internal channel |


#### `worker.mpsc_channels.event_systems.event_channel_buffer_size`

Bounded channel capacity for the worker event system internal channel

- **Type:** `usize`
- **Default:** `2000`
- **Valid Range:** 100-100000
- **System Impact:** Buffers events between the listener and processor; sized for worker-level throughput


### ffi_dispatch

**Path:** `worker.mpsc_channels.ffi_dispatch`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `callback_timeout_ms` | `u32` | `5000` | Maximum time in milliseconds for FFI fire-and-forget domain event callbacks |
| `completion_send_timeout_ms` | `u32` | `10000` | Maximum time in milliseconds to retry sending FFI completion results when the channel is full |
| `completion_timeout_ms` | `u32` | `30000` | Maximum time in milliseconds to wait for an FFI step handler to complete |
| `dispatch_buffer_size` | `usize` | `1000` | Bounded channel capacity for FFI step dispatch requests |
| `starvation_warning_threshold_ms` | `u32` | `10000` | Age in milliseconds of pending FFI events that triggers a starvation warning |


#### `worker.mpsc_channels.ffi_dispatch.callback_timeout_ms`

Maximum time in milliseconds for FFI fire-and-forget domain event callbacks

- **Type:** `u32`
- **Default:** `5000`
- **Valid Range:** 100-60000
- **System Impact:** Prevents indefinite blocking of FFI threads during domain event publishing


#### `worker.mpsc_channels.ffi_dispatch.completion_send_timeout_ms`

Maximum time in milliseconds to retry sending FFI completion results when the channel is full

- **Type:** `u32`
- **Default:** `10000`
- **Valid Range:** 1000-300000
- **System Impact:** Uses try_send with retry loop instead of blocking send to prevent deadlocks


#### `worker.mpsc_channels.ffi_dispatch.completion_timeout_ms`

Maximum time in milliseconds to wait for an FFI step handler to complete

- **Type:** `u32`
- **Default:** `30000`
- **Valid Range:** 1000-600000
- **System Impact:** FFI handlers exceeding this timeout are considered failed; guards against hung FFI threads


#### `worker.mpsc_channels.ffi_dispatch.dispatch_buffer_size`

Bounded channel capacity for FFI step dispatch requests

- **Type:** `usize`
- **Default:** `1000`
- **Valid Range:** 100-50000
- **System Impact:** Buffers step execution requests destined for Ruby/Python FFI handlers


#### `worker.mpsc_channels.ffi_dispatch.starvation_warning_threshold_ms`

Age in milliseconds of pending FFI events that triggers a starvation warning

- **Type:** `u32`
- **Default:** `10000`
- **Valid Range:** 1000-300000
- **System Impact:** Proactive detection of FFI channel starvation before completion_timeout_ms is reached


### handler_dispatch

**Path:** `worker.mpsc_channels.handler_dispatch`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `completion_buffer_size` | `usize` | `1000` | Bounded channel capacity for step handler completion notifications |
| `dispatch_buffer_size` | `usize` | `1000` | Bounded channel capacity for step handler dispatch requests |
| `handler_timeout_ms` | `u32` | `30000` | Maximum time in milliseconds for a step handler to complete execution |
| `max_concurrent_handlers` | `u32` | `10` | Maximum number of step handlers executing simultaneously |


#### `worker.mpsc_channels.handler_dispatch.completion_buffer_size`

Bounded channel capacity for step handler completion notifications

- **Type:** `usize`
- **Default:** `1000`
- **Valid Range:** 100-50000
- **System Impact:** Buffers handler completion results before they are forwarded to the result processor


#### `worker.mpsc_channels.handler_dispatch.dispatch_buffer_size`

Bounded channel capacity for step handler dispatch requests

- **Type:** `usize`
- **Default:** `1000`
- **Valid Range:** 100-50000
- **System Impact:** Buffers incoming step execution requests before handler assignment


#### `worker.mpsc_channels.handler_dispatch.handler_timeout_ms`

Maximum time in milliseconds for a step handler to complete execution

- **Type:** `u32`
- **Default:** `30000`
- **Valid Range:** 1000-600000
- **System Impact:** Handlers exceeding this timeout are cancelled; prevents hung handlers from consuming capacity


#### `worker.mpsc_channels.handler_dispatch.max_concurrent_handlers`

Maximum number of step handlers executing simultaneously

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-10000
- **System Impact:** Controls per-worker parallelism; bounded by the handler dispatch semaphore


#### load_shedding

**Path:** `worker.mpsc_channels.handler_dispatch.load_shedding`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `capacity_threshold_percent` | `f64` | `80.0` | Handler capacity percentage above which new step claims are refused |
| `enabled` | `bool` | `true` | Enable load shedding to refuse step claims when handler capacity is nearly exhausted |
| `warning_threshold_percent` | `f64` | `70.0` | Handler capacity percentage at which warning logs are emitted |


#### `worker.mpsc_channels.handler_dispatch.load_shedding.capacity_threshold_percent`

Handler capacity percentage above which new step claims are refused

- **Type:** `f64`
- **Default:** `80.0`
- **Valid Range:** 0.0-100.0
- **System Impact:** At 80%, the worker stops accepting new steps when 80% of max_concurrent_handlers are busy


#### `worker.mpsc_channels.handler_dispatch.load_shedding.enabled`

Enable load shedding to refuse step claims when handler capacity is nearly exhausted

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, the worker refuses new step claims above the capacity threshold to prevent overload


#### `worker.mpsc_channels.handler_dispatch.load_shedding.warning_threshold_percent`

Handler capacity percentage at which warning logs are emitted

- **Type:** `f64`
- **Default:** `70.0`
- **Valid Range:** 0.0-100.0
- **System Impact:** Observability: alerts operators that the worker is approaching its capacity limit


### in_process_events

**Path:** `worker.mpsc_channels.in_process_events`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `broadcast_buffer_size` | `usize` | `2000` | Bounded broadcast channel capacity for in-process domain event delivery |
| `dispatch_timeout_ms` | `u32` | `5000` | Maximum time in milliseconds to wait when dispatching an in-process event |
| `log_subscriber_errors` | `bool` | `true` | Log errors when in-process event subscribers fail to receive events |


#### `worker.mpsc_channels.in_process_events.broadcast_buffer_size`

Bounded broadcast channel capacity for in-process domain event delivery

- **Type:** `usize`
- **Default:** `2000`
- **Valid Range:** 100-100000
- **System Impact:** Controls how many domain events can be buffered before slow subscribers cause backpressure


#### `worker.mpsc_channels.in_process_events.dispatch_timeout_ms`

Maximum time in milliseconds to wait when dispatching an in-process event

- **Type:** `u32`
- **Default:** `5000`
- **Valid Range:** 100-60000
- **System Impact:** Prevents event dispatch from blocking indefinitely if all subscribers are slow


#### `worker.mpsc_channels.in_process_events.log_subscriber_errors`

Log errors when in-process event subscribers fail to receive events

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Observability: helps identify slow or failing event subscribers


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
| `request_timeout_ms` | `u32` | `30000` | Maximum time in milliseconds for a worker HTTP request to complete before timeout |


#### `worker.web.bind_address`

Socket address for the worker REST API server

- **Type:** `String`
- **Default:** `"${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8081}"`
- **Valid Range:** host:port
- **System Impact:** Must not conflict with orchestration.web.bind_address when co-located; default 8081

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
| test | 0.0.0.0:8081 | Default port offset from orchestration (8080) |
| production | 0.0.0.0:8081 | Standard worker port; use TASKER_WEB_BIND_ADDRESS env var to override |


#### `worker.web.enabled`

Enable the REST API server for the worker service

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no HTTP endpoints are available; the worker operates via messaging only


#### `worker.web.request_timeout_ms`

Maximum time in milliseconds for a worker HTTP request to complete before timeout

- **Type:** `u32`
- **Default:** `30000`
- **Valid Range:** 100-300000
- **System Impact:** Requests exceeding this timeout return HTTP 408; protects against slow client connections


### auth

**Path:** `worker.web.auth`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `api_key` | `String` | `""` | Static API key for simple key-based authentication on the worker API |
| `api_key_header` | `String` | `"X-API-Key"` | HTTP header name for API key authentication on the worker API |
| `enabled` | `bool` | `false` | Enable authentication for the worker REST API |
| `jwt_audience` | `String` | `"worker-api"` | Expected 'aud' claim in JWT tokens for the worker API |
| `jwt_issuer` | `String` | `"tasker-worker"` | Expected 'iss' claim in JWT tokens for the worker API |
| `jwt_private_key` | `String` | `""` | PEM-encoded private key for signing JWT tokens (if the worker issues tokens) |
| `jwt_public_key` | `String` | `"${TASKER_JWT_PUBLIC_KEY:-}"` | PEM-encoded public key for verifying JWT token signatures on the worker API |
| `jwt_public_key_path` | `String` | `"${TASKER_JWT_PUBLIC_KEY_PATH:-}"` | File path to a PEM-encoded public key for worker JWT verification |
| `jwt_token_expiry_hours` | `u32` | `24` | Default JWT token validity period in hours for worker API tokens |


#### `worker.web.auth.api_key`

Static API key for simple key-based authentication on the worker API

- **Type:** `String`
- **Default:** `""`
- **Valid Range:** non-empty string or empty to disable
- **System Impact:** When non-empty and auth is enabled, clients can authenticate by sending this key in the api_key_header


#### `worker.web.auth.api_key_header`

HTTP header name for API key authentication on the worker API

- **Type:** `String`
- **Default:** `"X-API-Key"`
- **Valid Range:** valid HTTP header name
- **System Impact:** Clients send their API key in this header; default is X-API-Key


#### `worker.web.auth.enabled`

Enable authentication for the worker REST API

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When false, all worker API endpoints are unauthenticated


#### `worker.web.auth.jwt_audience`

Expected 'aud' claim in JWT tokens for the worker API

- **Type:** `String`
- **Default:** `"worker-api"`
- **Valid Range:** non-empty string
- **System Impact:** Tokens with a different audience are rejected during validation


#### `worker.web.auth.jwt_issuer`

Expected 'iss' claim in JWT tokens for the worker API

- **Type:** `String`
- **Default:** `"tasker-worker"`
- **Valid Range:** non-empty string
- **System Impact:** Tokens with a different issuer are rejected; default 'tasker-worker' distinguishes worker tokens from orchestration tokens


#### `worker.web.auth.jwt_private_key`

PEM-encoded private key for signing JWT tokens (if the worker issues tokens)

- **Type:** `String`
- **Default:** `""`
- **Valid Range:** valid PEM private key or empty
- **System Impact:** Required only if the worker service issues its own JWT tokens; typically empty


#### `worker.web.auth.jwt_public_key`

PEM-encoded public key for verifying JWT token signatures on the worker API

- **Type:** `String`
- **Default:** `"${TASKER_JWT_PUBLIC_KEY:-}"`
- **Valid Range:** valid PEM public key or empty
- **System Impact:** Required for JWT validation; prefer jwt_public_key_path for file-based key management


#### `worker.web.auth.jwt_public_key_path`

File path to a PEM-encoded public key for worker JWT verification

- **Type:** `String`
- **Default:** `"${TASKER_JWT_PUBLIC_KEY_PATH:-}"`
- **Valid Range:** valid file path or empty
- **System Impact:** Alternative to inline jwt_public_key; supports key rotation by replacing the file


#### `worker.web.auth.jwt_token_expiry_hours`

Default JWT token validity period in hours for worker API tokens

- **Type:** `u32`
- **Default:** `24`
- **Valid Range:** 1-720
- **System Impact:** Tokens older than this are rejected; shorter values improve security


### database_pools

**Path:** `worker.web.database_pools`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_total_connections_hint` | `u32` | `25` | Advisory hint for the total number of database connections across all worker pools |
| `web_api_connection_timeout_seconds` | `u32` | `30` | Maximum time to wait when acquiring a connection from the worker web API pool |
| `web_api_idle_timeout_seconds` | `u32` | `600` | Time before an idle worker web API connection is closed |
| `web_api_max_connections` | `u32` | `15` | Maximum number of connections the worker web API pool can grow to under load |
| `web_api_pool_size` | `u32` | `10` | Target number of connections in the worker web API database pool |


#### `worker.web.database_pools.max_total_connections_hint`

Advisory hint for the total number of database connections across all worker pools

- **Type:** `u32`
- **Default:** `25`
- **Valid Range:** 1-1000
- **System Impact:** Used for capacity planning; not enforced but logged if actual connections exceed this hint


#### `worker.web.database_pools.web_api_connection_timeout_seconds`

Maximum time to wait when acquiring a connection from the worker web API pool

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-300
- **System Impact:** Worker API requests that cannot acquire a connection within this window return an error


#### `worker.web.database_pools.web_api_idle_timeout_seconds`

Time before an idle worker web API connection is closed

- **Type:** `u32`
- **Default:** `600`
- **Valid Range:** 1-3600
- **System Impact:** Controls how quickly the worker web API pool shrinks after traffic subsides


#### `worker.web.database_pools.web_api_max_connections`

Maximum number of connections the worker web API pool can grow to under load

- **Type:** `u32`
- **Default:** `15`
- **Valid Range:** 1-500
- **System Impact:** Hard ceiling for worker web API database connections


#### `worker.web.database_pools.web_api_pool_size`

Target number of connections in the worker web API database pool

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-200
- **System Impact:** Determines how many concurrent database queries the worker REST API can execute; smaller than orchestration


### resilience

**Path:** `worker.web.resilience`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `circuit_breaker_enabled` | `bool` | `true` | Enable circuit breaker protection for the worker REST API |


#### `worker.web.resilience.circuit_breaker_enabled`

Enable circuit breaker protection for the worker REST API

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, the worker API uses circuit breakers to protect against cascading failures



---

*Generated by `tasker-cli docs` — [Tasker Configuration System](https://github.com/tasker-systems/tasker-core)*
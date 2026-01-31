# Configuration Reference: common



> 63/102 parameters documented
> Generated: 2026-01-31T02:17:51.128440524+00:00

---


## backoff

**Path:** `common.backoff`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `backoff_multiplier` | `f64` | `2.0` | Multiplier applied to the previous delay for exponential backoff calculations |
| `default_backoff_seconds` | `Vec<u32>` | `[1, 5, 15, 30, 60]` | Sequence of backoff delays in seconds for successive retry attempts |
| `jitter_enabled` | `bool` | `true` | Add random jitter to backoff delays to prevent thundering herd on retry |
| `jitter_max_percentage` | `f64` | `0.15` | Maximum jitter as a fraction of the computed backoff delay |
| `max_backoff_seconds` | `u32` | `3600` | Hard upper limit on any single backoff delay |


#### `common.backoff.backoff_multiplier`

Multiplier applied to the previous delay for exponential backoff calculations

- **Type:** `f64`
- **Default:** `2.0`
- **Valid Range:** 1.0-10.0
- **System Impact:** Controls how aggressively delays grow; 2.0 means each delay is double the previous


#### `common.backoff.default_backoff_seconds`

Sequence of backoff delays in seconds for successive retry attempts

- **Type:** `Vec<u32>`
- **Default:** `[1, 5, 15, 30, 60]`
- **Valid Range:** non-empty array of positive integers
- **System Impact:** Defines the retry cadence; after exhausting the array, the last value is reused up to max_backoff_seconds


#### `common.backoff.jitter_enabled`

Add random jitter to backoff delays to prevent thundering herd on retry

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, backoff delays are randomized within jitter_max_percentage to spread retries across time


#### `common.backoff.jitter_max_percentage`

Maximum jitter as a fraction of the computed backoff delay

- **Type:** `f64`
- **Default:** `0.15`
- **Valid Range:** 0.0-1.0
- **System Impact:** A value of 0.15 means delays vary by up to +/-15% of the base delay


#### `common.backoff.max_backoff_seconds`

Hard upper limit on any single backoff delay

- **Type:** `u32`
- **Default:** `3600`
- **Valid Range:** 1-3600
- **System Impact:** Caps exponential backoff growth to prevent excessively long delays between retries


### reenqueue_delays

**Path:** `common.backoff.reenqueue_delays`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `blocked_by_failures` | `u32` | `120` | Delay in seconds before re-evaluating a task that is blocked due to step failures |
| `enqueuing_steps` | `integer` | `5` |  |
| `evaluating_results` | `integer` | `10` |  |
| `initializing` | `u32` | `10` | Delay in seconds before re-enqueueing a task stuck in the Initializing state |
| `steps_in_process` | `integer` | `15` |  |
| `waiting_for_dependencies` | `u32` | `60` | Delay in seconds before re-checking a task that is waiting for upstream step dependencies |
| `waiting_for_retry` | `integer` | `30` |  |


#### `common.backoff.reenqueue_delays.blocked_by_failures`

Delay in seconds before re-evaluating a task that is blocked due to step failures

- **Type:** `u32`
- **Default:** `120`
- **Valid Range:** 0-3600
- **System Impact:** Gives operators time to investigate before the system retries; longer values prevent retry storms


#### `common.backoff.reenqueue_delays.initializing`

Delay in seconds before re-enqueueing a task stuck in the Initializing state

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 0-300
- **System Impact:** Controls how quickly the system retries task initialization after a transient failure


#### `common.backoff.reenqueue_delays.waiting_for_dependencies`

Delay in seconds before re-checking a task that is waiting for upstream step dependencies

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 0-3600
- **System Impact:** Longer delays reduce polling overhead for tasks with slow dependencies; shorter delays improve responsiveness


## cache

**Path:** `common.cache`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `analytics_ttl_seconds` | `integer` | `60` |  |
| `backend` | `String` | `"redis"` | Cache backend implementation: 'redis' (distributed) or 'moka' (in-process) |
| `default_ttl_seconds` | `u32` | `3600` | Default time-to-live in seconds for cached entries |
| `enabled` | `bool` | `false` | Enable the distributed cache layer for template and analytics data |
| `key_prefix` | `String` | `"tasker"` | Prefix applied to all cache keys to namespace entries |
| `template_ttl_seconds` | `integer` | `3600` |  |


#### `common.cache.backend`

Cache backend implementation: 'redis' (distributed) or 'moka' (in-process)

- **Type:** `String`
- **Default:** `"redis"`
- **Valid Range:** redis | moka
- **System Impact:** Redis is required for multi-instance deployments to avoid stale data; moka is suitable for single-instance or DoS protection


#### `common.cache.default_ttl_seconds`

Default time-to-live in seconds for cached entries

- **Type:** `u32`
- **Default:** `3600`
- **Valid Range:** 1-86400
- **System Impact:** Controls how long cached data remains valid before being re-fetched from the database


#### `common.cache.enabled`

Enable the distributed cache layer for template and analytics data

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When false, all cache reads fall through to direct database queries; no cache dependency required


#### `common.cache.key_prefix`

Prefix applied to all cache keys to namespace entries

- **Type:** `String`
- **Default:** `"tasker"`
- **Valid Range:** non-empty string
- **System Impact:** Prevents key collisions when multiple Tasker instances or other applications share the same cache backend


### moka

**Path:** `common.cache.moka`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_capacity` | `u64` | `10000` | Maximum number of entries the in-process Moka cache can hold |


#### `common.cache.moka.max_capacity`

Maximum number of entries the in-process Moka cache can hold

- **Type:** `u64`
- **Default:** `10000`
- **Valid Range:** 1-1000000
- **System Impact:** Bounds memory usage; least-recently-used entries are evicted when capacity is reached


### redis

**Path:** `common.cache.redis`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `connection_timeout_seconds` | `integer` | `5` |  |
| `database` | `u32` | `0` | Redis database number (0-15) |
| `max_connections` | `integer` | `10` |  |
| `url` | `String` | `"${REDIS_URL:-redis://localhost:6379}"` | Redis connection URL |


#### `common.cache.redis.database`

Redis database number (0-15)

- **Type:** `u32`
- **Default:** `0`
- **Valid Range:** 0-15
- **System Impact:** Isolates Tasker cache keys from other applications sharing the same Redis instance


#### `common.cache.redis.url`

Redis connection URL

- **Type:** `String`
- **Default:** `"${REDIS_URL:-redis://localhost:6379}"`
- **Valid Range:** valid Redis URI
- **System Impact:** Must be reachable when cache is enabled with redis backend


## circuit_breakers

**Path:** `common.circuit_breakers`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Master switch for the circuit breaker subsystem |


#### `common.circuit_breakers.enabled`

Master switch for the circuit breaker subsystem

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, all circuit breakers are disabled and operations always proceed; use only for debugging


### component_configs

**Path:** `common.circuit_breakers.component_configs`

#### cache

**Path:** `common.circuit_breakers.component_configs.cache`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `integer` | `5` |  |
| `success_threshold` | `integer` | `2` |  |
| `timeout_seconds` | `integer` | `15` |  |


#### pgmq

**Path:** `common.circuit_breakers.component_configs.pgmq`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `integer` | `5` |  |
| `success_threshold` | `integer` | `2` |  |
| `timeout_seconds` | `integer` | `30` |  |


#### task_readiness

**Path:** `common.circuit_breakers.component_configs.task_readiness`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `integer` | `10` |  |
| `success_threshold` | `integer` | `3` |  |
| `timeout_seconds` | `integer` | `60` |  |


### default_config

**Path:** `common.circuit_breakers.default_config`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `u32` | `5` | Number of consecutive failures before a circuit breaker trips to the Open state |
| `success_threshold` | `u32` | `2` | Number of consecutive successes in Half-Open state required to close the circuit breaker |
| `timeout_seconds` | `u32` | `30` | Time the circuit breaker remains in Open state before transitioning to Half-Open for a probe |


#### `common.circuit_breakers.default_config.failure_threshold`

Number of consecutive failures before a circuit breaker trips to the Open state

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-100
- **System Impact:** Lower values make the breaker more sensitive; higher values tolerate more transient failures before tripping


#### `common.circuit_breakers.default_config.success_threshold`

Number of consecutive successes in Half-Open state required to close the circuit breaker

- **Type:** `u32`
- **Default:** `2`
- **Valid Range:** 1-100
- **System Impact:** Higher values require more proof of recovery before restoring full traffic


#### `common.circuit_breakers.default_config.timeout_seconds`

Time the circuit breaker remains in Open state before transitioning to Half-Open for a probe

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-300
- **System Impact:** Controls how long the system waits before retesting a failed dependency


### global_settings

**Path:** `common.circuit_breakers.global_settings`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_circuit_breakers` | `u32` | `50` | Maximum number of circuit breaker instances that can be registered |
| `metrics_collection_interval_seconds` | `integer` | `30` |  |
| `min_state_transition_interval_seconds` | `float` | `5.0` |  |


#### `common.circuit_breakers.global_settings.max_circuit_breakers`

Maximum number of circuit breaker instances that can be registered

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-1000
- **System Impact:** Safety limit to prevent unbounded circuit breaker allocation; increase only if adding many component-specific breakers


## database

**Path:** `common.database`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `database` | `String` | `"tasker_development"` | Database name used for display and connection verification logging |
| `skip_migration_check` | `bool` | `false` | Skip database migration version check at startup |
| `url` | `String` | `"${DATABASE_URL:-postgresql://localhost/tasker}"` | PostgreSQL connection URL for the primary database |


#### `common.database.database`

Database name used for display and connection verification logging

- **Type:** `String`
- **Default:** `"tasker_development"`
- **Valid Range:** valid PostgreSQL database name
- **System Impact:** Informational; the actual database is determined by the connection URL


#### `common.database.skip_migration_check`

Skip database migration version check at startup

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When true, the system will not verify that migrations are current; use only for testing or when migrations are managed externally


#### `common.database.url`

PostgreSQL connection URL for the primary database

- **Type:** `String`
- **Default:** `"${DATABASE_URL:-postgresql://localhost/tasker}"`
- **Valid Range:** valid PostgreSQL connection URI
- **System Impact:** All task, step, and workflow state is stored here; must be reachable at startup

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
| development | postgresql://localhost/tasker | Local default, no auth |
| production | ${DATABASE_URL} | Always use env var injection for secrets rotation |
| test | postgresql://tasker:tasker@localhost:5432/tasker_rust_test | Isolated test database with known credentials |

**Related:** `common.database.pool.max_connections`, `common.pgmq_database.url`


### pool

**Path:** `common.database.pool`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `acquire_timeout_seconds` | `u32` | `10` | Maximum time to wait when acquiring a connection from the pool |
| `idle_timeout_seconds` | `u32` | `300` | Time before an idle connection is closed and removed from the pool |
| `max_connections` | `u32` | `25` | Maximum number of concurrent database connections in the pool |
| `max_lifetime_seconds` | `u32` | `1800` | Maximum total lifetime of a connection before it is closed and replaced |
| `min_connections` | `u32` | `5` | Minimum number of idle connections maintained in the pool |
| `slow_acquire_threshold_ms` | `u32` | `100` | Threshold in milliseconds above which connection acquisition is logged as slow |


#### `common.database.pool.acquire_timeout_seconds`

Maximum time to wait when acquiring a connection from the pool

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-300
- **System Impact:** Queries fail with a timeout error if no connection is available within this window


#### `common.database.pool.idle_timeout_seconds`

Time before an idle connection is closed and removed from the pool

- **Type:** `u32`
- **Default:** `300`
- **Valid Range:** 1-3600
- **System Impact:** Controls how quickly the pool shrinks back to min_connections after load drops


#### `common.database.pool.max_connections`

Maximum number of concurrent database connections in the pool

- **Type:** `u32`
- **Default:** `25`
- **Valid Range:** 1-1000
- **System Impact:** Controls database connection concurrency; too few causes query queuing under load, too many risks DB resource exhaustion

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
| development | 10-25 | Small pool for local development |
| test | 10-30 | Moderate pool; cluster tests may run 10 services sharing the same DB |
| production | 30-50 | Scale based on worker count and concurrent task volume |

**Related:** `common.database.pool.min_connections`, `common.database.pool.acquire_timeout_seconds`


#### `common.database.pool.max_lifetime_seconds`

Maximum total lifetime of a connection before it is closed and replaced

- **Type:** `u32`
- **Default:** `1800`
- **Valid Range:** 60-86400
- **System Impact:** Prevents connection drift from server-side config changes or memory leaks in long-lived connections


#### `common.database.pool.min_connections`

Minimum number of idle connections maintained in the pool

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 0-100
- **System Impact:** Keeps connections warm to avoid cold-start latency on first queries after idle periods


#### `common.database.pool.slow_acquire_threshold_ms`

Threshold in milliseconds above which connection acquisition is logged as slow

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 10-60000
- **System Impact:** Observability: slow acquire warnings indicate pool pressure or network issues


### variables

**Path:** `common.database.variables`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `statement_timeout` | `u32` | `30000` | PostgreSQL statement_timeout in milliseconds, set per-connection via SET statement_timeout |


#### `common.database.variables.statement_timeout`

PostgreSQL statement_timeout in milliseconds, set per-connection via SET statement_timeout

- **Type:** `u32`
- **Default:** `30000`
- **Valid Range:** 100-600000
- **System Impact:** Prevents runaway queries from holding connections indefinitely; queries exceeding this limit are cancelled by PostgreSQL


## execution

**Path:** `common.execution`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `connection_timeout_seconds` | `integer` | `30` |  |
| `default_timeout_seconds` | `u32` | `3600` | Default maximum wall-clock time for an entire task to complete |
| `environment` | `String` | `"development"` | Runtime environment identifier used for configuration context selection and logging |
| `max_concurrent_steps` | `u32` | `1000` | Maximum number of steps that can be executing simultaneously across all tasks |
| `max_concurrent_tasks` | `u32` | `100` | Maximum number of tasks that can be actively processed simultaneously |
| `max_discovery_attempts` | `integer` | `5` |  |
| `max_retries` | `u32` | `3` | Default maximum number of retry attempts for a failed step |
| `max_workflow_steps` | `integer` | `10000` |  |
| `step_batch_size` | `u32` | `50` | Number of steps to enqueue in a single batch during task initialization |
| `step_execution_timeout_seconds` | `u32` | `600` | Default maximum time for a single step execution before it is considered timed out |


#### `common.execution.default_timeout_seconds`

Default maximum wall-clock time for an entire task to complete

- **Type:** `u32`
- **Default:** `3600`
- **Valid Range:** 1-86400
- **System Impact:** Tasks exceeding this timeout are transitioned to error state; individual step timeouts are separate


#### `common.execution.environment`

Runtime environment identifier used for configuration context selection and logging

- **Type:** `String`
- **Default:** `"development"`
- **Valid Range:** test | development | production
- **System Impact:** Affects log levels, default tuning, and environment-specific behavior throughout the system


#### `common.execution.max_concurrent_steps`

Maximum number of steps that can be executing simultaneously across all tasks

- **Type:** `u32`
- **Default:** `1000`
- **Valid Range:** 1-1000000
- **System Impact:** Bounds total worker-side parallelism; should be larger than max_concurrent_tasks since tasks have multiple steps


#### `common.execution.max_concurrent_tasks`

Maximum number of tasks that can be actively processed simultaneously

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 1-100000
- **System Impact:** Primary concurrency control; limits how many task state machines are active at once


#### `common.execution.max_retries`

Default maximum number of retry attempts for a failed step

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Applies when step definitions do not specify their own retry count; 0 means no retries


#### `common.execution.step_batch_size`

Number of steps to enqueue in a single batch during task initialization

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-1000
- **System Impact:** Controls step enqueueing throughput; larger batches reduce round trips but increase per-batch latency


#### `common.execution.step_execution_timeout_seconds`

Default maximum time for a single step execution before it is considered timed out

- **Type:** `u32`
- **Default:** `600`
- **Valid Range:** 1-3600
- **System Impact:** Guards against hung step handlers; overridden by step-level timeout configuration if present


## mpsc_channels

**Path:** `common.mpsc_channels`

### event_publisher

**Path:** `common.mpsc_channels.event_publisher`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `event_queue_buffer_size` | `integer` | `5000` |  |


### ffi

**Path:** `common.mpsc_channels.ffi`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ruby_event_buffer_size` | `integer` | `1000` |  |


### overflow_policy

**Path:** `common.mpsc_channels.overflow_policy`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `drop_policy` | `String` | `"block"` |  |
| `log_warning_threshold` | `float` | `0.8` |  |


#### metrics

**Path:** `common.mpsc_channels.overflow_policy.metrics`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` |  |
| `saturation_check_interval_seconds` | `integer` | `30` |  |


## pgmq_database

**Path:** `common.pgmq_database`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Enable PGMQ messaging subsystem |
| `skip_migration_check` | `bool` | `false` |  |
| `url` | `String` | `"${PGMQ_DATABASE_URL:-}"` | PostgreSQL connection URL for a dedicated PGMQ database; when empty, PGMQ shares the primary database |


#### `common.pgmq_database.enabled`

Enable PGMQ messaging subsystem

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, PGMQ queue operations are disabled; only useful if using RabbitMQ as the sole messaging backend


#### `common.pgmq_database.url`

PostgreSQL connection URL for a dedicated PGMQ database; when empty, PGMQ shares the primary database

- **Type:** `String`
- **Default:** `"${PGMQ_DATABASE_URL:-}"`
- **Valid Range:** valid PostgreSQL connection URI or empty string
- **System Impact:** Separating PGMQ to its own database isolates messaging I/O from task state queries, reducing contention under heavy load

**Related:** `common.database.url`, `common.pgmq_database.enabled`


### pool

**Path:** `common.pgmq_database.pool`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `acquire_timeout_seconds` | `integer` | `5` |  |
| `idle_timeout_seconds` | `integer` | `300` |  |
| `max_connections` | `integer` | `15` |  |
| `max_lifetime_seconds` | `integer` | `1800` |  |
| `min_connections` | `integer` | `3` |  |
| `slow_acquire_threshold_ms` | `integer` | `100` |  |


## queues

**Path:** `common.queues`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `backend` | `String` | `"${TASKER_MESSAGING_BACKEND:-pgmq}"` | Messaging backend: 'pgmq' (PostgreSQL-based, LISTEN/NOTIFY) or 'rabbitmq' (AMQP broker) |
| `default_batch_size` | `u32` | `10` | Default number of messages to dequeue in a single batch read |
| `default_visibility_timeout_seconds` | `u32` | `30` | Default time a dequeued message remains invisible to other consumers |
| `health_check_interval` | `u32` | `60` | Interval in seconds between queue health check probes |
| `max_batch_size` | `u32` | `100` | Hard upper limit on batch size for any single dequeue operation |
| `naming_pattern` | `String` | `"{namespace}_{name}_queue"` | Template pattern for constructing queue names from namespace and name |
| `orchestration_namespace` | `String` | `"orchestration"` | Namespace prefix for orchestration queue names |
| `worker_namespace` | `String` | `"worker"` | Namespace prefix for worker queue names |


#### `common.queues.backend`

Messaging backend: 'pgmq' (PostgreSQL-based, LISTEN/NOTIFY) or 'rabbitmq' (AMQP broker)

- **Type:** `String`
- **Default:** `"${TASKER_MESSAGING_BACKEND:-pgmq}"`
- **Valid Range:** pgmq | rabbitmq
- **System Impact:** Determines the entire message transport layer; pgmq requires only PostgreSQL, rabbitmq requires a separate AMQP broker

**Environment Recommendations:**

| Environment | Value | Rationale |
|-------------|-------|-----------|
| production | pgmq or rabbitmq | pgmq for simplicity, rabbitmq for high-throughput push semantics |
| test | pgmq | Single-dependency setup, simpler CI |

**Related:** `common.queues.pgmq`, `common.queues.rabbitmq`


#### `common.queues.default_batch_size`

Default number of messages to dequeue in a single batch read

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-1000
- **System Impact:** Larger batches improve throughput but increase per-batch processing latency


#### `common.queues.default_visibility_timeout_seconds`

Default time a dequeued message remains invisible to other consumers

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** If a consumer fails to process a message within this window, the message becomes visible again for retry


#### `common.queues.health_check_interval`

Interval in seconds between queue health check probes

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 1-3600
- **System Impact:** Controls how frequently queue connectivity and depth are checked for health reporting


#### `common.queues.max_batch_size`

Hard upper limit on batch size for any single dequeue operation

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 1-10000
- **System Impact:** Safety cap to prevent a single consumer from monopolizing queue messages


#### `common.queues.naming_pattern`

Template pattern for constructing queue names from namespace and name

- **Type:** `String`
- **Default:** `"{namespace}_{name}_queue"`
- **Valid Range:** string containing {namespace} and {name} placeholders
- **System Impact:** Determines the actual PGMQ/RabbitMQ queue names; changing this after deployment requires manual queue migration


#### `common.queues.orchestration_namespace`

Namespace prefix for orchestration queue names

- **Type:** `String`
- **Default:** `"orchestration"`
- **Valid Range:** non-empty string
- **System Impact:** Used in queue naming pattern to isolate orchestration queues from worker queues


#### `common.queues.worker_namespace`

Namespace prefix for worker queue names

- **Type:** `String`
- **Default:** `"worker"`
- **Valid Range:** non-empty string
- **System Impact:** Used in queue naming pattern to isolate worker queues from orchestration queues


### orchestration_queues

**Path:** `common.queues.orchestration_queues`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `step_results` | `String` | `"orchestration_step_results"` | Queue name for step execution results returned by workers |
| `task_finalizations` | `String` | `"orchestration_task_finalizations"` | Queue name for task finalization messages |
| `task_requests` | `String` | `"orchestration_task_requests"` | Queue name for incoming task execution requests |


#### `common.queues.orchestration_queues.step_results`

Queue name for step execution results returned by workers

- **Type:** `String`
- **Default:** `"orchestration_step_results"`
- **Valid Range:** valid queue name
- **System Impact:** Workers publish step completion results here for the orchestration result processor


#### `common.queues.orchestration_queues.task_finalizations`

Queue name for task finalization messages

- **Type:** `String`
- **Default:** `"orchestration_task_finalizations"`
- **Valid Range:** valid queue name
- **System Impact:** Tasks ready for completion evaluation are enqueued here


#### `common.queues.orchestration_queues.task_requests`

Queue name for incoming task execution requests

- **Type:** `String`
- **Default:** `"orchestration_task_requests"`
- **Valid Range:** valid queue name
- **System Impact:** The orchestration system reads new task requests from this queue


### pgmq

**Path:** `common.queues.pgmq`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_retries` | `u32` | `3` | Maximum number of times a failed PGMQ operation is retried before giving up |
| `poll_interval_ms` | `u32` | `500` | Interval in milliseconds between PGMQ polling cycles when no LISTEN/NOTIFY events arrive |
| `shutdown_timeout_seconds` | `u32` | `10` | Maximum time to wait for in-flight PGMQ operations to complete during graceful shutdown |


#### `common.queues.pgmq.max_retries`

Maximum number of times a failed PGMQ operation is retried before giving up

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Controls retry behavior for transient PGMQ failures such as connection resets


#### `common.queues.pgmq.poll_interval_ms`

Interval in milliseconds between PGMQ polling cycles when no LISTEN/NOTIFY events arrive

- **Type:** `u32`
- **Default:** `500`
- **Valid Range:** 10-10000
- **System Impact:** Lower values reduce message latency in polling mode but increase database load; in Hybrid mode this is the fallback interval


#### `common.queues.pgmq.shutdown_timeout_seconds`

Maximum time to wait for in-flight PGMQ operations to complete during graceful shutdown

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-300
- **System Impact:** Prevents shutdown from hanging indefinitely on stuck queue operations


#### queue_depth_thresholds

**Path:** `common.queues.pgmq.queue_depth_thresholds`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `critical_threshold` | `i64` | `5000` | Queue depth at which the API returns HTTP 503 Service Unavailable for new task submissions |
| `overflow_threshold` | `i64` | `10000` | Queue depth indicating an emergency condition requiring manual intervention |
| `warning_threshold` | `i64` | `1000` | Queue depth at which warning-level log messages are emitted |


#### `common.queues.pgmq.queue_depth_thresholds.critical_threshold`

Queue depth at which the API returns HTTP 503 Service Unavailable for new task submissions

- **Type:** `i64`
- **Default:** `5000`
- **Valid Range:** 1+
- **System Impact:** Backpressure mechanism: rejects new work to allow the system to drain existing messages


#### `common.queues.pgmq.queue_depth_thresholds.overflow_threshold`

Queue depth indicating an emergency condition requiring manual intervention

- **Type:** `i64`
- **Default:** `10000`
- **Valid Range:** 1+
- **System Impact:** Highest severity threshold; triggers error-level logging and metrics for operational alerting


#### `common.queues.pgmq.queue_depth_thresholds.warning_threshold`

Queue depth at which warning-level log messages are emitted

- **Type:** `i64`
- **Default:** `1000`
- **Valid Range:** 1+
- **System Impact:** Observability threshold only; no behavioral change, but alerts operators to growing backlog


### rabbitmq

**Path:** `common.queues.rabbitmq`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `connection_timeout_seconds` | `u32` | `10` | Maximum time to wait when establishing a new RabbitMQ connection |
| `heartbeat_seconds` | `u16` | `30` | AMQP heartbeat interval for connection liveness detection |
| `prefetch_count` | `u16` | `100` | Number of unacknowledged messages RabbitMQ will deliver before waiting for acks |
| `url` | `String` | `"${RABBITMQ_URL:-amqp://guest:guest@localhost:5672/%2F}"` | AMQP connection URL for RabbitMQ; %2F is the URL-encoded default vhost '/' |


#### `common.queues.rabbitmq.connection_timeout_seconds`

Maximum time to wait when establishing a new RabbitMQ connection

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-300
- **System Impact:** Connections that cannot be established within this timeout fail with an error


#### `common.queues.rabbitmq.heartbeat_seconds`

AMQP heartbeat interval for connection liveness detection

- **Type:** `u16`
- **Default:** `30`
- **Valid Range:** 0-3600
- **System Impact:** Detects dead connections; 0 disables heartbeats (not recommended in production)


#### `common.queues.rabbitmq.prefetch_count`

Number of unacknowledged messages RabbitMQ will deliver before waiting for acks

- **Type:** `u16`
- **Default:** `100`
- **Valid Range:** 1-65535
- **System Impact:** Controls consumer throughput vs. memory usage; higher values increase throughput but buffer more messages in-process


#### `common.queues.rabbitmq.url`

AMQP connection URL for RabbitMQ; %2F is the URL-encoded default vhost '/'

- **Type:** `String`
- **Default:** `"${RABBITMQ_URL:-amqp://guest:guest@localhost:5672/%2F}"`
- **Valid Range:** valid AMQP URI
- **System Impact:** Only used when queues.backend = 'rabbitmq'; must be reachable at startup


## system

**Path:** `common.system`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `default_dependent_system` | `String` | `"default"` | Default system name assigned to tasks that do not specify a dependent system |
| `max_recursion_depth` | `u32` | `50` | Maximum depth for recursive dependency resolution in workflow graphs |
| `version` | `String` | `"0.1.0"` | Tasker configuration schema version |


#### `common.system.default_dependent_system`

Default system name assigned to tasks that do not specify a dependent system

- **Type:** `String`
- **Default:** `"default"`
- **Valid Range:** non-empty string
- **System Impact:** Groups tasks for routing and reporting; most single-system deployments can leave this as default


#### `common.system.max_recursion_depth`

Maximum depth for recursive dependency resolution in workflow graphs

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-1000
- **System Impact:** Prevents infinite loops from circular dependencies; raise only for deeply nested workflows


#### `common.system.version`

Tasker configuration schema version

- **Type:** `String`
- **Default:** `"0.1.0"`
- **Valid Range:** semver
- **System Impact:** Used for configuration compatibility checks during upgrades


## task_templates

**Path:** `common.task_templates`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `search_paths` | `array` | `["config/tasks/**/*.{yml,yaml}"]` |  |


## telemetry

**Path:** `common.telemetry`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` |  |
| `sample_rate` | `float` | `0.1` |  |
| `service_name` | `String` | `"tasker-core"` |  |



---

*Generated by `tasker-cli docs` — [Tasker Configuration System](https://github.com/tasker-systems/tasker-core)*
# Configuration Reference: common



> 63/102 parameters documented
> Generated: 2026-01-31T02:17:40.976829977+00:00

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
| test | postgresql://tasker:tasker@localhost:5432/tasker_rust_test | Isolated test database with known credentials |
| development | postgresql://localhost/tasker | Local default, no auth |
| production | ${DATABASE_URL} | Always use env var injection for secrets rotation |

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
| production | 30-50 | Scale based on worker count and concurrent task volume |
| test | 10-30 | Moderate pool; cluster tests may run 10 services sharing the same DB |

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
---

# Configuration Reference: orchestration



> 23/100 parameters documented
> Generated: 2026-01-31T02:17:40.976829977+00:00

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
| test | 0.0.0.0:8080 | Default port for test fixtures |
| production | 0.0.0.0:8080 | Standard port; use TASKER_WEB_BIND_ADDRESS env var to override in CI |


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
---

# Configuration Reference: worker



> 19/101 parameters documented
> Generated: 2026-01-31T02:17:40.976829977+00:00

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
| production | http://orchestration:8080 | Container-internal DNS in Kubernetes/Docker |
| test | http://localhost:8080 | Local orchestration for testing |

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
| test | 0.0.0.0:8081 | Default port offset from orchestration (8080) |
| production | 0.0.0.0:8081 | Standard worker port; use TASKER_WEB_BIND_ADDRESS env var to override |


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
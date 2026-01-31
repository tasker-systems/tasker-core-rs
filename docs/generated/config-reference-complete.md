# Configuration Reference: common



> 102/102 parameters documented
> Generated: 2026-01-31T02:40:18.393404960+00:00

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
| `enqueuing_steps` | `u32` | `5` | Delay in seconds before re-enqueueing a task stuck in the EnqueuingSteps state |
| `evaluating_results` | `u32` | `10` | Delay in seconds before re-evaluating a task stuck in the EvaluatingResults state |
| `initializing` | `u32` | `10` | Delay in seconds before re-enqueueing a task stuck in the Initializing state |
| `steps_in_process` | `u32` | `15` | Delay in seconds before re-checking a task whose steps are still being processed |
| `waiting_for_dependencies` | `u32` | `60` | Delay in seconds before re-checking a task that is waiting for upstream step dependencies |
| `waiting_for_retry` | `u32` | `30` | Delay in seconds before re-processing a task that is waiting for step retries |


#### `common.backoff.reenqueue_delays.blocked_by_failures`

Delay in seconds before re-evaluating a task that is blocked due to step failures

- **Type:** `u32`
- **Default:** `120`
- **Valid Range:** 0-3600
- **System Impact:** Gives operators time to investigate before the system retries; longer values prevent retry storms


#### `common.backoff.reenqueue_delays.enqueuing_steps`

Delay in seconds before re-enqueueing a task stuck in the EnqueuingSteps state

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 0-300
- **System Impact:** Short delay (5s) since step enqueueing failures are usually transient


#### `common.backoff.reenqueue_delays.evaluating_results`

Delay in seconds before re-evaluating a task stuck in the EvaluatingResults state

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 0-300
- **System Impact:** Allows the result processor time to handle pending step results before re-evaluation


#### `common.backoff.reenqueue_delays.initializing`

Delay in seconds before re-enqueueing a task stuck in the Initializing state

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 0-300
- **System Impact:** Controls how quickly the system retries task initialization after a transient failure


#### `common.backoff.reenqueue_delays.steps_in_process`

Delay in seconds before re-checking a task whose steps are still being processed

- **Type:** `u32`
- **Default:** `15`
- **Valid Range:** 0-300
- **System Impact:** Allows workers time to complete in-flight steps before the orchestrator re-evaluates the task


#### `common.backoff.reenqueue_delays.waiting_for_dependencies`

Delay in seconds before re-checking a task that is waiting for upstream step dependencies

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 0-3600
- **System Impact:** Longer delays reduce polling overhead for tasks with slow dependencies; shorter delays improve responsiveness


#### `common.backoff.reenqueue_delays.waiting_for_retry`

Delay in seconds before re-processing a task that is waiting for step retries

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 0-3600
- **System Impact:** Should be long enough for the backoff delay on the failing steps to expire before re-checking


## cache

**Path:** `common.cache`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `analytics_ttl_seconds` | `u32` | `60` | Time-to-live in seconds for cached analytics and metrics data |
| `backend` | `String` | `"redis"` | Cache backend implementation: 'redis' (distributed) or 'moka' (in-process) |
| `default_ttl_seconds` | `u32` | `3600` | Default time-to-live in seconds for cached entries |
| `enabled` | `bool` | `false` | Enable the distributed cache layer for template and analytics data |
| `key_prefix` | `String` | `"tasker"` | Prefix applied to all cache keys to namespace entries |
| `template_ttl_seconds` | `u32` | `3600` | Time-to-live in seconds for cached task template definitions |


#### `common.cache.analytics_ttl_seconds`

Time-to-live in seconds for cached analytics and metrics data

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 1-3600
- **System Impact:** Analytics data is write-heavy and changes frequently; short TTL (60s) keeps metrics current


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


#### `common.cache.template_ttl_seconds`

Time-to-live in seconds for cached task template definitions

- **Type:** `u32`
- **Default:** `3600`
- **Valid Range:** 1-86400
- **System Impact:** Template changes take up to this long to propagate; shorter values increase DB load, longer values improve performance


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
| `connection_timeout_seconds` | `u32` | `5` | Maximum time to wait when establishing a new Redis connection |
| `database` | `u32` | `0` | Redis database number (0-15) |
| `max_connections` | `u32` | `10` | Maximum number of connections in the Redis connection pool |
| `url` | `String` | `"${REDIS_URL:-redis://localhost:6379}"` | Redis connection URL |


#### `common.cache.redis.connection_timeout_seconds`

Maximum time to wait when establishing a new Redis connection

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-60
- **System Impact:** Connections that cannot be established within this timeout fail; cache falls back to database


#### `common.cache.redis.database`

Redis database number (0-15)

- **Type:** `u32`
- **Default:** `0`
- **Valid Range:** 0-15
- **System Impact:** Isolates Tasker cache keys from other applications sharing the same Redis instance


#### `common.cache.redis.max_connections`

Maximum number of connections in the Redis connection pool

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-500
- **System Impact:** Bounds concurrent Redis operations; increase for high cache throughput workloads


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
| `failure_threshold` | `u32` | `5` | Failures before the cache circuit breaker trips to Open |
| `success_threshold` | `u32` | `2` | Successes in Half-Open required to close the cache breaker |
| `timeout_seconds` | `u32` | `15` | Time the cache breaker stays Open before probing |


#### `common.circuit_breakers.component_configs.cache.failure_threshold`

Failures before the cache circuit breaker trips to Open

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-100
- **System Impact:** Protects Redis/Dragonfly operations; when tripped, cache reads fall through to database


#### `common.circuit_breakers.component_configs.cache.success_threshold`

Successes in Half-Open required to close the cache breaker

- **Type:** `u32`
- **Default:** `2`
- **Valid Range:** 1-100
- **System Impact:** Low threshold (2) for fast recovery since cache failures gracefully degrade to database


#### `common.circuit_breakers.component_configs.cache.timeout_seconds`

Time the cache breaker stays Open before probing

- **Type:** `u32`
- **Default:** `15`
- **Valid Range:** 1-300
- **System Impact:** Shorter than default (15s) because cache is non-critical and can recover quickly


#### pgmq

**Path:** `common.circuit_breakers.component_configs.pgmq`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `u32` | `5` | Failures before the PGMQ circuit breaker trips to Open |
| `success_threshold` | `u32` | `2` | Successes in Half-Open required to close the PGMQ breaker |
| `timeout_seconds` | `u32` | `30` | Time the PGMQ breaker stays Open before probing |


#### `common.circuit_breakers.component_configs.pgmq.failure_threshold`

Failures before the PGMQ circuit breaker trips to Open

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-100
- **System Impact:** Protects the messaging layer; when tripped, queue operations are short-circuited


#### `common.circuit_breakers.component_configs.pgmq.success_threshold`

Successes in Half-Open required to close the PGMQ breaker

- **Type:** `u32`
- **Default:** `2`
- **Valid Range:** 1-100
- **System Impact:** Lower threshold (2) allows faster recovery since PGMQ failures are typically transient


#### `common.circuit_breakers.component_configs.pgmq.timeout_seconds`

Time the PGMQ breaker stays Open before probing

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-300
- **System Impact:** Controls recovery time after PGMQ failures; matches the default config


#### task_readiness

**Path:** `common.circuit_breakers.component_configs.task_readiness`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `u32` | `10` | Failures before the task readiness circuit breaker trips to Open |
| `success_threshold` | `u32` | `3` | Successes in Half-Open required to close the task readiness breaker |
| `timeout_seconds` | `u32` | `60` | Time the task readiness breaker stays Open before probing |


#### `common.circuit_breakers.component_configs.task_readiness.failure_threshold`

Failures before the task readiness circuit breaker trips to Open

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-100
- **System Impact:** Higher than default (10 vs 5) because task readiness queries are frequent and transient failures are expected


#### `common.circuit_breakers.component_configs.task_readiness.success_threshold`

Successes in Half-Open required to close the task readiness breaker

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 1-100
- **System Impact:** Slightly higher than default (3) for extra confidence before resuming readiness queries


#### `common.circuit_breakers.component_configs.task_readiness.timeout_seconds`

Time the task readiness breaker stays Open before probing

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 1-300
- **System Impact:** Longer than default (60s) to give the database time to recover from readiness query pressure


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
| `metrics_collection_interval_seconds` | `u32` | `30` | Interval in seconds between circuit breaker metrics collection sweeps |
| `min_state_transition_interval_seconds` | `f64` | `5.0` | Minimum time in seconds between circuit breaker state transitions |


#### `common.circuit_breakers.global_settings.max_circuit_breakers`

Maximum number of circuit breaker instances that can be registered

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-1000
- **System Impact:** Safety limit to prevent unbounded circuit breaker allocation; increase only if adding many component-specific breakers


#### `common.circuit_breakers.global_settings.metrics_collection_interval_seconds`

Interval in seconds between circuit breaker metrics collection sweeps

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Controls how frequently circuit breaker state, failure counts, and transition counts are collected for observability


#### `common.circuit_breakers.global_settings.min_state_transition_interval_seconds`

Minimum time in seconds between circuit breaker state transitions

- **Type:** `f64`
- **Default:** `5.0`
- **Valid Range:** 0.0-60.0
- **System Impact:** Prevents rapid oscillation between Open and Closed states during intermittent failures


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
| production | ${DATABASE_URL} | Always use env var injection for secrets rotation |
| test | postgresql://tasker:tasker@localhost:5432/tasker_rust_test | Isolated test database with known credentials |
| development | postgresql://localhost/tasker | Local default, no auth |

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
| production | 30-50 | Scale based on worker count and concurrent task volume |
| test | 10-30 | Moderate pool; cluster tests may run 10 services sharing the same DB |
| development | 10-25 | Small pool for local development |

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
| `connection_timeout_seconds` | `u32` | `30` | Default timeout in seconds for establishing external connections during step execution |
| `default_timeout_seconds` | `u32` | `3600` | Default maximum wall-clock time for an entire task to complete |
| `environment` | `String` | `"development"` | Runtime environment identifier used for configuration context selection and logging |
| `max_concurrent_steps` | `u32` | `1000` | Maximum number of steps that can be executing simultaneously across all tasks |
| `max_concurrent_tasks` | `u32` | `100` | Maximum number of tasks that can be actively processed simultaneously |
| `max_discovery_attempts` | `u32` | `5` | Maximum number of attempts to discover step handlers during task initialization |
| `max_retries` | `u32` | `3` | Default maximum number of retry attempts for a failed step |
| `max_workflow_steps` | `u32` | `10000` | Maximum number of steps allowed in a single workflow definition |
| `step_batch_size` | `u32` | `50` | Number of steps to enqueue in a single batch during task initialization |
| `step_execution_timeout_seconds` | `u32` | `600` | Default maximum time for a single step execution before it is considered timed out |


#### `common.execution.connection_timeout_seconds`

Default timeout in seconds for establishing external connections during step execution

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-300
- **System Impact:** Applies to outbound connections made by step handlers; prevents hung connections from blocking step completion


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


#### `common.execution.max_discovery_attempts`

Maximum number of attempts to discover step handlers during task initialization

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-50
- **System Impact:** Controls retry behavior when step handler lookup fails; prevents infinite discovery loops


#### `common.execution.max_retries`

Default maximum number of retry attempts for a failed step

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Applies when step definitions do not specify their own retry count; 0 means no retries


#### `common.execution.max_workflow_steps`

Maximum number of steps allowed in a single workflow definition

- **Type:** `u32`
- **Default:** `10000`
- **Valid Range:** 1-100000
- **System Impact:** Safety limit to prevent excessively large workflows from consuming unbounded resources


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
| `event_queue_buffer_size` | `usize` | `5000` | Bounded channel capacity for the event publisher MPSC channel |


#### `common.mpsc_channels.event_publisher.event_queue_buffer_size`

Bounded channel capacity for the event publisher MPSC channel

- **Type:** `usize`
- **Default:** `5000`
- **Valid Range:** 100-100000
- **System Impact:** Controls backpressure for domain event publishing; smaller buffers apply backpressure sooner


### ffi

**Path:** `common.mpsc_channels.ffi`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ruby_event_buffer_size` | `usize` | `1000` | Bounded channel capacity for Ruby FFI event delivery |


#### `common.mpsc_channels.ffi.ruby_event_buffer_size`

Bounded channel capacity for Ruby FFI event delivery

- **Type:** `usize`
- **Default:** `1000`
- **Valid Range:** 100-50000
- **System Impact:** Buffers events between the Rust runtime and Ruby FFI layer; overflow triggers backpressure on the dispatch side


### overflow_policy

**Path:** `common.mpsc_channels.overflow_policy`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `drop_policy` | `String` | `"block"` | Behavior when a bounded MPSC channel is full |
| `log_warning_threshold` | `f64` | `0.8` | Channel saturation fraction at which warning logs are emitted |


#### `common.mpsc_channels.overflow_policy.drop_policy`

Behavior when a bounded MPSC channel is full

- **Type:** `String`
- **Default:** `"block"`
- **Valid Range:** block | drop_oldest
- **System Impact:** 'block' pauses the sender until space is available; 'drop_oldest' discards the oldest message to make room


#### `common.mpsc_channels.overflow_policy.log_warning_threshold`

Channel saturation fraction at which warning logs are emitted

- **Type:** `f64`
- **Default:** `0.8`
- **Valid Range:** 0.0-1.0
- **System Impact:** A value of 0.8 means warnings fire when any channel reaches 80% capacity


#### metrics

**Path:** `common.mpsc_channels.overflow_policy.metrics`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Enable periodic channel saturation metrics collection |
| `saturation_check_interval_seconds` | `u32` | `30` | Interval in seconds between channel saturation metric samples |


#### `common.mpsc_channels.overflow_policy.metrics.enabled`

Enable periodic channel saturation metrics collection

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, channel fill levels are sampled and published as metrics at the configured interval


#### `common.mpsc_channels.overflow_policy.metrics.saturation_check_interval_seconds`

Interval in seconds between channel saturation metric samples

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Lower intervals give finer-grained capacity visibility but add sampling overhead


## pgmq_database

**Path:** `common.pgmq_database`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Enable PGMQ messaging subsystem |
| `skip_migration_check` | `bool` | `false` | Skip PGMQ database migration version check at startup |
| `url` | `String` | `"${PGMQ_DATABASE_URL:-}"` | PostgreSQL connection URL for a dedicated PGMQ database; when empty, PGMQ shares the primary database |


#### `common.pgmq_database.enabled`

Enable PGMQ messaging subsystem

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, PGMQ queue operations are disabled; only useful if using RabbitMQ as the sole messaging backend


#### `common.pgmq_database.skip_migration_check`

Skip PGMQ database migration version check at startup

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** When true, PGMQ migration verification is skipped; use only for testing or externally managed migrations


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
| `acquire_timeout_seconds` | `u32` | `5` | Maximum time to wait when acquiring a connection from the PGMQ pool |
| `idle_timeout_seconds` | `u32` | `300` | Time before an idle PGMQ connection is closed and removed from the pool |
| `max_connections` | `u32` | `15` | Maximum number of concurrent connections in the PGMQ database pool |
| `max_lifetime_seconds` | `u32` | `1800` | Maximum total lifetime of a PGMQ database connection before replacement |
| `min_connections` | `u32` | `3` | Minimum idle connections maintained in the PGMQ database pool |
| `slow_acquire_threshold_ms` | `u32` | `100` | Threshold in milliseconds above which PGMQ pool acquisition is logged as slow |


#### `common.pgmq_database.pool.acquire_timeout_seconds`

Maximum time to wait when acquiring a connection from the PGMQ pool

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-300
- **System Impact:** Queue operations fail with timeout if no PGMQ connection is available within this window


#### `common.pgmq_database.pool.idle_timeout_seconds`

Time before an idle PGMQ connection is closed and removed from the pool

- **Type:** `u32`
- **Default:** `300`
- **Valid Range:** 1-3600
- **System Impact:** Controls how quickly the PGMQ pool shrinks after messaging load drops


#### `common.pgmq_database.pool.max_connections`

Maximum number of concurrent connections in the PGMQ database pool

- **Type:** `u32`
- **Default:** `15`
- **Valid Range:** 1-500
- **System Impact:** Separate from the main database pool; size according to messaging throughput requirements


#### `common.pgmq_database.pool.max_lifetime_seconds`

Maximum total lifetime of a PGMQ database connection before replacement

- **Type:** `u32`
- **Default:** `1800`
- **Valid Range:** 60-86400
- **System Impact:** Prevents connection drift in long-running PGMQ connections


#### `common.pgmq_database.pool.min_connections`

Minimum idle connections maintained in the PGMQ database pool

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Keeps PGMQ connections warm to avoid cold-start latency on queue operations


#### `common.pgmq_database.pool.slow_acquire_threshold_ms`

Threshold in milliseconds above which PGMQ pool acquisition is logged as slow

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 10-60000
- **System Impact:** Observability: slow PGMQ acquire warnings indicate messaging pool pressure


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
| test | pgmq | Single-dependency setup, simpler CI |
| production | pgmq or rabbitmq | pgmq for simplicity, rabbitmq for high-throughput push semantics |

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
| `search_paths` | `Vec<String>` | `["config/tasks/**/*.{yml,yaml}"]` | Glob patterns for discovering task template YAML files |


#### `common.task_templates.search_paths`

Glob patterns for discovering task template YAML files

- **Type:** `Vec<String>`
- **Default:** `["config/tasks/**/*.{yml,yaml}"]`
- **Valid Range:** valid glob patterns
- **System Impact:** Templates matching these patterns are loaded at startup for task definition discovery


## telemetry

**Path:** `common.telemetry`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Enable OpenTelemetry-based telemetry collection (currently unused; see comment above) |
| `sample_rate` | `f64` | `0.1` | Fraction of traces to sample (currently unused; see comment above) |
| `service_name` | `String` | `"tasker-core"` | Service name reported in telemetry spans and metrics (currently unused; see comment above) |


#### `common.telemetry.enabled`

Enable OpenTelemetry-based telemetry collection (currently unused; see comment above)

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Reserved for future TOML-driven telemetry init; currently telemetry is controlled via TELEMETRY_ENABLED env var


#### `common.telemetry.sample_rate`

Fraction of traces to sample (currently unused; see comment above)

- **Type:** `f64`
- **Default:** `0.1`
- **Valid Range:** 0.0-1.0
- **System Impact:** Reserved for future use; a value of 0.1 would sample 10% of traces


#### `common.telemetry.service_name`

Service name reported in telemetry spans and metrics (currently unused; see comment above)

- **Type:** `String`
- **Default:** `"tasker-core"`
- **Valid Range:** non-empty string
- **System Impact:** Reserved for future use; currently OTEL_SERVICE_NAME env var is used instead



---

*Generated by `tasker-cli docs` — [Tasker Configuration System](https://github.com/tasker-systems/tasker-core)*
---

# Configuration Reference: orchestration



> 100/100 parameters documented
> Generated: 2026-01-31T02:40:18.393404960+00:00

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
| `checkpoint_stall_minutes` | `u32` | `15` | Minutes without a checkpoint update before a batch is considered stalled |
| `default_batch_size` | `u32` | `1000` | Default number of items in a single batch when not specified by the handler |
| `enabled` | `bool` | `true` | Enable the batch processing subsystem for large-scale step execution |
| `max_parallel_batches` | `u32` | `50` | Maximum number of batch operations that can execute concurrently |


#### `orchestration.batch_processing.checkpoint_stall_minutes`

Minutes without a checkpoint update before a batch is considered stalled

- **Type:** `u32`
- **Default:** `15`
- **Valid Range:** 1-1440
- **System Impact:** Stalled batches are flagged for investigation or automatic recovery; lower values detect issues faster


#### `orchestration.batch_processing.default_batch_size`

Default number of items in a single batch when not specified by the handler

- **Type:** `u32`
- **Default:** `1000`
- **Valid Range:** 1-100000
- **System Impact:** Larger batches improve throughput but increase memory usage and per-batch latency


#### `orchestration.batch_processing.enabled`

Enable the batch processing subsystem for large-scale step execution

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, batch step handlers cannot be used; all steps must be processed individually


#### `orchestration.batch_processing.max_parallel_batches`

Maximum number of batch operations that can execute concurrently

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-1000
- **System Impact:** Bounds resource usage from concurrent batch processing; increase for high-throughput batch workloads


## decision_points

**Path:** `orchestration.decision_points`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enable_detailed_logging` | `bool` | `false` | Enable verbose logging of decision point evaluation including expression results |
| `enable_metrics` | `bool` | `true` | Enable metrics collection for decision point evaluations |
| `enabled` | `bool` | `true` | Enable the decision point evaluation subsystem for conditional workflow branching |
| `max_decision_depth` | `u32` | `20` | Maximum depth of nested decision point chains |
| `max_steps_per_decision` | `u32` | `100` | Maximum number of steps that can be generated by a single decision point evaluation |
| `warn_threshold_depth` | `u32` | `10` | Decision depth above which a warning is logged |
| `warn_threshold_steps` | `u32` | `50` | Number of steps per decision above which a warning is logged |


#### `orchestration.decision_points.enable_detailed_logging`

Enable verbose logging of decision point evaluation including expression results

- **Type:** `bool`
- **Default:** `false`
- **Valid Range:** true/false
- **System Impact:** Produces high-volume logs; enable only for debugging specific decision point behavior


#### `orchestration.decision_points.enable_metrics`

Enable metrics collection for decision point evaluations

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Tracks evaluation counts, timings, and branch selection distribution


#### `orchestration.decision_points.enabled`

Enable the decision point evaluation subsystem for conditional workflow branching

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, all decision points are skipped and conditional steps are not evaluated


#### `orchestration.decision_points.max_decision_depth`

Maximum depth of nested decision point chains

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-100
- **System Impact:** Prevents infinite recursion from circular decision point references


#### `orchestration.decision_points.max_steps_per_decision`

Maximum number of steps that can be generated by a single decision point evaluation

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 1-10000
- **System Impact:** Safety limit to prevent decision points from creating unbounded step graphs


#### `orchestration.decision_points.warn_threshold_depth`

Decision depth above which a warning is logged

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-100
- **System Impact:** Observability: identifies deeply nested decision chains that may indicate design issues


#### `orchestration.decision_points.warn_threshold_steps`

Number of steps per decision above which a warning is logged

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-10000
- **System Impact:** Observability: identifies decision points that generate unusually large step sets


## dlq

**Path:** `orchestration.dlq`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `auto_dlq_on_staleness` | `bool` | `true` | Automatically move stale tasks to the DLQ when staleness detection identifies them |
| `enabled` | `bool` | `true` | Enable the Dead Letter Queue subsystem for handling unrecoverable tasks |
| `include_full_task_snapshot` | `bool` | `true` | Include a complete task state snapshot when moving a task to the DLQ |
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


#### `orchestration.dlq.include_full_task_snapshot`

Include a complete task state snapshot when moving a task to the DLQ

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, DLQ entries contain the full task context for debugging; increases DLQ storage requirements


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
| `dependency_cycle_detected` | `bool` | `true` | Enable DLQ routing for tasks with circular step dependency graphs |
| `manual_dlq` | `bool` | `true` | Allow manual DLQ routing via the API |
| `max_retries_exceeded` | `bool` | `true` | Enable DLQ routing for tasks whose steps have exhausted all retry attempts |
| `staleness_timeout` | `bool` | `true` | Enable DLQ routing for tasks that exceed staleness time thresholds |
| `worker_unavailable` | `bool` | `true` | Enable DLQ routing for tasks whose required worker becomes unavailable |


#### `orchestration.dlq.reasons.dependency_cycle_detected`

Enable DLQ routing for tasks with circular step dependency graphs

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Cycles make task completion impossible; DLQ routing preserves the task for debugging


#### `orchestration.dlq.reasons.manual_dlq`

Allow manual DLQ routing via the API

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, the manual DLQ API endpoint is disabled


#### `orchestration.dlq.reasons.max_retries_exceeded`

Enable DLQ routing for tasks whose steps have exhausted all retry attempts

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, tasks with exhausted retries remain in error state without DLQ routing


#### `orchestration.dlq.reasons.staleness_timeout`

Enable DLQ routing for tasks that exceed staleness time thresholds

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, stale tasks are not automatically moved to the DLQ even if detected


#### `orchestration.dlq.reasons.worker_unavailable`

Enable DLQ routing for tasks whose required worker becomes unavailable

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, tasks waiting for unavailable workers remain in their current state indefinitely


### staleness_detection

**Path:** `orchestration.dlq.staleness_detection`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `u32` | `100` | Number of potentially stale tasks to evaluate in a single detection sweep |
| `detection_interval_seconds` | `u32` | `300` | Interval in seconds between staleness detection sweeps |
| `dry_run` | `bool` | `false` | Run staleness detection in observation-only mode without taking action |
| `enabled` | `bool` | `true` | Enable periodic scanning for stale tasks |


#### `orchestration.dlq.staleness_detection.batch_size`

Number of potentially stale tasks to evaluate in a single detection sweep

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 1-10000
- **System Impact:** Larger batches process more stale tasks per sweep but increase per-sweep query cost


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
| `auto_move_to_dlq` | `bool` | `true` | Automatically move stale tasks to the DLQ after transitioning to error |
| `auto_transition_to_error` | `bool` | `true` | Automatically transition stale tasks to the Error state |
| `emit_events` | `bool` | `true` | Emit domain events when staleness is detected |
| `event_channel` | `String` | `"task_staleness_detected"` | PGMQ channel name for staleness detection events |


#### `orchestration.dlq.staleness_detection.actions.auto_move_to_dlq`

Automatically move stale tasks to the DLQ after transitioning to error

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, stale tasks are routed to the DLQ; when false, they remain in Error state for manual review


#### `orchestration.dlq.staleness_detection.actions.auto_transition_to_error`

Automatically transition stale tasks to the Error state

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, stale tasks are moved to Error before DLQ routing; when false, tasks stay in their current state


#### `orchestration.dlq.staleness_detection.actions.emit_events`

Emit domain events when staleness is detected

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, staleness events are published to the event_channel for external alerting or custom handling


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
| `steps_in_process_minutes` | `u32` | `30` | Minutes a task can have steps in process before being considered stale |
| `task_max_lifetime_hours` | `u32` | `24` | Absolute maximum lifetime for any task regardless of state |
| `waiting_for_dependencies_minutes` | `u32` | `60` | Minutes a task can wait for step dependencies before being considered stale |
| `waiting_for_retry_minutes` | `u32` | `30` | Minutes a task can wait for step retries before being considered stale |


#### `orchestration.dlq.staleness_detection.thresholds.steps_in_process_minutes`

Minutes a task can have steps in process before being considered stale

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-1440
- **System Impact:** Tasks in StepsInProcess state exceeding this age may have hung workers; flags for investigation


#### `orchestration.dlq.staleness_detection.thresholds.task_max_lifetime_hours`

Absolute maximum lifetime for any task regardless of state

- **Type:** `u32`
- **Default:** `24`
- **Valid Range:** 1-168
- **System Impact:** Hard cap; tasks exceeding this age are considered stale even if actively processing


#### `orchestration.dlq.staleness_detection.thresholds.waiting_for_dependencies_minutes`

Minutes a task can wait for step dependencies before being considered stale

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 1-1440
- **System Impact:** Tasks in WaitingForDependencies state exceeding this age are flagged for DLQ consideration


#### `orchestration.dlq.staleness_detection.thresholds.waiting_for_retry_minutes`

Minutes a task can wait for step retries before being considered stale

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-1440
- **System Impact:** Tasks in WaitingForRetry state exceeding this age are flagged for DLQ consideration


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
| `enabled` | `bool` | `true` | Enable health monitoring for the orchestration event system |
| `error_rate_threshold_per_minute` | `u32` | `20` | Error rate per minute above which the event system reports as unhealthy |
| `max_consecutive_errors` | `u32` | `10` | Number of consecutive errors before the event system reports as unhealthy |
| `performance_monitoring_enabled` | `bool` | `true` | Enable detailed performance metrics collection for event processing |


#### `orchestration.event_systems.orchestration.health.enabled`

Enable health monitoring for the orchestration event system

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no health checks or error tracking run for this event system


#### `orchestration.event_systems.orchestration.health.error_rate_threshold_per_minute`

Error rate per minute above which the event system reports as unhealthy

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-10000
- **System Impact:** Rate-based health signal; complements max_consecutive_errors for burst error detection


#### `orchestration.event_systems.orchestration.health.max_consecutive_errors`

Number of consecutive errors before the event system reports as unhealthy

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-1000
- **System Impact:** Triggers health status degradation after sustained failures; resets on any success


#### `orchestration.event_systems.orchestration.health.performance_monitoring_enabled`

Enable detailed performance metrics collection for event processing

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Tracks processing latency percentiles and throughput; adds minor overhead


#### processing

**Path:** `orchestration.event_systems.orchestration.processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `u32` | `20` | Number of events dequeued in a single batch read |
| `max_concurrent_operations` | `u32` | `50` | Maximum number of events processed concurrently by the orchestration event system |
| `max_retries` | `u32` | `3` | Maximum retry attempts for a failed event processing operation |


#### `orchestration.event_systems.orchestration.processing.batch_size`

Number of events dequeued in a single batch read

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-1000
- **System Impact:** Larger batches improve throughput but increase per-batch processing time


#### `orchestration.event_systems.orchestration.processing.max_concurrent_operations`

Maximum number of events processed concurrently by the orchestration event system

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-10000
- **System Impact:** Controls parallelism for task request, result, and finalization processing


#### `orchestration.event_systems.orchestration.processing.max_retries`

Maximum retry attempts for a failed event processing operation

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Events exceeding this retry count are dropped or sent to the DLQ


##### backoff

**Path:** `orchestration.event_systems.orchestration.processing.backoff`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_delay_ms` | `u64` | `100` | Initial backoff delay in milliseconds after first event processing failure |
| `jitter_percent` | `f64` | `0.1` | Maximum jitter as a fraction of the computed backoff delay |
| `max_delay_ms` | `u64` | `10000` | Maximum backoff delay in milliseconds between event processing retries |
| `multiplier` | `f64` | `2.0` | Multiplier applied to the backoff delay after each consecutive failure |

#### timing

**Path:** `orchestration.event_systems.orchestration.timing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `claim_timeout_seconds` | `u32` | `300` | Maximum time in seconds an event claim remains valid |
| `fallback_polling_interval_seconds` | `u32` | `5` | Interval in seconds between fallback polling cycles when LISTEN/NOTIFY is unavailable |
| `health_check_interval_seconds` | `u32` | `30` | Interval in seconds between health check probes for the orchestration event system |
| `processing_timeout_seconds` | `u32` | `60` | Maximum time in seconds allowed for processing a single event |
| `visibility_timeout_seconds` | `u32` | `30` | Time in seconds a dequeued message remains invisible to other consumers |


#### `orchestration.event_systems.orchestration.timing.claim_timeout_seconds`

Maximum time in seconds an event claim remains valid

- **Type:** `u32`
- **Default:** `300`
- **Valid Range:** 1-3600
- **System Impact:** Prevents abandoned claims from blocking event processing indefinitely


#### `orchestration.event_systems.orchestration.timing.fallback_polling_interval_seconds`

Interval in seconds between fallback polling cycles when LISTEN/NOTIFY is unavailable

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-60
- **System Impact:** Only active in Hybrid mode when event-driven delivery fails; lower values reduce latency but increase DB load


#### `orchestration.event_systems.orchestration.timing.health_check_interval_seconds`

Interval in seconds between health check probes for the orchestration event system

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Controls how frequently the event system verifies its own connectivity and responsiveness


#### `orchestration.event_systems.orchestration.timing.processing_timeout_seconds`

Maximum time in seconds allowed for processing a single event

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 1-3600
- **System Impact:** Events exceeding this timeout are considered failed and may be retried


#### `orchestration.event_systems.orchestration.timing.visibility_timeout_seconds`

Time in seconds a dequeued message remains invisible to other consumers

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** If processing is not completed within this window, the message becomes visible again for redelivery


### task_readiness

**Path:** `orchestration.event_systems.task_readiness`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `deployment_mode` | `DeploymentMode` | `"Hybrid"` | Event delivery mode for task readiness: 'Hybrid', 'EventDrivenOnly', or 'PollingOnly' |
| `system_id` | `String` | `"task-readiness-event-system"` | Unique identifier for the task readiness event system instance |


#### `orchestration.event_systems.task_readiness.deployment_mode`

Event delivery mode for task readiness: 'Hybrid', 'EventDrivenOnly', or 'PollingOnly'

- **Type:** `DeploymentMode`
- **Default:** `"Hybrid"`
- **Valid Range:** Hybrid | EventDrivenOnly | PollingOnly
- **System Impact:** Hybrid is recommended; task readiness events trigger step processing and benefit from low-latency delivery


#### `orchestration.event_systems.task_readiness.system_id`

Unique identifier for the task readiness event system instance

- **Type:** `String`
- **Default:** `"task-readiness-event-system"`
- **Valid Range:** non-empty string
- **System Impact:** Used in logging and metrics to distinguish task readiness events from other event systems


#### health

**Path:** `orchestration.event_systems.task_readiness.health`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Enable health monitoring for the task readiness event system |
| `error_rate_threshold_per_minute` | `u32` | `20` | Error rate per minute above which the task readiness system reports as unhealthy |
| `max_consecutive_errors` | `u32` | `10` | Number of consecutive errors before the task readiness system reports as unhealthy |
| `performance_monitoring_enabled` | `bool` | `true` | Enable detailed performance metrics for task readiness event processing |


#### `orchestration.event_systems.task_readiness.health.enabled`

Enable health monitoring for the task readiness event system

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When false, no health checks run for task readiness processing


#### `orchestration.event_systems.task_readiness.health.error_rate_threshold_per_minute`

Error rate per minute above which the task readiness system reports as unhealthy

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-10000
- **System Impact:** Rate-based health signal complementing max_consecutive_errors


#### `orchestration.event_systems.task_readiness.health.max_consecutive_errors`

Number of consecutive errors before the task readiness system reports as unhealthy

- **Type:** `u32`
- **Default:** `10`
- **Valid Range:** 1-1000
- **System Impact:** Triggers health status degradation; resets on any successful readiness check


#### `orchestration.event_systems.task_readiness.health.performance_monitoring_enabled`

Enable detailed performance metrics for task readiness event processing

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** Tracks readiness check latency and throughput; useful for tuning batch_size and concurrency


#### processing

**Path:** `orchestration.event_systems.task_readiness.processing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `batch_size` | `u32` | `50` | Number of task readiness events dequeued in a single batch |
| `max_concurrent_operations` | `u32` | `100` | Maximum number of task readiness events processed concurrently |
| `max_retries` | `u32` | `3` | Maximum retry attempts for a failed task readiness event |


#### `orchestration.event_systems.task_readiness.processing.batch_size`

Number of task readiness events dequeued in a single batch

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-1000
- **System Impact:** Larger batches improve throughput for readiness evaluation; 50 balances latency and throughput


#### `orchestration.event_systems.task_readiness.processing.max_concurrent_operations`

Maximum number of task readiness events processed concurrently

- **Type:** `u32`
- **Default:** `100`
- **Valid Range:** 1-10000
- **System Impact:** Higher than orchestration (100 vs 50) because readiness checks are lightweight SQL queries


#### `orchestration.event_systems.task_readiness.processing.max_retries`

Maximum retry attempts for a failed task readiness event

- **Type:** `u32`
- **Default:** `3`
- **Valid Range:** 0-100
- **System Impact:** Readiness events are idempotent so retries are safe; limits retry storms


##### backoff

**Path:** `orchestration.event_systems.task_readiness.processing.backoff`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_delay_ms` | `u64` | `100` | Initial backoff delay in milliseconds after first task readiness processing failure |
| `jitter_percent` | `f64` | `0.1` | Maximum jitter as a fraction of the computed backoff delay for readiness retries |
| `max_delay_ms` | `u64` | `10000` | Maximum backoff delay in milliseconds for task readiness retries |
| `multiplier` | `f64` | `2.0` | Multiplier applied to the backoff delay after each consecutive readiness failure |

#### timing

**Path:** `orchestration.event_systems.task_readiness.timing`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `claim_timeout_seconds` | `u32` | `300` | Maximum time in seconds a task readiness event claim remains valid |
| `fallback_polling_interval_seconds` | `u32` | `5` | Interval in seconds between fallback polling cycles for task readiness |
| `health_check_interval_seconds` | `u32` | `30` | Interval in seconds between health check probes for the task readiness event system |
| `processing_timeout_seconds` | `u32` | `60` | Maximum time in seconds allowed for processing a single task readiness event |
| `visibility_timeout_seconds` | `u32` | `30` | Time in seconds a dequeued task readiness message remains invisible to other consumers |


#### `orchestration.event_systems.task_readiness.timing.claim_timeout_seconds`

Maximum time in seconds a task readiness event claim remains valid

- **Type:** `u32`
- **Default:** `300`
- **Valid Range:** 1-3600
- **System Impact:** Prevents abandoned readiness claims from blocking task evaluation


#### `orchestration.event_systems.task_readiness.timing.fallback_polling_interval_seconds`

Interval in seconds between fallback polling cycles for task readiness

- **Type:** `u32`
- **Default:** `5`
- **Valid Range:** 1-60
- **System Impact:** Fallback interval when LISTEN/NOTIFY is unavailable; lower values improve responsiveness


#### `orchestration.event_systems.task_readiness.timing.health_check_interval_seconds`

Interval in seconds between health check probes for the task readiness event system

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Controls how frequently the task readiness system verifies its own connectivity


#### `orchestration.event_systems.task_readiness.timing.processing_timeout_seconds`

Maximum time in seconds allowed for processing a single task readiness event

- **Type:** `u32`
- **Default:** `60`
- **Valid Range:** 1-3600
- **System Impact:** Readiness events exceeding this timeout are considered failed


#### `orchestration.event_systems.task_readiness.timing.visibility_timeout_seconds`

Time in seconds a dequeued task readiness message remains invisible to other consumers

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Prevents duplicate processing of readiness events during normal operation


## grpc

**Path:** `orchestration.grpc`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `bind_address` | `String` | `"${TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS:-0.0.0.0:9190}"` | Socket address for the gRPC server |
| `enable_health_service` | `bool` | `true` | Enable the gRPC health checking service (grpc.health.v1) |
| `enable_reflection` | `bool` | `true` | Enable gRPC server reflection for service discovery |
| `enabled` | `bool` | `true` | Enable the gRPC API server alongside the REST API |
| `keepalive_interval_seconds` | `u32` | `30` | Interval in seconds between gRPC keepalive ping frames |
| `keepalive_timeout_seconds` | `u32` | `20` | Time in seconds to wait for a keepalive ping acknowledgment before closing the connection |
| `max_concurrent_streams` | `u32` | `200` | Maximum number of concurrent gRPC streams per connection |
| `max_frame_size` | `u32` | `16384` | Maximum size in bytes of a single HTTP/2 frame |
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


#### `orchestration.grpc.keepalive_interval_seconds`

Interval in seconds between gRPC keepalive ping frames

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-3600
- **System Impact:** Detects dead connections; lower values detect failures faster but increase network overhead


#### `orchestration.grpc.keepalive_timeout_seconds`

Time in seconds to wait for a keepalive ping acknowledgment before closing the connection

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-300
- **System Impact:** Connections that fail to acknowledge within this window are considered dead and closed


#### `orchestration.grpc.max_concurrent_streams`

Maximum number of concurrent gRPC streams per connection

- **Type:** `u32`
- **Default:** `200`
- **Valid Range:** 1-10000
- **System Impact:** Limits multiplexed request parallelism per connection; 200 is conservative for orchestration workloads


#### `orchestration.grpc.max_frame_size`

Maximum size in bytes of a single HTTP/2 frame

- **Type:** `u32`
- **Default:** `16384`
- **Valid Range:** 16384-16777215
- **System Impact:** Larger frames reduce framing overhead for large messages but increase memory per-stream


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
| `command_buffer_size` | `usize` | `5000` | Bounded channel capacity for the orchestration command processor |


#### `orchestration.mpsc_channels.command_processor.command_buffer_size`

Bounded channel capacity for the orchestration command processor

- **Type:** `usize`
- **Default:** `5000`
- **Valid Range:** 100-100000
- **System Impact:** Buffers incoming orchestration commands; larger values absorb traffic spikes but use more memory


### event_listeners

**Path:** `orchestration.mpsc_channels.event_listeners`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `pgmq_event_buffer_size` | `usize` | `50000` | Bounded channel capacity for PGMQ event listener notifications |


#### `orchestration.mpsc_channels.event_listeners.pgmq_event_buffer_size`

Bounded channel capacity for PGMQ event listener notifications

- **Type:** `usize`
- **Default:** `50000`
- **Valid Range:** 1000-1000000
- **System Impact:** Large buffer (50000) absorbs high-volume PGMQ LISTEN/NOTIFY events without backpressure on PostgreSQL


### event_systems

**Path:** `orchestration.mpsc_channels.event_systems`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `event_channel_buffer_size` | `usize` | `10000` | Bounded channel capacity for the orchestration event system internal channel |


#### `orchestration.mpsc_channels.event_systems.event_channel_buffer_size`

Bounded channel capacity for the orchestration event system internal channel

- **Type:** `usize`
- **Default:** `10000`
- **Valid Range:** 100-100000
- **System Impact:** Buffers events between the event listener and event processor; larger values absorb notification bursts


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
| `api_key` | `String` | `""` | Static API key for simple key-based authentication |
| `api_key_header` | `String` | `"X-API-Key"` | HTTP header name for API key authentication |
| `enabled` | `bool` | `false` | Enable authentication for the REST API |
| `jwt_audience` | `String` | `"tasker-api"` | Expected 'aud' claim in JWT tokens |
| `jwt_issuer` | `String` | `"tasker-core"` | Expected 'iss' claim in JWT tokens |
| `jwt_private_key` | `String` | `""` | PEM-encoded private key for signing JWT tokens (if this service issues tokens) |
| `jwt_public_key` | `String` | `"${TASKER_JWT_PUBLIC_KEY:-}"` | PEM-encoded public key for verifying JWT token signatures |
| `jwt_public_key_path` | `String` | `"${TASKER_JWT_PUBLIC_KEY_PATH:-}"` | File path to a PEM-encoded public key for JWT verification |
| `jwt_token_expiry_hours` | `u32` | `24` | Default JWT token validity period in hours |


#### `orchestration.web.auth.api_key`

Static API key for simple key-based authentication

- **Type:** `String`
- **Default:** `""`
- **Valid Range:** non-empty string or empty to disable
- **System Impact:** When non-empty and auth is enabled, clients can authenticate by sending this key in the api_key_header


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


#### `orchestration.web.auth.jwt_audience`

Expected 'aud' claim in JWT tokens

- **Type:** `String`
- **Default:** `"tasker-api"`
- **Valid Range:** non-empty string
- **System Impact:** Tokens with a different audience are rejected during validation


#### `orchestration.web.auth.jwt_issuer`

Expected 'iss' claim in JWT tokens

- **Type:** `String`
- **Default:** `"tasker-core"`
- **Valid Range:** non-empty string
- **System Impact:** Tokens with a different issuer are rejected during validation


#### `orchestration.web.auth.jwt_private_key`

PEM-encoded private key for signing JWT tokens (if this service issues tokens)

- **Type:** `String`
- **Default:** `""`
- **Valid Range:** valid PEM private key or empty
- **System Impact:** Required only if the orchestration service issues its own JWT tokens; leave empty when using external identity providers


#### `orchestration.web.auth.jwt_public_key`

PEM-encoded public key for verifying JWT token signatures

- **Type:** `String`
- **Default:** `"${TASKER_JWT_PUBLIC_KEY:-}"`
- **Valid Range:** valid PEM public key or empty
- **System Impact:** Required for JWT validation; prefer jwt_public_key_path for file-based key management in production


#### `orchestration.web.auth.jwt_public_key_path`

File path to a PEM-encoded public key for JWT verification

- **Type:** `String`
- **Default:** `"${TASKER_JWT_PUBLIC_KEY_PATH:-}"`
- **Valid Range:** valid file path or empty
- **System Impact:** Alternative to inline jwt_public_key; supports key rotation by replacing the file


#### `orchestration.web.auth.jwt_token_expiry_hours`

Default JWT token validity period in hours

- **Type:** `u32`
- **Default:** `24`
- **Valid Range:** 1-720
- **System Impact:** Tokens older than this are rejected; shorter values improve security but require more frequent re-authentication


### database_pools

**Path:** `orchestration.web.database_pools`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_total_connections_hint` | `u32` | `50` | Advisory hint for the total number of database connections across all orchestration pools |
| `web_api_connection_timeout_seconds` | `u32` | `30` | Maximum time to wait when acquiring a connection from the web API pool |
| `web_api_idle_timeout_seconds` | `u32` | `600` | Time before an idle web API connection is closed |
| `web_api_max_connections` | `u32` | `30` | Maximum number of connections the web API pool can grow to under load |
| `web_api_pool_size` | `u32` | `20` | Target number of connections in the web API database pool |


#### `orchestration.web.database_pools.max_total_connections_hint`

Advisory hint for the total number of database connections across all orchestration pools

- **Type:** `u32`
- **Default:** `50`
- **Valid Range:** 1-1000
- **System Impact:** Used for capacity planning; not enforced but logged if actual connections exceed this hint


#### `orchestration.web.database_pools.web_api_connection_timeout_seconds`

Maximum time to wait when acquiring a connection from the web API pool

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-300
- **System Impact:** API requests that cannot acquire a connection within this window return an error


#### `orchestration.web.database_pools.web_api_idle_timeout_seconds`

Time before an idle web API connection is closed

- **Type:** `u32`
- **Default:** `600`
- **Valid Range:** 1-3600
- **System Impact:** Controls how quickly the web API pool shrinks after traffic subsides


#### `orchestration.web.database_pools.web_api_max_connections`

Maximum number of connections the web API pool can grow to under load

- **Type:** `u32`
- **Default:** `30`
- **Valid Range:** 1-500
- **System Impact:** Hard ceiling for web API database connections; prevents connection exhaustion from traffic spikes


#### `orchestration.web.database_pools.web_api_pool_size`

Target number of connections in the web API database pool

- **Type:** `u32`
- **Default:** `20`
- **Valid Range:** 1-200
- **System Impact:** Determines how many concurrent database queries the REST API can execute


### resilience

**Path:** `orchestration.web.resilience`

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `circuit_breaker_enabled` | `bool` | `true` | Enable circuit breaker protection for the orchestration REST API |


#### `orchestration.web.resilience.circuit_breaker_enabled`

Enable circuit breaker protection for the orchestration REST API

- **Type:** `bool`
- **Default:** `true`
- **Valid Range:** true/false
- **System Impact:** When true, the API uses circuit breakers to protect against cascading failures from downstream dependencies



---

*Generated by `tasker-cli docs` — [Tasker Configuration System](https://github.com/tasker-systems/tasker-core)*
---

# Configuration Reference: worker



> 101/101 parameters documented
> Generated: 2026-01-31T02:40:18.393404960+00:00

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
# TAS-75 Phase 5: Health Module Refactor

## Problem Statement

The current backpressure and health checking logic in `tasker-orchestration/src/web/state.rs` (lines 292-577) has several architectural issues:

1. **Wrong Location**: Health/backpressure logic doesn't belong in web application state
2. **Real-Time Interrogation**: Every API call queries databases/channels in the hot path
3. **No-Op Placeholder**: `try_get_queue_depth_status()` returns `None` (TODO marker = code smell)
4. **Mixed Concerns**: State management, health checking, and metrics intertwined
5. **Performance**: Database queries in synchronous backpressure checks

---

## Proposed Architecture

### New Module Structure

```
tasker-orchestration/src/health/
├── mod.rs                    # Module exports + HealthMonitor orchestration
├── types.rs                  # Shared types (QueueDepthTier, BackpressureMetrics, etc.)
├── caches.rs                 # Thread-safe status caches (Arc<RwLock<>>)
├── db_status.rs              # Database health evaluation
├── channel_status.rs         # MPSC channel status evaluation
├── queue_status.rs           # PGMQ queue depth evaluation
├── backpressure.rs           # Public try_* API reading from caches
└── status_evaluator.rs       # Background task updating all caches
```

### Core Design Principles

1. **Cache-First Architecture**
   - All status data stored in `Arc<RwLock<StatusCache>>`
   - Web handlers read from cache only (non-blocking)
   - Background task updates caches at configurable intervals

2. **Single Background Evaluator**
   - One `StatusEvaluator` task runs all health checks
   - Configurable interval via `health.evaluation_interval_ms` in TOML
   - Updates all caches atomically per evaluation cycle

3. **Public `try_*` API**
   - `try_check_backpressure() -> Option<ApiError>` - synchronous, reads cache
   - `try_get_queue_depth_status() -> QueueDepthStatus` - always returns cached data
   - `try_get_channel_status() -> ChannelStatus` - always returns cached data
   - No async, no database queries in API path

4. **Metrics Integration**
   - Status evaluator updates OpenTelemetry gauges/counters
   - Reuse `tasker-shared/src/metrics/` patterns (channels, orchestration)
   - Add `tasker-shared/src/metrics/health.rs` for health-specific metrics

---

## Status Cache Design

```rust
// health/caches.rs

/// Thread-safe health status caches
pub struct HealthStatusCaches {
    /// Database pool health status
    db_status: Arc<RwLock<DatabaseHealthStatus>>,

    /// Channel saturation status
    channel_status: Arc<RwLock<ChannelHealthStatus>>,

    /// Queue depth status
    queue_status: Arc<RwLock<QueueDepthStatus>>,

    /// Aggregate backpressure decision
    backpressure: Arc<RwLock<BackpressureStatus>>,

    /// Last evaluation timestamp
    last_evaluated: Arc<RwLock<Instant>>,
}

/// Pre-computed backpressure decision
pub struct BackpressureStatus {
    /// Is backpressure currently active?
    pub active: bool,
    /// Reason for backpressure (if active)
    pub reason: Option<String>,
    /// Suggested retry-after seconds
    pub retry_after_secs: Option<u64>,
    /// Source of backpressure (circuit_breaker, channel, queue)
    pub source: Option<BackpressureSource>,
}

pub enum BackpressureSource {
    CircuitBreaker,
    ChannelSaturation { channel: String, saturation_percent: f64 },
    QueueDepth { queue: String, depth: i64, tier: QueueDepthTier },
}
```

---

## Status Evaluator Design

```rust
// health/status_evaluator.rs

pub struct StatusEvaluator {
    /// Caches to update
    caches: HealthStatusCaches,

    /// Database pool for health checks
    db_pool: PgPool,

    /// Channel monitors (from OrchestrationCore)
    channel_monitors: Vec<Arc<ChannelMonitor>>,

    /// Queue configuration (queue names + thresholds)
    queue_config: QueueHealthConfig,

    /// Circuit breaker reference
    circuit_breaker: Arc<CircuitBreaker>,

    /// Configuration
    config: StatusEvaluatorConfig,
}

pub struct StatusEvaluatorConfig {
    /// How often to evaluate health (milliseconds)
    pub evaluation_interval_ms: u64,

    /// Enable/disable specific checks
    pub check_database: bool,
    pub check_channels: bool,
    pub check_queues: bool,
}

impl StatusEvaluator {
    /// Spawn background evaluation task
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_millis(self.config.evaluation_interval_ms)
            );

            loop {
                interval.tick().await;
                self.evaluate_all().await;
            }
        })
    }

    async fn evaluate_all(&self) {
        // 1. Check database health
        let db_status = self.check_database_health().await;
        *self.caches.db_status.write().await = db_status;

        // 2. Check channel saturation
        let channel_status = self.check_channel_status();
        *self.caches.channel_status.write().await = channel_status;

        // 3. Check queue depths (async DB query)
        let queue_status = self.check_queue_depths().await;
        *self.caches.queue_status.write().await = queue_status;

        // 4. Compute aggregate backpressure decision
        let backpressure = self.compute_backpressure(&db_status, &channel_status, &queue_status);
        *self.caches.backpressure.write().await = backpressure;

        // 5. Update metrics
        self.update_metrics(&db_status, &channel_status, &queue_status, &backpressure);

        // 6. Update timestamp
        *self.caches.last_evaluated.write().await = Instant::now();
    }
}
```

---

## Public Backpressure API

```rust
// health/backpressure.rs

/// TAS-75: Public backpressure checking API
///
/// All methods are synchronous and read from caches - no database queries.
/// Caches are updated by background StatusEvaluator task.
pub struct BackpressureChecker {
    caches: HealthStatusCaches,
}

impl BackpressureChecker {
    /// Check if backpressure is active (synchronous, cache-only)
    ///
    /// Returns `Some(ApiError)` if backpressure is active, `None` if healthy.
    /// This is the primary method for API handlers to check before accepting requests.
    pub fn try_check_backpressure(&self) -> Option<ApiError> {
        let status = self.caches.backpressure.read().blocking_read();

        if status.active {
            Some(ApiError::backpressure(
                status.reason.clone().unwrap_or_default(),
                status.retry_after_secs.unwrap_or(30),
            ))
        } else {
            None
        }
    }

    /// Get current queue depth status (synchronous, cache-only)
    pub fn try_get_queue_depth_status(&self) -> QueueDepthStatus {
        self.caches.queue_status.read().blocking_read().clone()
    }

    /// Get current channel status (synchronous, cache-only)
    pub fn try_get_channel_status(&self) -> ChannelHealthStatus {
        self.caches.channel_status.read().blocking_read().clone()
    }

    /// Get detailed backpressure metrics for health endpoint
    pub async fn get_backpressure_metrics(&self) -> BackpressureMetrics {
        let db = self.caches.db_status.read().await.clone();
        let channel = self.caches.channel_status.read().await.clone();
        let queue = self.caches.queue_status.read().await.clone();
        let bp = self.caches.backpressure.read().await.clone();

        BackpressureMetrics {
            circuit_breaker_open: db.circuit_breaker_open,
            circuit_breaker_failures: db.circuit_breaker_failures,
            command_channel_saturation_percent: channel.command_saturation_percent,
            command_channel_available_capacity: channel.command_available_capacity,
            // ... etc
            backpressure_active: bp.active,
            queue_depth_tier: format!("{:?}", queue.tier),
            queue_depth_max: queue.max_depth,
            queue_depths: queue.queue_depths.clone(),
        }
    }
}
```

---

## Configuration

### TOML Configuration (in orchestration.toml)

Health configuration is added to `config/tasker/base/orchestration.toml` following
the project's three-file convention (`common.toml`, `worker.toml`, `orchestration.toml`).

```toml
# config/tasker/base/orchestration.toml

# ... existing orchestration config ...

# TAS-75 Phase 5: Health status evaluation
[health]
# Background evaluation interval (milliseconds)
evaluation_interval_ms = 5000  # 5 seconds

# Enable/disable specific checks
check_database = true
check_channels = true
check_queues = true

# Stale data threshold (if cache older than this, log warning)
stale_threshold_ms = 30000  # 30 seconds

[health.database]
# Database health check query timeout
query_timeout_ms = 1000

[health.channels]
# Channel saturation thresholds (percent)
warning_threshold = 70.0
critical_threshold = 80.0
emergency_threshold = 95.0

[health.queues]
# Queue depth thresholds (message count)
# Note: These can also be set in common.queues.pgmq.queue_depth_thresholds
warning_threshold = 1000
critical_threshold = 5000
overflow_threshold = 10000
```

Environment overrides in `config/tasker/environments/{test,development,production}/orchestration.toml`:

```toml
# config/tasker/environments/test/orchestration.toml
[health]
evaluation_interval_ms = 1000  # Faster for tests

# config/tasker/environments/production/orchestration.toml
[health]
evaluation_interval_ms = 10000  # 10 seconds in production
stale_threshold_ms = 60000     # 60 seconds stale threshold
```

---

## Migration from state.rs

### Methods to Extract

| Current Location (state.rs) | New Location | Changes |
|---------------------------|--------------|---------|
| `get_command_channel_saturation()` | `channel_status.rs` | Moves to evaluator |
| `is_command_channel_saturated()` | `backpressure.rs` | Reads from cache |
| `is_command_channel_critical()` | `backpressure.rs` | Reads from cache |
| `check_backpressure_status()` | `backpressure.rs::try_check_backpressure()` | Non-blocking |
| `try_get_queue_depth_status()` | `backpressure.rs::try_get_queue_depth_status()` | Returns cached |
| `get_backpressure_metrics()` | `backpressure.rs::get_backpressure_metrics()` | Reads caches |
| `get_queue_depth()` | `queue_status.rs` | Internal to evaluator |
| `get_orchestration_queue_depths()` | `queue_status.rs` | Internal to evaluator |
| `check_queue_depth_status()` | `queue_status.rs` | Internal to evaluator |

### Types to Move

| Type | New Location |
|------|--------------|
| `BackpressureMetrics` | `health/types.rs` |
| `QueueDepthTier` | `health/types.rs` |
| `QueueDepthStatus` | `health/types.rs` |

---

## Integration Points

### health.rs Handler Updates

```rust
// web/handlers/health.rs

/// Update check_queue_depth_health to use cache
async fn check_queue_depth_health(state: &AppState) -> HealthCheck {
    let start = std::time::Instant::now();

    // Read from cache instead of querying DB
    let queue_status = state.backpressure_checker().try_get_queue_depth_status();

    let (status, message) = match queue_status.tier {
        QueueDepthTier::Normal => (
            "healthy",
            format!("All queues normal (max: {} messages)", queue_status.max_depth),
        ),
        // ... same logic, but data comes from cache
    };

    HealthCheck {
        status: status.to_string(),
        message: Some(message),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
```

### tasker-shared/src/metrics/health.rs

```rust
// New file: metrics for health module

/// Total health evaluations performed
pub fn health_evaluations_total() -> Counter<u64> {
    meter().u64_counter("tasker.health.evaluations.total")
        .with_description("Total number of health evaluation cycles")
        .build()
}

/// Current backpressure status (0=healthy, 1=active)
pub fn backpressure_active() -> Gauge<u64> {
    meter().u64_gauge("tasker.backpressure.active")
        .with_description("Whether backpressure is currently active (1) or not (0)")
        .build()
}

/// Queue depth gauge by queue name
pub fn queue_depth_current() -> Gauge<i64> {
    meter().i64_gauge("tasker.queue.depth.current")
        .with_description("Current queue depth in messages")
        .build()
}
```

---

## Bootstrap Integration

```rust
// orchestration/core.rs - during bootstrap

impl OrchestrationCore {
    pub async fn start(&mut self) -> TaskerResult<()> {
        // ... existing bootstrap ...

        // TAS-75 Phase 5: Start health status evaluator
        // Health config is in orchestration.toml: [health]
        let health_caches = HealthStatusCaches::new();
        let evaluator = StatusEvaluator::new(
            health_caches.clone(),
            self.context.database_pool.clone(),
            self.command_channel_monitor(),
            self.context.tasker_config.orchestration.health.clone(),
            self.web_circuit_breaker.clone(),
        );

        self.health_evaluator_handle = Some(evaluator.spawn());
        self.backpressure_checker = Some(BackpressureChecker::new(health_caches));

        // ... rest of bootstrap ...
    }
}
```

---

## Implementation Phases

### Phase 5a: Types and Caches
1. Create `health/` directory structure
2. Extract types to `health/types.rs`
3. Implement `HealthStatusCaches` in `health/caches.rs`

### Phase 5b: Status Evaluators
1. Implement `db_status.rs` - database health evaluation
2. Implement `channel_status.rs` - channel saturation evaluation
3. Implement `queue_status.rs` - queue depth evaluation
4. Implement `status_evaluator.rs` - background orchestration

### Phase 5c: Public API
1. Implement `backpressure.rs` with `try_*` methods
2. Update `state.rs` to delegate to `BackpressureChecker`
3. Update `health.rs` handlers to use caches

### Phase 5d: Metrics Integration
1. Create `tasker-shared/src/metrics/health.rs`
2. Wire metrics updates into `StatusEvaluator`
3. Update `/metrics` endpoint

### Phase 5e: Configuration
1. Add `[health]` section to `orchestration.toml` (base + environment overrides)
2. Add `HealthConfig` struct to `tasker-shared/src/config/orchestration.rs`
3. Wire configuration through bootstrap

### Phase 5f: Cleanup
1. Remove extracted code from `state.rs`
2. Update all imports
3. Add unit tests for new modules

---

## Benefits

1. **Performance**: No database queries in API hot path
2. **Reliability**: Health checks isolated from API request handling
3. **Observability**: Clear metrics for health evaluation frequency
4. **Testability**: Cache-based design easier to test
5. **Configurability**: Evaluation interval tunable per environment
6. **Separation of Concerns**: Clear module boundaries

---

## Risk Mitigation

1. **Stale Data**: Add `last_evaluated` timestamp, log warnings if stale
2. **Evaluation Failures**: Log errors, keep last known good state
3. **Migration**: Implement incrementally, keep both paths during transition

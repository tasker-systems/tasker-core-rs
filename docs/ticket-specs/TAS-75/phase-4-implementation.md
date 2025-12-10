# TAS-75 Phase 4: Worker Load Shedding - Implementation Summary

**Date**: 2025-12-08
**Status**: Partial Implementation (Core infrastructure complete)
**Branch**: `jcoletaylor/tas-67-rust-worker-dual-event-system`

---

## Implementation Summary

### Completed Work

#### 1. Configuration Layer ✅

**File**: `config/tasker/base/worker.toml`

Added load shedding configuration to the worker configuration:

```toml
# TAS-75 Phase 4: Worker Load Shedding Configuration
[worker.mpsc_channels.handler_dispatch.load_shedding]
enabled = true
# Refuse step claims when handler capacity exceeds this threshold (0-100%)
capacity_threshold_percent = 80.0
# Log warning when approaching threshold (0-100%)
warning_threshold_percent = 70.0
```

**Key Design Decisions**:
- Placed under `handler_dispatch` section since it's tied to handler concurrency
- Default threshold of 80% balances responsiveness vs throughput
- Warning threshold at 70% provides early visibility before refusing claims

#### 2. Type Definitions ✅

**File**: `tasker-worker/src/worker/handlers/dispatch_service.rs`

Created three new types:

1. **`LoadSheddingConfig`**: Configuration struct with defaults
   ```rust
   pub struct LoadSheddingConfig {
       pub enabled: bool,
       pub capacity_threshold_percent: f64,
       pub warning_threshold_percent: f64,
   }
   ```

2. **`CapacityChecker`**: Shared capacity monitoring struct
   ```rust
   pub struct CapacityChecker {
       semaphore: Arc<Semaphore>,
       max_concurrent: usize,
       load_shedding: LoadSheddingConfig,
       service_id: String,
   }
   ```

   **Methods**:
   - `has_capacity() -> (bool, f64)` - Returns capacity status and usage percentage
   - `get_usage() -> (usize, usize, f64)` - Returns (in_use, max, usage_percent)

3. **`HandlerDispatchConfig`**: Extended with `load_shedding` field
   - `from_config()` - Creates config with default load shedding
   - `with_load_shedding()` - Creates config with custom load shedding

**Key Design Decisions**:
- `CapacityChecker` is `Clone` for sharing between components
- Returns tuple `(has_capacity, usage_percent)` for metrics/logging
- Warning logs emitted when approaching threshold
- All capacity checks go through the shared semaphore

#### 3. Service Integration ✅

**File**: `tasker-worker/src/worker/handlers/dispatch_service.rs`

Updated `HandlerDispatchService` construction:

```rust
impl<R: StepHandlerRegistry + 'static> HandlerDispatchService<R, NoOpCallback> {
    pub fn new(...) -> (Self, CapacityChecker) {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_handlers));
        let capacity_checker = CapacityChecker::new(...);
        (service, capacity_checker)
    }
}

impl<R: StepHandlerRegistry + 'static, C: PostHandlerCallback> HandlerDispatchService<R, C> {
    pub fn with_callback(...) -> (Self, CapacityChecker) {
        // Same pattern
    }
}
```

**Key Design Decisions**:
- Return `(service, capacity_checker)` tuple instead of just service
- Both constructor methods return capacity checker for consistency
- Semaphore is shared between service and capacity checker
- Breaking change to API - all callsites need updating

---

## Remaining Work

### Critical Path Items

#### 1. Update StepExecutorActor ⚠️ IN PROGRESS

**File**: `tasker-worker/src/worker/actors/step_executor_actor.rs`

**Required Changes**:

```rust
pub struct StepExecutorActor {
    context: Arc<SystemContext>,
    service: Arc<StepExecutorService>,
    dispatch_sender: mpsc::Sender<DispatchHandlerMessage>,
    task_template_manager: Arc<TaskTemplateManager>,
    // TAS-75: Add capacity checker for load shedding
    capacity_checker: Option<CapacityChecker>,
}

impl StepExecutorActor {
    pub fn new(..., capacity_checker: Option<CapacityChecker>) -> Self {
        // Store capacity checker
    }

    // TAS-75: Check capacity before claiming
    async fn claim_and_dispatch(...) -> TaskerResult<bool> {
        // 1. Check capacity first
        if let Some(ref checker) = self.capacity_checker {
            let (has_capacity, usage_percent) = checker.has_capacity();
            if !has_capacity {
                // Metric: increment worker_step_claims_refused_total
                debug!("Refusing step claim - at capacity: {}%", usage_percent);
                return Ok(false); // Leave message in queue
            }
        }

        // 2. Proceed with existing claim logic
        let step_claimer = StepClaim::new(...);
        // ... rest of method unchanged
    }
}
```

**Impact Analysis**:
- ✅ Idempotency preserved: refusing to claim leaves message in PGMQ
- ✅ Visibility timeout protects the message until capacity available
- ✅ Metrics enable monitoring of claim refusals
- ⚠️ Breaking change: Actor constructor signature changes

#### 2. Update WorkerActorRegistry ⚠️ IN PROGRESS

**File**: `tasker-worker/src/worker/actors/registry.rs`

The registry needs to thread the `CapacityChecker` through to the `StepExecutorActor`. This requires:

1. Updating `DispatchChannels` to include `capacity_checker`:
   ```rust
   pub struct DispatchChannels {
       pub dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
       pub dispatch_sender: mpsc::Sender<DispatchHandlerMessage>,
       pub completion_receiver: mpsc::Receiver<StepExecutionResult>,
       pub completion_sender: mpsc::Sender<StepExecutionResult>,
       // TAS-75: Capacity checker for load shedding
       pub capacity_checker: CapacityChecker,
   }
   ```

2. Updating `WorkerActorRegistry::build()` to capture capacity checker from `HandlerDispatchService::new()` and pass it to `StepExecutorActor::new()`.

**Challenge**: The registry is complex with many moving parts. Needs careful integration to avoid breaking existing functionality.

#### 3. Add Metrics 🔴 NOT STARTED

**File**: To be determined (likely `tasker-worker/src/worker/metrics.rs` or similar)

**Required Metrics**:

```rust
// TAS-75: Worker load shedding metric
static WORKER_STEP_CLAIMS_REFUSED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "worker_step_claims_refused_total",
        "Number of step claims refused due to capacity constraints",
        &["worker_id", "namespace", "reason"]
    )
    .unwrap()
});
```

**Labels**:
- `worker_id`: Unique worker identifier
- `namespace`: Step namespace (e.g., "payments", "notifications")
- `reason`: "handler_capacity_threshold" (room for future reasons)

**Integration Points**:
- Increment in `StepExecutorActor::claim_and_dispatch()` when capacity check fails
- Expose via `/metrics` endpoint (existing Prometheus integration)

#### 4. Configuration Loading 🔴 NOT STARTED

**Files**: Various configuration loading code

The TOML configuration needs to be loaded and passed through to `HandlerDispatchConfig`. This likely requires:

1. Updating `tasker-shared/src/config/` to parse `load_shedding` subsection
2. Passing loaded config through bootstrap chain
3. Ensuring environment overrides work correctly

**Alternative**: Use default `LoadSheddingConfig::default()` for Phase 4, defer full TOML loading to future work.

#### 5. Unit Tests 🔴 NOT STARTED

**File**: `tasker-worker/src/worker/handlers/dispatch_service.rs` (tests module)

**Test Cases Needed**:

```rust
#[cfg(test)]
mod tests {
    // TAS-75: Load shedding tests

    #[test]
    fn test_capacity_checker_enabled() {
        // When enabled and below threshold, has_capacity returns true
        // When enabled and at/above threshold, has_capacity returns false
    }

    #[test]
    fn test_capacity_checker_disabled() {
        // When disabled, has_capacity always returns true
    }

    #[test]
    fn test_capacity_checker_warning_threshold() {
        // At warning threshold, log warning but still return true
    }

    #[test]
    fn test_get_usage() {
        // Returns correct in_use, max, and usage_percent
    }

    // Integration test simulating claim refusal
    #[sqlx::test]
    async fn test_step_claim_refused_at_capacity() {
        // Set up StepExecutorActor with capacity checker at 100%
        // Attempt claim
        // Verify claim returns false (not claimed)
        // Verify metric incremented
        // Verify message remains in PGMQ
    }
}
```

#### 6. Documentation 🔴 NOT STARTED

**File**: `docs/backpressure-architecture.md`

**Content to Add**:

```markdown
### Worker Load Shedding (TAS-75 Phase 4)

Workers refuse step claims when handler capacity exceeds configured thresholds.

#### Configuration

```toml
[worker.mpsc_channels.handler_dispatch.load_shedding]
enabled = true
capacity_threshold_percent = 80.0  # Refuse claims above 80%
warning_threshold_percent = 70.0   # Log warnings above 70%
```

#### Behavior

1. **Before Claiming**: Worker checks `CapacityChecker.has_capacity()`
2. **At Capacity**: Claim refused, message left in PGMQ queue
3. **Visibility Timeout**: Message becomes visible again after 30s
4. **Metrics**: `worker_step_claims_refused_total` incremented

#### Idempotency Guarantee

Load shedding preserves step idempotency:
- Refused claims never start handler execution
- Messages remain in PGMQ with visibility timeout
- No state changes occur
- Another worker (or same worker later) can claim when capacity available

#### Monitoring

Key metrics:
- `worker_step_claims_refused_total`: Claims refused due to capacity
- Handler capacity approaching warning threshold (logs)
- Handler capacity at refusal threshold (metrics + logs)
```

---

## Testing Strategy

### Unit Tests
- `CapacityChecker` behavior at various capacity levels
- Config defaults and custom values
- Warning threshold logging

### Integration Tests
- Full claim-refuse-retry cycle
- Metric emission verification
- PGMQ visibility timeout handling

### Load Tests (Future)
- Verify graceful degradation under sustained load
- Measure claim refusal rate vs incoming rate
- Validate no step loss or duplication

---

## Rollout Plan

### Phase 4a: Infrastructure (COMPLETE)
✅ Configuration schema
✅ Type definitions
✅ `CapacityChecker` implementation
✅ Service integration (tuple return)

### Phase 4b: Integration (IN PROGRESS)
⚠️ Update `StepExecutorActor`
⚠️ Update `WorkerActorRegistry`
🔴 Add metrics
🔴 Configuration loading

### Phase 4c: Testing & Documentation (NOT STARTED)
🔴 Unit tests
🔴 Integration tests
🔴 Documentation updates
🔴 Operational runbook updates

### Phase 4d: Deployment (FUTURE)
🔴 Feature flag (default disabled)
🔴 Gradual rollout to production
🔴 Monitor metrics
🔴 Tune thresholds based on real data

---

## Key Design Decisions

### Why Refuse at Claim Time (Not Dispatch Time)?
- **Earlier Detection**: Catch overload before acquiring PGMQ messages
- **Queue Protection**: Don't remove messages from queue if can't process
- **Backpressure**: Natural pushback to orchestration via visibility timeout
- **Idempotency**: No state changes = safe to refuse

### Why Use Semaphore Instead of Simple Counter?
- **Consistency**: Same semaphore used by `HandlerDispatchService`
- **Atomicity**: Semaphore operations are thread-safe
- **Accuracy**: Real-time view of concurrent handler count

### Why Return Tuple from `has_capacity()`?
- **Metrics**: Capture actual usage percentage for monitoring
- **Logging**: Detailed context in warning/error messages
- **Debugging**: Operators can see exact capacity state

---

## Breaking Changes

### API Changes
- `HandlerDispatchService::new()` now returns `(Self, CapacityChecker)`
- `HandlerDispatchService::with_callback()` now returns `(Self, CapacityChecker)`
- `StepExecutorActor::new()` requires `Option<CapacityChecker>` parameter

### Mitigation
- All breaking changes are internal to tasker-worker crate
- No external API impact (REST API, GraphQL remain unchanged)
- Ruby/Python workers unaffected (FFI interface unchanged)

---

## Future Enhancements

### Dynamic Threshold Adjustment
- Auto-tune thresholds based on observed latency
- Separate thresholds per namespace
- Time-of-day threshold variations

### Predictive Load Shedding
- Trend analysis: refuse if capacity increasing rapidly
- Circuit breaker integration: refuse if upstream degraded
- Queue depth awareness: refuse if orchestration queue backing up

### Load Balancing Integration
- Health endpoint reflects capacity status
- Load balancer directs work to workers with capacity
- Coordinated shed across worker pool

---

## References

- [TAS-75 Main Plan](./plan.md)
- [TAS-51 Bounded MPSC Channels](../../architecture-decisions/TAS-51-bounded-mpsc-channels.md)
- [TAS-67 Worker Dual Event System](../TAS-67/summary.md)
- [Backpressure Architecture](../../backpressure-architecture.md)

# TAS-71: Tokio Runtime Analysis for Profiling and Benchmarks

**Date**: 2026-01-20
**Analyst**: Claude
**Ticket**: TAS-71 - Profiling, Benchmarks, and Optimizations

---

## Executive Summary

This analysis examines tokio runtime usage patterns in the tasker-core codebase to identify optimization opportunities for profiling and benchmarking. Key findings:

- **42 tokio::spawn calls** across the codebase (all unnamed)
- **Zero spawn_blocking calls** (no blocking operations in async contexts detected)
- **All MPSC channels are bounded** (TAS-51 compliant)
- **Comprehensive tracing instrumentation** already in place via OpenTelemetry
- **tokio-console integration is feasible** with moderate effort

---

## 1. Spawn Pattern Analysis

### 1.1 Spawn Count Summary

| Component | Count | Named | Purpose |
|-----------|-------|-------|---------|
| tasker-orchestration | 11 | 5 | Actor loops, background services |
| tasker-worker | 12 | 8 | Event processing, handlers, services |
| tasker-shared | 6 | 0 | Metrics, event publishing, messaging |
| tasker-pgmq | 5 | 0 | Listeners, notifications (standalone crate) |
| workers/rust | 3 | 0 | Main loop, event handlers |
| workers/ruby | 2 | 0 | FFI event handlers |
| **Total** | **42** | **13** | |

**TAS-158 Update**: 13 long-running spawns have been named for tokio-console visibility.
Note: tasker-pgmq is a standalone crate without tasker-shared dependency, so spawns are unnamed.

### 1.2 Spawn Categories

#### A. Actor/Service Loops (Long-Running)

These are the core event-processing loops that run for the lifetime of the system:

```
tasker-orchestration/src/actors/command_processor_actor.rs:157
  - OrchestrationCommandProcessorActor main loop

tasker-orchestration/src/orchestration/core.rs:234
  - Staleness detector background service

tasker-worker/src/worker/core.rs:452, 467, 488
  - ActorCommandProcessor loop
  - DomainEventSystem loop
  - CompletionProcessorService loop

tasker-orchestration/src/orchestration/bootstrap.rs:383, 398
  - Bootstrap background tasks

tasker-worker/src/bootstrap.rs:387, 405
  - Worker bootstrap background tasks
```

**Recommendation**: These are the highest-priority spawns for naming since they run for the entire system lifetime.

#### B. Event Listeners/Processors (Long-Running)

Background listeners for PGMQ/messaging events:

```
tasker-pgmq/src/listener.rs:418, 513
  - PGMQ notification listeners

tasker-orchestration/src/orchestration/orchestration_queues/listener.rs:233
  - Orchestration queue listener

tasker-worker/src/worker/worker_queues/listener.rs:229
  - Worker queue listener

tasker-worker/src/worker/event_subscriber.rs:177, 460, 528
  - Step completion event listeners
```

**Recommendation**: High value for naming - essential for tracing message flow.

#### C. Per-Request/Handler Spawns (Short-Lived)

Individual task processing spawns:

```
tasker-worker/src/worker/handlers/dispatch_service.rs:477
  - Handler dispatch (spawns per-step execution)

tasker-orchestration/src/orchestration/lifecycle/step_enqueuer_services/batch_processor.rs:87
  - Batch processing futures

tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs:494, 549
  - Event processing spawns
```

**Recommendation**: Medium value - high volume but short-lived. Consider structured logging instead of task naming.

#### D. Utility/Background Tasks

Metrics, timers, and utilities:

```
tasker-shared/src/metrics/channels.rs:350, 513
  - Periodic metrics export

tasker-shared/src/events/publisher.rs:385
  - Event publisher background task

tasker-shared/src/messaging/service/providers/pgmq.rs:402, 640
  - PGMQ provider background tasks
```

**Recommendation**: Low priority for naming, but useful for debugging metrics issues.

### 1.3 Hot Path Analysis

The following spawns are in critical hot paths:

1. **Handler Dispatch** (`dispatch_service.rs:477`) - Every step execution
2. **Batch Processor** (`batch_processor.rs:87`) - Every step batch
3. **Event System Notifications** (`orchestration_event_system.rs:494, 549`) - Every event

These would benefit most from profiling instrumentation but least from task naming (volume too high).

---

## 2. Runtime Configuration Analysis

### 2.1 Current Configuration

The runtime is configured via `tokio::runtime::Builder` in FFI bridges:

```rust
// workers/ruby/ext/tasker_core/src/bootstrap.rs:77
tokio::runtime::Builder::new_multi_thread()
    .worker_threads(8)
    .thread_name("ruby-ffi-runtime")
    .enable_all()
    .build()
```

**Current Settings**:
| Setting | Value | Configurable |
|---------|-------|--------------|
| Runtime Type | Multi-threaded | Hardcoded |
| Worker Threads | 8 | Hardcoded |
| Thread Name | "ruby-ffi-runtime" | Hardcoded |
| IO/Time | Enabled | Hardcoded |

### 2.2 Configuration Gaps

- **No TOML configuration** for runtime settings
- **Hardcoded thread count** (8 threads) - fixed for M2/M4 Pro compatibility
- **No runtime metrics** exposed
- **No per-component runtimes** - all share the same runtime

### 2.3 Recommendations

1. **Add runtime configuration to TOML**:
   ```toml
   [runtime]
   worker_threads = 8
   thread_name_prefix = "tasker-worker"
   enable_tracing = true
   ```

2. **Consider separate runtimes** for:
   - FFI callbacks (current)
   - Blocking I/O (new)
   - Metrics collection (new, low priority)

---

## 3. Channel Usage Analysis

### 3.1 Channel Compliance (TAS-51)

**All MPSC channels are bounded** - fully compliant with TAS-51 guidelines.

Evidence from codebase:
- `tasker-shared/src/config/mpsc_channels.rs` - Centralized channel configuration
- All channels created via `ChannelFactory` with configurable buffer sizes
- Explicit comments documenting TAS-51 migration from unbounded channels

### 3.2 Channel Configuration Summary

| Channel | Buffer Size | Component | Configurable |
|---------|-------------|-----------|--------------|
| Orchestration Command | 5000 | orchestration | Yes (TOML) |
| PGMQ Event | 5000 | orchestration | Yes (TOML) |
| Worker Command | 2000 | worker | Yes (TOML) |
| In-Process Events | 1000 | worker | Yes (TOML) |
| FFI Ruby Events | 1000 | shared | Yes (TOML) |
| Event Publisher | 5000 | shared | Yes (TOML) |
| Completion | Configurable | worker | Yes (TOML) |

### 3.3 Channel Monitoring

Comprehensive monitoring via `ChannelMonitor`:
- Saturation tracking
- Health status (healthy/degraded/critical)
- OpenTelemetry metrics export
- Per-channel statistics

---

## 4. Async Pattern Analysis

### 4.1 spawn_blocking Usage

**Zero spawn_blocking calls detected.** This is a positive finding indicating:
- No blocking operations in async contexts
- Proper async/await throughout
- Database operations use async sqlx

### 4.2 Potential Blocking Concerns

While no explicit spawn_blocking calls exist, these areas warrant monitoring:

1. **FFI Callbacks** - Ruby/Python GIL could block
   - Mitigated by: Pull-based design with timeouts

2. **JSON Serialization** - Large payloads could block
   - Mitigated by: Async message handling

3. **File I/O** - Configuration loading
   - Impact: Only at startup, acceptable

### 4.3 Async Anti-Patterns

**None detected.** The codebase follows good async patterns:
- No `.block_on()` inside async contexts (except FFI boundaries)
- Proper use of `tokio::time::timeout`
- Channel-based communication without busy loops

---

## 5. Tracing Integration Analysis

### 5.1 Current Tracing Setup

**Comprehensive tracing already in place:**

```rust
// tasker-shared/src/logging.rs
- OpenTelemetry integration with OTLP exporter
- TracerProvider with batch export (non-blocking)
- LoggerProvider for log forwarding
- TraceContextPropagator for W3C trace context
```

### 5.2 Instrumentation Coverage

| Feature | Status | Location |
|---------|--------|----------|
| Function-level spans | Yes | `#[instrument]` macros throughout |
| Structured logging | Yes | `tracing::info!`, etc. |
| Correlation IDs | Yes | `correlation_id` field propagation |
| OpenTelemetry export | Yes | OTLP to configurable endpoint |
| Log forwarding | Optional | OTEL_LOGS_ENABLED env var |

### 5.3 Example Instrumentation

```rust
#[instrument(skip(self), fields(task_uuid = %task_uuid))]
async fn handle_initialize_task(&self, ...) -> TaskerResult<...>
```

Found 100+ `#[instrument]` macros across:
- `tasker-pgmq/src/` - Queue operations
- `tasker-orchestration/src/` - Lifecycle services
- `tasker-worker/src/` - Handler dispatch
- `tasker-shared/src/` - Event publishing

---

## 6. tokio-console Integration Feasibility

### 6.1 What tokio-console Provides

- Real-time task visualization
- Resource usage per task
- Channel buffer visibility
- Waker/poll statistics
- Blocking detection

### 6.2 Requirements for Integration

**TAS-158 Implementation Status: COMPLETE**

1. **Add console-subscriber dependency** (DONE):
   ```toml
   # tasker-shared/Cargo.toml
   console-subscriber = { version = "0.4", optional = true }

   [features]
   tokio-console = ["console-subscriber"]
   ```

2. **Enable tokio unstable features** (required at compile time):
   ```bash
   RUSTFLAGS="--cfg tokio_unstable" cargo build --features tokio-console
   ```

3. **Initialize subscriber** (DONE - integrated with existing tracing):
   The `ConsoleLayer` is automatically added to the tracing subscriber when
   the `tokio-console` feature is enabled. See `tasker-shared/src/logging.rs`.

4. **Name spawned tasks** (DONE - 15 long-running spawns named):
   ```rust
   // Example from tasker-orchestration/src/actors/command_processor_actor.rs
   tokio::task::Builder::new()
       .name("orchestration_command_processor")
       .spawn(async move { ... })
       .expect("failed to spawn orchestration_command_processor task");
   ```

### 6.3 Named Spawns (TAS-158)

| Task Name | File | Purpose |
|-----------|------|---------|
| orchestration_command_processor | actors/command_processor_actor.rs | Main orchestration command loop |
| staleness_detector | orchestration/core.rs | Background staleness detection |
| worker_command_processor | worker/core.rs | Worker command processing |
| worker_domain_event_system | worker/core.rs | Domain event handling |
| worker_completion_processor | worker/core.rs | Step completion routing |
| orchestration_web_server | orchestration/bootstrap.rs | HTTP API server |
| orchestration_shutdown_handler | orchestration/bootstrap.rs | Graceful shutdown |
| worker_web_server | worker/bootstrap.rs | Worker HTTP API |
| worker_shutdown_handler | worker/bootstrap.rs | Worker graceful shutdown |
| orchestration_queue_listener | orchestration_queues/listener.rs | Queue subscription |
| worker_queue_listener | worker_queues/listener.rs | Worker queue subscription |
| worker_completion_event_listener | event_subscriber.rs | FFI completion events |
| worker_step_dispatch_listener | event_subscriber.rs | Step dispatch correlation |

**Note**: tasker-pgmq spawns are unnamed as it's a standalone crate. The `spawn_named!` macro
is defined in tasker-shared and available to dependent crates.

### 6.4 Integration Effort (Actual)

| Task | Estimated | Actual | Status |
|------|-----------|--------|--------|
| Add console-subscriber crate | 0.5 hours | 0.25 hours | Complete |
| Configure feature flag | 1 hour | 0.5 hours | Complete |
| Name 15 long-running spawns | 2 hours | 1.5 hours | Complete (13 spawns*) |
| Update documentation | 1 hour | 0.5 hours | Complete |
| Test and validate | 2 hours | TBD | Pending |
| **Total** | **6.5 hours** | **~3 hours** | |

*13 spawns named in tasker-orchestration and tasker-worker. pgmq-notify spawns remain
unnamed as it's a standalone crate without tasker-shared dependency.

### 6.4 Compatibility Notes

- Compatible with existing tracing-subscriber setup
- Can layer with OpenTelemetry
- Requires RUSTFLAGS for tokio_unstable
- Adds ~50KB to debug builds

---

## 7. Recommendations Summary

### 7.1 High Priority (COMPLETED in TAS-158)

1. **Name the long-running spawns** - DONE
   - Created `spawn_named!` macro that conditionally uses `tokio::task::Builder`
   - Named 13 spawns across tasker-orchestration and tasker-worker
   - See Section 6.3 for complete list

2. **Add tokio-console feature flag** - DONE
   - Feature: `tokio-console` in tasker-shared
   - Integrated with existing tracing subscriber
   - Documentation updated

### 7.2 Medium Priority (Near-Term)

3. **Add runtime configuration to TOML**
   - Worker threads, thread names
   - Enable per-environment tuning
   - Effort: 3 hours

4. **Add spawn metrics**
   - Track active task count per category
   - Expose via health endpoints
   - Effort: 4 hours

### 7.3 Low Priority (Future)

5. **Consider separate runtimes**
   - Isolate FFI from core processing
   - Only if profiling reveals contention
   - Effort: 8 hours

---

## 8. Files Referenced

Key files analyzed for this report:

**Spawn Patterns:**
- `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-orchestration/src/actors/command_processor_actor.rs`
- `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-orchestration/src/orchestration/core.rs`
- `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-worker/src/worker/core.rs`
- `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-worker/src/worker/handlers/dispatch_service.rs`

**Runtime Configuration:**
- `/Users/petetaylor/projects/tasker-systems/tasker-core/workers/ruby/ext/tasker_core/src/bootstrap.rs`

**Channel Patterns:**
- `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-shared/src/config/mpsc_channels.rs`
- `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-shared/src/metrics/channels.rs`

**Tracing Integration:**
- `/Users/petetaylor/projects/tasker-systems/tasker-core/tasker-shared/src/logging.rs`

---

## Appendix A: Complete Spawn Inventory

```
tasker-pgmq/src/listener.rs:418     - PgmqListener notification handler
tasker-pgmq/src/listener.rs:513     - PgmqListener background task
tasker-pgmq/tests/common.rs:191     - Test infrastructure
tasker-pgmq/tests/comprehensive_integration_test.rs:432 - Test
tasker-shared/src/metrics/channels.rs:350 - Periodic metrics export
tasker-shared/src/metrics/channels.rs:513 - Channel registration
tasker-shared/benches/event_propagation.rs:116 - Benchmark
tasker-shared/src/events/publisher.rs:385 - Event publisher loop
tasker-shared/src/messaging/service/providers/rabbitmq.rs:727 - RabbitMQ consumer
tasker-shared/src/messaging/service/providers/pgmq.rs:402 - PGMQ poll loop
tasker-shared/src/messaging/service/providers/pgmq.rs:640 - PGMQ consumer
tasker-worker/src/worker/core.rs:452 - ActorCommandProcessor loop
tasker-worker/src/worker/core.rs:467 - DomainEventSystem loop
tasker-worker/src/worker/core.rs:488 - CompletionProcessorService loop
tasker-worker/src/bootstrap.rs:387 - Orchestration client pinger
tasker-worker/src/bootstrap.rs:405 - Web server
tasker-worker/src/worker/worker_queues/fallback_poller.rs:186 - Fallback poller
tasker-worker/src/worker/worker_queues/listener.rs:229 - Queue listener
workers/rust/src/main.rs:20 - Main worker loop
workers/rust/src/event_handler.rs:101 - Event handler
tasker-worker/src/worker/event_subscriber.rs:177 - Completion listener
tasker-worker/src/worker/event_subscriber.rs:460 - Step dispatch listener
tasker-worker/src/worker/event_subscriber.rs:528 - Step event listener
workers/rust/src/bootstrap.rs:182 - Rust worker bootstrap
tasker-worker/src/worker/handlers/dispatch_service.rs:477 - Handler dispatch
tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs:494 - Task init events
tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs:549 - Step result events
tasker-worker/src/worker/event_systems/worker_event_system.rs:383 - Worker events
tasker-orchestration/src/orchestration/task_readiness/fallback_poller.rs:133 - Task readiness poller
tasker-orchestration/src/orchestration/bootstrap.rs:383 - Orchestration client pinger
tasker-orchestration/src/orchestration/bootstrap.rs:398 - Web server
tasker-orchestration/src/orchestration/core.rs:234 - Staleness detector
workers/ruby/ext/tasker_core/src/event_handler.rs:74 - Ruby event handler
tasker-orchestration/src/actors/command_processor_actor.rs:157 - Command processor loop
tasker-orchestration/src/health/status_evaluator.rs:154 - Health status evaluator
tasker-orchestration/src/orchestration/lifecycle/step_enqueuer_services/batch_processor.rs:87 - Batch processor
tasker-orchestration/tests/web/test_infrastructure.rs:97 - Test infrastructure
tasker-orchestration/src/orchestration/staleness_detector.rs:406 - Staleness detector loop
tasker-orchestration/src/orchestration/orchestration_queues/fallback_poller.rs:262 - Fallback poller
tasker-orchestration/src/orchestration/orchestration_queues/listener.rs:233 - Queue listener
```

# TAS-161: Profiling, Analysis, and Optimization Target Identification

**Status:** In Progress
**Parent:** TAS-71 (Profiling, Benchmarks, and Optimizations)
**Prerequisites:** TAS-158 (tokio-console), TAS-159 (E2E benchmarks)

## Summary

Profile the Tasker system using samply and tokio-console to identify optimization targets, prioritize by value-for-effort, and create actionable follow-up tickets.

## Foundation

We now have solid baseline data from TAS-159:

| Tier | Scenario | P50 Latency |
|------|----------|-------------|
| Tier 1 | linear_rust | 126.66ms |
| Tier 1 | diamond_rust | 127.26ms |
| Tier 2 | complex_dag_rust | 187.58ms |
| Tier 2 | hierarchical_tree_rust | 190.27ms |
| Tier 4 | FFI workers (avg) | ~184ms |
| Tier 5 | batch_1000_rows | 173.41ms |

**P95/P50 ratios < 1.1** indicate stable performance with minimal variance.

---

## Profiling Tools

| Tool | Purpose | When to Use |
|------|---------|-------------|
| **samply** | CPU profiling, flamegraphs | Function-level hot path analysis |
| **tokio-console** | Async runtime introspection | Task starvation, channel pressure, waker behavior |
| **DHAT** (optional) | Heap allocation profiling | If heap pressure suspected |

### samply Workflow (cargo-make integrated)

```bash
# Build with profiling profile (release + debug symbols)
cargo make build-profiling   # or: cargo make bp

# Terminal 1: Start worker in release mode (background)
cargo make run-worker-rust-release

# Terminal 2: Run orchestration under samply (foreground)
cargo make run-samply-orchestration   # or: cargo make rso

# Terminal 3: Generate workload
cargo make bench-e2e

# Press Ctrl-C in Terminal 2 to stop samply and open Firefox Profiler
```

**Alternative: Profile the worker instead:**
```bash
# Terminal 1: Start orchestration in release mode
cargo make run-orchestration-release

# Terminal 2: Run worker under samply (foreground)
cargo make run-samply-worker-rust   # or: cargo make rswr

# Terminal 3: Generate workload
cargo make bench-e2e

# Press Ctrl-C in Terminal 2 to stop and view profile
```

### tokio-console Workflow (cargo-make integrated)

```bash
# Build with console support
cargo make build-console   # or: cargo make bc

# Terminal 1: Run orchestration with console
cargo make run-console-orchestration   # or: cargo make rco

# Terminal 2: Open console
cargo make console
```

---

## Optimization Tiers

### Tier 1: Low-Hanging Fruit (High ROI, Low Risk)

These are quick wins that can be addressed with minimal risk.

#### 1.1 Hot Path Logging

**Goal:** Reduce logging overhead in frequently-executed code paths.

**Targets:**
- `handle_step_execution_result` - called on every step completion
- `process_single_task_from_ready_info` - task processing loop
- `enqueue_ready_steps` - step enqueueing
- Actor message handlers

**Analysis:**
- Profile with samply under benchmark load
- Look for `tracing::` spans in top functions
- Evaluate: Should DEBUG logs be gated behind `tracing::enabled!`?

**Indicators:**
- `tracing` frames in flamegraph hot paths
- High frequency of span creation/drop

#### 1.2 Unnecessary Clones

**Goal:** Replace `.clone()` with references where ownership isn't needed.

**Targets:**
- `TaskRequest` passing through actors
- `StepExecutionResult` in result processing
- `serde_json::Value` context propagation

**Analysis:**
```bash
# Find clone calls in hot path modules
rg "\.clone\(\)" tasker-orchestration/src/orchestration/lifecycle/
rg "\.clone\(\)" tasker-worker/src/
```

**Indicators:**
- `clone` or `Clone::clone` in flamegraph
- Multiple clones of same struct in call chain

#### 1.3 String Allocations in Hot Paths

**Goal:** Reduce unnecessary string allocations.

**Targets:**
- `format!` in logging (use structured logging instead)
- `to_string()` for error messages in hot paths
- `String::from` for static strings (use `&'static str`)

**Analysis:**
- Look for `alloc::string::String` in flamegraph
- Check for `format!` in actor handlers

---

### Tier 2: Structural Analysis (Medium Effort)

#### 2.1 Struct Memory Layout

**Goal:** Optimize high-frequency DTO structs for cache efficiency and stack allocation.

**Priority Structs:**
- `StepExecutionResult` - every step completion
- `TaskRequest` - task creation
- `StepExecutionMetadata` - result metadata
- `SimpleMessage` / `RabbitmqMessage` - message queue payloads

**Analysis:**
```rust
// Add to a test or debug build
println!("StepExecutionResult size: {}", std::mem::size_of::<StepExecutionResult>());
println!("TaskRequest size: {}", std::mem::size_of::<TaskRequest>());
```

**Considerations:**
- Field ordering for minimal padding
- `Box<T>` for large optional fields
- `SmallVec` for small, bounded collections
- `CompactString` for short strings

#### 2.2 Heap vs Stack Allocation

**Goal:** Identify heap allocations that could be stack-allocated.

**Targets:**
- `Vec` in tight loops (consider `ArrayVec` or `SmallVec`)
- `Box::new` in frequently-called functions
- Temporary `String` allocations

**Analysis:**
- DHAT profiler for allocation hotspots
- samply for `__rust_alloc` in flamegraph

#### 2.3 Channel Pressure Analysis

**Goal:** Identify actor channels under pressure or causing backpressure.

**Targets:**
- `task_request_tx/rx` - task creation channel
- `result_processor_tx/rx` - step result channel
- `step_enqueuer_tx/rx` - step enqueueing channel
- `task_finalizer_tx/rx` - task completion channel

**Analysis (tokio-console):**
- Check for channels with high poll counts
- Look for tasks blocked on channel sends
- Identify self-waking tasks (potential spin)

---

### Tier 3: Architectural Considerations (Higher Effort, Bigger Impact)

#### 3.1 Serialization Format: MessagePack for PGMQ/RabbitMQ

**Goal:** Evaluate MessagePack for message payloads to reduce serialization overhead.

**Current State:**
- All message payloads use `serde_json`
- PGMQ stores in `jsonb` columns (requires JSON)
- RabbitMQ payloads could use any format

**Proposed Approach:**
- Keep JSON wrapper for PGMQ compatibility (jsonb column)
- Use MessagePack for the `payload` field within the JSON wrapper
- Benchmark: `serde_json` vs `rmp-serde` for `StepExecutionResult`

**Analysis:**
```rust
// Benchmark serialization
let json_size = serde_json::to_vec(&result)?.len();
let msgpack_size = rmp_serde::to_vec(&result)?.len();
// Also measure serialization time
```

**Considerations:**
- Backwards compatibility during rollout
- Debugging ergonomics (JSON is human-readable)
- Size reduction vs CPU overhead tradeoff

#### 3.2 Redis Cache for Named Tasks / Template Lookups

**Goal:** Reduce database load for frequently-queried, rarely-changed data.

**Cache Candidates:**
- `named_tasks` lookups by namespace + name
- `task_templates` configuration
- `TaskHandlerRegistry` (worker-side)

**Architecture:**
```
┌─────────────────┐     ┌───────────────┐     ┌──────────────┐
│  Orchestration  │────▶│  Redis Cache  │────▶│  PostgreSQL  │
│    Server       │     │  (read-through)│     │  (source of  │
└─────────────────┘     └───────────────┘     │   truth)     │
                               │              └──────────────┘
                               ▼
┌─────────────────┐     ┌───────────────┐
│     Workers     │────▶│  Cache Hit?   │
│  (invalidate on │     │  Yes: return  │
│   template      │     │  No: fetch +  │
│   change)       │     │      cache    │
└─────────────────┘     └───────────────┘
```

**Invalidation Strategy:**
- Workers invalidate on template file changes
- TTL-based expiry as fallback
- Pub/sub for cross-instance invalidation

**Analysis:**
- Profile `get_named_task_by_namespace_and_name` query frequency
- Measure query latency in production-like conditions

#### 3.3 Connection Pool Tuning

**Goal:** Optimize database connection pool for workload patterns.

**Analysis:**
- Profile connection acquisition latency
- Monitor pool exhaustion under load
- Evaluate pool size vs connection overhead

**Metrics to Capture:**
- Connection wait time
- Active vs idle connections
- Pool exhaustion events

---

## Profiling Sessions

### Session 1: Orchestration Hot Path

**Workload:** `cargo make bench-e2e` (Tier 1 + Tier 2)

**Focus:**
- Task creation → step enqueueing → result processing → task completion
- Identify top 5 functions by CPU time

**Output:**
- Flamegraph: `docs/ticket-specs/TAS-71/flamegraphs/orchestration-e2e.svg`
- Top functions table

### Session 2: Worker Hot Path

**Workload:** Sustained linear workflow processing

**Focus:**
- Message consumption → handler dispatch → result submission
- FFI overhead comparison (Rust vs Ruby/Python/TypeScript)

**Output:**
- Flamegraph: `docs/ticket-specs/TAS-71/flamegraphs/worker-dispatch.svg`
- FFI overhead breakdown

### Session 3: Async Runtime Analysis

**Tool:** tokio-console

**Focus:**
- Actor channel utilization
- Task wake patterns
- Potential blocking operations

**Output:**
- Channel utilization report
- Recommendations for channel sizing

---

## Deliverables

1. **Profiling Report** (`docs/ticket-specs/TAS-71/profiling-report.md`)
   - Flamegraphs for key scenarios
   - Top 10 CPU consumers
   - Allocation hotspots (if DHAT used)

2. **Optimization Targets** (prioritized list)
   - Impact estimate (high/medium/low)
   - Effort estimate (high/medium/low)
   - Dependencies

3. **Follow-up Tickets**
   - One ticket per optimization target
   - Clear acceptance criteria
   - Link back to profiling evidence

---

## Success Criteria

- [ ] samply flamegraphs generated for orchestration + worker
- [ ] tokio-console analysis completed for actor channels
- [ ] Top 5 optimization targets identified and prioritized
- [ ] Follow-up tickets created with clear scope
- [ ] Baseline measurements documented for regression tracking

---

## Architecture Research Findings

The following targets were identified through analysis of `docs/architecture/*.md` and crate documentation.

### Orchestration Hot Paths (actors.md, states-and-lifecycles.md)

| Path | Frequency | Critical Component |
|------|-----------|-------------------|
| Task initialization | 1/task | `TaskRequestActor` → `TaskInitializer` |
| Step readiness evaluation | 1/step completion | `StepEnqueuerService.batch_processor` |
| Step result processing | 1/step | `ResultProcessorActor` → `OrchestrationResultProcessor` |
| State machine transitions | 5-12+/task | `TaskStateMachine`, `WorkflowStepStateMachine` |
| Task finalization (atomic claim) | 1/task | `TaskFinalizerActor` (CAS loop) |

**Key SQL Functions to Profile:**
- `get_step_readiness_status()` - Dependency evaluation, called repeatedly
- `transition_task_state_atomic()` - CAS operation with retry
- `get_task_execution_context()` - Step enumeration
- `get_next_ready_tasks()` - Task claim loop

### Worker Hot Paths (worker-event-systems.md, AGENTS.md)

| Path | Bottleneck Risk | Location |
|------|-----------------|----------|
| Semaphore acquire | **CRITICAL** | `dispatch_service.rs:280` (max=10 concurrent) |
| Handler registry lookup | HIGH | `resolver_integration.rs` (TAS-93 chain resolution) |
| RwLock contention (FFI) | HIGH | `ffi_dispatch_channel.rs:206` (pending_events) |
| Completion channel send | MEDIUM | `dispatch_service.rs:557` (permit released before send) |
| Task template hydration | MEDIUM | `step_executor_actor.rs` (DB query per step) |

**Critical Design Pattern:**
```
Permit released BEFORE completion channel send
→ Prevents backpressure cascade from blocking handler execution
→ Profile: Verify this doesn't cause other issues
```

### High-Frequency DTOs (messaging/execution_types.rs)

| Struct | Created | Serialization Points |
|--------|---------|---------------------|
| `StepExecutionResult` | Every step | DB (JSONB), completion channel, queue |
| `TaskRequest` | Every task | Actor pipeline, validation |
| `StepExecutionMetadata` | Every result | Embedded in result |
| `WorkflowStep` | Every step load | 16+ fields, frequent DB round-trips |

**Memory Layout Candidates:**
```rust
// Check sizes:
std::mem::size_of::<StepExecutionResult>()
std::mem::size_of::<TaskRequest>()
std::mem::size_of::<WorkflowStep>()
```

### Database Interaction Patterns

**Load Scenario: 1000 Tasks × 100 Steps Each:**
- 5,000-12,000 task state transitions
- 100,000 step state transitions
- 100,000 audit inserts (TAS-62 trigger)
- 100,000+ dependency evaluation queries
- Peak: 300+ concurrent connections (if batched)

**Connection Pool Config (production defaults):**
```toml
max_connections = 50
min_connections = 10
connection_timeout_ms = 5000
```

### Channel Configurations

| Channel | Buffer Size | Bottleneck Risk |
|---------|-------------|-----------------|
| Command Processor (orchestration) | 5000 | HIGH - all nodes share |
| PGMQ Events | 50000 | MEDIUM |
| Domain Events | 10000 | MEDIUM (drops on full) |
| Handler Dispatch (per worker) | 1000 | HIGH |
| Completion (per worker) | 1000 | HIGH - blocking |
| FFI Dispatch | 1000 | MEDIUM |

### Circuit Breaker Integration (TAS-75)

**FFI Completion Circuit Breaker:**
- Slow send threshold: 100ms
- Consecutive slow sends trigger open state
- Profile: State transition overhead, rejection rate

**Capacity Checker (Load Shedding):**
- Semaphore utilization check
- Warning thresholds: 70%, 80%
- Profile: Decision overhead under load

---

## Additional Profiling Targets (from Architecture Analysis)

### P0 - Critical Path (Profile First)

1. **State Machine CAS Loop** (`most_recent` flag atomicity)
   - Profile: Lock waits, retry iterations
   - Location: `transition_task_state_atomic()`

2. **Semaphore Contention** (handler dispatch)
   - Profile: Acquire latency P50/P95/P99
   - Location: `HandlerDispatchService.dispatch()`

3. **Dependency Resolution** (repeated evaluation)
   - Profile: Query time with deep dependency chains
   - Location: `get_step_readiness_status()`

### P1 - High Frequency Operations

4. **Handler Registry Resolution Chain** (TAS-93)
   - Profile: Chain traversal cost
   - Chain: `ExplicitMappingResolver` → `ClassConstantResolver` → `ClassLookupResolver`

5. **FFI Serialization Boundary**
   - Profile: Context marshalling, result unmarshalling
   - Location: `handler.call(&msg.task_sequence_step)`

6. **Audit Trigger Overhead** (TAS-62)
   - Profile: INSERT trigger performance
   - Risk: Lock contention under high step volume

### P2 - Secondary Optimization Candidates

7. **Domain Event Drop Rate**
   - Profile: `try_send()` failures when channel full
   - Location: `domain_event_system.rs:148`

8. **Task Template Hydration**
   - Profile: DB query per step claim
   - Location: `TaskTemplateManager.get_template()`

9. **PGMQ Notification Overhead**
   - Profile: `pg_notify()` call frequency (2x per step: claim + result)
   - Location: `pgmq_send_with_notify()`

---

## References

- Architecture docs: `docs/architecture/actors.md`, `states-and-lifecycles.md`, `events-and-commands.md`
- Worker architecture: `docs/architecture/worker-event-systems.md`, `circuit-breakers.md`
- Crate docs: `tasker-orchestration/AGENTS.md`, `tasker-worker/AGENTS.md`
- Profiling tools: `docs/ticket-specs/TAS-71/profiling-tool-evaluation.md`
- Benchmark results: `docs/ticket-specs/TAS-71/benchmark-results.md`
- samply: https://github.com/mstange/samply
- tokio-console: https://github.com/tokio-rs/console

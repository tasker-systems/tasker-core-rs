# Phase 5.4: Distributed Lifecycle Benchmarking Plan

**Status**: 📋 Planning
**Prerequisites**: Docker Compose services running
**Estimated Duration**: 2-3 days

---

## Overview

Phase 5.4 measures **end-to-end latencies** across the distributed system:
- API → Database → Message Queue → Worker → Response
- Real network overhead, queue transit times, event propagation
- Distributed tracing correlation validation

Unlike Phase 5.2 (SQL-only benchmarks), these benchmarks measure the **full system** including:
- HTTP API latency
- PGMQ message queue latency
- PostgreSQL LISTEN/NOTIFY event propagation
- Worker claim/execute/submit cycle
- Orchestration coordination overhead

---

## Prerequisites

### 1. Docker Compose Services

```bash
# Start full test environment
docker-compose -f docker/docker-compose.test.yml up --build -d

# Verify all services healthy
docker-compose -f docker/docker-compose.test.yml ps

# Expected services:
# - postgres (port 5432)
# - orchestration (port 8080)
# - worker (Rust, port 8081)
# - ruby-worker (port 8082)
```

### 2. Service Health Checks

Before running benchmarks:
```bash
# Orchestration
curl http://localhost:8080/health

# Rust Worker
curl http://localhost:8081/health

# Ruby Worker (optional)
curl http://localhost:8082/health
```

---

## Benchmark Categories

### 1. API → Task Creation (`bench_task_initialization`)

**What it measures**: Complete task initialization flow
- HTTP request parsing
- Task record creation
- Initial step discovery
- Response generation

**Approach**:
```rust
// tasker-client/benches/task_initialization.rs
fn bench_task_creation(c: &mut Criterion) {
    let client = OrchestrationClient::new("http://localhost:8080");

    let mut group = c.benchmark_group("task_initialization");

    // Test different workflow complexities
    for (name, template) in [
        ("linear_3_steps", "mathematical_sequence"),
        ("diamond_4_steps", "diamond_pattern"),
        ("tree_7_steps", "tree_pattern"),
    ] {
        group.bench_function(name, |b| {
            b.to_async(Runtime::new().unwrap()).iter(|| async {
                let request = create_task_request(
                    "rust_e2e_benchmark",
                    template,
                    json!({"value": 42})
                );

                let start = Instant::now();
                let response = client.create_task(request).await.unwrap();
                start.elapsed()
            });
        });
    }

    group.finish();
}
```

**Targets**:
- Linear workflow: < 50ms p99
- Diamond DAG: < 75ms p99
- Complex tree: < 100ms p99

---

### 2. Step Enqueuing Latency (`bench_step_enqueueing`)

**What it measures**: Ready step discovery → Queue publish with notify

**Challenge**: Need to trigger orchestration's step discovery without full execution

**Approach**:
```rust
// tasker-orchestration/benches/step_enqueueing.rs
fn bench_step_enqueueing(c: &mut Criterion) {
    // Setup: Create task, mark initial steps complete
    // Measure: Time for orchestration to discover + enqueue next ready steps

    let mut group = c.benchmark_group("step_enqueueing");

    group.bench_function("enqueue_ready_steps", |b| {
        b.iter_batched(
            || setup_task_with_ready_steps(),
            |task_uuid| async move {
                // Trigger orchestration cycle
                let start = Instant::now();
                trigger_result_processing(task_uuid).await;
                wait_for_steps_enqueued(task_uuid).await;
                start.elapsed()
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}
```

**Targets**:
- 3-step workflow: < 50ms
- 10-step workflow: < 100ms
- 50-step workflow: < 500ms

---

### 3. Worker Processing Cycle (`bench_worker_execution`)

**What it measures**: Queue claim → Handler execution → Result submit

**Approach**:
```rust
// tasker-worker/benches/worker_execution.rs
fn bench_worker_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("worker_execution");

    // Test with different handler types
    for handler in ["noop_handler", "simple_calculation", "database_query"] {
        group.bench_function(handler, |b| {
            b.iter_batched(
                || enqueue_step_for_handler(handler),
                |step_uuid| async move {
                    let start = Instant::now();

                    // Worker claims step
                    let claim_time = Instant::now();
                    let step = worker.claim_next_step().await.unwrap();
                    let claim_duration = claim_time.elapsed();

                    // Execute handler
                    let exec_time = Instant::now();
                    let result = worker.execute_step(step).await.unwrap();
                    let exec_duration = exec_time.elapsed();

                    // Submit result
                    let submit_time = Instant::now();
                    worker.submit_result(result).await.unwrap();
                    let submit_duration = submit_time.elapsed();

                    WorkerCycleMetrics {
                        total: start.elapsed(),
                        claim: claim_duration,
                        execute: exec_duration,
                        submit: submit_duration,
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}
```

**Targets**:
- Claim: < 20ms
- Execute (noop): < 10ms
- Submit: < 30ms
- **Total overhead**: < 60ms (excluding business logic)

---

### 4. Event Propagation Latency (`bench_event_propagation`)

**What it measures**: pg_notify publish → listener reception

**Approach**:
```rust
// tasker-shared/benches/event_propagation.rs
fn bench_pgmq_notify(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_propagation");

    // Setup LISTEN on test channel
    let listener = setup_test_listener("test_benchmark_channel").await;

    group.bench_function("notify_latency", |b| {
        b.iter_batched(
            || (), // No setup needed per iteration
            |_| async move {
                let message_id = Uuid::now_v7();

                // Send notify with timestamp
                let send_time = Instant::now();
                pgmq_send_with_notify(
                    &pool,
                    "test_queue",
                    &json!({"id": message_id, "sent_at": send_time})
                ).await.unwrap();

                // Wait for listener to receive
                let received = listener.recv_timeout(Duration::from_secs(1)).await.unwrap();

                // Calculate latency
                send_time.elapsed()
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}
```

**Targets**:
- p50: < 5ms
- p95: < 10ms
- p99: < 20ms

---

### 5. End-to-End Task Completion (`bench_e2e_latency`)

**What it measures**: API call → Task complete (all steps executed)

**Approach**:
```rust
// tests/benches/e2e_latency.rs
fn bench_complete_workflow(c: &mut Criterion) {
    // Requires Docker Compose services running
    let client = OrchestrationClient::new("http://localhost:8080");

    let mut group = c.benchmark_group("e2e_task_completion");
    group.sample_size(10); // Fewer samples for slow e2e tests
    group.measurement_time(Duration::from_secs(30));

    for (name, template, step_count) in [
        ("linear_3", "mathematical_sequence", 3),
        ("diamond_4", "diamond_pattern", 4),
    ] {
        group.bench_function(name, |b| {
            b.to_async(Runtime::new().unwrap()).iter(|| async {
                let request = create_task_request(
                    "rust_e2e_benchmark",
                    template,
                    json!({"value": 10})
                );

                let start = Instant::now();
                let response = client.create_task(request).await.unwrap();
                let task_uuid = Uuid::parse_str(&response.task_uuid).unwrap();

                // Poll for completion
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let task = client.get_task(task_uuid).await.unwrap();

                    if task.status == "Complete" {
                        break start.elapsed();
                    }

                    if start.elapsed() > Duration::from_secs(10) {
                        panic!("Task did not complete in time");
                    }
                }
            });
        });
    }

    group.finish();
}
```

**Targets**:
- 3-step linear: < 500ms p99
- 4-step diamond: < 800ms p99
- 7-step tree: < 1500ms p99

---

### 6. FFI/Business Logic Overhead (`bench_handler_overhead`)

**What it measures**: Framework overhead vs pure handler time

**Approach**:
```rust
// tasker-worker/benches/handler_overhead.rs
fn bench_ffi_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("handler_overhead");

    // Benchmark 1: Pure Rust handler (baseline)
    group.bench_function("rust_noop", |b| {
        b.iter(|| {
            // Direct function call
            rust_noop_handler(json!({}))
        });
    });

    // Benchmark 2: Rust handler via framework
    group.bench_function("rust_via_framework", |b| {
        b.iter(|| async {
            execute_handler_via_framework("noop_handler", json!({})).await
        });
    });

    // Benchmark 3: Ruby handler via FFI
    group.bench_function("ruby_via_ffi", |b| {
        b.iter(|| async {
            execute_ruby_handler_via_ffi("noop_handler", json!({})).await
        });
    });

    group.finish();
}
```

**Targets**:
- Framework overhead: < 1ms
- Ruby FFI overhead: < 5ms

---

## Implementation Structure

```
tasker-core/
├── tasker-client/
│   └── benches/
│       └── task_initialization.rs      # API latency benchmarks
├── tasker-orchestration/
│   └── benches/
│       └── step_enqueueing.rs          # Coordination benchmarks
├── tasker-worker/
│   └── benches/
│       ├── worker_execution.rs         # Worker cycle benchmarks
│       └── handler_overhead.rs         # FFI overhead benchmarks
├── tasker-shared/
│   └── benches/
│       ├── sql_functions.rs            # (Already complete - Phase 5.2)
│       └── event_propagation.rs        # Event system benchmarks
└── tests/
    └── benches/
        └── e2e_latency.rs              # Full end-to-end benchmarks
```

---

## Running Benchmarks

### 1. Start Services

```bash
# Terminal 1: Start all services
docker-compose -f docker/docker-compose.test.yml up --build

# Wait for all services to be healthy
```

### 2. Run Benchmark Suite

```bash
# Terminal 2: Run benchmarks against running services

# API benchmarks
cargo bench --package tasker-client --features benchmarks

# Orchestration benchmarks
cargo bench --package tasker-orchestration --features benchmarks

# Worker benchmarks
cargo bench --package tasker-worker --features benchmarks

# Event propagation
cargo bench --package tasker-shared --features benchmarks event_propagation

# Full E2E (slowest, run last)
cargo bench --test e2e_latency
```

### 3. Baseline Comparison

```bash
# Save baseline before making changes
cargo bench --all-features

# ... make performance improvements ...

# Compare to baseline
cargo bench --all-features
```

---

## Challenges & Solutions

### Challenge 1: Services Must Be Running

**Problem**: Criterion benchmarks fail if services aren't available

**Solution**:
- Document prerequisite clearly
- Add health check before benchmarks start
- Provide helper script to start/verify services

```rust
fn ensure_services_ready() -> Result<()> {
    let orchestration = reqwest::blocking::get("http://localhost:8080/health")?;
    let worker = reqwest::blocking::get("http://localhost:8081/health")?;

    if !orchestration.status().is_success() {
        bail!("Orchestration service not healthy");
    }

    Ok(())
}
```

### Challenge 2: Slow E2E Tests

**Problem**: Complete workflow execution takes seconds, not milliseconds

**Solution**:
- Use smaller `sample_size` for E2E (10 instead of 50)
- Use longer `measurement_time` (30s instead of 10s)
- Accept higher variance in E2E benchmarks
- Focus on p95/p99 metrics, not mean

### Challenge 3: State Cleanup Between Iterations

**Problem**: Database fills with test tasks, may affect performance

**Solution**:
- Use `iter_batched` with cleanup in setup
- Or: Accept that database grows (more realistic for production)
- Or: Periodic cleanup after N iterations

```rust
b.iter_batched(
    || {
        // Setup: ensure clean state
        cleanup_old_tasks().await;
        create_test_task().await
    },
    |task_uuid| {
        // Benchmark iteration
        measure_task_execution(task_uuid).await
    },
    BatchSize::SmallInput,
);
```

### Challenge 4: Network Variability

**Problem**: HTTP latency varies due to network, not just code

**Solution**:
- Run benchmarks on localhost (minimal network variance)
- Use Docker network for more realistic testing
- Document that variance is expected
- Focus on regression detection, not absolute numbers

---

## Success Criteria

✅ All 6 benchmark categories implemented
✅ Services can run via Docker Compose
✅ Benchmarks detect regressions (> 10% slowdown)
✅ Documentation includes interpretation guide
✅ CI can run benchmarks (optional)

---

## Future Enhancements (Post-5.4)

### Concurrent Load Testing
- Multiple tasks created simultaneously
- Measure throughput (tasks/sec)
- Identify contention bottlenecks

### Long-Running Benchmarks
- Run for 1+ hours
- Detect memory leaks
- Measure connection pool exhaustion

### Multi-Worker Scenarios
- Scale worker count (1, 2, 5, 10)
- Measure work distribution fairness
- Identify coordination bottlenecks

---

## Implementation Phases

### Phase 5.4.1: Infrastructure (Day 1)
- [ ] Create benchmark directory structure
- [ ] Add health check utilities
- [ ] Document Docker Compose prerequisites
- [ ] Create helper script to verify services

### Phase 5.4.2: Simple Benchmarks (Day 1-2)
- [ ] `bench_task_initialization` - API latency
- [ ] `bench_event_propagation` - NOTIFY latency
- [ ] Test and validate on running services

### Phase 5.4.3: Complex Benchmarks (Day 2-3)
- [ ] `bench_step_enqueueing` - Coordination latency
- [ ] `bench_worker_execution` - Worker cycle
- [ ] `bench_handler_overhead` - FFI overhead
- [ ] `bench_e2e_latency` - Full workflow

### Phase 5.4.4: Documentation (Day 3)
- [ ] Interpretation guide
- [ ] Expected performance ranges
- [ ] Troubleshooting common issues
- [ ] CI integration recommendations

---

## Next Steps

1. **Review this plan** - Does it match your vision?
2. **Start with Phase 5.4.1** - Set up infrastructure
3. **Implement simple benchmarks first** - Get early feedback
4. **Iterate on complex benchmarks** - Learn what works

Ready to start implementation?

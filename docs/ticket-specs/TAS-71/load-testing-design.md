# TAS-71: Load Testing Framework Design

**Date:** 2026-01-20
**Status:** Design Phase
**Depends On:** Research Areas A (profiling tools), C (E2E benchmark design)

---

## Executive Summary

This document designs a load testing framework for measuring throughput, saturation points, and system behavior under sustained load. Unlike E2E benchmarks (which measure latency for individual workflows), load testing focuses on aggregate throughput and system stability under concurrent workloads.

---

## Goals

### Primary Goals

1. **Throughput measurement**: Tasks completed per second at various load levels
2. **Saturation detection**: Identify when adding more load stops improving throughput
3. **Bottleneck identification**: Determine which component saturates first (orchestration, workers, database, queue)
4. **Scaling validation**: Verify that horizontal scaling improves throughput linearly

### Secondary Goals

1. **Stability testing**: Verify system remains stable under sustained load
2. **Resource efficiency**: Monitor CPU/memory usage at different load levels
3. **Degradation patterns**: Understand how performance degrades past saturation

---

## Existing Foundation

From TAS-73, we have concurrent testing infrastructure:

```rust
// tests/e2e/multi_instance/concurrent_task_creation_test.rs

// Concurrent task creation
let responses = manager.create_tasks_concurrent(requests).await?;

// Burst with throughput measurement
let start = std::time::Instant::now();
let responses = manager.create_tasks_concurrent(requests).await?;
let creation_time = start.elapsed();
println!("Created {} tasks in {:?} ({:.1} tasks/sec)",
    burst_size, creation_time,
    burst_size as f64 / creation_time.as_secs_f64()
);
```

---

## Load Testing Scenarios

### Scenario 1: Ramp-Up Test

Gradually increase load to find the saturation point.

```
Load Level:  10 → 25 → 50 → 100 → 150 → 200 tasks/sec
Duration:    30s  30s  30s   30s   30s   30s
```

**Measurements per level:**
- Actual throughput achieved (tasks/sec)
- P95 latency
- Error rate
- Resource utilization

**Saturation indicator**: When actual throughput < target throughput.

### Scenario 2: Sustained Load Test

Maintain steady load over extended period.

```
Load Level:  80% of saturation point
Duration:    5 minutes
```

**Measurements:**
- Throughput stability (variance over time)
- Memory growth (leak detection)
- Connection pool stability
- Queue depth over time

### Scenario 3: Burst Load Test

Simulate sudden traffic spikes.

```
Base Load:   50 tasks/sec
Burst:       500 tasks in 1 second
Recovery:    Measure time to return to baseline
```

**Measurements:**
- Tasks queued during burst
- Queue drain time
- Error rate during burst
- Latency spike magnitude

### Scenario 4: Scaling Comparison Test

Compare throughput across different cluster configurations.

| Config | Orchestration | Workers | Target Comparison |
|--------|---------------|---------|-------------------|
| Baseline | 1 | 1 | N/A |
| Worker scale | 1 | 2, 4, 8 | 2x, 4x, 8x workers |
| Orch scale | 2 | 4 | vs 1 orch + 4 workers |
| Full scale | 2 | 8 | Maximum config |

**Key question**: Does throughput scale linearly with workers?

---

## Load Generation Architecture

### Option A: Custom Rust Load Generator

Build load generator as part of the test suite.

```rust
struct LoadGenerator {
    target_rate: f64,           // tasks/sec
    duration: Duration,
    cluster: BenchmarkCluster,
    pattern: WorkflowPattern,
}

impl LoadGenerator {
    /// Run load test and collect metrics
    async fn run(&self) -> LoadTestResult {
        let mut metrics = LoadTestMetrics::new();
        let interval = Duration::from_secs_f64(1.0 / self.target_rate);

        let start = Instant::now();
        while start.elapsed() < self.duration {
            let task_start = Instant::now();

            // Create task
            let result = self.cluster.create_task(self.pattern.clone()).await;

            // Track metrics
            match result {
                Ok(response) => {
                    metrics.record_success(task_start.elapsed());
                    // Spawn background completion tracker
                    tokio::spawn(track_completion(response.task_uuid));
                }
                Err(e) => metrics.record_failure(e),
            }

            // Rate limiting
            tokio::time::sleep(interval).await;
        }

        metrics.finalize()
    }
}
```

**Pros:**
- Full control over timing and patterns
- Native integration with our types
- No external dependencies

**Cons:**
- Must build and maintain
- May have its own bottlenecks

### Option B: External Load Testing Tool

Use established tools like `k6`, `goose`, or `drill`.

**k6 example:**
```javascript
import http from 'k6/http';
import { check } from 'k6';

export const options = {
    stages: [
        { duration: '30s', target: 50 },
        { duration: '1m', target: 100 },
        { duration: '30s', target: 0 },
    ],
};

export default function() {
    const payload = JSON.stringify({
        namespace: 'rust_e2e_linear',
        name: 'mathematical_sequence',
        context: { even_number: 6 }
    });

    const response = http.post('http://localhost:8080/v1/tasks', payload, {
        headers: { 'Content-Type': 'application/json' },
    });

    check(response, { 'status is 201': (r) => r.status === 201 });
}
```

**Pros:**
- Battle-tested load generation
- Rich reporting
- Standard tooling

**Cons:**
- External dependency
- Less control over task completion tracking
- Only measures creation, not completion

### Recommendation: Hybrid Approach

1. **Use custom Rust load generator** for throughput-to-completion metrics
2. **Use k6 or similar** for API stress testing (creation rate only)
3. **Build completion tracker** that can be used with either

---

## Metrics Collection

### Real-Time Metrics

```rust
struct LoadTestMetrics {
    // Creation metrics
    tasks_created: AtomicU64,
    creation_failures: AtomicU64,
    creation_latencies: Arc<Mutex<Vec<Duration>>>,

    // Completion metrics
    tasks_completed: AtomicU64,
    completion_failures: AtomicU64,
    completion_latencies: Arc<Mutex<Vec<Duration>>>,

    // System metrics (sampled periodically)
    cpu_samples: Arc<Mutex<Vec<f64>>>,
    memory_samples: Arc<Mutex<Vec<u64>>>,
    connection_samples: Arc<Mutex<Vec<u32>>>,
    queue_depth_samples: Arc<Mutex<Vec<u64>>>,
}

impl LoadTestMetrics {
    fn throughput(&self, duration: Duration) -> f64 {
        self.tasks_completed.load(Ordering::Relaxed) as f64 / duration.as_secs_f64()
    }

    fn summary(&self) -> LoadTestSummary {
        // Calculate percentiles, rates, etc.
    }
}
```

### Database Metrics (via pg_stat)

```sql
-- Connection pool utilization
SELECT count(*) FROM pg_stat_activity WHERE datname = 'tasker_rust_test';

-- Queue depth (PGMQ)
SELECT count(*) FROM pgmq.q_worker_<namespace>_queue;

-- Transaction rate
SELECT xact_commit + xact_rollback as total_txn FROM pg_stat_database;
```

### Queue Metrics

For PGMQ:
```sql
SELECT queue_name, queue_length, oldest_msg_age_sec
FROM pgmq.metrics_all();
```

For RabbitMQ:
```bash
rabbitmqctl list_queues name messages
```

---

## Output Format

### Console Summary

```
================================================================================
LOAD TEST RESULTS
================================================================================

Configuration:
  Pattern:        linear (4 steps)
  Target Rate:    100 tasks/sec
  Duration:       60s
  Cluster:        2 orchestration, 4 workers

Creation Metrics:
  Tasks Created:  5,847
  Success Rate:   99.8%
  Rate Achieved:  97.5 tasks/sec
  P50 Latency:    8.2ms
  P95 Latency:    15.4ms
  P99 Latency:    28.1ms

Completion Metrics:
  Tasks Completed: 5,821
  Success Rate:    99.6%
  Throughput:      97.0 tasks/sec
  P50 Latency:     142ms
  P95 Latency:     198ms
  P99 Latency:     256ms

System Metrics:
  Avg CPU:         45%
  Peak CPU:        78%
  Avg Memory:      512MB
  Peak Memory:     624MB
  Avg Connections: 28
  Peak Connections: 35
  Max Queue Depth: 127

Bottleneck Analysis:
  ⚠️  Queue depth spiked at 45s mark
  ✅ CPU headroom available
  ✅ Memory stable (no leaks detected)
================================================================================
```

### JSON Export

```json
{
  "test_id": "load-test-2026-01-20-143052",
  "config": {
    "pattern": "linear",
    "target_rate": 100,
    "duration_secs": 60,
    "orchestration_count": 2,
    "worker_count": 4
  },
  "creation": {
    "total": 5847,
    "success": 5835,
    "failure": 12,
    "rate_achieved": 97.5,
    "latency_p50_ms": 8.2,
    "latency_p95_ms": 15.4,
    "latency_p99_ms": 28.1
  },
  "completion": {
    "total": 5821,
    "success": 5798,
    "failure": 23,
    "throughput": 97.0,
    "latency_p50_ms": 142,
    "latency_p95_ms": 198,
    "latency_p99_ms": 256
  },
  "system": {
    "cpu_avg": 45,
    "cpu_peak": 78,
    "memory_avg_mb": 512,
    "memory_peak_mb": 624,
    "connections_avg": 28,
    "connections_peak": 35,
    "queue_depth_max": 127
  },
  "timeseries": {
    "interval_secs": 1,
    "throughput": [92, 95, 97, ...],
    "queue_depth": [12, 15, 23, ...]
  }
}
```

---

## Implementation Plan

### Phase 1: Core Load Generator

1. Create `tests/load/mod.rs` with load generator infrastructure
2. Implement `LoadGenerator` struct with configurable rate and duration
3. Add `LoadTestMetrics` for real-time collection
4. Create basic ramp-up test scenario

### Phase 2: Completion Tracking

1. Implement async completion tracker
2. Add timeout handling for stuck tasks
3. Track completion rate alongside creation rate

### Phase 3: System Metrics Integration

1. Add CPU/memory sampling (via sysinfo crate or /proc)
2. Add database connection monitoring
3. Add queue depth monitoring (PGMQ/RabbitMQ)

### Phase 4: Reporting

1. Console summary output
2. JSON export for analysis
3. Timeseries data for graphing
4. Comparison tooling (baseline vs current)

### Phase 5: cargo-make Integration

```toml
[tasks.load-test]
description = "Run load tests (requires services)"
script = '''
#!/bin/bash
set -a; source .env; set +a
cargo test --release --features test-services load_tests -- --nocapture
'''

[tasks.load-test-cluster]
description = "Run load tests against cluster"
dependencies = ["cluster-status"]
script = '''
#!/bin/bash
set -a; source .env; set +a
cargo test --release --features test-cluster load_tests_cluster -- --nocapture
'''

[tasks.load-test-ramp]
description = "Run ramp-up load test to find saturation"
script = '''
cargo run --release --bin load-test-ramp
'''
```

---

## Saturation Detection Algorithm

```rust
fn detect_saturation(results: &[LoadTestResult]) -> SaturationAnalysis {
    let mut analysis = SaturationAnalysis::new();

    for window in results.windows(2) {
        let prev = &window[0];
        let curr = &window[1];

        let rate_increase = curr.target_rate - prev.target_rate;
        let throughput_increase = curr.actual_throughput - prev.actual_throughput;

        // If throughput didn't increase proportionally, we're saturating
        let efficiency = throughput_increase / rate_increase;

        if efficiency < 0.5 {
            analysis.saturation_point = Some(prev.target_rate);
            analysis.bottleneck = identify_bottleneck(curr);
            break;
        }
    }

    analysis
}

fn identify_bottleneck(result: &LoadTestResult) -> Bottleneck {
    // Check which metric is most constrained
    if result.cpu_peak > 90 {
        Bottleneck::CPU
    } else if result.queue_depth_max > result.worker_count * 100 {
        Bottleneck::Queue
    } else if result.connections_peak > result.connection_pool_max * 0.9 {
        Bottleneck::Database
    } else {
        Bottleneck::Unknown
    }
}
```

---

## Expected Results

Based on system design and TAS-73 findings:

### Single Instance (1 orch + 1 worker)

| Pattern | Expected Throughput | Bottleneck |
|---------|---------------------|------------|
| Linear | 50-100 tasks/sec | Worker CPU |
| Diamond | 40-80 tasks/sec | Convergence overhead |

### Cluster (2 orch + 4 workers)

| Pattern | Expected Throughput | Expected Scaling |
|---------|---------------------|------------------|
| Linear | 150-300 tasks/sec | ~3x single |
| Diamond | 120-240 tasks/sec | ~3x single |
| Tree | 200-400 tasks/sec | Better parallelism |

### Scaling Efficiency

| Workers | Expected Efficiency | Notes |
|---------|---------------------|-------|
| 1→2 | 90%+ | Near-linear |
| 2→4 | 80-90% | Some contention |
| 4→8 | 70-80% | DB/queue limits |
| 8→16 | 50-70% | Diminishing returns |

---

## Success Criteria

1. **Framework functional**: Can run ramp-up, sustained, and burst tests
2. **Saturation detected**: Clear saturation point identified for default config
3. **Bottleneck identified**: Know which component saturates first
4. **Scaling measured**: Quantified scaling efficiency for 1→2→4 workers
5. **Reproducible**: Results within 15% across multiple runs

---

## Open Questions

1. **How long should sustained tests run?**
   - 5 minutes minimum for stability
   - 30+ minutes for leak detection
   - Recommendation: Configurable, default 5m

2. **Should we test failure injection?**
   - e.g., What happens when a worker dies under load?
   - Recommendation: Phase 2 after core framework works

3. **How do we avoid polluting the database?**
   - Tests create many tasks
   - Need cleanup strategy or dedicated test database
   - Recommendation: Database reset between test runs

---

## References

- TAS-73 concurrent tests: `tests/e2e/multi_instance/`
- E2E benchmark design: `docs/ticket-specs/TAS-71/e2e-benchmark-design.md`
- Cluster infrastructure: `cargo-make/scripts/multi-deploy/`
- Existing burst test: `test_rapid_task_creation_burst`

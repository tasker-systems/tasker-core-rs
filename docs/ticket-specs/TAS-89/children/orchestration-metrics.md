# TAS-89-5: Implement Real Orchestration Metrics

**Parent**: TAS-89 (Codebase Evaluation)
**Type**: Enhancement
**Priority**: Medium
**Effort**: 3-4 hours

---

## Summary

Orchestration metrics currently return hardcoded values, making capacity planning and monitoring impossible. Implement real metric queries to enable proper observability.

---

## Problem Statement

### Current State (Broken)

| Metric | Current Value | Should Be |
|--------|---------------|-----------|
| `active_processors` | 1 | Actual processor count |
| `pool_utilization` | 0.0 | Connection pool usage % |
| `request_queue_size` | -1 | Actual PGMQ queue depth |

### Impact

- **Load balancing decisions** based on wrong processor count
- **Database saturation** invisible (always 0%)
- **Request backlog** undetectable (always -1)
- **Capacity alerts** never fire

---

## Scope

### 1. Track Actual Active Processors

**Location**: `tasker-orchestration/src/orchestration/command_processor.rs:1135`

**Current**:
```rust
Ok(SystemHealth {
    active_processors: 1, // TODO: Track actual active processors
    // ...
})
```

**Approach**: Track processor spawns/completions with atomic counter.

```rust
pub struct CommandProcessor {
    active_processor_count: Arc<AtomicUsize>,
    // ...
}

impl CommandProcessor {
    fn spawn_processor(&self) {
        self.active_processor_count.fetch_add(1, Ordering::SeqCst);
        // ... spawn logic
    }

    fn processor_completed(&self) {
        self.active_processor_count.fetch_sub(1, Ordering::SeqCst);
    }

    fn active_processors(&self) -> usize {
        self.active_processor_count.load(Ordering::SeqCst)
    }
}
```

---

### 2. Implement Pool Utilization Query

**Location**: `tasker-orchestration/src/web/handlers/analytics.rs:198`

**Current**:
```rust
// TODO: Consider fetching from separate connection pool monitoring if needed
let pool_utilization = 0.0;
```

**Approach**: Query SQLx pool statistics.

```rust
fn pool_utilization(pool: &PgPool) -> f64 {
    let size = pool.size() as f64;
    let idle = pool.num_idle() as f64;

    if size > 0.0 {
        (size - idle) / size
    } else {
        0.0
    }
}
```

---

### 3. Implement Queue Size Query

**Location**: `tasker-orchestration/src/orchestration/lifecycle/task_request_processor.rs:297`

**Current**:
```rust
// TODO: Implement queue_size method in PgmqClient
Ok(TaskRequestProcessorStats {
    request_queue_size: -1,
    request_queue_name: self.config.request_queue_name.clone(),
})
```

**Approach**: Query PGMQ queue depth.

```rust
impl PgmqClient {
    pub async fn queue_size(&self, queue_name: &str) -> Result<i64> {
        let result = sqlx::query_scalar!(
            "SELECT count(*) FROM pgmq.q_$1",
            queue_name
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.unwrap_or(0))
    }
}

// In TaskRequestProcessor:
pub async fn get_statistics(&self) -> TaskerResult<TaskRequestProcessorStats> {
    let queue_size = self.pgmq_client
        .queue_size(&self.config.request_queue_name)
        .await
        .unwrap_or(-1);

    Ok(TaskRequestProcessorStats {
        request_queue_size: queue_size,
        request_queue_name: self.config.request_queue_name.clone(),
    })
}
```

---

## Files to Modify

| File | Change |
|------|--------|
| `tasker-orchestration/src/orchestration/command_processor.rs` | Track active processors |
| `tasker-orchestration/src/web/handlers/analytics.rs` | Query pool utilization |
| `tasker-orchestration/src/orchestration/lifecycle/task_request_processor.rs` | Query queue size |
| `tasker-pgmq/src/client.rs` or equivalent | Add `queue_size()` method if not exists |

---

## Acceptance Criteria

- [ ] `active_processors` reflects actual spawned processor count
- [ ] `pool_utilization` reflects actual connection pool usage (0.0 - 1.0)
- [ ] `request_queue_size` reflects actual PGMQ queue depth
- [ ] Metrics update as system state changes
- [ ] Tests verify metrics accuracy

---

## Testing

### Active Processors
```rust
#[tokio::test]
async fn active_processors_tracks_spawned_processors() {
    let processor = CommandProcessor::new(config).await;

    assert_eq!(processor.active_processors(), 0);

    // Spawn processors
    processor.spawn_processor();
    processor.spawn_processor();

    assert_eq!(processor.active_processors(), 2);

    // Complete one
    processor.processor_completed();

    assert_eq!(processor.active_processors(), 1);
}
```

### Pool Utilization
```rust
#[tokio::test]
async fn pool_utilization_reflects_connection_usage() {
    let pool = create_test_pool(max_connections: 10).await;

    // Initially low utilization
    let initial = pool_utilization(&pool);
    assert!(initial < 0.5);

    // Acquire connections
    let _conns: Vec<_> = (0..8)
        .map(|_| pool.acquire())
        .collect();

    // Should show high utilization
    let after = pool_utilization(&pool);
    assert!(after > 0.7);
}
```

### Queue Size
```rust
#[tokio::test]
async fn queue_size_reflects_actual_depth() {
    let client = PgmqClient::new(pool).await;
    let queue_name = "test_queue";

    // Empty queue
    assert_eq!(client.queue_size(queue_name).await.unwrap(), 0);

    // Add messages
    for i in 0..10 {
        client.send(queue_name, &TestMessage { id: i }).await.unwrap();
    }

    // Should show 10
    assert_eq!(client.queue_size(queue_name).await.unwrap(), 10);
}
```

---

## Risk Assessment

**Risk**: Low
- Adding metrics tracking has minimal performance overhead
- Pool and queue queries are lightweight
- No change to existing behavior, only adds measurement

# TAS-171: Cache Circuit Breaker and Dragonfly Migration

## Problem

Redis unavailability at runtime causes repeated timeout delays on every cache
operation. We handle startup failures gracefully (fall back to NoOp), but
runtime failures cause per-request latency spikes until Redis recovers.

Additionally, our test and CI infrastructure uses Redis, but Dragonfly offers
better performance with full Redis protocol compatibility.

## Goals

1. **Circuit breaker for cache operations**: Stop hitting a failing cache
   backend after N failures, fall back to NoOp behavior until recovery
2. **Dragonfly migration**: Switch test/CI infrastructure from Redis to
   Dragonfly for better performance

## Non-Goals

- Custom Dragonfly client (use Redis protocol compatibility)
- Cache-level retry logic (circuit breaker is fail-fast, not retry)
- Per-operation circuit breakers (global is sufficient)

## Design

### Circuit Breaker

Global circuit breaker wrapping all CacheProvider operations:

```
CacheProvider::get()
  → check circuit state
  → if OPEN: return None (cache miss), skip network call
  → if CLOSED/HALF_OPEN: attempt operation
  → on failure: record failure, maybe trip circuit
  → on success: record success, maybe close circuit
```

**Configuration:**
```toml
[common.circuit_breakers.component_configs.cache]
failure_threshold = 5      # Trip after 5 consecutive failures
timeout_seconds = 30       # Stay open for 30s before half-open probe
success_threshold = 2      # Close after 2 successes in half-open
```

**Behavior when open:**
- `get()` → returns `None` (cache miss)
- `set()` → no-op, returns `Ok(())`
- `delete()` → no-op, returns `Ok(())`
- Log state transitions at INFO level
- Log skipped operations at DEBUG level

**Integration with existing circuit breaker infrastructure:**

We already have `CircuitBreaker` in `tasker-shared/src/resilience/circuit_breaker.rs`
with the same pattern used for PGMQ and task readiness. The cache circuit breaker
should use the same infrastructure for consistency.

### Dragonfly Migration

Dragonfly is Redis protocol-compatible, so no client changes needed. We use
the existing `redis-rs` crate which works with both Redis and Dragonfly.

**Changes:**

1. Add `"dragonfly"` as config alias for Redis provider (alongside existing
   `"redis"` value) - both route to the same `RedisCacheService`
2. Update `docker/docker-compose.test.yml`: replace redis image with dragonfly
3. Update CI workflows to use dragonfly image
4. Document in `docs/guides/caching.md`

**Why Dragonfly:**

- Drop-in Redis replacement (same protocol, same client)
- Better multi-threaded performance
- Lower memory usage for same workload
- Active development and good Kubernetes support

## Files to Change

| File | Change |
|------|--------|
| `tasker-shared/src/cache/provider.rs` | Wrap operations with circuit breaker |
| `tasker-shared/src/cache/mod.rs` | Export circuit breaker state for health checks |
| `config/tasker/base/common.toml` | Add cache circuit breaker config |
| `config/tasker/*.toml` | Add cache circuit breaker to generated configs |
| `docker/docker-compose.test.yml` | Switch redis → dragonfly |
| `.github/workflows/*.yml` | Update CI service images |
| `docs/guides/caching.md` | Document Dragonfly, circuit breaker behavior |

## Implementation Notes

### Circuit Breaker Integration

The `CacheProvider` already wraps operations with error handling. The circuit
breaker should be checked at the top of each operation:

```rust
impl CacheProvider {
    pub async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        // Check circuit breaker first
        if self.circuit_breaker.is_open() {
            debug!(key = key, "Cache circuit open, returning miss");
            return Ok(None);
        }

        let result = match self {
            Self::Redis(s) => s.get(key).await,
            // ...
        };

        // Record success/failure
        match &result {
            Ok(_) => self.circuit_breaker.record_success(),
            Err(_) => self.circuit_breaker.record_failure(),
        }

        result
    }
}
```

### Dragonfly Docker Image

```yaml
# docker/docker-compose.test.yml
services:
  cache:
    image: docker.dragonflydb.io/dragonflydb/dragonfly:latest
    ports:
      - "6379:6379"
    # Dragonfly uses same port and protocol as Redis
```

## Acceptance Criteria

- [ ] Cache operations fail-fast when circuit is open (no network call)
- [ ] Circuit trips after configurable failure threshold
- [ ] Circuit recovers automatically after timeout + successful probes
- [ ] Circuit state visible in health endpoint
- [ ] `"dragonfly"` config alias works identically to `"redis"`
- [ ] Local tests run against Dragonfly
- [ ] CI runs against Dragonfly
- [ ] Existing Redis deployments continue working (no breaking changes)
- [ ] Documentation updated

## Testing

### Unit Tests
- Circuit breaker state machine logic (existing tests in circuit_breaker.rs)
- CacheProvider respects circuit state

### Integration Tests
- Simulate cache failure, verify circuit trips
- Verify circuit recovery after timeout
- Verify Dragonfly compatibility with existing cache tests

### Manual Testing
- Start system with Redis/Dragonfly
- Kill cache service, observe circuit trip
- Restart cache service, observe circuit recovery

## Rollout

1. Add circuit breaker to cache (backwards compatible, default config)
2. Add `"dragonfly"` alias (backwards compatible)
3. Update local docker-compose to Dragonfly
4. Update CI to Dragonfly
5. Document migration path for users

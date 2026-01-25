# Response caching for analytics and template endpoints (Redis-aware)

## Context

Analytics endpoints (`/v1/analytics/performance`, `/v1/analytics/bottlenecks`) execute non-trivial aggregation queries that don't change frequently. Under load, these become a source of unnecessary database pressure and contribute to flaky test behavior (500s from circuit breaker trips during concurrent test runs).

## Requirements

### Analytics Response Caching

* Add time-based response caching for analytics endpoints (30-60s TTL suggested)
* Cache key should incorporate query parameters (time range, namespace filters)
* Cache invalidation on relevant task/step state transitions (or rely purely on TTL)

### Architecture: Redis-Aware Caching Layer

* If Redis is available, use it as the cache backend so multiple orchestration instances share cached results
* If Redis is not available, fall back to in-process caching (e.g., `moka` or `dashmap` with TTL)
* This same caching layer should be reusable for future task template caching needs

### Configuration

```toml
[orchestration.cache]
backend = "redis"  # or "in-process"
redis_url = "redis://localhost:6379"
default_ttl_seconds = 60

[orchestration.cache.analytics]
ttl_seconds = 30  # override per-resource
```

### Future Alignment

* Task template caching will follow the same pattern (Redis for multi-instance, in-process fallback)
* Design the cache trait/interface to be generic: `CacheBackend::get<T>()`, `CacheBackend::set<T>(ttl)`
* Consider cache warming strategies for analytics on startup

## Files

* `tasker-orchestration/src/web/handlers/analytics.rs` — current TODO location
* New: caching service/trait in `tasker-shared` (shared between orchestration and worker)

## Acceptance Criteria

- [ ] Analytics endpoints return cached responses within TTL window
- [ ] Redis backend works for multi-instance deployments
- [ ] In-process fallback works when Redis is unavailable
- [ ] Cache trait is generic enough for template caching reuse
- [ ] Configuration via TOML (backend selection, TTL per resource)
- [ ] Existing tests pass (analytics flakiness reduced)

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-168/response-caching-for-analytics-and-template-endpoints-redis-aware](https://linear.app/tasker-systems/issue/TAS-168/response-caching-for-analytics-and-template-endpoints-redis-aware)
- Identifier: TAS-168
- Status: In Progress
- Priority: Medium
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Rust](https://linear.app/tasker-systems/project/tasker-core-rust-9b5a1c23b7b1). Alpha version of the Tasker Core in Rust
- Created: 2026-01-24T14:30:31.573Z
- Updated: 2026-01-25T11:32:39.062Z

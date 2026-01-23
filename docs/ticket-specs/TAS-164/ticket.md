# Connection Pool Observability and Tuning

## Summary

Add connection pool utilization metrics and evaluate tuning based on observed patterns. Current profiling shows 4.6% of worker time in database operations, with pool acquire contributing measurably.

**Impact:** Better capacity planning, potential latency reduction

## Profiling Evidence

| Operation | % of Total | Samples |
| -- | -- | -- |
| Connection waiting | 1.32% | 587 |
| Query execution | 1.28% | 569 |
| Pool acquire | 0.85% | 376 |
| Stream recv | 1.18% | 524 |

**Total database overhead: \~4.6%** of worker time

## Current Configuration

```toml
[common.database.pool]
max_connections = 25
min_connections = 5
acquire_timeout_seconds = 10
idle_timeout_seconds = 300
max_lifetime_seconds = 1800

[common.pgmq_database.pool]
max_connections = 15
min_connections = 3
acquire_timeout_seconds = 5
```

## Implementation

### 1\. Add Pool Metrics Endpoint

* Expose pool utilization via `/health` or `/metrics`
* Track: active connections, idle connections, acquire latency P50/P95/P99

### 2\. SQLx Event Handlers

* Implement connection event callbacks
* Log/metric slow connection acquisitions (>100ms threshold)

### 3\. Tuning Recommendations (after metrics collection)

* If acquire waits common: increase `max_connections`
* If connections frequently expire: adjust `max_lifetime`
* If many idle connections: reduce `min_connections`

### 4\. Config-Driven Pool Sizing

* Add environment-specific overrides in `config/tasker/environments/`
* Consider separate pool sizes for orchestration vs worker

## Files to Modify

* `tasker-shared/src/database/pools.rs`
* `config/tasker/base/common.toml`
* `config/tasker/environments/*/common.toml`
* Health endpoint handlers

## Acceptance Criteria

- [ ] Pool utilization metrics exposed in health/metrics endpoint
- [ ] Slow acquisition logging (>100ms threshold)
- [ ] Document tuning guidelines based on observed metrics
- [ ] Add pool configuration to environment overrides

## References

* Profiling Report: `docs/ticket-specs/TAS-71/profiling-report.md`
* Optimization Tickets: `docs/ticket-specs/TAS-71/optimization-tickets.md`

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-164/connection-pool-observability-and-tuning](https://linear.app/tasker-systems/issue/TAS-164/connection-pool-observability-and-tuning)
- Identifier: TAS-164
- Status: In Progress
- Priority: Medium
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Rust](https://linear.app/tasker-systems/project/tasker-core-rust-9b5a1c23b7b1). Alpha version of the Tasker Core in Rust
- Related issues: TAS-71
- Created: 2026-01-22T13:25:18.123Z
- Updated: 2026-01-23T13:45:15.914Z

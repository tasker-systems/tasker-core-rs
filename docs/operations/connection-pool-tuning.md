# Connection Pool Tuning Guide (TAS-164)

## Overview

Tasker maintains two connection pools: **tasker** (task/step/transition operations) and **pgmq** (queue operations). Pool observability is provided via:

- `/health/detailed` - Pool utilization in `pool_utilization` field
- `/metrics` - Prometheus gauges `tasker_db_pool_connections{pool,state}`
- Atomic counters tracking acquire latency and errors

## Pool Sizing Guidelines

### Formula

```
max_connections = (peak_concurrent_operations * avg_hold_time_ms) / 1000 + headroom
```

Rules of thumb:
- **Orchestration pool**: 2-3x the number of concurrent tasks expected
- **PGMQ pool**: 1-2x the number of workers × batch size
- **min_connections**: 20-30% of max to avoid cold-start latency
- Never exceed PostgreSQL's `max_connections / number_of_services`

### Environment Defaults

| Parameter | Base | Production | Development | Test |
|-----------|------|------------|-------------|------|
| `max_connections` (tasker) | 25 | 50 | 25 | 30 |
| `min_connections` (tasker) | 5 | 10 | 5 | 2 |
| `max_connections` (pgmq) | 15 | 25 | 15 | 10 |
| `slow_acquire_threshold_ms` | 100 | 50 | 200 | 500 |

## Metrics Interpretation

### Utilization Thresholds

| Level | Utilization | Action |
|-------|------------|--------|
| Healthy | < 80% | Normal operation |
| Degraded | 80-95% | Monitor closely, consider increasing `max_connections` |
| Unhealthy | > 95% | Pool exhaustion imminent; increase pool size or reduce load |

### Slow Acquires

The `slow_acquire_threshold_ms` setting controls when an acquire is classified as "slow":
- **Production (50ms)**: Tight threshold for SLO-sensitive workloads
- **Development (200ms)**: Relaxed for local debugging with fewer resources
- **Test (500ms)**: Very relaxed for CI environments with contention

A high `slow_acquires` count relative to `total_acquires` (>5%) suggests:
1. Pool is undersized for the workload
2. Connections are held too long (long queries or transactions)
3. Connection creation is slow (network latency to DB)

### Acquire Errors

Non-zero `acquire_errors` indicates pool exhaustion (timeout waiting for connection). Remediation:
1. Increase `max_connections`
2. Increase `acquire_timeout_seconds` (masks the problem)
3. Reduce query execution time
4. Check for connection leaks (connections not returned to pool)

## PostgreSQL Server-Side Considerations

### max_connections

PostgreSQL's `max_connections` is a hard limit across all clients. For cluster deployments:

```
pg_max_connections >= sum(service_max_pool * service_instance_count) + superuser_reserved
```

Default PostgreSQL `max_connections` is 100. For production:
- Set `max_connections = 500` or higher
- Reserve 5-10 connections for superuser (`superuser_reserved_connections`)
- Monitor with `SELECT count(*) FROM pg_stat_activity`

### Connection Overhead

Each PostgreSQL connection consumes ~5-10MB RAM. Size accordingly:
- 100 connections ~ 0.5-1GB additional RAM
- 500 connections ~ 2.5-5GB additional RAM

### Statement Timeout

The `statement_timeout` database variable protects against runaway queries:
- Production: 30s (default)
- Test: 5s (fail fast)

## Alert Threshold Recommendations

| Metric | Warning | Critical |
|--------|---------|----------|
| Pool utilization | > 80% for 5 min | > 95% for 1 min |
| Slow acquires / total | > 5% over 5 min | > 20% over 1 min |
| Acquire errors | > 0 in 5 min | > 10 in 1 min |
| Average acquire time | > 50ms | > 200ms |

## Configuration Reference

Pool settings are in `config/tasker/base/common.toml` under `[common.database.pool]` and `[common.pgmq_database.pool]`. Environment-specific overrides are in `config/tasker/environments/{env}/common.toml`.

```toml
[common.database.pool]
max_connections = 25
min_connections = 5
acquire_timeout_seconds = 10
idle_timeout_seconds = 300
max_lifetime_seconds = 1800
slow_acquire_threshold_ms = 100
```

# Caching Guide

This guide covers Tasker's distributed caching system, including configuration,
backend selection, and operational considerations.

## Overview

Tasker provides optional caching for:

- **Task Templates**: Reduces database queries when loading workflow definitions
- **Analytics**: Caches performance metrics and bottleneck analysis results

Caching is disabled by default and must be explicitly enabled in configuration.

## Configuration

### Basic Setup

```toml
[common.cache]
enabled = true
backend = "redis"              # or "moka" / "memory" / "in-memory"
default_ttl_seconds = 3600     # 1 hour default
template_ttl_seconds = 3600    # 1 hour for templates
analytics_ttl_seconds = 60     # 1 minute for analytics
key_prefix = "tasker"          # Namespace for cache keys

[common.cache.redis]
url = "${REDIS_URL:-redis://localhost:6379}"
max_connections = 10
connection_timeout_seconds = 5
database = 0

[common.cache.moka]
max_capacity = 10000           # Maximum entries in cache
```

### Backend Selection

| Backend | Config Value | Use Case |
|---------|--------------|----------|
| Redis | `"redis"` | Multi-instance deployments (production) |
| Moka | `"moka"`, `"memory"`, `"in-memory"` | Single-instance, development, DoS protection |
| NoOp | (enabled = false) | Disabled, always-miss |

## Cache Backends

### Redis (Distributed)

Redis is the recommended backend for production deployments:

- **Shared state**: All instances see the same cache entries
- **Invalidation works**: Worker bootstrap invalidations propagate to all instances
- **Persistence**: Survives process restarts (if Redis is configured for persistence)

```toml
[common.cache]
enabled = true
backend = "redis"

[common.cache.redis]
url = "redis://redis.internal:6379"
```

### Moka (In-Memory)

Moka provides a high-performance in-memory cache:

- **Zero network latency**: All operations are in-process
- **DoS protection**: Rate-limits expensive operations without Redis dependency
- **Single-instance only**: Cache is not shared across processes

```toml
[common.cache]
enabled = true
backend = "moka"

[common.cache.moka]
max_capacity = 10000
```

**Important**: Moka is only suitable for:
- Single-instance deployments
- Development environments
- Analytics caching (where brief staleness is acceptable)

### NoOp (Disabled)

When caching is disabled or a backend fails to initialize:

```toml
[common.cache]
enabled = false
```

The NoOp provider always returns cache misses and succeeds on writes (no-op).
This is also used as a graceful fallback when Redis connection fails.

## Usage Context Constraints

Different caching use cases have different consistency requirements. Tasker
enforces these constraints at runtime:

### Template Caching

**Constraint**: Requires distributed cache (Redis) or no cache (NoOp)

Templates are cached to avoid repeated database queries when loading workflow
definitions. However, workers invalidate the template cache on bootstrap when
they register new handler versions.

If an in-memory cache (Moka) is used:
1. Orchestration server caches templates in its local memory
2. Worker boots and invalidates templates in Redis (or nowhere, if Moka)
3. Orchestration server never sees the invalidation
4. Stale templates are served → operational errors

**Behavior with Moka**: Template caching is automatically disabled with a warning:

```
WARN Cache provider 'moka' is not safe for template caching (in-memory cache
     would drift from worker invalidations). Template caching disabled.
```

### Analytics Caching

**Constraint**: Any backend allowed

Analytics data is informational and TTL-bounded. Brief staleness is acceptable,
and in-memory caching provides DoS protection for expensive aggregation queries.

**Behavior with Moka**: Analytics caching works normally.

## Cache Keys

Cache keys are prefixed with the configured `key_prefix` to allow multiple
Tasker deployments to share a Redis instance:

| Resource | Key Pattern |
|----------|-------------|
| Templates | `{prefix}:template:{namespace}:{name}:{version}` |
| Performance Metrics | `{prefix}:analytics:performance:{hours}` |
| Bottleneck Analysis | `{prefix}:analytics:bottlenecks:{limit}:{min_executions}` |

## Operational Patterns

### Multi-Instance Production

```toml
[common.cache]
enabled = true
backend = "redis"
template_ttl_seconds = 3600    # Long TTL, rely on invalidation
analytics_ttl_seconds = 60     # Short TTL for fresh data
```

- Templates cached for 1 hour but invalidated on worker registration
- Analytics cached briefly to reduce database load

### Single-Instance / Development

```toml
[common.cache]
enabled = true
backend = "moka"
template_ttl_seconds = 300     # Shorter TTL since no invalidation
analytics_ttl_seconds = 30
```

- Template caching automatically disabled (Moka constraint)
- Analytics caching works, provides DoS protection

### Caching Disabled

```toml
[common.cache]
enabled = false
```

- All cache operations are no-ops
- Every request hits the database
- Useful for debugging or when cache adds complexity without benefit

## Graceful Degradation

Tasker never fails to start due to cache issues:

1. **Redis connection failure**: Falls back to NoOp with warning
2. **Backend misconfiguration**: Falls back to NoOp with warning
3. **Cache operation errors**: Logged as warnings, never propagated

```
WARN Failed to connect to Redis, falling back to NoOp cache (graceful degradation)
```

The cache layer uses "best-effort" writes—failures are logged but never block
request processing.

## Monitoring

### Cache Hit/Miss Rates

Cache operations are logged at `DEBUG` level:

```
DEBUG hours=24 "Performance metrics cache HIT"
DEBUG hours=24 "Performance metrics cache MISS, querying DB"
```

### Provider Status

On startup, the active cache provider is logged:

```
INFO backend="redis" "Distributed cache provider initialized successfully"
INFO backend="moka" max_capacity=10000 "In-memory cache provider initialized"
INFO "Distributed cache disabled by configuration"
```

## Troubleshooting

### Templates Not Caching

1. Check if backend is Moka—template caching is disabled with Moka
2. Check for Redis connection warnings in logs
3. Verify `enabled = true` in configuration

### Stale Templates Being Served

1. Verify all instances point to the same Redis
2. Check that workers are properly invalidating on bootstrap
3. Consider reducing `template_ttl_seconds`

### High Cache Miss Rate

1. Check Redis connectivity and latency
2. Verify TTL settings aren't too aggressive
3. Check for cache key collisions (multiple deployments, same prefix)

### Memory Growth with Moka

1. Reduce `max_capacity` setting
2. Check TTL settings—items evict on TTL or capacity limit
3. Monitor entry count if metrics are available

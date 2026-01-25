# TAS-168: Response Caching for Analytics Endpoints

## Overview

This plan adds response caching to analytics endpoints while addressing architectural debt. The implementation includes:

1. **Service layer extraction** - Move analytics logic from handlers into proper services
2. **In-memory cache provider** - Add Moka-based cache for single-instance deployments
3. **Type-safe cache constraints** - Prevent templates from using in-memory cache
4. **Analytics response caching** - Cache performance and bottleneck results

## Current State Analysis

### Existing Cache Infrastructure (TAS-156)

| Component | Location | Status |
|-----------|----------|--------|
| `CacheService` trait | `tasker-shared/src/cache/traits.rs` | ✅ Complete |
| `CacheProvider` enum | `tasker-shared/src/cache/provider.rs` | ✅ Complete |
| `RedisCacheService` | `tasker-shared/src/cache/providers/redis.rs` | ✅ Complete |
| `NoOpCacheService` | `tasker-shared/src/cache/providers/noop.rs` | ✅ Complete |
| Configuration | `config/tasker/base/common.toml` | ✅ Complete |
| Template caching | `tasker-shared/src/registry/task_handler_registry.rs` | ✅ Complete |

### Problem: Fat Controller Anti-Pattern

The current analytics handlers (`analytics.rs`) violate separation of concerns:

```
Handler responsibilities (current):
├── Request validation ✓
├── Permission checking ✓
├── DB query execution ✗ (should be in service)
├── Complex aggregation logic ✗ (should be in service)
├── Recommendation generation ✗ (should be in service)
└── Response formatting ✓
```

The bottleneck handler alone contains:
- 3 parallel DB queries
- Step aggregation by name with duration/error statistics
- Task aggregation with step count averaging
- Resource utilization calculation
- Recommendation generation based on thresholds

This logic should be in a service layer.

### Problem: No In-Memory Cache Option

Current cache backends:
- **Redis** - Distributed, requires external infrastructure
- **NoOp** - Always miss, no caching benefit

Missing: In-process cache for single-instance deployments or as DoS protection layer.

---

## Architecture Design

### Service Layer Structure

```
┌─────────────────────────────────────────────────────────────┐
│                      Analytics Handler                       │
│  (Request validation, permission check, response formatting) │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     AnalyticsService                         │
│        (Cache-aside pattern, TTL management)                 │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              AnalyticsCacheProvider                  │    │
│  │    (In-memory OR Redis, type-constrained)           │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                  AnalyticsQueryService                       │
│      (DB queries, aggregation, recommendations)              │
└─────────────────────────────────────────────────────────────┘
```

### Type-Safe Cache Constraints

To prevent templates from using in-memory cache (which would drift from worker invalidations), we introduce a type-safe constraint system:

```rust
/// Cache usage context - determines which backends are valid
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheUsageContext {
    /// Templates require distributed cache (Redis) or no cache (NoOp)
    /// In-memory would drift when workers invalidate their local caches
    Templates,

    /// Analytics can use any backend including in-memory
    /// Data is informational and TTL-bounded, drift is acceptable
    Analytics,

    /// Generic caching with no constraints
    Generic,
}

/// Type-safe cache provider wrapper that enforces usage constraints
pub struct ConstrainedCacheProvider {
    inner: Arc<CacheProvider>,
    context: CacheUsageContext,
}

impl ConstrainedCacheProvider {
    /// Create a constrained provider. Returns None if backend is invalid for context.
    pub fn new(provider: Arc<CacheProvider>, context: CacheUsageContext) -> Option<Self> {
        if Self::is_valid_for_context(&provider, context) {
            Some(Self { inner: provider, context })
        } else {
            None
        }
    }

    fn is_valid_for_context(provider: &CacheProvider, context: CacheUsageContext) -> bool {
        match context {
            CacheUsageContext::Templates => {
                // Templates can only use Redis or NoOp, never in-memory
                matches!(provider, CacheProvider::Redis(_) | CacheProvider::NoOp(_))
            }
            CacheUsageContext::Analytics | CacheUsageContext::Generic => {
                // No restrictions
                true
            }
        }
    }
}
```

### Cache Provider Enum (Extended)

```rust
#[derive(Debug, Clone)]
pub enum CacheProvider {
    #[cfg(feature = "cache-redis")]
    Redis(Box<RedisCacheService>),

    #[cfg(feature = "cache-moka")]
    Moka(Box<MokaCacheService>),

    NoOp(NoOpCacheService),
}

impl CacheProvider {
    /// Check if this provider is distributed (safe for multi-instance)
    pub fn is_distributed(&self) -> bool {
        match self {
            #[cfg(feature = "cache-redis")]
            Self::Redis(_) => true,
            #[cfg(feature = "cache-moka")]
            Self::Moka(_) => false,  // In-process only
            Self::NoOp(_) => true,   // NoOp is "safe" (no state)
        }
    }

    /// Check if this provider actually caches (not NoOp)
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::NoOp(_))
    }
}
```

---

## Implementation Plan

### Phase 1: Add Moka In-Memory Cache Provider

**New file**: `tasker-shared/src/cache/providers/moka.rs`

```rust
use moka::future::Cache;
use std::time::Duration;
use crate::cache::{CacheResult, CacheService};

pub struct MokaCacheService {
    cache: Cache<String, String>,
}

impl MokaCacheService {
    pub fn new(max_capacity: u64, default_ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(default_ttl)
            .build();
        Self { cache }
    }
}

impl CacheService for MokaCacheService {
    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        Ok(self.cache.get(key).await)
    }

    async fn set(&self, key: &str, value: &str, ttl: Duration) -> CacheResult<()> {
        // Moka supports per-entry TTL via policy
        self.cache.insert(key.to_string(), value.to_string()).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> CacheResult<()> {
        self.cache.invalidate(key).await;
        Ok(())
    }

    async fn delete_pattern(&self, pattern: &str) -> CacheResult<u64> {
        // Moka doesn't support pattern iteration efficiently
        // For analytics use case, we rely on TTL expiry
        // Log warning and return 0
        tracing::debug!(pattern, "Pattern deletion not supported for Moka, relying on TTL");
        Ok(0)
    }

    async fn health_check(&self) -> CacheResult<bool> {
        Ok(true)  // In-memory is always healthy
    }

    fn provider_name(&self) -> &'static str {
        "moka"
    }
}
```

**Updates to** `tasker-shared/Cargo.toml`:

```toml
[dependencies]
moka = { version = "0.12", optional = true, features = ["future"] }

[features]
default = ["cache-redis", "cache-moka", "postgres", "test-utils", "web-api"]
cache-moka = ["moka"]
```

**Updates to** `tasker-shared/src/cache/provider.rs`:

Add Moka variant and update `from_config_graceful()`:

```rust
match config.backend.as_str() {
    "redis" => Self::create_redis_provider(config).await,
    "moka" | "memory" | "in-memory" => Self::create_moka_provider(config),
    other => {
        warn!(backend = other, "Unknown cache backend, falling back to NoOp");
        Self::NoOp(NoOpCacheService::new())
    }
}
```

**Configuration additions** to `CacheConfig`:

```rust
pub struct MokaConfig {
    /// Maximum number of entries (default: 10000)
    pub max_capacity: u64,
}
```

### Phase 2: Add Cache Usage Context System

**New file**: `tasker-shared/src/cache/constraints.rs`

Contains:
- `CacheUsageContext` enum
- `ConstrainedCacheProvider` wrapper
- Validation logic

**Update** `TaskHandlerRegistry` to use constrained provider:

```rust
impl TaskHandlerRegistry {
    pub fn with_system_context(context: Arc<SystemContext>) -> Self {
        // Validate that cache provider is valid for templates
        let cache_provider = ConstrainedCacheProvider::new(
            context.cache_provider.clone(),
            CacheUsageContext::Templates,
        );

        if cache_provider.is_none() && context.cache_provider.is_enabled() {
            warn!(
                "Cache provider {} is not valid for template caching (in-memory cache \
                 would drift from worker invalidations). Templates will not be cached.",
                context.cache_provider.provider_name()
            );
        }

        Self {
            cache_provider,
            // ...
        }
    }
}
```

### Phase 3: Create AnalyticsQueryService

**New file**: `tasker-orchestration/src/services/analytics_query_service.rs`

Responsibilities:
- Execute DB queries via `SqlFunctionExecutor`
- Aggregate step/task data
- Calculate resource utilization
- Generate recommendations

```rust
pub struct AnalyticsQueryService {
    db_pool: PgPool,
}

impl AnalyticsQueryService {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Fetch and compute performance metrics
    pub async fn get_performance_metrics(&self, hours: u32) -> TaskerResult<PerformanceMetrics> {
        let executor = SqlFunctionExecutor::new(self.db_pool.clone());
        let since = if hours > 0 {
            Some(Utc::now() - Duration::hours(hours as i64))
        } else {
            None
        };

        let (analytics, health) = tokio::try_join!(
            executor.get_analytics_metrics(since),
            executor.get_system_health_counts()
        )?;

        Ok(PerformanceMetrics {
            total_tasks: health.total_tasks,
            active_tasks: analytics.active_tasks_count,
            // ... rest of mapping
        })
    }

    /// Fetch and compute bottleneck analysis
    pub async fn get_bottleneck_analysis(
        &self,
        limit: i32,
        min_executions: i32,
    ) -> TaskerResult<BottleneckAnalysis> {
        let executor = SqlFunctionExecutor::new(self.db_pool.clone());

        let (slow_steps, slow_tasks, health) = tokio::try_join!(
            executor.get_slowest_steps(Some(limit), Some(min_executions)),
            executor.get_slowest_tasks(Some(limit), Some(min_executions)),
            executor.get_system_health_counts()
        )?;

        // Aggregate steps by name
        let slow_step_infos = self.aggregate_slow_steps(slow_steps);

        // Aggregate tasks by name
        let slow_task_infos = self.aggregate_slow_tasks(slow_tasks);

        // Calculate resource utilization
        let resource_utilization = self.calculate_resource_utilization(&health);

        // Generate recommendations
        let recommendations = self.generate_recommendations(
            &slow_step_infos,
            &resource_utilization,
        );

        Ok(BottleneckAnalysis {
            slow_steps: slow_step_infos,
            slow_tasks: slow_task_infos,
            resource_utilization,
            recommendations,
        })
    }

    // Private helper methods for aggregation logic (moved from handler)
    fn aggregate_slow_steps(&self, steps: Vec<SlowestStepAnalysis>) -> Vec<SlowStepInfo> { ... }
    fn aggregate_slow_tasks(&self, tasks: Vec<SlowestTaskAnalysis>) -> Vec<SlowTaskInfo> { ... }
    fn calculate_resource_utilization(&self, health: &SystemHealthCounts) -> ResourceUtilization { ... }
    fn generate_recommendations(&self, steps: &[SlowStepInfo], util: &ResourceUtilization) -> Vec<String> { ... }
}
```

### Phase 4: Create AnalyticsService (Cache Wrapper)

**New file**: `tasker-orchestration/src/services/analytics_service.rs`

```rust
pub struct AnalyticsService {
    query_service: AnalyticsQueryService,
    cache_provider: Arc<CacheProvider>,
    cache_config: Option<CacheConfig>,
}

impl AnalyticsService {
    pub fn new(
        db_pool: PgPool,
        cache_provider: Arc<CacheProvider>,
        cache_config: Option<CacheConfig>,
    ) -> Self {
        Self {
            query_service: AnalyticsQueryService::new(db_pool),
            cache_provider,
            cache_config,
        }
    }

    /// Get performance metrics with caching
    pub async fn get_performance_metrics(&self, hours: u32) -> TaskerResult<PerformanceMetrics> {
        let cache_key = self.performance_cache_key(hours);

        // Try cache first
        if let Some(cached) = self.try_cache_get::<PerformanceMetrics>(&cache_key).await {
            return Ok(cached);
        }

        // Cache miss - fetch from DB
        let metrics = self.query_service.get_performance_metrics(hours).await?;

        // Populate cache (best-effort)
        self.try_cache_set(&cache_key, &metrics).await;

        Ok(metrics)
    }

    /// Get bottleneck analysis with caching
    pub async fn get_bottleneck_analysis(
        &self,
        limit: i32,
        min_executions: i32,
    ) -> TaskerResult<BottleneckAnalysis> {
        let cache_key = self.bottleneck_cache_key(limit, min_executions);

        if let Some(cached) = self.try_cache_get::<BottleneckAnalysis>(&cache_key).await {
            return Ok(cached);
        }

        let analysis = self.query_service.get_bottleneck_analysis(limit, min_executions).await?;

        self.try_cache_set(&cache_key, &analysis).await;

        Ok(analysis)
    }

    // Cache key generation
    fn performance_cache_key(&self, hours: u32) -> String {
        let prefix = self.key_prefix();
        format!("{prefix}:analytics:performance:{hours}")
    }

    fn bottleneck_cache_key(&self, limit: i32, min_executions: i32) -> String {
        let prefix = self.key_prefix();
        format!("{prefix}:analytics:bottlenecks:{limit}:{min_executions}")
    }

    fn key_prefix(&self) -> &str {
        self.cache_config
            .as_ref()
            .map(|c| c.key_prefix.as_str())
            .unwrap_or("tasker")
    }

    fn analytics_ttl(&self) -> Duration {
        Duration::from_secs(
            self.cache_config
                .as_ref()
                .map(|c| c.analytics_ttl_seconds as u64)
                .unwrap_or(60)
        )
    }

    // Best-effort cache operations (errors logged, never propagated)
    async fn try_cache_get<T: DeserializeOwned>(&self, key: &str) -> Option<T> { ... }
    async fn try_cache_set<T: Serialize>(&self, key: &str, value: &T) { ... }
}
```

### Phase 5: Wire into AppState

**Update** `tasker-orchestration/src/web/state.rs`:

```rust
pub struct AppState {
    // ... existing fields ...

    /// TAS-168: Analytics service with caching
    pub analytics_service: Arc<AnalyticsService>,
}

impl AppState {
    pub async fn from_orchestration_core(
        orchestration_core: Arc<OrchestrationCore>,
    ) -> ApiResult<Self> {
        // ... existing initialization ...

        // TAS-168: Create analytics service
        let analytics_service = Arc::new(AnalyticsService::new(
            orchestration_core.context.database_pool().clone(),
            orchestration_core.context.cache_provider.clone(),
            orchestration_core.context.tasker_config.common.cache.clone(),
        ));

        Ok(Self {
            // ... existing fields ...
            analytics_service,
        })
    }
}
```

### Phase 6: Simplify Analytics Handlers

**Update** `tasker-orchestration/src/web/handlers/analytics.rs`:

```rust
pub async fn get_performance_metrics(
    State(state): State<AppState>,
    security: SecurityContext,
    Query(params): Query<MetricsQuery>,
) -> ApiResult<Json<PerformanceMetrics>> {
    require_permission(&security, Permission::AnalyticsRead)?;

    let hours = params.hours.unwrap_or(24);

    execute_with_circuit_breaker(&state, || async {
        let metrics = state.analytics_service
            .get_performance_metrics(hours)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        Ok(Json(metrics))
    })
    .await
}

pub async fn get_bottlenecks(
    State(state): State<AppState>,
    security: SecurityContext,
    Query(params): Query<BottleneckQuery>,
) -> ApiResult<Json<BottleneckAnalysis>> {
    require_permission(&security, Permission::AnalyticsRead)?;

    let limit = params.limit.unwrap_or(10);
    let min_executions = params.min_executions.unwrap_or(5);

    execute_with_circuit_breaker(&state, || async {
        let analysis = state.analytics_service
            .get_bottleneck_analysis(limit, min_executions)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        Ok(Json(analysis))
    })
    .await
}
```

### Phase 7: Update Module Structure

**New directory**: `tasker-orchestration/src/services/`

```
tasker-orchestration/src/services/
├── mod.rs
├── analytics_service.rs
└── analytics_query_service.rs
```

**Update** `tasker-shared/src/cache/providers/mod.rs`:

```rust
pub mod noop;
#[cfg(feature = "cache-redis")]
pub mod redis;
#[cfg(feature = "cache-moka")]
pub mod moka;
```

---

## Configuration

### Extended Cache Configuration

```toml
[common.cache]
enabled = true
backend = "redis"           # "redis", "moka", "in-memory"
default_ttl_seconds = 3600
template_ttl_seconds = 3600
analytics_ttl_seconds = 60
key_prefix = "tasker"

[common.cache.redis]
url = "${REDIS_URL:-redis://localhost:6379}"
max_connections = 10
connection_timeout_seconds = 5
database = 0

[common.cache.moka]
max_capacity = 10000        # Maximum entries in cache
```

### Backend Selection Guidelines

| Backend | Use Case | Template Safe? | Distributed? |
|---------|----------|----------------|--------------|
| `redis` | Multi-instance production | ✅ Yes | ✅ Yes |
| `moka` | Single-instance, DoS protection | ❌ No | ❌ No |
| (disabled) | Development, testing | N/A | N/A |

---

## Design Decisions

### 1. No Cache-Busting Endpoint

**Decision**: Do not implement cache-busting for analytics.

**Rationale**:
- Analytics data is informational, not transactional
- Real-time analytics needs would use CDC → streaming pipelines
- 60-second TTL provides sufficient freshness for dashboards
- Avoids permission complexity (would need new `analytics:cache_clear` permission)
- Can be added later if genuine need emerges

### 2. In-Memory Cache for Analytics Only

**Decision**: Moka cache is valid for analytics but NOT for templates.

**Rationale**:
- Templates: Workers invalidate cache on bootstrap. In-memory cache on orchestration would never see these invalidations → stale templates → operational errors.
- Analytics: Data is aggregated, historical, and TTL-bounded. Brief staleness is acceptable. In-memory provides DoS protection even without Redis.

### 3. Type-Safe Constraints via Runtime Check

**Decision**: Use `CacheUsageContext` enum + runtime validation rather than phantom types.

**Rationale**:
- Matches existing codebase patterns (graceful degradation)
- Allows configuration-driven backend selection
- Provides clear error messages via logging
- Avoids complex generic proliferation

### 4. Service Layer Separation

**Decision**: Split into `AnalyticsQueryService` (pure logic) and `AnalyticsService` (cache wrapper).

**Rationale**:
- Query service is testable without cache
- Cache wrapper follows established `TaskHandlerRegistry` pattern
- Clear separation of concerns
- Query service could be reused for non-cached internal calls

---

## Files Changed/Created

### New Files

| File | Purpose |
|------|---------|
| `tasker-shared/src/cache/providers/moka.rs` | Moka in-memory cache implementation |
| `tasker-shared/src/cache/constraints.rs` | Cache usage context and constraints |
| `tasker-orchestration/src/services/mod.rs` | Services module |
| `tasker-orchestration/src/services/analytics_service.rs` | Cache-aware analytics service |
| `tasker-orchestration/src/services/analytics_query_service.rs` | Analytics query logic |

### Modified Files

| File | Changes |
|------|---------|
| `tasker-shared/Cargo.toml` | Add moka dependency |
| `tasker-shared/src/cache/mod.rs` | Export constraints module |
| `tasker-shared/src/cache/provider.rs` | Add Moka variant, `is_distributed()` |
| `tasker-shared/src/cache/providers/mod.rs` | Export moka module |
| `tasker-shared/src/config/tasker.rs` | Add `MokaConfig` |
| `tasker-shared/src/registry/task_handler_registry.rs` | Use constrained provider |
| `tasker-orchestration/src/web/state.rs` | Add analytics_service field |
| `tasker-orchestration/src/web/handlers/analytics.rs` | Simplify to use service |
| `tasker-orchestration/src/lib.rs` | Export services module |
| `config/tasker/base/common.toml` | Add moka config section |

---

## Testing Strategy

### Unit Tests

1. **Moka provider**: CRUD, TTL expiry, health check
2. **Cache constraints**: Valid/invalid context combinations
3. **AnalyticsQueryService**: Aggregation logic, recommendations
4. **AnalyticsService**: Cache key generation, TTL configuration

### Integration Tests

1. **E2E with Redis**: Cache hit/miss flow
2. **E2E with Moka**: Cache hit/miss flow
3. **Fallback to NoOp**: Graceful degradation
4. **Template constraint enforcement**: Log warning when Moka + templates

### Existing Tests

All existing analytics endpoint tests should pass unchanged (caching is transparent).

---

## Acceptance Criteria Mapping

| Criteria | Implementation |
|----------|----------------|
| Analytics endpoints return cached responses within TTL window | AnalyticsService cache-aside pattern |
| Redis backend works for multi-instance deployments | Existing from TAS-156 |
| In-process fallback works when Redis is unavailable | Moka cache provider |
| Cache trait is generic enough for template caching reuse | Existing from TAS-156 |
| Configuration via TOML (backend selection, TTL per resource) | Extended CacheConfig |
| Existing tests pass (analytics flakiness reduced) | Integration tests |
| **[NEW]** Service layer properly separates concerns | AnalyticsQueryService + AnalyticsService |
| **[NEW]** Templates cannot use in-memory cache | CacheUsageContext constraints |

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Moka memory growth | Configure `max_capacity`, use TTL eviction |
| Template using wrong cache | Type-safe constraints + warning logs |
| Service layer adds latency | Minimal overhead, cache hit fast path |
| Breaking existing tests | Service layer is additive, handlers delegate |

---

## Future Enhancements (Out of Scope)

- Cache warming on startup
- Per-endpoint cache invalidation API
- Cache hit/miss metrics (Prometheus)
- Tiered caching (Moka → Redis fallback)

---

## Implementation Status

**Status: ✅ COMPLETE** (2025-01-25)

### Files Created

| File | Purpose |
|------|---------|
| `tasker-shared/src/cache/providers/moka.rs` | Moka in-memory cache provider |
| `tasker-shared/src/cache/constraints.rs` | Cache usage context constraints |
| `tasker-orchestration/src/services/mod.rs` | Service layer module |
| `tasker-orchestration/src/services/analytics_query_service.rs` | DB queries (simplified, no Rust aggregation) |
| `tasker-orchestration/src/services/analytics_service.rs` | Cache-aside pattern wrapper |
| `migrations/20260125000001_analytics_aggregated_functions.sql` | Pre-aggregated SQL functions |

### Files Modified

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace) | Added moka dependency |
| `tasker-shared/Cargo.toml` | Added moka dep and cache-moka feature |
| `tasker-shared/src/cache/provider.rs` | Added Moka variant, is_distributed() method |
| `tasker-shared/src/cache/providers/mod.rs` | Export MokaCacheService |
| `tasker-shared/src/cache/mod.rs` | Export constraints module |
| `tasker-shared/src/config/tasker.rs` | Added MokaConfig struct |
| `tasker-shared/src/registry/task_handler_registry.rs` | Validate cache provider for templates |
| `tasker-shared/src/database/sql_functions.rs` | Added AggregatedStepAnalysis, AggregatedTaskAnalysis |
| `tasker-shared/src/types/api/orchestration.rs` | Added template identity to SlowStepInfo/SlowTaskInfo |
| `tasker-orchestration/src/lib.rs` | Added services module |
| `tasker-orchestration/src/web/state.rs` | Added analytics_service field |
| `tasker-orchestration/src/web/handlers/analytics.rs` | Simplified to use service layer |
| `config/tasker/base/common.toml` | Added moka configuration section |

### SQL Function Improvements

The original `get_slowest_steps` and `get_slowest_tasks` functions returned individual execution
records, requiring Rust-side HashMap aggregation. This had two problems:

1. **Code smell**: Rust code had intermediate data structures mixing Vec<f64> with counts
2. **Incorrect grouping**: Aggregation was by `step_name` alone, but step names are only unique
   within a task template context `(namespace, task_name, version)`

New pre-aggregated functions:
- `get_slowest_steps_aggregated` - Groups by `(namespace, task_name, version, step_name)`
- `get_slowest_tasks_aggregated` - Groups by `(namespace, task_name, version)`

This eliminated ~70 lines of Rust aggregation code and fixed the identity ambiguity bug.

### Test Results

- 663 tests pass (lib tests for tasker-shared + tasker-orchestration)
- 55 cache-specific tests pass
- 16 analytics service tests pass

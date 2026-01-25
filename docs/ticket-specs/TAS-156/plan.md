# TAS-156: Redis Cache for Task Templates

## Overview

Add a distributed Redis cache layer **internal to `TaskHandlerRegistry`**. The registry already owns both the read path (`resolve_handler`) and write path (`register_task_template`), so cache-aside and invalidation are co-located. No wrapper struct needed. Entirely opt-in; PostgreSQL remains the only hard dependency.

---

## Phase 1: Foundation (tasker-shared cache module)

### 1A. Configuration Structs

**Modify: `tasker-shared/src/config/tasker.rs`**
- Add `pub cache: Option<CacheConfig>` to `CommonConfig` (with serde default + skip_serializing_if)
- New structs:
  - `CacheConfig`: `enabled`, `backend`, `default_ttl_seconds`, `template_ttl_seconds`, `analytics_ttl_seconds`, `key_prefix`, `redis: Option<RedisConfig>`
  - `RedisConfig`: `url`, `max_connections`, `connection_timeout_seconds`, `database`
- Use `#[validate(...)]` + `#[builder(default = ...)]` per existing patterns
- `impl_builder_default!` for both

### 1B. Cache Module

**Create: `tasker-shared/src/cache/`**
```
cache/
  mod.rs              - Module exports
  errors.rs           - CacheError enum (ConnectionError, SerializationError, Timeout, BackendError)
  traits.rs           - CacheService trait (get, set, delete, delete_pattern, health_check, provider_name)
  provider.rs         - CacheProvider enum dispatch (Redis, NoOp)
  providers/
    mod.rs
    redis.rs          - RedisCacheService (uses redis::aio::ConnectionManager)
    noop.rs           - NoOpCacheService (always None/success)
```

Key design:
- `CacheProvider` enum with `Redis(RedisCacheService)` and `NoOp(NoOpCacheService)` variants
- `from_config_graceful()` constructor: if Redis fails to connect, log warning and use NoOp
- `is_enabled()` returns false for NoOp variant
- All methods dispatch via match (zero-cost, no vtable)
- Redis provider uses `ConnectionManager` for async multiplexed connections
- `delete_pattern` uses Redis SCAN + DEL (not KEYS, to avoid blocking)

### 1C. Dependencies

**Modify: `Cargo.toml` (workspace root)**
- Add: `redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }`

**Modify: `tasker-shared/Cargo.toml`**
- Add: `redis = { workspace = true, optional = true }`
- Add feature: `cache-redis = ["redis"]`
- Add `cache-redis` to `test-services` feature list
- Add `cache-redis` to `default` feature list

### 1D. Config Loader Allowlist

**Modify: `tasker-shared/src/config/config_loader.rs`**
- Add `EnvVarRule` for `REDIS_URL`: pattern `^redis://...`

---

## Phase 2: Integrate Cache into TaskHandlerRegistry

**Modify: `tasker-shared/src/registry/task_handler_registry.rs`**

Add cache provider as an internal field:

```rust
pub struct TaskHandlerRegistry {
    db_pool: PgPool,
    event_publisher: Option<Arc<EventPublisher>>,
    search_paths: Option<Vec<String>>,
    // TAS-156: Optional distributed cache
    cache_provider: Option<Arc<CacheProvider>>,
    cache_config: Option<CacheConfig>,
}
```

#### resolve_handler (cache-aside read path)
When cache is available:
1. Build key: `{prefix}:task_template:{namespace}:{name}:{version}`
2. Try `cache_provider.get(key)` → deserialize `HandlerMetadata`
3. On hit: return immediately
4. On miss or error: fall through to existing DB queries
5. After successful DB fetch: `cache_provider.set(key, serialized, template_ttl)` (best-effort)
6. On any cache error: log warning, never fail the request

#### register_task_template (invalidation on write path)
After successful DB write (existing logic unchanged):
1. Call `self.invalidate_cache(namespace, name, version)`
2. This deletes the specific key from cache
3. Best-effort - cache errors logged but don't fail registration

#### discover_and_register_templates (bulk invalidation on worker boot)
After processing each template file, `register_task_template` is called which invalidates individually. Additionally, after the full discovery loop completes, call `invalidate_all_templates()` to ensure no stale entries remain from templates that may have been removed from disk.

#### New helper methods:
```rust
/// Invalidate a single template cache entry
async fn invalidate_cache(&self, namespace: &str, name: &str, version: &str) { ... }

/// Invalidate all template cache entries (pattern delete)
pub async fn invalidate_all_templates(&self) { ... }

/// Invalidate all templates in a namespace
pub async fn invalidate_namespace_templates(&self, namespace: &str) { ... }

/// Check if cache is available and enabled
pub fn cache_enabled(&self) -> bool { ... }
```

#### Constructor updates:
- `with_system_context()`: reads cache config from `context.tasker_config.common.cache`, creates or receives `CacheProvider`
- New: `with_cache(db_pool, cache_provider, cache_config)` constructor for explicit injection

---

## Phase 3: SystemContext Integration

**Modify: `tasker-shared/src/system_context.rs`**
- Add field: `pub cache_provider: Arc<CacheProvider>`
- Add `create_cache_provider()` method (reads `config.common.cache`, calls `from_config_graceful`)
- Wire into `from_pools_and_config()`: create cache provider, pass to `TaskHandlerRegistry`
- Update `TaskHandlerRegistry` construction to pass the cache provider
- Add accessor: `pub fn cache_provider(&self) -> &Arc<CacheProvider>`

The `task_handler_registry` field type stays `Arc<TaskHandlerRegistry>` - no new wrapper type. Orchestration's `TemplateLoader` continues calling `registry.resolve_handler()` with zero code changes since the cache is internal.

---

## Phase 4: Orchestration Integration

**No changes required.** The `TemplateLoader` already calls `registry.resolve_handler(task_request)` which now internally checks cache first. This is the key benefit of keeping the cache boundary inside the registry.

---

## Phase 5: Worker Integration

### 5A. TaskTemplateManager Updates

**Modify: `tasker-worker/src/worker/task_template_manager.rs`**

The worker's `ensure_templates_in_database()` flow:
1. Calls `discover_and_register_templates(config_directory)`
2. Which delegates to `self.registry.discover_and_register_templates()`
3. Which calls `self.register_task_template(&template)` per file
4. **Each `register_task_template` now internally invalidates the cache entry** (Phase 2)
5. After full discovery, registry calls `invalidate_all_templates()` for completeness

This means **worker boot automatically invalidates stale cache entries with no new code in TaskTemplateManager**.

The worker's local in-memory cache (`self.cache: HashMap`) remains for process-local hot-path. On `fetch_and_cache_template`, it calls `self.registry.resolve_handler()` which uses Redis internally, then the worker caches the resolved template locally.

### 5B. Worker Template API Updates

**Modify: `tasker-worker/src/web/handlers/templates.rs`**
- Update `clear_cache` handler: after clearing local cache, also call `registry.invalidate_all_templates()`
- Update `refresh_template` handler: after refreshing local entry, also call `registry.invalidate_cache(ns, name, ver)`
- New handler: `get_distributed_cache_status` - returns whether cache is enabled, provider name, health check result

**Modify: `tasker-worker/src/web/routes.rs`**
- Add route: `GET /templates/cache/distributed` for distributed cache status

**Modify: `tasker-worker/src/worker/services/template_query/service.rs`**
- Add `distributed_cache_status()` method that queries `registry.cache_enabled()` and `cache_provider.health_check()`
- Update `clear_cache()` to also clear distributed cache
- Update `refresh_template()` to also invalidate distributed cache entry

---

## Phase 6: Infrastructure

### 6A. Docker Compose

**Modify: `docker/docker-compose.test.yml`**
```yaml
  # ==========================================================================
  # Redis Cache (TAS-156: Distributed Template Cache)
  # ==========================================================================
  redis:
    image: redis:7-alpine
    container_name: tasker-test-redis
    ports:
      - "6379:6379"
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 5
    networks:
      - tasker-test
```
- Add `redis_data:` to volumes section

### 6B. Dotenv Files

**Modify: `config/dotenv/test.env`**
```bash
# Redis Cache (TAS-156)
REDIS_URL=redis://localhost:6379
```

### 6C. TOML Configuration

**Modify: `config/tasker/base/common.toml`**
```toml
# Distributed Cache Configuration (TAS-156)
# When enabled=false or section missing, system uses direct DB queries only
[common.cache]
enabled = false
backend = "redis"
default_ttl_seconds = 3600
template_ttl_seconds = 3600
analytics_ttl_seconds = 60
key_prefix = "tasker"

[common.cache.redis]
url = "${REDIS_URL:-redis://localhost:6379}"
max_connections = 10
connection_timeout_seconds = 5
database = 0
```

**Modify: `config/tasker/environments/test/common.toml`**
```toml
# Redis cache enabled for integration/E2E tests
[common.cache]
enabled = true
template_ttl_seconds = 300
analytics_ttl_seconds = 30

[common.cache.redis]
url = "${REDIS_URL:-redis://localhost:6379}"
```

**Modify: `config/tasker/environments/development/common.toml`**
```toml
[common.cache]
enabled = true
```

**Modify: `config/tasker/environments/production/common.toml`**
```toml
[common.cache]
enabled = true
template_ttl_seconds = 3600
analytics_ttl_seconds = 120
```

---

## Phase 7: Tests

### Unit Tests (no Redis required)
- `tasker-shared/src/cache/providers/noop.rs` - NoOp always returns None/success
- `tasker-shared/src/cache/provider.rs` - `from_config_graceful` fallback logic
- Config deserialization for `CacheConfig`/`RedisConfig`
- `TaskHandlerRegistry` with no cache provider: existing behavior unchanged

### Integration Tests (behind `test-services` feature)
- Redis provider: CRUD, TTL expiry, pattern delete via SCAN
- `TaskHandlerRegistry` with Redis: cache-aside flow (hit/miss/populate)
- `register_task_template` invalidates cache entry
- `discover_and_register_templates` invalidates all on completion
- Graceful degradation: simulate Redis down, verify DB fallback (warn log, no error)

### E2E Tests
- Worker boot loads templates → cache entries invalidated
- Orchestration resolves templates → cache populated on first call, hit on second
- Worker `clear_cache` endpoint → both local and distributed caches cleared

---

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Cache inside TaskHandlerRegistry | Single responsibility boundary: reads and writes co-located with invalidation |
| Enum dispatch (not trait objects) | Matches MessagingProvider; zero vtable overhead |
| NoOp as default | All code paths identical; no conditional branching in business logic |
| Graceful degradation | System never fails to start or serve requests due to cache issues |
| Best-effort cache writes | Cache errors logged but never propagated; DB always source of truth |
| Worker boot = invalidation | `register_task_template` invalidates per-entry; bulk invalidation after discovery |
| No orchestration code changes | TemplateLoader calls registry as before; cache is transparent |
| Complementary to worker local cache | Redis = cross-instance consistency; worker HashMap = process-local hot path |
| Separate TTLs per use case | Templates (infrequent change, 1h) vs analytics (frequent change, 1m) |
| SCAN for pattern delete | Never use KEYS in production (blocking); SCAN is non-blocking |

---

## Implementation Order

1. Phase 1A-1C: Config structs + cache module + dependencies
2. Phase 6: Infrastructure (Docker, env, TOML) - enables testing as we build
3. Phase 1D: Config loader allowlist
4. Phase 2: Integrate cache into TaskHandlerRegistry (core logic)
5. Phase 3: SystemContext wiring
6. Phase 5: Worker API updates (cache status + distributed clear)
7. Phase 7: Tests

Note: Phase 4 (orchestration) requires zero changes.

---

## Critical Files

| File | Action |
|------|--------|
| `tasker-shared/src/config/tasker.rs` | Add CacheConfig, RedisConfig to CommonConfig |
| `tasker-shared/src/cache/` (new dir) | Cache module: traits, provider, errors, redis/noop |
| `tasker-shared/src/registry/task_handler_registry.rs` | Add cache_provider field, cache-aside in resolve_handler, invalidation in register_task_template |
| `tasker-shared/src/system_context.rs` | Add cache_provider field, pass to registry constructor |
| `tasker-shared/src/config/config_loader.rs` | Add REDIS_URL to env var allowlist |
| `tasker-shared/Cargo.toml` | Add redis dep + cache-redis feature |
| `Cargo.toml` (workspace) | Add redis workspace dep |
| `tasker-worker/src/web/handlers/templates.rs` | Distributed cache clear + status endpoint |
| `tasker-worker/src/web/routes.rs` | Add distributed cache route |
| `tasker-worker/src/worker/services/template_query/service.rs` | Distributed cache operations |
| `docker/docker-compose.test.yml` | Add Redis service |
| `config/dotenv/test.env` | Add REDIS_URL |
| `config/tasker/base/common.toml` | Add [common.cache] section |
| `config/tasker/environments/*/common.toml` | Environment overrides |

---

## Verification

1. `cargo check --all-features` - compiles cleanly
2. `cargo test --features test-messaging --lib` - unit tests pass (no Redis needed)
3. Start Redis: `docker compose -f docker/docker-compose.test.yml up -d redis`
4. `cargo test --features test-services` - integration tests pass with Redis
5. Run with `common.cache.enabled = false` - verifies opt-out (direct DB queries)
6. Run with `common.cache.enabled = true` - verifies cache-aside works
7. Stop Redis while running - verifies graceful degradation (falls back to DB, logs warning)
8. Worker boot with cache enabled - verify cache keys are invalidated (check Redis with `redis-cli KEYS tasker:*`)

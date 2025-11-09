# Config Struct Refactoring Plan

## Problem
Files like `circuit_breaker.rs`, `queues.rs`, `web.rs`, and `mpsc_channels.rs` are **defining config structs** when they should only be **adding impl blocks** with business logic on top of structs defined in `tasker_v2.rs`.

## Current Pattern (Wrong)
```rust
// circuit_breaker.rs
pub struct CircuitBreakerConfig { ... }  // ❌ Defining struct
impl CircuitBreakerConfig { ... }         // ✅ Business logic
```

## Desired Pattern (Right)
```rust
// circuit_breaker.rs
use crate::config::tasker::tasker_v2::CircuitBreakerConfig;  // ✅ Import from V2
impl CircuitBreakerConfig { ... }                              // ✅ Business logic only
```

---

## File-by-File Refactoring Plan

### 1. ✅ `circuit_breaker.rs` - STRAIGHTFORWARD

**V2 Status**: All 3 structs already defined in V2 ✅

| Legacy Struct | V2 Equivalent | Action |
|---------------|---------------|--------|
| `CircuitBreakerConfig` | `CircuitBreakerConfig` | Import from V2 |
| `CircuitBreakerGlobalSettings` | `GlobalCircuitBreakerSettings` | Import from V2 |
| `CircuitBreakerComponentConfig` | `CircuitBreakerComponentConfig` | Import from V2 |

**Business Logic to Keep**:
```rust
impl CircuitBreakerConfig {
    pub fn config_for_component(&self, component_name: &str) -> CircuitBreakerComponentConfig
}

impl CircuitBreakerComponentConfig {
    pub fn to_resilience_config(&self) -> crate::resilience::config::CircuitBreakerConfig
}

impl CircuitBreakerGlobalSettings {
    pub fn to_resilience_config(&self) -> crate::resilience::config::GlobalCircuitBreakerSettings
}
```

**Refactoring Steps**:
1. Remove all `pub struct` definitions (3 structs, ~30 lines)
2. Add imports from `tasker_v2`
3. Keep all `impl` blocks
4. Update type references if needed (e.g., `GlobalCircuitBreakerSettings` vs `CircuitBreakerGlobalSettings`)

**Issue**: Name mismatch
- Legacy: `CircuitBreakerGlobalSettings`
- V2: `GlobalCircuitBreakerSettings`
- Solution: Use V2 name, add type alias if needed for compatibility

---

### 2. ✅ `queues.rs` - STRAIGHTFORWARD

**V2 Status**: All 4 structs already defined in V2 ✅

| Legacy Struct | V2 Equivalent | Action |
|---------------|---------------|--------|
| `QueuesConfig` | `QueuesConfig` | Import from V2 |
| `OrchestrationQueuesConfig` | `OrchestrationQueuesConfig` | Import from V2 |
| `PgmqBackendConfig` | `PgmqConfig` | Import from V2 (name diff) |
| `RabbitMqBackendConfig` | `RabbitmqConfig` | Import from V2 (name diff) |
| `OrchestrationOwnedQueues` | ❌ Not in V2 | Keep or move to V2 |

**Business Logic to Keep**:
```rust
impl QueuesConfig {
    pub fn get_queue_name(&self, namespace: &str, name: &str) -> String
    pub fn get_orchestration_queue_name(&self, queue_name: &str) -> String
    pub fn get_worker_queue_name(&self, queue_name: &str) -> String
    pub fn step_results_queue_name(&self) -> String
    pub fn task_requests_queue_name(&self) -> String
    pub fn task_finalizations_queue_name(&self) -> String
}

impl Default for OrchestrationOwnedQueues { ... }
```

**Refactoring Steps**:
1. Remove `pub struct` for: QueuesConfig, OrchestrationQueuesConfig, PgmqBackendConfig, RabbitMqBackendConfig
2. Add imports from `tasker_v2`
3. Decide on `OrchestrationOwnedQueues`: move to V2 or keep as utility struct
4. Keep all `impl` blocks
5. Add type aliases for name changes:
   - `pub type PgmqBackendConfig = crate::config::tasker::tasker_v2::PgmqConfig;`
   - `pub type RabbitMqBackendConfig = crate::config::tasker::tasker_v2::RabbitmqConfig;`

---

### 3. ⚠️ `web.rs` - COMPLEX (Multiple Name Differences)

**V2 Status**: All structs exist but many name differences

| Legacy Struct | V2 Equivalent | Name Match? | Action |
|---------------|---------------|-------------|--------|
| `WebConfig` | `OrchestrationWebConfig` | ❌ Different | Type alias |
| `WebTlsConfig` | `TlsConfig` | ❌ Different | Type alias |
| `WebDatabasePoolsConfig` | `WebDatabasePoolsConfig` | ✅ Same | Import |
| `WebCorsConfig` | `CorsConfig` | ❌ Different | Type alias |
| `WebAuthConfig` | `AuthConfig` | ❌ Different structure | **Complex** |
| `RouteAuthConfig` | `RouteAuthConfig` | ✅ Same | Import (V2 has it) |
| `WebRateLimitConfig` | `RateLimitingConfig` | ❌ Different | Type alias |
| `WebResilienceConfig` | `ResilienceConfig` | ❌ Different | Type alias |

**Special Case: WebAuthConfig vs AuthConfig**
- V2 `AuthConfig` has `protected_routes: Vec<ProtectedRouteConfig>`
- V2 has helper method: `AuthConfig::routes_map()` → `HashMap<String, RouteAuthConfig>`
- Legacy `WebAuthConfig` has `protected_routes: HashMap<String, RouteAuthConfig>` directly
- **Different structure!**

**Business Logic to Keep** (86 lines):
```rust
impl WebAuthConfig {
    pub fn route_requires_auth(&self, method: &str, path: &str) -> bool
    pub fn auth_type_for_route(&self, method: &str, path: &str) -> Option<String>
    fn route_matches_pattern(&self, route: &str, pattern: &str) -> bool
}
```

**Refactoring Steps**:
1. Remove `pub struct` definitions for types that match V2
2. Add type aliases for name differences:
   ```rust
   pub type WebConfig = crate::config::tasker::tasker_v2::OrchestrationWebConfig;
   pub type WebTlsConfig = crate::config::tasker::tasker_v2::TlsConfig;
   pub type WebCorsConfig = crate::config::tasker::tasker_v2::CorsConfig;
   pub type WebRateLimitConfig = crate::config::tasker::tasker_v2::RateLimitingConfig;
   pub type WebResilienceConfig = crate::config::tasker::tasker_v2::ResilienceConfig;
   ```
3. **Keep `WebAuthConfig` struct definition** (different from V2's AuthConfig)
4. Keep all `impl` blocks
5. Keep `From<V2>` conversion implementations

**Why keep WebAuthConfig struct**:
- V2's `AuthConfig` uses `Vec<ProtectedRouteConfig>` (TOML-friendly)
- Legacy `WebAuthConfig` uses `HashMap<String, RouteAuthConfig>` (runtime-optimized)
- This is an intentional adapter: Vec (V2) → HashMap (runtime)

---

### 4. 📋 `mpsc_channels.rs` - NEEDS V2 COMPARISON FIRST

**V2 Status**: Similar structs exist with naming differences

**Action**: Compare field-by-field before refactoring
- V2 uses: `SharedMpscChannelsConfig`, `OrchestrationMpscChannelsConfig`, etc.
- Legacy uses: `SharedChannelsConfig`, `OrchestrationChannelsConfig`, etc.

**Next Step**: Create detailed comparison table

---

## Summary of Refactoring Work

### ✅ Easy Refactorings (Full Import)
**Files**: `circuit_breaker.rs` (with name aliases)

**Work**:
- Remove struct definitions
- Import from V2
- Add type aliases for name differences
- Keep impl blocks

### ⚠️ Medium Refactorings (Mixed)
**Files**: `queues.rs`, `web.rs`

**Work**:
- Remove most struct definitions
- Import from V2 with type aliases
- Keep 1-2 adapter structs (WebAuthConfig, OrchestrationOwnedQueues)
- Keep impl blocks

### 📋 Pending Investigation
**Files**: `mpsc_channels.rs`

**Work**:
- Field-by-field V2 comparison needed first
- Then decide on refactoring approach

---

## Implementation Order

### Phase 1: circuit_breaker.rs ✅
- Simplest case
- Test the refactoring pattern

### Phase 2: queues.rs ✅
- Similar to circuit_breaker
- Introduces type aliases

### Phase 3: web.rs ⚠️
- More complex with WebAuthConfig adapter
- Validates the adapter pattern

### Phase 4: mpsc_channels.rs 📋
- After V2 comparison
- Largest refactoring

---

## Expected Outcomes

**Before** (circuit_breaker.rs example):
- 82 lines total
- 3 struct definitions (~30 lines)
- 3 impl blocks (~35 lines)
- Helper functions (~17 lines)

**After**:
- ~55 lines total (33% reduction)
- 0 struct definitions
- 3 imports + type aliases (~10 lines)
- 3 impl blocks (~35 lines) - **UNCHANGED**
- Helper functions (~10 lines)

**Benefits**:
1. Single source of truth for config data (tasker_v2.rs)
2. Clear separation: data (V2) vs logic (extension files)
3. Easier to maintain and validate
4. Type aliases provide backward compatibility

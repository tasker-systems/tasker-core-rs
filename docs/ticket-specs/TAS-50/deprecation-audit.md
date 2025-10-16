# TAS-50 Phase 2-3: Deprecation and Migration Audit

**Document Version**: 1.0
**Date**: 2025-10-15
**Status**: Migration In Progress

---

## Executive Summary

This document audits the existing configuration system and identifies what needs to be deprecated/removed during the migration to context-specific configuration. Since this is a **greenfield system with no production deployments**, we can migrate directly without long deprecation periods.

---

## Configuration File Analysis

### New Context-Specific Files (Keep - Active)

Located in: `config/tasker/contexts/`

✅ **Keep and Use:**
- `common.toml` - Shared infrastructure (database, queues, shared MPSC channels)
- `orchestration.toml` - Orchestration-specific configuration
- `worker.toml` - Worker-specific configuration (language-agnostic)

**Environment overrides:** `config/tasker/environments/{test,development,production}/contexts/`
- Each has common.toml, orchestration.toml, worker.toml

**Status:** ✅ Complete and tested (Phase 1)

---

### Legacy Component-Based Files (Deprecate/Remove)

Located in: `config/tasker/base/`

#### Files Superseded by Context-Specific Structure

❌ **Remove After Migration:**

1. **`execution.toml`** (491 bytes)
   - **Reason:** Environment detection now in context-specific configs
   - **Replaced by:** `environment` field in common.toml, orchestration.toml, worker.toml
   - **Fields migrated:** `environment`, `max_concurrent_tasks` (moved to orchestration)

2. **`engine.toml`** (654 bytes)
   - **Reason:** Generic engine config not used
   - **Status:** Check usage before removal
   - **Migration:** Verify no references in codebase

3. **`state_machine.toml`** (4,750 bytes)
   - **Reason:** State machine logic is hardcoded in Rust (not configurable)
   - **Status:** Verify this is purely documentation/unused
   - **Migration:** Check if any fields are actually loaded

4. **`task_templates.toml`** (162 bytes)
   - **Reason:** Task template discovery via filesystem/database, not config
   - **Migration:** Verify template_path in WorkerConfig covers use case

5. **`telemetry.toml`** (4,826 bytes)
   - **Reason:** OpenTelemetry configuration not implemented yet
   - **Status:** Placeholder for future TAS ticket
   - **Migration:** Remove for now, add back when telemetry implemented

6. **`query_cache.toml`** (356 bytes)
   - **Reason:** Query caching not implemented
   - **Status:** Placeholder
   - **Migration:** Remove for now

7. **`system.toml`** (89 bytes)
   - **Reason:** Generic system config merged into contexts
   - **Migration:** Check what fields it has and where they moved

#### Files Migrated to Context-Specific Structure

✅ **Already Migrated (Safe to Remove After Verification):**

1. **`database.toml`** → `contexts/common.toml` [database] section ✅
2. **`queues.toml`** → `contexts/common.toml` [queues] section ✅
3. **`backoff.toml`** → `contexts/orchestration.toml` [backoff] section ✅
4. **`orchestration.toml`** → `contexts/orchestration.toml` [orchestration_system] section ✅
5. **`worker.toml`** → `contexts/worker.toml` [worker_system] section ✅
6. **`event_systems.toml`** → Split between orchestration.toml and worker.toml ✅
7. **`mpsc_channels.toml`** → Split between common.toml, orchestration.toml, worker.toml ✅
8. **`task_readiness.toml`** → `contexts/orchestration.toml` [task_readiness_events] section ✅

#### Files Requiring Migration

🔄 **Needs Migration to CommonConfig:**

1. **`circuit_breakers.toml`** (532 bytes) - **ACTIVELY USED**
   - **Status:** ✅ Verification complete - actively used throughout system
   - **Migration:** Add to `contexts/common.toml` [circuit_breakers] section
   - **Required Changes:**
     - Add field to CommonConfig struct
     - Add TOML section to common.toml
     - Update environment overrides (test/development/production)
   - **Remove:** After migration complete and tested

---

## Environment Override Files

### Files to Remove (After Migration)

All environment overrides for deprecated components:

```bash
config/tasker/environments/test/
├── execution.toml          ❌ Remove
├── engine.toml             ❌ Remove (if exists)
├── state_machine.toml      ❌ Remove (if exists)
├── task_templates.toml     ❌ Remove (if exists)
├── telemetry.toml          ❌ Remove (if exists)
├── query_cache.toml        ❌ Remove (if exists)
├── system.toml             ❌ Remove (if exists)
├── backoff.toml            ❌ Remove (migrated)
├── database.toml           ❌ Remove (migrated)
├── orchestration.toml      ❌ Remove (migrated)
├── worker.toml             ❌ Remove (migrated)
├── queues.toml             ❌ Remove (migrated)
├── event_systems.toml      ❌ Remove (migrated)
├── mpsc_channels.toml      ❌ Remove (migrated)
├── task_readiness.toml     ❌ Remove (migrated)
└── circuit_breakers.toml   ❌ Remove (check usage first)

# Same pattern for development/ and production/
```

---

## Code Migration Tasks

### 1. ConfigManager Loading Methods

**Current (legacy):**
```rust
// tasker-shared/src/config/unified_loader.rs
pub fn load_tasker_config(&mut self) -> ConfigResult<TaskerConfig> {
    let validated_config = self.load_with_validation()?;
    validated_config.to_tasker_config()
}
```

**Status:**
- ✅ New method `load_context_direct()` exists (Phase 1)
- ❌ Need to migrate callers from `load_tasker_config()` to new method
- ❌ Then deprecate `load_tasker_config()`

**Migration:**
```rust
// OLD:
let config = ConfigManager::load()?;

// NEW (Orchestration):
let config = ConfigManager::load_context_direct(ConfigContext::Orchestration)?;
let common = config.common().expect("Orchestration requires CommonConfig");
let orch = config.orchestration().expect("Orchestration requires OrchestrationConfig");

// NEW (Worker):
let config = ConfigManager::load_context_direct(ConfigContext::Worker)?;
let common = config.common().expect("Worker requires CommonConfig");
let worker = config.worker().expect("Worker requires WorkerConfig");
```

---

### 2. OrchestrationCore Bootstrap

**File:** `tasker-orchestration/src/core.rs`

**Current Status:** Need to check current implementation

**Required Changes:**
- [ ] Update to use `ConfigManager::load_context_direct(ConfigContext::Orchestration)`
- [ ] Access `common()` and `orchestration()` from ContextConfigManager
- [ ] Update SystemContext creation
- [ ] Update all downstream code using config

---

### 3. WorkerBootstrap

**File:** `tasker-worker/src/bootstrap.rs` (or similar)

**Current Status:** Need to check current implementation

**Required Changes:**
- [ ] Update to use `ConfigManager::load_context_direct(ConfigContext::Worker)`
- [ ] Access `common()` and `worker()` from ContextConfigManager
- [ ] Apply ENV overrides: `common.database_url()`, `worker.effective_template_path()`
- [ ] Update SystemContext creation
- [ ] Update all downstream code using config

---

### 4. Ruby Worker FFI Bootstrap

**File:** `workers/ruby/lib/tasker_core/worker/bootstrap.rb`

**Current Status:** Need to check if already using WorkerConfig

**Required Changes:**
- [ ] Verify Rust tasker-worker library loads WorkerConfig via FFI
- [ ] Ensure Ruby bootstrap points to correct TOML location
- [ ] Test Ruby worker with new config structure

---

### 5. Test Code Migration

**Pattern to Update:**

```rust
// OLD pattern (many tests):
let config = ConfigManager::load().unwrap();
assert!(config.database.pool.max_connections > 0);

// NEW pattern:
let config = ConfigManager::load_context_direct(ConfigContext::Orchestration).unwrap();
let common = config.common().unwrap();
assert!(common.database.pool.max_connections > 0);
```

**Affected files (estimate):**
- tasker-orchestration tests: ~10-20 files
- tasker-worker tests: ~5-10 files
- tasker-shared tests: Already updated (Phase 1)
- Integration tests: ~5-10 files

---

## Struct Field Deprecations

### TaskerConfig Fields to Deprecate

**File:** `tasker-shared/src/config/tasker.rs`

After migration complete, mark these as deprecated:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskerConfig {
    // Keep for backward compatibility tests:
    pub database: DatabaseConfig,
    pub queues: QueuesConfig,
    pub backoff: BackoffConfig,
    pub orchestration: OrchestrationSystemConfig,
    pub worker: Option<WorkerSystemConfig>,
    pub event_systems: EventSystemsConfig,
    pub mpsc_channels: MpscChannelsConfig,

    // Deprecate - unused:
    #[deprecated(since = "1.1.0", note = "Not implemented")]
    pub telemetry: Option<TelemetryConfig>,

    #[deprecated(since = "1.1.0", note = "Not configurable")]
    pub state_machine: Option<StateMachineConfig>,

    #[deprecated(since = "1.1.0", note = "Not used")]
    pub engine: Option<EngineConfig>,

    #[deprecated(since = "1.1.0", note = "Not used")]
    pub task_templates: Option<TaskTemplatesConfig>,

    #[deprecated(since = "1.1.0", note = "Use context-specific configuration")]
    pub execution: ExecutionConfig,

    #[deprecated(since = "1.1.0", note = "Not implemented")]
    pub query_cache: Option<QueryCacheConfig>,
}
```

---

## Verification Checklist

✅ **Verification Complete (2025-10-15)**

### Circuit Breaker Analysis

**Status:** ✅ **ACTIVELY USED - MUST MIGRATE**

**Findings:**
- CircuitBreakerManager extensively used in tasker-shared
- SystemContext contains circuit_breaker_manager field
- Resilience module has full circuit breaker implementation
- Configuration loaded as part of TaskerConfig.circuit_breakers
- Used throughout system for database, queue, and API resilience

**Decision:** **circuit_breakers.toml MUST BE MIGRATED to CommonConfig**
- Circuit breakers are shared infrastructure (used by both orchestration and worker)
- Need to add `circuit_breakers: CircuitBreakerConfig` field to CommonConfig struct
- Need to add `[circuit_breakers]` section to `config/tasker/contexts/common.toml`

**Migration Required:**
```rust
// tasker-shared/src/config/contexts/common.rs
pub struct CommonConfig {
    pub database: DatabaseConfig,
    pub queues: QueuesConfig,
    pub environment: String,
    pub shared_channels: SharedChannelsConfig,
    pub circuit_breakers: CircuitBreakerConfig,  // ADD THIS
}
```

### State Machine Config Analysis

**Status:** ✅ **NOT USED - SAFE TO REMOVE**

**Findings:**
- No StateMachineConfig struct instantiation anywhere
- Only `use_unified_state_machine` boolean flags found
- State machine logic is hardcoded in Rust (not configurable via TOML)
- state_machine.toml (4,750 bytes) is purely documentation

**Decision:** **state_machine.toml CAN BE REMOVED ENTIRELY**
- State transitions are defined in Rust code
- TOML file does not affect system behavior

### Engine Config Analysis

**Status:** ✅ **NOT USED - SAFE TO REMOVE**

**Findings:**
- EngineConfig struct defined but never instantiated
- Only imported (but not used) in one file: `tasker-orchestration/src/orchestration/config.rs`
- No runtime usage of EngineConfig anywhere in codebase

**Decision:** **engine.toml CAN BE REMOVED ENTIRELY**
- Unused import can be removed during cleanup
- TOML file serves no purpose

---

## Migration Timeline

### Immediate (This Sprint)

1. ✅ **Phase 1 Complete:** Context-specific structs and TOML files created
2. 🔄 **Phase 2-3 Combined:** Migrate bootstrap code and remove legacy files

### Week 1: Code Migration (3-5 days)

- [x] Day 1: Verify circuit breaker, state machine, engine usage
  - ✅ Circuit breakers: MUST MIGRATE to CommonConfig
  - ✅ State machine: REMOVE (not used)
  - ✅ Engine: REMOVE (not used)
- [ ] Day 1b: Migrate circuit_breakers to CommonConfig
  - Add field to CommonConfig struct
  - Add TOML section to common.toml and environment overrides
  - Update tests
- [ ] Day 2: Update OrchestrationCore bootstrap
- [ ] Day 3: Update WorkerBootstrap
- [ ] Day 4-5: Update tests, fix compilation issues

### Week 2: Cleanup (2-3 days)

- [ ] Day 1: Remove unused TOML files (11+ files)
- [ ] Day 2: Deprecate old loading methods
- [ ] Day 3: Update documentation, verify all tests pass

---

## Files to Remove (Summary)

### Base Configuration Files (15 files confirmed)

```bash
config/tasker/base/
├── execution.toml          ❌ Remove (environment in contexts now)
├── engine.toml             ❌ Remove (verified: not used)
├── state_machine.toml      ❌ Remove (verified: hardcoded in Rust)
├── task_templates.toml     ❌ Remove (template_path in WorkerConfig)
├── telemetry.toml          ❌ Remove (not implemented yet)
├── query_cache.toml        ❌ Remove (not implemented)
├── system.toml             ❌ Remove (check fields first)
├── backoff.toml            ❌ Remove (migrated to orchestration.toml)
├── database.toml           ❌ Remove (migrated to common.toml)
├── orchestration.toml      ❌ Remove (migrated to orchestration.toml)
├── worker.toml             ❌ Remove (migrated to worker.toml)
├── queues.toml             ❌ Remove (migrated to common.toml)
├── event_systems.toml      ❌ Remove (split across contexts)
├── mpsc_channels.toml      ❌ Remove (split across contexts)
├── task_readiness.toml     ❌ Remove (migrated to orchestration.toml)
└── circuit_breakers.toml   🔄 Migrate first → then remove
```

**Total base files to remove:** 15 files (+ 1 after circuit_breaker migration)

### Environment Override Files (48 files)

Each environment (test, development, production) has overrides for the above files:
- 16 files × 3 environments = **48 environment override files**

**Total files to remove:** ~64 files

---

## Success Criteria

✅ **Migration complete when:**

1. OrchestrationCore uses `load_context_direct(ConfigContext::Orchestration)`
2. WorkerBootstrap uses `load_context_direct(ConfigContext::Worker)`
3. Ruby workers use WorkerConfig via FFI
4. All tests pass with new loading mechanism
5. Unused TOML files removed (64 files)
6. Old loading methods deprecated
7. Documentation updated
8. No legacy cruft remaining

---

## Rollback Plan

**If issues occur during migration:**

Since this is greenfield (no production deployments):
1. Revert Git commits
2. Fix issues
3. Re-attempt migration

**No production impact** - we can iterate freely until migration is solid.

---

**Next Steps:** Begin Step 2 - Update OrchestrationCore bootstrap

**Document Status:** Living document - update as migration progresses

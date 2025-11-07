# TAS-61 Phase 5.5: V2 Promotion to Primary Config

**Date**: 2025-11-07
**Purpose**: Promote v2 config to primary, remove v2 suffixes, establish unified foundation for Phase 6

---

## Overview

This phase removes all "v2" naming artifacts and makes the new configuration system the **canonical** system, not an alternative.

### Goals

1. Archive v1 configs to `config/archive/v1`
2. Promote `config/v2` to `config/tasker` (primary location)
3. Remove `_v2` suffix from all methods
4. Delete legacy v1 loading methods entirely
5. Update all references across codebase
6. Establish clean foundation for Phase 6 (native TaskerConfigV2 usage)

---

## Step 1: Archive V1 Configs

### 1.1. Create Archive Directory

```bash
mkdir -p config/archive/v1
```

### 1.2. Move V1 Configs to Archive

```bash
# Move entire legacy config directory
mv config/tasker config/archive/v1/

# Verify structure
tree config/archive/v1 -L 2
```

**Expected Result**:
```
config/archive/v1/
├── base/
│   ├── common.toml
│   ├── orchestration.toml
│   └── worker.toml
├── environments/
│   ├── test/
│   ├── development/
│   └── production/
├── complete-test.toml
├── orchestration-test.toml
└── worker-test.toml
```

---

## Step 2: Promote V2 to Primary

### 2.1. Move V2 to Primary Location

```bash
# Move v2 to become the primary config location
mv config/v2 config/tasker

# Verify structure
tree config/tasker -L 2
```

**Expected Result**:
```
config/tasker/
├── base/
│   ├── common.toml
│   ├── orchestration.toml
│   └── worker.toml
├── environments/
│   ├── test/
│   ├── development/
│   └── production/
├── complete-test.toml
├── orchestration-test.toml
├── orchestration-production.toml
├── worker-test.toml
└── worker-production.toml
```

---

## Step 3: Update Configuration Paths

### 3.1. Update .env File

**File**: `.env`

```bash
# OLD:
TASKER_CONFIG_PATH=/Users/petetaylor/projects/tasker-systems/tasker-core/config/v2/complete-test.toml

# NEW:
TASKER_CONFIG_PATH=/Users/petetaylor/projects/tasker-systems/tasker-core/config/tasker/complete-test.toml
```

### 3.2. Update Docker Compose Files

**File**: `docker/docker-compose.test.yml`

```yaml
# OLD:
orchestration:
  environment:
    TASKER_CONFIG_PATH: /app/config/v2/orchestration-test.toml
worker:
  environment:
    TASKER_CONFIG_PATH: /app/config/v2/worker-test.toml

# NEW:
orchestration:
  environment:
    TASKER_CONFIG_PATH: /app/config/tasker/orchestration-test.toml
worker:
  environment:
    TASKER_CONFIG_PATH: /app/config/tasker/worker-test.toml
```

**Files to Update**:
- `docker/docker-compose.test.yml`
- `docker/docker-compose.prod.yml`
- Any other docker-compose files

### 3.3. Update CI Scripts

**File**: `.github/scripts/start-native-services.sh`

Search and replace:
```bash
# Find all references
grep -r "config/v2" .github/

# Update any found references:
config/v2 → config/tasker
```

### 3.4. Update Test File References

**Search for hardcoded paths**:
```bash
grep -r "config/v2" --include="*.rs" tasker-shared/tests/
grep -r "config/v2" --include="*.rs" tasker-orchestration/tests/
grep -r "config/v2" --include="*.rs" tasker-worker/tests/
```

**Update found references**:
```rust
// OLD:
"config/v2/complete-test.toml"

// NEW:
"config/tasker/complete-test.toml"
```

---

## Step 4: Rename Methods (Remove _v2 Suffix)

### 4.1. Identify All _v2 Methods

```bash
grep -r "_v2(" --include="*.rs" tasker-shared/src/system_context.rs
```

**Expected Methods**:
- `new_for_orchestration_v2()`
- `new_for_worker_v2()`

### 4.2. Update SystemContext Methods

**File**: `tasker-shared/src/system_context.rs`

**Step A: Delete Current Delegating Methods**

Delete these methods entirely (they currently just delegate to _v2):
```rust
// DELETE THIS:
pub async fn new_for_orchestration() -> TaskerResult<Self> {
    // TAS-61: Delegate to v2 loading (breaking change - v1 config no longer supported)
    Self::new_for_orchestration_v2().await
}

// DELETE THIS:
pub async fn new_for_worker() -> TaskerResult<Self> {
    // TAS-61: Delegate to v2 loading
    Self::new_for_worker_v2().await
}
```

**Step B: Rename _v2 Methods to Primary Methods**

```rust
// OLD:
pub async fn new_for_orchestration_v2() -> TaskerResult<Self> {
    info!("Initializing SystemContext for orchestration with v2 configuration (TAS-61)");
    // ...
}

// NEW:
pub async fn new_for_orchestration() -> TaskerResult<Self> {
    info!("Initializing SystemContext for orchestration");
    // ... same implementation, just rename
}
```

```rust
// OLD:
pub async fn new_for_worker_v2() -> TaskerResult<Self> {
    info!("Initializing SystemContext for worker with v2 configuration (TAS-61)");
    // ...
}

// NEW:
pub async fn new_for_worker() -> TaskerResult<Self> {
    info!("Initializing SystemContext for worker");
    // ... same implementation, just rename
}
```

**Documentation Update**:
Remove all "v2" references in doc comments:
```rust
/// Create SystemContext for orchestration using single-file configuration
///
/// This method loads orchestration configuration from a single merged TOML file
/// specified by the TASKER_CONFIG_PATH environment variable. The file should contain:
/// - CommonConfig (database, queues, circuit breakers)
/// - OrchestrationConfig (event systems, decision points, web API)
///
/// # Configuration File Format
///
/// The configuration uses context-based structure with [common] and [orchestration] sections.
/// See config/tasker/orchestration-test.toml for reference.
```

### 4.3. Search for Other _v2 Methods

```bash
grep -rn "_v2(" --include="*.rs" tasker-shared/src/
grep -rn "_v2(" --include="*.rs" tasker-orchestration/src/
grep -rn "_v2(" --include="*.rs" tasker-worker/src/
```

**Action**: Rename all found _v2 methods to remove suffix

---

## Step 5: Remove Legacy V1 Loading Methods

### 5.1. Identify Legacy Methods to Delete

**File**: `tasker-shared/src/system_context.rs`

**Methods marked as deprecated - DELETE ENTIRELY**:

1. Any methods with "DEPRECATED (TAS-61)" comments
2. Methods that load v1 configs
3. Legacy bootstrap methods

**Search Pattern**:
```bash
grep -B 5 "DEPRECATED.*TAS-61" tasker-shared/src/system_context.rs
```

### 5.2. Delete Legacy Methods

Delete these entirely (examples - verify exact methods exist):
- Any method that uses `ConfigLoader::load_from_path()` with legacy v1 paths
- Any method marked as deprecated for TAS-61
- `from_legacy_config()` or similar legacy conversion methods

**Verification**:
After deletion, search for any remaining "v1" or "legacy" references:
```bash
grep -i "legacy.*config" tasker-shared/src/system_context.rs
grep -i "v1.*config" tasker-shared/src/system_context.rs
```

---

## Step 6: Update Module Exports

### 6.1. Update tasker/mod.rs

**File**: `tasker-shared/src/config/tasker/mod.rs`

```rust
// OLD:
// Legacy config (will be deprecated in Phase 6-8 of TAS-61)
pub mod tasker;

// V2 config (new baseline - TAS-61)
pub mod tasker_v2;

// Bridge for v2 → legacy conversion (TAS-61 Phase 2)
pub mod bridge;

// Re-export v2 config with alias for clarity
pub use tasker_v2::TaskerConfig as TaskerConfigV2;

// NEW:
// Current config types (TAS-61 Phase 5.5)
pub mod tasker_v2;

// Bridge for v2 → legacy conversion (TAS-61 Phase 2)
// Will be removed in Phase 6-8 when runtime code uses TaskerConfigV2 directly
pub mod bridge;

// Legacy config (deprecated, kept only for bridge)
// Will be removed in Phase 6-8
pub mod tasker;

// Primary config export
pub use tasker_v2::TaskerConfig as TaskerConfigV2;

// Legacy exports for bridge compatibility only
// These re-exports will be removed in Phase 6-8
pub use tasker::{
    // ... existing re-exports
};
```

**Rationale**:
- Makes `tasker_v2` the primary module (move to top)
- Marks `tasker` as legacy for bridge only
- Documents Phase 6-8 removal plan

---

## Step 7: Update Config File References

### 7.1. Update ConfigLoader Default Paths

**File**: `tasker-shared/src/config/config_loader.rs`

Check for any hardcoded "v2" references in comments or error messages:

```bash
grep -n "v2" tasker-shared/src/config/config_loader.rs
```

**Update Documentation**:
```rust
// OLD:
/// See config/v2/complete-test.toml for reference

// NEW:
/// See config/tasker/complete-test.toml for reference
```

### 7.2. Update ConfigMerger Documentation

**File**: `tasker-shared/src/config/merger.rs`

Search for "v2" references:
```bash
grep -n "v2" tasker-shared/src/config/merger.rs
```

**Update Comments**:
```rust
// OLD:
/// Load v2 configuration from source directory

// NEW:
/// Load configuration from source directory
```

---

## Step 8: Update Integration Tests

### 8.1. V2 Integration Tests

**File**: `tasker-shared/tests/config_v2_integration_tests.rs`

**Option A: Rename File**
```bash
mv tasker-shared/tests/config_v2_integration_tests.rs \
   tasker-shared/tests/config_integration_tests.rs
```

**Option B: Keep Name, Update Content**

Remove "v2" from all comments and function names:
```rust
// OLD:
/// Helper function to get the v2 config directory
fn v2_config_dir() -> PathBuf {
    workspace_root().join("config/v2")
}

#[test]
fn test_v2_config_generation_and_loading() {
    // ...
}

// NEW:
/// Helper function to get the config directory
fn config_dir() -> PathBuf {
    workspace_root().join("config/tasker")
}

#[test]
fn test_config_generation_and_loading() {
    // ...
}
```

### 8.2. Update Test Helper Functions

Search for test helpers with "v2" in name:
```bash
grep -rn "fn.*v2" --include="*.rs" tasker-shared/tests/
```

**Rename All**:
- `v2_config_dir()` → `config_dir()`
- `load_v2_test_config()` → `load_test_config()`
- etc.

---

## Step 9: Update CLAUDE.md

**File**: `CLAUDE.md`

Update project documentation to reflect v2 as primary:

```markdown
# OLD:
TAS-61 Phase 2 Complete: Successfully migrated from 630-line monolithic configuration to component-based system

# NEW:
TAS-61 Phases 1-5.5 Complete: Context-based configuration is now the production standard
- Component-based configuration architecture
- Automatic .env loading
- Documentation stripping from merged configs
- 736 tests passing (195% of target)
```

---

## Step 10: Validation

### 10.1. Cargo Check

```bash
cargo check --all-features
```

**Expected**: Pass with no errors

### 10.2. Clippy

```bash
cargo clippy --all-targets --all-features
```

**Expected**: Pass (or only warnings we've accepted)

### 10.3. Test Suite

```bash
cargo nextest run --package tasker-shared --package tasker-orchestration --package tasker-worker --package pgmq-notify --package tasker-client
```

**Expected**: 736 tests pass (same as before)

### 10.4. Docker Compose Test

```bash
docker compose -f docker/docker-compose.test.yml up --build -d
curl http://localhost:8080/health
curl http://localhost:8081/health
docker compose -f docker/docker-compose.test.yml down
```

**Expected**: Services start and health checks pass

---

## Step 11: Commit Strategy

### Commit 1: Archive V1 Config
```bash
git add config/archive/v1/
git commit -m "chore(TAS-61): Archive v1 configs to config/archive/v1

- Move config/tasker to config/archive/v1
- Prepare for v2 promotion to primary config location
- Part of Phase 5.5: V2 Promotion to Primary"
```

### Commit 2: Promote V2 to Primary
```bash
git add config/tasker/
git rm -r config/v2/
git commit -m "feat(TAS-61): Promote v2 config to primary location

- Move config/v2 to config/tasker
- V2 config is now the canonical configuration system
- Part of Phase 5.5: V2 Promotion to Primary"
```

### Commit 3: Update All References
```bash
git add .env docker/ .github/ tasker-shared/tests/
git commit -m "refactor(TAS-61): Update all config path references

- Update .env to point to config/tasker
- Update docker-compose files for new paths
- Update test file references
- Update CI script references
- Part of Phase 5.5: V2 Promotion to Primary"
```

### Commit 4: Remove _v2 Suffix from Methods
```bash
git add tasker-shared/src/system_context.rs
git commit -m "refactor(TAS-61): Remove _v2 suffix from primary methods

- Rename new_for_orchestration_v2() → new_for_orchestration()
- Rename new_for_worker_v2() → new_for_worker()
- Delete legacy delegating methods
- V2 is now the primary (and only) config loading method
- Part of Phase 5.5: V2 Promotion to Primary"
```

### Commit 5: Update Module Documentation
```bash
git add tasker-shared/src/config/
git commit -m "docs(TAS-61): Update configuration documentation

- Remove v2 references from comments
- Update module organization
- Mark legacy modules for Phase 6-8 removal
- Part of Phase 5.5: V2 Promotion to Primary"
```

### Commit 6: Update Tests
```bash
git add tasker-shared/tests/
git commit -m "test(TAS-61): Update integration test naming

- Remove v2 suffixes from test names
- Update test documentation
- All 736 tests passing
- Part of Phase 5.5: V2 Promotion to Primary"
```

---

## Success Criteria

### ✅ Phase 5.5 Complete When:

1. **Config Structure**:
   - ✅ `config/archive/v1/` exists with old v1 configs
   - ✅ `config/tasker/` contains promoted v2 configs
   - ✅ No `config/v2/` directory exists

2. **Methods**:
   - ✅ `new_for_orchestration()` is the primary method (no _v2 suffix)
   - ✅ `new_for_worker()` is the primary method (no _v2 suffix)
   - ✅ No legacy v1 loading methods exist

3. **References**:
   - ✅ `.env` points to `config/tasker/`
   - ✅ Docker compose files point to `config/tasker/`
   - ✅ Test files reference `config/tasker/`
   - ✅ No "v2" in path references

4. **Validation**:
   - ✅ 736 tests pass
   - ✅ Cargo check passes
   - ✅ Clippy passes
   - ✅ Docker services start successfully

5. **Documentation**:
   - ✅ CLAUDE.md updated
   - ✅ Module docs updated
   - ✅ No misleading "v2" references

---

## Phase 6 Foundation Established

After Phase 5.5, we have a **unified foundation** for Phase 6:

### Clean State:
- ✅ One config system: `config/tasker/` (no v2 suffix)
- ✅ One set of methods: `new_for_orchestration()`, `new_for_worker()`
- ✅ Clear separation:
  - `TaskerConfigV2` - New config structs (keep)
  - `TaskerConfig` - Legacy structs (remove in Phase 6)
  - `bridge.rs` - Conversion layer (remove in Phase 6)

### Phase 6 Becomes Simpler:
```
CURRENT:  Runtime code → legacy TaskerConfig → bridge ← TaskerConfigV2 ← TOML
TARGET:   Runtime code → TaskerConfigV2 ← TOML (no bridge!)
```

**Phase 6 Work**:
1. Update runtime field accesses: `config.database` → `config.common.database`
2. Delete `bridge.rs`
3. Delete `tasker.rs` (legacy structs)
4. Rename `TaskerConfigV2` → `TaskerConfig` (optional cleanup)

---

## Estimated Time

**Total: 2-3 hours**

- Step 1-2: Archive and move (15 min)
- Step 3: Update paths (30 min)
- Step 4: Rename methods (30 min)
- Step 5: Delete legacy methods (15 min)
- Step 6-9: Update docs and tests (45 min)
- Step 10: Validation (15 min)
- Step 11: Commits and cleanup (30 min)

---

**Phase 5.5 Completion**: Clean, unified foundation ready for Phase 6 runtime migration.

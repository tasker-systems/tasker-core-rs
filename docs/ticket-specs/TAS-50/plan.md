# TAS-50 Recovery Plan

**Created**: 2025-10-24
**Branch**: `jcoletaylor/tas-50-config-generation`
**Status**: ✅ Phase 1 Complete → Ready for Phase 2 Execution
**Last Updated**: 2025-10-24

---

## Executive Summary

This plan addresses test failures (155/712 tests failing) caused by misalignment between TOML configuration files and Rust struct definitions after Phase 1 configuration cleanup. The recovery uses a **greenfield cleanup strategy**: remove unused Config fields from Rust code rather than adding them back to TOML.

**Approach**: Three-phase systematic recovery focusing on removing truly unused fields while preserving runtime-critical configuration.

---

## Current Status

### Test Results
- **Passing**: 557 tests (78%)
- **Failing**: 155 tests (22%)
  - 151 config deserialization errors
  - 4 pre-existing CLI test failures

### Root Cause Analysis

**Primary Issue**: Missing configuration field `poll_interval_ms`

**Error Pattern**:
```
ConfigurationError("Failed to load test configuration:
JSON serialization error in TOML to CommonConfig deserialization:
missing field `poll_interval_ms` in `queues.pgmq`")
```

**Why This Happens**:
1. Phase 1 cleanup removed `poll_interval_ms` from `config/tasker/base/common.toml`
2. Field still exists in merged files (`orchestration-test.toml`, etc.)
3. Rust struct `PgmqBackendConfig` (tasker-shared/src/config/queues.rs:46) still requires the field
4. Context loading system reads from base files → deserialization fails

**Affected Systems**:
- All integration tests (tasker-core, tasker-shared, tasker-orchestration, tasker-worker)
- All tests that load configuration via `SystemContext::new_for_*()` methods

---

## Background: What Led Us Here

### Phase 2-3 Success (Oct 15-17)
✅ Context-specific configuration loading implemented
✅ Runtime single-file loading via `TASKER_CONFIG_PATH`
✅ Clean directory structure: `base/` + `environments/`
✅ 90 integration tests passing
✅ CLI tools partially implemented
✅ Runtime `/config` API endpoints with secret redaction

### Phase 1 Cleanup (Oct 19)
✅ Removed 47 truly unused parameters
✅ Protected 16 parameters incorrectly marked as unused
⚠️ But created TOML/Rust misalignment

**The Disconnect**:
- Phase 1 removed fields from TOML based on usage analysis
- BUT didn't remove corresponding Rust struct fields
- Original plan said "keep these fields" but they were removed from base TOML anyway
- Merged files still have them (from older generation), base files don't

---

## Recovery Strategy: Greenfield Cleanup

### Core Principle

**Remove unused Config fields from Rust code**, not add them back to TOML.

**Rationale**:
1. This is a greenfield project (no external dependencies)
2. Configuration should drive behavior, not exist for its own sake
3. Smaller structs = less maintenance burden
4. Clearer signal about what's actually configurable

### Decision Framework

For each field found in Rust structs:

**REMOVE_FROM_RUST** if:
- Field is never accessed in runtime code
- Only appears in: struct definition, Default impl, tests
- No behavior changes if removed

**KEEP_IN_BOTH** if:
- Field is accessed in runtime code (outside tests)
- Used to configure actual system behavior
- Appears in polling loops, builders, service initialization, etc.

**Example Analysis**:
```rust
// queues.rs:46
pub struct PgmqBackendConfig {
    pub poll_interval_ms: u64,  // Used in step_result_processor.rs:40,54
    // ...
}

// step_result_processor.rs:40
polling_interval_seconds: config.queues.pgmq.poll_interval_ms / 1000

// Decision: KEEP_IN_BOTH (used in runtime polling configuration)
```

---

## Three-Phase Recovery Plan

### Phase 1: Audit & Categorize Config Fields

**Goal**: Create definitive action manifest (REMOVE vs KEEP lists)

**Tasks**:

1. **Systematic Field Audit**:
   ```bash
   # For each Rust config struct:
   # 1. Find all field definitions
   # 2. Search for runtime usage (exclude tests, defaults, deserialize)
   # 3. Categorize as REMOVE or KEEP
   ```

2. **Critical Fields to Review**:
   - `poll_interval_ms` in `PgmqBackendConfig`
   - Other fields from Phase 1 removal list that are still in Rust structs
   - Any field where TOML and Rust are misaligned

3. **Document Decisions**:
   - Create action manifest in this document
   - Include reasoning for each decision
   - Get approval before Phase 2 execution

**Deliverable**: Action manifest section below (to be filled)

---

### Phase 2: Execute Cleanup

**Goal**: Align Rust structs with base TOML configuration

**For REMOVE_FROM_RUST fields**:

1. **Remove from struct**:
   ```rust
   pub struct PgmqBackendConfig {
       // pub poll_interval_ms: u64,  // REMOVED - not used
       pub shutdown_timeout_seconds: u64,
   }
   ```

2. **Remove from Default impl**:
   ```rust
   impl Default for PgmqBackendConfig {
       fn default() -> Self {
           Self {
               // poll_interval_ms: 250,  // REMOVED
               shutdown_timeout_seconds: 30,
           }
       }
   }
   ```

3. **Update accessor methods** (if any):
   - Remove getters that return the field
   - Update builders that accept the field

4. **Search for usage**:
   ```bash
   rg "poll_interval_ms" --type rust
   # Should return 0 results after removal
   ```

**For KEEP_IN_BOTH fields**:

1. **Add to base TOML**:
   ```toml
   # base/common.toml
   [queues.pgmq]
   poll_interval_ms = 250  # Polling interval for PGMQ queue checks
   ```

2. **Add ._docs attribute** (foundation for TAS-55):
   ```toml
   [queues.pgmq._docs]
   poll_interval_ms = """
   Interval in milliseconds between PGMQ queue polling checks.
   Lower values = more responsive but higher database load.
   Recommended: 100-1000ms depending on load.
   """
   ```

3. **Verify in merged configs**:
   - Regenerate merged files using CLI
   - Confirm field appears correctly

**Validation After Each Change**:
```bash
# Build
cargo build --all-features

# Run affected tests
cargo nextest run --package tasker-shared --package tasker-orchestration \
  --package tasker-worker --package tasker-core --no-fail-fast

# Check for regressions
```

**Strategy**: One field at a time, validate, commit.

---

### Phase 3: Fix CLI Tests & Finalize

**Goal**: Address remaining failures and complete feature

**Tasks**:

1. **Fix 4 Pre-Existing CLI Test Failures**:
   - `test_config_merger_missing_context` - Fix error message assertion
   - `test_default_value_when_env_var_missing` - Fix env var substitution
   - `test_environment_variable_substitution` - Fix placeholder handling
   - `test_documentation_stripping_during_merge` - Fix metadata removal

2. **Complete or Remove CLI Stubs**:
   - `explain` command - Complete (uses `._docs` attributes) OR defer to TAS-55
   - `detect-unused` command - Complete OR remove stub

3. **Regenerate Merged Configs**:
   ```bash
   # After base TOML updates, regenerate merged files
   cargo run --bin tasker-cli -- config generate \
     --context orchestration --environment test \
     --output config/tasker/orchestration-test.toml

   # Repeat for all contexts and environments
   ```

4. **Final Validation**:
   ```bash
   # Full test suite
   cargo nextest run --package tasker-shared --package tasker-orchestration \
     --package tasker-worker --package pgmq-notify --package tasker-client \
     --package tasker-core --no-fail-fast

   # Should see: 712 tests, ~700+ passing, 0-4 acceptable failures
   ```

5. **Update Documentation**:
   - Mark this plan as COMPLETE
   - Update main CLAUDE.md if needed
   - Document final state in TAS-50 completion summary

---

## Action Manifest

**Status**: ✅ Phase 1 Audit Complete
**Date**: 2025-10-24
**Method**: Extended `analyze_config_usage.py` + Phase 1 learnings + runtime code analysis

### Critical Finding: Only 2 Fields Need Action

The test failures (155 tests) are all caused by **2 missing fields** in `base/common.toml`:
- `queues.pgmq.poll_interval_ms`
- `queues.pgmq.shutdown_timeout_seconds`

Both fields are present in:
- ✅ Rust struct `PgmqBackendConfig` (queues.rs:46-47)
- ✅ Merged config files (orchestration-test.toml, worker-test.toml)
- ❌ **Missing** from `base/common.toml`

### Fields to REMOVE_FROM_RUST

**None identified at this time.**

All fields in current Rust config structs appear to be either:
- In use in runtime code
- Part of configuration architecture (even if implementation incomplete)
- Properly represented in TOML

### Fields to KEEP_IN_BOTH (add to base TOML)

| Field | Location | Reasoning | Base TOML Location | Runtime Usage |
|-------|----------|-----------|-------------------|---------------|
| `poll_interval_ms` | `PgmqBackendConfig` (queues.rs:46) | Used in runtime polling configuration | `base/common.toml [queues.pgmq]` | `step_result_processor.rs:40,54` - converts ms to seconds for polling intervals |
| `shutdown_timeout_seconds` | `PgmqBackendConfig` (queues.rs:47) | Used in shutdown duration configuration | `base/common.toml [queues.pgmq]` | `tasker.rs` - creates Duration for shutdown timeout via `pgmq_shutdown_timeout()` |

### Fields ALREADY_CORRECT (no action)

| Field | Location | Status |
|-------|----------|--------|
| `max_retries` | `PgmqBackendConfig` (queues.rs:48) | ✅ Present in base/common.toml |
| All other fields in CommonConfig, OrchestrationConfig, WorkerConfig | Various locations | ✅ Properly aligned between TOML and Rust |

### Audit Methodology

Building on Phase 1 learnings, this audit used:

1. **Analysis Script**: Ran `analyze_config_usage.py` to identify field usage patterns
2. **Test Failure Analysis**: Reviewed 155 test failures - all report same missing fields
3. **Runtime Code Search**: Used `rg` to find actual usage in non-test code:
   ```bash
   # Direct field access
   rg "poll_interval_ms" --type rust -g '!test*'
   # Method-based access
   rg "\.shutdown_timeout" --type rust
   ```
4. **Phase 1 Cross-Check**: Verified against `phase1-completion-summary.md` learnings about protected parameters

### Key Insight from Phase 1

The original `config-usage-analysis.md` was 78% accurate because it missed:
- Accessor methods (e.g., `pgmq_shutdown_timeout()`)
- Conversions (e.g., `poll_interval_ms / 1000`)
- Indirect usage via builder patterns

**Both missing fields were protected in Phase 1** as "Used in polling configuration" and "Used in shutdown timeout".

---

## Critical Decisions Needed

### Decision 1: `poll_interval_ms` in PgmqBackendConfig ✅ DECIDED

**Decision**: **KEEP_IN_BOTH** - Add to `base/common.toml`

**Rationale**:
- ✅ Actively used in runtime polling configuration (`step_result_processor.rs:40,54`)
- ✅ Legitimate configuration parameter for environment-specific tuning
- ✅ Phase 1 protected this field as runtime-critical
- ✅ All 155 test failures caused by this missing field (+ shutdown_timeout_seconds)

**Action**: Add to `[queues.pgmq]` section in `base/common.toml`:
```toml
poll_interval_ms = 250  # Default polling interval for PGMQ queue checks
```

### Decision 2: Scope of Phase 1 Audit ✅ DECIDED

**Decision**: **Quick Fix** - Focus only on deserialization errors

**Rationale**:
- ✅ Audit revealed only 2 fields need action (both in `PgmqBackendConfig`)
- ✅ All other fields properly aligned between TOML and Rust
- ✅ Phase 1 cleanup was thorough - no unused fields lingering in Rust
- ✅ Test failures all trace to same root cause

**Result**: Comprehensive audit not needed - the problem is localized and well-defined.

### Decision 3: CLI Stub Commands ✅ DECIDED

**Decision**: **Defer to TAS-55** - Remove stubs, implement in dedicated ticket

**Rationale**:
- ✅ Commands require full `._docs` documentation system
- ✅ Foundation for `._docs` will be laid in Phase 2 (adding docs to kept fields)
- ✅ TAS-55 is already scheduled for configuration documentation
- ✅ Focus Phase 3 on fixing pre-existing CLI test failures instead

**Action**: Remove stub commands or mark as "TODO - TAS-55" in Phase 3.

---

## Testing Strategy

### Incremental Validation

After each field change:
```bash
# Quick build check
cargo build --all-features

# Targeted package tests
cargo nextest run --package tasker-shared

# If passing, commit
git add -A
git commit -m "refactor(TAS-50): remove unused field X from ConfigY"
```

### Final Validation

Before marking complete:
```bash
# Full test suite with no fail-fast
cargo nextest run --package tasker-shared --package tasker-orchestration \
  --package tasker-worker --package pgmq-notify --package tasker-client \
  --package tasker-core --no-fail-fast

# Expected: ~700+ passing, 0-4 acceptable CLI test failures
```

### Acceptance Criteria

- ✅ All 151 config deserialization tests passing
- ✅ Integration tests green
- ✅ Base TOML files match Rust struct requirements
- ✅ Merged config files generate correctly
- ✅ Runtime behavior unchanged (validated via e2e tests)
- ✅ CLI tests: fixed or documented as pre-existing/deferred
- ⚠️ `._docs` attributes added to kept fields (foundation for TAS-55)

---

## Timeline Estimate

**Phase 1** (Audit): 1-2 hours
- Review ~10-15 critical config fields
- Categorize REMOVE vs KEEP
- Document decisions

**Phase 2** (Execute): 2-4 hours
- Remove unused fields from Rust (5-10 fields estimated)
- Add necessary fields to base TOML (1-3 fields estimated)
- Validate incrementally

**Phase 3** (Finalize): 2-3 hours
- Fix 4 CLI tests OR document as deferred
- Regenerate merged configs
- Final validation
- Update documentation

**Total**: 5-9 hours of focused work

---

## CRITICAL DISCOVERY: CLI Generate Bug

**Status**: ⚠️ Bug Found During Phase 2 Validation
**Date**: 2025-10-24
**Impact**: Blocks Phase 2 completion

### The Bug

**Problem**: CLI `config generate` command produces context-only TOML files, NOT merged common+context files.

**Expected Behavior**:
```toml
# orchestration-test.toml (EXPECTED - merged common + orchestration)
[database]           # From common.toml
[queues.pgmq]        # From common.toml
[orchestration_events]  # From orchestration.toml
[backoff]           # From orchestration.toml
```

**Actual Behavior**:
```toml
# orchestration-test.toml (ACTUAL - only orchestration)
[orchestration_events]  # From orchestration.toml
[backoff]           # From orchestration.toml
# Missing: [database], [queues], [circuit_breakers] from common.toml!
```

### Root Cause Analysis

**File**: `tasker-client/src/bin/tasker-cli.rs:937`
```rust
// CLI generate command calls:
let merged_config = merger.merge_context(&context).map_err(...)?;
```

**File**: `tasker-shared/src/config/merger.rs:111`
```rust
pub fn merge_context(&mut self, context: &str) -> ConfigResult<String> {
    // This loads ONLY context-specific TOML (orchestration.toml, worker.toml)
    let merged_toml_value = self.loader.load_context_toml(context)?;
    // Does NOT merge common.toml into output!
}
```

**File**: `tasker-shared/src/config/unified_loader.rs:631`
```rust
pub fn load_context_toml(&mut self, context_name: &str) -> ConfigResult<toml::Value> {
    // Loads base/{context}.toml + environments/{env}/{context}.toml
    // Does NOT include common.toml!
}
```

### Why This Breaks Runtime Loading

**File**: `tasker-shared/src/config/manager.rs:226-254`
```rust
pub fn load_from_single_file(..., context: ConfigContext) -> ConfigResult<...> {
    // Runtime expects MERGED file with BOTH common and context sections:
    let common: CommonConfig = toml::from_str(&contents)?;  // Needs [database], [queues], etc.
    let orchestration: OrchestrationConfig = toml::from_str(&contents)?;  // Needs [orchestration_events], [backoff]
    Box::new((common, orchestration))
}
```

**Result**: All tests calling `load_from_single_file()` with generated config files fail with:
```
missing field `poll_interval_ms` in `queues.pgmq`
```

Because `[queues.pgmq]` section doesn't exist in the generated file!

### The Fix

**Option 1: Fix CLI Generate (Recommended)**
Modify `ConfigMerger::merge_context()` to merge common.toml + context.toml together:

```rust
// In tasker-shared/src/config/merger.rs
pub fn merge_context(&mut self, context: &str) -> ConfigResult<String> {
    // 1. Load common.toml
    let mut merged = self.loader.load_context_toml("common")?;

    // 2. Load context.toml (orchestration/worker)
    let context_config = self.loader.load_context_toml(context)?;

    // 3. Merge context on top of common
    self.merge_toml_values(&mut merged, context_config)?;

    // 4. Return merged TOML string
    toml::to_string_pretty(&merged)?
}
```

**Option 2: Change Runtime Loading**
Load common.toml + context.toml separately instead of from single merged file.
- **Con**: Defeats the purpose of single-file deployment
- **Con**: More complex configuration management in K8s
- **Con**: Not what the code was designed for (manager.rs:160-188 comment)

**Decision**: **Use Option 1** - Fix the CLI generate command.

### Impact on Phase 2 Plan

**New Step Required**: Fix CLI generate command BEFORE adding fields to base/common.toml

**Revised Phase 2 Steps**:
1. ✅ **Add missing fields to base/common.toml** (DONE - see previous messages)
2. ⏭️ **Fix CLI generate to merge common + context** (NEW - required)
3. **Regenerate merged config files** (now will work correctly)
4. **Validate all tests pass**
5. **Commit changes**

---

## Phase 2 Execution Plan

**Status**: ⏸️ Paused - Fixing CLI generate bug first
**Expected Duration**: 60-90 minutes (was 30-60, added 30min for CLI fix)
**Risk Level**: Low (adding back runtime-critical fields + fixing merge logic)

### Step-by-Step Execution

**Step 1: Add Missing Fields to base/common.toml** ✅ COMPLETE

Fields added with `._docs` attributes - see earlier session messages for full implementation.

**Step 2: Fix CLI Generate Command** ⏭️ IN PROGRESS (20 minutes)

Modify `ConfigMerger::merge_context()` to merge common + context:

```rust
// File: tasker-shared/src/config/merger.rs:111
pub fn merge_context(&mut self, context: &str) -> ConfigResult<String> {
    info!(
        "Merging context '{}' for environment '{}' (including common config)",
        context, self.environment
    );

    // 1. Load common configuration as base
    let mut merged = self.loader.load_context_toml("common")?;
    debug!("Loaded common config as base: {} bytes", merged.to_string().len());

    // 2. Load context-specific configuration
    let context_config = self.loader.load_context_toml(context)?;
    debug!(
        "Loaded {} config: {} bytes",
        context,
        context_config.to_string().len()
    );

    // 3. Merge context on top of common (context wins in conflicts)
    if let (toml::Value::Table(ref mut base_table), toml::Value::Table(context_table)) =
        (&mut merged, context_config)
    {
        for (key, value) in context_table {
            base_table.insert(key, value);
        }
    }

    debug!("Merged config total: {} bytes", merged.to_string().len());

    // 4. Convert to TOML string
    let merged_toml_string = toml::to_string_pretty(&merged).map_err(|e| {
        ConfigurationError::json_serialization_error(
            format!("Failed to serialize merged {} config", context),
            e,
        )
    })?;

    info!(
        "Successfully merged {} configuration with common ({} bytes)",
        context,
        merged_toml_string.len()
    );

    Ok(merged_toml_string)
}
```

**Validation After Fix**:
```bash
# Test generation command
cargo run --quiet --package tasker-client --bin tasker-cli -- config generate \
  --context orchestration --environment test \
  --output /tmp/test-orch.toml

# Verify common sections are present
grep "\[database\]" /tmp/test-orch.toml
grep "\[queues.pgmq\]" /tmp/test-orch.toml
grep "poll_interval_ms" /tmp/test-orch.toml

# Should see both common and orchestration sections
```

**Step 3: Regenerate Merged Config Files** (10 minutes)
```bash
# Test environment
cargo run --bin tasker-cli -- config generate \
  --context orchestration --environment test \
  --output config/tasker/orchestration-test.toml

cargo run --bin tasker-cli -- config generate \
  --context worker --environment test \
  --output config/tasker/worker-test.toml

# Production environment
cargo run --bin tasker-cli -- config generate \
  --context orchestration --environment production \
  --output config/tasker/orchestration-production.toml

cargo run --bin tasker-cli -- config generate \
  --context worker --environment production \
  --output config/tasker/worker-production.toml
```

**Step 3: Validate Changes** (20 minutes)
```bash
# Quick build check
cargo build --all-features

# Run full test suite
cargo nextest run --package tasker-shared --package tasker-orchestration \
  --package tasker-worker --package pgmq-notify --package tasker-client \
  --package tasker-core --no-fail-fast

# Expected: 700+ tests passing, only 4 pre-existing CLI failures
```

**Step 4: Commit Changes** (5 minutes)
```bash
git add config/tasker/base/common.toml
git add config/tasker/*.toml
git commit -m "fix(TAS-50): add missing PGMQ fields to base configuration

Add poll_interval_ms and shutdown_timeout_seconds to base/common.toml.
These fields are used in runtime polling and shutdown configuration.

- Fixes 155 config deserialization test failures
- Adds ._docs attributes for TAS-55 foundation
- Regenerates merged config files for all environments

Refs: docs/ticket-specs/TAS-50/plan.md (Phase 2)"
```

---

## Next Steps

1. ✅ **Phase 1 Complete** - Audit complete, action manifest documented
2. ⏭️ **Execute Phase 2** - Add missing fields, regenerate configs, validate
3. **Execute Phase 3** - Fix or document 4 CLI test failures
4. **Final validation** - Full test suite run
5. **Update documentation** - Mark plan complete
6. **Ready for PR** - Merge to main

---

## Success Criteria Summary

**When this plan is complete**:
- ✅ All config-related tests passing (700+ tests)
- ✅ Base TOML files contain only necessary, documented fields
- ✅ Rust config structs align with base TOML
- ✅ Merged config files generate correctly from base + environment
- ✅ Runtime behavior unchanged (e2e tests green)
- ✅ Foundation laid for TAS-55 documentation system (`._docs` attributes)
- ✅ CLI tools functional (generate, validate) or stubs removed
- ✅ Branch ready for PR to main

---

**Ready to begin Phase 1 audit?**

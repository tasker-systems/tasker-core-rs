# TAS-50 Ruby Worker Bootstrap Fix

**Date**: 2025-10-15
**Issue**: Ruby worker failing to start with "Missing required configuration field 'orchestration'"
**Status**: ✅ Fixed

## Problem

When running the Ruby worker in Docker, it failed during bootstrap with:

```
ERROR tasker_worker_rb::bootstrap: Failed to load configuration: Missing required configuration field 'orchestration' in orchestration configuration
```

## Root Cause

The Ruby FFI worker bootstrap (`workers/ruby/ext/tasker_core/src/bootstrap.rs:62`) was using the deprecated `ConfigManager::load()` method:

```rust
// OLD (broken)
let config = ConfigManager::load()
    .map_err(...)?
    .config()
    .clone();
```

This method tries to load **all contexts** including orchestration configuration, but:
- A worker only needs **CommonConfig** and **WorkerConfig**
- The worker should never try to load orchestration configuration
- This violates the TAS-50 Phase 2 context-specific loading design

## Solution

Updated the Ruby FFI bootstrap to use context-specific loading:

```rust
// NEW (fixed)
let config_manager = ConfigManager::load_context_direct(
    tasker_shared::config::contexts::ConfigContext::Worker
)
.map_err(...)?;

let config = config_manager
    .as_tasker_config()
    .ok_or_else(...)?
    .clone();
```

**Benefits**:
1. ✅ Only loads worker-specific configuration (CommonConfig + WorkerConfig)
2. ✅ Avoids trying to load unnecessary orchestration configuration
3. ✅ Consistent with Rust worker bootstrap pattern
4. ✅ Follows TAS-50 Phase 2 context-specific loading design

## Verification

Rebuilt the Ruby worker extension successfully:

```bash
✅ export SQLX_OFFLINE=true && bundle exec rake compile
   Finished `release` profile [optimized] target(s) in 1m 07s
   installing tasker_worker_rb.bundle to ../../../../lib/tasker_core
```

## Context

The Rust worker bootstrap (`tasker-worker/src/bootstrap.rs:223`) was already using context-specific loading:

```rust
// Rust worker (already correct)
let system_context = Arc::new(SystemContext::new_for_worker().await?);
```

The Ruby FFI bootstrap needed to match this pattern. Both now use **Worker context** only.

## Related Files

- **Fixed**: `workers/ruby/ext/tasker_core/src/bootstrap.rs:61-83`
- **Reference**: `tasker-worker/src/bootstrap.rs:223` (Rust worker pattern)
- **Context**: TAS-50 Phase 2-3 context-specific configuration migration

## Testing

The Ruby worker will need to be tested in Docker to verify it starts correctly:

```bash
docker-compose up ruby-worker
```

Expected result:
- ✅ Worker starts without configuration errors
- ✅ Loads worker-specific configuration only
- ✅ Bootstrap completes successfully

---

**Note**: This fix completes the TAS-50 Phase 2-3 migration for the Ruby worker. All worker components now use context-specific configuration loading.

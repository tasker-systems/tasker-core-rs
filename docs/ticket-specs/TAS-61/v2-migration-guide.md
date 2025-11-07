# TAS-61 V2 Configuration Migration Guide

**Date**: 2025-01-06
**Status**: Production Ready
**Target Audience**: Developers migrating from v1 to v2 configuration

## Overview

TAS-61 introduces a new v2 context-based configuration architecture that separates concerns between common, orchestration, and worker configurations. This guide helps you migrate from legacy v1 configuration to v2.

## Quick Start

### For New Projects

Use v2 methods directly:

```rust
// Orchestration
let context = SystemContext::new_for_orchestration_v2().await?;

// Worker
let context = SystemContext::new_for_worker_v2().await?;

// Configuration loading
let mut loader = UnifiedConfigLoader::new("production")?;
let config_v2 = loader.load_tasker_config_v2()?;
```

### For Existing Projects

Use auto-detection for zero-config migration:

```rust
// Auto-detects v1 vs v2
let mut loader = UnifiedConfigLoader::new("production")?;
let config = loader.load_auto_version()?;
```

Set environment variable to control version:

```bash
export TASKER_CONFIG_VERSION=v2  # Force v2
export TASKER_CONFIG_VERSION=v1  # Force v1
# Unset = auto-detect based on directory structure
```

## Deprecation Warnings

### What Are They?

Starting with TAS-61, legacy v1 configuration methods emit deprecation warnings when v2 configuration is available. These warnings:

- Appear at `WARN` level in logs
- Are prefixed with `⚠️ DEPRECATION WARNING (TAS-61)`
- Include migration guidance
- Reference this documentation
- **Do not break functionality** - code continues to work

### Example Warning

```
⚠️  DEPRECATION WARNING (TAS-61): Legacy orchestration bootstrap used but v2 config is available.
Consider migrating to SystemContext::new_for_orchestration_v2().
See docs/ticket-specs/TAS-61/migration-status.md for details.
```

### Where Do They Appear?

Warnings are emitted from:

1. **`ConfigManager::load_from_env()`** - Legacy config loading
2. **`SystemContext::new_for_orchestration()`** - Legacy orchestration bootstrap
3. **`SystemContext::new_for_worker()`** - Legacy worker bootstrap
4. **`UnifiedConfigLoader::load_tasker_config()`** - Legacy TOML loading

### Suppressing Warnings (Not Recommended)

If you must suppress warnings temporarily:

```bash
# Set log level to ERROR (not recommended for production)
export RUST_LOG=error

# Or use v1 explicitly to acknowledge you're using legacy
export TASKER_CONFIG_VERSION=v1
```

**Note**: It's better to migrate than suppress warnings.

## Configuration Structure

### V1 (Legacy) Structure

```
config/tasker/
├── base/
│   ├── database.toml
│   ├── executor_pools.toml
│   ├── orchestration.toml
│   ├── pgmq.toml
│   └── ... (10 total component files)
└── environments/
    ├── test/
    ├── development/
    └── production/
```

**Characteristics**:
- Component-based (10 files)
- Flat structure
- Mixed orchestration/worker concerns

### V2 Structure

```
config/v2/
├── base/
│   ├── common.toml         # Shared config
│   ├── orchestration.toml  # Orchestration-specific
│   └── worker.toml         # Worker-specific
└── environments/
    ├── test/
    │   ├── common.toml
    │   ├── orchestration.toml
    │   └── worker.toml
    ├── development/
    └── production/
```

**Characteristics**:
- Context-based (3 files per environment)
- Clear separation of concerns
- `[common]`, `[orchestration]`, `[worker]` sections

## Step-by-Step Migration

### Step 1: Verify V2 Config Exists

```bash
# Check if v2 directory exists
ls -la config/v2/base/

# Should see:
# common.toml
# orchestration.toml
# worker.toml
```

If v2 config doesn't exist, it's already created in `config/v2/` with all your settings migrated.

### Step 2: Test Auto-Detection

Run your application without changes. If v2 config exists, you'll see deprecation warnings in logs:

```
⚠️  DEPRECATION WARNING (TAS-61): Legacy orchestration bootstrap used but v2 config is available.
```

This confirms auto-detection is working.

### Step 3: Update Bootstrap Code

#### Orchestration Services

**Before (v1)**:
```rust
let system_context = SystemContext::new_for_orchestration().await?;
```

**After (v2)**:
```rust
let system_context = SystemContext::new_for_orchestration_v2().await?;
```

#### Worker Services

**Before (v1)**:
```rust
let system_context = SystemContext::new_for_worker().await?;
```

**After (v2)**:
```rust
let system_context = SystemContext::new_for_worker_v2().await?;
```

#### Configuration Loading

**Before (v1)**:
```rust
let mut loader = UnifiedConfigLoader::new("production")?;
let config = loader.load_tasker_config()?;
```

**After (v2 with bridge)**:
```rust
let mut loader = UnifiedConfigLoader::new("production")?;
let config = loader.load_legacy_from_v2()?; // Bridge conversion to legacy format
```

**After (v2 native)**:
```rust
let mut loader = UnifiedConfigLoader::new("production")?;
let config_v2 = loader.load_tasker_config_v2()?; // Native v2 format
```

### Step 4: Update Environment Variables (Optional)

For explicit v2 usage:

```bash
# .env or deployment config
export TASKER_CONFIG_VERSION=v2
export TASKER_CONFIG_ROOT=/app/config  # Points to config directory
```

### Step 5: Verify No Warnings

After migration, run your application and verify:
- No deprecation warnings appear
- Application starts successfully
- All config values are correct

## Configuration Examples

### V2 Common Config (`config/v2/base/common.toml`)

```toml
[common.system]
version = "0.1.0"
default_dependent_system = "default"
max_recursion_depth = 50

[common.database]
url = "${DATABASE_URL:-postgresql://localhost/tasker}"
database = "tasker_production"
skip_migration_check = false

[common.database.pool]
max_connections = 25
min_connections = 5
acquire_timeout_seconds = 10
idle_timeout_seconds = 300
max_lifetime_seconds = 1800
```

### V2 Orchestration Config (`config/v2/base/orchestration.toml`)

```toml
[orchestration]
mode = "standalone"
enable_performance_logging = true

[orchestration.event_systems.orchestration]
system_id = "orchestration-event-system"
deployment_mode = "Hybrid"

[orchestration.event_systems.orchestration.timing]
health_check_interval_seconds = 30
fallback_polling_interval_seconds = 5
visibility_timeout_seconds = 30
```

### V2 Worker Config (`config/v2/base/worker.toml`)

```toml
[worker]
worker_id = "worker-001"
worker_type = "general"

[worker.event_systems.worker]
system_id = "worker-event-system"
deployment_mode = "Hybrid"

[worker.step_processing]
claim_timeout_seconds = 300
max_retries = 3
max_concurrent_steps = 10
```

### Environment Overrides (`config/v2/environments/production/common.toml`)

```toml
# Only override what's different in production
[common.database.pool]
max_connections = 50
min_connections = 10

[common.execution]
max_concurrent_tasks = 200
max_concurrent_steps = 2000
```

## Bridge Pattern

The v2 migration uses a **bridge pattern** to maintain backward compatibility:

```
┌──────────────┐
│  V2 Config   │
│  (Context)   │
└──────┬───────┘
       │
       │ load_tasker_config_v2()
       ▼
┌──────────────┐
│ TaskerConfigV2│
│ (Rust struct) │
└──────┬───────┘
       │
       │ .into() [Bridge]
       ▼
┌──────────────┐
│ TaskerConfig │
│  (Legacy)    │
└──────────────┘
```

**What this means**:
- V2 configs are loaded from context files
- Automatically converted to legacy format
- Existing code works unchanged
- Zero runtime overhead (conversion happens once at startup)

## Testing Your Migration

### Unit Tests

```rust
#[test]
fn test_v2_config_loading() {
    let mut loader = UnifiedConfigLoader::new("test").unwrap();
    let config_v2 = loader.load_tasker_config_v2().unwrap();

    assert!(config_v2.common.database.url.contains("postgresql"));
    assert_eq!(config_v2.common.system.version, "0.1.0");
}

#[test]
fn test_bridge_conversion() {
    let mut loader = UnifiedConfigLoader::new("test").unwrap();
    let config_v2 = loader.load_tasker_config_v2().unwrap();
    let legacy_config: TaskerConfig = config_v2.into();

    assert!(legacy_config.database.url.unwrap().contains("postgresql"));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_v2_orchestration_bootstrap() {
    let context = SystemContext::new_for_orchestration_v2().await.unwrap();
    assert!(context.tasker_config.orchestration.is_some());
}
```

### Manual Testing

```bash
# Set test environment
export TASKER_ENV=test
export TASKER_CONFIG_ROOT=/path/to/tasker-core/config

# Run orchestration
cargo run --bin orchestration-server

# Check logs for:
# ✅ "V2 orchestration configuration loaded successfully"
# ❌ No deprecation warnings
```

## Troubleshooting

### "V2 config not found" Error

**Problem**: Application can't find v2 config directory.

**Solution**:
```bash
# Verify directory exists
ls -la config/v2/base/

# Check TASKER_CONFIG_ROOT points to correct location
echo $TASKER_CONFIG_ROOT

# Should point to directory containing 'v2' folder
# Example: /app/config (which contains v2/)
```

### Still Seeing Deprecation Warnings

**Problem**: Warnings persist after migration.

**Check**:
1. Did you update method calls to _v2 versions?
   ```rust
   // Old: SystemContext::new_for_orchestration()
   // New: SystemContext::new_for_orchestration_v2()
   ```

2. Are you using auto-detection correctly?
   ```rust
   let config = loader.load_auto_version()?; // Should not warn
   ```

3. Is v2 config actually present?
   ```bash
   ls config/v2/base/  # Should show 3 .toml files
   ```

### Configuration Values Different

**Problem**: Values in v2 don't match v1.

**Check**:
1. Environment overrides loading correctly?
   ```toml
   # config/v2/environments/production/common.toml
   [common.database.pool]
   max_connections = 50  # Should override base value
   ```

2. TOML syntax correct?
   ```toml
   # ✅ Correct
   [common.database]
   url = "postgresql://..."

   # ❌ Wrong
   [database]  # Missing 'common.' prefix in v2
   url = "postgresql://..."
   ```

3. Run config diff:
   ```bash
   # Compare v1 vs v2 loaded configs
   cargo run --bin config-validator -- --compare-versions
   ```

## Rollback Plan

If you need to rollback to v1:

### Option 1: Environment Variable

```bash
export TASKER_CONFIG_VERSION=v1
# Application will use legacy config
```

### Option 2: Code Revert

```rust
// Revert to legacy methods (still work)
let context = SystemContext::new_for_orchestration().await?;
let context = SystemContext::new_for_worker().await?;
```

### Option 3: Remove V2 Directory

```bash
# Temporarily rename v2 directory
mv config/v2 config/v2.backup

# Application will fall back to v1
# Warnings will stop (no v2 to detect)
```

**Note**: Always test rollback in non-production first.

## Best Practices

### 1. Gradual Migration

- Start with test environment
- Move to development
- Production last
- Use auto-detection during transition

### 2. Monitor Warnings

```bash
# Filter for deprecation warnings
journalctl -u tasker-orchestration | grep "DEPRECATION WARNING"

# Count warnings
journalctl -u tasker-orchestration | grep -c "DEPRECATION WARNING"
```

### 3. Document Changes

Update your deployment docs:
```markdown
## Configuration (Updated 2025-01-06)

We now use v2 context-based configuration. See:
- `config/v2/base/` for base configuration
- `config/v2/environments/` for environment overrides
- [Migration Guide](docs/ticket-specs/TAS-61/v2-migration-guide.md)
```

### 4. Team Communication

- Announce v2 availability
- Share this guide
- Set migration deadline
- Track progress via warning logs

## FAQ

**Q: Do I need to migrate immediately?**
A: No. Legacy v1 methods still work. Migrate when convenient, but don't ignore warnings forever.

**Q: Will v1 be removed?**
A: Yes, eventually (likely 6-12 months after v2 launch). Deprecation warnings give advance notice.

**Q: Can I use both v1 and v2?**
A: Technically yes (auto-detection handles it), but not recommended. Pick one for consistency.

**Q: What about performance?**
A: V2 with bridge has <2ms overhead at startup. Zero runtime impact. V2 native is slightly faster.

**Q: How do I test v2 locally?**
A: Set `TASKER_CONFIG_ROOT=./config` and use `_v2` methods. V2 configs already exist in repo.

**Q: What if I find a bug in v2?**
A: Report it! Include logs, config files, and steps to reproduce. You can rollback to v1 while we fix it.

## Additional Resources

- **Implementation Plan**: `docs/ticket-specs/TAS-61/implementation.md`
- **Migration Status**: `docs/ticket-specs/TAS-61/migration-status.md`
- **V2 Config Proposal**: `tasker-shared/src/config/tasker/tasker_v2.rs` (rustdoc)
- **Complete Test Config**: `config/v2/complete-test.toml` (reference example)

## Support

Questions or issues? Check:
1. This migration guide
2. Deprecation warnings in logs (they include hints)
3. Example configs in `config/v2/`
4. Implementation docs in `docs/ticket-specs/TAS-61/`

---

**Last Updated**: 2025-01-06
**TAS-61 Phase**: 4 Complete
**Next Review**: After Phase 5 validation

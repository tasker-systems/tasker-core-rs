# TAS-34 Supplemental: Configuration System Consolidation Plan

## Current State Analysis (August 16, 2025)

### Problem Statement
The configuration system has evolved into three overlapping implementations that violate our fail-fast principle:
1. **`src/config/mod.rs`** - Original configuration with legacy YAML support
2. **`src/config/loader.rs`** - Transitional wrapper with fallback behaviors
3. **`src/config/unified_loader.rs`** - Clean TAS-34 implementation (CORRECT)

This redundancy causes:
- Compile errors due to type mismatches between implementations
- Confusion about which system to use
- Silent fallbacks that violate fail-fast principle
- Maintenance burden of three parallel systems

### Current Implementation Status
- ✅ `unified_loader.rs` fully implements TAS-34 spec
- ❌ `loader.rs` still has emergency fallbacks and legacy support
- ❌ `mod.rs` contains duplicate functionality
- ❌ Ruby side has been simplified but expects unified behavior

## Consolidation Strategy

### Phase 1: Make UnifiedConfigLoader the Primary Implementation
**Goal**: Establish `unified_loader.rs` as the single source of truth

#### 1.1 Update mod.rs exports
```rust
// src/config/mod.rs
pub mod unified_loader;
pub use unified_loader::{UnifiedConfigLoader, ValidatedConfig};

// Remove or deprecate:
// - Direct TaskerConfig construction functions
// - YAML loading functions
// - Legacy validation functions
```

#### 1.2 Simplify ConfigManager to thin wrapper
```rust
// src/config/loader.rs - Simplified wrapper for compatibility
pub struct ConfigManager {
    loader: UnifiedConfigLoader,
    config: Arc<TaskerConfig>,
}

impl ConfigManager {
    pub fn load() -> ConfigResult<Arc<ConfigManager>> {
        let loader = UnifiedConfigLoader::new_from_env()?;
        let config = Arc::new(loader.load_tasker_config()?);
        Ok(Arc::new(ConfigManager { loader, config }))
    }
    
    pub fn config(&self) -> &TaskerConfig {
        &self.config
    }
    
    // No fallbacks, no legacy support
}
```

### Phase 2: Update All Consumers

#### 2.1 Orchestration System
- `src/orchestration/bootstrap.rs` - Use UnifiedConfigLoader directly
- `src/orchestration/orchestration_system.rs` - Remove ConfigManager dependency
- `src/orchestration/config.rs` - Delete if redundant with unified system

#### 2.2 FFI Bridge
- `src/ffi/shared/config.rs` - Already uses UnifiedConfigLoader ✅
- Keep WorkerConfigManager as-is (it correctly wraps UnifiedConfigLoader)

#### 2.3 Database Management
- Update database pool creation to use UnifiedConfigLoader
- Remove any YAML-based config paths

### Phase 3: Remove Legacy Code

#### 3.1 Files to Delete/Simplify
```
src/config/
├── mod.rs              -> Simplify to just exports
├── loader.rs           -> Keep minimal wrapper or delete
├── unified_loader.rs   -> PRIMARY (keep as-is)
├── component_loader.rs -> Delete if redundant
└── error.rs           -> Keep error types only
```

#### 3.2 Methods to Remove
From `loader.rs`:
- `emergency_fallback()` - Violates fail-fast
- `sanitize_config_for_logging()` - Move to unified if needed
- `is_component_based` field - Always true now
- `load_and_merge_config()` - Redundant with unified
- `expand_environment_variables()` - Handle in unified only

From `mod.rs`:
- All YAML loading functions
- Direct TaskerConfig builders
- Legacy validation functions

### Phase 4: Ruby Integration Cleanup

#### 4.1 Ruby Config Simplification (Already Done ✅)
- Config class now delegates to UnifiedConfig::Manager
- Removed all transformation and fallback methods
- Direct access to Rust-validated configuration

#### 4.2 Fix Remaining Ruby Issues
- Ensure proper symbol/string key handling
- Add nil checks with clear error messages
- Remove references to deprecated methods

## Implementation Order

### Step 1: Fix Immediate Compile Errors (DONE ✅)
- Fixed `PathBuf` vs `Path` issues in loader.rs
- Fixed `load_from_directory` signature mismatch

### Step 2: Create Minimal ConfigManager Wrapper
```rust
// New loader.rs - Compatibility wrapper only
use crate::config::{unified_loader::UnifiedConfigLoader, ConfigResult, TaskerConfig};
use std::sync::Arc;

pub struct ConfigManager {
    config: Arc<TaskerConfig>,
}

impl ConfigManager {
    pub fn load() -> ConfigResult<Arc<ConfigManager>> {
        let loader = UnifiedConfigLoader::new_from_env()?;
        let config = Arc::new(loader.load_tasker_config()?);
        Ok(Arc::new(ConfigManager { config }))
    }
    
    pub fn config(&self) -> &TaskerConfig {
        &self.config
    }
    
    // Minimal compatibility methods only
}
```

### Step 3: Update Bootstrap System
- Modify `orchestration/bootstrap.rs` to use UnifiedConfigLoader
- Remove all ConfigManager usage where possible
- Use WorkerConfigManager for FFI contexts

### Step 4: Clean mod.rs
```rust
// New mod.rs - Just exports and types
pub mod unified_loader;
pub mod error;

pub use error::{ConfigError, ConfigResult};
pub use unified_loader::{UnifiedConfigLoader, ValidatedConfig};

// Re-export types
pub use crate::types::config::TaskerConfig;

// Optional: Compatibility alias
pub use unified_loader::UnifiedConfigLoader as ConfigLoader;
```

### Step 5: Test and Validate
- Run all Rust tests with `--all-features`
- Run Ruby integration tests
- Verify no fallback behaviors occur
- Ensure fail-fast on all config errors

## Success Criteria

1. **Single Source of Truth**: Only `UnifiedConfigLoader` loads configuration
2. **No Fallbacks**: All configuration errors fail immediately
3. **Clean Separation**: FFI uses WorkerConfigManager, core uses UnifiedConfigLoader
4. **Type Safety**: No raw JSON/YAML manipulation outside loaders
5. **Test Coverage**: All paths tested for fail-fast behavior

## Benefits of Consolidation

1. **Reduced Complexity**: One system instead of three
2. **Clear Error Messages**: No silent failures or fallbacks
3. **Maintainability**: Single place to update configuration logic
4. **Performance**: No redundant loading or validation
5. **Correctness**: Guaranteed validation before use

## Migration Checklist

- [ ] Fix compile errors in loader.rs and bootstrap.rs
- [ ] Create minimal ConfigManager wrapper
- [ ] Update orchestration bootstrap to use UnifiedConfigLoader
- [ ] Clean up mod.rs to just exports
- [ ] Remove all YAML support code
- [ ] Remove all fallback behaviors
- [ ] Update tests to expect fail-fast behavior
- [ ] Run full test suite
- [ ] Update documentation

## Notes

- Keep WorkerConfigManager as-is - it correctly wraps UnifiedConfigLoader for FFI
- ValidatedConfig provides type-safe access to all components
- Component-based TOML is the ONLY supported format
- Environment detection uses standard precedence: TASKER_ENV > RAILS_ENV > RACK_ENV > APP_ENV > "development"

## Risk Mitigation

- **Risk**: Breaking existing consumers
- **Mitigation**: Keep thin ConfigManager wrapper for compatibility

- **Risk**: Ruby integration failures  
- **Mitigation**: WorkerConfigManager already handles this correctly

- **Risk**: Missing configuration in tests
- **Mitigation**: All tests already use proper TOML configs in config/tasker/

## Timeline

- Phase 1-2: 1 day (consolidate Rust side)
- Phase 3: 1 day (remove legacy code)
- Phase 4: Already complete (Ruby simplified)
- Testing: 1 day

Total: 3 days to complete consolidation
# TAS-50 Phase 2: Context-Specific TOML Configuration

**Status**: In Progress
**Created**: 2025-10-15
**Phase**: 2 of 3

## Overview

Phase 2 creates context-specific TOML configuration files and enables direct loading from them, while maintaining full backward compatibility with the monolithic configuration system.

## Goals

1. **Create context-specific TOML files** that replace monolithic configuration
2. **Enhance UnifiedConfigLoader** to support context-specific loading
3. **Update ConfigManager** to load directly from context TOML files
4. **Maintain 100% backward compatibility** during migration

## Architecture

### File Structure

```
config/tasker/base/
├── common.toml              # NEW: Shared infrastructure configuration
├── orchestration.toml       # NEW: Orchestration-specific configuration
├── worker.toml              # NEW: Worker-specific configuration
└── [existing components]    # KEEP: For backward compatibility

config/tasker/environments/{test,development,production}/
├── common.toml              # NEW: Environment-specific overrides
├── orchestration.toml       # NEW: Environment-specific overrides
└── worker.toml              # NEW: Environment-specific overrides
```

### Context-Specific File Mappings

#### common.toml
Combines:
- `database.toml` - Database connection and pooling
- `queues.toml` - PGMQ queue configuration
- `execution.toml` - Environment settings
- `mpsc_channels.toml` (shared section only) - Shared MPSC channels

Fields:
- `[database]` - Full database configuration
- `[queues]` - Full queue configuration
- `[execution]` - Environment field only
- `[mpsc_channels.shared]` - Shared channel configuration

#### orchestration.toml
Combines:
- `backoff.toml` - Retry and backoff configuration
- `orchestration.toml` - Orchestration system settings
- `event_systems.toml` (orchestration + task_readiness sections)
- `mpsc_channels.toml` (orchestration + task_readiness sections)
- `task_readiness.toml` - Task readiness system settings

Fields:
- `[backoff]` - Full backoff configuration
- `[orchestration]` - Full orchestration system configuration
- `[event_systems.orchestration]` - Orchestration event system
- `[event_systems.task_readiness]` - Task readiness event system
- `[mpsc_channels.orchestration]` - Orchestration MPSC channels
- `[mpsc_channels.task_readiness]` - Task readiness MPSC channels
- `[task_readiness]` - Full task readiness configuration

#### worker.toml
Combines:
- `worker.toml` - Worker system settings
- `event_systems.toml` (worker section)
- `mpsc_channels.toml` (worker section)

Fields:
- `[worker]` - Full worker system configuration
- `[event_systems.worker]` - Worker event system
- `[mpsc_channels.worker]` - Worker MPSC channels
- `template_path` - NEW: Optional template discovery path

### Implementation Steps

#### Step 1: Create Context-Specific TOML Files (Manual Generation)

Create new TOML files in `config/tasker/base/`:
1. `common.toml` - Combine database + queues + execution environment + shared channels
2. `orchestration.toml` - Combine backoff + orchestration + orchestration events + orchestration channels
3. `worker.toml` - Combine worker system + worker events + worker channels

Create environment overrides in `config/tasker/environments/{env}/`:
1. `common.toml` - Environment-specific database, queue, and channel overrides
2. `orchestration.toml` - Environment-specific orchestration overrides
3. `worker.toml` - Environment-specific worker overrides

#### Step 2: Enhance UnifiedConfigLoader

Add context-specific loading methods:
```rust
impl UnifiedConfigLoader {
    /// Load CommonConfig from common.toml
    pub fn load_common_config(&mut self) -> ConfigResult<CommonConfig>;

    /// Load OrchestrationConfig from orchestration.toml
    pub fn load_orchestration_config(&mut self) -> ConfigResult<OrchestrationConfig>;

    /// Load WorkerConfig from worker.toml
    pub fn load_worker_config(&mut self) -> ConfigResult<WorkerConfig>;
}
```

Each method:
1. Loads base context file (e.g., `config/tasker/base/common.toml`)
2. Applies environment overrides (e.g., `config/tasker/environments/test/common.toml`)
3. Deserializes into context struct
4. Validates configuration

#### Step 3: Update ConfigManager

Add direct context loading:
```rust
impl ConfigManager {
    /// Load context-specific configuration directly from TOML files
    pub fn load_context_direct(context: ConfigContext)
        -> ConfigResult<ContextConfigManager>;
}
```

Deprecation strategy:
- `load_for_context()` - Phase 1 method (converts from TaskerConfig) - keep for backward compat
- `load_context_direct()` - Phase 2 method (loads from context TOML) - new preferred method
- Both coexist during Phase 2 migration

#### Step 4: Testing

Create comprehensive tests:
1. Test loading each context type from TOML
2. Test environment overrides work correctly
3. Test backward compatibility with Phase 1 loading
4. Test validation catches configuration errors
5. Test field-level equivalence between Phase 1 and Phase 2 loading

## Benefits

### Configuration Efficiency
- **Orchestration**: Load only 51 fields instead of 161 (68% reduction)
- **Worker**: Load only 70 fields instead of 161 (56% reduction)
- **Combined**: Load 161 fields when needed (no change)

### Development Experience
- **Smaller files**: Each context file ~100-200 lines vs 1000+ line monolith
- **Clear separation**: Easy to see what configuration affects what system
- **Environment overrides**: Override only relevant fields per context

### Deployment Flexibility
- **K8s ConfigMaps**: Separate ConfigMaps per deployment type
- **ENV variable overrides**: DATABASE_URL and TASKER_TEMPLATE_PATH still work
- **Independent updates**: Update orchestration config without touching worker config

## Backward Compatibility

### Phase 2 Maintains Full Compatibility
1. **Component files remain**: All existing `config/tasker/base/*.toml` files stay
2. **TaskerConfig loading works**: `ConfigManager::load()` continues to work
3. **Phase 1 methods work**: `load_for_context()` continues to work
4. **Two loading paths coexist**:
   - Old path: Load TaskerConfig → Convert to context configs
   - New path: Load context TOML → Directly into context configs

### Migration Path
Phase 2 enables gradual migration:
1. Create context TOML files alongside existing component files
2. Test new loading path thoroughly
3. Begin using `load_context_direct()` in new code
4. Gradually migrate existing code to use context loading
5. Phase 3 removes old loading path after full migration

## Non-Goals (Phase 2)

These are deferred to Phase 3:
- ❌ Remove monolithic TaskerConfig loading
- ❌ Remove component-based TOML files
- ❌ Deprecate `load_for_context()` method
- ❌ Make context loading the only way to load configuration

## Success Criteria

- ✅ Context-specific TOML files created for all 3 environments
- ✅ UnifiedConfigLoader can load each context type
- ✅ ConfigManager `load_context_direct()` method works
- ✅ All existing tests still pass (backward compatibility)
- ✅ New tests verify context TOML loading works correctly
- ✅ Field-level equivalence between Phase 1 and Phase 2 loading

## Next Phase (Phase 3)

Phase 3 will:
1. Migrate all code to use context-specific configurations
2. Deprecate monolithic TaskerConfig loading
3. Remove backward compatibility layer
4. Make context-specific loading the only supported method

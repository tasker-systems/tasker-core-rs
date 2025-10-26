# TAS-50 Phase 3: Single-File Configuration Runtime Loading

**Status**: ✅ Complete
**Date**: October 17, 2025
**Implementation**: Greenfield (no backward compatibility)

## Overview

Phase 3 completes the TAS-50 configuration modernization by enabling runtime systems (orchestration and worker) to load configuration from single merged TOML files instead of directory structures. This ensures deployment certainty: the exact configuration used at runtime is known at build time.

## Key Principle

**Fail Loudly**: All configuration errors halt execution immediately with explicit error messages explaining exactly what failed.

## Architecture

### Configuration Flow

```
┌─────────────────────────────────────────────────────────┐
│ Build Time (CI/CD)                                      │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  tasker-cli config generate \                           │
│    --environment production \                           │
│    --context orchestration \                            │
│    --output orchestration-production.toml               │
│                                                          │
│  Result: Single merged file with:                       │
│  - base/common.toml                                     │
│  - base/orchestration.toml                              │
│  - environments/production/common.toml (overrides)      │
│  - environments/production/orchestration.toml (overrides)│
│                                                          │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ Deploy Time (Docker/K8s)                                │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Environment: TASKER_CONFIG_PATH=/app/config/          │
│               orchestration-production.toml             │
│                                                          │
│  Volume Mount: ./config/tasker:/app/config/tasker:ro   │
│                                                          │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│ Runtime (Orchestration/Worker)                          │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  SystemContext::new_for_orchestration() {               │
│    1. Read TASKER_CONFIG_PATH env var → FAIL if not set│
│    2. Load TOML file → FAIL if not found               │
│    3. Parse TOML → FAIL if invalid syntax              │
│    4. Deserialize to structs → FAIL if schema mismatch │
│    5. Validate configs → FAIL if constraints violated   │
│    6. Use validated config                              │
│  }                                                       │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

## Implementation Details

### 1. Configuration Loading Layer

#### UnifiedConfigLoader (`tasker-shared/src/config/unified_loader.rs`)

**New Static Method**:
```rust
pub fn load_from_single_file(path: &Path) -> ConfigResult<toml::Value>
```

**Responsibilities**:
- Read file from path (FAIL if not exists)
- Parse TOML (FAIL if invalid)
- Apply environment variable substitution
- Return parsed TOML value

**No State Required**: Static method - doesn't need loader instance.

#### ConfigManager (`tasker-shared/src/config/manager.rs`)

**New Method**:
```rust
pub fn load_from_single_file(
    path: &Path,
    context: ConfigContext,
) -> ConfigResult<ContextConfigManager>
```

**Explicit Context Loading**:
- `ConfigContext::Orchestration` → `CommonConfig` + `OrchestrationConfig`
- `ConfigContext::Worker` → `CommonConfig` + `WorkerConfig`

**Validation Chain** (each step FAILS LOUDLY):
1. Load TOML via `UnifiedConfigLoader::load_from_single_file()`
2. Deserialize into context-specific structs
3. Validate each struct independently
4. Return validated `ContextConfigManager`

**Error Messages Include**:
- Exact file path that failed
- Specific operation that failed (parse/deserialize/validate)
- Underlying error details

### 2. System Bootstrap Layer

#### SystemContext (`tasker-shared/src/system_context.rs`)

**Updated Methods**:

```rust
pub async fn new_for_orchestration() -> TaskerResult<Self>
pub async fn new_for_worker() -> TaskerResult<Self>
```

**Environment Variable Requirement**:
- `TASKER_CONFIG_PATH` must be set
- FAIL with helpful error if not set
- Error message includes example CLI command to generate file

**Load Path**:
```
TASKER_CONFIG_PATH → ConfigManager::load_from_single_file() →
  ContextConfigManager → TaskerConfig (backward compat) → SystemContext
```

### 3. Docker Deployment Configuration

#### docker-compose.test.yml

**Orchestration Service**:
```yaml
environment:
  TASKER_CONFIG_PATH: /app/config/tasker/orchestration-test.toml
volumes:
  - ../config/tasker:/app/config/tasker:ro
```

**Worker Service**:
```yaml
environment:
  TASKER_CONFIG_PATH: /app/config/tasker/worker-test.toml
volumes:
  - ../config/tasker:/app/config/tasker:ro
```

**Ruby Worker Service**:
```yaml
environment:
  TASKER_CONFIG_PATH: /app/config/tasker/worker-test.toml
volumes:
  - ../config/tasker:/app/config/tasker:ro
```

#### docker-compose.prod.yml

**Orchestration Service**:
```yaml
environment:
  TASKER_CONFIG_PATH: /app/config/tasker/orchestration-production.toml
volumes:
  - ../config/tasker:/app/config/tasker:ro
```

**Worker Service**:
```yaml
environment:
  TASKER_CONFIG_PATH: /app/config/tasker/worker-production.toml
volumes:
  - ../config/tasker:/app/config/tasker:ro
```

### 4. Generated Configuration Files

**Location**: `config/tasker/`

**Files**:
- `orchestration-test.toml` (5.7 KB)
- `worker-test.toml` (5.4 KB)
- `orchestration-production.toml` (5.7 KB)
- `worker-production.toml` (5.5 KB)

**Generation Command**:
```bash
# Orchestration config for production
tasker-cli config generate \
  --environment production \
  --context orchestration \
  --output config/tasker/orchestration-production.toml

# Worker config for production
tasker-cli config generate \
  --environment production \
  --context worker \
  --output config/tasker/worker-production.toml
```

## Error Handling Philosophy

### Fail Loudly Principle

Every failure includes:
1. **What failed**: Specific operation (file read, parse, deserialize, validate)
2. **Where it failed**: Exact file path
3. **Why it failed**: Underlying error details
4. **How to fix**: Example CLI command or configuration guidance

### Example Error Messages

**Missing Environment Variable**:
```
TASKER_CONFIG_PATH environment variable is not set.
This must point to a single merged configuration file generated by:
tasker-cli config generate --environment <env> --context orchestration
```

**File Not Found**:
```
Failed to load orchestration configuration from /app/config/orchestration-prod.toml:
Configuration file not found at: /app/config/orchestration-prod.toml
```

**Deserialization Failure**:
```
Failed to load orchestration configuration from /app/config/orchestration-prod.toml:
Failed to deserialize CommonConfig: missing field 'database.pool.max_connections'
```

**Validation Failure**:
```
Failed to load orchestration configuration from /app/config/orchestration-prod.toml:
CommonConfig validation failed: [
  "database.pool.max_connections must be greater than 0",
  "queues.default_batch_size must be between 1 and 1000"
]
```

## Benefits

### 1. Deployment Certainty

**Before (Phase 2)**: Runtime merges base + environment configs
**Problem**: Configuration only known at runtime, hard to audit

**After (Phase 3)**: Single file generated at build time
**Benefit**: Exact runtime configuration known before deployment

### 2. Simplified Debugging

**Before**: "Which TOML files were loaded? What overrides applied?"
**After**: "Here's the single file - this is exactly what's running"

### 3. Configuration Auditing

**Before**: Need to mentally merge multiple TOML files
**After**: Single file can be:
- Version controlled
- Code reviewed
- Compared across environments
- Attached to deployment records

### 4. Deployment Simplicity

**Before**: Mount directory structure, rely on environment variable for overrides
**After**: Mount single file at known path

**Docker Compose**:
```yaml
# Simple and explicit
volumes:
  - ../config/tasker:/app/config/tasker:ro
environment:
  TASKER_CONFIG_PATH: /app/config/tasker/orchestration-production.toml
```

**Kubernetes ConfigMap**:
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: orchestration-config
data:
  orchestration-production.toml: |
    # Single merged configuration file
    # Generated: 2025-10-17
    # Environment: production
    # Context: orchestration
    [database]
    # ...
```

### 5. Environment Separation

Each context and environment has its own dedicated file:
- No risk of test config leaking into production
- Clear separation of orchestration vs worker configs
- Easy to spot differences between environments

## Migration Path

### Greenfield Approach

**No backward compatibility**: This is intentional design.

**Rationale**:
1. Clear separation from legacy approach
2. Forces explicit, intentional configuration
3. Simplifies codebase (no dual code paths)
4. Aligns with "explicit is better than implicit" principle

### Legacy Support (Tests Only)

**Method**: `ConfigManager::load_context_direct()`
**Status**: Marked as LEGACY
**Use Case**: Test compatibility during transition
**Production Use**: ❌ Not supported

## Files Changed

### Core Implementation
1. `tasker-shared/src/config/unified_loader.rs` - Added `load_from_single_file()`
2. `tasker-shared/src/config/manager.rs` - Added `load_from_single_file()` with context
3. `tasker-shared/src/system_context.rs` - Updated bootstrap methods to use `TASKER_CONFIG_PATH`

### Docker Configuration
4. `docker/docker-compose.test.yml` - Updated 3 services (orchestration, worker, ruby-worker)
5. `docker/docker-compose.prod.yml` - Updated 2 services (orchestration, worker)
6. `docker/docker-compose.dev.yml` - No changes needed (infrastructure only)

### Generated Configurations
7. `config/tasker/orchestration-test.toml` - ✅ Generated
8. `config/tasker/worker-test.toml` - ✅ Generated
9. `config/tasker/orchestration-production.toml` - ✅ Generated
10. `config/tasker/worker-production.toml` - ✅ Generated

## Testing Strategy

### Compilation Verification

```bash
# Verify all packages compile with changes
SQLX_OFFLINE=true cargo check --all-features
```

**Result**: ✅ All packages compile successfully

### Configuration Generation

```bash
# Generate all required configuration files
tasker-cli config generate --environment test --context orchestration \
  --output config/tasker/orchestration-test.toml

tasker-cli config generate --environment test --context worker \
  --output config/tasker/worker-test.toml

tasker-cli config generate --environment production --context orchestration \
  --output config/tasker/orchestration-production.toml

tasker-cli config generate --environment production --context worker \
  --output config/tasker/worker-production.toml
```

**Result**: ✅ All files generated (5.4-5.7 KB each)

### Runtime Integration

**Test Environment**:
```bash
export TASKER_CONFIG_PATH=/app/config/tasker/orchestration-test.toml
# Orchestration should start successfully and load config
```

**Production Environment**:
```bash
export TASKER_CONFIG_PATH=/app/config/tasker/orchestration-production.toml
# Orchestration should start successfully and load config
```

## Future Enhancements

### 1. Configuration Versioning

Add version metadata to generated files:
```toml
[metadata]
generated_at = "2025-10-17T15:36:00Z"
generated_by = "tasker-cli v0.1.0"
git_commit = "abc123def"
environment = "production"
context = "orchestration"
```

### 2. Configuration Diff Tool

```bash
tasker-cli config diff \
  config/tasker/orchestration-production.toml \
  config/tasker/orchestration-staging.toml
```

### 3. Configuration Validation API

```bash
# Validate generated file before deployment
tasker-cli config validate \
  --file config/tasker/orchestration-production.toml \
  --context orchestration
```

### 4. Hot Reload Support

Watch for file changes and reload configuration without restart (future phase).

## Lessons Learned

### 1. Explicit is Better Than Implicit

**Decision**: Require explicit `ConfigContext` parameter
**Benefit**: No guessing, clear intent, type-safe

### 2. Fail Fast, Fail Loud

**Decision**: No fallbacks, no silent failures
**Benefit**: Configuration errors caught immediately, not during operation

### 3. Greenfield > Backward Compatibility

**Decision**: No support for legacy directory-based loading
**Benefit**: Simpler code, clearer expectations, forces modernization

### 4. Static Methods for Stateless Operations

**Decision**: `load_from_single_file()` is static
**Benefit**: No need for loader instance, simpler call site

### 5. Environment Variables for Runtime Configuration

**Decision**: Use `TASKER_CONFIG_PATH` env var
**Benefit**: Standard Docker/K8s pattern, easy to override per environment

## Completion Checklist

- [x] Implement `UnifiedConfigLoader::load_from_single_file()`
- [x] Implement `ConfigManager::load_from_single_file()`
- [x] Update `SystemContext::new_for_orchestration()`
- [x] Update `SystemContext::new_for_worker()`
- [x] Update `docker-compose.test.yml`
- [x] Update `docker-compose.prod.yml`
- [x] Generate `orchestration-test.toml`
- [x] Generate `worker-test.toml`
- [x] Generate `orchestration-production.toml`
- [x] Generate `worker-production.toml`
- [x] Verify compilation
- [x] Create documentation

## Next Steps

1. **Update CLAUDE.md** with new configuration loading approach
2. **Update README.md** with TASKER_CONFIG_PATH documentation
3. **Update deployment guides** with new environment variable
4. **Create CI/CD examples** for config file generation
5. **Add integration tests** for single-file loading path

## References

- **TAS-50 Phase 1**: Context-specific configuration structure
- **TAS-50 Phase 2**: Context-specific TOML loading
- **TAS-50 Phase 3**: Single-file runtime loading (this document)
- **CLI Implementation**: `tasker-client/src/cli/commands/config/generate.rs`
- **Merger Implementation**: `tasker-shared/src/config/merger.rs`

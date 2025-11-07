# TAS-61: Configuration Architecture Implementation Plan

**Status**: Ready to Execute
**Created**: 2025-11-03
**Estimated Total Effort**: 16-22 hours across 9 phases

## Overview

This document provides the detailed implementation plan for TAS-61 configuration architecture redesign. This is a **greenfield project** where breaking changes are acceptable and necessary for consistency.

**Key Principles**:
- ✅ Breaking changes are acceptable (greenfield project)
- ✅ Fail-fast configuration (no fallback paths)
- ✅ Domain logic validation only (not operational concerns)
- ✅ Single file loading at runtime
- ✅ Incremental validation at each phase

## Phase Structure

Each phase has:
- **Entry Criteria**: What must be true before starting
- **Tasks**: Specific work to be done
- **Validation**: How to verify success
- **Exit Criteria**: What must be true before moving to next phase
- **Estimated Effort**: Time budget for planning

---

## PHASE 1: Foundation (Validators & Enums)

**Estimated Effort**: 1-2 hours
**Risk**: Low

### Entry Criteria
- [x] Architecture document approved
- [x] validator crate added to tasker-shared Cargo.toml
- [x] Implementation plan reviewed

### Tasks

#### 1.1 Create Validators Module
```bash
# Create module structure
mkdir -p tasker-shared/src/config/validators
touch tasker-shared/src/config/validators.rs
```

**File**: `tasker-shared/src/config/validators.rs`
```rust
use validator::ValidationError;

/// Validate PostgreSQL URL format
///
/// Allows ${DATABASE_URL} template substitution or actual postgres:// URLs.
/// This is domain-specific validation for Tasker's template expansion feature.
pub fn validate_postgres_url(url: &str) -> Result<(), ValidationError> {
    // Allow template substitution (Tasker-specific feature)
    if url.contains("${DATABASE_URL}") || url.contains("$DATABASE_URL") {
        return Ok(());
    }

    // Validate actual PostgreSQL URL format
    if !url.starts_with("postgresql://") && !url.starts_with("postgres://") {
        let mut err = ValidationError::new("invalid_postgres_url");
        err.message = Some(
            "Database URL must start with postgresql:// or postgres:// or use ${DATABASE_URL}".into()
        );
        return Err(err);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_url_with_template() {
        assert!(validate_postgres_url("${DATABASE_URL}").is_ok());
        assert!(validate_postgres_url("$DATABASE_URL").is_ok());
    }

    #[test]
    fn test_postgres_url_valid() {
        assert!(validate_postgres_url("postgresql://localhost/mydb").is_ok());
        assert!(validate_postgres_url("postgres://localhost/mydb").is_ok());
    }

    #[test]
    fn test_postgres_url_invalid() {
        assert!(validate_postgres_url("mysql://localhost/mydb").is_err());
        assert!(validate_postgres_url("http://localhost").is_err());
    }
}
```

#### 1.2 Create DeploymentMode Enum
**File**: `tasker-shared/src/config/enums.rs`
```rust
use serde::{Deserialize, Serialize};

/// Deployment mode for event systems
///
/// Enum deserialization will fail if TOML contains an invalid value,
/// so no custom validator needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DeploymentMode {
    /// Pure event-driven using PostgreSQL LISTEN/NOTIFY
    EventDrivenOnly,
    /// Traditional polling-based coordination
    PollingOnly,
    /// Event-driven with polling fallback (recommended)
    Hybrid,
}

impl Default for DeploymentMode {
    fn default() -> Self {
        Self::Hybrid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_mode_deserialization() {
        let toml = r#"mode = "Hybrid""#;
        let parsed: toml::Value = toml::from_str(toml).unwrap();
        let mode: DeploymentMode = toml::from_str(&format!("mode = \"{}\"",
            parsed.get("mode").unwrap().as_str().unwrap())).unwrap();
        assert_eq!(mode, DeploymentMode::Hybrid);
    }

    #[test]
    fn test_deployment_mode_invalid() {
        let toml = r#"mode = "InvalidMode""#;
        let result: Result<DeploymentMode, _> = toml::from_str(toml);
        assert!(result.is_err());
    }
}
```

#### 1.3 Update Module Exports
**File**: `tasker-shared/src/config/mod.rs`
```rust
pub mod validators;
pub mod enums;

pub use enums::DeploymentMode;
```

### Validation
```bash
# Verify builds
cargo check --all-features

# Run validator tests
cargo test --all-features config::validators

# Run enum tests
cargo test --all-features config::enums
```

### Exit Criteria
- [ ] validators.rs created with validate_postgres_url
- [ ] enums.rs created with DeploymentMode
- [ ] All validator tests pass
- [ ] cargo check --all-features succeeds
- [ ] Zero clippy warnings in new code

---

## PHASE 2: Core Config Structs

**Estimated Effort**: 3-4 hours
**Risk**: Low

### Entry Criteria
- [x] Phase 1 complete
- [x] validator and enums modules available

### Tasks

#### 2.1 Implement CommonConfig
**File**: `tasker-shared/src/config/contexts/common.rs`

Implement as documented in architecture doc with:
- CommonConfig with #[derive(Validate)]
- DatabaseConfig with validate_postgres_url
- PoolConfig with range validation (no cross-field)
- All nested structs with appropriate validators

**Key Points**:
- Use `#[validate]` on nested struct fields
- Use `#[validate(range(min = 1))]` for counts
- Use `#[validate(custom = "crate::config::validators::validate_postgres_url")]` for DB URL

#### 2.2 Implement OrchestrationConfig
**File**: `tasker-shared/src/config/contexts/orchestration.rs`

Implement with:
- OrchestrationConfig top-level struct
- OrchestrationEventSystemsConfig (uses DeploymentMode enum)
- OrchestrationChannelsConfig
- CommandProcessorChannels (range validation for buffer sizes)
- EventListenerChannels (range validation)
- OrchestrationSystemConfig (length validation for namespaces)

**Key Points**:
- Use DeploymentMode enum (not string with validator)
- Buffer sizes: `#[validate(range(min = 10, max = 1000000))]`
- Namespaces: `#[validate(length(min = 1))]`

#### 2.3 Implement WorkerConfig
**File**: `tasker-shared/src/config/contexts/worker.rs`

Implement with:
- WorkerConfig top-level struct
- WorkerEventSystemsConfig (uses DeploymentMode)
- StepProcessingConfig
- HealthMonitoringConfig
- WorkerChannelsConfig

#### 2.4 Redesign TaskerConfig
**File**: `tasker-shared/src/config/tasker.rs`

Replace existing implementation with:
```rust
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TaskerConfig {
    #[validate]
    pub common: CommonConfig,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub orchestration: Option<OrchestrationConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub worker: Option<WorkerConfig>,
}

impl TaskerConfig {
    pub fn has_orchestration(&self) -> bool {
        self.orchestration.is_some()
    }

    pub fn has_worker(&self) -> bool {
        self.worker.is_some()
    }

    pub fn is_complete(&self) -> bool {
        self.orchestration.is_some() && self.worker.is_some()
    }
}
```

### Validation
```bash
# Incremental compilation checks
cargo check --all-features --package tasker-shared

# Run tests for each module as you complete it
cargo test --all-features --package tasker-shared config::contexts::common
cargo test --all-features --package tasker-shared config::contexts::orchestration
cargo test --all-features --package tasker-shared config::contexts::worker
cargo test --all-features --package tasker-shared config::tasker

# Clippy compliance
cargo clippy --all-features --package tasker-shared
```

### Exit Criteria
- [ ] CommonConfig implemented with all nested structs
- [ ] OrchestrationConfig implemented with all nested structs
- [ ] WorkerConfig implemented with all nested structs
- [ ] TaskerConfig redesigned with optional contexts
- [ ] All struct field validations use appropriate attributes
- [ ] Unit tests for each config struct pass
- [ ] Zero clippy warnings
- [ ] Validation focuses on domain logic (not operational concerns)

---

## PHASE 3: ConfigLoader Implementation

**Estimated Effort**: 2-3 hours
**Risk**: Low

### Entry Criteria
- [x] Phase 2 complete
- [x] All config structs have Validate derived

### Tasks

#### 3.1 Create ConfigLoader Module
**File**: `tasker-shared/src/config/loader.rs`

Implement as documented in architecture doc with:
- `ConfigLoader::load()` - Read, deserialize, validate
- `ConfigLoader::determine_config_path()` - Fail-fast, no fallback
- `ConfigLoader::load_for_orchestration()` - Validate orchestration present
- `ConfigLoader::load_for_worker()` - Validate worker present
- `ConfigLoader::load_complete()` - Validate both present

**Critical Implementation Details**:

```rust
pub fn determine_config_path() -> ConfigResult<PathBuf> {
    // REQUIRE explicit path - no fallback
    let path_str = std::env::var("TASKER_CONFIG_PATH")
        .map_err(|_| ConfigurationError::missing_environment_variable(
            "TASKER_CONFIG_PATH",
            "Configuration file path must be explicitly set. No fallback paths."
        ))?;

    let path = PathBuf::from(path_str);

    if !path.exists() {
        return Err(ConfigurationError::config_file_not_found(
            vec![path],
            "TASKER_CONFIG_PATH points to non-existent file"
        ));
    }

    Ok(path)
}
```

#### 3.2 Add Comprehensive Tests
Tests must cover:
- Successful load with valid config
- Fail-fast when TASKER_CONFIG_PATH not set
- Fail-fast when file doesn't exist
- Validation errors from validator crate
- Context-specific validation (orchestration/worker/complete)

### Validation
```bash
# Run ConfigLoader tests
cargo test --all-features --package tasker-shared config::loader

# Test fail-fast behavior specifically
cargo test --all-features test_determine_config_path_requires_env_var
cargo test --all-features test_determine_config_path_validates_file_exists

# Verify error messages are helpful
RUST_LOG=debug cargo test --all-features config::loader -- --nocapture
```

### Exit Criteria
- [ ] ConfigLoader implemented (~75 lines)
- [ ] determine_config_path() requires TASKER_CONFIG_PATH (no fallback)
- [ ] All loader methods validate config structure
- [ ] Comprehensive tests cover all paths
- [ ] Fail-fast tests pass
- [ ] Error messages are clear and actionable
- [ ] Zero clippy warnings

---

## PHASE 4: TOML Structure & ConfigMerger

**Estimated Effort**: 3-4 hours
**Risk**: Medium

### Entry Criteria
- [x] Phase 3 complete
- [x] ConfigLoader ready to load configs

### Tasks

#### 4.1 Archive Existing Config
```bash
# Archive old config structure
mkdir -p config/archive/2025-11-03-pre-tas-61-refactor
mv config/tasker/* config/archive/2025-11-03-pre-tas-61-refactor/
```

#### 4.2 Create New Base Config Files
**File**: `config/tasker/base/common.toml`
```toml
# Common configuration shared across all contexts

[system]
version = "0.1.0"
environment = "${TASKER_ENV:-development}"
max_recursion_depth = 50

[database]
url = "${DATABASE_URL}"
[database.pool]
max_connections = 20
min_connections = 5
idle_timeout_seconds = 600
max_lifetime_seconds = 1800
acquire_timeout_seconds = 30

[queues]
backend = "pgmq"
[queues.pgmq]
connection_pool_size = 10
[queues.orchestration_namespace]
task_requests = "orchestration_task_requests"
step_results = "orchestration_step_results"
task_finalization = "orchestration_task_finalization"

[circuit_breakers]
enabled = true
[circuit_breakers.global]
failure_threshold = 5
timeout_seconds = 60
half_open_max_requests = 3

[mpsc_channels]
[mpsc_channels.shared]
event_publisher_buffer_size = 5000
ruby_ffi_buffer_size = 1000
```

**File**: `config/tasker/base/orchestration.toml`
```toml
# Orchestration-specific configuration

[event_systems]
deployment_mode = "Hybrid"
[event_systems.orchestration]
queue_name = "orchestration_step_results"
enable_fallback_poller = true
poller_interval_ms = 5000
[event_systems.task_readiness]
enable_fallback_poller = true
poller_interval_ms = 3000

[decision_points]
enabled = true
max_steps_per_decision = 50
max_decision_depth = 10

[executor_pools]
[executor_pools.task_execution]
core_size = 10
max_size = 50

[mpsc_channels]
[mpsc_channels.command_processor]
command_buffer_size = 5000
[mpsc_channels.event_listeners]
orchestration_buffer_size = 10000
task_readiness_buffer_size = 5000

[system]
max_concurrent_tasks = 100
namespaces = ["default"]
enable_web_api = true
bind_address = "0.0.0.0:8080"
```

**File**: `config/tasker/base/worker.toml`
```toml
# Worker-specific configuration

[event_systems]
deployment_mode = "Hybrid"
[event_systems.worker]
enable_fallback_poller = true
poller_interval_ms = 2000

[step_processing]
max_concurrent_steps = 1000
step_execution_timeout_seconds = 300
result_submission_timeout_seconds = 30
enable_step_metrics = true

[health_monitoring]
enable_health_checks = true
health_check_interval_seconds = 30
failure_threshold = 3
recovery_check_interval_seconds = 10

[mpsc_channels]
[mpsc_channels.command_processor]
command_buffer_size = 2000
[mpsc_channels.event_subscribers]
worker_buffer_size = 5000
in_process_events_buffer_size = 1000
```

#### 4.3 Create Environment Overrides
**File**: `config/tasker/environments/test/common.toml`
```toml
# Test environment overrides - small buffers, fast timeouts

[database]
[database.pool]
max_connections = 5
min_connections = 1

[mpsc_channels]
[mpsc_channels.shared]
event_publisher_buffer_size = 100
```

**File**: `config/tasker/environments/production/common.toml`
```toml
# Production environment overrides - large buffers, monitoring

[database]
[database.pool]
max_connections = 50
min_connections = 10

[circuit_breakers]
[circuit_breakers.global]
failure_threshold = 10

[mpsc_channels]
[mpsc_channels.shared]
event_publisher_buffer_size = 50000
```

#### 4.4 Update ConfigMerger
**File**: `tasker-shared/src/config/merger.rs`

Update `merge_context()` method as documented in architecture doc to:
1. Load base/common.toml
2. Merge with environments/{env}/common.toml
3. Load base/{context}.toml
4. Merge with environments/{env}/{context}.toml
5. Create output with [common] and [{context}] top-level sections

#### 4.5 Test Config Generation
```bash
# Generate configs for all contexts and environments
cargo run --bin tasker-cli config generate \
  --context orchestration --environment test \
  --output generated/orchestration-test.toml

cargo run --bin tasker-cli config generate \
  --context worker --environment test \
  --output generated/worker-test.toml

cargo run --bin tasker-cli config generate \
  --context complete --environment test \
  --output generated/complete-test.toml

# Validate generated configs
cargo run --bin tasker-cli config dump \
  --file generated/orchestration-test.toml \
  --format toml

# Test loading with ConfigLoader
TASKER_CONFIG_PATH=generated/orchestration-test.toml \
  cargo test --all-features test_load_orchestration_config
```

### Validation
```bash
# Verify all base configs are valid TOML
for f in config/tasker/base/*.toml; do
  echo "Validating $f"
  toml-cli get -f "$f" .
done

# Verify generated configs can be loaded
for ctx in orchestration worker complete; do
  for env in test development production; do
    cargo run --bin tasker-cli config generate \
      --context $ctx --environment $env \
      --output generated/$ctx-$env.toml

    # Verify structure
    echo "Checking generated/$ctx-$env.toml has [common] section"
    grep -q '^\[common\]' generated/$ctx-$env.toml || echo "ERROR: Missing [common]"
  done
done
```

### Exit Criteria
- [ ] Old config archived to config/archive/
- [ ] New base TOML files created (common, orchestration, worker)
- [ ] Environment overrides created for test/dev/prod
- [ ] ConfigMerger updated to merge with new structure
- [ ] All generated configs have correct structure ([common], [{context}])
- [ ] ConfigLoader can successfully load all generated configs
- [ ] TOML structure mirrors Rust struct hierarchy exactly

---

## PHASE 5: Breaking Changes Tier 1 - SystemContext

**Estimated Effort**: 1-2 hours
**Risk**: Low (no signature changes)

### Entry Criteria
- [x] Phases 1-4 complete
- [x] ConfigLoader working with new config structure

### Tasks

#### 5.1 Update SystemContext Field Access
**File**: `tasker-shared/src/system_context.rs`

Update all field access patterns:
- `config.database` → `config.common.database`
- `config.circuit_breakers` → `config.common.circuit_breakers`
- `config.queues` → `config.common.queues`
- `config.mpsc_channels` → `config.common.mpsc_channels`

**Pattern**:
```rust
// OLD
let pool_config = &config.database.pool;

// NEW
let pool_config = &config.common.database.pool;
```

#### 5.2 Update Related Files
Update field access in all files that use SystemContext:
- `tasker-orchestration/src/actors/traits.rs`
- `tasker-orchestration/src/orchestration/bootstrap.rs`
- `tasker-worker/src/bootstrap.rs`
- All test files using SystemContext

### Validation
```bash
# Run SystemContext tests
cargo test --all-features --package tasker-shared system_context

# Run integration tests that use SystemContext
cargo test --all-features system_context

# Verify no regressions
cargo test --all-features --package tasker-orchestration
cargo test --all-features --package tasker-worker
```

### Exit Criteria
- [ ] All config field accesses updated to use config.common.*
- [ ] SystemContext::new() signature unchanged (still takes Arc<TaskerConfig>)
- [ ] All SystemContext tests pass
- [ ] No compilation errors in dependent crates
- [ ] Zero clippy warnings

---

## PHASE 6: Breaking Changes Tier 2 - Bootstrap

**Estimated Effort**: 2-3 hours
**Risk**: Low (well-defined interfaces)

### Entry Criteria
- [x] Phase 5 complete
- [x] SystemContext using config.common.*

### Tasks

#### 6.1 Update Orchestration Bootstrap
**File**: `tasker-orchestration/src/orchestration/bootstrap.rs`

Replace:
```rust
// OLD
let config = UnifiedConfigLoader::new_from_env()?;

// NEW
let config_path = ConfigLoader::determine_config_path()?;
let config = ConfigLoader::load_for_orchestration(&config_path)?;
```

Update all field accesses to use:
- `config.common.*` for shared config
- `config.orchestration.as_ref()?.field` for orchestration config

#### 6.2 Update Worker Bootstrap
**File**: `tasker-worker/src/bootstrap.rs`

Same pattern as orchestration:
```rust
let config_path = ConfigLoader::determine_config_path()?;
let config = ConfigLoader::load_for_worker(&config_path)?;
```

#### 6.3 Update Ruby FFI Bootstrap
**File**: `workers/ruby/ext/tasker_core/src/bootstrap.rs`

Update to use new ConfigLoader and field access patterns.

### Validation
```bash
# Test with TASKER_CONFIG_PATH set
export TASKER_CONFIG_PATH=generated/orchestration-test.toml
cargo test --all-features --package tasker-orchestration bootstrap

export TASKER_CONFIG_PATH=generated/worker-test.toml
cargo test --all-features --package tasker-worker bootstrap

# Test fail-fast behavior
unset TASKER_CONFIG_PATH
cargo test --all-features bootstrap -- --should-panic
```

### Exit Criteria
- [ ] UnifiedConfigLoader removed from orchestration bootstrap
- [ ] UnifiedConfigLoader removed from worker bootstrap
- [ ] UnifiedConfigLoader removed from Ruby FFI bootstrap
- [ ] All bootstrap code uses ConfigLoader::determine_config_path()
- [ ] All bootstrap tests pass with TASKER_CONFIG_PATH set
- [ ] Fail-fast behavior verified when TASKER_CONFIG_PATH missing

---

## PHASE 7: Breaking Changes Tier 3 - Access Patterns

**Estimated Effort**: 3-4 hours
**Risk**: Medium (many files, but mechanical)

### Entry Criteria
- [x] Phase 6 complete
- [x] Bootstrap using ConfigLoader

### Tasks

#### 7.1 Create Field Access Update Script
**File**: `scripts/update-config-access.sh`
```bash
#!/usr/bin/env bash
# Update config field access patterns across codebase

set -euo pipefail

# Find all Rust files with config field access
files=$(rg --files-with-matches 'config\.(database|circuit_breakers|queues|mpsc_channels)' \
  --glob '*.rs' \
  --glob '!target' \
  --glob '!config/archive')

for file in $files; do
  echo "Updating $file"

  # Update common config access
  sed -i '' 's/config\.database/config.common.database/g' "$file"
  sed -i '' 's/config\.circuit_breakers/config.common.circuit_breakers/g' "$file"
  sed -i '' 's/config\.queues/config.common.queues/g' "$file"
  sed -i '' 's/config\.mpsc_channels\.shared/config.common.mpsc_channels.shared/g' "$file"

  # Update orchestration config access
  sed -i '' 's/config\.orchestration\./config.orchestration.as_ref()?./' "$file" || true

  # Update worker config access
  sed -i '' 's/config\.worker\./config.worker.as_ref()?./' "$file" || true
done
```

#### 7.2 Manual Review Strategy
After script runs, manually review critical files:
- All files in `tasker-orchestration/src/orchestration/`
- All files in `tasker-worker/src/worker/`
- All files in `tasker-shared/src/`
- All actor implementations

#### 7.3 Incremental Testing
After updating each subsystem, run its tests:
```bash
# Update orchestration, test immediately
cargo test --all-features --package tasker-orchestration

# Update worker, test immediately
cargo test --all-features --package tasker-worker

# Update shared, test immediately
cargo test --all-features --package tasker-shared
```

### Validation
```bash
# Full test suite
cargo test --all-features

# Verify no unwrap() on Option access
rg 'config\.(orchestration|worker)\.unwrap\(\)' --glob '*.rs' --glob '!target'
# Should return zero results (all should use as_ref()? or expect())

# Clippy check
cargo clippy --all-features --all-targets
```

### Exit Criteria
- [ ] All orchestration config access uses config.common.* or config.orchestration.as_ref()?
- [ ] All worker config access uses config.common.* or config.worker.as_ref()?
- [ ] All shared config access uses config.common.*
- [ ] No direct unwrap() on optional context fields
- [ ] All package tests pass
- [ ] Zero clippy warnings

---

## PHASE 8: Breaking Changes Tier 4 - Tests

**Estimated Effort**: 2-3 hours
**Risk**: Low (tests are flexible)

### Entry Criteria
- [x] Phase 7 complete
- [x] All production code updated

### Tasks

#### 8.1 Update Integration Test Infrastructure
**File**: `tasker-shared/tests/support/mod.rs` (or similar)

Create helper for test config generation:
```rust
pub fn create_test_config_complete() -> TaskerConfig {
    let common = CommonConfig { /* ... */ };
    let orchestration = OrchestrationConfig { /* ... */ };
    let worker = WorkerConfig { /* ... */ };

    TaskerConfig {
        common,
        orchestration: Some(orchestration),
        worker: Some(worker),
    }
}
```

#### 8.2 Update Integration Tests
Update tests to use:
- `ConfigLoader::load_complete()` for full integration tests
- Test config helpers for unit tests
- Generated test TOML files

Files to update:
- `tasker-orchestration/tests/`
- `tasker-worker/tests/`
- `tasker-shared/tests/`
- `tests/` (workspace integration tests)

#### 8.3 Regenerate Test Fixtures
```bash
# Generate test configs
cargo run --bin tasker-cli config generate \
  --context complete --environment test \
  --output tests/fixtures/complete-test.toml

# Update all test files to reference new fixtures
```

### Validation
```bash
# Run all integration tests
cargo test --all-features --test '*'

# Run all tests including doctests
cargo test --all-features --doc

# Run tests with TASKER_CONFIG_PATH set
export TASKER_CONFIG_PATH=tests/fixtures/complete-test.toml
cargo test --all-features
```

### Exit Criteria
- [ ] Integration tests use ConfigLoader::load_complete()
- [ ] Unit tests use test config helpers
- [ ] All test fixtures regenerated with new structure
- [ ] All integration tests pass
- [ ] All unit tests pass
- [ ] All doctests pass

---

## PHASE 9: Final Validation & Documentation

**Estimated Effort**: 2-3 hours
**Risk**: Low

### Entry Criteria
- [x] Phases 1-8 complete
- [x] All code changes implemented

### Tasks

#### 9.1 Comprehensive Testing
```bash
# Full test suite
cargo test --all-features

# Clippy all targets
cargo clippy --all-features --all-targets

# Check formatting
cargo fmt --all -- --check

# Run benchmarks to ensure no regressions
cargo bench --all-features

# Test config generation for all combinations
for ctx in orchestration worker complete; do
  for env in test development production; do
    cargo run --bin tasker-cli config generate \
      --context $ctx --environment $env \
      --output /tmp/test-$ctx-$env.toml
    echo "Generated $ctx-$env config"
  done
done
```

#### 9.2 Performance Validation
```bash
# Measure config loading time
cargo run --release --bin config-perf-test
# Target: <10ms including validation
```

**File**: `tasker-shared/benches/config_loading.rs`
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn config_loading_benchmark(c: &mut Criterion) {
    std::env::set_var("TASKER_CONFIG_PATH", "generated/orchestration-test.toml");

    c.bench_function("load_orchestration_config", |b| {
        b.iter(|| {
            let path = ConfigLoader::determine_config_path().unwrap();
            let config = ConfigLoader::load_for_orchestration(&path).unwrap();
            black_box(config);
        });
    });
}

criterion_group!(benches, config_loading_benchmark);
criterion_main!(benches);
```

#### 9.3 Validation Error Quality
Create test cases that trigger validation errors and verify messages are helpful:
```bash
# Test with invalid postgres URL
echo '[common]
[common.database]
url = "mysql://localhost/test"' > /tmp/invalid.toml

TASKER_CONFIG_PATH=/tmp/invalid.toml cargo run --bin tasker-server 2>&1 | \
  grep "Database URL must start with postgresql://"

# Test with missing TASKER_CONFIG_PATH
unset TASKER_CONFIG_PATH
cargo run --bin tasker-server 2>&1 | \
  grep "TASKER_CONFIG_PATH"
```

#### 9.4 Update Documentation
**File**: `CLAUDE.md`
```markdown
### Configuration Management

**TAS-61 Configuration Architecture** (Implemented 2025-11-03)

The system uses a context-specific configuration architecture with fail-fast loading:

```bash
# REQUIRED: Set config path explicitly
export TASKER_CONFIG_PATH=/app/config/orchestration-production.toml

# Generate config from base + environment
cargo run --bin tasker-cli config generate \
  --context orchestration \
  --environment production \
  --output /app/config/orchestration-production.toml
```

**Key Features**:
- ✅ Single file loading (no runtime merging)
- ✅ Fail-fast validation (no fallback paths)
- ✅ Domain logic validation (via `validator` crate)
- ✅ Optional contexts (orchestration/worker/complete)

**Configuration Files**:
- Base: `config/tasker/base/{common,orchestration,worker}.toml`
- Overrides: `config/tasker/environments/{env}/{context}.toml`
- Generated: `generated/{context}-{environment}.toml`
```

**File**: `README.md`
Update with:
- TASKER_CONFIG_PATH requirement
- Config generation workflow
- Validator integration mention

#### 9.5 Create Migration Guide
**File**: `docs/ticket-specs/TAS-61/MIGRATION_GUIDE.md`
```markdown
# TAS-61 Configuration Migration Guide

## Breaking Changes

### 1. Configuration Loading

**Before**:
```rust
let config = UnifiedConfigLoader::new_from_env()?;
let db_url = config.database_url();
```

**After**:
```rust
let config_path = ConfigLoader::determine_config_path()?;
let config = ConfigLoader::load_for_orchestration(&config_path)?;
let db_url = config.common.database_url();
```

### 2. Field Access Patterns

**Before**:
```rust
config.database.pool.max_connections
config.circuit_breakers.enabled
```

**After**:
```rust
config.common.database.pool.max_connections
config.common.circuit_breakers.enabled
```

### 3. Environment Variables

**REQUIRED**:
```bash
export TASKER_CONFIG_PATH=/app/config/orchestration-production.toml
```

No fallback paths. Service will abort if not set.

## Migration Checklist

- [ ] Generate new config files for your environments
- [ ] Set TASKER_CONFIG_PATH in deployment scripts
- [ ] Update any custom config loading code
- [ ] Update field access patterns in custom code
- [ ] Test with new config structure
```

#### 9.6 Tag Architecture Document
**File**: `docs/ticket-specs/TAS-61/new-config-architecture.md`

Update header:
```markdown
**Document Status**: ✅ IMPLEMENTED (2025-11-03)
**Implementation Plan**: [implementation-plan.md](./implementation-plan.md)
**Migration Guide**: [MIGRATION_GUIDE.md](./MIGRATION_GUIDE.md)
```

### Validation
```bash
# Final smoke test
export TASKER_CONFIG_PATH=generated/orchestration-production.toml
cargo run --bin tasker-server --dry-run

export TASKER_CONFIG_PATH=generated/worker-production.toml
cargo run --bin tasker-worker --dry-run

# All tests pass
cargo test --all-features

# Zero clippy warnings
cargo clippy --all-features --all-targets

# Performance target met
cargo bench config_loading
# Verify: <10ms per config load
```

### Exit Criteria
- [ ] All 377+ tests pass
- [ ] Zero clippy warnings
- [ ] Config loading < 10ms (including validation)
- [ ] Validation error messages are clear and helpful
- [ ] CLAUDE.md updated with new config strategy
- [ ] README updated with TASKER_CONFIG_PATH requirement
- [ ] Migration guide created
- [ ] Architecture document tagged as IMPLEMENTED
- [ ] Performance benchmarks pass
- [ ] All generated configs validate successfully

---

## Success Metrics

### Functional
- ✅ All config structs use validator crate
- ✅ Single custom validator (validate_postgres_url)
- ✅ ConfigLoader requires TASKER_CONFIG_PATH (fail-fast)
- ✅ Generated configs load successfully
- ✅ All 377+ tests pass

### Quality
- ✅ Zero clippy warnings
- ✅ Config loading <10ms
- ✅ Clear validation error messages
- ✅ Domain logic validation only

### Documentation
- ✅ Architecture document tagged IMPLEMENTED
- ✅ Migration guide complete
- ✅ CLAUDE.md updated
- ✅ README updated

---

## Rollback Plan

If critical issues found:

1. **Immediate Rollback**:
   ```bash
   # Restore archived config
   rm -rf config/tasker
   cp -r config/archive/2025-11-03-pre-tas-61-refactor config/tasker

   # Revert git branch
   git reset --hard <commit-before-tas-61>
   ```

2. **Partial Rollback** (if only tests broken):
   - Keep new structs
   - Restore old loader temporarily
   - Fix tests incrementally

3. **Issues Log**: Document any rollback in `docs/ticket-specs/TAS-61/ROLLBACK_LOG.md`

---

## Post-Implementation

### Monitoring
- Watch config loading times in production logs
- Monitor validation error rates
- Track TASKER_CONFIG_PATH missing errors

### Future Enhancements
- [ ] Config hot-reload support (TAS-62)
- [ ] Config schema documentation generation (TAS-63)
- [ ] Web UI for config visualization (TAS-64)

---

**Document Status**: Ready to Execute
**Next Step**: Begin Phase 1 - Foundation

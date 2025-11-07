# TAS-61 V2 Config Architecture Migration - Implementation Plan

**Date**: 2025-01-06
**Status**: Ready for execution
**Branch**: `jcoletaylor/tas-61-configuration-architecture-redesign-system-orchestration`

---

## Table of Contents

1. [Overview & Strategy](#overview--strategy)
2. [Phase 1: Foundation & File Structure](#phase-1-foundation--file-structure-steps-1-3)
3. [Phase 2: Bridge & Loading Infrastructure](#phase-2-bridge--loading-infrastructure-steps-4-5)
4. [Phase 3: Bootstrapping Migration](#phase-3-bootstrapping-migration-steps-6-7)
5. [Phase 4: Environment Configuration](#phase-4-environment-configuration-step-8)
6. [Phase 5: Validation & Iteration](#phase-5-validation--iteration-step-9)
7. [Risk Mitigation](#risk-mitigation)
8. [Success Criteria](#success-criteria)
9. [Timeline](#timeline)
10. [Post-Migration Next Steps](#post-migration-next-steps)

---

## Overview & Strategy

### Goals

Migrate from legacy flat configuration to v2 context-based architecture with:
- **Common**: Shared config across all contexts (database, queues, circuit breakers, etc.)
- **Orchestration**: Orchestration-specific config (event systems, decision points, web API)
- **Worker**: Worker-specific config (step processing, health monitoring, FFI integration)

### Migration Strategy

**Hard-switch with temporary bridge**:
1. Load v2 TOML files (context-based structure)
2. Deserialize to `TaskerConfigV2` structs
3. Validate using `validator` crate
4. Convert via `.into()` to legacy `TaskerConfig` (temporary bridge)
5. Use existing `SystemContext` and runtime logic unchanged
6. Remove bridge in Phase 6-8 (post-migration)

### User Strategic Decisions

Based on clarifying questions answered 2025-01-06:

1. **Rollback Approach**: Hard-switch to v2 with bridge (chosen over dual-loading feature flag)
2. **CLI Output**: Add `--version` flag to choose v1/v2 output format (chosen over v2-only)
3. **Test Handling**: Keep legacy `complete-test.toml` working, add v2 in parallel (chosen over immediate switch)
4. **Bridge Permanence**: Temporary - remove once all code migrated to v2 (Phase 6-8)

### Key Architectural Improvements

**Protected Routes Ergonomics** (Implemented 2025-01-06):
- **Problem**: HashMap syntax with route patterns as quoted strings in TOML section names was "difficult to use / maintain over time"
- **Solution**: Array-of-tables syntax with separate `method` and `path` fields
- **Implementation**: `Vec<ProtectedRouteConfig>` in TOML, `routes_map()` helper converts to `HashMap` at load time for O(1) lookups
- **User Feedback**: "excellent - this is the closest we've gotten so far, excellent work"

---

## Phase 1: Foundation & File Structure (Steps 1-3)

### 1.1. Create V2 Directory Structure

Create mirrored directory structure for v2 configs:

```bash
# Create v2 config directory structure
mkdir -p config/tasker/v2/base
mkdir -p config/tasker/v2/environments/test
mkdir -p config/tasker/v2/environments/development
mkdir -p config/tasker/v2/environments/production
```

**Success Criteria**: Directory structure exists and mirrors legacy `config/tasker/`

### 1.2. Copy Legacy Configs to V2

Copy entire legacy structure as transformation baseline:

```bash
# Copy base configs
cp -a config/tasker/base/*.toml config/tasker/v2/base/

# Copy environment-specific configs
cp -a config/tasker/environments/test/*.toml config/tasker/v2/environments/test/
cp -a config/tasker/environments/development/*.toml config/tasker/v2/environments/development/
cp -a config/tasker/environments/production/*.toml config/tasker/v2/environments/production/
```

**Expected File Count**: 12 files total
- 3 base files: `base/common.toml`, `base/orchestration.toml`, `base/worker.toml`
- 9 environment files: 3 contexts × 3 environments

**Success Criteria**: All legacy config files copied to v2 directory

### 1.3. Transform V2 Config Files (File-by-File)

Transform each TOML file from legacy flat structure to v2 context-based structure. **Preserve all values** - only restructure syntax.

#### Transform `common.toml` (Base + All Environments)

**Critical**: All shared configuration must be wrapped under `[common]` section.

**Files to Transform**:
- `config/tasker/v2/base/common.toml`
- `config/tasker/v2/environments/test/common.toml`
- `config/tasker/v2/environments/development/common.toml`
- `config/tasker/v2/environments/production/common.toml`

**Transformation Rules**:

```toml
# OLD (legacy flat structure):
[system]
version = "0.1.0"
default_dependent_system = "default"

[database]
url = "${DATABASE_URL:-postgresql://localhost/tasker}"
database = "tasker_development"

[database.pool]
max_connections = 20

[queues]
backend = "pgmq"

[mpsc_channels.shared]
event_queue_buffer_size = 5000

# NEW (v2 context-based structure):
[common.system]
version = "0.1.0"
default_dependent_system = "default"

[common.database]
url = "${DATABASE_URL:-postgresql://localhost/tasker}"
database = "tasker_development"

[common.database.pool]
max_connections = 20

[common.queues]
backend = "pgmq"

[common.mpsc_channels.event_publisher]
event_queue_buffer_size = 5000

[common.mpsc_channels.ffi]
ruby_event_buffer_size = 1000
```

**MPSC Channels Remapping**:
- `[mpsc_channels.shared]` → `[common.mpsc_channels.event_publisher]` + `[common.mpsc_channels.ffi]`
- Split shared channels into semantic categories per v2 proposal
- Preserve all buffer size values exactly

**Environment Variable Substitutions**:
- Preserve all `${VAR:-default}` patterns unchanged
- Do NOT expand environment variables - keep as templates

**Success Criteria**:
- All common config under `[common]` section
- All values preserved exactly as in legacy
- Environment variable templates unchanged

#### Transform `orchestration.toml` (Base + All Environments)

**Critical**: All orchestration-specific config must be wrapped under `[orchestration]` section.

**Files to Transform**:
- `config/tasker/v2/base/orchestration.toml`
- `config/tasker/v2/environments/test/orchestration.toml`
- `config/tasker/v2/environments/development/orchestration.toml`
- `config/tasker/v2/environments/production/orchestration.toml`

**Transformation Rules**:

```toml
# OLD (legacy flat structure):
[orchestration]
mode = "standalone"

[orchestration_events]
system_id = "orchestration-event-system"
deployment_mode = "Hybrid"

[orchestration_events.timing]
health_check_interval_seconds = 30

[task_readiness_events]
system_id = "task-readiness-event-system"

[mpsc_channels.orchestration]
command_buffer_size = 5000

[web.auth.protected_routes."POST /v1/tasks"]
auth_type = "bearer"
required = false

# NEW (v2 context-based structure):
[orchestration]
mode = "standalone"

[orchestration.event_systems.orchestration]
system_id = "orchestration-event-system"
deployment_mode = "Hybrid"

[orchestration.event_systems.orchestration.timing]
health_check_interval_seconds = 30

[orchestration.event_systems.task_readiness]
system_id = "task-readiness-event-system"

[orchestration.mpsc_channels.command_processor]
command_buffer_size = 5000

# CRITICAL: Protected routes transformation (array-of-tables syntax)
[[orchestration.web.auth.protected_routes]]
method = "POST"
path = "/v1/tasks"
auth_type = "bearer"
required = false

[[orchestration.web.auth.protected_routes]]
method = "DELETE"
path = "/v1/tasks/{task_uuid}"
auth_type = "bearer"
required = true

[[orchestration.web.auth.protected_routes]]
method = "PATCH"
path = "/v1/tasks/{task_uuid}/workflow_steps/{step_uuid}"
auth_type = "bearer"
required = true

[[orchestration.web.auth.protected_routes]]
method = "GET"
path = "/v1/analytics/bottlenecks"
auth_type = "bearer"
required = false

[[orchestration.web.auth.protected_routes]]
method = "GET"
path = "/v1/analytics/performance"
auth_type = "bearer"
required = false
```

**Event Systems Remapping**:
- `[orchestration_events]` → `[orchestration.event_systems.orchestration]`
- `[task_readiness_events]` → `[orchestration.event_systems.task_readiness]`
- Preserve all nested timing, processing, health, backoff fields

**MPSC Channels Remapping**:
- `[mpsc_channels.orchestration]` → `[orchestration.mpsc_channels.command_processor]` + `[orchestration.mpsc_channels.event_systems]` + `[orchestration.mpsc_channels.event_listeners]`
- Split into semantic categories per v2 proposal

**Protected Routes Transformation** (CRITICAL):
- **OLD**: HashMap syntax with route pattern as quoted section name
  ```toml
  [web.auth.protected_routes."METHOD /path"]
  auth_type = "bearer"
  required = true
  ```
- **NEW**: Array-of-tables with separate method/path fields
  ```toml
  [[orchestration.web.auth.protected_routes]]
  method = "METHOD"
  path = "/path"
  auth_type = "bearer"
  required = true
  ```
- Transformation: Split route pattern "METHOD /path" into separate `method` and `path` fields
- Reference: See `docs/ticket-specs/TAS-61/config-protected-routes-syntax.md` for rationale

**Success Criteria**:
- All orchestration config under `[orchestration]` section
- Event systems properly nested
- Protected routes use array-of-tables syntax
- All values preserved

#### Transform `worker.toml` (Base + All Environments)

**Critical**: All worker-specific config must be wrapped under `[worker]` section.

**Files to Transform**:
- `config/tasker/v2/base/worker.toml`
- `config/tasker/v2/environments/test/worker.toml`
- `config/tasker/v2/environments/development/worker.toml`
- `config/tasker/v2/environments/production/worker.toml`

**Transformation Rules**:

```toml
# OLD (legacy flat structure):
[worker]
worker_id = "worker-001"
worker_type = "general"

[worker_events]
system_id = "worker-event-system"
deployment_mode = "Hybrid"

[mpsc_channels.worker]
command_buffer_size = 2000

[web.auth.protected_routes."GET /status/detailed"]
auth_type = "bearer"
required = false

# NEW (v2 context-based structure):
[worker]
worker_id = "worker-001"
worker_type = "general"

[worker.event_systems.worker]
system_id = "worker-event-system"
deployment_mode = "Hybrid"

[worker.mpsc_channels.command_processor]
command_buffer_size = 2000

# CRITICAL: Protected routes transformation (array-of-tables syntax)
[[worker.web.auth.protected_routes]]
method = "GET"
path = "/status/detailed"
auth_type = "bearer"
required = false

[[worker.web.auth.protected_routes]]
method = "DELETE"
path = "/templates/cache"
auth_type = "bearer"
required = true

[[worker.web.auth.protected_routes]]
method = "GET"
path = "/handlers"
auth_type = "bearer"
required = false

[[worker.web.auth.protected_routes]]
method = "POST"
path = "/templates/cache/maintain"
auth_type = "bearer"
required = true

[[worker.web.auth.protected_routes]]
method = "POST"
path = "/templates/{namespace}/{name}/{version}/refresh"
auth_type = "bearer"
required = true
```

**Event Systems Remapping**:
- `[worker_events]` → `[worker.event_systems.worker]`
- Preserve all nested metadata, timing, processing, health fields

**MPSC Channels Remapping**:
- `[mpsc_channels.worker]` → `[worker.mpsc_channels.command_processor]` + `[worker.mpsc_channels.event_systems]` + `[worker.mpsc_channels.event_subscribers]` + `[worker.mpsc_channels.in_process_events]` + `[worker.mpsc_channels.event_listeners]`
- Split into semantic categories per v2 proposal

**Protected Routes Transformation**: Same rules as orchestration (array-of-tables syntax)

**Success Criteria**:
- All worker config under `[worker]` section
- Event systems properly nested
- Protected routes use array-of-tables syntax
- All values preserved

### 1.4. Rename Proposal to Baseline

**Rationale**: The "proposal" suffix was appropriate during design phase. Now that we're implementing, rename to official v2 module.

```bash
# Rename the file
cd tasker-shared/src/config/tasker/
mv tasker_v2_proposal.rs tasker_v2.rs
```

**Update `mod.rs`**:

```rust
// tasker-shared/src/config/tasker/mod.rs

// Legacy config (will be deprecated in Phase 6-8)
pub mod tasker;

// V2 config (new baseline)
pub mod tasker_v2;

// Re-export both for compatibility during migration
pub use tasker::TaskerConfig;           // Legacy
pub use tasker_v2::TaskerConfig as TaskerConfigV2;  // V2 (alias for clarity)
```

**Update Imports Across Workspace**:

```bash
# Search for all imports of the old module
grep -r "tasker_v2_proposal" --include="*.rs" .

# Expected locations:
# - tasker-shared/src/config/tasker/tasker_v2_proposal.rs (the file itself - deleted after rename)
# - Any test files that import it

# Update manually or with sed:
find . -name "*.rs" -type f -exec sed -i '' 's/tasker_v2_proposal/tasker_v2/g' {} +
```

**Success Criteria**:
- `tasker_v2_proposal.rs` renamed to `tasker_v2.rs`
- `mod.rs` exports both `TaskerConfig` (legacy) and `TaskerConfigV2`
- All imports updated
- `cargo check --all-features` passes with no module errors

---

## Phase 2: Bridge & Loading Infrastructure (Steps 4-5)

### 2.1. Add `--version` Flag to CLI Tools

**Rationale**: Support both legacy and v2 config generation during migration. This enables gradual rollout and testing of v2 configs.

**Modify**: `tasker-shared/src/config/merger.rs`

**Add ConfigVersion Enum**:

```rust
// tasker-shared/src/config/merger.rs

/// Configuration version for CLI generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigVersion {
    /// Legacy flat configuration (default for backward compatibility)
    V1,
    /// V2 context-based configuration
    V2,
}

impl Default for ConfigVersion {
    fn default() -> Self {
        Self::V1  // Default to v1 for backward compatibility
    }
}

impl std::str::FromStr for ConfigVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "v1" | "1" => Ok(Self::V1),
            "v2" | "2" => Ok(Self::V2),
            _ => Err(format!("Invalid config version: '{}'. Expected 'v1' or 'v2'", s)),
        }
    }
}
```

**Update ConfigMerger Struct**:

```rust
pub struct ConfigMerger {
    loader: UnifiedConfigLoader,
    source_dir: PathBuf,
    environment: String,
    version: ConfigVersion,  // NEW FIELD
}

impl ConfigMerger {
    /// Create a new configuration merger
    ///
    /// # Arguments
    /// * `source_dir` - Path to config directory (e.g., "config/tasker")
    /// * `environment` - Target environment (test, development, production)
    /// * `version` - Config version (v1 or v2)
    pub fn new(
        source_dir: PathBuf,
        environment: &str,
        version: ConfigVersion,
    ) -> ConfigResult<Self> {
        // Adjust source_dir based on version
        let versioned_dir = match version {
            ConfigVersion::V1 => source_dir,
            ConfigVersion::V2 => source_dir.join("v2"),
        };

        // Validate source directory exists
        if !versioned_dir.exists() {
            return Err(ConfigurationError::config_file_not_found(vec![
                versioned_dir.clone()
            ]));
        }

        // Create unified loader
        let loader = UnifiedConfigLoader::for_generation(
            versioned_dir.clone(),
            environment
        )?;

        Ok(Self {
            loader,
            source_dir: versioned_dir,
            environment: environment.to_string(),
            version,
        })
    }

    /// Get the config version
    pub fn version(&self) -> ConfigVersion {
        self.version
    }
}
```

**CLI Integration** (if CLI binary exists - check for `bin/` directory):

```rust
// Example CLI integration (location TBD)
use clap::Parser;

#[derive(Parser)]
struct Cli {
    /// Config version to generate (v1 or v2)
    #[arg(long, default_value = "v1")]
    version: ConfigVersion,

    /// Environment name
    #[arg(short, long)]
    environment: String,

    /// Context to generate (common, orchestration, worker)
    #[arg(short, long)]
    context: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut merger = ConfigMerger::new(
        PathBuf::from("config/tasker"),
        &cli.environment,
        cli.version,
    )?;

    let merged_config = merger.merge_context(&cli.context)?;
    println!("{}", merged_config);

    Ok(())
}
```

**Success Criteria**:
- `ConfigVersion` enum added
- `ConfigMerger` accepts version parameter
- Source directory correctly adjusted for v1 vs v2
- CLI (if exists) accepts `--version` flag

### 2.2. Implement `From<TaskerConfigV2>` for `TaskerConfig` Bridge

**Rationale**: Temporary bridge allows v2 configs to be loaded while runtime code still uses legacy `TaskerConfig`. This is a non-breaking migration strategy.

**Create**: `tasker-shared/src/config/tasker/bridge.rs`

**Core Bridge Implementation**:

```rust
// tasker-shared/src/config/tasker/bridge.rs

use super::tasker::{TaskerConfig, OrchestrationConfig, WorkerConfig, MpscChannelsConfig};
use super::tasker_v2::{TaskerConfigV2, OrchestrationConfig as OrchV2, WorkerConfig as WorkerV2};
use std::collections::HashMap;

/// Bridge conversion from v2 to legacy config
///
/// This is a temporary implementation to enable v2 config loading while
/// runtime code still uses legacy TaskerConfig structure. Will be removed
/// in Phase 6-8 when codebase migrates to v2 directly.
impl From<TaskerConfigV2> for TaskerConfig {
    fn from(v2: TaskerConfigV2) -> Self {
        TaskerConfig {
            // === COMMON FIELDS (flatten from v2.common) ===
            system: v2.common.system,
            database: v2.common.database,
            queues: v2.common.queues,
            circuit_breakers: v2.common.circuit_breakers,
            execution: v2.common.execution,
            backoff: v2.common.backoff,
            task_templates: v2.common.task_templates,
            telemetry: v2.common.telemetry,

            // === ORCHESTRATION CONFIG ===
            orchestration: v2.orchestration.map(|o| convert_orchestration_config(o)),

            // === WORKER CONFIG ===
            worker: v2.worker.map(|w| convert_worker_config(w)),

            // === MPSC CHANNELS (reconstruct from common + orchestration + worker) ===
            mpsc_channels: reconstruct_mpsc_channels(&v2),
        }
    }
}

/// Convert v2 orchestration config to legacy structure
fn convert_orchestration_config(v2: OrchV2) -> OrchestrationConfig {
    OrchestrationConfig {
        mode: v2.mode,
        enable_performance_logging: v2.enable_performance_logging,

        // Flatten event systems (v2.event_systems.orchestration → legacy.orchestration_events)
        orchestration_events: convert_event_system(v2.event_systems.orchestration),
        task_readiness_events: convert_event_system(v2.event_systems.task_readiness),

        // Decision points (unchanged structure)
        decision_points: v2.decision_points,

        // MPSC channels (flattened)
        mpsc_channels: flatten_orchestration_mpsc(v2.mpsc_channels),

        // Web config with protected routes conversion
        web: v2.web.map(|w| convert_web_config(w)),
    }
}

/// Convert v2 event system config to legacy structure
fn convert_event_system(v2: EventSystemConfig) -> LegacyEventSystemConfig {
    LegacyEventSystemConfig {
        system_id: v2.system_id,
        deployment_mode: v2.deployment_mode,

        // Flatten timing (v2.timing.* → legacy.*)
        health_check_interval_seconds: v2.timing.health_check_interval_seconds,
        fallback_polling_interval_seconds: v2.timing.fallback_polling_interval_seconds,
        visibility_timeout_seconds: v2.timing.visibility_timeout_seconds,
        processing_timeout_seconds: v2.timing.processing_timeout_seconds,
        claim_timeout_seconds: v2.timing.claim_timeout_seconds,

        // Flatten processing
        max_concurrent_operations: v2.processing.max_concurrent_operations,
        batch_size: v2.processing.batch_size,
        max_retries: v2.processing.max_retries,

        // Backoff (nested under processing in v2)
        backoff: convert_event_system_backoff(v2.processing.backoff),

        // Health
        health: v2.health,
    }
}

/// Convert v2 event system backoff to legacy structure
fn convert_event_system_backoff(v2: EventSystemBackoffConfig) -> LegacyBackoffConfig {
    LegacyBackoffConfig {
        initial_delay_ms: v2.initial_delay_ms,
        max_delay_ms: v2.max_delay_ms,
        multiplier: v2.multiplier,
        jitter_percent: v2.jitter_percent,
    }
}

/// Convert v2 web config to legacy structure (INCLUDING protected routes transformation)
fn convert_web_config(v2: OrchestrationWebConfig) -> LegacyWebConfig {
    LegacyWebConfig {
        enabled: v2.enabled,
        bind_address: v2.bind_address,
        request_timeout_ms: v2.request_timeout_ms,
        tls: v2.tls,
        database_pools: v2.database_pools,
        cors: v2.cors,

        // CRITICAL: Convert protected routes from Vec to HashMap
        auth: v2.auth.map(|a| LegacyAuthConfig {
            enabled: a.enabled,
            jwt_issuer: a.jwt_issuer,
            jwt_audience: a.jwt_audience,
            jwt_token_expiry_hours: a.jwt_token_expiry_hours,
            jwt_private_key: a.jwt_private_key,
            jwt_public_key: a.jwt_public_key,
            api_key: a.api_key,
            api_key_header: a.api_key_header,

            // Use existing routes_map() helper from v2 AuthConfig
            protected_routes: a.routes_map(),
        }),

        rate_limiting: v2.rate_limiting,
        resilience: v2.resilience,
    }
}

/// Flatten orchestration MPSC channels from v2 nested structure to legacy flat structure
fn flatten_orchestration_mpsc(v2: OrchestrationMpscChannelsConfig) -> LegacyOrchestrationMpscConfig {
    LegacyOrchestrationMpscConfig {
        // Command processor
        command_buffer_size: v2.command_processor.command_buffer_size,

        // Event systems
        event_channel_buffer_size: v2.event_systems.event_channel_buffer_size,

        // Event listeners
        pgmq_event_buffer_size: v2.event_listeners.pgmq_event_buffer_size,
    }
}

/// Convert v2 worker config to legacy structure
fn convert_worker_config(v2: WorkerV2) -> WorkerConfig {
    WorkerConfig {
        worker_id: v2.worker_id,
        worker_type: v2.worker_type,

        // Flatten event systems (v2.event_systems.worker → legacy.worker_events)
        worker_events: convert_worker_event_system(v2.event_systems.worker),

        // Step processing (unchanged)
        step_processing: v2.step_processing,

        // Health monitoring (unchanged)
        health_monitoring: v2.health_monitoring,

        // MPSC channels (flattened)
        mpsc_channels: flatten_worker_mpsc(v2.mpsc_channels),

        // Web config
        web: v2.web.map(|w| convert_web_config(w)),  // Reuse same conversion function
    }
}

/// Convert v2 worker event system to legacy structure
fn convert_worker_event_system(v2: WorkerEventSystemConfig) -> LegacyWorkerEventSystemConfig {
    LegacyWorkerEventSystemConfig {
        system_id: v2.system_id,
        deployment_mode: v2.deployment_mode,

        // Flatten timing
        health_check_interval_seconds: v2.timing.health_check_interval_seconds,
        fallback_polling_interval_seconds: v2.timing.fallback_polling_interval_seconds,
        visibility_timeout_seconds: v2.timing.visibility_timeout_seconds,
        processing_timeout_seconds: v2.timing.processing_timeout_seconds,
        claim_timeout_seconds: v2.timing.claim_timeout_seconds,

        // Flatten processing
        max_concurrent_operations: v2.processing.max_concurrent_operations,
        batch_size: v2.processing.batch_size,
        max_retries: v2.processing.max_retries,

        // Backoff
        backoff: convert_event_system_backoff(v2.processing.backoff),

        // Health
        health: v2.health,

        // Worker-specific metadata (flatten)
        ffi_integration_enabled: v2.metadata.in_process_events.ffi_integration_enabled,
        deduplication_cache_size: v2.metadata.in_process_events.deduplication_cache_size,

        listener: v2.metadata.listener,
        fallback_poller: v2.metadata.fallback_poller,
        resource_limits: v2.metadata.resource_limits,
    }
}

/// Flatten worker MPSC channels from v2 nested structure to legacy flat structure
fn flatten_worker_mpsc(v2: WorkerMpscChannelsConfig) -> LegacyWorkerMpscConfig {
    LegacyWorkerMpscConfig {
        // Command processor
        command_buffer_size: v2.command_processor.command_buffer_size,

        // Event systems
        event_channel_buffer_size: v2.event_systems.event_channel_buffer_size,

        // Event subscribers
        completion_buffer_size: v2.event_subscribers.completion_buffer_size,
        result_buffer_size: v2.event_subscribers.result_buffer_size,

        // In-process events
        broadcast_buffer_size: v2.in_process_events.broadcast_buffer_size,

        // Event listeners
        pgmq_event_buffer_size: v2.event_listeners.pgmq_event_buffer_size,
    }
}

/// Reconstruct legacy MPSC channels from v2 structure
///
/// Legacy expects a single flat MpscChannelsConfig with all channels.
/// V2 splits channels into common.mpsc_channels, orchestration.mpsc_channels, worker.mpsc_channels.
/// This function merges them back together.
fn reconstruct_mpsc_channels(v2: &TaskerConfigV2) -> MpscChannelsConfig {
    MpscChannelsConfig {
        // === SHARED CHANNELS (from common.mpsc_channels) ===
        shared: LegacySharedMpscConfig {
            event_queue_buffer_size: v2.common.mpsc_channels.event_publisher.event_queue_buffer_size,
            ruby_event_buffer_size: v2.common.mpsc_channels.ffi.ruby_event_buffer_size,
            overflow_policy: v2.common.mpsc_channels.overflow_policy.clone(),
        },

        // === ORCHESTRATION CHANNELS ===
        orchestration: v2.orchestration.as_ref().map(|o| {
            flatten_orchestration_mpsc(o.mpsc_channels.clone())
        }),

        // === WORKER CHANNELS ===
        worker: v2.worker.as_ref().map(|w| {
            flatten_worker_mpsc(w.mpsc_channels.clone())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v2_to_legacy_bridge_round_trip() {
        // Load v2 config
        let toml_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("config/v2/complete-test.toml");

        let toml_content = std::fs::read_to_string(&toml_path)
            .expect("Failed to read complete-test.toml");

        let v2_config: TaskerConfigV2 = toml::from_str(&toml_content)
            .expect("Failed to deserialize v2 config");

        // Validate v2 config
        v2_config.validate().expect("V2 config validation failed");

        // Convert to legacy via bridge
        let legacy_config: TaskerConfig = v2_config.clone().into();

        // Verify critical fields preserved
        assert_eq!(legacy_config.database.url, v2_config.common.database.url);
        assert_eq!(legacy_config.queues.backend, v2_config.common.queues.backend);

        // Verify orchestration config converted
        let orch = legacy_config.orchestration.expect("Orchestration config missing");
        let orch_v2 = v2_config.orchestration.expect("V2 orchestration missing");
        assert_eq!(orch.mode, orch_v2.mode);

        // Verify protected routes converted correctly
        if let Some(web) = orch.web {
            if let Some(auth) = web.auth {
                // Should have HashMap with route keys "METHOD /path"
                assert!(auth.protected_routes.contains_key("DELETE /v1/tasks/{task_uuid}"));

                let route = auth.protected_routes.get("DELETE /v1/tasks/{task_uuid}").unwrap();
                assert_eq!(route.auth_type, "bearer");
                assert!(route.required);
            }
        }

        // Verify MPSC channels reconstructed
        assert_eq!(
            legacy_config.mpsc_channels.shared.event_queue_buffer_size,
            v2_config.common.mpsc_channels.event_publisher.event_queue_buffer_size
        );
    }

    #[test]
    fn test_protected_routes_vec_to_hashmap_conversion() {
        use super::super::tasker_v2::{ProtectedRouteConfig, AuthConfig as AuthV2};

        let v2_auth = AuthV2 {
            enabled: true,
            jwt_issuer: "tasker".to_string(),
            jwt_audience: "api".to_string(),
            jwt_token_expiry_hours: 24,
            jwt_private_key: "key".to_string(),
            jwt_public_key: "pub".to_string(),
            api_key: "secret".to_string(),
            api_key_header: "X-API-Key".to_string(),
            protected_routes: vec![
                ProtectedRouteConfig {
                    method: "POST".to_string(),
                    path: "/v1/tasks".to_string(),
                    auth_type: "bearer".to_string(),
                    required: false,
                },
                ProtectedRouteConfig {
                    method: "DELETE".to_string(),
                    path: "/v1/tasks/{task_uuid}".to_string(),
                    auth_type: "bearer".to_string(),
                    required: true,
                },
            ],
        };

        // Convert using routes_map()
        let routes_map = v2_auth.routes_map();

        // Verify HashMap structure
        assert_eq!(routes_map.len(), 2);
        assert!(routes_map.contains_key("POST /v1/tasks"));
        assert!(routes_map.contains_key("DELETE /v1/tasks/{task_uuid}"));

        // Verify route config
        let delete_route = routes_map.get("DELETE /v1/tasks/{task_uuid}").unwrap();
        assert_eq!(delete_route.auth_type, "bearer");
        assert!(delete_route.required);
    }
}
```

**Add to mod.rs**:

```rust
// tasker-shared/src/config/tasker/mod.rs

pub mod tasker;
pub mod tasker_v2;
pub mod bridge;  // NEW

pub use tasker::TaskerConfig;
pub use tasker_v2::TaskerConfig as TaskerConfigV2;
```

**Success Criteria**:
- Bridge compiles without errors
- Round-trip test passes (v2 → legacy → verify all fields preserved)
- Protected routes conversion test passes
- All critical fields mapped correctly

### 2.3. Add V2 Loading to SystemContext

**Rationale**: Provide `load_v2_for_orchestration()` and `load_v2_for_worker()` methods that:
1. Load v2 TOML
2. Deserialize to TaskerConfigV2
3. Validate
4. Convert via `.into()` bridge
5. Delegate to existing `new_for_orchestration()` / `new_for_worker()` methods

**Modify**: `tasker-shared/src/system_context.rs`

**Add V2 Loading Methods**:

```rust
// tasker-shared/src/system_context.rs

use crate::config::tasker_v2::TaskerConfigV2;
use validator::Validate;

impl SystemContext {
    /// Load v2 configuration for orchestration context
    ///
    /// This loads v2 TOML, validates, converts to legacy via bridge, then delegates
    /// to existing new_for_orchestration() logic. Temporary during migration - will
    /// be replaced in Phase 6-8 with direct v2 usage.
    ///
    /// # Arguments
    /// * `config_path` - Path to v2 TOML config file
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    /// * `Arc<SystemContext>` - Initialized system context
    ///
    /// # Errors
    /// * Config loading errors (file not found, TOML syntax)
    /// * Validation errors (invalid field values)
    /// * Conversion errors (bridge implementation bugs)
    pub fn load_v2_for_orchestration(
        config_path: &str,
        pool: PgPool,
    ) -> TaskerResult<Arc<Self>> {
        info!("Loading v2 config for orchestration from: {}", config_path);

        // 1. Load v2 TOML
        let toml_content = std::fs::read_to_string(config_path)
            .map_err(|e| ConfigurationError::config_file_not_found(
                vec![PathBuf::from(config_path)]
            ))?;

        // 2. Deserialize to v2 struct
        let v2_config: TaskerConfigV2 = toml::from_str(&toml_content)
            .map_err(|e| ConfigurationError::json_deserialization_error(
                format!("Failed to deserialize v2 config: {}", config_path),
                e,
            ))?;

        // 3. Validate v2 config
        v2_config.validate()
            .map_err(|e| ConfigurationError::validation_error(
                format!("V2 config validation failed: {:?}", e)
            ))?;

        // 4. Verify orchestration context present
        if !v2_config.has_orchestration() {
            return Err(ConfigurationError::validation_error(
                "Orchestration config missing - expected [orchestration] section in v2 config"
            ).into());
        }

        info!("V2 config loaded and validated, converting to legacy format");

        // 5. Convert to legacy via bridge
        let legacy_config: TaskerConfig = v2_config.into();

        // 6. Delegate to existing orchestration initialization
        Self::new_for_orchestration(legacy_config, pool)
    }

    /// Load v2 configuration for worker context
    ///
    /// This loads v2 TOML, validates, converts to legacy via bridge, then delegates
    /// to existing new_for_worker() logic. Temporary during migration - will
    /// be replaced in Phase 6-8 with direct v2 usage.
    ///
    /// # Arguments
    /// * `config_path` - Path to v2 TOML config file
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    /// * `Arc<SystemContext>` - Initialized system context
    pub fn load_v2_for_worker(
        config_path: &str,
        pool: PgPool,
    ) -> TaskerResult<Arc<Self>> {
        info!("Loading v2 config for worker from: {}", config_path);

        // Same steps as orchestration
        let toml_content = std::fs::read_to_string(config_path)
            .map_err(|e| ConfigurationError::config_file_not_found(
                vec![PathBuf::from(config_path)]
            ))?;

        let v2_config: TaskerConfigV2 = toml::from_str(&toml_content)
            .map_err(|e| ConfigurationError::json_deserialization_error(
                format!("Failed to deserialize v2 config: {}", config_path),
                e,
            ))?;

        v2_config.validate()
            .map_err(|e| ConfigurationError::validation_error(
                format!("V2 config validation failed: {:?}", e)
            ))?;

        // Verify worker context present
        if !v2_config.has_worker() {
            return Err(ConfigurationError::validation_error(
                "Worker config missing - expected [worker] section in v2 config"
            ).into());
        }

        info!("V2 config loaded and validated, converting to legacy format");

        let legacy_config: TaskerConfig = v2_config.into();

        Self::new_for_worker(legacy_config, pool)
    }

    /// Load v2 configuration for complete context (tests only)
    ///
    /// Loads v2 TOML with both orchestration and worker contexts.
    /// Used primarily in integration tests.
    pub fn load_v2_complete(
        config_path: &str,
        pool: PgPool,
    ) -> TaskerResult<Arc<Self>> {
        let toml_content = std::fs::read_to_string(config_path)
            .map_err(|e| ConfigurationError::config_file_not_found(
                vec![PathBuf::from(config_path)]
            ))?;

        let v2_config: TaskerConfigV2 = toml::from_str(&toml_content)
            .map_err(|e| ConfigurationError::json_deserialization_error(
                format!("Failed to deserialize v2 config: {}", config_path),
                e,
            ))?;

        v2_config.validate()
            .map_err(|e| ConfigurationError::validation_error(
                format!("V2 config validation failed: {:?}", e)
            ))?;

        // Verify complete config
        if !v2_config.is_complete() {
            return Err(ConfigurationError::validation_error(
                "Complete config required - expected both [orchestration] and [worker] sections"
            ).into());
        }

        let legacy_config: TaskerConfig = v2_config.into();

        // Use orchestration initialization (includes full context)
        Self::new_for_orchestration(legacy_config, pool)
    }
}
```

**Success Criteria**:
- `load_v2_for_orchestration()` method added
- `load_v2_for_worker()` method added
- `load_v2_complete()` method added for tests
- All methods perform validation before conversion
- All methods delegate to existing `new_for_*()` logic

---

## Phase 3: Bootstrapping Migration (Steps 6-7)

### 3.1. Update Orchestration Bootstrap

**Locate**: Orchestration main entry point (likely `tasker-orchestration/src/main.rs` or `tasker-orchestration/src/bin/orchestration.rs`)

**Current Pattern** (to be replaced):
```rust
// OLD - Legacy loading
use tasker_shared::config::ConfigLoader;
use tasker_shared::system_context::SystemContext;

#[tokio::main]
async fn main() -> TaskerResult<()> {
    // Load legacy config
    let config = ConfigLoader::load_from_env()?;
    let pool = create_postgres_pool(&config.database).await?;
    let system_context = SystemContext::new_for_orchestration(config, pool)?;

    // ... orchestration initialization
}
```

**New Pattern** (v2 loading):
```rust
// NEW - V2 loading
use tasker_shared::system_context::SystemContext;
use std::env;

#[tokio::main]
async fn main() -> TaskerResult<()> {
    // Get v2 config path from environment
    let config_path = env::var("TASKER_CONFIG_PATH")
        .unwrap_or_else(|_| {
            let env_name = env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string());
            format!("config/tasker/v2/orchestration-{}.toml", env_name)
        });

    info!("Loading orchestration v2 config from: {}", config_path);

    // Load v2 config and initialize system context
    // Note: Pool creation happens inside load_v2_for_orchestration via database config
    let system_context = SystemContext::load_v2_for_orchestration(&config_path).await?;

    // ... orchestration initialization (unchanged)
}
```

**Pool Creation** (might need adjustment):
- Check if `load_v2_for_orchestration()` signature expects pool as parameter
- If yes, create pool first using v2 database config
- If no, pool creation should happen inside loading method

**Success Criteria**:
- Orchestration binary loads v2 config on startup
- `TASKER_CONFIG_PATH` environment variable respected
- Fallback to `orchestration-{env}.toml` pattern works
- All existing orchestration initialization unchanged

### 3.2. Update Worker Bootstrap

**Locate**: Worker main entry point (likely `tasker-worker/src/main.rs` or `tasker-worker/src/bin/worker.rs`)

**Apply Same Transformation** as orchestration:

```rust
// NEW - V2 loading
use tasker_shared::system_context::SystemContext;
use std::env;

#[tokio::main]
async fn main() -> TaskerResult<()> {
    let config_path = env::var("TASKER_CONFIG_PATH")
        .unwrap_or_else(|_| {
            let env_name = env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string());
            format!("config/tasker/v2/worker-{}.toml", env_name)
        });

    info!("Loading worker v2 config from: {}", config_path);

    let system_context = SystemContext::load_v2_for_worker(&config_path).await?;

    // ... worker initialization (unchanged)
}
```

**Success Criteria**:
- Worker binary loads v2 config on startup
- Same environment variable and fallback logic as orchestration
- All existing worker initialization unchanged

### 3.3. Update Test Helpers

**Modify**: `tasker-shared/src/system_context.rs` test helpers

**Current Test Pattern** (to be replaced):
```rust
impl SystemContext {
    #[cfg(test)]
    pub fn with_pool(pool: PgPool) -> TaskerResult<Arc<Self>> {
        let config = ConfigLoader::load_from_path("config/tasker/complete-test.toml")?;
        Self::new_for_orchestration(config, pool)
    }
}
```

**New Test Pattern**:
```rust
impl SystemContext {
    #[cfg(test)]
    pub fn with_pool(pool: PgPool) -> TaskerResult<Arc<Self>> {
        // Load v2 complete test config
        let config_path = "config/tasker/v2/complete-test.toml";
        Self::load_v2_complete(config_path, pool)
    }

    #[cfg(test)]
    pub fn with_pool_legacy(pool: PgPool) -> TaskerResult<Arc<Self>> {
        // Keep legacy loading available during migration for legacy tests
        let config = ConfigLoader::load_from_path("config/tasker/complete-test.toml")?;
        Self::new_for_orchestration(config, pool)
    }
}
```

**Success Criteria**:
- Test helper uses v2 config by default
- Legacy helper available for tests not yet migrated
- All tests using `with_pool()` automatically use v2 config

---

## Phase 4: Environment Configuration (Step 8)

### 4.1. Update .env Files

**Files to Update**:
- `.env` (root)
- `.env.test` (if exists)
- `.env.development` (if exists)
- `.env.production` (if exists)

**Transformation**:

```bash
# OLD (legacy path):
TASKER_CONFIG_PATH=config/tasker/complete-test.toml

# NEW (v2 path):
TASKER_CONFIG_PATH=config/tasker/v2/complete-test.toml

# OR context-specific:
TASKER_CONFIG_PATH=config/tasker/v2/orchestration-test.toml  # Orchestration only
TASKER_CONFIG_PATH=config/tasker/v2/worker-test.toml         # Worker only
```

**Success Criteria**:
- All `.env` files updated to point to v2 paths
- Context-appropriate paths (orchestration vs worker vs complete)
- Committed to git

### 4.2. Update Docker Compose Files

**Files to Update**:
- `docker/docker-compose.test.yml`
- `docker/docker-compose.dev.yml` (if exists)
- `docker/docker-compose.prod.yml` (if exists)
- `docker-compose.yml` (root, if exists)

**Transformation**:

```yaml
# OLD (in orchestration service):
services:
  orchestration:
    environment:
      TASKER_CONFIG_PATH: /app/config/tasker/orchestration-test.toml

# NEW (v2 path):
services:
  orchestration:
    environment:
      TASKER_CONFIG_PATH: /app/config/tasker/v2/orchestration-test.toml

# OLD (in worker service):
services:
  worker:
    environment:
      TASKER_CONFIG_PATH: /app/config/tasker/worker-test.toml

# NEW (v2 path):
services:
  worker:
    environment:
      TASKER_CONFIG_PATH: /app/config/tasker/v2/worker-test.toml
```

**Success Criteria**:
- All docker-compose files updated
- Orchestration and worker services point to correct v2 paths
- Docker builds successfully with new paths

### 4.3. Update CI Scripts

**Files to Check**:
- `.github/scripts/start-native-services.sh`
- `.github/workflows/*.yml` (any workflow files setting TASKER_CONFIG_PATH)
- Any other CI/CD scripts

**Transformation**:

```bash
# OLD (in CI script):
export TASKER_CONFIG_PATH="config/tasker/orchestration-test.toml"

# NEW (v2 path):
export TASKER_CONFIG_PATH="config/tasker/v2/orchestration-test.toml"
```

**Success Criteria**:
- All CI scripts updated
- CI pipeline references v2 paths
- GitHub Actions workflows updated (if applicable)

---

## Phase 5: Validation & Iteration (Step 9)

### 5.1. Cargo Check

```bash
cargo check --all-features
```

**Expected Issues**:
1. **Import Errors**: `tasker_v2_proposal` → `tasker_v2` rename
   - **Fix**: Already handled in Phase 1.4
   - **Verify**: `grep -r "tasker_v2_proposal" --include="*.rs" .` should return empty

2. **Type Mismatches**: Bridge conversion missing fields
   - **Fix**: Review bridge.rs `From` implementation
   - **Verify**: Compare v2 struct fields with legacy struct fields

3. **Missing Struct Fields**: v2 adds new fields not in legacy
   - **Fix**: Add default values in bridge conversion
   - **Document**: Note fields that can't be converted

**Success Criteria**:
- `cargo check --all-features` passes with zero errors
- All import paths correct
- All type conversions compile

### 5.2. Clippy

```bash
cargo clippy --all-targets --all-features
```

**Expected Warnings**:
1. **Unused Code**: Legacy config structs may show unused warnings
   - **Fix**: Add `#[allow(dead_code)]` temporarily (will be removed in Phase 6-8)
   - **Document**: Track unused code for eventual removal

2. **Complex Functions**: Bridge implementation may trigger complexity warnings
   - **Fix**: Extract helper functions to reduce complexity
   - **Example**: `convert_orchestration_config()` split into smaller functions

3. **Performance**: HashMap conversions may show performance warnings
   - **Fix**: Document that conversion happens once at startup (acceptable cost)
   - **Alternative**: Consider caching if needed

**Success Criteria**:
- `cargo clippy` passes or all warnings documented
- No critical warnings (e.g., potential bugs)
- Performance warnings acceptable for startup-time operations

### 5.3. Run Test Suite

```bash
cargo test --all-features
```

**Expected Failures**:

1. **Config Structure Tests**: Tests checking config field paths
   ```rust
   // Test expecting legacy structure:
   assert_eq!(config.orchestration_events.system_id, "...");

   // Fails because now it's:
   // config.orchestration.event_systems.orchestration.system_id
   ```
   - **Fix**: Update test assertions for new structure (or keep using bridge)

2. **Config Value Tests**: Tests validating specific config values
   - **Fix**: Verify values in v2 TOML match expected test values
   - **Example**: If test expects `pool_size = 5`, ensure v2 test config has same value

3. **Integration Tests**: End-to-end tests if bridge has bugs
   - **Fix**: Debug bridge conversion logic
   - **Tool**: Add logging to bridge functions to trace conversions

**Debugging Strategy**:

```rust
// Add to bridge.rs for debugging
impl From<TaskerConfigV2> for TaskerConfig {
    fn from(v2: TaskerConfigV2) -> Self {
        tracing::debug!("Converting v2 config to legacy");
        tracing::debug!("V2 database URL: {}", v2.common.database.url);

        let legacy = TaskerConfig {
            database: v2.common.database.clone(),
            // ...
        };

        tracing::debug!("Legacy database URL: {}", legacy.database.url);
        legacy
    }
}
```

**Iterative Fix Process**:
1. Run tests: `cargo test --all-features 2>&1 | tee test-output.log`
2. Identify first failure
3. Determine root cause:
   - Missing field in bridge? Add it.
   - Incorrect value in v2 TOML? Fix TOML.
   - Test assertion wrong? Update test.
4. Fix and re-run
5. Repeat until all tests pass

**Success Criteria**:
- 377 tests pass (or known failures documented with issues)
- No regressions in existing functionality
- Bridge conversion verified correct

### 5.4. Docker Compose Integration Test

```bash
# Start services with v2 config
docker compose -f docker/docker-compose.test.yml up --build -d

# Check orchestration health
curl http://localhost:8080/health

# Check worker health
curl http://localhost:8081/health

# Run integration tests
cargo test --test '*integration*'

# Clean up
docker compose -f docker/docker-compose.test.yml down
```

**Expected Issues**:
1. **Config Loading Failures**: Services fail to start
   - **Symptom**: Container exits immediately
   - **Debug**: `docker logs <container-id>`
   - **Fix**: Check v2 TOML syntax, validate paths

2. **Health Check Failures**: Services start but unhealthy
   - **Symptom**: Health endpoint returns error
   - **Debug**: Check logs for config validation errors
   - **Fix**: Verify v2 config values match expectations

3. **Integration Test Failures**: Services healthy but tests fail
   - **Symptom**: Tests can't communicate with services
   - **Debug**: Check if bridge converted config correctly
   - **Fix**: Review bridge logic, compare with expected behavior

**Success Criteria**:
- Docker Compose starts successfully
- All health checks pass
- Integration tests pass
- Services operate correctly with v2 config

### 5.5. Document Breaking Changes

Create migration log: `docs/ticket-specs/TAS-61/migration-log.md`

**Template**:

```markdown
# TAS-61 Migration Log

## Compilation Errors Encountered

### Error 1: Module not found 'tasker_v2_proposal'
- **Location**: tasker-shared/src/config/mod.rs
- **Fix**: Renamed to tasker_v2
- **Commit**: <hash>

### Error 2: Missing field in bridge conversion
- **Location**: bridge.rs line 145
- **Fix**: Added missing field with default value
- **Commit**: <hash>

## Bridge Conversion Edge Cases

### Protected Routes HashMap Conversion
- **Issue**: Route keys need exact "METHOD /path" format
- **Solution**: Used routes_map() helper for conversion
- **Test**: test_protected_routes_vec_to_hashmap_conversion

### MPSC Channel Reconstruction
- **Issue**: Common, orchestration, worker channels must merge
- **Solution**: reconstruct_mpsc_channels() function
- **Test**: test_v2_to_legacy_bridge_round_trip

## Config Value Changes

### Database Pool Size (test environment)
- **Legacy**: 20 connections
- **V2**: 5 connections
- **Rationale**: Test environment should use smaller pool
- **Location**: config/tasker/v2/environments/test/common.toml

## Test Updates Required

### Test 1: test_config_structure()
- **File**: tasker-shared/src/config/tests.rs
- **Change**: Updated assertions for v2 structure
- **Before**: config.orchestration_events.system_id
- **After**: config.orchestration.event_systems.orchestration.system_id

### Test 2: test_mpsc_channel_buffer_sizes()
- **File**: tasker-orchestration/tests/config_tests.rs
- **Change**: Updated buffer size paths
- **Impact**: Tests now use v2 config

## Known Issues

### Issue 1: Legacy tests still use legacy config
- **Status**: Intentional - dual support during migration
- **Resolution**: Phase 6-8 will migrate tests
- **Workaround**: Use with_pool_legacy() for legacy tests
```

**Success Criteria**:
- All errors documented
- All edge cases noted
- All config changes tracked
- Known issues with resolution plan

---

## Risk Mitigation

### Critical Risks

#### Risk 1: Bridge Conversion Loses Data or Introduces Bugs

**Mitigation**:
- Write comprehensive unit tests for bridge in Phase 2.2
- Round-trip test: v2 → legacy → verify all fields preserved
- Log conversions during initial rollout to catch issues

**Test Coverage**:
```rust
#[test]
fn test_bridge_preserves_all_fields() {
    // Test every field is preserved in conversion
}

#[test]
fn test_bridge_handles_optional_configs() {
    // Test orchestration-only, worker-only, complete configs
}

#[test]
fn test_bridge_nested_structures() {
    // Test event systems, MPSC channels, web config
}
```

#### Risk 2: Protected Routes Transformation Breaks Auth Checks

**Mitigation**:
- Create specific test for `routes_map()` conversion
- Manual verification of all protected routes in generated configs
- Integration test with actual auth checks

**Verification**:
```bash
# Generate v2 config
cargo run --bin config-merger -- --version v2 --environment test --context orchestration > /tmp/orch-test-v2.toml

# Check protected routes section
grep -A 5 "protected_routes" /tmp/orch-test-v2.toml

# Verify all expected routes present:
# - POST /v1/tasks
# - DELETE /v1/tasks/{task_uuid}
# - PATCH /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}
# - GET /v1/analytics/bottlenecks
# - GET /v1/analytics/performance
```

#### Risk 3: MPSC Channel Reconstruction Produces Invalid Structure

**Mitigation**:
- Log and validate MPSC channel buffer sizes after conversion
- Compare buffer sizes between v2 source and legacy output
- Test with actual channel creation to ensure sizes work

**Validation**:
```rust
#[test]
fn test_mpsc_channel_reconstruction() {
    let v2_config = load_v2_test_config();
    let legacy_config: TaskerConfig = v2_config.clone().into();

    // Verify common channels
    assert_eq!(
        legacy_config.mpsc_channels.shared.event_queue_buffer_size,
        v2_config.common.mpsc_channels.event_publisher.event_queue_buffer_size
    );

    // Verify orchestration channels
    if let Some(orch) = legacy_config.mpsc_channels.orchestration {
        let orch_v2 = v2_config.orchestration.unwrap();
        assert_eq!(
            orch.command_buffer_size,
            orch_v2.mpsc_channels.command_processor.command_buffer_size
        );
    }
}
```

#### Risk 4: Tests Fail Due to Config Path Changes

**Mitigation**:
- Keep legacy `complete-test.toml` as instructed
- Tests discover configs via `TASKER_ENV=test`, not hardcoded paths
- Provide `with_pool_legacy()` helper for tests not yet migrated

**Test Strategy**:
- Default tests use v2 (`with_pool()` → v2 config)
- Legacy tests explicitly use `with_pool_legacy()`
- Document which tests still use legacy in migration log

### Rollback Strategy

Even with hard-switch approach, maintain rollback capability:

1. **Keep Legacy Configs**: Don't delete `config/tasker/base/*` until v2 proven stable
2. **Git Branch**: All work on `jcoletaylor/tas-61-...` branch, can revert main
3. **Docker Images**: Tag with v1/v2 labels for quick rollback
4. **Emergency Fallback**: Temporarily revert `SystemContext` to legacy loading

**Rollback Steps** (if critical issue found):
```bash
# 1. Revert environment variables
export TASKER_CONFIG_PATH="config/tasker/complete-test.toml"

# 2. Revert SystemContext loading (emergency only)
# Edit system_context.rs:
# - Comment out v2 loading in with_pool()
# - Restore legacy loading

# 3. Rebuild and test
cargo build --all-features
cargo test --all-features

# 4. Redeploy with legacy config
docker compose up --build -d
```

---

## Success Criteria

### Phase 1 Complete
- ✅ V2 directory structure exists (`config/tasker/v2/{base,environments}`)
- ✅ All 12 v2 config files created (3 base + 9 environment)
- ✅ All files transformed to v2 syntax (context-based structure)
- ✅ `tasker_v2.rs` exports `TaskerConfigV2`
- ✅ Imports updated across workspace (no `tasker_v2_proposal` references)
- ✅ Protected routes use array-of-tables syntax
- ✅ All values preserved from legacy configs

### Phase 2 Complete
- ✅ `ConfigVersion` enum added to merger.rs
- ✅ CLI accepts `--version v2` flag (if CLI exists)
- ✅ Bridge implementation compiles (`bridge.rs`)
- ✅ Bridge unit tests pass (100% coverage of conversion logic)
- ✅ `SystemContext` has `load_v2_for_orchestration()`, `load_v2_for_worker()`, `load_v2_complete()` methods
- ✅ Round-trip test passes (v2 → legacy → verify)

### Phase 3 Complete
- ✅ Orchestration binary bootstraps from v2 config
- ✅ Worker binary bootstraps from v2 config
- ✅ Test helpers use v2 config (`with_pool()`)
- ✅ Legacy test helper available (`with_pool_legacy()`)
- ✅ `TASKER_CONFIG_PATH` environment variable respected

### Phase 4 Complete
- ✅ All `.env` files updated to v2 paths
- ✅ All docker-compose files updated to v2 paths
- ✅ CI scripts updated to v2 paths
- ✅ Changes committed to git

### Phase 5 Complete
- ✅ `cargo check --all-features` passes
- ✅ `cargo clippy --all-targets --all-features` passes (or documented exceptions)
- ✅ 377 tests pass (or known failures documented)
- ✅ Docker Compose starts successfully
- ✅ Integration tests pass in CI
- ✅ Migration log created with all issues documented

---

## Timeline

**Total Estimated Time**: 8-14 hours

### Phase 1: 2-3 hours
- Directory creation: 5 minutes
- File copying: 5 minutes
- TOML transformation: 1.5-2 hours (12 files, ~15 min each)
- Module rename: 30 minutes

### Phase 2: 3-4 hours
- CLI version flag: 30 minutes
- Bridge implementation: 2-2.5 hours
- SystemContext loading: 30 minutes
- Testing: 30 minutes

### Phase 3: 1-2 hours
- Orchestration bootstrap: 30 minutes
- Worker bootstrap: 30 minutes
- Test helpers: 30 minutes
- Verification: 30 minutes

### Phase 4: 30 minutes
- .env updates: 10 minutes
- Docker Compose updates: 10 minutes
- CI script updates: 10 minutes

### Phase 5: 2-4 hours
- Cargo check: 15 minutes
- Clippy: 15 minutes
- Test suite: 1-2 hours (iterative fixes)
- Docker integration: 30 minutes
- Documentation: 30 minutes

---

## Post-Migration Next Steps

After this implementation plan completes, future phases:

### Phase 6: Migrate Codebase to Use TaskerConfigV2 Directly
**Estimated**: 15-20 hours

- Update all 100+ config field accesses
- Change `config.database` → `config.common.database`
- Change `config.orchestration_events` → `config.orchestration.event_systems.orchestration`
- Update MPSC channel path accesses
- Remove bridge implementation (temporary code)

### Phase 7: Remove Legacy Config Structs
**Estimated**: 3-5 hours

- Delete `tasker.rs` (legacy `TaskerConfig`)
- Archive `config/tasker/` to `config/legacy/`
- Update all documentation
- Remove backward compatibility code

### Phase 8: Generate Production Configs in V2 Format
**Estimated**: 2-3 hours + deployment time

- Run CLI with `--version v2` for all environments
- Validate generated production configs
- Deploy v2 configs to production
- Monitor for config-related issues
- Remove v1 fallback paths

---

## Execution Order

**Start to Finish**:

1. **Phase 1.1-1.4**: Directory structure and file transformation (2-3 hours)
2. **Phase 2.1-2.3**: Bridge and loading infrastructure (3-4 hours)
3. **Phase 3.1-3.3**: Bootstrapping migration (1-2 hours)
4. **Phase 4.1-4.3**: Environment config updates (30 min)
5. **Phase 5.1-5.4**: Validation and iteration (2-4 hours)

**Total**: 8.5-13.5 hours (allow buffer to 14 hours)

**Checkpoint**: After each phase completes, commit changes with descriptive message referencing TAS-61 and phase number.

---

## Appendix: Key Files Reference

### Configuration Files
- **V2 Base Configs**: `config/tasker/v2/base/{common,orchestration,worker}.toml`
- **V2 Test Configs**: `config/tasker/v2/environments/test/{common,orchestration,worker}.toml`
- **V2 Complete Test**: `config/v2/complete-test.toml` (reference implementation)

### Rust Code Files
- **V2 Proposal**: `tasker-shared/src/config/tasker/tasker_v2.rs` (renamed from tasker_v2_proposal.rs)
- **Bridge**: `tasker-shared/src/config/tasker/bridge.rs` (new file)
- **SystemContext**: `tasker-shared/src/system_context.rs` (modified)
- **ConfigMerger**: `tasker-shared/src/config/merger.rs` (modified)

### Environment Files
- **Env Variables**: `.env`, `.env.test`, `.env.development`, `.env.production`
- **Docker Compose**: `docker/docker-compose.test.yml`, etc.
- **CI Scripts**: `.github/scripts/start-native-services.sh`

### Documentation
- **Architecture Spec**: `docs/ticket-specs/TAS-61/new-config-architecture.md`
- **Implementation Plan**: `docs/ticket-specs/TAS-61/implementation-plan.md`
- **Protected Routes**: `docs/ticket-specs/TAS-61/config-protected-routes-syntax.md`
- **Migration Log**: `docs/ticket-specs/TAS-61/migration-log.md` (to be created)

---

**End of Implementation Plan**

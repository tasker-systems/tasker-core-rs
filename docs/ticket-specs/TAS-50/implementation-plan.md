# TAS-50 Implementation Plan: Configuration Cleanup

**Status**: In Progress
**Created**: 2025-10-19
**Last Updated**: 2025-10-19

## Overview

This document provides a resumable, phase-by-phase plan for removing UNUSED configuration parameters identified in the [configuration usage analysis](./config-usage-analysis.md).

**Goal**: Remove ~63 UNUSED parameters to reduce configuration bloat, improve operator clarity, and reduce maintenance burden.

**Analysis Reference**: [config-usage-analysis.md](./config-usage-analysis.md) - Comprehensive analysis of all 308 parameters

---

## Progress Summary

- [ ] **Phase 1**: Remove entire UNUSED subsystems (59 parameters)
- [ ] **Phase 2**: Investigate NEEDS_REVIEW parameters (9 parameters)
- [ ] **Phase 3**: Document FUNCTIONAL parameters via `._docs`
- [ ] **Phase 4**: Update tests and validation
- [ ] **Phase 5**: Update documentation and examples

**Current Phase**: Not Started

---

## Phase 1: Remove UNUSED Subsystems

### 1.1 Remove `orchestration_system.operational_state.*` (10 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Remove**:
- `operational_state.enable_shutdown_aware_monitoring`
- `operational_state.suppress_alerts_during_shutdown`
- `operational_state.startup_health_threshold_multiplier`
- `operational_state.shutdown_health_threshold_multiplier`
- `operational_state.graceful_shutdown_timeout_seconds`
- `operational_state.emergency_shutdown_timeout_seconds`
- `operational_state.enable_transition_logging`
- `operational_state.transition_log_level`

**Files to Modify**:

TOML Configuration:
- [ ] `config/tasker/base/orchestration.toml` - Remove `[orchestration_system.operational_state]` section
- [ ] `config/tasker/environments/test/orchestration.toml` - Remove any overrides
- [ ] `config/tasker/environments/development/orchestration.toml` - Remove any overrides
- [ ] `config/tasker/environments/production/orchestration.toml` - Remove any overrides

Rust Code:
- [ ] `tasker-shared/src/config/components/orchestration.rs` - Remove `OperationalStateConfig` struct
- [ ] `tasker-shared/src/config/components/orchestration.rs` - Remove field from `OrchestrationSystemConfig`
- [ ] Search for any references: `rg "operational_state" --type rust`

Tests:
- [ ] Remove test expectations referencing `operational_state`
- [ ] Verify compilation: `cargo build --all-features`
- [ ] Verify tests pass: `cargo test --all-features --package tasker-shared --package tasker-orchestration`

**Verification**:
```bash
# Should return 0 results after cleanup
rg "operational_state" --type rust
rg "operational_state" config/tasker/
```

**Commit Message**:
```
refactor(TAS-50): remove UNUSED operational_state configuration

Remove 8 UNUSED parameters from operational_state subsystem.
Analysis showed 0 references in codebase.

- Remove OperationalStateConfig struct
- Remove TOML configuration sections
- Update tests

Refs: docs/ticket-specs/TAS-50/config-usage-analysis.md
```

---

### 1.2 Remove `worker_system.web.endpoints.*` (14 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Remove**:
- `endpoints.health_enabled`, `endpoints.health_path`
- `endpoints.readiness_path`, `endpoints.liveness_path`
- `endpoints.prometheus_path`, `endpoints.worker_metrics_path`
- `endpoints.status_enabled`, `endpoints.basic_status_path`, `endpoints.detailed_status_path`
- `endpoints.namespace_health_path`, `endpoints.registered_handlers_path`
- `endpoints.templates_enabled`, `endpoints.templates_base_path`, `endpoints.template_cache_enabled`

**Files to Modify**:

TOML Configuration:
- [ ] `config/tasker/base/worker.toml` - Remove `[worker_system.web.endpoints]` section
- [ ] `config/tasker/environments/test/worker.toml` - Remove any overrides
- [ ] `config/tasker/environments/development/worker.toml` - Remove any overrides
- [ ] `config/tasker/environments/production/worker.toml` - Remove any overrides

Rust Code:
- [ ] `tasker-shared/src/config/components/worker.rs` - Remove `WebEndpointsConfig` struct
- [ ] `tasker-shared/src/config/components/worker.rs` - Remove field from `WorkerWebConfig`
- [ ] Search for any references: `rg "WebEndpointsConfig|endpoints\.(health_|readiness_|liveness_)" --type rust`

Tests:
- [ ] Remove test expectations
- [ ] Verify compilation: `cargo build --all-features`
- [ ] Verify tests pass: `cargo test --all-features --package tasker-worker`

**Verification**:
```bash
# Should return 0 results after cleanup
rg "WebEndpointsConfig" --type rust
rg "endpoints\.(health_enabled|templates_enabled)" config/tasker/
```

**Commit Message**:
```
refactor(TAS-50): remove UNUSED worker web endpoints configuration

Remove 14 UNUSED endpoint configuration parameters.
These paths are hardcoded in the web handlers.

- Remove WebEndpointsConfig struct
- Remove TOML configuration sections
- Update tests

Refs: docs/ticket-specs/TAS-50/config-usage-analysis.md
```

---

### 1.3 Remove `task_readiness_events.metadata.*` (12 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Remove**:
- `metadata.enhanced_settings.startup_timeout_seconds`
- `metadata.enhanced_settings.shutdown_timeout_seconds`
- `metadata.enhanced_settings.rollback_threshold_percent`
- `metadata.notification.global_channels`
- `metadata.notification.max_payload_size_bytes`
- `metadata.notification.parse_timeout_ms`
- `metadata.notification.connection.max_connection_retries`
- `metadata.notification.connection.connection_retry_delay_seconds`
- `metadata.notification.connection.auto_reconnect`
- `metadata.coordinator.instance_id_prefix`
- `metadata.coordinator.operation_timeout_ms`
- `metadata.coordinator.stats_interval_seconds`

**Files to Modify**:

TOML Configuration:
- [ ] `config/tasker/base/common.toml` - Remove `[task_readiness_events.metadata.*]` sections
- [ ] Check environment overrides for any metadata references

Rust Code:
- [ ] `tasker-shared/src/config/components/common.rs` - Remove metadata structs
- [ ] Search: `rg "TaskReadinessMetadata|enhanced_settings|metadata\.notification" --type rust`

Tests:
- [ ] Remove test expectations
- [ ] Verify compilation: `cargo build --all-features`
- [ ] Verify tests: `cargo test --all-features --package tasker-shared`

**Verification**:
```bash
rg "metadata\.(enhanced_settings|notification|coordinator)" --type rust
rg "metadata\.(enhanced_settings|notification|coordinator)" config/tasker/
```

**Commit Message**:
```
refactor(TAS-50): remove UNUSED task_readiness_events.metadata configuration

Remove 12 UNUSED metadata parameters. Analysis showed these
were placeholders that never influenced runtime behavior.

Refs: docs/ticket-specs/TAS-50/config-usage-analysis.md
```

---

### 1.4 Remove Individual UNUSED Queue Parameters (6 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Remove**:
- `queues.default_visibility_timeout_seconds`
- `queues.max_batch_size`
- `queues.pgmq.poll_interval_ms`
- `queues.pgmq.shutdown_timeout_seconds`
- `queues.rabbitmq.heartbeat_interval_seconds`
- `queues.rabbitmq.channel_pool_size`

**Files to Modify**:

TOML Configuration:
- [ ] `config/tasker/base/common.toml` - Remove from `[queues]`, `[queues.pgmq]`, `[queues.rabbitmq]`

Rust Code:
- [ ] `tasker-shared/src/config/components/common.rs` - Remove fields from QueueConfig, PgmqConfig, RabbitMqConfig
- [ ] Search: `rg "default_visibility_timeout_seconds|max_batch_size|poll_interval_ms" --type rust`

Tests:
- [ ] Remove test expectations
- [ ] Verify compilation: `cargo build --all-features`
- [ ] Verify tests: `cargo test --all-features --package tasker-shared --package pgmq-notify`

**Verification**:
```bash
rg "default_visibility_timeout_seconds" --type rust
rg "poll_interval_ms" --type rust
rg "heartbeat_interval_seconds" --type rust
```

**Commit Message**:
```
refactor(TAS-50): remove UNUSED queue configuration parameters

Remove 6 UNUSED queue parameters:
- queues.default_visibility_timeout_seconds (0 refs)
- queues.max_batch_size (0 refs)
- queues.pgmq.poll_interval_ms (0 refs)
- queues.pgmq.shutdown_timeout_seconds (0 refs)
- queues.rabbitmq.heartbeat_interval_seconds (0 refs)
- queues.rabbitmq.channel_pool_size (0 refs)

Refs: docs/ticket-specs/TAS-50/config-usage-analysis.md
```

---

### 1.5 Remove UNUSED Orchestration System Parameters (7 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Remove**:
- `orchestration_system.max_concurrent_orchestrators`
- `orchestration_system.use_unified_state_machine`
- `orchestration_system.web.max_request_size_mb`
- `orchestration_system.web.cors.max_age_seconds`
- `orchestration_system.web.database_pools.max_total_connections_hint`
- `orchestration_system.web.resource_monitoring.*` (3 params)
- `orchestration_events.metadata.queues_populated_at_runtime`

**Files to Modify**:

TOML Configuration:
- [ ] `config/tasker/base/orchestration.toml` - Remove individual parameters

Rust Code:
- [ ] `tasker-shared/src/config/components/orchestration.rs` - Remove fields
- [ ] Search: `rg "max_concurrent_orchestrators|use_unified_state_machine" --type rust`

Tests:
- [ ] Remove test expectations
- [ ] Verify compilation: `cargo build --all-features`
- [ ] Verify tests: `cargo test --all-features --package tasker-orchestration`

**Verification**:
```bash
rg "max_concurrent_orchestrators" --type rust
rg "use_unified_state_machine" --type rust
rg "resource_monitoring" config/tasker/
```

**Commit Message**:
```
refactor(TAS-50): remove UNUSED orchestration system parameters

Remove 7 UNUSED orchestration parameters including the entire
resource_monitoring subsystem (0 references).

Refs: docs/ticket-specs/TAS-50/config-usage-analysis.md
```

---

### 1.6 Remove UNUSED Worker System Parameters (8 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Remove**:
- `worker_system.step_processing.retry_backoff_multiplier`
- `worker_system.step_processing.heartbeat_interval_seconds`
- `worker_system.health_monitoring.metrics_collection_enabled`
- `worker_system.health_monitoring.step_processing_rate_threshold`
- `worker_system.health_monitoring.memory_usage_threshold_mb`
- `worker_system.web.max_request_size_mb`
- `worker_system.web.database_pools.max_total_connections_hint`
- `worker_system.web.cors.max_age_seconds`

**Files to Modify**:

TOML Configuration:
- [ ] `config/tasker/base/worker.toml` - Remove individual parameters

Rust Code:
- [ ] `tasker-shared/src/config/components/worker.rs` - Remove fields
- [ ] Search: `rg "retry_backoff_multiplier|heartbeat_interval_seconds" --type rust`

Tests:
- [ ] Remove test expectations
- [ ] Verify compilation: `cargo build --all-features`
- [ ] Verify tests: `cargo test --all-features --package tasker-worker`

**Verification**:
```bash
rg "retry_backoff_multiplier" --type rust
rg "metrics_collection_enabled" --type rust
rg "step_processing_rate_threshold" --type rust
```

**Commit Message**:
```
refactor(TAS-50): remove UNUSED worker system parameters

Remove 8 UNUSED worker parameters. Analysis showed these were
never referenced in the codebase.

Refs: docs/ticket-specs/TAS-50/config-usage-analysis.md
```

---

### 1.7 Remove UNUSED MPSC and Backoff Parameters (3 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Remove**:
- `mpsc_channels.in_process_events.broadcast_buffer_size` (appears in both orchestration and worker)
- `backoff.default_reenqueue_delay`
- `backoff.buffer_seconds`

**Files to Modify**:

TOML Configuration:
- [ ] `config/tasker/base/orchestration.toml` - Remove `mpsc_channels.in_process_events`
- [ ] `config/tasker/base/worker.toml` - Remove `mpsc_channels.in_process_events`
- [ ] `config/tasker/base/orchestration.toml` - Remove backoff parameters

Rust Code:
- [ ] `tasker-shared/src/config/components/mpsc_channels.rs` - Remove `InProcessEventsConfig` if exists
- [ ] `tasker-shared/src/config/components/orchestration.rs` - Remove backoff fields

Tests:
- [ ] Verify compilation: `cargo build --all-features`
- [ ] Verify tests: `cargo test --all-features`

**Verification**:
```bash
rg "broadcast_buffer_size" --type rust
rg "default_reenqueue_delay" --type rust
```

**Commit Message**:
```
refactor(TAS-50): remove UNUSED MPSC and backoff parameters

Remove 3 UNUSED parameters:
- mpsc_channels.in_process_events.broadcast_buffer_size (0 refs)
- backoff.default_reenqueue_delay (0 refs)
- backoff.buffer_seconds (0 refs)

Refs: docs/ticket-specs/TAS-50/config-usage-analysis.md
```

---

## Phase 2: Investigate NEEDS_REVIEW Parameters

### 2.1 Investigate Database Parameters (6 parameters)

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

**Parameters to Investigate**:
- `database.enable_secondary_database` (6 refs) - Check if test-only
- `database.adapter` (2 refs) - Check if test-only
- `database.username` (3 refs) - Check if always from DATABASE_URL
- `database.skip_migration_check` (1 ref) - Check if test-only
- `database.pool.max_lifetime_seconds` (2 refs) - **We just added this! Verify it's actually used**
- `circuit_breakers.global_settings.auto_create_enabled` (4 refs)

**Investigation Steps**:

For each parameter:
1. [ ] Find all references: `rg "<parameter_name>" --type rust -A 3 -B 3`
2. [ ] Check if references are:
   - In test files only (test-only)
   - In config deserialization only (structural)
   - Actually driving behavior (functional)
3. [ ] Document findings in table below
4. [ ] Decide: KEEP (functional) or REMOVE (structural/test-only)

**Findings**:

| Parameter | Total Refs | Functional Refs | Test-Only Refs | Decision | Notes |
|-----------|------------|-----------------|----------------|----------|-------|
| `enable_secondary_database` | 6 | ? | ? | TBD | |
| `adapter` | 2 | ? | ? | TBD | |
| `username` | 3 | ? | ? | TBD | Likely unused if DATABASE_URL contains credentials |
| `skip_migration_check` | 1 | ? | ? | TBD | |
| `max_lifetime_seconds` | 2 | ? | ? | TBD | **CRITICAL: We just added this for integration tests!** |
| `auto_create_enabled` | 4 | ? | ? | TBD | |

**Actions After Investigation**:
- [ ] Remove parameters marked for removal
- [ ] Add `._docs` for FUNCTIONAL parameters
- [ ] Update integration tests if needed

---

## Phase 3: Document FUNCTIONAL Parameters

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

This phase adds `._docs` metadata to all FUNCTIONAL parameters per TAS-55.

**Reference**: [TAS-55.md](../TAS-55.md) - Complete TOML Configuration Documentation

**High Priority** (Critical performance parameters):
- [ ] `database.pool.max_connections` - DONE (proof of concept)
- [ ] `database.pool.min_connections` - DONE (proof of concept)
- [ ] `database.pool.idle_timeout_seconds`
- [ ] `database.pool.acquire_timeout_seconds`
- [ ] MPSC channel buffer sizes (TAS-51 - all 10+ parameters)
- [ ] Event system concurrency limits
- [ ] Circuit breaker thresholds

**See TAS-55 for complete documentation roadmap.**

---

## Phase 4: Update Tests and Validation

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

After all removals:

1. [ ] Full compilation check: `cargo build --all-features`
2. [ ] Full test suite: `cargo test --all-features`
3. [ ] Integration tests: `cargo test --all-features --test '*'`
4. [ ] Clippy check: `cargo clippy --all-features`
5. [ ] Config validation: `TASKER_ENV=test cargo run --bin config-validator`
6. [ ] Config validation: `TASKER_ENV=development cargo run --bin config-validator`
7. [ ] Config validation: `TASKER_ENV=production cargo run --bin config-validator`

**Test Failures to Fix**:
- [ ] Config command tests expecting removed parameters
- [ ] Context loading tests expecting removed sections
- [ ] Secret redaction tests with removed fields

---

## Phase 5: Update Documentation

**Status**: [ ] Not Started | [ ] In Progress | [ ] Complete

1. [ ] Update `docs/configuration.md` if exists
2. [ ] Update `CLAUDE.md` configuration section
3. [ ] Update `README.md` configuration examples
4. [ ] Update any operator runbooks
5. [ ] Create migration guide for deployments

---

## How to Resume This Work

If context is lost or we need to resume:

1. **Read this file** to understand current phase and progress
2. **Check checkboxes** to see what's complete
3. **Find current task**: Look for first unchecked box in current phase
4. **Verify state**: Run verification commands to ensure clean state
5. **Reference analysis**: Link back to [config-usage-analysis.md](./config-usage-analysis.md) for details

### Quick Resume Commands

```bash
# Check which phase you're in
cat docs/ticket-specs/TAS-50/implementation-plan.md | grep "Current Phase:"

# Verify current state
cargo build --all-features
cargo test --all-features --package tasker-shared

# Search for a specific parameter to see if removed
rg "operational_state" --type rust
rg "operational_state" config/tasker/
```

### Context Recovery

If you need to understand WHY a parameter is being removed:
1. Check the [config-usage-analysis.md](./config-usage-analysis.md)
2. Search for the parameter name
3. Look at "Usage Type" column - should say "UNUSED | 0 | None"

---

## Safety Guidelines

1. **One subsystem at a time**: Complete each section 1.1-1.7 independently
2. **Verify after each change**: Run compilation and tests
3. **Commit frequently**: Each completed section = 1 commit
4. **Keep analysis reference**: Always link back to usage analysis in commits
5. **Document unexpected findings**: Update this plan if you find references that analysis missed

---

**Last Updated**: 2025-10-19
**Next Action**: Begin Phase 1.1 - Remove operational_state subsystem

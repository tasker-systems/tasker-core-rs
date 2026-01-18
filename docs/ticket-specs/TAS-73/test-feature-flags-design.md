# TAS-73: Test Feature Flags Design

## Problem Statement

Currently, tests use `#[ignore]` to mark tests that require specific infrastructure (like the multi-instance cluster). This creates several problems:

1. **False confidence**: Tests exist but may never run, giving an illusion of coverage
2. **Easy to forget**: Running ignored tests requires explicit `--ignored` flag
3. **No compile-time separation**: All tests compile regardless of whether they can run
4. **Unclear requirements**: Hard to know what infrastructure a test needs

## Decisions Made

Based on review discussion:

1. **`--all-features` means all features** (principle of least surprise) - includes cluster tests
2. **Keep `tests/e2e/multi_instance/`** structure with feature gates (not separate `tests/cluster/`)
3. **Use `test-messaging`** as the default for crate-level tests, document it
4. **Clean break** - no transition period, just get to the good state
5. **CI limitations acknowledged** - cluster tests excluded from GitHub Actions due to resource constraints

## Current CI Structure

```
build-postgres → build-workers → [parallel]
                                 ├── integration-tests (unit → E2E matrix: pgmq, rabbitmq)
                                 ├── ruby-framework-tests
                                 ├── python-framework-tests
                                 └── typescript-framework-tests
```

**CI already handles the hierarchy correctly**:
- Unit tests: `cargo nextest run --lib --package ...` (DB only)
- E2E tests: `cargo nextest run --test '*'` (services running)
- Matrix tests both PGMQ and RabbitMQ messaging backends

## Feature Flag Design

### Hierarchy

```
test-db          # Database available (PostgreSQL)
    ↓
test-messaging   # Messaging backend available (PGMQ and/or RabbitMQ)
    ↓
test-services    # Services running (orchestration + worker, single instance)
    ↓
test-cluster     # Multi-instance cluster running (LOCAL ONLY - not in CI)
```

### Cargo.toml Features

```toml
# Root Cargo.toml
[features]
default = []

# Existing features
test-utils = [
  "tasker-orchestration/test-utils",
  "tasker-shared/test-utils",
  "tasker-worker/test-utils",
]
benchmarks = []

# Test infrastructure levels (cumulative)
test-db = []
test-messaging = ["test-db"]
test-services = ["test-messaging"]
test-cluster = ["test-services"]
```

### Test Module Structure

```
tests/
├── basic_tests.rs        # Always compiles (basic smoke tests)
├── integration_tests.rs  # #[cfg(feature = "test-messaging")]
├── e2e_tests.rs         # #[cfg(feature = "test-services")]
├── common/              # Always compiled (test utilities)
├── basics/              # Always compiled
├── integration/         # test-messaging
└── e2e/
    ├── multi_instance/  # test-cluster (LOCAL ONLY)
    ├── rust/           # test-services
    ├── python/         # test-services
    ├── ruby/           # test-services
    └── typescript/     # test-services
```

### Test File Annotations

```rust
// tests/integration_tests.rs
#![cfg(feature = "test-messaging")]
mod common;
mod integration;

// tests/e2e_tests.rs
#![cfg(feature = "test-services")]
mod common;
mod e2e;

// tests/e2e/multi_instance/mod.rs
#![cfg(feature = "test-cluster")]
mod concurrent_task_creation_test;
mod consistency_test;
```

### cargo-make Integration

```toml
# Makefile.toml

# Default test-rust now uses explicit features
[tasks.test-rust]
description = "Run all Rust tests (requires DB + messaging + services)"
script = '''
#!/bin/bash
set -a
source .env 2>/dev/null || true
set +a
cargo nextest run --workspace --features test-services
'''

# Unit tests only (crate-level, requires DB + messaging)
[tasks.test-rust-unit]
description = "Run unit tests (requires DB + messaging)"
script = '''
cargo nextest run --workspace --features test-messaging \
  -E 'not binary(e2e_tests)'
'''

# E2E tests (requires services running)
[tasks.test-rust-e2e]
description = "Run E2E tests (requires services)"
script = '''
#!/bin/bash
set -a
source .env
set +a
cargo nextest run --workspace --features test-services \
  -E 'binary(e2e_tests) | binary(integration_tests)'
'''

# Cluster tests (requires multi-instance cluster - LOCAL ONLY)
[tasks.test-rust-cluster]
description = "Run cluster tests (requires: cargo make cluster-start)"
script = '''
#!/bin/bash
set -a
source .env
set +a
echo "⚠️  Cluster tests require: cargo make cluster-start"
echo "⚠️  These tests are NOT run in CI due to resource constraints"
cargo nextest run --workspace --features test-cluster \
  -E 'test(multi_instance)'
'''

# All tests including cluster (local development only)
[tasks.test-rust-all]
description = "Run ALL tests including cluster (local only)"
script = '''
cargo nextest run --workspace --all-features
'''

# Shortcuts
[tasks.t]
alias = "test-rust"

[tasks.tu]
alias = "test-rust-unit"

[tasks.te]
alias = "test-rust-e2e"

[tasks.tc]
alias = "test-rust-cluster"
```

## CI Configuration

### Current Approach (Unchanged)

CI already correctly handles test levels by using explicit package/binary selection:

```yaml
# Unit tests - packages only, no integration test binaries
cargo nextest run --profile ci --lib \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker

# E2E tests - all test binaries
cargo nextest run --profile ci --test '*'
```

### Cluster Tests in CI

**Cluster tests are explicitly excluded from CI** due to GitHub Actions resource limitations:
- Running multiple orchestration instances + multiple workers requires more memory than free GHA runners provide
- This is a conscious tradeoff for an open-source, pre-alpha project

**Future consideration**: When project reaches alpha/beta, could add cluster testing via:
- Self-hosted runners with more resources
- Paid GHA larger runners
- Separate manual workflow trigger for cluster tests

### Documentation in CI

Add comment to workflow file:

```yaml
# NOTE: Cluster tests (test-cluster feature) are NOT run in CI.
# These tests require multi-instance infrastructure that exceeds
# GitHub Actions free tier resources. Run locally with:
#   cargo make cluster-start
#   cargo make test-rust-cluster
```

## Implementation Plan

### Phase 1: Add Feature Flags to Cargo.toml

1. Add `test-db`, `test-messaging`, `test-services`, `test-cluster` features to root Cargo.toml
2. Add same features to crate-level Cargo.toml files for consistency
3. Features are cumulative (each implies the previous)

### Phase 2: Add Feature Gates to Test Entry Points

1. Add `#![cfg(feature = "test-messaging")]` to `tests/integration_tests.rs`
2. Add `#![cfg(feature = "test-services")]` to `tests/e2e_tests.rs`
3. Add `#![cfg(feature = "test-cluster")]` to `tests/e2e/multi_instance/mod.rs`
4. Remove `#[ignore]` from multi_instance tests (feature gate replaces it)

### Phase 3: Update cargo-make Tasks

1. Update `test-rust` to use `--features test-services`
2. Add `test-rust-cluster` task with `--features test-cluster`
3. Keep `test-rust-all` using `--all-features`
4. Add shortcuts (`t`, `tu`, `te`, `tc`)

### Phase 4: Update Documentation

1. Update CLAUDE.md with new test commands
2. Add comment to CI workflow explaining cluster test exclusion
3. Update any test-related documentation

### Phase 5: Verify and Test

1. Verify CI still passes (no changes to CI workflows needed)
2. Test locally that feature flags work correctly
3. Verify `--all-features` includes cluster tests
4. Document in release notes

## Summary

| Test Level | Feature Flag | Infrastructure | In CI? | Command |
|------------|-------------|----------------|--------|---------|
| Crate unit | `test-messaging` | DB + messaging | ✅ | `cargo make test-rust-unit` |
| Integration | `test-messaging` | DB + messaging | ✅ | (part of test-rust) |
| E2E | `test-services` | Services running | ✅ | `cargo make test-rust-e2e` |
| Cluster | `test-cluster` | Multi-instance | ❌ | `cargo make test-rust-cluster` |

**Key principle**: `--all-features` = truly all features. If you run it and cluster isn't running, cluster tests will fail - that's expected behavior, not a bug.

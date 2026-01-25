# Cluster Testing Guide

**Last Updated**: 2026-01-19
**Audience**: Developers, QA
**Status**: Active
**Related**: [Tooling](../development/tooling.md) | [Idempotency and Atomicity](../architecture/idempotency-and-atomicity.md)

---

## Overview

This guide covers multi-instance cluster testing for validating horizontal scaling, race condition detection, and concurrent processing scenarios. The cluster testing infrastructure was developed as part of TAS-73 (Resiliency and Redundancy).

**Key Capabilities**:
- Run N orchestration instances with M worker instances
- Test concurrent task creation across instances
- Validate state consistency across cluster
- Detect race conditions and data corruption
- Measure performance under concurrent load

---

## Test Infrastructure

### Feature Flags

Tests are organized by infrastructure requirements using Cargo feature flags:

| Feature Flag | Infrastructure Required | In CI? |
|--------------|------------------------|--------|
| `test-db` | PostgreSQL database | Yes |
| `test-messaging` | DB + messaging (PGMQ/RabbitMQ) | Yes |
| `test-services` | DB + messaging + services running | Yes |
| `test-cluster` | Multi-instance cluster running | **No** |

**Hierarchy**: Each flag implies the previous (`test-cluster` includes `test-services` includes `test-messaging` includes `test-db`).

### Test Commands

```bash
# Unit tests (DB + messaging only)
cargo make test-rust-unit

# E2E tests (services running)
cargo make test-rust-e2e

# Cluster tests (cluster running - LOCAL ONLY)
cargo make test-rust-cluster

# All tests including cluster
cargo make test-rust-all
```

### Test Entry Points

```
tests/
├── basic_tests.rs        # Always compiles
├── integration_tests.rs  # #[cfg(feature = "test-messaging")]
├── e2e_tests.rs         # #[cfg(feature = "test-services")]
└── e2e/
    └── multi_instance/   # #[cfg(feature = "test-cluster")]
        ├── mod.rs
        ├── concurrent_task_creation_test.rs
        └── consistency_test.rs
```

---

## Multi-Instance Test Manager

The `MultiInstanceTestManager` provides high-level APIs for cluster testing.

### Location

```
tests/common/multi_instance_test_manager.rs
tests/common/orchestration_cluster.rs
```

### Basic Usage

```rust
use crate::common::multi_instance_test_manager::MultiInstanceTestManager;

#[tokio::test]
#[cfg(feature = "test-cluster")]
async fn test_concurrent_operations() -> Result<()> {
    // Setup from environment (reads TASKER_TEST_ORCHESTRATION_URLS, etc.)
    let manager = MultiInstanceTestManager::setup_from_env().await?;

    // Wait for all instances to become healthy
    manager.wait_for_healthy(Duration::from_secs(30)).await?;

    // Create tasks concurrently across the cluster
    let requests = vec![create_task_request("namespace", "task", json!({})); 10];
    let responses = manager.create_tasks_concurrent(requests).await?;

    // Wait for completion
    let task_uuids: Vec<_> = responses.iter()
        .map(|r| uuid::Uuid::parse_str(&r.task_uuid).unwrap())
        .collect();
    let completed = manager.wait_for_tasks_completion(task_uuids.clone(), timeout).await?;

    // Verify consistency across all instances
    for uuid in &task_uuids {
        manager.verify_task_consistency(*uuid).await?;
    }

    Ok(())
}
```

### Key Methods

| Method | Description |
|--------|-------------|
| `setup_from_env()` | Create manager from environment variables |
| `setup(orch_count, worker_count)` | Create manager with explicit counts |
| `wait_for_healthy(timeout)` | Wait for all instances to be healthy |
| `create_tasks_concurrent(requests)` | Create tasks using round-robin distribution |
| `wait_for_task_completion(uuid, timeout)` | Wait for single task completion |
| `wait_for_tasks_completion(uuids, timeout)` | Wait for multiple tasks |
| `verify_task_consistency(uuid)` | Verify task state across all instances |
| `orchestration_count()` | Number of orchestration instances |
| `worker_count()` | Number of worker instances |

### OrchestrationCluster

Lower-level cluster abstraction with load balancing:

```rust
use crate::common::orchestration_cluster::{OrchestrationCluster, ClusterConfig, LoadBalancingStrategy};

// Create cluster with round-robin load balancing
let config = ClusterConfig {
    orchestration_urls: vec!["http://localhost:8080", "http://localhost:8081"],
    worker_urls: vec!["http://localhost:8100", "http://localhost:8101"],
    load_balancing: LoadBalancingStrategy::RoundRobin,
    health_timeout: Duration::from_secs(5),
};
let cluster = OrchestrationCluster::new(config).await?;

// Get client using load balancing strategy
let client = cluster.get_client();

// Get all clients for parallel operations
for client in cluster.all_clients() {
    let task = client.get_task(task_uuid).await?;
}
```

---

## Running Cluster Tests

### Prerequisites

1. **PostgreSQL running** with PGMQ extension
2. **Environment configured** for cluster mode

### Step-by-Step

```bash
# 1. Start PostgreSQL (if not already running)
cargo make docker-up

# 2. Setup cluster environment
cargo make setup-env-cluster

# 3. Start the full cluster
cargo make cluster-start-all

# 4. Verify cluster health
cargo make cluster-status

# Expected output:
# Instance Status:
# ─────────────────────────────────────────────────────────────
# INSTANCE              STATUS     PID        PORT
# ─────────────────────────────────────────────────────────────
# orchestration-1       healthy    12345      8080
# orchestration-2       healthy    12346      8081
# worker-rust-1         healthy    12347      8100
# worker-rust-2         healthy    12348      8101
# ... (more workers)

# 5. Run cluster tests
cargo make test-rust-cluster

# 6. Stop cluster when done
cargo make cluster-stop
```

### Monitoring During Tests

```bash
# In separate terminal: Watch cluster logs
cargo make cluster-logs

# Or orchestration logs only
cargo make cluster-logs-orchestration

# Quick status check (no health probes)
cargo make cluster-status-quick
```

---

## Test Scenarios

### Concurrent Task Creation

Validates that tasks can be created concurrently across orchestration instances without conflicts.

**File**: `tests/e2e/multi_instance/concurrent_task_creation_test.rs`

**Test**: `test_concurrent_task_creation_across_instances`

**Validates**:
1. Tasks created through different orchestration instances
2. All tasks complete successfully
3. State is consistent across all instances
4. No duplicate UUIDs generated

### Rapid Task Burst

Stress tests the system by creating many tasks in quick succession.

**Test**: `test_rapid_task_creation_burst`

**Validates**:
1. System handles high task creation rate
2. No duplicate task UUIDs
3. All tasks created successfully

### Round-Robin Distribution

Verifies tasks are distributed across instances using round-robin.

**Test**: `test_task_creation_round_robin_distribution`

**Validates**:
1. Tasks distributed across instances
2. Distribution is approximately even
3. No single-instance bottleneck

---

## Validation Results (TAS-73)

The cluster testing infrastructure was validated with the following results:

### Test Summary

| Metric | Result |
|--------|--------|
| Tests Passed | 1645 |
| Intermittent Failures | 3 (resource contention, not race conditions) |
| Tests Skipped | 21 (domain event tests, require single-instance) |
| Cluster Configuration | 2x orchestration + 2x each worker type (10 total) |

### Key Findings

1. **No Race Conditions Detected**: All concurrent operations completed without data corruption or invalid states

2. **Defense-in-Depth Validated**: Four protection layers (database atomicity, state machine guards, transaction boundaries, application logic) work correctly together

3. **Recovery Mechanism Works**: Tasks and steps recover correctly after simulated failures

4. **Consistent State**: Task state is consistent when queried from any orchestration instance

### Connection Pool Deadlock (Fixed)

Initial testing revealed intermittent failures under high parallelization:

- **Cause**: Connection pool deadlock in task initialization - transactions held connections while template loading needed additional connections
- **Root Cause Fix**: Moved template loading BEFORE transaction begins in `task_initialization/service.rs`
- **Additional Tuning**: Increased pool sizes (20→30 max, 1→2 min connections)
- **Status**: ✅ Fixed - all 9 cluster tests now pass in parallel

See [Connection Pool Deadlock Pattern](../ticket-specs/TAS-73/connection-pool-deadlock-pattern.md) for details.

### Domain Event Tests

21 tests were skipped in cluster mode (marked with `#[cfg(not(feature = "test-cluster"))]`):

- **Reason**: Domain event tests verify in-process event delivery, incompatible with multi-process cluster
- **Status**: Working as designed - these tests run in single-instance CI

---

## Test Feature Flag Implementation

### Adding the Feature Gate

Tests requiring cluster infrastructure should use the feature gate:

```rust
// At module level
#![cfg(feature = "test-cluster")]

// Or at test level
#[tokio::test]
#[cfg(feature = "test-cluster")]
async fn test_cluster_specific_behavior() -> Result<()> {
    // ...
}
```

### Skipping Tests in Cluster Mode

Some tests (like domain events) don't work in cluster mode:

```rust
// Only run when NOT in cluster mode
#[tokio::test]
#[cfg(not(feature = "test-cluster"))]
async fn test_domain_event_delivery() -> Result<()> {
    // In-process event testing
}
```

### Conditional Imports

```rust
// Only import cluster test utilities when needed
#[cfg(feature = "test-cluster")]
use crate::common::multi_instance_test_manager::MultiInstanceTestManager;
```

---

## Nextest Configuration

The `.config/nextest.toml` configures test execution for cluster scenarios:

```toml
[profile.default]
retries = 0
leak-timeout = { period = "500ms", result = "fail" }
fail-fast = false

# Multi-instance tests can run in parallel once cluster is warmed up
[[profile.default.overrides]]
filter = 'test(multi_instance)'

[profile.ci]
# Limit parallelism to avoid database connection pool exhaustion
test-threads = 4
```

**Cluster Warmup**: Multi-instance tests can run in parallel. Connection pools now start with `min_connections=2` for faster warmup. The 5-second delay built into `cluster-start-all` usually suffices. If you see "Failed to create task after all retries" errors immediately after startup, wait a few more seconds for pools to fully initialize.

---

## Troubleshooting

### Cluster Won't Start

```bash
# Check for port conflicts
lsof -i :8080-8089
lsof -i :8100-8109

# Check for stale PID files
ls -la .pids/
rm -rf .pids/*.pid  # Clean up stale PIDs

# Retry start
cargo make cluster-start-all
```

### Tests Timeout / "Failed to create task after all retries"

This typically indicates the cluster wasn't fully warmed up:

```bash
# Check cluster health
cargo make cluster-status

# If health is green but tests fail, wait for warmup
sleep 10 && cargo make test-rust-cluster

# Check logs for errors
cargo make cluster-logs | head -100

# Restart cluster with extra warmup
cargo make cluster-stop
cargo make cluster-start-all
sleep 10
cargo make test-rust-cluster
```

**Root cause**: Connection pools start at `min_connections=2` and grow on demand. The first requests after startup may timeout while pools are establishing connections.

### Connection Pool Exhaustion

If tests fail with "pool timed out" errors, the issue was likely fixed in TAS-73. Ensure you have the latest code with:
- Template loading before transaction in `task_initialization/service.rs`
- Pool sizes: `max_connections=30`, `min_connections=2` in test config

If issues persist, verify pool configuration:
```bash
# Check test config
cat config/tasker/generated/orchestration-test.toml | grep -A5 "pool"
```

### Environment Variables Not Set

```bash
# Verify environment
env | grep TASKER_TEST

# Re-source environment
source .env

# Or regenerate
cargo make setup-env-cluster
```

---

## CI Considerations

**Cluster tests are NOT run in CI** due to GitHub Actions resource constraints:

- Running multiple orchestration + worker instances requires more memory than free GHA runners provide
- This is a conscious tradeoff for an open-source, pre-alpha project

**Future Options** (when project matures):
- Self-hosted runners with more resources
- Paid GHA larger runners
- Separate manual workflow trigger for cluster tests

**Workaround**: Run cluster tests locally before PRs that touch concurrent processing code.

---

## Related Documentation

- [Tooling](../development/tooling.md) - Cluster deployment tasks
- [Idempotency and Atomicity](../architecture/idempotency-and-atomicity.md) - Protection mechanisms
- [TAS-73 Connection Pool Deadlock Pattern](../ticket-specs/TAS-73/connection-pool-deadlock-pattern.md) - Transaction patterns
- [TAS-73 Multi-Instance Deployment](../ticket-specs/TAS-73/multi-instance-deployment.md) - Design spec
- [TAS-73 Test Feature Flags](../ticket-specs/TAS-73/test-feature-flags-design.md) - Feature flag design
- [TAS-73 Research Findings](../ticket-specs/TAS-73/research-findings.md) - Research results

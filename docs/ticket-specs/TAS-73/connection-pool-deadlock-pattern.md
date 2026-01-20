# Connection Pool Deadlock Pattern

**Date:** 2026-01-19
**Status:** Documented Pattern with Fix Applied
**Branch:** `jcoletaylor/tas-73-resiliency-and-redundancy-ensuring-atomicity-and-idempotency`

---

## Overview

During TAS-73 cluster testing, we discovered a **connection pool deadlock pattern** that occurred under concurrent load. This document describes the anti-pattern, the fix applied, and establishes guidelines for preventing similar issues.

---

## The Anti-Pattern

### Problem Description

When a database transaction is started but additional read operations are performed that require acquiring a **new connection from the same pool**, a deadlock can occur under high concurrency:

```rust
// BAD: Transaction holds connection while reads need another
pub async fn create_task(&self, request: TaskRequest) -> Result<Task> {
    // Start transaction - holds connection #1
    let mut tx = self.pool.begin().await?;

    // Template loading needs connection #2 from same pool!
    let template = self.template_loader.load_template(&request).await?;

    // ... writes using tx ...
    tx.commit().await?;
}
```

### Deadlock Scenario

Under high concurrency (e.g., 25+ concurrent task creations):

```
T0: Request A starts transaction → holds connection 1
T0: Request B starts transaction → holds connection 2
T0: Request C starts transaction → holds connection 3
... (connections 4-N acquired by other requests)

T1: Request A needs another connection for template loading → WAITS
T1: Request B needs another connection for template loading → WAITS
T1: Request C needs another connection for template loading → WAITS
... all requests waiting for connections that are held by transactions

RESULT: Pool exhausted, all requests timeout waiting for connections
```

### Error Message

```
pool timed out while waiting for an open connection
```

This error appeared during `load_task_template()` when the system was under concurrent load.

---

## The Fix

### Correct Pattern

Move read-only operations **before** starting the transaction:

```rust
// GOOD: Reads complete before transaction starts
pub async fn create_task(&self, request: TaskRequest) -> Result<Task> {
    // Template loading releases connection after read completes
    let template = self.template_loader.load_template(&request).await?;

    // Now start transaction - single connection needed for writes
    let mut tx = self.pool.begin().await?;

    // ... writes using tx ...
    tx.commit().await?;
}
```

### Applied Fix

**File:** `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs`

**Commit:** Lines 112-155

```rust
// IMPORTANT: Load task template BEFORE starting the transaction.
// Template loading requires database queries. If we start the transaction first,
// we hold a connection while trying to acquire another for template loading.
// Under high concurrency, this causes connection pool deadlock:
// - N transactions hold N connections
// - Each needs 1 more connection for template loading
// - Pool exhausted → deadlock
// By loading templates first, we release the connection before starting the transaction.
let task_template = match self
    .template_loader
    .load_task_template(&task_request_for_handler)
    .await
{
    Ok(template) => template,
    Err(e) => return Err(...),
};

// Use SQLx transaction for atomicity (now safe - template already loaded)
let mut tx = self.context.database_pool().begin().await.map_err(...)?;
```

### Pool Configuration Tuning

Additionally, pool sizes were increased for test environment to handle burst loads:

**File:** `config/tasker/environments/test/common.toml`

```toml
[common.database.pool]
max_connections = 30      # Was: 20
min_connections = 2       # Was: 1 (faster warmup)
acquire_timeout_seconds = 30  # Was: 10 (more buffer for bursts)
```

---

## Guidelines

### Rule 1: Load Before Lock

Always complete read-only operations **before** starting a transaction:

| Operation Type | Before Transaction | Inside Transaction |
|---------------|-------------------|-------------------|
| Template loading | ✅ Yes | ❌ No |
| Configuration fetching | ✅ Yes | ❌ No |
| Validation queries | ✅ Yes | ❌ No |
| Existence checks (read-only) | ✅ Yes | Depends* |
| State transitions | ❌ No | ✅ Yes |
| Record creation | ❌ No | ✅ Yes |
| Atomic updates | ❌ No | ✅ Yes |

*Existence checks should be inside transaction only if they're part of find-or-create pattern where atomicity matters.

### Rule 2: Pass Transaction References

When transaction-scoped operations call other functions, pass `&mut Transaction` rather than acquiring new connections:

```rust
// GOOD: Receives transaction reference
async fn create_steps_in_tx(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_uuid: Uuid,
) -> Result<()> {
    sqlx::query!(...).execute(&mut **tx).await?;
    Ok(())
}

// BAD: Acquires its own connection inside transaction scope
async fn create_steps_bad(&self, task_uuid: Uuid) -> Result<()> {
    sqlx::query!(...).execute(self.pool).await?; // New connection!
    Ok(())
}
```

### Rule 3: Document Transaction Boundaries

When a function starts a transaction, document what operations occur inside:

```rust
/// Create task with atomic step initialization.
///
/// # Transaction Scope
/// - Task record creation
/// - Workflow step creation
/// - Step dependency linking
/// - Initial state transitions
///
/// # Pre-Transaction (no lock held)
/// - Template loading
/// - Named task resolution
pub async fn create_task_from_request(&self, request: TaskRequest) -> Result<Task> {
    // ...
}
```

### Rule 4: Use SELECT FOR UPDATE Intentionally

When using `SELECT ... FOR UPDATE` for row-level locking, keep the locked scope minimal:

```rust
// GOOD: Lock only what's needed, commit quickly
let mut tx = pool.begin().await?;
sqlx::query!("SELECT task_uuid FROM tasks WHERE task_uuid = $1 FOR UPDATE", id)
    .fetch_one(&mut *tx).await?;
// Minimal operations while holding lock
sqlx::query!("UPDATE tasks SET state = $1 WHERE task_uuid = $2", new_state, id)
    .execute(&mut *tx).await?;
tx.commit().await?;

// BAD: Holding lock while doing expensive operations
let mut tx = pool.begin().await?;
sqlx::query!("SELECT ... FOR UPDATE").fetch_one(&mut *tx).await?;
expensive_computation().await; // Lock held during computation!
external_service_call().await; // Lock held during I/O!
tx.commit().await?;
```

---

## Codebase Audit Results

A systematic audit of all SQLx transaction usage found **no other instances** of the anti-pattern. The codebase demonstrates proper transaction management:

### Files Audited

| File | Risk | Status |
|------|------|--------|
| task_initialization/service.rs | LOW | ✅ Fixed (this document) |
| task_finalization/completion_handler.rs | LOW | ✅ Proper FOR UPDATE usage |
| workflow_step_creator.rs | LOW | ✅ All writes in transaction |
| task_transition.rs | LOW | ✅ Atomic sort key generation |
| workflow_step_transition.rs | LOW | ✅ Same pattern as task_transition |
| state_initializer.rs | LOW | ✅ Receives transaction from caller |
| batch_processing/service.rs | LOW | ✅ Idempotency check in transaction |
| decision_point/service.rs | LOW | ✅ Template loads before transaction |
| backoff_calculator.rs | LOW | ✅ Proper row-level locking |

### Potential Future Improvements

While no critical issues were found, the following areas could benefit from optimization under extreme load:

1. **Message Handler Chain** (`message_handler.rs`): Multiple sequential database lookups could be batched
2. **Task Coordinator** (`task_coordinator.rs`): State machine chains could cache task context
3. **State Handlers** (`state_handlers.rs`): Redundant task lookups could be eliminated

These are performance optimizations, not correctness issues.

---

## Testing Validation

After applying the fix, all cluster tests pass:

```
cargo make test-rust-cluster

Summary [5.451s] 9 tests run: 9 passed, 1660 skipped
```

Including:
- `test_rapid_task_creation_burst` - 25 concurrent task creations
- `test_concurrent_task_creation_across_instances` - 10 tasks across 2 orchestrators
- `test_eventual_consistency_after_concurrent_operations` - 10 concurrent operations

---

## Related Documentation

- [Idempotency and Atomicity](../../architecture/idempotency-and-atomicity.md)
- [Cluster Testing Guide](../../testing/cluster-testing-guide.md)
- [Atomic Finalization Design](./atomic-finalization-design.md)
- [Research Findings](./research-findings.md)

---

## Appendix: Connection Pool Math

For test environment with `postgres max_connections=500`:

| Component | Instances | Max Connections | Total |
|-----------|-----------|-----------------|-------|
| Orchestration | 2 | 30 | 60 |
| Worker (Rust) | 2 | 30 | 60 |
| Worker (Ruby) | 2 | 30 | 60 |
| Worker (Python) | 2 | 30 | 60 |
| Worker (TypeScript) | 2 | 30 | 60 |
| **Total** | 10 | - | **300** |
| **Headroom** | - | - | **200** |

The increased pool sizes (20→30) provide adequate connections for burst loads while leaving 200 connections as headroom for test parallelism and ad-hoc queries.

# TAS-73: Task Identity Hash Strategy

**Status:** Design Required
**Priority:** P1 (prerequisite for multi-instance testing)
**Related:** TAS-73 research findings, idempotency-and-atomicity.md

---

## Problem Statement

### Current State

The `identity_hash` column on `tasker.tasks` is computed from `hash(named_task_uuid, context)` and is intended to provide task deduplication. However:

1. **Missing constraint**: The column has only a regular B-tree index, not a UNIQUE constraint (regression from schema migration)
2. **One-size-fits-all**: The current approach assumes all domains want strict idempotency based on full context
3. **Domain mismatch**: Different use cases have different identity semantics

### The Nuance

Task identity is domain-specific:

| Use Case | Same Template + Same Context | Desired Behavior |
|----------|------------------------------|------------------|
| Payment processing | Likely accidental duplicate | **Deduplicate** (safety) |
| Nightly batch job | Intentional repetition | **Allow** (operational) |
| Report generation | Could be either | **Configurable** |
| Event-driven triggers | Often intentional | **Allow** |
| Retry with same params | Intentional | **Allow** |

A TaskRequest with identical context might be:
- An accidental duplicate (network retry, user double-click) → should deduplicate
- An intentional repetition (scheduled job, legitimate re-run) → should allow

---

## Proposed Solution

### Identity Strategy Pattern

Allow named tasks to define their identity strategy, with per-request override capability.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Task Identity Strategies                      │
├─────────────────────────────────────────────────────────────────┤
│ STRICT (default)                                                 │
│   identity_hash = hash(named_task_uuid, context)                 │
│   → Same request = same task (full idempotency)                  │
│                                                                  │
│ CALLER_PROVIDED                                                  │
│   identity_hash = request.idempotency_key                        │
│   → Caller controls uniqueness (like Stripe's Idempotency-Key)   │
│                                                                  │
│ ALWAYS_UNIQUE                                                    │
│   identity_hash = uuidv7()                                       │
│   → Every request creates new task (no deduplication)            │
│                                                                  │
│ CONTEXTUAL (future)                                              │
│   identity_hash = hash(named_task_uuid, context[identity_keys])  │
│   → Named task defines which context fields matter for identity  │
└─────────────────────────────────────────────────────────────────┘
```

### Configuration Hierarchy

1. **Named task defines default strategy** (declared in task template)
2. **Per-request override** via optional `idempotency_key` field
3. **Global fallback** is STRICT (safe by default)

### API Changes

#### TaskRequest Enhancement

```rust
pub struct TaskRequest {
    // ... existing fields ...

    /// Optional caller-provided idempotency key.
    /// - If provided: used as identity_hash (CALLER_PROVIDED strategy)
    /// - If not provided: named task's default strategy applies
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}
```

#### Named Task Enhancement

```rust
pub enum IdentityStrategy {
    /// Hash of (named_task_uuid, context) - strict idempotency
    Strict,
    /// Caller must provide idempotency_key, reject if missing
    CallerProvided,
    /// Always generate unique identity (uuidv7)
    AlwaysUnique,
}

// In NamedTask or task template configuration
pub struct NamedTaskConfig {
    // ... existing fields ...

    /// How task identity is determined for deduplication
    #[serde(default)]
    pub identity_strategy: IdentityStrategy,  // Default: Strict
}
```

#### API Response Enhancement

```rust
pub struct TaskResponse {
    pub task: Task,
    /// Whether this request created a new task or returned existing
    pub created: bool,
    /// If deduplicated, the original creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicated_from: Option<DateTime<Utc>>,
}
```

### Database Changes

1. **Add UNIQUE constraint** to `identity_hash` in existing migration file (`20260110000002_constraints_and_indexes.sql`)
2. **Add column** to `named_tasks` table: `identity_strategy VARCHAR(20) DEFAULT 'strict'`
3. **Add enum type** (optional): `CREATE TYPE tasker.identity_strategy AS ENUM ('strict', 'caller_provided', 'always_unique')`

### Identity Hash Computation

```rust
impl Task {
    pub fn compute_identity_hash(
        named_task: &NamedTask,
        context: &serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> String {
        // Per-request override takes precedence
        if let Some(key) = idempotency_key {
            return hash_string(key);
        }

        // Apply named task's strategy
        match named_task.identity_strategy {
            IdentityStrategy::Strict => {
                // Current behavior: hash(named_task_uuid, context)
                Self::generate_identity_hash(named_task.named_task_uuid, context)
            }
            IdentityStrategy::CallerProvided => {
                // Reject - caller must provide key
                panic!("idempotency_key required for CallerProvided strategy");
            }
            IdentityStrategy::AlwaysUnique => {
                // Generate unique hash
                Uuid::now_v7().to_string()
            }
        }
    }
}
```

---

## Implementation Plan

### Phase 1: Database Constraint (Immediate)

1. Update `migrations/20260110000002_constraints_and_indexes.sql`:
   - Change `CREATE INDEX idx_tasks_identity_hash` to `CREATE UNIQUE INDEX`
2. Update documentation to match implementation

### Phase 2: Strategy Infrastructure

1. Add `IdentityStrategy` enum to `tasker-shared`
2. Add `identity_strategy` column to `named_tasks` table
3. Update `Task::compute_identity_hash()` with strategy support
4. Update task creation endpoint to use strategy

### Phase 3: API Enhancement

1. Add `idempotency_key` to `TaskRequest`
2. Add `created` and `deduplicated_from` to response
3. Update API documentation

### Phase 4: Testing

1. Unit tests for each strategy
2. Integration tests for deduplication behavior
3. Multi-instance tests for concurrent identical requests

---

## Edge Cases

### CallerProvided without key
- **Behavior**: Return 400 Bad Request with clear error message
- **Rationale**: Named task explicitly requires caller to manage idempotency

### Key collision across named tasks
- **Current**: `identity_hash` is globally unique (could collide)
- **Option A**: Include `named_task_uuid` in hash even for caller-provided keys
- **Option B**: Document that keys should be globally unique
- **Recommendation**: Option A (safer, prevents cross-task collisions)

### Migration of existing tasks
- **No migration needed**: Existing tasks have valid identity_hash values
- **Existing named tasks**: Default to STRICT (current behavior)

---

## Open Questions

1. **Should CONTEXTUAL strategy be included in initial implementation?**
   - Adds complexity (need to define which fields)
   - Could be Phase 2 if there's demand

2. **Should we support time-bounded idempotency?**
   - "Same key within 24 hours = dedupe, after that = new task"
   - Common in payment systems
   - Adds complexity, defer for now

3. **What HTTP status for deduplicated requests?**
   - 200 OK with `created: false` (current implicit behavior)
   - 409 Conflict (some APIs do this)
   - **Recommendation**: 200 OK with clear response fields

---

## References

- `docs/architecture/idempotency-and-atomicity.md` - Current idempotency documentation
- `tasker-shared/src/models/core/task.rs:863-876` - Current `generate_identity_hash()`
- Stripe Idempotency: https://stripe.com/docs/api/idempotent_requests
- AWS ClientToken pattern

---

## Action Items

- [ ] Add UNIQUE constraint to existing migration file
- [ ] Update idempotency-and-atomicity.md to reflect actual implementation
- [ ] Create sub-ticket for strategy implementation (post-TAS-73)

# TAS-154: Task Identity Strategy Pattern

**Status:** Implemented
**Priority:** P1 (feature enhancement for configurable deduplication)
**Parent:** Identified during TAS-73 resiliency research
**Related:** `docs/ticket-specs/TAS-73/research-findings.md`, `docs/architecture/idempotency-and-atomicity.md`

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

### Phase 4: Testing & Validation

1. Unit tests for each strategy
2. Integration tests for deduplication behavior
3. **Thundering herd test** (critical validation):
   - Submit N=50 identical tasks simultaneously from multiple clients
   - Verify STRICT strategy deduplicates to exactly 1 task
   - Verify ALWAYS_UNIQUE creates N separate tasks
   - Verify CALLER_PROVIDED with same key deduplicates correctly
4. Multi-instance tests for concurrent identical requests across orchestrators

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

## Double Submission & Time-Bounded Deduplication

### Design Decision: User-Space Responsibility

After analysis, we've decided **not** to implement built-in time-bounded idempotency. Instead, callers manage temporal deduplication through explicit context or idempotency keys.

#### Rationale

1. **Implicit time windows are hard to reason about**
   - Bucket boundary edge cases (23:59:59 vs 00:00:01)
   - Debugging becomes difficult when deduplication depends on hidden time state

2. **It conflates two concerns**
   - **Deduplication**: Preventing accidental duplicates
   - **Scheduling**: Allowing intentional repetition
   - These are different problems with different solutions

3. **Clean solutions already exist**
   - CALLER_PROVIDED with time-aware keys
   - STRICT with time-aware context fields
   - ALWAYS_UNIQUE for no deduplication

4. **Explicit is better than implicit**
   - Callers understand their domain's "sameness" semantics
   - Tasker provides clean primitives; callers compose them

### Double Submission Scenarios

| Scenario | Time Gap | Same Context? | Recommended Strategy |
|----------|----------|---------------|---------------------|
| Network retry | ms-seconds | Yes | STRICT (auto-dedupes) |
| User double-click | seconds | Yes | STRICT (auto-dedupes) |
| Webhook replay | seconds-minutes | Yes | STRICT (auto-dedupes) |
| Scheduled job re-run | minutes-hours | Yes | CALLER_PROVIDED or time-aware context |
| Daily batch (same params) | 24h+ | Yes | Include date in context |
| Event re-trigger | varies | Yes | Domain-specific decision |

### Recommended Patterns

#### Pattern 1: CALLER_PROVIDED with Time-Bucketed Keys

For callers who need deduplication within a window but allow repetition across windows:

```rust
// Dedupe within same hour, allow across hours
let hour_bucket = chrono::Utc::now().format("%Y-%m-%d-%H");
let idempotency_key = format!("{}-{}-{}", job_name, customer_id, hour_bucket);

TaskRequest {
    named_task_name: "generate-report".to_string(),
    context: json!({ "customer_id": 12345 }),
    idempotency_key: Some(idempotency_key),
    ..Default::default()
}
```

#### Pattern 2: STRICT with Time-Aware Context

Include scheduling context directly in the request:

```rust
TaskRequest {
    named_task_name: "daily-reconciliation".to_string(),
    context: json!({
        "account_id": "ACC-001",
        "run_date": "2026-01-20",      // Changes daily
        "run_window": "morning"         // Optional: finer granularity
    }),
    ..Default::default()
}
```

#### Pattern 3: ALWAYS_UNIQUE for Independent Tasks

When every submission should create a new task:

```rust
// Named task configured with identity_strategy: AlwaysUnique
// Every request creates a new task, no deduplication
TaskRequest {
    named_task_name: "send-notification".to_string(),
    context: json!({ "user_id": 123, "message": "Hello" }),
    ..Default::default()
}
```

### Granularity Guide

| Dedup Window | Key/Context Pattern | Use Case |
|--------------|---------------------|----------|
| Per-minute | `{job}-{YYYY-MM-DD-HH-mm}` | High-frequency event processing |
| Per-hour | `{job}-{YYYY-MM-DD-HH}` | Hourly reports, rate-limited APIs |
| Per-day | `{job}-{YYYY-MM-DD}` | Daily batch jobs, EOD processing |
| Per-week | `{job}-{YYYY-Www}` | Weekly aggregations |
| Per-month | `{job}-{YYYY-MM}` | Monthly billing cycles |

### Anti-Patterns to Avoid

❌ **Don't rely on submission timing for identity**
```rust
// BAD: Hoping requests are "far enough apart"
TaskRequest { context: json!({ "customer_id": 123 }) }
```

❌ **Don't use ALWAYS_UNIQUE when you need deduplication**
```rust
// BAD: Creates duplicate work on network retries
// Named task with AlwaysUnique for payment processing
```

✅ **Do make identity explicit**
```rust
// GOOD: Clear what makes this task unique
TaskRequest {
    context: json!({
        "payment_id": "PAY-123",  // Natural idempotency key
        "amount": 100
    })
}
```

---

## Open Questions

1. **Should CONTEXTUAL strategy be included in initial implementation?**
   - Adds complexity (need to define which fields)
   - Could be Phase 2 if there's demand

2. ~~**Should we support time-bounded idempotency?**~~
   - **RESOLVED**: No. Time-bounded deduplication is a user-space concern.
   - Callers use CALLER_PROVIDED with time-bucketed keys or include time-aware fields in context.
   - See "Double Submission & Time-Bounded Deduplication" section above.

3. ~~**What HTTP status for deduplicated requests?**~~
   - **RESOLVED**: 409 Conflict
   - Returning 200 OK with existing task UUID was rejected as a security risk (enables UUID probing)
   - 409 Conflict clearly indicates the request failed due to deduplication policy
   - See `docs/guides/identity-strategy.md` for full documentation

---

## References

- `docs/architecture/idempotency-and-atomicity.md` - Current idempotency documentation
- `tasker-shared/src/models/core/task.rs:863-876` - Current `generate_identity_hash()`
- Stripe Idempotency: https://stripe.com/docs/api/idempotent_requests
- AWS ClientToken pattern

---

## Action Items

- [x] Add UNIQUE constraint to existing migration file (`20260110000002_constraints_and_indexes.sql`)
- [x] Implement `IdentityStrategy` enum in `tasker-shared`
- [x] Add `identity_strategy` column to `named_tasks` table (new migration)
- [x] Update `Task::compute_identity_hash()` with strategy support
- [x] Add `idempotency_key` to `TaskRequest`
- [x] Return 409 Conflict for duplicate identity (security-conscious alternative to returning existing task)
- [x] Add JSON normalization for consistent hashing (key order, whitespace insensitive)
- [x] Unit tests for identity strategy pattern (27 tests)
- [x] E2E tests for 409 Conflict behavior (3 tests)
- [x] Create `docs/guides/identity-strategy.md` with usage documentation
- [ ] Implement thundering herd test (`tests/integration/concurrency/thundering_herd_test.rs`) - future enhancement

## Success Criteria

TAS-154 will be considered complete when:

1. **UNIQUE constraint enforced**: `identity_hash` has database-level uniqueness
2. **Strategy pattern implemented**: Named tasks can declare STRICT, CALLER_PROVIDED, or ALWAYS_UNIQUE
3. **API enhanced**: `idempotency_key` accepted on TaskRequest, `created` flag in response
4. **Thundering herd validated**: 50 simultaneous identical requests deduplicate correctly
5. **Documentation updated**: Architecture docs reflect actual implementation

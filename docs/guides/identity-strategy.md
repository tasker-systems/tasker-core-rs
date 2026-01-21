# Task Identity Strategy Pattern

**Last Updated**: 2026-01-20
**Audience**: Developers, Operators
**Status**: Active
**Related Docs**: [Documentation Hub](README.md) | [Idempotency and Atomicity](../architecture/idempotency-and-atomicity.md)

← Back to [Documentation Hub](README.md)

---

## Overview

Task identity determines how Tasker deduplicates task creation requests. The **identity strategy pattern** allows named tasks to configure their deduplication behavior based on domain requirements.

When a task creation request arrives, Tasker computes an **identity hash** based on the configured strategy. If a task with that identity hash already exists, the request is rejected with a **409 Conflict** response.

## Why This Matters

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

## Identity Strategies

### STRICT (Default)

```
identity_hash = hash(named_task_uuid, normalized_context)
```

Same named task + same context = same identity hash = deduplicated.

**Use when:**
- Accidental duplicates are a risk (payments, orders, notifications)
- Context fully describes the work to be done
- Network retries or user double-clicks should be safe

**Example:**
```rust
// Payment processing - same payment_id should never create duplicate tasks
TaskRequest {
    namespace: "payments".to_string(),
    name: "process_payment".to_string(),
    context: json!({
        "payment_id": "PAY-12345",
        "amount": 100.00,
        "currency": "USD"
    }),
    idempotency_key: None,  // Uses STRICT strategy
    ..Default::default()
}
```

### CALLER_PROVIDED

```
identity_hash = hash(named_task_uuid, idempotency_key)
```

Caller must provide `idempotency_key`. Request is rejected with **400 Bad Request** if the key is missing.

**Use when:**
- Caller has a natural idempotency key (order_id, transaction_id, request_id)
- Caller needs control over deduplication scope
- Similar to Stripe's Idempotency-Key pattern

**Example:**
```rust
// Order processing - caller controls idempotency with their order ID
TaskRequest {
    namespace: "orders".to_string(),
    name: "fulfill_order".to_string(),
    context: json!({
        "order_id": "ORD-98765",
        "items": [...]
    }),
    idempotency_key: Some("ORD-98765".to_string()),  // Required for CallerProvided
    ..Default::default()
}
```

### ALWAYS_UNIQUE

```
identity_hash = uuidv7()
```

Every request creates a new task. No deduplication.

**Use when:**
- Every submission should create work (notifications, events)
- Repetition is intentional (scheduled jobs, cron-like triggers)
- Context doesn't define uniqueness

**Example:**
```rust
// Notification sending - every call should send a notification
TaskRequest {
    namespace: "notifications".to_string(),
    name: "send_email".to_string(),
    context: json!({
        "user_id": 123,
        "template": "welcome",
        "data": {...}
    }),
    idempotency_key: None,  // ALWAYS_UNIQUE ignores this
    ..Default::default()
}
```

## Configuration

### Named Task Configuration

Set the identity strategy in your task template:

```yaml
# templates/payments/process_payment.yaml
namespace: payments
name: process_payment
version: "1.0.0"
identity_strategy: strict  # strict | caller_provided | always_unique

steps:
  - name: validate_payment
    handler: payment_validator
    # ...
```

### Per-Request Override

The `idempotency_key` field overrides any strategy:

```rust
// Even if named task is ALWAYS_UNIQUE, this key makes it deduplicate
TaskRequest {
    idempotency_key: Some("my-custom-key-12345".to_string()),
    // ... other fields
}
```

**Precedence:**
1. `idempotency_key` (if provided) → always uses hash of key
2. Named task's `identity_strategy` → applies if no key provided
3. Default → STRICT (if strategy not configured)

## API Behavior

### Successful Creation (201 Created)

```json
{
  "task_uuid": "019bddae-b818-7d82-b7c5-bd42e5db27fc",
  "step_count": 4,
  "message": "Task created successfully"
}
```

### Duplicate Identity (409 Conflict)

When a task with the same identity hash exists:

```json
{
  "error": {
    "code": "CONFLICT",
    "message": "A task with this identity already exists. The task's identity strategy prevents duplicate creation."
  }
}
```

**Security Note:** The API returns 409 Conflict rather than the existing task's UUID. This prevents potential data leakage where attackers could probe for existing task UUIDs by submitting requests with guessed contexts.

### Missing Idempotency Key (400 Bad Request)

When `CallerProvided` strategy requires a key:

```json
{
  "error": {
    "code": "BAD_REQUEST",
    "message": "idempotency_key is required when named task uses CallerProvided identity strategy"
  }
}
```

## JSON Normalization

For STRICT strategy, the context JSON is **normalized** before hashing:

- **Key ordering**: Keys are sorted alphabetically (recursively)
- **Whitespace**: Removed for consistency
- **Semantic equivalence**: `{"b":2,"a":1}` and `{"a":1,"b":2}` produce the same hash

This means these two requests produce the **same** identity hash:

```rust
// Request 1
context: json!({"user_id": 123, "action": "create"})

// Request 2 - same content, different key order
context: json!({"action": "create", "user_id": 123})
```

**Note:** Array order is preserved (arrays are ordered by definition).

## Recommended Patterns

### Pattern 1: Time-Bucketed Keys

For deduplication within a time window but allowing repetition across windows:

```rust
// Dedupe within same hour, allow across hours
let hour_bucket = chrono::Utc::now().format("%Y-%m-%d-%H");
let idempotency_key = format!("{}-{}-{}", job_name, customer_id, hour_bucket);

TaskRequest {
    namespace: "reports".to_string(),
    name: "generate_report".to_string(),
    context: json!({ "customer_id": 12345 }),
    idempotency_key: Some(idempotency_key),
    ..Default::default()
}
```

### Pattern 2: Time-Aware Context

Include scheduling context directly in the request:

```rust
TaskRequest {
    namespace: "batch".to_string(),
    name: "daily_reconciliation".to_string(),
    context: json!({
        "account_id": "ACC-001",
        "run_date": "2026-01-20",      // Changes daily
        "run_window": "morning"         // Optional: finer granularity
    }),
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

## Anti-Patterns

### Don't Rely on Timing

```rust
// BAD: Hoping requests are "far enough apart"
TaskRequest { context: json!({ "customer_id": 123 }) }
```

### Don't Use ALWAYS_UNIQUE for Critical Operations

```rust
// BAD: Creates duplicate work on network retries
// Named task with AlwaysUnique for payment processing
```

### Do Make Identity Explicit

```rust
// GOOD: Clear what makes this task unique
TaskRequest {
    context: json!({
        "payment_id": "PAY-123",  // Natural idempotency key
        "amount": 100
    }),
    ..Default::default()
}
```

## Database Implementation

The identity strategy is enforced at the database level:

1. **UNIQUE constraint** on `identity_hash` column prevents duplicates
2. **identity_strategy** column on `named_tasks` stores the configured strategy
3. **Atomic insertion** with constraint violation returns 409 Conflict

```sql
-- Identity hash has unique constraint
CREATE UNIQUE INDEX idx_tasks_identity_hash ON tasker.tasks(identity_hash);

-- Named tasks store their strategy
ALTER TABLE tasker.named_tasks
ADD COLUMN identity_strategy VARCHAR(20) DEFAULT 'strict';
```

## Testing Considerations

When writing tests that create tasks, inject a unique identifier to avoid identity hash collisions:

```rust
// Test utility that ensures unique identity per test run
fn create_task_request(namespace: &str, name: &str, context: Value) -> TaskRequest {
    let mut ctx = context.as_object().cloned().unwrap_or_default();
    ctx.insert("_test_run_id".to_string(), json!(Uuid::now_v7().to_string()));

    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        context: Value::Object(ctx),
        ..Default::default()
    }
}
```

## Summary

| Strategy | Identity Hash | Deduplicates? | Key Required? |
|----------|---------------|---------------|---------------|
| **STRICT** | `hash(uuid, context)` | Yes | No |
| **CALLER_PROVIDED** | `hash(uuid, key)` | Yes | Yes |
| **ALWAYS_UNIQUE** | `uuidv7()` | No | No |

**Choose STRICT** (default) unless you have a specific reason not to. It's the safest option for preventing accidental duplicate task creation.

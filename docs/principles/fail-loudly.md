# Fail Loudly

This document describes Tasker Core's philosophy on error handling: errors are first-class citizens, not inconveniences to hide.

## The Core Principle

> **A system that lies is worse than one that fails.**

When data is missing, malformed, or unexpected, the correct response is an explicit error—not a fabricated default that makes the problem invisible.

---

## The Problem: Phantom Data

"Phantom data" is data that:
- Looks valid to consumers
- Passes type checks and validation
- Contains no actual information from the source
- Was fabricated by defensive code trying to be "helpful"

### Example: The Silent Default

```rust
// WRONG: Silent default hides protocol violation
fn get_pool_utilization(response: Response) -> PoolUtilization {
    response.pool_utilization.unwrap_or_else(|| PoolUtilization {
        active_connections: 0,
        idle_connections: 0,
        max_connections: 0,
        utilization_percent: 0.0,  // Looks like "no load"
    })
}
```

A monitoring system receiving this response sees:
- `utilization_percent: 0.0` — "Great, the system is idle!"
- Reality: The server never sent pool data. The system might be at 100% load.

The consumer cannot distinguish "server reported 0%" from "server sent nothing."

### The Trust Equation

```
Silent default
  → Consumer receives valid-looking data
  → Consumer makes decisions based on phantom values
  → Phantom bugs manifest in production
  → Debugging nightmare: "But the data looked correct!"
```

vs.

```
Explicit error
  → Consumer receives clear failure
  → Consumer handles error appropriately
  → Problem visible immediately
  → Fix applied at source
```

---

## The Solution: Explicit Errors

### Pattern: Required Fields Return Errors

```rust
// RIGHT: Explicit error on missing required data
fn get_pool_utilization(response: Response) -> Result<PoolUtilization, ClientError> {
    response.pool_utilization.ok_or_else(|| {
        ClientError::invalid_response(
            "Response.pool_utilization",
            "Server omitted required pool utilization data",
        )
    })
}
```

Now the consumer:
- Knows data is missing
- Can retry, alert, or degrade gracefully
- Never operates on phantom values

### Pattern: Distinguish Required vs Optional

Not all fields should fail on absence. The distinction matters:

| Field Type | Missing Means | Response |
|------------|---------------|----------|
| **Required** | Protocol violation, server bug | Return error |
| **Optional** | Legitimately absent, feature not configured | Return `None` |

```rust
// Required: Server MUST send health checks
let checks = response.checks.ok_or_else(||
    ClientError::invalid_response("checks", "missing")
)?;

// Optional: Distributed cache may not be configured
let cache = response.distributed_cache; // Option<T> preserved
```

### Pattern: Propagate, Don't Swallow

Errors should flow up, not disappear:

```rust
// WRONG: Error swallowed, default returned
fn convert_response(r: Response) -> DomainType {
    let info = r.info.unwrap_or_default();  // Error hidden
    // ...
}

// RIGHT: Error propagated to caller
fn convert_response(r: Response) -> Result<DomainType, ClientError> {
    let info = r.info.ok_or_else(||
        ClientError::invalid_response("info", "missing")
    )?;  // Error visible
    // ...
}
```

---

## When Defaults Are Acceptable

Not every `unwrap_or_default()` is wrong. Defaults are acceptable when:

1. **The field is explicitly optional in the domain model**
   ```rust
   // Optional metadata that may legitimately be absent
   let metadata: Option<Value> = response.metadata;
   ```

2. **The default is semantically meaningful**
   ```rust
   // Empty tags list is valid—means "no tags"
   let tags = response.tags.unwrap_or_default(); // Vec<String>
   ```

3. **Absence cannot be confused with a valid value**
   ```rust
   // description being None vs "" are distinguishable
   let description: Option<String> = response.description;
   ```

---

## Red Flags to Watch For

When reviewing code, these patterns indicate potential phantom data:

### 1. `unwrap_or_default()` on Numeric Types
```rust
// RED FLAG: 0 looks like a valid measurement
let active_connections = pool.active.unwrap_or_default();
```

### 2. `unwrap_or_else(|| ...)` with Fabricated Values
```rust
// RED FLAG: "unknown" looks like real status
let status = check.status.unwrap_or_else(|| "unknown".to_string());
```

### 3. Default Structs for Missing Nested Data
```rust
// RED FLAG: Entire section fabricated
let config = response.config.unwrap_or_else(default_config);
```

### 4. Silent Fallbacks in Health Checks
```rust
// RED FLAG: Health check that never fails is useless
let health = check_health().unwrap_or(HealthStatus::Ok);
```

---

## Implementation Checklist

When implementing new conversions or response handling:

- [ ] Is this field required by the protocol/API contract?
- [ ] If missing, would a default be indistinguishable from a valid value?
- [ ] Could a consumer make incorrect decisions based on a default?
- [ ] Is the error message actionable? (includes field name, explains what's wrong)
- [ ] Is the error type appropriate? (`InvalidResponse` for protocol violations)

---

## The TAS-177 Discovery

### What We Found

During gRPC client implementation (TAS-177), analysis revealed pervasive patterns like:

```rust
// Found throughout conversions.rs
let checks = response.checks.unwrap_or_else(|| ReadinessChecks {
    web_database: HealthCheck { status: "unknown".into(), ... },
    orchestration_database: HealthCheck { status: "unknown".into(), ... },
    // ... more fabricated checks
});
```

A client calling `get_readiness()` would receive what looked like a valid response with "unknown" status for all checks—when in reality, the server sent nothing.

### The Refactoring

All required-field patterns were changed to explicit errors:

```rust
// After refactoring
let checks = response.checks.ok_or_else(|| {
    ClientError::invalid_response(
        "ReadinessResponse.checks",
        "Readiness response missing required health checks",
    )
})?;
```

Now a malformed server response immediately fails with:
```
Error: Invalid response: ReadinessResponse.checks - Readiness response missing required health checks
```

The problem is visible. The fix can be applied. Trust is preserved.

---

## Related Principles

- **Tenet #11: Fail Loudly** in [Tasker Core Tenets](./tasker-core-tenets.md)
- **Meta-Principle #6: Errors Over Defaults**
- **Defense in Depth** — fail loudly is a form of protection; silent defaults are a form of hiding

---

## Summary

| Don't | Do |
|-------|-----|
| Hide missing data with defaults | Return explicit errors |
| Make consumers guess if data is real | Distinguish required vs optional |
| Fabricate "unknown" status values | Error: "status unavailable" |
| Swallow errors in conversions | Propagate with `?` operator |
| Treat all fields as optional | Model optionality in types |

**The golden rule**: If you can't tell the difference between "server sent 0" and "server sent nothing," you have a phantom data problem.

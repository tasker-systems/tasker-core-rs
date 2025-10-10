# Retry Semantics: max_attempts and retryable

**Last Updated**: 2025-10-10
**Audience**: Developers
**Status**: Active
**Related Docs**: [Documentation Hub](README.md) | [Bug Report: Retry Eligibility Logic](bug-reports/2025-10-05-retry-eligibility-bug.md) | [States and Lifecycles](states-and-lifecycles.md)

← Back to [Documentation Hub](README.md)

---

## Overview

The Tasker orchestration system uses two configuration fields to control step execution and retry behavior:

1. **`max_attempts`**: Maximum number of total execution attempts (including first execution)
2. **`retryable`**: Whether the step can be retried after failure

## Semantic Definitions

### max_attempts

**Definition**: The maximum number of times a step can be attempted, **including the first execution**.

This is NOT "number of retries" - it's **total attempts**:
- `max_attempts=0`: Step can never execute (likely a configuration error)
- `max_attempts=1`: Exactly one attempt (no retries after failure)
- `max_attempts=3`: First attempt + up to 2 retries = 3 total attempts

**Implementation**: SQL formula `attempts < max_attempts` where `attempts` starts at 0.

### retryable

**Definition**: Whether a step can be retried **after the first execution fails**.

**Important**: The `retryable` flag does NOT affect the first execution attempt:
- First execution (attempts=0): **Always eligible** regardless of retryable setting
- Retry attempts (attempts>0): Require `retryable=true`

## Configuration Examples

### Single Execution, No Retries

```yaml
retry:
  retryable: false
  max_attempts: 1  # First attempt only
  backoff: exponential
```

**Behavior**:
| attempts | retry_eligible | Outcome |
|----------|---------------|---------|
| 0 | ✅ true | First execution allowed |
| 1 | ❌ false | No retries (retryable=false) |

**Use Case**: Idempotent operations that should not retry (e.g., record creation with unique constraints)

### Multiple Attempts with Retries

```yaml
retry:
  retryable: true
  max_attempts: 3  # First attempt + 2 retries
  backoff: exponential
  backoff_base_ms: 1000
```

**Behavior**:
| attempts | retry_eligible | Outcome |
|----------|---------------|---------|
| 0 | ✅ true | First execution allowed |
| 1 | ✅ true | First retry allowed (1 < 3) |
| 2 | ✅ true | Second retry allowed (2 < 3) |
| 3 | ❌ false | Max attempts exhausted (3 >= 3) |

**Use Case**: External API calls that might have transient failures

### Unlimited Retries (Not Recommended)

```yaml
retry:
  retryable: true
  max_attempts: 999999
  backoff: exponential
  backoff_base_ms: 1000
  max_backoff_ms: 300000  # Cap at 5 minutes
```

**Behavior**: Will retry until external intervention (task cancellation, system restart)

**Use Case**: Critical operations that must eventually succeed (use with caution!)

## Retry Eligibility Logic

### SQL Implementation

From `migrations/20251006000000_fix_retry_eligibility_logic.sql`:

```sql
-- retry_eligible calculation
(
  COALESCE(ws.attempts, 0) = 0  -- First attempt always eligible
  OR (
    COALESCE(ws.retryable, true) = true  -- Must be retryable for retries
    AND COALESCE(ws.attempts, 0) < COALESCE(ws.max_attempts, 3)
  )
) as retry_eligible
```

### Decision Tree

```
Is attempts = 0?
├─ YES → retry_eligible = TRUE (first execution)
└─ NO  → Is retryable = true?
    ├─ YES → Is attempts < max_attempts?
    │   ├─ YES → retry_eligible = TRUE (retry allowed)
    │   └─ NO  → retry_eligible = FALSE (max attempts exhausted)
    └─ NO  → retry_eligible = FALSE (retries disabled)
```

## Edge Cases

### max_attempts=0

```yaml
retry:
  max_attempts: 0
```

**Behavior**: Step can never execute (0 < 0 = false for all attempts)

**Status**: ⚠️ Configuration error - likely unintended

**Recommendation**: Use `max_attempts: 1` for single execution

### retryable=false with max_attempts > 1

```yaml
retry:
  retryable: false
  max_attempts: 3  # Only first attempt will execute
```

**Behavior**: First execution allowed, but no retries regardless of max_attempts

**Effective Result**: Same as `max_attempts: 1`

**Recommendation**: Set `max_attempts: 1` when `retryable: false` for clarity

## Historical Context

### Why "max_attempts" instead of "retry_limit"?

The original field name `retry_limit` was semantically confusing:

**Old Interpretation** (incorrect):
- `retry_limit=1` → "1 retry allowed" → 2 total attempts?
- `retry_limit=0` → "0 retries" → 1 attempt or blocked?

**New Interpretation** (clear):
- `max_attempts=1` → "1 total attempt" → exactly 1 execution
- `max_attempts=0` → "0 attempts" → clearly invalid

### Migration Timeline

- **Original**: `retry_limit` field with ambiguous semantics
- **2025-10-05**: Bug discovered - `retry_limit=0` blocked all execution
- **2025-10-06**: Fixed SQL logic + renamed to `max_attempts`
- **2025-10-06**: Added 6 SQL boundary tests for edge cases

## Testing

### Boundary Condition Tests

See `tests/integration/sql_functions/retry_boundary_tests.rs` for comprehensive coverage:

1. `test_max_attempts_zero_allows_first_execution` - Edge case handling
2. `test_max_attempts_zero_blocks_after_first` - Exhaustion after first
3. `test_max_attempts_one_semantics` - Single execution semantics
4. `test_max_attempts_three_progression` - Standard retry progression
5. `test_first_attempt_ignores_retryable_flag` - First execution independence
6. `test_retries_require_retryable_true` - Retry flag enforcement

All tests passing as of 2025-10-06.

## Best Practices

### For Single-Execution Steps

```yaml
retry:
  retryable: false
  max_attempts: 1
  backoff: exponential  # Ignored, but required for schema
```

**Why**: Makes intent crystal clear - execute once, never retry

### For Transient Failure Tolerance

```yaml
retry:
  retryable: true
  max_attempts: 3
  backoff: exponential
  backoff_base_ms: 1000
  max_backoff_ms: 30000
```

**Why**: Reasonable retry count with exponential backoff prevents thundering herd

### For Critical Operations

```yaml
retry:
  retryable: true
  max_attempts: 10
  backoff: exponential
  backoff_base_ms: 5000
  max_backoff_ms: 300000  # 5 minutes
```

**Why**: More attempts with longer backoff for operations that must succeed

## Related Documentation

- [Bug Report: Retry Eligibility Logic](bug-reports/2025-10-05-retry-eligibility-bug.md)
- [State Machine Documentation](states-and-lifecycles.md)
- SQL Function: `get_step_readiness_status_batch`
- Migration: `20251006000000_fix_retry_eligibility_logic.sql`

---

**Questions or Issues?** See test suite for comprehensive examples or consult bug report for historical context.

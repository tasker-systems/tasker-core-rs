# TAS-29 High Concurrency Bug: Race Conditions in Distributed Orchestration

**Ticket**: TAS-29 Phase 5.4 - Distributed Lifecycle Benchmarking
**Date Discovered**: 2025-10-08
**Status**: ✅ RESOLVED
**Severity**: Critical (blocked E2E benchmarking and high-concurrency operations)

---

## Executive Summary

During E2E latency benchmarking implementation, two subtle race conditions were discovered that prevented task completion under high concurrency load. Both issues stemmed from microsecond-level timing assumptions that only manifested under benchmark load conditions.

**Impact**:
- Tasks completing successfully at low load (~3s for manual execution)
- **100% failure rate** during benchmark warmup (high concurrency)
- Database constraint violations and invalid state transitions

**Root Causes**:
1. **Worker**: Event firing before successful step claim verification
2. **Orchestration**: PGMQ notifications sent before state transitions committed

**Discovery Method**: Load testing via Criterion benchmarks exposed timing assumptions that manual testing did not reveal.

---

## Symptoms and Timeline

### Initial Observation

**Manual Testing** (Low Load):
```bash
# Single task creation - WORKS
cargo run --package tasker-client --bin tasker-cli -- task create \
  --namespace rust_e2e_linear --name mathematical_sequence --input '{"even_number":8}'

# Result: 4/4 steps complete in ~3 seconds ✅
```

**Benchmark Testing** (High Concurrency):
```bash
# E2E benchmark warmup - FAILS
cargo bench --bench e2e_latency

# Result: Tasks stuck at 3/4 steps, timeout after 10s ❌
# Error: "Invalid state transition from Some('enqueued') to 'EnqueueForOrchestration'"
```

### Diagnostic Process

1. **Logs Analysis**: Discovered state transition error during handler completion
2. **Code Review**: Found event firing outside claim verification block
3. **First Fix Applied**: Moved event firing inside claim block, added state verification
4. **Rebuild and Test**: Manual tasks worked, but benchmark still failed (different error)
5. **Second Issue Discovered**: Database constraint violation on `most_recent` unique index
6. **Timeline Analysis**: PGMQ notification fired before state transition committed
7. **Second Fix Applied**: Reordered orchestration operations
8. **Verification**: Both fixes together enabled successful high-concurrency execution

---

## Race Condition #1: Worker Event Firing

### Technical Details

**File**: `tasker-worker/src/worker/command_processor.rs:625-801`
**Function**: `process_step_execution_message`

**Problem**: Event firing happened **outside** the claim verification block, causing handlers to execute even when step claims failed.

**Original Code** (Buggy):
```rust
if claimed {
    // State verification and message deletion
    // ...
}  // <-- End of if block

// ❌ Event fires OUTSIDE the if block - happens regardless of claim result!
match &self.event_publisher {
    Some(publisher) => publisher.fire_step_execution_event(&task_sequence_step).await
}
```

**Timeline Under Load**:
```
t=0ms   : Worker receives PGMQ notification for step X
t=5ms   : Worker attempts to claim step X
t=8ms   : Claim returns false (step already claimed by another worker)
t=10ms  : Code skips claim-specific logic
t=12ms  : Event STILL FIRES (outside if block)
t=15ms  : Handler executes against step in wrong state
t=20ms  : Handler tries to transition "enqueued" → "enqueued_for_orchestration"
t=22ms  : ERROR: Invalid state transition (step must be "in_progress" first)
```

**Why Manual Testing Didn't Catch It**:
- Low concurrency meant claims almost always succeeded
- ~3 second gaps between operations allowed state transitions to complete
- Race condition only manifested when workers competed for the same step

### Solution: State Verification with Exponential Backoff

**Fixed Code**:
```rust
if claimed {
    // RACE CONDITION FIX: Verify state visibility before handler execution
    let state_machine_verify = StepStateMachine::new(
        task_sequence_step.workflow_step.clone().into(),
        self.context.clone(),
    );

    let mut verified = false;
    for attempt in 0..5 {
        match state_machine_verify.current_state().await {
            Ok(WorkflowStepState::InProgress) => {
                verified = true;
                break;
            }
            Ok(other_state) if attempt < 4 => {
                // Exponential backoff: 1ms, 2ms, 4ms, 8ms
                let delay_ms = 2_u64.pow(attempt);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            _ => return Err(...)
        }
    }

    if !verified {
        return Err(TaskerError::StateTransitionError(
            "State transition not visible after retries"
        ));
    }

    // ... message deletion ...

    // ✅ Event fires INSIDE the if block - only after successful claim + verification
    match &self.event_publisher {
        Some(publisher) => publisher
            .fire_step_execution_event(&task_sequence_step)
            .await
    }

    Ok(())
} else {
    // ✅ Step not claimed - skip execution entirely
    debug!("Step not claimed - skipping execution");
    Ok(())
}
```

**Key Improvements**:
1. Event firing moved **inside** `if claimed {}` block
2. State verification with exponential backoff (1-2-4-8ms)
3. Explicit `else` block to handle claim failures cleanly
4. PostgreSQL Read Committed isolation accounted for

**Performance Impact**: Verification typically succeeds on first attempt (0 retries) under fixed conditions.

---

## Race Condition #2: Orchestration PGMQ Timing

### Technical Details

**File**: `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs:521-574`
**Function**: `enqueue_individual_step`

**Problem**: PGMQ notification sent **before** step state transition completed, causing workers to receive notifications before state was visible.

**Original Code** (Buggy):
```rust
// 1. Send PGMQ message (notification fires IMMEDIATELY)
message_client.send_with_notify(&queue_name, step_message).await?;

// 2. Mark step as "enqueued" (happens AFTER notification)
step_state_machine.transition(StepEvent::Enqueue).await?;
```

**Timeline Example** (Task 0199c5cb-b39e-7cbc-b6ff-3d6f23c3b91c):

| Time (ms) | Event | Component |
|-----------|-------|-----------|
| **0** | PGMQ message sent (notification fires) | Orchestration |
| **12** | Worker receives notification, starts processing | Worker |
| **31** | Worker attempts step claim/transition | Worker |
| **52** | Orchestration state transition completes | Orchestration |
| **54** | Worker claim fails: duplicate key constraint violation | Worker |

**Error**:
```
duplicate key value violates unique constraint "idx_workflow_step_transitions_uuid_most_recent"
```

**Root Cause**: Both orchestration and worker tried to create transition records with `most_recent=true` simultaneously.

**User's Insight**: *"I'm really glad we found the bug, but it's also amusing to realize that we inadvertently built an event driven system that moves that fast."*

The pgmq-notify library was so efficient it exposed microsecond-level transaction timing that traditional polling-based systems would never encounter.

### Solution: Atomic State-Then-Notify Ordering

**Fixed Code**:
```rust
// TAS-29 Phase 5.4 Fix: State transition BEFORE PGMQ notification
// This ensures the state is fully committed and visible before workers
// receive the notification.

// 1. Mark step as enqueued FIRST (commit transaction)
self.state_manager
    .mark_step_enqueued(viable_step.step_uuid)
    .await
    .map_err(|e| {
        error!(
            correlation_id = %correlation_id,
            step_uuid = %viable_step.step_uuid,
            error = %e,
            "Failed to transition step to enqueued state before enqueueing"
        );
        TaskerError::StateTransitionError(format!(
            "Failed to mark step {} as enqueued: {}",
            viable_step.step_uuid, e
        ))
    })?;

info!(
    correlation_id = %correlation_id,
    step_uuid = %viable_step.step_uuid,
    "Successfully marked step as enqueued - now sending to PGMQ"
);

// 2. THEN send PGMQ message (notification fires AFTER state is committed)
let msg_id = self
    .context
    .message_client()
    .send_json_message(&queue_name, &simple_message)
    .await?;

info!(
    correlation_id = %correlation_id,
    step_uuid = %viable_step.step_uuid,
    queue_name = %queue_name,
    msg_id = msg_id,
    "Successfully sent step to pgmq queue (state transition completed first)"
);
```

**Key Improvements**:
1. State transition happens **FIRST** (line 526-540)
2. PGMQ message sent **SECOND** (line 549-566)
3. Diagnostic logging to verify correct ordering
4. Error handling fails fast if state transition fails
5. No transaction overlap between orchestration and worker

---

## Troubleshooting Methodology

### 1. Symptom Recognition
- Compare manual vs benchmark behavior
- Identify load-dependent failures
- Look for timing-related errors

### 2. Log Analysis
- Check timestamps of related events
- Identify ordering assumptions
- Look for constraint violations

### 3. Code Flow Tracing
- Map event flow through components
- Identify claim/verification boundaries
- Check event firing locations

### 4. Timeline Reconstruction
- Build microsecond-level event timeline
- Identify race windows
- Correlate with database transactions

### 5. Hypothesis Testing
- Apply fix to one component
- Test under load
- Iterate if additional issues found

### 6. Verification
- Test at multiple load levels:
  - Single task (manual)
  - Concurrent tasks (5 parallel)
  - Benchmark load (220 iterations)

---

## Files Modified

### Worker Fix
**`tasker-worker/src/worker/command_processor.rs`** (lines 625-801):
- Added state verification with exponential backoff
- Moved event firing inside `if claimed {}` block
- Added `else` block to skip execution when claim fails

### Orchestration Fix
**`tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs`** (lines 526-574):
- Reordered operations: state transition before PGMQ send
- Added diagnostic logging
- Changed error handling to fail if state transition fails

### Supporting Changes
**`docker/docker-compose.test.yml`** (line 85):
- Changed `RUST_LOG: info` to `RUST_LOG: debug` for detailed logging

**`tasker-worker/Cargo.toml`** and **`tasker-orchestration/Cargo.toml`**:
- Removed placeholder benchmark references

---

## Testing Results

### Before Fixes
- ❌ Manual tasks: 4/4 steps complete (~3s)
- ❌ Benchmark warmup: Tasks stuck at 3/4 steps, timeout
- ❌ Error rate: 100% under load
- ❌ Database errors: Duplicate key constraint violations

### After Worker Fix Only
- ✅ Manual tasks: 4/4 steps complete (~250ms)
- ❌ Benchmark warmup: Tasks stuck at 0/4 steps
- ❌ Error: Duplicate key constraint violations

### After Both Fixes
- ✅ Manual tasks: 4/4 steps complete (~250-900ms)
- ✅ Concurrent tasks (5 parallel): All complete successfully
- ✅ Benchmark warmup: All tasks completing 4/4 steps
- ✅ Benchmark measurement: 220 iterations successful
- ✅ Error rate: 0%
- ✅ Database errors: 0

### Performance Metrics

**Manual Task Creation**:
```
Status: all_complete
Steps: 4/4 completed
Progress: 100.0%
Time: 250-900ms (linear workflow)
```

**Concurrent Tasks** (5 parallel):
```
All 5 tasks: 4/4 steps, all_complete
Total time: ~1 second
```

**E2E Benchmark** (Rust Native):
```
Linear workflow:  133.5ms mean (target < 500ms) ✅ 3.7x better
Diamond workflow: 140.1ms mean (target < 800ms) ✅ 5.7x better
```

---

## Key Learnings

### 1. Event-Driven Speed
pgmq-notify is fast enough to expose microsecond-level transaction timing. Traditional polling-based systems (checking every 100ms+) would never encounter this race condition.

### 2. PostgreSQL Read Committed Isolation
Under high load, transaction visibility can be delayed by milliseconds. State verification with exponential backoff accounts for this.

### 3. Load Testing Value
Benchmarking serves dual purpose:
- Performance measurement
- Race condition detection
- Production simulation

Manual testing at low concurrency cannot reveal timing-dependent bugs.

### 4. Diagnostic Logging Importance
Added log messages like "Successfully marked step as enqueued - now sending to PGMQ" were crucial for verifying fix deployment and correct ordering.

### 5. Atomic Operation Ordering
In distributed systems with instant notifications:
- **State must be committed before notifications fire**
- **Workers should never race with orchestration's state management**
- **Notification speed can become a bug exposer, not just a feature**

---

## Performance Impact

### State Verification Overhead
- **Typical case**: 0 retries (state visible immediately)
- **Under load**: 0-1 retries (1-2ms delay)
- **Worst case**: 4 retries (15ms total delay) before error

### Ordering Change Impact
- **Orchestration**: No performance impact (same operations, different order)
- **Worker**: Eliminates constraint violation overhead and retry logic
- **Overall**: Net performance improvement due to eliminated failures

---

## Related Tickets

- **TAS-29**: Distributed Lifecycle Benchmarking (parent ticket)
- **TAS-40**: Command Pattern (architectural context)
- **TAS-41**: Enhanced State Machines (state management)
- **TAS-34**: Component-based Configuration (system architecture)

---

## Recommendations

### For Future Development

1. **Load Test Early**: Run benchmarks during development, not just for performance measurement
2. **State Verification**: Always verify state visibility after transitions in distributed systems
3. **Atomic Ordering**: Document and enforce ordering requirements for state + notification patterns
4. **Diagnostic Logging**: Include operation ordering in log messages for debugging
5. **Integration Tests**: Write tests that simulate high concurrency, not just correctness

### For Event-Driven Systems

1. **Assume Instant Notifications**: pgmq-notify is faster than transaction commits
2. **State Before Notify**: Always commit state before sending notifications
3. **Verification Loops**: Use exponential backoff for state verification
4. **Monitor Constraint Violations**: Track unique constraint violations as race condition indicators
5. **Document Timing Assumptions**: Make transaction visibility assumptions explicit

---

## Conclusion

Two subtle race conditions in distributed orchestration were discovered through E2E benchmarking and systematically resolved. Both issues stemmed from timing assumptions that were valid at low concurrency but failed under load.

**Final Status**:
- ✅ Worker event firing race condition: **FIXED**
- ✅ Orchestration PGMQ timing race condition: **FIXED**
- ✅ E2E benchmarking: **OPERATIONAL**
- ✅ High concurrency support: **VERIFIED**

The fixes demonstrate that microsecond-level event-driven systems require careful consideration of transaction commit timing and state visibility in distributed architectures.

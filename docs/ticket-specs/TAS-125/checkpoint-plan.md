# TAS-125: Checkpoint Persistence for Batch Processing

**Status**: Planning Complete  
**Author**: Claude (with Pete Taylor)  
**Date**: 2025-12-31  
**Related**: TAS-59 (Batch Processing), TAS-64 (Checkpoint Progress), TAS-49 (DLQ Integration)

---

## Table of Contents

- [Problem Statement](#problem-statement)
- [Architectural Concerns with Initial PGMQ Approach](#architectural-concerns-with-initial-pgmq-approach)
- [Solution: Checkpoint-Yield Pattern](#solution-checkpoint-yield-pattern)
- [Schema Design](#schema-design)
- [Handler API Design](#handler-api-design)
- [FFI Extension Points](#ffi-extension-points)
- [Error Handling](#error-handling)
- [ResetForRetry Behavior](#resetforretry-behavior)
- [Implementation Phases](#implementation-phases)
- [Testing Strategy](#testing-strategy)

---

## Problem Statement

The `checkpoint_interval` configuration exists in batch worker templates but doesn't actually persist intermediate progress during batch processing. When a batch worker crashes mid-execution:

- All progress since the last step result is lost
- Worker must restart from the beginning of its assigned range
- For large batches (e.g., 10,000 items), this creates significant waste

**Current State**: `CheckpointProgress` exists in `batch_worker.rs` but only captures checkpoint data in failure results, not during successful incremental processing.

**Goal**: Enable batch workers to persist progress at `checkpoint_interval` boundaries such that crashes result in resumption from the last checkpoint, not from the beginning.

---

## Architectural Concerns with Initial PGMQ Approach

An initial design proposed using PGMQ for async checkpoint persistence:

```
Worker: write_checkpoint(cursor) → PGMQ queue → CheckpointProcessor → DB update
```

### Concern 1: Race Conditions Break Transactional Certainty

**Problem**: Async messaging creates a gap between worker's mental model and persisted state.

```
Worker: processes items 1-1000, sends checkpoint(1000) to PGMQ
Worker: processes items 1001-2000, sends checkpoint(2000) to PGMQ
Worker: crashes at item 2500
Queue: checkpoint(1000) consumed and persisted
Queue: checkpoint(2000) still in queue (timing issue)
Retry: resumes from 1000, reprocesses 1001-2500
Result: Items 1001-2000 processed TWICE → idempotency violation
```

**Key distinction from final step results**: When a worker sends a final result, it exits immediately (no gap for drift). Intermediate checkpoints have continued processing after send, creating opportunity for lost updates.

### Concern 2: Overloaded `workflow_step.results`

The existing `results` column mixes concerns:
- Business output (`result`)
- Orchestration metadata (`metadata.context.checkpoint_progress`)
- Batch outcomes (`batch_processing_outcome`)

**Issues**:
1. Deep serialization paths: `results.metadata.context.checkpoint_progress`
2. Ownership confusion: handler output vs orchestration bookkeeping
3. Coupling: checkpoint logic tangled with result persistence

### Concern 3: Direct DB Access Violates Architecture

Workers (Ruby, Python, TypeScript, Rust in worker context) should not access the database directly. The Tasker architecture deliberately separates:

- **Domain code**: Business logic in worker handlers
- **Infrastructure code**: DB access in tasker-worker Rust core

This separation prevents transaction conflicts when domain code has its own DB layer.

---

## Solution: Checkpoint-Yield Pattern

Instead of async side-effects during execution, handlers **yield control** with a special result type, allowing the Rust infrastructure to:

1. Persist checkpoint atomically
2. Re-dispatch the step for continuation
3. Maintain step claim throughout (no visibility timeout issues)

### Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    CHECKPOINT-YIELD RE-INVOCATION FLOW                       │
└─────────────────────────────────────────────────────────────────────────────┘

Handler invoked (first time or resumed)
         │
         ▼
Extract checkpoint context (if present)
         │
         ▼
Process items from checkpoint_cursor to checkpoint_cursor + checkpoint_interval
         │
         ├────────────────────────────────────────┐
         │                                        │
         ▼                                        ▼
   More items to process?                   Batch complete?
         │                                        │
         │ YES                                    │ YES
         ▼                                        ▼
Return CheckpointYield result            Return Success result
(cursor, items_processed,                (final aggregated data)
 accumulated_results)                           │
         │                                        │
         ▼                                        │
┌────────────────────────────────────────┐        │
│ tasker-worker Rust side:               │        │
│ 1. Persist checkpoint atomically       │        │
│ 2. Step remains claimed (in_progress)  │        │
│ 3. Re-dispatch step with checkpoint    │        │
└────────────────────────────────────────┘        │
         │                                        │
         ▼                                        │
Handler re-invoked with checkpoint context        │
         │                                        │
         └────────────────────────(loop)──────────┘
```

### Key Properties

1. **Handler doesn't block** - yields with a special result type
2. **Rust side has transactional certainty** - checkpoint persists BEFORE re-dispatch
3. **Step remains claimed throughout** - no visibility timeout issues
4. **Uses existing dispatch infrastructure** - no new queues needed
5. **State machine untouched** - step stays `in_progress` during yield cycle

---

## Schema Design

### New Column: `checkpoint`

```sql
ALTER TABLE tasker_workflow_steps ADD COLUMN checkpoint JSONB;
```

**Rationale for dedicated column**:
- Clean separation: `checkpoint` is orchestration bookkeeping, `results` is business output
- Simple read path: `step.checkpoint?.cursor`
- Independent lifecycle: checkpoint updates don't mutate results
- ResetForRetry semantics clearer: explicitly preserve/clear checkpoint

### Checkpoint Structure (with SOC2-Compliant Audit Trail)

```json
{
  "cursor": 7000,
  "items_processed": 7000,
  "timestamp": "2025-01-15T10:30:00Z",
  "accumulated_results": {
    "total_value": 15432.50,
    "category_counts": {"electronics": 3200, "clothing": 2100}
  },
  "history": [
    {"cursor": 1000, "timestamp": "2025-01-15T10:20:00Z"},
    {"cursor": 2000, "timestamp": "2025-01-15T10:22:00Z"},
    {"cursor": 3000, "timestamp": "2025-01-15T10:24:00Z"},
    {"cursor": 4000, "timestamp": "2025-01-15T10:26:00Z"},
    {"cursor": 5000, "timestamp": "2025-01-15T10:28:00Z"},
    {"cursor": 6000, "timestamp": "2025-01-15T10:29:00Z"},
    {"cursor": 7000, "timestamp": "2025-01-15T10:30:00Z"}
  ]
}
```

**Fields**:
- `cursor`: Current position (any JSON value - integer, timestamp, UUID, etc.)
- `items_processed`: Running count for observability
- `timestamp`: When checkpoint was persisted
- `accumulated_results`: Optional partial aggregations carried across yields
- `history`: Append-only log for audit trail

### Accumulated Results: Use Cases

**When accumulated results help:**
1. **Running aggregations** (inventory totals): Can't re-derive without re-reading processed items
2. **Error collection**: Accumulate errors for final report without losing them on restart
3. **Per-segment metrics**: Keep segment-level metrics for debugging/observability
4. **Distributed counters**: Category counts, histograms that span segments

**When cursor-only suffices:**
1. **CSV row processing**: Just track row number, re-read from file
2. **API pagination**: Server maintains state, just need offset/page
3. **Database streaming**: Just track ID cutoff

**Recommendation**: Make `accumulated_results` optional. Heavy accumulated results is a code smell - consider restructuring. Lightweight running aggregations are valuable.

---

## Handler API Design

### New Result Type: CheckpointYield

```rust
// In tasker-shared/src/messaging/execution_types.rs or new module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointYieldData {
    pub step_uuid: Uuid,
    pub cursor: serde_json::Value,
    pub items_processed: u64,
    pub accumulated_results: Option<serde_json::Value>,
}

// Extend StepExecutionResult or create wrapper
pub enum BatchWorkerResult {
    /// Worker completed, return final results
    Complete(StepExecutionResult),
    
    /// Worker checkpointing, persist and re-invoke
    CheckpointYield(CheckpointYieldData),
}
```

### Rust Handler Pattern

```rust
async fn call(&self, context: &BatchWorkerContext) -> BatchWorkerResult {
    // Get starting position from checkpoint or beginning
    let start = context.checkpoint_cursor().unwrap_or(0);
    let end = start + context.checkpoint_interval();
    
    // Get accumulated results from previous yield (if any)
    let mut accumulated = context.accumulated_results()
        .cloned()
        .unwrap_or_default();
    
    // Process this segment
    let segment_results = self.process_range(start, end, &mut accumulated);
    
    if context.has_more_items(end) {
        // More work to do - yield with checkpoint
        BatchWorkerResult::CheckpointYield(CheckpointYieldData {
            step_uuid: context.step_uuid(),
            cursor: json!(end),
            items_processed: end as u64,
            accumulated_results: Some(accumulated),
        })
    } else {
        // Complete - return final results
        BatchWorkerResult::Complete(success_result(
            context.step_uuid(),
            json!({
                "items_processed": end,
                "final_results": accumulated
            }),
            elapsed_ms,
            None,
        ))
    }
}
```

### Ruby Handler Pattern

```ruby
class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
  def call(context)
    batch_ctx = get_batch_context(context)
    
    # Handle no-op placeholder
    no_op_result = handle_no_op_worker(batch_ctx)
    return no_op_result if no_op_result
    
    # Get starting position from checkpoint or batch start
    start_cursor = batch_ctx.checkpoint_cursor || batch_ctx.start_cursor
    end_cursor = [start_cursor + batch_ctx.checkpoint_interval, batch_ctx.end_cursor].min
    
    # Get accumulated results from previous yield
    accumulated = batch_ctx.accumulated_results || initial_accumulator
    
    # Process this segment
    process_segment(start_cursor, end_cursor, accumulated)
    
    if end_cursor < batch_ctx.end_cursor
      # More work to do - yield with checkpoint
      checkpoint_yield(
        cursor: end_cursor,
        items_processed: end_cursor - batch_ctx.start_cursor,
        accumulated_results: accumulated
      )
    else
      # Complete - return final results
      success(result: {
        'items_processed' => end_cursor - batch_ctx.start_cursor,
        'final_results' => accumulated
      })
    end
  end
end
```

---

## FFI Extension Points

### Ruby EventBridge

```ruby
# lib/tasker_core/event_bridge.rb
def publish_step_checkpoint_yield(checkpoint_data)
  return unless active?
  
  logger.debug "Sending checkpoint yield to Rust: #{checkpoint_data[:event_id]}"
  
  validate_checkpoint_yield!(checkpoint_data)
  
  # New FFI method
  TaskerCore::FFI.checkpoint_yield_step_event(
    checkpoint_data[:event_id].to_s,
    checkpoint_data
  )
  
  publish('step.checkpoint_yield.sent', checkpoint_data)
  
  logger.debug 'Checkpoint yield sent to Rust'
end

private

def validate_checkpoint_yield!(data)
  required = %i[event_id step_uuid cursor items_processed]
  missing = required - data.keys
  raise ArgumentError, "Missing: #{missing.join(', ')}" if missing.any?
end
```

### Ruby Batchable Base Class

```ruby
# lib/tasker_core/step_handler/batchable.rb
def checkpoint_yield(cursor:, items_processed:, accumulated_results: nil)
  StepHandlerCallResult::CheckpointYield.new(
    cursor: cursor,
    items_processed: items_processed,
    accumulated_results: accumulated_results
  )
end
```

### Rust FfiDispatchChannel

```rust
// tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs

/// Handle checkpoint yield from FFI handler
/// 
/// Unlike complete(), this persists checkpoint and re-dispatches
/// without releasing the step claim.
pub fn checkpoint_yield(&self, event_id: Uuid, checkpoint_data: CheckpointYieldData) -> bool {
    let pending_event = {
        let mut pending = self.pending_events.write()
            .unwrap_or_else(|p| p.into_inner());
        // Don't remove - step stays pending for re-dispatch
        pending.get(&event_id).cloned()
    };
    
    if let Some(pending) = pending_event {
        // Block on checkpoint persistence + re-dispatch
        let result = self.config.runtime_handle.block_on(async {
            self.handle_checkpoint_yield(&pending.event, checkpoint_data).await
        });
        
        match result {
            Ok(()) => {
                debug!(
                    event_id = %event_id,
                    cursor = ?checkpoint_data.cursor,
                    "Checkpoint persisted, step re-dispatched"
                );
                true
            }
            Err(e) => {
                error!(
                    event_id = %event_id,
                    error = %e,
                    "Checkpoint yield failed - generating failure result"
                );
                // Send failure to completion channel
                self.send_checkpoint_failure(event_id, &pending.event, e);
                false
            }
        }
    } else {
        warn!(event_id = %event_id, "Checkpoint yield for unknown event");
        false
    }
}

async fn handle_checkpoint_yield(
    &self,
    event: &FfiStepEvent,
    checkpoint_data: CheckpointYieldData,
) -> Result<(), CheckpointError> {
    // 1. Persist checkpoint atomically
    self.checkpoint_service
        .persist_checkpoint(event.step_uuid, &checkpoint_data)
        .await?;
    
    // 2. Re-dispatch step (stays claimed)
    self.dispatch_sender
        .send(DispatchHandlerMessage::from_checkpoint_continuation(event))
        .await
        .map_err(|_| CheckpointError::RedispatchFailed)?;
    
    Ok(())
}
```

### CompletionProcessorService Integration

```rust
// tasker-worker/src/worker/handlers/completion_processor.rs

async fn process_completion(&self, result: StepExecutionResult) {
    match result.result_type() {
        ResultType::Success | ResultType::Failure => {
            // Existing flow: send to orchestration queue
            self.send_to_orchestration(result).await;
        }
        ResultType::CheckpointYield => {
            // New flow: persist and re-dispatch
            self.handle_checkpoint_yield(result).await;
        }
    }
}
```

---

## Error Handling

### Atomic Boundaries

The yield flow has two atomic operations:

```
┌─────────────────────────────────────────────────────────────────┐
│ Handler returns CheckpointYield                                  │
│         │                                                        │
│         ▼                                                        │
│ ┌───────────────────────────────────────────────────────────┐   │
│ │ Atomic Boundary 1: Persist Checkpoint                      │   │
│ │                                                            │   │
│ │ BEGIN TRANSACTION                                          │   │
│ │   UPDATE workflow_steps                                    │   │
│ │   SET checkpoint = jsonb_set(                              │   │
│ │     COALESCE(checkpoint, '{}'),                            │   │
│ │     '{history}',                                           │   │
│ │     COALESCE(checkpoint->'history', '[]') || $new_entry    │   │
│ │   )                                                        │   │
│ │   WHERE workflow_step_uuid = $1                            │   │
│ │ COMMIT                                                     │   │
│ └────────────────────────────────────────────────────────────┘   │
│         │                                                        │
│         │ On error → Generate retryable failure                  │
│         │            → Send to error handling path               │
│         │            → Step goes to WaitingForRetry              │
│         ▼                                                        │
│ ┌───────────────────────────────────────────────────────────┐   │
│ │ Atomic Boundary 2: Re-dispatch Step                        │   │
│ │                                                            │   │
│ │ Send DispatchHandlerMessage to dispatch channel            │   │
│ │ (Step remains claimed/in_progress throughout)              │   │
│ └────────────────────────────────────────────────────────────┘   │
│         │                                                        │
│         │ On error → Checkpoint saved but stuck                  │
│         │          → Staleness detection catches it              │
│         │          → Next execution resumes from checkpoint      │
│         ▼                                                        │
│ Handler re-invoked with checkpoint context                       │
└─────────────────────────────────────────────────────────────────┘
```

### Error Case Matrix

| Error Location | Step State | Checkpoint State | Recovery |
|----------------|------------|------------------|----------|
| Persist fails | in_progress | OLD checkpoint | Retryable failure → WaitingForRetry → retry from last good checkpoint |
| Re-dispatch fails | in_progress | NEW checkpoint saved | Staleness detection → timeout → retry from NEW checkpoint |
| Handler crashes before yield | in_progress | unchanged | Staleness detection → retry from existing checkpoint |

**Key invariant**: Checkpoint-persist-then-redispatch ordering ensures we never lose progress. Worst case is duplicate work from staleness timeout, not data loss.

---

## ResetForRetry Behavior

### Default: Preserve Checkpoint (Idempotency-First)

```rust
impl WorkflowStep {
    pub async fn reset_for_retry(
        &mut self, 
        pool: &PgPool, 
        reset_checkpoint: bool
    ) -> Result<(), sqlx::Error> {
        if reset_checkpoint {
            // Full reset - restart from beginning
            sqlx::query!(
                r#"
                UPDATE tasker_workflow_steps
                SET in_process = false,
                    processed = false,
                    processed_at = NULL,
                    results = NULL,
                    checkpoint = NULL,
                    backoff_request_seconds = NULL,
                    attempts = 0,
                    updated_at = NOW()
                WHERE workflow_step_uuid = $1::uuid
                "#,
                self.workflow_step_uuid
            )
            .execute(pool)
            .await?;
            
            self.checkpoint = None;
        } else {
            // Preserve checkpoint - continue from last position (default)
            sqlx::query!(
                r#"
                UPDATE tasker_workflow_steps
                SET in_process = false,
                    processed = false,
                    processed_at = NULL,
                    results = NULL,
                    backoff_request_seconds = NULL,
                    attempts = 0,
                    updated_at = NOW()
                WHERE workflow_step_uuid = $1::uuid
                "#,
                self.workflow_step_uuid
            )
            .execute(pool)
            .await?;
            
            // checkpoint field untouched
        }
        
        Ok(())
    }
}
```

### Operator API

```bash
# Default: preserve checkpoint (idempotent continuation)
curl -X PATCH /v1/tasks/{task}/workflow_steps/{step} \
  -H "Content-Type: application/json" \
  -d '{"action_type": "reset_for_retry"}'

# Explicit: reset checkpoint (restart from beginning)
curl -X PATCH /v1/tasks/{task}/workflow_steps/{step} \
  -H "Content-Type: application/json" \
  -d '{"action_type": "reset_for_retry", "reset_checkpoint": true}'
```

### Rationale

If batch worker 4 of 5 fails after processing 1000 of 2000 assigned items:
- **Without checkpoint reset**: Retry processes remaining 1000 items only
- **With checkpoint reset**: Retry reprocesses all 2000 items

Default to continuation for idempotency. Operators can explicitly reset when:
- Data corruption suspected in processed items
- Business logic changed requiring reprocessing
- Debugging requires fresh execution

---

## Implementation Phases

### Phase 1: Schema & Core Types (2-3 days)

1. Add `checkpoint` column migration
2. Create `CheckpointYieldData` type in tasker-shared
3. Create `CheckpointService` for persistence logic
4. Update `WorkflowStep` model with checkpoint accessors
5. Update `reset_for_retry` to support `reset_checkpoint` flag

### Phase 2: Rust Handler Support (2-3 days)

1. Create `BatchWorkerResult` enum (or extend `StepExecutionResult`)
2. Update `CompletionProcessorService` to handle checkpoint yields
3. Implement checkpoint persistence in completion flow
4. Implement re-dispatch logic
5. Add checkpoint context to `BatchWorkerContext`

### Phase 3: FFI Bridge Extension (3-4 days)

1. Add `checkpoint_yield_step_event` FFI function
2. Update `FfiDispatchChannel` with `checkpoint_yield()` method
3. Create Ruby `StepHandlerCallResult::CheckpointYield` type
4. Add `checkpoint_yield()` helper to `Batchable` base class
5. Update `EventBridge` with `publish_step_checkpoint_yield()`
6. Mirror changes for Python/TypeScript as needed

### Phase 4: Handler Migration & Testing (3-4 days)

1. Update CSV batch processing example to use checkpoint yields
2. Create integration tests for checkpoint-yield-resume cycle
3. Test error scenarios (persist failure, re-dispatch failure)
4. Test ResetForRetry with/without checkpoint reset
5. Performance testing with various checkpoint intervals

### Phase 5: Documentation & Operator Tools (1-2 days)

1. Update batch-processing.md guide
2. Document operator workflows for checkpoint inspection
3. Add checkpoint metrics to monitoring
4. Update DLQ documentation for checkpoint-aware resolution

**Total Estimate**: 11-16 days

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_checkpoint_structure_serialization() {
    let checkpoint = Checkpoint {
        cursor: json!(7000),
        items_processed: 7000,
        timestamp: Utc::now(),
        accumulated_results: Some(json!({"total": 100.0})),
        history: vec![...],
    };
    
    let json = serde_json::to_value(&checkpoint).unwrap();
    assert!(json.get("history").is_some());
}

#[test]
fn test_checkpoint_history_append() {
    let mut checkpoint = Checkpoint::new(1000);
    checkpoint.append_to_history(2000);
    checkpoint.append_to_history(3000);
    
    assert_eq!(checkpoint.history.len(), 3);
    assert_eq!(checkpoint.cursor, json!(3000));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_checkpoint_yield_cycle() {
    // Create batch worker step with 1000 items, checkpoint_interval=200
    let step = create_batch_worker_step(1000, 200).await;
    
    // First invocation: processes 0-200, yields
    let result1 = invoke_handler(&step).await;
    assert!(matches!(result1, BatchWorkerResult::CheckpointYield(_)));
    
    // Verify checkpoint persisted
    let checkpoint = get_step_checkpoint(step.uuid).await;
    assert_eq!(checkpoint.cursor, json!(200));
    
    // Second invocation: processes 200-400, yields
    let result2 = invoke_handler(&step).await;
    assert!(matches!(result2, BatchWorkerResult::CheckpointYield(_)));
    
    // ... continue until completion
    
    // Final invocation: processes 800-1000, completes
    let result_final = invoke_handler(&step).await;
    assert!(matches!(result_final, BatchWorkerResult::Complete(_)));
}

#[tokio::test]
async fn test_checkpoint_crash_recovery() {
    let step = create_batch_worker_step(1000, 200).await;
    
    // Process to checkpoint 600
    for _ in 0..3 {
        invoke_handler(&step).await;
    }
    
    // Simulate crash by resetting in_process without clearing checkpoint
    reset_step_for_staleness(&step).await;
    
    // Re-invoke should resume from 600
    let context = get_handler_context(&step).await;
    assert_eq!(context.checkpoint_cursor(), Some(json!(600)));
}
```

### Ruby Integration Tests

```ruby
RSpec.describe 'Checkpoint Yield Integration' do
  it 'persists checkpoint and re-invokes handler' do
    step = create_batch_worker_step(items: 1000, checkpoint_interval: 200)
    
    # First invocation
    result = handler.call(step_context(step))
    expect(result).to be_a(StepHandlerCallResult::CheckpointYield)
    expect(result.cursor).to eq(200)
    
    # Verify checkpoint via API
    checkpoint = fetch_step_checkpoint(step.uuid)
    expect(checkpoint['cursor']).to eq(200)
    expect(checkpoint['history'].length).to eq(1)
  end
end
```

---

## Open Questions (Resolved)

1. **Accumulated results column?** → Include optional `accumulated_results` in checkpoint column. Keep it lightweight.

2. **SOC2 audit trail?** → Append-only `history` array in checkpoint column captures cursor progression with timestamps.

3. **ResetForRetry checkpoint behavior?** → Default preserves checkpoint (idempotency). Explicit `reset_checkpoint: true` for full restart.

4. **FFI boundary changes?** → New `checkpoint_yield_step_event` FFI function + `checkpoint_yield()` method in dispatch channel.

5. **State machine impact?** → None. Step stays `in_progress` throughout yield cycle. Only final Success/Failure triggers state transitions.

---

## References

- [Batch Processing Guide](../../guides/batch-processing.md)
- [Worker Event Systems](../../architecture/worker-event-systems.md)
- [States and Lifecycles](../../architecture/states-and-lifecycles.md)
- [DLQ System Guide](../../guides/dlq-system.md)
- [TAS-59: Batch Processing Implementation](https://linear.app/tasker-systems/issue/TAS-59)
- [TAS-64: Checkpoint Progress Types](https://linear.app/tasker-systems/issue/TAS-64)

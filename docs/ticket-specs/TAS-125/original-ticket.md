# Batchable Handler Checkpoint

## Executive Summary

Enable batch workers to persist intermediate checkpoint progress **during** processing, not just at step completion/failure. This implements the missing write capability for the existing `checkpoint_interval` configuration.

**Origin**: Discovered during TAS-112 cross-language ergonomics work while analyzing batch processing patterns.

**Related Tickets**:

* [TAS-59](https://linear.app/tasker-systems/issue/TAS-59/batch-processing-implementation-plan): Batch Processing Implementation (defines `checkpoint_interval` config)
* [TAS-64](https://linear.app/tasker-systems/issue/TAS-64/retryability-and-resumability-e2e-testing): Retryability E2E Testing (validates cursor resumption)
* [TAS-118](https://linear.app/tasker-systems/issue/TAS-118/phase-6-batch-processing-lifecycle-analysis): Batch Processing Lifecycle Analysis (identifies checkpoint write gap)

**Full Specification**: `docs/ticket-specs/TAS-125/checkpoint-plan.md`

---

## Problem Statement

### Current State

1. **Configuration exists but is unused**:
   * `checkpoint_interval` is defined in `BatchConfiguration` (YAML templates)
   * Passed to workers via `BatchMetadata.checkpoint_interval`
   * **Not actually used for persisting intermediate progress**
2. **Checkpoint data only persists at step boundaries**:
   * When step **succeeds**: `last_cursor` included in final result
   * When step **fails**: `CheckpointProgress` stored in `metadata.context`
   * When step **crashes**: All progress is lost

### Gap

If a batch worker processing 10,000 items with `checkpoint_interval: 1000` crashes at item 8,000:

* **Today**: Recovery starts from 0 (or previous attempt's final `last_cursor`)
* **After TAS-125**: Recovery starts from 7,000 (last persisted checkpoint)

---

## Solution: Checkpoint-Yield Pattern

### Why Not PGMQ-Based Async Checkpoints?

An initial design proposed using PGMQ for async checkpoint persistence. This was **rejected** due to:

1. **Race conditions**: Async messaging creates gaps between worker's mental model and persisted state, risking duplicate processing
2. **Overloaded results column**: Mixing business output with orchestration bookkeeping creates deep serialization paths and ownership confusion
3. **Architecture violation**: Workers should not access DB directly; Tasker deliberately separates domain code from infrastructure

### Checkpoint-Yield Approach

Instead of async side-effects, handlers **yield control** with a special result type:

```
Handler invoked (first time or resumed)
         │
         ▼
Process items: checkpoint_cursor → checkpoint_cursor + checkpoint_interval
         │
         ├─────────────────────────────────┐
         │                                 │
         ▼                                 ▼
   More items?                       Batch complete?
         │ YES                             │ YES
         ▼                                 ▼
Return CheckpointYield             Return Success
(cursor, accumulated_results)      (final results)
         │
         ▼
┌─────────────────────────────────────────┐
│ tasker-worker Rust side:                │
│ 1. Persist checkpoint atomically        │
│ 2. Step remains claimed (in_progress)   │
│ 3. Re-dispatch step with checkpoint     │
└─────────────────────────────────────────┘
         │
         ▼
Handler re-invoked with checkpoint context
```

### Key Properties

1. **Handler doesn't block** - yields with special result type
2. **Rust side has transactional certainty** - checkpoint persists BEFORE re-dispatch
3. **Step remains claimed throughout** - no visibility timeout issues
4. **Uses existing dispatch infrastructure** - no new queues
5. **State machine untouched** - step stays `in_progress` during yield cycle

---

## Schema Design

### New Column: `checkpoint`

```sql
ALTER TABLE tasker_workflow_steps ADD COLUMN checkpoint JSONB;
```

### Structure (with SOC2-Compliant Audit Trail)

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
    ...
  ]
}
```

**Rationale for dedicated column**:

* Clean separation: `checkpoint` is orchestration bookkeeping, `results` is business output
* Simple read path: `step.checkpoint?.cursor`
* Independent lifecycle: checkpoint updates don't mutate results
* ResetForRetry semantics clearer: explicitly preserve/clear checkpoint

---

## Handler API

### New Result Type

```rust
pub enum BatchWorkerResult {
    Complete(StepExecutionResult),
    CheckpointYield(CheckpointYieldData),
}

pub struct CheckpointYieldData {
    pub step_uuid: Uuid,
    pub cursor: serde_json::Value,
    pub items_processed: u64,
    pub accumulated_results: Option<serde_json::Value>,
}
```

### Ruby Example

```ruby
class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
  def call(context)
    batch_ctx = get_batch_context(context)
    start_cursor = batch_ctx.checkpoint_cursor || batch_ctx.start_cursor
    end_cursor = [start_cursor + batch_ctx.checkpoint_interval, batch_ctx.end_cursor].min
    
    accumulated = batch_ctx.accumulated_results || initial_accumulator
    process_segment(start_cursor, end_cursor, accumulated)
    
    if end_cursor < batch_ctx.end_cursor
      checkpoint_yield(cursor: end_cursor, accumulated_results: accumulated)
    else
      success(result: { 'final_results' => accumulated })
    end
  end
end
```

---

## Error Handling

| Error Location | Step State | Recovery |
| -- | -- | -- |
| Persist fails | in_progress | Retryable failure → WaitingForRetry → retry from last good checkpoint |
| Re-dispatch fails | in_progress (checkpoint saved) | Staleness detection → timeout → retry from NEW checkpoint |
| Handler crashes | in_progress | Staleness detection → retry from existing checkpoint |

**Key invariant**: Checkpoint-persist-then-redispatch ordering ensures we never lose progress.

---

## ResetForRetry Behavior

**Default**: Preserve checkpoint (idempotency-first)

```bash
# Default: preserve checkpoint (continue from last position)
curl -X PATCH /v1/tasks/{task}/workflow_steps/{step} \
  -d '{"action_type": "reset_for_retry"}'

# Explicit: reset checkpoint (restart from beginning)
curl -X PATCH /v1/tasks/{task}/workflow_steps/{step} \
  -d '{"action_type": "reset_for_retry", "reset_checkpoint": true}'
```

---

## Implementation Phases

### Phase 1: Schema & Core Types (2-3 days)

* Add `checkpoint` column migration
* Create `CheckpointYieldData` type in tasker-shared
* Create `CheckpointService` for persistence logic
* Update `WorkflowStep` model with checkpoint accessors
* Update `reset_for_retry` to support `reset_checkpoint` flag

### Phase 2: Rust Handler Support (2-3 days)

* Create `BatchWorkerResult` enum
* Update `CompletionProcessorService` to handle checkpoint yields
* Implement checkpoint persistence in completion flow
* Implement re-dispatch logic
* Add checkpoint context to `BatchWorkerContext`

### Phase 3: FFI Bridge Extension (3-4 days)

* Add `checkpoint_yield_step_event` FFI function
* Update `FfiDispatchChannel` with `checkpoint_yield()` method
* Create Ruby `StepHandlerCallResult::CheckpointYield` type
* Add `checkpoint_yield()` helper to `Batchable` base class
* Update `EventBridge` with `publish_step_checkpoint_yield()`
* Mirror changes for Python/TypeScript

### Phase 4: Handler Migration & Testing (3-4 days)

* Update CSV batch processing example to use checkpoint yields
* Create integration tests for checkpoint-yield-resume cycle
* Test error scenarios (persist failure, re-dispatch failure)
* Test ResetForRetry with/without checkpoint reset
* Performance testing with various checkpoint intervals

### Phase 5: Documentation & Operator Tools (1-2 days)

* Update [batch-processing.md](http://batch-processing.md) guide
* Document operator workflows for checkpoint inspection
* Add checkpoint metrics to monitoring
* Update DLQ documentation for checkpoint-aware resolution

**Total Estimate**: 11-16 days

---

## Success Criteria

1. ✅ `checkpoint_interval` config is actually used
2. ✅ Workers can yield control at checkpoint boundaries
3. ✅ Crash at item X resumes from last checkpoint, not beginning
4. ✅ All four languages have consistent `checkpoint_yield()` API
5. ✅ E2E tests prove crash recovery works
6. ✅ State machine unchanged - step stays `in_progress` during yields
7. ✅ No race conditions - atomic persist-then-redispatch ordering

---

## Code References

### Rust Types

* `CheckpointProgress`: `tasker-shared/src/models/core/batch_worker.rs`
* `BatchWorkerInputs`: `tasker-shared/src/models/core/batch_worker.rs`
* `FfiDispatchChannel`: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`
* `CompletionProcessorService`: `tasker-worker/src/worker/handlers/completion_processor.rs`

### Ruby

* `Batchable`: `workers/ruby/lib/tasker_core/step_handler/batchable.rb`
* `EventBridge`: `workers/ruby/lib/tasker_core/event_bridge.rb`

### Documentation

* Full specification: `docs/ticket-specs/TAS-125/checkpoint-plan.md`
* Batch processing guide: `docs/guides/batch-processing.md`
* Worker event systems: `docs/architecture/worker-event-systems.md`

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-125/batchable-handler-checkpoint](https://linear.app/tasker-systems/issue/TAS-125/batchable-handler-checkpoint)
- Identifier: TAS-125
- Status: In Progress
- Priority: Medium
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Rust](https://linear.app/tasker-systems/project/tasker-core-rust-9b5a1c23b7b1). Alpha version of the Tasker Core in Rust
- Created: 2025-12-31T15:02:26.211Z
- Updated: 2026-01-02T17:01:08.507Z

## Comments

- Pete Taylor:

  ## Research Findings from TAS-112

  During the TAS-112 cross-language ergonomics work, we identified a gap in checkpoint persistence for batch processing handlers.

  ### Current State

  * `checkpoint_interval` exists in `BatchWorkerConfig` across all languages (Ruby, Rust, Python, TypeScript)
  * **No language has checkpoint write capability** - this is documented as "Future Enhancement" in Rust examples
  * The orchestration layer has the infrastructure to persist checkpoints (via `last_cursor` in step results), but there's no explicit API for handlers to write intermediate checkpoints during processing

  ### What Works Today

  1. **Final checkpoint**: When a batch worker completes, `last_cursor` is included in the result and persisted
  2. **Retry recovery**: On retry, the worker can read its previous `last_cursor` from the step's stored result
  3. `checkpoint_interval` config: The configuration value exists but is currently unused

  ### Gap Identified

  There's no mechanism for batch workers to persist intermediate checkpoints *during* processing. If a worker processes 10,000 items with `checkpoint_interval: 1000`, it cannot save progress at each 1,000-item boundary. If it fails at item 8,000, recovery starts from 0 (or the previous attempt's final `last_cursor`).

  ### Proposed Solution Direction

  Two possible approaches:

  1. **Implicit checkpointing**: Orchestration layer periodically queries/updates step progress
  2. **Explicit checkpoint API**: Workers call `write_checkpoint(cursor)` to persist progress

  The explicit API approach aligns better with the existing pattern where workers control their own progress tracking.

  ### Reference Files

  * Ruby: `workers/ruby/lib/tasker_core/step_handler/batchable.rb` - `checkpoint_interval` defined but unused
  * Rust: `workers/rust/src/step_handlers/batch_processing_example.rs` - documents "Future Enhancement: checkpoint writing"
  * Python/TypeScript: `BatchWorkerConfig` type includes `checkpoint_interval`

  ### Related

  This was discovered while adding `BatchAggregationScenario.detect()` for TAS-112 cross-language parity.

# TAS-125: Checkpoint Yield Implementation Plan

**Status**: In Progress - Phase 5 Complete ✓
**Author**: Claude (with Pete Taylor)
**Date**: 2026-01-02
**Updated**: 2026-01-06
**Estimated Effort**: 11-16 days
**Related**: [TAS-59](https://linear.app/tasker-systems/issue/TAS-59) (Batch Processing), [TAS-64](https://linear.app/tasker-systems/issue/TAS-64) (Checkpoint Progress), [TAS-112](https://linear.app/tasker-systems/issue/TAS-112) (Cross-Language Ergonomics)

---

## Implementation Progress

| Phase | Status | Notes |
|-------|--------|-------|
| **Phase 1**: Schema & Model Changes | ✅ Complete | Migration, types, SQL queries updated |
| **Phase 2**: Rust Service Modifications | ✅ Complete | CheckpointService, FfiDispatchChannel.checkpoint_yield() |
| **Phase 3**: FFI Bridge Changes | ✅ Complete | Ruby, Python, TypeScript (Bun/Node/Deno) |
| **Phase 3.5**: Validation Gate | ✅ Complete | All tests pass, clippy clean, SQLx cache updated |
| **Phase 4**: Language Handler Updates | ✅ Complete | Batchable mixins, handler result types, context accessors |
| **Phase 5**: Integration & E2E Testing | ✅ Complete | Cross-language E2E tests, error scenarios, unit tests |
| **Phase 6**: Documentation & Tools | 🔲 Not Started | Guides, metrics, DLQ updates |

### Completed Files (Phases 1-4)

**Created:**
- `migrations/20260102000000_add_checkpoint_column.sql` - Database migration
- `tasker-worker/src/worker/services/checkpoint/mod.rs` - Module exports
- `tasker-worker/src/worker/services/checkpoint/service.rs` - CheckpointService
- `tasker-worker/src/worker/services/checkpoint/error.rs` - CheckpointError type

**Modified (Rust Core):**
- `tasker-shared/src/models/core/batch_worker.rs` - Added CheckpointRecord, CheckpointYieldData, BatchWorkerResult
- `tasker-shared/src/models/core/workflow_step.rs` - Added checkpoint field, updated 14+ SQL queries
- `tasker-shared/src/models/core/task.rs` - Updated get_step_by_name query
- `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` - Added checkpoint_yield(), with_checkpoint_support()
- `tasker-worker/src/worker/actors/messages.rs` - Added from_checkpoint_continuation()
- `tasker-worker/src/worker/services/mod.rs` - Added checkpoint module

**Modified (Ruby FFI):**
- `workers/ruby/ext/tasker_core/src/bridge.rs` - checkpoint_yield_step_event FFI function
- `workers/ruby/ext/tasker_core/src/conversions.rs` - convert_ruby_checkpoint_to_yield_data()

**Modified (Python FFI):**
- `workers/python/src/lib.rs` - Registered checkpoint_yield_step_event
- `workers/python/src/bridge.rs` - Added checkpoint_yield_step_event to PythonBridgeHandle
- `workers/python/src/event_dispatch.rs` - checkpoint_yield_step_event function
- `workers/python/src/conversions.rs` - convert_python_checkpoint_to_yield_data()

**Modified (TypeScript FFI):**
- `workers/typescript/src/ffi/types.ts` - Added CheckpointYieldData interface
- `workers/typescript/src/ffi/runtime-interface.ts` - Added checkpointYieldStepEvent to interface
- `workers/typescript/src/ffi/bun-runtime.ts` - FFI symbol and implementation
- `workers/typescript/src/ffi/node-runtime.ts` - FFI symbol and implementation
- `workers/typescript/src/ffi/deno-runtime.ts` - FFI symbol and implementation

**Modified (Ruby Language-Side - Phase 4):**
- `workers/ruby/lib/tasker_core/event_bridge.rb` - Added publish_step_checkpoint_yield() method
- `workers/ruby/lib/tasker_core/types/step_handler_call_result.rb` - Added CheckpointYield result type
- `workers/ruby/lib/tasker_core/step_handler/mixins/batchable.rb` - Added checkpoint_yield() helper
- `workers/ruby/lib/tasker_core/batch_processing/batch_worker_context.rb` - Added checkpoint accessors

**Modified (Python Language-Side - Phase 4):**
- `workers/python/python/tasker_core/batch_processing/batchable.py` - Added checkpoint_yield() method

**Modified (TypeScript Language-Side - Phase 4):**
- `workers/typescript/src/handler/batchable.ts` - Added checkpointYield() method to interface, mixin, and class

### Completed Files (Phase 5)

**E2E Test Files Created:**
- `tests/e2e/ruby/checkpoint_yield_test.rs` - Ruby checkpoint yield E2E tests (3 scenarios)
- `tests/e2e/python/checkpoint_yield_test.rs` - Python checkpoint yield E2E tests (4 scenarios)
- `tests/e2e/typescript/checkpoint_yield_test.rs` - TypeScript checkpoint yield E2E tests (3 scenarios)

**Task Template Fixtures Created:**
- `tests/fixtures/task_templates/ruby/checkpoint_yield_test.yaml` - Ruby E2E task template
- `tests/fixtures/task_templates/python/checkpoint_yield_test.yaml` - Python E2E task template
- `tests/fixtures/task_templates/typescript/checkpoint_yield_test_ts.yaml` - TypeScript E2E task template

**TypeScript Handler & Test Files:**
- `workers/typescript/tests/handlers/step_handlers/checkpoint-handlers.ts` - Checkpoint yield example handlers
- `workers/typescript/tests/handlers/examples/checkpoint_yield/index.ts` - Checkpoint yield exports
- `workers/typescript/tests/handlers/examples/index.ts` - Updated exports
- `workers/typescript/tests/unit/handler/batchable.test.ts` - Comprehensive unit tests including checkpointYield

**E2E Test Scenarios Implemented:**
| Language | Happy Path | Transient Failure Resume | Permanent Failure | Frequent Checkpoints |
|----------|------------|--------------------------|-------------------|---------------------|
| Ruby | ✅ | ✅ | ✅ | - |
| Python | ✅ | ✅ | ✅ | ✅ |
| TypeScript | ✅ | ✅ | ✅ | - |

**Test Fixtures Updated:**
- `tests/common/fast_event_test_helper.rs`
- `tasker-shared/src/events/registry.rs`
- `tasker-shared/src/events/worker_events.rs`
- `tasker-shared/tests/domain_events_test.rs`
- `tasker-worker/src/worker/domain_event_commands.rs`
- `tasker-worker/src/worker/event_publisher.rs`
- `tasker-worker/src/worker/event_router.rs`
- `tasker-worker/src/worker/event_systems/domain_event_system.rs`
- `tasker-worker/src/worker/in_process_event_bus.rs`

---

## Table of Contents

- [Executive Summary](#executive-summary)
- [Design Decision: Handler-Driven Checkpoints](#design-decision-handler-driven-checkpoints)
- [Phase 1: Schema & Model Changes](#phase-1-schema--model-changes)
- [Phase 2: Rust Type Additions](#phase-2-rust-type-additions-tasker-shared)
- [Phase 3: tasker-worker Service Modifications](#phase-3-tasker-worker-service-modifications)
- [Phase 4: FFI Bridge Changes](#phase-4-ffi-bridge-changes-all-languages)
- [Phase 5: Step Hydration & Query Updates](#phase-5-step-hydration--query-updates)
- [Phase 6: Integration & E2E Testing](#phase-6-integration--e2e-testing)
- [Implementation Summary](#implementation-summary)

---

## Executive Summary

The research confirms that `checkpoint_interval` configuration exists across all languages but is **not used for intermediate checkpoint persistence**. Rather than making this configuration drive automatic behavior, we're implementing a **handler-driven checkpoint model** where the handler decides when to yield based on business logic.

The checkpoint-yield pattern requires:

1. **New database column**: `tasker_workflow_steps.checkpoint` (JSONB)
2. **New Rust types**: `CheckpointYieldData`, `BatchWorkerResult` enum
3. **New FFI function**: `checkpoint_yield_step_event()` across all language bridges
4. **Service updates**: `FfiDispatchChannel`, `CompletionProcessorService`
5. **Query updates**: All `WorkflowStep` loading queries must include `checkpoint` column
6. **Remove unused config**: Delete `checkpoint_interval` from all configuration and types

---

## Design Decision: Handler-Driven Checkpoints

### Why Not Configuration-Driven?

The original design included a `checkpoint_interval` configuration that implied automatic checkpointing every N items. However, this approach has fundamental issues:

1. **Business logic leakage**: When to checkpoint depends on the nature of the work being done, not a fixed interval
2. **Handler knows best**: Only the handler understands the cost/benefit of checkpointing at a given point
3. **Flexibility required**: Some handlers may want to checkpoint after expensive operations, others after logical boundaries

### The Handler-Driven Model

Instead of orchestration-driven checkpointing, handlers explicitly call `checkpoint_yield()` when they decide it's appropriate:

```ruby
# Handler decides when to checkpoint based on business logic
def call(context)
  batch_ctx = get_batch_context(context)

  items.each_with_index do |item, index|
    process_item(item)

    # Handler decides: checkpoint after expensive operations or logical boundaries
    if should_checkpoint?(index, item)
      return checkpoint_yield(cursor: index, items_processed: index + 1)
    end
  end

  success(result: final_results)
end
```

### Cleanup: Remove `checkpoint_interval` Configuration

Since `checkpoint_interval` was never functional and we're not making it drive automatic behavior, we remove it entirely to avoid confusion. Configuration that doesn't drive behavior is always problematic.

**Files to modify (remove `checkpoint_interval`):**

| Category | Files |
|----------|-------|
| **Rust Types** | `tasker-shared/src/models/core/task_template/mod.rs` (BatchConfiguration) |
| | `tasker-shared/src/models/core/batch_worker.rs` (BatchMetadata) |
| | `tasker-shared/src/config/orchestration/batch_processing.rs` |
| **Worker Contexts** | `tasker-worker/src/batch_processing/worker_helper.rs` |
| | `workers/ruby/lib/tasker_core/batch_processing/batch_worker_context.rb` |
| | `workers/python/python/tasker_core/types.py` |
| | `workers/typescript/src/types/batch.ts` |
| **TOML Configs** | `config/tasker/base/orchestration.toml` |
| | `config/tasker/environments/*/orchestration.toml` |
| | All `complete-*.toml` and `orchestration-*.toml` files |
| **YAML Fixtures** | `tests/fixtures/task_templates/*/batch_*.yaml` |
| **Documentation** | `docs/guides/batch-processing.md` |
| | `docs/reference/ffi-boundary-types.md` |
| **Handler Examples** | `workers/rust/src/step_handlers/batch_processing_example.rs` |
| | `workers/rust/src/step_handlers/error_injection/checkpoint_and_fail_handler.rs` |

**Note**: This cleanup should be done as the first step in Phase 1, before adding the new checkpoint column. It's a breaking change but safe since no one is using the feature yet.

---

## Phase 1: Schema & Model Changes

### 1.1 Database Migration

**New file**: `migrations/YYYYMMDD_add_checkpoint_column.sql`

```sql
ALTER TABLE tasker_workflow_steps ADD COLUMN checkpoint JSONB;

-- Optional index for checkpoint queries
CREATE INDEX idx_workflow_steps_checkpoint_cursor
ON tasker_workflow_steps ((checkpoint->>'cursor'))
WHERE checkpoint IS NOT NULL;
```

### 1.2 WorkflowStep Model Updates

**Files to modify**:

| File | Lines | Change |
|------|-------|--------|
| `tasker-shared/src/models/core/workflow_step.rs` | 86-104 | Add `checkpoint: Option<serde_json::Value>` field |
| `tasker-shared/src/models/core/workflow_step.rs` | 1411-1436 | Add to `WorkflowStepWithName` struct |

**New field in both structs**:

```rust
#[sqlx(default)]
pub checkpoint: Option<serde_json::Value>,
```

### 1.3 Checkpoint Data Structure

**New type in** `tasker-shared/src/models/core/batch_worker.rs` (after line 376):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointRecord {
    pub cursor: serde_json::Value,
    pub items_processed: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub accumulated_results: Option<serde_json::Value>,
    pub history: Vec<CheckpointHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointHistoryEntry {
    pub cursor: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

---

## Phase 2: Rust Type Additions (tasker-shared)

### 2.1 CheckpointYieldData Type

**File**: `tasker-shared/src/messaging/execution_types.rs` (after `StepExecutionResult` ~line 654)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointYieldData {
    pub step_uuid: Uuid,
    pub cursor: serde_json::Value,
    pub items_processed: u64,
    pub accumulated_results: Option<serde_json::Value>,
}

impl CheckpointYieldData {
    pub fn new(
        step_uuid: Uuid,
        cursor: serde_json::Value,
        items_processed: u64,
        accumulated_results: Option<serde_json::Value>,
    ) -> Self {
        Self { step_uuid, cursor, items_processed, accumulated_results }
    }
}
```

### 2.2 BatchWorkerResult Enum

**File**: `tasker-shared/src/models/core/batch_worker.rs` (after `BatchMetadata` ~line 175)

Following the pattern of `DecisionPointOutcome` and `BatchProcessingOutcome`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchWorkerResult {
    /// Worker completed successfully
    Complete(StepExecutionResult),

    /// Worker yielding at checkpoint for persistence
    CheckpointYield(CheckpointYieldData),
}
```

### 2.3 CheckpointProgress Extensions

**File**: `tasker-shared/src/models/core/batch_worker.rs` (add methods after line 738)

```rust
impl CheckpointProgress {
    /// Extract from dedicated checkpoint column (TAS-125)
    pub fn from_checkpoint_column(
        checkpoint_json: Option<&serde_json::Value>
    ) -> anyhow::Result<Option<CheckpointRecord>> {
        match checkpoint_json {
            Some(json) => serde_json::from_value(json.clone())
                .map(Some)
                .map_err(|e| anyhow::anyhow!("Checkpoint deserialization failed: {}", e)),
            None => Ok(None),
        }
    }
}
```

### 2.4 BatchWorkerContext Checkpoint Accessors

**File**: `tasker-worker/src/batch_processing/worker_helper.rs` (add methods after line 191)

```rust
impl BatchWorkerContext {
    /// Get checkpoint cursor from workflow step (TAS-125)
    pub fn checkpoint_cursor(&self, workflow_step: &WorkflowStepWithName) -> Option<serde_json::Value> {
        workflow_step.checkpoint
            .as_ref()
            .and_then(|cp| cp.get("cursor").cloned())
    }

    /// Get accumulated results from previous checkpoint yield
    pub fn accumulated_results(&self, workflow_step: &WorkflowStepWithName) -> Option<serde_json::Value> {
        workflow_step.checkpoint
            .as_ref()
            .and_then(|cp| cp.get("accumulated_results").cloned())
    }

    /// Check if more items remain to process
    pub fn has_more_items(&self, current_cursor: u64) -> bool {
        current_cursor < self.end_position()
    }
}
```

---

## Phase 3: tasker-worker Service Modifications

### 3.1 FfiDispatchChannel - New `checkpoint_yield()` Method

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` (add after `complete()` ~line 640)

```rust
/// Handle checkpoint yield from FFI handler (TAS-125)
///
/// Unlike complete(), this persists checkpoint and re-dispatches
/// without releasing the step claim.
pub fn checkpoint_yield(&self, event_id: Uuid, checkpoint_data: CheckpointYieldData) -> bool {
    let pending_event = {
        let pending = self.pending_events.read()
            .unwrap_or_else(|p| p.into_inner());
        // NOTE: Do NOT remove - step stays pending for re-dispatch
        pending.get(&event_id).cloned()
    };

    if let Some(pending) = pending_event {
        let result = self.config.runtime_handle.block_on(async {
            self.handle_checkpoint_yield_async(&pending.event, checkpoint_data).await
        });

        match result {
            Ok(()) => {
                debug!(event_id = %event_id, "Checkpoint persisted, step re-dispatched");
                true
            }
            Err(e) => {
                error!(event_id = %event_id, error = %e, "Checkpoint yield failed");
                self.send_checkpoint_failure(event_id, &pending.event, e);
                false
            }
        }
    } else {
        warn!(event_id = %event_id, "Checkpoint yield for unknown event");
        false
    }
}

async fn handle_checkpoint_yield_async(
    &self,
    event: &FfiStepEvent,
    checkpoint_data: CheckpointYieldData,
) -> Result<(), CheckpointError> {
    // 1. Persist checkpoint atomically
    self.checkpoint_service
        .persist_checkpoint(event.step_uuid, &checkpoint_data)
        .await?;

    // 2. Re-dispatch step (stays claimed, in_progress)
    self.dispatch_sender
        .send(DispatchHandlerMessage::from_checkpoint_continuation(event))
        .await
        .map_err(|_| CheckpointError::RedispatchFailed)?;

    Ok(())
}
```

### 3.2 New CheckpointService

**New file**: `tasker-worker/src/worker/services/checkpoint_service.rs`

```rust
pub struct CheckpointService {
    db_pool: PgPool,
}

impl CheckpointService {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    pub async fn persist_checkpoint(
        &self,
        step_uuid: Uuid,
        data: &CheckpointYieldData,
    ) -> Result<(), CheckpointError> {
        let checkpoint_record = CheckpointRecord {
            cursor: data.cursor.clone(),
            items_processed: data.items_processed,
            timestamp: chrono::Utc::now(),
            accumulated_results: data.accumulated_results.clone(),
            history: vec![], // History appended via SQL
        };

        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps
            SET checkpoint = jsonb_set(
                COALESCE(checkpoint, '{"history": []}'),
                '{history}',
                COALESCE(checkpoint->'history', '[]'::jsonb) ||
                    jsonb_build_array(jsonb_build_object(
                        'cursor', $2::jsonb,
                        'timestamp', to_jsonb(now())
                    ))
            ) || $3::jsonb,
            updated_at = NOW()
            WHERE workflow_step_uuid = $1::uuid
            "#,
            step_uuid,
            &data.cursor,
            &serde_json::to_value(&checkpoint_record)?
        )
        .execute(&self.db_pool)
        .await?;

        Ok(())
    }
}
```

### 3.3 CompletionProcessorService Updates

**File**: `tasker-worker/src/worker/handlers/completion_processor.rs` (modify `process_completion` ~line 131)

```rust
async fn process_completion(&self, result: StepExecutionResult) {
    // TAS-125: Check if this is a checkpoint yield
    if let Some(_checkpoint_marker) = result.metadata.custom.get("__checkpoint_yield") {
        // Handle as checkpoint yield - should not reach here normally
        // (checkpoint yields go through separate FFI path)
        warn!("Checkpoint yield received via completion channel - unexpected");
        return;
    }

    // Existing: send to orchestration
    self.ffi_completion_service.send_step_result(result.clone()).await
}
```

### 3.4 DispatchHandlerMessage Extension

**File**: `tasker-worker/src/worker/actors/messages.rs` (add method ~line 204)

```rust
impl DispatchHandlerMessage {
    /// Create continuation message from checkpoint yield (TAS-125)
    pub fn from_checkpoint_continuation(event: &FfiStepEvent) -> Self {
        Self {
            event_id: Uuid::new_v4(), // New event ID for this continuation
            step_uuid: event.step_uuid,
            task_uuid: event.task_uuid,
            task_sequence_step: event.execution_event.payload.task_sequence_step.clone(),
            correlation_id: event.correlation_id,
            trace_context: event.trace_id.as_ref().map(|t| TraceContext {
                trace_id: t.clone(),
                span_id: event.span_id.clone().unwrap_or_default(),
            }),
        }
    }
}
```

---

## Phase 4: FFI Bridge Changes (All Languages)

### 4.1 Ruby FFI Bridge

**File**: `workers/ruby/ext/tasker_core/src/bridge.rs` (add after `complete_step_event` ~line 195)

```rust
pub fn checkpoint_yield_step_event(event_id_str: String, checkpoint_data: Value) -> Result<bool, Error> {
    let event_id = Uuid::parse_str(&event_id_str)
        .map_err(|e| Error::new(
            magnus::exception::arg_error(),
            format!("Invalid event ID: {}", e),
        ))?;

    let checkpoint_yield_data = convert_ruby_to_checkpoint_yield_data(checkpoint_data)?;

    let bridge_handle = BRIDGE_HANDLE.lock()
        .map_err(|_| Error::new(magnus::exception::runtime_error(), "Failed to lock bridge"))?;

    if let Some(ref handle) = *bridge_handle {
        Ok(handle.checkpoint_yield(event_id, checkpoint_yield_data))
    } else {
        Err(Error::new(magnus::exception::runtime_error(), "Bridge not initialized"))
    }
}

// Register in init_bridge() after line 278:
module.define_singleton_method("checkpoint_yield_step_event", function!(checkpoint_yield_step_event, 2))?;
```

**File**: `workers/ruby/ext/tasker_core/src/conversions.rs` (add conversion function)

```rust
pub fn convert_ruby_to_checkpoint_yield_data(value: Value) -> Result<CheckpointYieldData, Error> {
    let hash = RHash::try_convert(value)?;

    let step_uuid_str: String = hash.fetch::<_, String>(Symbol::new("step_uuid"))?;
    let step_uuid = Uuid::parse_str(&step_uuid_str)
        .map_err(|e| Error::new(magnus::exception::arg_error(), format!("Invalid step UUID: {}", e)))?;

    let cursor: serde_json::Value = convert_ruby_to_json(hash.fetch::<_, Value>(Symbol::new("cursor"))?)?;
    let items_processed: u64 = hash.fetch::<_, u64>(Symbol::new("items_processed"))?;
    let accumulated_results: Option<serde_json::Value> = hash
        .get(Symbol::new("accumulated_results"))
        .and_then(|v| convert_ruby_to_json(v).ok());

    Ok(CheckpointYieldData::new(step_uuid, cursor, items_processed, accumulated_results))
}
```

### 4.2 Ruby EventBridge

**File**: `workers/ruby/lib/tasker_core/event_bridge.rb` (add after `publish_step_completion` ~line 216)

```ruby
def publish_step_checkpoint_yield(checkpoint_data)
  return unless active?

  logger.debug "Sending checkpoint yield to Rust: #{checkpoint_data[:event_id]}"

  validate_checkpoint_yield!(checkpoint_data)

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
  raise ArgumentError, "Missing checkpoint yield fields: #{missing.join(', ')}" if missing.any?
end
```

### 4.3 Ruby StepHandlerCallResult

**File**: `workers/ruby/lib/tasker_core/types/step_handler_call_result.rb` (add after `Error` ~line 72)

```ruby
class CheckpointYield < Dry::Struct
  attribute :cursor, Types::Any
  attribute :items_processed, Types::Integer
  attribute :accumulated_results, Types::Any.optional

  def checkpoint?
    true
  end

  def success?
    false  # Not a final result
  end
end

# Add factory method in class << self block:
def checkpoint_yield(cursor:, items_processed:, accumulated_results: nil)
  CheckpointYield.new(
    cursor: cursor,
    items_processed: items_processed,
    accumulated_results: accumulated_results
  )
end
```

### 4.4 Ruby Batchable Mixin

**File**: `workers/ruby/lib/tasker_core/step_handler/mixins/batchable.rb` (add after `batch_worker_success` ~line 331)

```ruby
# Yield checkpoint for batch processing (TAS-125)
#
# @param cursor [Integer, String, Hash] Current position in dataset
# @param items_processed [Integer] Total items processed so far
# @param accumulated_results [Hash, nil] Optional partial aggregations
# @return [StepHandlerCallResult::CheckpointYield]
def checkpoint_yield(cursor:, items_processed:, accumulated_results: nil)
  StepHandlerCallResult.checkpoint_yield(
    cursor: cursor,
    items_processed: items_processed,
    accumulated_results: accumulated_results
  )
end
```

### 4.5 Ruby BatchWorkerContext

**File**: `workers/ruby/lib/tasker_core/batch_processing/batch_worker_context.rb` (add after `no_op?` ~line 131)

```ruby
# Get checkpoint cursor from workflow step (TAS-125)
# @return [Integer, String, Hash, nil] Last persisted cursor position
def checkpoint_cursor
  @workflow_step.checkpoint&.dig('cursor')
end

# Get accumulated results from previous checkpoint yield
# @return [Hash, nil] Partial aggregations from previous yields
def accumulated_results
  @workflow_step.checkpoint&.dig('accumulated_results')
end

# Check if checkpoint exists
# @return [Boolean]
def has_checkpoint?
  @workflow_step.checkpoint.present?
end
```

### 4.6 Python FFI Bridge

**File**: `workers/python/src/bridge.rs` (add similar to Ruby)

```rust
#[pyfunction]
pub fn checkpoint_yield_step_event(
    py: Python,
    event_id_str: String,
    checkpoint_data: PyObject,
) -> PyResult<bool> {
    let event_id = Uuid::parse_str(&event_id_str)
        .map_err(|e| PyValueError::new_err(format!("Invalid event ID: {}", e)))?;

    let checkpoint_yield_data = convert_python_to_checkpoint_yield_data(py, checkpoint_data)?;

    // Get bridge handle and call checkpoint_yield
    // Similar pattern to complete_step_event
}
```

### 4.7 Python Batchable Mixin

**File**: `workers/python/python/tasker_core/batch_processing/batchable.py` (add method)

```python
def checkpoint_yield(
    self,
    cursor: int | str | dict[str, Any],
    items_processed: int,
    accumulated_results: dict[str, Any] | None = None,
) -> CheckpointYieldResult:
    """
    Yield checkpoint for batch processing (TAS-125).

    This signals the orchestrator to persist progress and re-invoke
    the handler with the updated checkpoint context.

    Args:
        cursor: Current position in dataset
        items_processed: Total items processed so far
        accumulated_results: Optional partial aggregations to carry forward

    Returns:
        CheckpointYieldResult for the FFI bridge
    """
    return CheckpointYieldResult(
        cursor=cursor,
        items_processed=items_processed,
        accumulated_results=accumulated_results,
    )
```

### 4.8 TypeScript FFI Bridge

**File**: `workers/typescript/src-rust/bridge.rs` (add similar pattern)

**File**: `workers/typescript/src/handler/batchable.ts` (add method)

```typescript
checkpointYield(
  cursor: number | string | Record<string, unknown>,
  itemsProcessed: number,
  accumulatedResults?: Record<string, unknown>,
): CheckpointYieldResult {
  return {
    type: 'checkpoint_yield',
    cursor,
    itemsProcessed,
    accumulatedResults,
  };
}
```

---

## Phase 5: Step Hydration & Query Updates

### 5.1 WorkflowStepWithName Query Updates

**File**: `tasker-shared/src/models/core/workflow_step.rs`

The following queries need `ws.checkpoint` added to their SELECT clauses:

| Method | Lines | Change |
|--------|-------|--------|
| `find_by_id` (WorkflowStepWithName) | 1481-1499 | Add `ws.checkpoint` |
| `find_by_ids` | 1453-1475 | Add `ws.checkpoint` |
| `find_by_id` (WorkflowStep) | 190-205 | Add `checkpoint` |
| `completed()` scope | 623-643 | Add `checkpoint` |
| `failed()` scope | 645-666 | Add `checkpoint` |
| `pending()` scope | 668-689 | Add `checkpoint` |
| `by_current_state()` | 691-738 | Add `checkpoint` |
| `completed_since()` | 741-766 | Add `checkpoint` |
| `failed_since()` | 768-794 | Add `checkpoint` |
| `for_tasks_since()` | 796-819 | Add `checkpoint` |

**Example updated query** (`find_by_id` at lines 1481-1499):

```rust
pub async fn find_by_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<WorkflowStepWithName>, sqlx::Error> {
    let step = sqlx::query_as!(
        WorkflowStepWithName,
        r#"
        SELECT ws.workflow_step_uuid, ws.task_uuid, ws.named_step_uuid, ns.name as name,
               COALESCE(ws.inputs->>'__template_step_name', ns.name) as "template_step_name!",
               ws.retryable, ws.max_attempts,
               ws.in_process, ws.processed, ws.processed_at, ws.attempts, ws.last_attempted_at,
               ws.backoff_request_seconds, ws.inputs, ws.results,
               ws.checkpoint,  -- TAS-125: NEW
               ws.skippable, ws.created_at, ws.updated_at
        FROM tasker_workflow_steps ws
        INNER JOIN tasker_named_steps ns ON ws.named_step_uuid = ns.named_step_uuid
        WHERE workflow_step_uuid = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(step)
}
```

### 5.2 ResetForRetry Update

**File**: `tasker-shared/src/models/core/workflow_step.rs` (modify existing `reset_for_retry`)

```rust
pub async fn reset_for_retry(
    &mut self,
    pool: &PgPool,
    reset_checkpoint: bool,  // TAS-125: New parameter
) -> Result<(), sqlx::Error> {
    if reset_checkpoint {
        // Full reset - clear checkpoint (restart from beginning)
        sqlx::query!(
            r#"
            UPDATE tasker_workflow_steps
            SET in_process = false,
                processed = false,
                processed_at = NULL,
                results = NULL,
                checkpoint = NULL,  -- TAS-125: Clear checkpoint
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
        // Default: Preserve checkpoint (continue from last position)
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

        // checkpoint field preserved
    }

    Ok(())
}
```

### 5.3 SQLx Query Cache Update

After all query modifications:

```bash
DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test \
cargo sqlx prepare --workspace -- --all-targets --all-features

git add .sqlx/
```

---

## Phase 6: Integration & E2E Testing

### 6.1 Unit Tests (Phase 3.5 Validation Gate)

These tests are added at the Phase 3.5 validation gate once core types exist.

**File**: `tasker-shared/src/models/core/batch_worker.rs` - Add tests

```rust
#[cfg(test)]
mod checkpoint_tests {
    use super::*;

    #[test]
    fn test_checkpoint_record_serialization() {
        let checkpoint = CheckpointRecord {
            cursor: json!(7000),
            items_processed: 7000,
            timestamp: chrono::Utc::now(),
            accumulated_results: Some(json!({"total": 100.0})),
            history: vec![
                CheckpointHistoryEntry { cursor: json!(1000), timestamp: chrono::Utc::now() },
            ],
        };

        let json = serde_json::to_value(&checkpoint).unwrap();
        assert!(json.get("history").is_some());
        assert_eq!(json["cursor"], 7000);
    }

    #[test]
    fn test_checkpoint_yield_data_creation() {
        let data = CheckpointYieldData::new(
            Uuid::new_v4(),
            json!(5000),
            5000,
            Some(json!({"count": 50})),
        );

        assert_eq!(data.items_processed, 5000);
        assert_eq!(data.cursor, json!(5000));
    }

    #[test]
    fn test_batch_worker_result_serialization() {
        let yield_data = CheckpointYieldData::new(Uuid::new_v4(), json!(100), 100, None);
        let result = BatchWorkerResult::CheckpointYield(yield_data);

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["type"], "checkpoint_yield");
    }
}
```

### 6.2 Rust Integration Tests (Phase 6)

**File**: `tasker-worker/tests/checkpoint_yield_tests.rs`

```rust
#[tokio::test]
async fn test_checkpoint_yield_persistence() {
    // Setup: Create batch worker step with 1000 items, checkpoint_interval=200
    let (pool, step) = setup_batch_worker_step(1000, 200).await;

    let checkpoint_data = CheckpointYieldData::new(
        step.workflow_step_uuid,
        json!(200),
        200,
        Some(json!({"partial_sum": 1000})),
    );

    // Act: Persist checkpoint
    let service = CheckpointService::new(pool.clone());
    service.persist_checkpoint(step.workflow_step_uuid, &checkpoint_data).await.unwrap();

    // Assert: Checkpoint persisted correctly
    let updated_step = WorkflowStepWithName::find_by_id(&pool, step.workflow_step_uuid)
        .await.unwrap().unwrap();

    let checkpoint = updated_step.checkpoint.unwrap();
    assert_eq!(checkpoint["cursor"], 200);
    assert_eq!(checkpoint["items_processed"], 200);
    assert!(checkpoint["history"].as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn test_checkpoint_yield_cycle() {
    // Test full checkpoint-yield-resume cycle
    let (pool, step) = setup_batch_worker_step(1000, 200).await;

    // First invocation: processes 0-200, yields
    let context1 = get_handler_context(&step).await;
    assert!(context1.checkpoint_cursor().is_none());

    // Simulate checkpoint yield
    persist_checkpoint(&pool, &step, 200, Some(json!({"count": 200}))).await;

    // Second invocation should have checkpoint
    let context2 = get_handler_context(&step).await;
    assert_eq!(context2.checkpoint_cursor(), Some(json!(200)));
    assert_eq!(context2.accumulated_results(), Some(json!({"count": 200})));
}

#[tokio::test]
async fn test_crash_recovery_from_checkpoint() {
    let (pool, step) = setup_batch_worker_step(1000, 200).await;

    // Process to checkpoint 600
    for cursor in [200, 400, 600] {
        persist_checkpoint(&pool, &step, cursor, None).await;
    }

    // Simulate crash by resetting in_process without clearing checkpoint
    reset_step_for_staleness(&pool, &step).await;

    // Re-invoke should resume from 600
    let context = get_handler_context(&step).await;
    assert_eq!(context.checkpoint_cursor(), Some(json!(600)));
}

#[tokio::test]
async fn test_reset_for_retry_preserves_checkpoint() {
    let (pool, step) = setup_batch_worker_step(1000, 200).await;
    persist_checkpoint(&pool, &step, 500, None).await;

    // Default reset preserves checkpoint
    step.reset_for_retry(&pool, false).await.unwrap();

    let reloaded = WorkflowStepWithName::find_by_id(&pool, step.workflow_step_uuid)
        .await.unwrap().unwrap();
    assert!(reloaded.checkpoint.is_some());
    assert_eq!(reloaded.checkpoint.unwrap()["cursor"], 500);
}

#[tokio::test]
async fn test_reset_for_retry_clears_checkpoint() {
    let (pool, step) = setup_batch_worker_step(1000, 200).await;
    persist_checkpoint(&pool, &step, 500, None).await;

    // Explicit reset clears checkpoint
    step.reset_for_retry(&pool, true).await.unwrap();

    let reloaded = WorkflowStepWithName::find_by_id(&pool, step.workflow_step_uuid)
        .await.unwrap().unwrap();
    assert!(reloaded.checkpoint.is_none());
}
```

### 6.3 Ruby Integration Tests (Phase 4/6)

**File**: `workers/ruby/spec/integration/checkpoint_yield_spec.rb`

```ruby
RSpec.describe 'Checkpoint Yield Integration' do
  describe 'checkpoint persistence' do
    it 'persists checkpoint and re-invokes handler' do
      step = create_batch_worker_step(items: 1000, checkpoint_interval: 200)

      result = handler.call(step_context(step))
      expect(result).to be_a(StepHandlerCallResult::CheckpointYield)
      expect(result.cursor).to eq(200)

      checkpoint = fetch_step_checkpoint(step.uuid)
      expect(checkpoint['cursor']).to eq(200)
      expect(checkpoint['history'].length).to eq(1)
    end

    it 'accumulates results across checkpoints' do
      step = create_batch_worker_step(items: 600, checkpoint_interval: 200)

      # First yield
      result1 = handler.call(step_context(step))
      expect(result1.accumulated_results).to eq({ 'count' => 200 })

      # Second yield (with accumulated from first)
      result2 = handler.call(step_context_with_checkpoint(step, result1))
      expect(result2.accumulated_results).to eq({ 'count' => 400 })

      # Final completion
      result3 = handler.call(step_context_with_checkpoint(step, result2))
      expect(result3).to be_a(StepHandlerCallResult::Success)
      expect(result3.result['final_count']).to eq(600)
    end
  end

  describe 'crash recovery' do
    it 'resumes from last checkpoint after crash' do
      step = create_batch_worker_step(items: 1000, checkpoint_interval: 200)

      # Process to checkpoint 600
      3.times { simulate_checkpoint_yield(step, 200) }

      # Simulate crash
      simulate_step_staleness(step)

      # Resume should start from 600
      context = step_context(step)
      expect(context.batch_context.checkpoint_cursor).to eq(600)
    end
  end
end
```

### 6.4 Python Integration Tests (Phase 4/6)

**File**: `workers/python/tests/integration/test_checkpoint_yield.py`

```python
import pytest
from tasker_core.batch_processing import Batchable, BatchWorkerContext
from tasker_core.types import CheckpointYieldResult, StepHandlerCallResult


class TestCheckpointYieldIntegration:
    """Integration tests for checkpoint yield functionality."""

    def test_checkpoint_persistence_and_reinvocation(self, batch_step_factory):
        """Test that checkpoint persists and handler is re-invoked."""
        step = batch_step_factory(items=1000, checkpoint_interval=200)
        handler = TestBatchHandler()

        result = handler.call(step_context(step))

        assert isinstance(result, CheckpointYieldResult)
        assert result.cursor == 200
        assert result.items_processed == 200

        checkpoint = fetch_step_checkpoint(step.uuid)
        assert checkpoint["cursor"] == 200
        assert len(checkpoint["history"]) == 1

    def test_accumulated_results_across_checkpoints(self, batch_step_factory):
        """Test that accumulated results carry forward across yields."""
        step = batch_step_factory(items=600, checkpoint_interval=200)
        handler = AccumulatingBatchHandler()

        # First yield
        result1 = handler.call(step_context(step))
        assert result1.accumulated_results == {"count": 200}

        # Second yield (with accumulated from first)
        result2 = handler.call(step_context_with_checkpoint(step, result1))
        assert result2.accumulated_results == {"count": 400}

        # Final completion
        result3 = handler.call(step_context_with_checkpoint(step, result2))
        assert isinstance(result3, StepHandlerCallResult.Success)
        assert result3.result["final_count"] == 600

    def test_crash_recovery_resumes_from_checkpoint(self, batch_step_factory):
        """Test that crash recovery starts from last checkpoint."""
        step = batch_step_factory(items=1000, checkpoint_interval=200)

        # Process to checkpoint 600
        for _ in range(3):
            simulate_checkpoint_yield(step, 200)

        # Simulate crash
        simulate_step_staleness(step)

        # Resume should start from 600
        context = step_context(step)
        assert context.batch_context.checkpoint_cursor == 600


class TestBatchHandler(Batchable):
    """Test handler that yields at checkpoint boundaries."""

    def call(self, context):
        batch_ctx = self.get_batch_context(context)
        start = batch_ctx.checkpoint_cursor or batch_ctx.start_cursor

        # Process one checkpoint interval
        end = min(start + 200, batch_ctx.end_cursor)
        items_processed = end - batch_ctx.start_cursor

        if end < batch_ctx.end_cursor:
            return self.checkpoint_yield(
                cursor=end,
                items_processed=items_processed,
            )
        else:
            return self.success(result={"completed": True})
```

### 6.5 TypeScript Integration Tests (Phase 4/6)

**File**: `workers/typescript/tests/integration/checkpoint-yield.test.ts`

```typescript
import { describe, it, expect, beforeEach } from 'bun:test';
import { Batchable, BatchWorkerContext } from '../../src/handler/batchable';
import { CheckpointYieldResult, StepHandlerCallResult } from '../../src/types';

describe('Checkpoint Yield Integration', () => {
  describe('checkpoint persistence', () => {
    it('persists checkpoint and re-invokes handler', async () => {
      const step = await createBatchWorkerStep({ items: 1000, checkpointInterval: 200 });
      const handler = new TestBatchHandler();

      const result = await handler.call(stepContext(step));

      expect(result.type).toBe('checkpoint_yield');
      expect((result as CheckpointYieldResult).cursor).toBe(200);
      expect((result as CheckpointYieldResult).itemsProcessed).toBe(200);

      const checkpoint = await fetchStepCheckpoint(step.uuid);
      expect(checkpoint.cursor).toBe(200);
      expect(checkpoint.history.length).toBe(1);
    });

    it('accumulates results across checkpoints', async () => {
      const step = await createBatchWorkerStep({ items: 600, checkpointInterval: 200 });
      const handler = new AccumulatingBatchHandler();

      // First yield
      const result1 = await handler.call(stepContext(step));
      expect((result1 as CheckpointYieldResult).accumulatedResults).toEqual({ count: 200 });

      // Second yield
      const result2 = await handler.call(stepContextWithCheckpoint(step, result1));
      expect((result2 as CheckpointYieldResult).accumulatedResults).toEqual({ count: 400 });

      // Final completion
      const result3 = await handler.call(stepContextWithCheckpoint(step, result2));
      expect(result3.type).toBe('success');
      expect((result3 as StepHandlerCallResult.Success).result.finalCount).toBe(600);
    });
  });

  describe('crash recovery', () => {
    it('resumes from last checkpoint after crash', async () => {
      const step = await createBatchWorkerStep({ items: 1000, checkpointInterval: 200 });

      // Process to checkpoint 600
      for (let i = 0; i < 3; i++) {
        await simulateCheckpointYield(step, 200);
      }

      // Simulate crash
      await simulateStepStaleness(step);

      // Resume should start from 600
      const context = await stepContext(step);
      expect(context.batchContext.checkpointCursor).toBe(600);
    });
  });
});

class TestBatchHandler extends Batchable {
  async call(context: HandlerContext): Promise<StepHandlerCallResult> {
    const batchCtx = this.getBatchContext(context);
    const start = batchCtx.checkpointCursor ?? batchCtx.startCursor;

    // Process one checkpoint interval
    const end = Math.min(start + 200, batchCtx.endCursor);
    const itemsProcessed = end - batchCtx.startCursor;

    if (end < batchCtx.endCursor) {
      return this.checkpointYield(end, itemsProcessed);
    } else {
      return this.success({ completed: true });
    }
  }
}
```

### 6.6 Error Scenario Tests (Phase 6)

```rust
#[tokio::test]
async fn test_checkpoint_persist_failure_triggers_retry() {
    // Setup with failing database
    let (pool, step) = setup_batch_worker_step(1000, 200).await;

    // Inject DB failure
    inject_db_failure(&pool).await;

    // Attempt checkpoint yield
    let result = ffi_channel.checkpoint_yield(event_id, checkpoint_data);

    // Should fail and trigger retryable error
    assert!(!result);

    let step_state = get_step_state(&pool, step.workflow_step_uuid).await;
    assert_eq!(step_state, StepState::WaitingForRetry);
}

#[tokio::test]
async fn test_redispatch_failure_preserves_checkpoint() {
    // Setup
    let (pool, step) = setup_batch_worker_step(1000, 200).await;

    // Persist checkpoint successfully
    persist_checkpoint(&pool, &step, 200, None).await;

    // Inject dispatch channel failure
    inject_dispatch_failure().await;

    // Attempt re-dispatch
    let result = ffi_channel.checkpoint_yield(event_id, checkpoint_data);

    // Checkpoint should be preserved even though re-dispatch failed
    let checkpoint = get_step_checkpoint(&pool, step.workflow_step_uuid).await;
    assert_eq!(checkpoint["cursor"], 200);

    // Staleness detection will eventually retry from this checkpoint
}
```

### 6.7 Performance Tests (Phase 6)

```rust
#[tokio::test]
async fn test_checkpoint_performance() {
    let (pool, step) = setup_batch_worker_step(10000, 100).await;

    let start = std::time::Instant::now();

    // Simulate 100 checkpoint yields
    for i in 1..=100 {
        let checkpoint_data = CheckpointYieldData::new(
            step.workflow_step_uuid,
            json!(i * 100),
            (i * 100) as u64,
            Some(json!({"iteration": i})),
        );
        service.persist_checkpoint(step.workflow_step_uuid, &checkpoint_data).await.unwrap();
    }

    let elapsed = start.elapsed();

    // Should complete 100 checkpoints in under 5 seconds
    assert!(elapsed.as_secs() < 5, "Checkpoint persistence too slow: {:?}", elapsed);

    // Verify history growth
    let final_step = WorkflowStepWithName::find_by_id(&pool, step.workflow_step_uuid)
        .await.unwrap().unwrap();
    let history_len = final_step.checkpoint.unwrap()["history"].as_array().unwrap().len();
    assert_eq!(history_len, 100);
}
```

---

## Implementation Summary

### Files to Create

| File | Purpose |
|------|---------|
| `migrations/YYYYMMDD_add_checkpoint_column.sql` | Database migration |
| `tasker-worker/src/worker/services/checkpoint_service.rs` | Checkpoint persistence service |
| `tasker-worker/tests/checkpoint_yield_tests.rs` | Rust integration tests |
| `workers/ruby/spec/integration/checkpoint_yield_spec.rb` | Ruby integration tests |
| `workers/python/tests/integration/test_checkpoint_yield.py` | Python integration tests |
| `workers/typescript/tests/integration/checkpoint-yield.test.ts` | TypeScript integration tests |

### Files to Modify (Remove `checkpoint_interval`)

| Category | Files | Changes |
|----------|-------|---------|
| **Rust Types** | `tasker-shared/src/models/core/task_template/mod.rs` | Remove from `BatchConfiguration` |
| | `tasker-shared/src/models/core/batch_worker.rs` | Remove from `BatchMetadata` |
| | `tasker-shared/src/config/orchestration/batch_processing.rs` | Remove config field |
| **Worker Contexts** | `tasker-worker/src/batch_processing/worker_helper.rs` | Remove accessor |
| | `workers/ruby/lib/tasker_core/batch_processing/batch_worker_context.rb` | Remove accessor |
| | `workers/python/python/tasker_core/types.py` | Remove from types |
| | `workers/typescript/src/types/batch.ts` | Remove from types |
| **Config Files** | `config/tasker/base/orchestration.toml` | Remove field |
| | `config/tasker/environments/*/orchestration.toml` | Remove field |
| **YAML Fixtures** | `tests/fixtures/task_templates/*/batch_*.yaml` | Remove field |
| **Documentation** | `docs/guides/batch-processing.md` | Update docs |

### Files to Modify (Add Checkpoint Functionality)

| Crate/Package | File | Changes |
|---------------|------|---------|
| **tasker-shared** | `src/models/core/workflow_step.rs` | Add `checkpoint` field + update ~10 queries |
| | `src/models/core/batch_worker.rs` | Add `CheckpointRecord`, `CheckpointYieldData`, `BatchWorkerResult` |
| | `src/messaging/execution_types.rs` | Add `CheckpointYieldData` type |
| **tasker-worker** | `src/worker/handlers/ffi_dispatch_channel.rs` | Add `checkpoint_yield()` method |
| | `src/worker/handlers/completion_processor.rs` | Branch on checkpoint yield detection |
| | `src/worker/actors/messages.rs` | Add `from_checkpoint_continuation()` |
| | `src/batch_processing/worker_helper.rs` | Add checkpoint accessors |
| **workers/ruby** | `ext/tasker_core/src/bridge.rs` | Add `checkpoint_yield_step_event` FFI |
| | `ext/tasker_core/src/conversions.rs` | Add checkpoint conversion |
| | `lib/tasker_core/event_bridge.rb` | Add `publish_step_checkpoint_yield` |
| | `lib/tasker_core/types/step_handler_call_result.rb` | Add `CheckpointYield` type |
| | `lib/tasker_core/step_handler/mixins/batchable.rb` | Add `checkpoint_yield()` helper |
| | `lib/tasker_core/batch_processing/batch_worker_context.rb` | Add checkpoint accessors |
| **workers/python** | `src/bridge.rs` | Add `checkpoint_yield_step_event` FFI |
| | `python/tasker_core/types.py` | Add `CheckpointYieldResult` type |
| | `python/tasker_core/batch_processing/batchable.py` | Add `checkpoint_yield()` method |
| **workers/typescript** | `src-rust/bridge.rs` | Add `checkpoint_yield_step_event` FFI |
| | `src/types/batch.ts` | Add `CheckpointYieldResult` type |
| | `src/handler/batchable.ts` | Add `checkpointYield()` method |
| **workers/rust** | `src/step_handlers/batch_processing_example.rs` | Update to use checkpoint yields |

### Phased Implementation Order

| Phase | Days | Focus |
|-------|------|-------|
| **1** | 2-3 | Remove `checkpoint_interval` + Schema + Core Types (cleanup, migration, `CheckpointRecord`, `CheckpointYieldData`, model updates) |
| **2** | 2-3 | Rust Handler Support (`CheckpointService`, `FfiDispatchChannel.checkpoint_yield()`, re-dispatch) |
| **3** | 3-4 | FFI Bridge Extension (Ruby/Python/TypeScript bindings, conversion functions) |
| **3.5** | 1 | **Validation Gate**: Unit tests updated for new types/structures, compilation verified |
| **4** | 2-3 | Language-Specific Handler Updates (Ruby, Python, TypeScript, Rust examples + tests) |
| **5** | 1-2 | Documentation & Operator Tools (guides, metrics, DLQ updates) |
| **6** | 2-3 | Integration & E2E Testing (net-new tests, error scenarios, performance validation) |

**Total**: 11-17 days

### Testing Strategy

**Phases 1-3**: Minimal test updates - only what's needed for compilation. No validation gate until core infrastructure is complete.

**Phase 3.5 (Validation Gate)**: After FFI bridges are in place:
- Update unit tests for `CheckpointRecord`, `CheckpointYieldData`, `BatchWorkerResult`
- Update struct tests to account for new `checkpoint` field
- Verify all Rust crates compile with `--all-features`
- SQLx query cache updated and verified

**Phase 4**: Language-specific tests updated as each language is completed:
- **Ruby**: `spec/integration/checkpoint_yield_spec.rb` - full integration coverage
- **Python**: `tests/integration/test_checkpoint_yield.py` - pytest-based integration tests
- **TypeScript**: `tests/integration/checkpoint-yield.test.ts` - Bun test runner
- **Rust**: `src/step_handlers/batch_processing_example.rs` updated with checkpoint usage

**Phase 6**: Integration & E2E:
- Cross-language checkpoint round-trip tests
- Crash recovery scenarios
- Error injection tests (persist failure, re-dispatch failure)
- Performance benchmarks (100 checkpoints under 5s target)

### Key Invariants to Maintain

1. **Checkpoint-persist-then-redispatch ordering** - Never lose progress
2. **Step remains `in_progress` during yield cycle** - No visibility timeout issues
3. **State machine untouched** - Only Success/Failure trigger transitions
4. **Atomic checkpoint updates** - Single DB transaction per yield
5. **Backward compatible** - Steps without checkpoint work unchanged
6. **Handler-driven checkpoints** - No automatic checkpointing based on configuration

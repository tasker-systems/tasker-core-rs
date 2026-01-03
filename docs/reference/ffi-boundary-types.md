# FFI Boundary Types Reference

> TAS-112/TAS-123: Cross-language type harmonization for Rust ↔ Python/TypeScript boundaries

This document defines the canonical FFI boundary types that cross the Rust orchestration layer
and the Python/TypeScript worker implementations. These types are critical for correct
serialization/deserialization between languages.

## Overview

The tasker-core system uses FFI (Foreign Function Interface) to integrate Rust orchestration
with Python and TypeScript step handlers. Data crosses this boundary via JSON serialization.
These types must remain consistent across all three languages.

**Source of Truth**: Rust types in `tasker-shared/src/messaging/execution_types.rs` and
`tasker-shared/src/models/core/batch_worker.rs`.

## Type Mapping

| Rust Type | Python Type | TypeScript Type |
|-----------|-------------|-----------------|
| `CursorConfig` | `RustCursorConfig` | `RustCursorConfig` |
| `BatchProcessingOutcome` | `BatchProcessingOutcome` | `BatchProcessingOutcome` |
| `BatchWorkerInputs` | `RustBatchWorkerInputs` | `RustBatchWorkerInputs` |
| `BatchMetadata` | `BatchMetadata` | `BatchMetadata` |
| `FailureStrategy` | `FailureStrategy` | `FailureStrategy` |

## CursorConfig

Cursor configuration for a single batch's position and range.

### Flexible Cursor Types

Unlike simple integer cursors, `RustCursorConfig` supports flexible cursor values:

- **Integer** for record IDs: `123`
- **String** for timestamps: `"2025-11-01T00:00:00Z"`
- **Object** for composite keys: `{"page": 1, "offset": 0}`

This enables cursor-based pagination across diverse data sources.

### Rust Definition

```rust
// tasker-shared/src/messaging/execution_types.rs
pub struct CursorConfig {
    pub batch_id: String,
    pub start_cursor: serde_json::Value,  // Flexible type
    pub end_cursor: serde_json::Value,    // Flexible type
    pub batch_size: u32,
}
```

### TypeScript Definition

```typescript
// workers/typescript/src/types/batch.ts
export interface RustCursorConfig {
  batch_id: string;
  start_cursor: unknown;  // Flexible: number | string | object
  end_cursor: unknown;
  batch_size: number;
}
```

### Python Definition

```python
# workers/python/python/tasker_core/types.py
class RustCursorConfig(BaseModel):
    batch_id: str
    start_cursor: Any  # Flexible: int | str | dict
    end_cursor: Any
    batch_size: int
```

### JSON Wire Format

```json
{
  "batch_id": "batch_001",
  "start_cursor": 0,
  "end_cursor": 1000,
  "batch_size": 1000
}
```

## BatchProcessingOutcome

Discriminated union representing the outcome of a batchable step.

### Rust Definition

```rust
// tasker-shared/src/messaging/execution_types.rs
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchProcessingOutcome {
    NoBatches,
    CreateBatches {
        worker_template_name: String,
        worker_count: u32,
        cursor_configs: Vec<CursorConfig>,
        total_items: u64,
    },
}
```

### TypeScript Definition

```typescript
// workers/typescript/src/types/batch.ts
export interface NoBatchesOutcome {
  type: 'no_batches';
}

export interface CreateBatchesOutcome {
  type: 'create_batches';
  worker_template_name: string;
  worker_count: number;
  cursor_configs: RustCursorConfig[];
  total_items: number;
}

export type BatchProcessingOutcome = NoBatchesOutcome | CreateBatchesOutcome;
```

### Python Definition

```python
# workers/python/python/tasker_core/types.py
class NoBatchesOutcome(BaseModel):
    type: str = "no_batches"

class CreateBatchesOutcome(BaseModel):
    type: str = "create_batches"
    worker_template_name: str
    worker_count: int
    cursor_configs: list[RustCursorConfig]
    total_items: int

BatchProcessingOutcome = NoBatchesOutcome | CreateBatchesOutcome
```

### JSON Wire Formats

**NoBatches**:
```json
{
  "type": "no_batches"
}
```

**CreateBatches**:
```json
{
  "type": "create_batches",
  "worker_template_name": "batch_worker_template",
  "worker_count": 5,
  "cursor_configs": [
    { "batch_id": "001", "start_cursor": 0, "end_cursor": 1000, "batch_size": 1000 },
    { "batch_id": "002", "start_cursor": 1000, "end_cursor": 2000, "batch_size": 1000 }
  ],
  "total_items": 5000
}
```

## BatchWorkerInputs

Initialization inputs for batch worker instances, stored in `workflow_steps.inputs`.

### Rust Definition

```rust
// tasker-shared/src/models/core/batch_worker.rs
pub struct BatchWorkerInputs {
    pub cursor: CursorConfig,
    pub batch_metadata: BatchMetadata,
    pub is_no_op: bool,
}

pub struct BatchMetadata {
    // TAS-125: checkpoint_interval removed - handlers decide when to checkpoint
    pub cursor_field: String,
    pub failure_strategy: FailureStrategy,
}

pub enum FailureStrategy {
    ContinueOnFailure,
    FailFast,
    Isolate,
}
```

### TypeScript Definition

```typescript
// workers/typescript/src/types/batch.ts
export type FailureStrategy = 'continue_on_failure' | 'fail_fast' | 'isolate';

export interface BatchMetadata {
  // TAS-125: checkpoint_interval removed - handlers decide when to checkpoint
  cursor_field: string;
  failure_strategy: FailureStrategy;
}

export interface RustBatchWorkerInputs {
  cursor: RustCursorConfig;
  batch_metadata: BatchMetadata;
  is_no_op: boolean;
}
```

### Python Definition

```python
# workers/python/python/tasker_core/types.py
class FailureStrategy(str, Enum):
    CONTINUE_ON_FAILURE = "continue_on_failure"
    FAIL_FAST = "fail_fast"
    ISOLATE = "isolate"

class BatchMetadata(BaseModel):
    # TAS-125: checkpoint_interval removed - handlers decide when to checkpoint
    cursor_field: str
    failure_strategy: FailureStrategy

class RustBatchWorkerInputs(BaseModel):
    cursor: RustCursorConfig
    batch_metadata: BatchMetadata
    is_no_op: bool
```

### JSON Wire Format

```json
{
  "cursor": {
    "batch_id": "batch_001",
    "start_cursor": 0,
    "end_cursor": 1000,
    "batch_size": 1000
  },
  "batch_metadata": {
    "cursor_field": "id",
    "failure_strategy": "continue_on_failure"
  },
  "is_no_op": false
}
```

## BatchAggregationResult

Standardized result from aggregating multiple batch worker results.

### Cross-Language Standard

All three languages produce identical aggregation results:

| Field | Type | Description |
|-------|------|-------------|
| `total_processed` | int | Items processed across all batches |
| `total_succeeded` | int | Items that succeeded |
| `total_failed` | int | Items that failed |
| `total_skipped` | int | Items that were skipped |
| `batch_count` | int | Number of batch workers that ran |
| `success_rate` | float | Success rate (0.0 to 1.0) |
| `errors` | array | Collected errors (limited to 100) |
| `error_count` | int | Total error count |

### Usage Examples

**TypeScript**:
```typescript
import { aggregateBatchResults } from 'tasker-core';

const workerResults = Object.values(context.previousResults)
  .filter(r => r?.batch_worker);
const summary = aggregateBatchResults(workerResults);
return this.success(summary);
```

**Python**:
```python
from tasker_core.types import aggregate_batch_results

worker_results = [
    context.get_dependency_result(f"worker_{i}")
    for i in range(batch_count)
]
summary = aggregate_batch_results(worker_results)
return self.success(summary.model_dump())
```

## Factory Functions

### Creating BatchProcessingOutcome

**TypeScript**:
```typescript
import { noBatches, createBatches, RustCursorConfig } from 'tasker-core';

// No batches needed
const outcome1 = noBatches();

// Create batch workers
const configs: RustCursorConfig[] = [
  { batch_id: '001', start_cursor: 0, end_cursor: 1000, batch_size: 1000 },
  { batch_id: '002', start_cursor: 1000, end_cursor: 2000, batch_size: 1000 },
];
const outcome2 = createBatches('process_batch', 2, configs, 2000);
```

**Python**:
```python
from tasker_core.types import no_batches, create_batches, RustCursorConfig

# No batches needed
outcome1 = no_batches()

# Create batch workers
configs = [
    RustCursorConfig(batch_id="001", start_cursor=0, end_cursor=1000, batch_size=1000),
    RustCursorConfig(batch_id="002", start_cursor=1000, end_cursor=2000, batch_size=1000),
]
outcome2 = create_batches("process_batch", 2, configs, 2000)
```

## Type Guards (TypeScript)

```typescript
import {
  BatchProcessingOutcome,
  isNoBatches,
  isCreateBatches
} from 'tasker-core';

function handleOutcome(outcome: BatchProcessingOutcome): void {
  if (isNoBatches(outcome)) {
    console.log('No batches needed');
    return;
  }

  if (isCreateBatches(outcome)) {
    console.log(`Creating ${outcome.worker_count} workers`);
    console.log(`Total items: ${outcome.total_items}`);
  }
}
```

## Migration Notes

### From Legacy Types

If migrating from older batch processing types:

1. **CursorConfig** → **RustCursorConfig**: The new type adds `batch_id` field and uses
   flexible cursor types (`unknown`/`Any`) instead of fixed `number`/`int`.

2. **Inline batch_processing_outcome** → **BatchProcessingOutcome**: Use the discriminated
   union type with factory functions instead of building JSON manually.

3. **Manual aggregation** → **aggregateBatchResults**: Use the standardized aggregation
   function for consistent cross-language behavior.

### Backwards Compatibility

The legacy `CursorConfig` type (with `number`/`int` cursors) is preserved for simple
use cases. Use `RustCursorConfig` when:

- Working with Rust orchestration inputs
- Needing flexible cursor types (timestamps, UUIDs, composites)
- Building `BatchProcessingOutcome` structures

## Related Documentation

- [Batch Processing Guide](../guides/batch-processing.md)
- [Worker Architecture](../architecture/worker-event-systems.md)
- [Configuration Management](../guides/configuration-management.md)

# Batch Processing in Tasker

**Last Updated**: 2026-01-06
**Status**: Production Ready (TAS-59, TAS-125 Complete)
**Related**: [Conditional Workflows](./conditional-workflows.md), [DLQ System](./dlq-system.md)

---

## Table of Contents

- [Overview](#overview)
- [Architecture Foundations](#architecture-foundations)
- [Core Concepts](#core-concepts)
- [Checkpoint Yielding (TAS-125)](#checkpoint-yielding-tas-125)
- [Workflow Pattern](#workflow-pattern)
- [Data Structures](#data-structures)
- [Implementation Patterns](#implementation-patterns)
- [Use Cases](#use-cases)
- [Operator Workflows](#operator-workflows)
- [Code Examples](#code-examples)
- [Best Practices](#best-practices)

---

## Overview

Batch processing in Tasker enables **parallel processing of large datasets** by dynamically creating worker steps at runtime. A single "batchable" step analyzes a workload and instructs orchestration to create N worker instances, each processing a subset of data using cursor-based boundaries.

### Key Characteristics

**Dynamic Worker Creation**: Workers are created at runtime based on dataset analysis, using predefined in templates for structure, but scaled according to need.

**Cursor-Based Resumability**: Each worker processes a specific range (cursor) and can resume from checkpoints on failure.

**Deferred Convergence**: Aggregation steps use intersection semantics to wait for all created workers, regardless of count.

**Standard Lifecycle**: Workers use existing retry, timeout, and DLQ mechanics - no special recovery system needed.

### Example Flow

```
Task: Process 1000-row CSV file

1. analyze_csv (batchable step)
   → Counts rows: 1000
   → Calculates workers: 5 (200 rows each)
   → Returns BatchProcessingOutcome::CreateBatches

2. Orchestration creates workers dynamically:
   ├─ process_csv_batch_001 (rows 1-200)
   ├─ process_csv_batch_002 (rows 201-400)
   ├─ process_csv_batch_003 (rows 401-600)
   ├─ process_csv_batch_004 (rows 601-800)
   └─ process_csv_batch_005 (rows 801-1000)

3. Workers process in parallel

4. aggregate_csv_results (deferred convergence)
   → Waits for all 5 workers (intersection semantics)
   → Aggregates results from completed workers
   → Returns combined metrics
```

---

## Architecture Foundations

Batch processing builds on and extends three foundational Tasker patterns:

### 1. DAG (Directed Acyclic Graph) Workflow Orchestration

**What Batch Processing Inherits:**
- Worker steps are full DAG nodes with standard state machines
- Parent-child dependencies enforced via `tasker_workflow_step_edges`
- Cycle detection prevents circular dependencies
- Topological ordering ensures correct execution sequence

**What Batch Processing Extends:**
- **Dynamic node creation**: Template steps instantiated N times at runtime
- **Edge generation**: Batchable step → worker instances → convergence step
- **Transactional atomicity**: All workers created in single database transaction

**Code Reference**: `tasker-orchestration/src/orchestration/lifecycle/batch_processing/service.rs:357-400`

```rust
// Transaction ensures all-or-nothing worker creation
let mut tx = pool.begin().await?;

for (i, cursor_config) in cursor_configs.iter().enumerate() {
    // Create worker instance from template
    let worker_step = WorkflowStepCreator::create_from_template(
        &mut tx,
        task_uuid,
        &worker_template,
        &format!("{}_{:03}", worker_template_name, i + 1),
        Some(batch_worker_inputs.clone()),
    ).await?;

    // Create edge: batchable → worker
    WorkflowStepEdge::create_with_transaction(
        &mut tx,
        batchable_step.workflow_step_uuid,
        worker_step.workflow_step_uuid,
        "batch_dependency",
    ).await?;
}

tx.commit().await?; // Atomic - all workers or none
```

### 2. Retryability and Lifecycle Management

**What Batch Processing Inherits:**
- Standard `lifecycle.max_retries` configuration per template
- Exponential backoff via `lifecycle.backoff_multiplier`
- Staleness detection using `lifecycle.max_steps_in_process_minutes`
- Standard state transitions (Pending → Enqueued → InProgress → Complete/Error)

**What Batch Processing Extends:**
- **Checkpoint-based resumability**: Workers checkpoint progress and resume from last cursor position
- **Cursor preservation during retry**: `workflow_steps.results` field preserved by `ResetForRetry` action
- **Additional staleness detection**: Checkpoint timestamp tracking alongside duration-based detection

**Key Simplification (TAS-49 Integration):**
- ❌ **No BatchRecoveryService** - Uses standard retry + DLQ
- ❌ **No duplicate timeout settings** - Uses `lifecycle` config only
- ✅ **Cursor data preserved** during `ResetForRetry` (verified in TAS-59 plan)

**Configuration Example**: `tests/fixtures/task_templates/ruby/batch_processing_products_csv.yaml:749-752`

```yaml
- name: process_csv_batch
  type: batch_worker
  lifecycle:
    max_steps_in_process_minutes: 120  # DLQ timeout
    max_retries: 3                     # Standard retry limit
    backoff_multiplier: 2.0            # Exponential backoff
```

### 3. Deferred Convergence (TAS-53)

**What Batch Processing Inherits:**
- **Intersection semantics**: Wait for declared dependencies ∩ actually created steps
- **Template-level dependencies**: Convergence step depends on worker template, not instances
- **Runtime resolution**: System computes effective dependencies when workers are created

**What Batch Processing Extends:**
- **Batch aggregation pattern**: Convergence steps aggregate results from N workers
- **NoBatches scenario handling**: Placeholder worker created when dataset too small
- **Scenario detection helpers**: `BatchAggregationScenario::detect()` for both cases

**Flow Comparison:**

**Conditional Workflows** (Decision Points):
```
decision_step → creates → option_a, option_b (conditional)
                            ↓
convergence_step (depends on option_a AND option_b templates)
                 → waits for whichever were created (intersection)
```

**Batch Processing** (Batchable Steps):
```
batchable_step → creates → worker_001, worker_002, ..., worker_N
                            ↓
convergence_step (depends on worker template)
                 → waits for ALL workers created (intersection)
```

**Code Reference**: `tasker-orchestration/src/orchestration/lifecycle/batch_processing/service.rs:600-650`

```rust
// Determine and create convergence step with intersection semantics
pub async fn determine_and_create_convergence_step(
    &self,
    tx: &mut PgTransaction,
    task_uuid: Uuid,
    convergence_template: &StepDefinition,
    created_workers: &[WorkflowStep],
) -> Result<Option<WorkflowStep>> {
    // Create convergence step as deferred type
    let convergence_step = WorkflowStepCreator::create_from_template(
        tx,
        task_uuid,
        convergence_template,
        &convergence_template.name,
        None,
    ).await?;

    // Create edges from ALL worker instances to convergence step
    for worker in created_workers {
        WorkflowStepEdge::create_with_transaction(
            tx,
            worker.workflow_step_uuid,
            convergence_step.workflow_step_uuid,
            "batch_convergence_dependency",
        ).await?;
    }

    Ok(Some(convergence_step))
}
```

---

## Core Concepts

### Batchable Steps

**Purpose**: Analyze a workload and decide whether to create batch workers.

**Responsibilities**:
1. Examine dataset (size, complexity, business logic)
2. Calculate optimal worker count based on batch size
3. Generate cursor configurations defining batch boundaries
4. Return `BatchProcessingOutcome` instructing orchestration

**Returns**: `BatchProcessingOutcome` enum with two variants:
- `NoBatches`: Dataset too small or empty - create placeholder worker
- `CreateBatches`: Create N workers with cursor configurations

**Code Reference**: `workers/rust/src/step_handlers/batch_processing_products_csv.rs:60-120`

```rust
// Batchable handler example
async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    let csv_file_path = step_data.task.context.get("csv_file_path").unwrap();

    // Count rows in CSV (excluding header)
    let total_rows = count_csv_rows(csv_file_path)?;

    // Get batch configuration from handler initialization
    let batch_size = step_data.handler_initialization
        .get("batch_size").and_then(|v| v.as_u64()).unwrap_or(200);

    if total_rows == 0 {
        // No batches needed
        let outcome = BatchProcessingOutcome::no_batches();
        return Ok(success_result(
            step_uuid,
            json!({ "batch_processing_outcome": outcome.to_value() }),
            elapsed_ms,
            None,
        ));
    }

    // Calculate workers
    let worker_count = (total_rows as f64 / batch_size as f64).ceil() as u32;

    // Generate cursor configs
    let cursor_configs = create_cursor_configs(total_rows, worker_count);

    // Return CreateBatches outcome
    let outcome = BatchProcessingOutcome::create_batches(
        "process_csv_batch".to_string(),
        worker_count,
        cursor_configs,
        total_rows,
    );

    Ok(success_result(
        step_uuid,
        json!({
            "batch_processing_outcome": outcome.to_value(),
            "worker_count": worker_count,
            "total_rows": total_rows
        }),
        elapsed_ms,
        None,
    ))
}
```

### Batch Workers

**Purpose**: Process a specific subset of data defined by cursor configuration.

**Responsibilities**:
1. Extract cursor config from `workflow_step.inputs`
2. Check for `is_no_op` flag (NoBatches placeholder scenario)
3. Process items within cursor range (start_cursor to end_cursor)
4. Checkpoint progress periodically for resumability
5. Return processed results for aggregation

**Cursor Configuration**: Each worker receives `BatchWorkerInputs` in `workflow_step.inputs`:

```json
{
  "cursor": {
    "batch_id": "001",
    "start_cursor": 1,
    "end_cursor": 200,
    "batch_size": 200
  },
  "batch_metadata": {
    "checkpoint_interval": 100,
    "cursor_field": "row_number",
    "failure_strategy": "fail_fast"
  },
  "is_no_op": false
}
```

**Code Reference**: `workers/rust/src/step_handlers/batch_processing_products_csv.rs:200-280`

```rust
// Batch worker handler example
async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    // Extract context using helper
    let context = BatchWorkerContext::from_step_data(step_data)?;

    // Check for no-op placeholder worker
    if context.is_no_op() {
        return Ok(success_result(
            step_uuid,
            json!({
                "no_op": true,
                "reason": "NoBatches scenario - no items to process"
            }),
            elapsed_ms,
            None,
        ));
    }

    // Get cursor range
    let start_row = context.start_position();
    let end_row = context.end_position();

    // Get CSV file path from dependency results
    let csv_file_path = step_data
        .dependency_results
        .get("analyze_csv")
        .and_then(|r| r.result.get("csv_file_path"))
        .unwrap();

    // Process CSV rows in cursor range
    let mut processed_count = 0;
    let mut metrics = initialize_metrics();

    let file = File::open(csv_file_path)?;
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(BufReader::new(file));

    for (row_idx, result) in csv_reader.deserialize::<Product>().enumerate() {
        let data_row_num = row_idx + 1; // 1-indexed after header

        if data_row_num < start_row {
            continue; // Skip rows before our range
        }
        if data_row_num >= end_row {
            break; // Processed all our rows
        }

        let product: Product = result?;

        // Update metrics
        metrics.total_inventory_value += product.price * (product.stock as f64);
        metrics.category_counts.entry(product.category.clone())
            .or_insert(0) += 1;

        processed_count += 1;

        // Checkpoint progress periodically
        if processed_count % context.checkpoint_interval() == 0 {
            checkpoint_progress(step_uuid, data_row_num).await?;
        }
    }

    // Return results for aggregation
    Ok(success_result(
        step_uuid,
        json!({
            "processed_count": processed_count,
            "total_inventory_value": metrics.total_inventory_value,
            "category_counts": metrics.category_counts,
            "batch_id": context.batch_id(),
            "start_row": start_row,
            "end_row": end_row
        }),
        elapsed_ms,
        None,
    ))
}
```

### Convergence Steps

**Purpose**: Aggregate results from all batch workers using deferred intersection semantics.

**Responsibilities**:
1. Detect scenario using `BatchAggregationScenario::detect()`
2. Handle both NoBatches and WithBatches scenarios
3. Aggregate metrics from all worker results
4. Return combined results for task completion

**Scenario Detection**:

```rust
pub enum BatchAggregationScenario {
    /// No batches created - placeholder worker used
    NoBatches {
        batchable_result: StepDependencyResult,
    },

    /// Batches created - multiple workers processed data
    WithBatches {
        batch_results: Vec<(String, StepDependencyResult)>,
        worker_count: u32,
    },
}
```

**Code Reference**: `workers/rust/src/step_handlers/batch_processing_products_csv.rs:400-480`

```rust
// Convergence handler example
async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    // Detect scenario using helper
    let scenario = BatchAggregationScenario::detect(
        &step_data.dependency_results,
        "analyze_csv",        // batchable step name
        "process_csv_batch_", // batch worker prefix
    )?;

    match scenario {
        BatchAggregationScenario::NoBatches { batchable_result } => {
            // No workers created - get dataset size from batchable step
            let total_rows = batchable_result
                .result.get("total_rows")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            // Return zero metrics
            Ok(success_result(
                step_uuid,
                json!({
                    "total_processed": total_rows,
                    "total_inventory_value": 0.0,
                    "category_counts": {},
                    "worker_count": 0
                }),
                elapsed_ms,
                None,
            ))
        }

        BatchAggregationScenario::WithBatches { batch_results, worker_count } => {
            // Aggregate results from all workers
            let mut total_processed = 0u64;
            let mut total_inventory_value = 0.0;
            let mut global_category_counts = HashMap::new();
            let mut max_price = 0.0;
            let mut max_price_product = None;

            for (step_name, result) in batch_results {
                // Sum processed counts
                total_processed += result.result
                    .get("processed_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                // Sum inventory values
                total_inventory_value += result.result
                    .get("total_inventory_value")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                // Merge category counts
                if let Some(categories) = result.result
                    .get("category_counts")
                    .and_then(|v| v.as_object()) {
                    for (category, count) in categories {
                        *global_category_counts.entry(category.clone()).or_insert(0)
                            += count.as_u64().unwrap_or(0);
                    }
                }

                // Find global max price
                let batch_max_price = result.result
                    .get("max_price")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if batch_max_price > max_price {
                    max_price = batch_max_price;
                    max_price_product = result.result
                        .get("max_price_product")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                }
            }

            // Return aggregated metrics
            Ok(success_result(
                step_uuid,
                json!({
                    "total_processed": total_processed,
                    "total_inventory_value": total_inventory_value,
                    "category_counts": global_category_counts,
                    "max_price": max_price,
                    "max_price_product": max_price_product,
                    "worker_count": worker_count
                }),
                elapsed_ms,
                None,
            ))
        }
    }
}
```

---

## Checkpoint Yielding (TAS-125)

Checkpoint yielding enables **handler-driven progress persistence** during long-running batch operations. Handlers explicitly checkpoint their progress, persist state to the database, and yield control back to the orchestrator for re-dispatch.

### Key Characteristics

**Handler-Driven**: Handlers decide when to checkpoint based on business logic, not configuration timers. This gives handlers full control over checkpoint frequency and timing.

**Checkpoint-Persist-Then-Redispatch**: Progress is atomically saved to the database before the step is re-dispatched. This ensures no progress is ever lost, even during infrastructure failures.

**Step Remains In-Progress**: During checkpoint yield cycles, the step stays in `InProgress` state. It is not released or re-enqueued through normal channels—the re-dispatch happens internally.

**State Machine Integrity**: Only `Success` or `Failure` results trigger state transitions. Checkpoint yields are internal handler mechanics that don't affect the step state machine.

### When to Use Checkpoint Yielding

**Use checkpoint yielding when**:
- Processing takes longer than your visibility timeout (prevents DLQ escalation)
- You want resumable processing after transient failures
- You need to periodically release resources (memory, connections)
- Long-running operations need progress visibility

**Don't use checkpoint yielding when**:
- Batch processing completes quickly (<30 seconds)
- The overhead of checkpointing exceeds the benefit
- Operations are inherently non-resumable

### API Reference

All languages provide a `checkpoint_yield()` method (or `checkpointYield()` in TypeScript) on the Batchable mixin:

#### Ruby

```ruby
class MyBatchWorkerHandler
  include Tasker::StepHandler::Batchable

  def call(step_data)
    context = BatchWorkerContext.from_step_data(step_data)

    # Resume from checkpoint if present
    start_item = context.has_checkpoint? ? context.checkpoint_cursor : 0
    accumulated = context.accumulated_results || []

    items = fetch_items_to_process(start_item)

    items.each_with_index do |item, idx|
      result = process_item(item)
      accumulated << result

      # Checkpoint every 1000 items
      if (idx + 1) % 1000 == 0
        checkpoint_yield(
          cursor: start_item + idx + 1,
          items_processed: accumulated.size,
          accumulated_results: { processed: accumulated }
        )
        # Handler execution stops here and resumes on re-dispatch
      end
    end

    # Return final success result
    success_result(results: { all_processed: accumulated })
  end
end
```

**BatchWorkerContext Accessors** (Ruby):
- `checkpoint_cursor` - Current cursor position (or nil if no checkpoint)
- `accumulated_results` - Previously accumulated results (or nil)
- `has_checkpoint?` - Returns true if checkpoint data exists
- `checkpoint_items_processed` - Number of items processed at checkpoint

#### Python

```python
class MyBatchWorkerHandler(BatchableHandler):
    def call(self, step_data: TaskSequenceStep) -> StepExecutionResult:
        context = BatchWorkerContext.from_step_data(step_data)

        # Resume from checkpoint if present
        start_item = context.checkpoint_cursor if context.has_checkpoint() else 0
        accumulated = context.accumulated_results or []

        items = self.fetch_items_to_process(start_item)

        for idx, item in enumerate(items):
            result = self.process_item(item)
            accumulated.append(result)

            # Checkpoint every 1000 items
            if (idx + 1) % 1000 == 0:
                self.checkpoint_yield(
                    cursor=start_item + idx + 1,
                    items_processed=len(accumulated),
                    accumulated_results={"processed": accumulated}
                )
                # Handler execution stops here and resumes on re-dispatch

        # Return final success result
        return self.success_result(results={"all_processed": accumulated})
```

**BatchWorkerContext Accessors** (Python):
- `checkpoint_cursor: int | str | dict | None` - Current cursor position
- `accumulated_results: dict | None` - Previously accumulated results
- `has_checkpoint() -> bool` - Returns true if checkpoint data exists
- `checkpoint_items_processed: int` - Number of items processed at checkpoint

#### TypeScript

```typescript
class MyBatchWorkerHandler extends BatchableHandler {
  async call(stepData: TaskSequenceStep): Promise<StepExecutionResult> {
    const context = BatchWorkerContext.fromStepData(stepData);

    // Resume from checkpoint if present
    const startItem = context.hasCheckpoint() ? context.checkpointCursor : 0;
    const accumulated = context.accumulatedResults ?? [];

    const items = await this.fetchItemsToProcess(startItem);

    for (let idx = 0; idx < items.length; idx++) {
      const result = await this.processItem(items[idx]);
      accumulated.push(result);

      // Checkpoint every 1000 items
      if ((idx + 1) % 1000 === 0) {
        await this.checkpointYield({
          cursor: startItem + idx + 1,
          itemsProcessed: accumulated.length,
          accumulatedResults: { processed: accumulated }
        });
        // Handler execution stops here and resumes on re-dispatch
      }
    }

    // Return final success result
    return this.successResult({ results: { allProcessed: accumulated } });
  }
}
```

**BatchWorkerContext Properties** (TypeScript):
- `checkpointCursor: number | string | Record<string, unknown> | undefined`
- `accumulatedResults: Record<string, unknown> | undefined`
- `hasCheckpoint(): boolean`
- `checkpointItemsProcessed: number`

### Checkpoint Data Structure

Checkpoints are persisted in the `checkpoint` JSONB column on `workflow_steps`:

```json
{
  "cursor": 1000,
  "items_processed": 1000,
  "timestamp": "2026-01-06T12:00:00Z",
  "accumulated_results": {
    "processed": ["item1", "item2", "..."]
  },
  "history": [
    {"cursor": 500, "timestamp": "2026-01-06T11:59:30Z"},
    {"cursor": 1000, "timestamp": "2026-01-06T12:00:00Z"}
  ]
}
```

**Fields**:
- `cursor` - Flexible JSON value representing position (integer, string, or object)
- `items_processed` - Total items processed at this checkpoint
- `timestamp` - ISO 8601 timestamp when checkpoint was created
- `accumulated_results` - Optional intermediate results to preserve
- `history` - Array of previous checkpoint positions (appended automatically)

### Checkpoint Flow

```
┌──────────────────────────────────────────────────────────────────┐
│  Handler calls checkpoint_yield(cursor, items_processed, ...)   │
└───────────────────────────────┬──────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────┐
│  FFI Bridge: checkpoint_yield_step_event()                       │
│  Converts language-specific types to CheckpointYieldData         │
└───────────────────────────────┬──────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────┐
│  CheckpointService::save_checkpoint()                            │
│  - Atomic SQL update with history append                         │
│  - Uses JSONB jsonb_set for history array                        │
└───────────────────────────────┬──────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────┐
│  Worker re-dispatches step via internal MPSC channel             │
│  - Step stays InProgress (not released)                          │
│  - Re-queued for immediate processing                            │
└───────────────────────────────┬──────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────┐
│  Handler resumes with checkpoint data in workflow_step           │
│  - BatchWorkerContext provides checkpoint accessors              │
│  - Handler continues from saved cursor position                  │
└──────────────────────────────────────────────────────────────────┘
```

### Failure and Recovery

**Transient Failure After Checkpoint**:
1. Handler checkpoints at position 500
2. Handler fails at position 750 (transient error)
3. Step is retried (standard retry semantics)
4. Handler resumes from checkpoint (position 500)
5. Items 500-750 are reprocessed (idempotency required)
6. Processing continues to completion

**Permanent Failure**:
1. Handler checkpoints at position 500
2. Handler encounters non-retryable error
3. Step transitions to Error state
4. Checkpoint data preserved for operator inspection
5. Manual intervention may use checkpoint to resume later

### Best Practices

**Checkpoint Frequency**:
- Too frequent: Overhead dominates (database writes, re-dispatch latency)
- Too infrequent: Lost progress on failure, long recovery time
- Rule of thumb: Checkpoint every 1-5 minutes of work, or every 1000-10000 items

**Accumulated Results**:
- Keep accumulated results small (summaries, counts, IDs)
- For large result sets, write to external storage and store reference
- Unbounded accumulated results can cause performance degradation

**Cursor Design**:
- Use monotonic cursors (integers, timestamps) when possible
- Complex cursors (objects) are supported but harder to debug
- Cursor must uniquely identify resume position

**Idempotency**:
- Items between last checkpoint and failure will be reprocessed
- Ensure item processing is idempotent or use deduplication
- Consider storing processed item IDs in accumulated_results

### Monitoring

**Checkpoint Events** (logged automatically):
```
INFO checkpoint_yield_step_event step_uuid=abc cursor=1000 items_processed=1000
INFO checkpoint_saved step_uuid=abc history_length=2
```

**Metrics to Monitor**:
- Checkpoint frequency per step
- Average items processed between checkpoints
- Checkpoint history size (detect unbounded growth)
- Re-dispatch latency after checkpoint

### Known Limitations

**History Array Growth**: The `history` array grows with each checkpoint. For very long-running processes with frequent checkpoints, this can lead to large JSONB values. Consider:
- Setting a maximum history length (future enhancement)
- Clearing history on step completion
- Using external storage for detailed history

**Accumulated Results Size**: No built-in size limit on `accumulated_results`. Handlers must self-regulate to prevent database bloat. Consider:
- Storing summaries instead of raw data
- Using external storage for large intermediate results
- Implementing size checks before checkpoint

---

## Workflow Pattern

### Template Definition

Batch processing workflows use three step types in YAML templates:

```yaml
name: csv_product_inventory_analyzer
namespace_name: csv_processing
version: "1.0.0"

steps:
  # BATCHABLE STEP: Analyzes dataset and decides batching strategy
  - name: analyze_csv
    type: batchable
    dependencies: []
    handler:
      callable: BatchProcessing::CsvAnalyzerHandler
      initialization:
        batch_size: 200
        max_workers: 5

  # BATCH WORKER TEMPLATE: Single batch processing unit
  # Orchestration creates N instances from this template at runtime
  - name: process_csv_batch
    type: batch_worker
    dependencies:
      - analyze_csv
    lifecycle:
      max_steps_in_process_minutes: 120
      max_retries: 3
      backoff_multiplier: 2.0
    handler:
      callable: BatchProcessing::CsvBatchProcessorHandler
      initialization:
        operation: "inventory_analysis"

  # DEFERRED CONVERGENCE STEP: Aggregates results from all workers
  - name: aggregate_csv_results
    type: deferred_convergence
    dependencies:
      - process_csv_batch  # Template dependency - resolves to all instances
    handler:
      callable: BatchProcessing::CsvResultsAggregatorHandler
      initialization:
        aggregation_type: "inventory_metrics"
```

### Runtime Execution Flow

**1. Task Initialization**
```
User creates task with context: { "csv_file_path": "/path/to/data.csv" }
↓
Task enters Initializing state
↓
Orchestration discovers ready steps: [analyze_csv]
```

**2. Batchable Step Execution**
```
analyze_csv step enqueued to worker queue
↓
Worker claims step, executes CsvAnalyzerHandler
↓
Handler counts rows: 1000
Handler calculates workers: 5 (200 rows each)
Handler generates cursor configs
Handler returns BatchProcessingOutcome::CreateBatches
↓
Step completes with batch_processing_outcome in results
```

**3. Batch Worker Creation** (Orchestration)
```
ResultProcessorActor processes analyze_csv completion
↓
Detects batch_processing_outcome in step results
↓
Sends ProcessBatchableStepMessage to BatchProcessingActor
↓
BatchProcessingService.process_batchable_step():
  - Begins database transaction
  - Creates 5 worker instances from process_csv_batch template:
    * process_csv_batch_001 (cursor: rows 1-200)
    * process_csv_batch_002 (cursor: rows 201-400)
    * process_csv_batch_003 (cursor: rows 401-600)
    * process_csv_batch_004 (cursor: rows 601-800)
    * process_csv_batch_005 (cursor: rows 801-1000)
  - Creates edges: analyze_csv → each worker
  - Creates convergence step: aggregate_csv_results
  - Creates edges: each worker → aggregate_csv_results
  - Commits transaction (all-or-nothing)
↓
Workers enqueued to worker queue with PGMQ notifications
```

**4. Parallel Worker Execution**
```
5 workers execute in parallel:

Worker 001:
  - Extracts cursor: start=1, end=200
  - Processes CSV rows 1-200
  - Returns: processed_count=200, metrics={...}

Worker 002:
  - Extracts cursor: start=201, end=400
  - Processes CSV rows 201-400
  - Returns: processed_count=200, metrics={...}

... (workers 003-005 similar)

All workers complete
```

**5. Convergence Step Execution**
```
Orchestration discovers aggregate_csv_results is ready
(all parent workers completed - intersection semantics)
↓
aggregate_csv_results enqueued to worker queue
↓
Worker claims step, executes CsvResultsAggregatorHandler
↓
Handler detects scenario: WithBatches (5 workers)
Handler aggregates results from all 5 workers:
  - total_processed: 1000
  - total_inventory_value: $XXX,XXX.XX
  - category_counts: {electronics: 300, clothing: 250, ...}
Handler returns aggregated metrics
↓
Step completes
```

**6. Task Completion**
```
Orchestration detects all steps complete
↓
TaskFinalizerActor finalizes task
↓
Task state: Complete
```

### NoBatches Scenario Flow

**When dataset is too small or empty**:

```
analyze_csv determines dataset too small (e.g., 50 rows < 200 batch_size)
↓
Returns BatchProcessingOutcome::NoBatches
↓
Orchestration creates single placeholder worker:
  - process_csv_batch_001 (is_no_op: true)
  - No cursor configuration needed
  - Still maintains DAG structure
↓
Placeholder worker executes:
  - Detects is_no_op flag
  - Returns immediately with no_op: true
  - No actual data processing
↓
Convergence step detects NoBatches scenario:
  - Uses batchable step result directly
  - Returns zero metrics or empty aggregation
```

**Why placeholder workers?**
- Maintains consistent DAG structure
- Convergence step logic handles both scenarios uniformly
- No special-case orchestration logic needed
- Standard retry/DLQ mechanics still apply

---

## Data Structures

### BatchProcessingOutcome

**Location**: `tasker-shared/src/messaging/execution_types.rs`

**Purpose**: Returned by batchable handlers to instruct orchestration.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchProcessingOutcome {
    /// No batching needed - create placeholder worker
    NoBatches,

    /// Create N batch workers with cursor configurations
    CreateBatches {
        /// Template step name (e.g., "process_csv_batch")
        worker_template_name: String,

        /// Number of workers to create
        worker_count: u32,

        /// Cursor configurations for each worker
        cursor_configs: Vec<CursorConfig>,

        /// Total items across all batches
        total_items: u64,
    },
}

impl BatchProcessingOutcome {
    pub fn no_batches() -> Self {
        BatchProcessingOutcome::NoBatches
    }

    pub fn create_batches(
        worker_template_name: String,
        worker_count: u32,
        cursor_configs: Vec<CursorConfig>,
        total_items: u64,
    ) -> Self {
        BatchProcessingOutcome::CreateBatches {
            worker_template_name,
            worker_count,
            cursor_configs,
            total_items,
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(json!({}))
    }
}
```

**Ruby Mirror**: `workers/ruby/lib/tasker_core/types/batch_processing_outcome.rb`

```ruby
module TaskerCore
  module Types
    module BatchProcessingOutcome
      class NoBatches < Dry::Struct
        attribute :type, Types::String.default('no_batches')

        def to_h
          { 'type' => 'no_batches' }
        end

        def requires_batch_creation?
          false
        end
      end

      class CreateBatches < Dry::Struct
        attribute :type, Types::String.default('create_batches')
        attribute :worker_template_name, Types::Strict::String
        attribute :worker_count, Types::Coercible::Integer.constrained(gteq: 1)
        attribute :cursor_configs, Types::Array.of(Types::Hash).constrained(min_size: 1)
        attribute :total_items, Types::Coercible::Integer.constrained(gteq: 0)

        def to_h
          {
            'type' => 'create_batches',
            'worker_template_name' => worker_template_name,
            'worker_count' => worker_count,
            'cursor_configs' => cursor_configs,
            'total_items' => total_items
          }
        end

        def requires_batch_creation?
          true
        end
      end

      class << self
        def no_batches
          NoBatches.new
        end

        def create_batches(worker_template_name:, worker_count:, cursor_configs:, total_items:)
          CreateBatches.new(
            worker_template_name: worker_template_name,
            worker_count: worker_count,
            cursor_configs: cursor_configs,
            total_items: total_items
          )
        end
      end
    end
  end
end
```

### CursorConfig

**Location**: `tasker-shared/src/messaging/execution_types.rs`

**Purpose**: Defines batch boundaries for each worker.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Batch identifier (e.g., "001", "002", "003")
    pub batch_id: String,

    /// Starting position (inclusive) - flexible JSON value
    pub start_cursor: serde_json::Value,

    /// Ending position (exclusive) - flexible JSON value
    pub end_cursor: serde_json::Value,

    /// Number of items in this batch
    pub batch_size: u32,
}
```

**Design Notes**:
- Cursor values use `serde_json::Value` for flexibility
- Supports integers, strings, timestamps, UUIDs, etc.
- Batch IDs are zero-padded strings for consistent ordering
- `start_cursor` is inclusive, `end_cursor` is exclusive

**Example Cursor Configs**:

```json
// Numeric cursors (CSV row numbers)
{
  "batch_id": "001",
  "start_cursor": 1,
  "end_cursor": 200,
  "batch_size": 200
}

// Timestamp cursors (event processing)
{
  "batch_id": "002",
  "start_cursor": "2025-01-01T00:00:00Z",
  "end_cursor": "2025-01-01T01:00:00Z",
  "batch_size": 3600
}

// UUID cursors (database pagination)
{
  "batch_id": "003",
  "start_cursor": "00000000-0000-0000-0000-000000000000",
  "end_cursor": "3fffffff-ffff-ffff-ffff-ffffffffffff",
  "batch_size": 1000000
}
```

### BatchWorkerInputs

**Location**: `tasker-shared/src/models/core/batch_worker.rs`

**Purpose**: Stored in `workflow_steps.inputs` for each worker instance.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchWorkerInputs {
    /// Cursor configuration defining this worker's batch range
    pub cursor: CursorConfig,

    /// Batch processing metadata
    pub batch_metadata: BatchMetadata,

    /// Flag indicating if this is a placeholder worker (NoBatches scenario)
    #[serde(default)]
    pub is_no_op: bool,
}

impl BatchWorkerInputs {
    pub fn new(
        cursor_config: CursorConfig,
        batch_config: &BatchConfiguration,
        is_no_op: bool,
    ) -> Self {
        Self {
            cursor: cursor_config,
            batch_metadata: BatchMetadata {
                checkpoint_interval: batch_config.checkpoint_interval,
                cursor_field: batch_config.cursor_field.clone(),
                failure_strategy: batch_config.failure_strategy.clone(),
            },
            is_no_op,
        }
    }
}
```

**Storage Location**:
- ✅ `workflow_steps.inputs` (instance-specific runtime data)
- ❌ NOT in `step_definition.handler.initialization` (that's the template)

### BatchMetadata

**Location**: `tasker-shared/src/models/core/batch_worker.rs`

**Purpose**: Runtime configuration for batch processing behavior.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchMetadata {
    /// Checkpoint frequency (every N items)
    pub checkpoint_interval: u32,

    /// Field name used for cursor tracking (e.g., "id", "row_number")
    pub cursor_field: String,

    /// How to handle failures during batch processing
    pub failure_strategy: FailureStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureStrategy {
    /// Fail immediately if any item fails
    FailFast,

    /// Continue processing remaining items, report failures at end
    ContinueOnFailure,

    /// Isolate failed items to separate queue
    IsolateFailed,
}
```

---

## Implementation Patterns

### Rust Implementation

**1. Batchable Handler Pattern**:

```rust
use tasker_shared::messaging::execution_types::{BatchProcessingOutcome, CursorConfig};
use serde_json::json;

async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    // 1. Analyze dataset
    let dataset_size = analyze_dataset(step_data)?;
    let batch_size = get_batch_size_from_config(step_data)?;

    // 2. Check if batching needed
    if dataset_size == 0 || dataset_size < batch_size {
        let outcome = BatchProcessingOutcome::no_batches();
        return Ok(success_result(
            step_uuid,
            json!({ "batch_processing_outcome": outcome.to_value() }),
            elapsed_ms,
            None,
        ));
    }

    // 3. Calculate worker count
    let worker_count = (dataset_size as f64 / batch_size as f64).ceil() as u32;

    // 4. Generate cursor configs
    let cursor_configs = create_cursor_configs(dataset_size, worker_count, batch_size);

    // 5. Return CreateBatches outcome
    let outcome = BatchProcessingOutcome::create_batches(
        "worker_template_name".to_string(),
        worker_count,
        cursor_configs,
        dataset_size,
    );

    Ok(success_result(
        step_uuid,
        json!({
            "batch_processing_outcome": outcome.to_value(),
            "worker_count": worker_count,
            "total_items": dataset_size
        }),
        elapsed_ms,
        None,
    ))
}

fn create_cursor_configs(
    total_items: u64,
    worker_count: u32,
    batch_size: u64,
) -> Vec<CursorConfig> {
    let mut cursor_configs = Vec::new();
    let items_per_worker = (total_items as f64 / worker_count as f64).ceil() as u64;

    for i in 0..worker_count {
        let start_position = i as u64 * items_per_worker;
        let end_position = ((i + 1) as u64 * items_per_worker).min(total_items);

        cursor_configs.push(CursorConfig {
            batch_id: format!("{:03}", i + 1),
            start_cursor: json!(start_position),
            end_cursor: json!(end_position),
            batch_size: (end_position - start_position) as u32,
        });
    }

    cursor_configs
}
```

**2. Batch Worker Handler Pattern**:

```rust
use tasker_worker::batch_processing::BatchWorkerContext;

async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    // 1. Extract batch worker context using helper
    let context = BatchWorkerContext::from_step_data(step_data)?;

    // 2. Check for no-op placeholder worker
    if context.is_no_op() {
        return Ok(success_result(
            step_uuid,
            json!({
                "no_op": true,
                "reason": "NoBatches scenario",
                "batch_id": context.batch_id()
            }),
            elapsed_ms,
            None,
        ));
    }

    // 3. Extract cursor range
    let start_idx = context.start_position();
    let end_idx = context.end_position();
    let checkpoint_interval = context.checkpoint_interval();

    // 4. Process items in range
    let mut processed_count = 0;
    let mut results = initialize_results();

    for idx in start_idx..end_idx {
        // Process item
        let item = get_item(idx)?;
        update_results(&mut results, item);

        processed_count += 1;

        // 5. Checkpoint progress periodically
        if processed_count % checkpoint_interval == 0 {
            checkpoint_progress(step_uuid, idx).await?;
        }
    }

    // 6. Return results for aggregation
    Ok(success_result(
        step_uuid,
        json!({
            "processed_count": processed_count,
            "results": results,
            "batch_id": context.batch_id(),
            "start_position": start_idx,
            "end_position": end_idx
        }),
        elapsed_ms,
        None,
    ))
}
```

**3. Convergence Handler Pattern**:

```rust
use tasker_worker::batch_processing::BatchAggregationScenario;

async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    // 1. Detect scenario using helper
    let scenario = BatchAggregationScenario::detect(
        &step_data.dependency_results,
        "batchable_step_name",
        "batch_worker_prefix_",
    )?;

    // 2. Handle both scenarios
    let aggregated_results = match scenario {
        BatchAggregationScenario::NoBatches { batchable_result } => {
            // Get dataset info from batchable step
            let total_items = batchable_result
                .result.get("total_items")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            // Return zero metrics
            json!({
                "total_processed": total_items,
                "worker_count": 0
            })
        }

        BatchAggregationScenario::WithBatches { batch_results, worker_count } => {
            // Aggregate results from all workers
            let mut total_processed = 0u64;

            for (step_name, result) in batch_results {
                total_processed += result.result
                    .get("processed_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                // Additional aggregation logic...
            }

            json!({
                "total_processed": total_processed,
                "worker_count": worker_count
            })
        }
    };

    // 3. Return aggregated results
    Ok(success_result(
        step_uuid,
        aggregated_results,
        elapsed_ms,
        None,
    ))
}
```

### Ruby Implementation

**1. Batchable Handler Pattern** (using Batchable base class):

```ruby
module BatchProcessing
  class CsvAnalyzerHandler < TaskerCore::StepHandler::Batchable
    def call(task, _sequence, step)
      csv_file_path = task.context['csv_file_path']
      total_rows = count_csv_rows(csv_file_path)

      # Get batch configuration
      batch_size = step_definition_initialization['batch_size'] || 200
      max_workers = step_definition_initialization['max_workers'] || 5

      # Calculate worker count
      worker_count = [(total_rows.to_f / batch_size).ceil, max_workers].min

      if worker_count == 0 || total_rows == 0
        # Use helper for NoBatches outcome
        return no_batches_success(
          reason: 'dataset_too_small',
          total_rows: total_rows
        )
      end

      # Generate cursor configs using helper
      cursor_configs = generate_cursor_configs(
        total_items: total_rows,
        worker_count: worker_count
      )

      # Use helper for CreateBatches outcome
      create_batches_success(
        worker_template_name: 'process_csv_batch',
        worker_count: worker_count,
        cursor_configs: cursor_configs,
        total_items: total_rows,
        additional_data: {
          'csv_file_path' => csv_file_path
        }
      )
    end

    private

    def count_csv_rows(csv_file_path)
      CSV.read(csv_file_path, headers: true).length
    end
  end
end
```

**2. Batch Worker Handler Pattern** (using Batchable base class):

```ruby
module BatchProcessing
  class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
    def call(context)
      # Extract batch context using helper
      batch_ctx = get_batch_context(context)

      # Use helper to check for no-op worker
      no_op_result = handle_no_op_worker(batch_ctx)
      return no_op_result if no_op_result

      # Get CSV file path from dependency results
      csv_file_path = context.get_dependency_result('analyze_csv')&.dig('csv_file_path')

      # Process CSV rows in cursor range
      metrics = process_csv_batch(
        csv_file_path,
        batch_ctx.start_cursor,
        batch_ctx.end_cursor
      )

      # Return results for aggregation
      success(
        result_data: {
          'processed_count' => metrics[:processed_count],
          'total_inventory_value' => metrics[:total_inventory_value],
          'category_counts' => metrics[:category_counts],
          'batch_id' => batch_ctx.batch_id
        }
      )
    end

    private

    def process_csv_batch(csv_file_path, start_row, end_row)
      metrics = {
        processed_count: 0,
        total_inventory_value: 0.0,
        category_counts: Hash.new(0)
      }

      CSV.foreach(csv_file_path, headers: true).with_index(1) do |row, data_row_num|
        next if data_row_num < start_row
        break if data_row_num >= end_row

        product = parse_product(row)

        metrics[:total_inventory_value] += product.price * product.stock
        metrics[:category_counts][product.category] += 1
        metrics[:processed_count] += 1
      end

      metrics
    end
  end
end
```

**3. Convergence Handler Pattern** (using Batchable base class):

```ruby
module BatchProcessing
  class CsvResultsAggregatorHandler < TaskerCore::StepHandler::Batchable
    def call(_task, sequence, _step)
      # Detect scenario using helper
      scenario = detect_aggregation_scenario(
        sequence,
        batchable_step_name: 'analyze_csv',
        batch_worker_prefix: 'process_csv_batch_'
      )

      # Use helper for aggregation with custom block
      aggregate_batch_worker_results(scenario) do |batch_results|
        # Custom aggregation logic
        total_processed = 0
        total_inventory_value = 0.0
        global_category_counts = Hash.new(0)

        batch_results.each do |step_name, result|
          total_processed += result.dig('result', 'processed_count') || 0
          total_inventory_value += result.dig('result', 'total_inventory_value') || 0.0

          (result.dig('result', 'category_counts') || {}).each do |category, count|
            global_category_counts[category] += count
          end
        end

        {
          'total_processed' => total_processed,
          'total_inventory_value' => total_inventory_value,
          'category_counts' => global_category_counts,
          'worker_count' => batch_results.size
        }
      end
    end
  end
end
```

---

## Use Cases

### 1. Large Dataset Processing

**Scenario**: Process millions of records from a database, file, or API.

**Why Batch Processing?**
- Single worker would timeout
- Memory constraints prevent loading entire dataset
- Want parallelism for speed

**Example**: Product catalog synchronization
```
Batchable: Analyze product table (5 million products)
Workers: 100 workers × 50,000 products each
Convergence: Aggregate sync statistics
Result: 5M products synced in 10 minutes vs 2 hours sequential
```

### 2. Time-Based Event Processing

**Scenario**: Process events from a time-series database or log aggregation system.

**Why Batch Processing?**
- Events span long time ranges
- Want to process hourly/daily chunks in parallel
- Need resumability for long-running processing

**Example**: Analytics event processing
```
Batchable: Analyze events (30 days × 24 hours)
Workers: 720 workers (1 per hour)
Cursors: Timestamp ranges (2025-01-01T00:00 to 2025-01-01T01:00)
Convergence: Aggregate daily/monthly metrics
```

### 3. Multi-Source Data Integration

**Scenario**: Fetch data from multiple external APIs or services.

**Why Batch Processing?**
- Each source is independent
- Want parallel fetching for speed
- Some sources may fail (need retry per source)

**Example**: Third-party data enrichment
```
Batchable: Analyze customer list (partition by data provider)
Workers: 5 workers (1 per provider: Stripe, Salesforce, HubSpot, etc.)
Cursors: Provider-specific identifiers
Convergence: Merge enriched customer profiles
```

### 4. Bulk File Processing

**Scenario**: Process multiple files (CSVs, images, documents).

**Why Batch Processing?**
- Each file is independent processing unit
- Want parallelism across files
- File sizes vary (dynamic batch sizing)

**Example**: Image transformation pipeline
```
Batchable: List S3 bucket objects (1000 images)
Workers: 20 workers × 50 images each
Cursors: S3 object key ranges
Convergence: Verify all images transformed
```

### 5. Geographical Data Partitioning

**Scenario**: Process data partitioned by geography (regions, countries, cities).

**Why Batch Processing?**
- Geographic boundaries provide natural partitions
- Want parallel processing per region
- Different regions may have different data volumes

**Example**: Regional sales report generation
```
Batchable: Analyze sales data (50 US states)
Workers: 50 workers (1 per state)
Cursors: State codes (AL, AK, AZ, ...)
Convergence: National sales dashboard
```

---

## Operator Workflows

Batch processing integrates seamlessly with the DLQ (Dead Letter Queue) system for operator visibility and manual intervention. This section shows how operators manage failed batch workers.

### DLQ Integration Principles

**From [DLQ System Documentation](./dlq-system.md)**:

1. **Investigation Tracking Only**: DLQ tracks "why task is stuck" and "who investigated" - it doesn't manipulate tasks
2. **Step-Level Resolution**: Operators fix problem steps using step APIs, not task-level operations
3. **Three Resolution Types**:
   - **ResetForRetry**: Reset attempts, return step to pending (cursor preserved)
   - **ResolveManually**: Skip step, mark resolved without results
   - **CompleteManually**: Provide manual results for dependent steps

**Key for Batch Processing**: Cursor data in `workflow_steps.results` is preserved during `ResetForRetry`, enabling resumability without data loss.

### Staleness Detection for Batch Workers

Batch workers have **two staleness detection mechanisms**:

**1. Duration-Based (Standard)**:
```yaml
lifecycle:
  max_steps_in_process_minutes: 120  # DLQ threshold
```

If worker stays in `InProgress` state for > 120 minutes, flagged as stale.

**2. Checkpoint-Based (Batch-Specific)**:
```rust
// Workers checkpoint progress periodically
if processed_count % checkpoint_interval == 0 {
    checkpoint_progress(step_uuid, current_cursor).await?;
}
```

If last checkpoint timestamp is too old, flagged as stale even if within duration threshold.

### Common Operator Scenarios

#### Scenario 1: Transient Database Failure

**Problem**: 3 out of 5 batch workers failed due to database connection timeout.

**Step 1: Find the stuck task in DLQ**:
```bash
# Get investigation queue (prioritized by age and reason)
curl http://localhost:8080/v1/dlq/investigation-queue | jq
```

**Step 2: Get task details and identify failed workers**:
```sql
-- Get DLQ entry for the task
SELECT
    dlq.dlq_entry_uuid,
    dlq.task_uuid,
    dlq.dlq_reason,
    dlq.resolution_status,
    dlq.task_snapshot->'workflow_steps' as steps
FROM tasker.tasks_dlq dlq
WHERE dlq.task_uuid = 'task-uuid-here'
  AND dlq.resolution_status = 'pending';

-- Query task's workflow steps to find failed batch workers
SELECT
    ws.workflow_step_uuid,
    ws.name,
    ws.current_state,
    ws.attempts,
    ws.last_error
FROM tasker.workflow_steps ws
WHERE ws.task_uuid = 'task-uuid-here'
  AND ws.name LIKE 'process_csv_batch_%'
  AND ws.current_state = 'Error';
```

**Result**:
```
workflow_step_uuid | name                   | current_state | attempts | last_error
-------------------|------------------------|---------------|----------|------------------
uuid-worker-2      | process_csv_batch_002  | Error         | 3        | DB timeout
uuid-worker-4      | process_csv_batch_004  | Error         | 3        | DB timeout
uuid-worker-5      | process_csv_batch_005  | Error         | 3        | DB timeout
```

**Operator Action**: Database is now healthy - reset workers for retry

```bash
# Get task UUID from DLQ entry
TASK_UUID="abc-123-task-uuid"

# Reset worker 2 (preserves cursor: rows 201-400)
curl -X PATCH http://localhost:8080/v1/tasks/${TASK_UUID}/workflow_steps/uuid-worker-2 \
  -H "Content-Type: application/json" \
  -d '{
    "action_type": "reset_for_retry",
    "reset_by": "operator@example.com",
    "reason": "Database connection restored, resetting attempts"
  }'

# Reset workers 4 and 5
curl -X PATCH http://localhost:8080/v1/tasks/${TASK_UUID}/workflow_steps/uuid-worker-4 \
  -H "Content-Type: application/json" \
  -d '{"action_type": "reset_for_retry", "reset_by": "operator@example.com", "reason": "Database connection restored"}'

curl -X PATCH http://localhost:8080/v1/tasks/${TASK_UUID}/workflow_steps/uuid-worker-5 \
  -H "Content-Type: application/json" \
  -d '{"action_type": "reset_for_retry", "reset_by": "operator@example.com", "reason": "Database connection restored"}'

# Update DLQ entry to track resolution
curl -X PATCH http://localhost:8080/v1/dlq/entry/${DLQ_ENTRY_UUID} \
  -H "Content-Type: application/json" \
  -d '{
    "resolution_status": "manually_resolved",
    "resolution_notes": "Reset 3 failed batch workers after database connection restored",
    "resolved_by": "operator@example.com"
  }'
```

**Result**:
- Workers 2, 4, 5 return to `Pending` state
- Cursor configs preserved in `workflow_steps.inputs`
- Retry attempt counter reset to 0
- Workers re-enqueued for execution
- DLQ entry updated with resolution metadata

#### Scenario 2: Bad Data in Specific Batch

**Problem**: Worker 3 repeatedly fails due to malformed CSV row in its range (rows 401-600).

**Investigation**:
```sql
-- Get worker details
SELECT
    ws.name,
    ws.current_state,
    ws.attempts,
    ws.last_error
FROM tasker.workflow_steps ws
WHERE ws.workflow_step_uuid = 'uuid-worker-3';
```

**Result**:
```
name: process_csv_batch_003
current_state: Error
attempts: 3
last_error: "CSV parsing failed at row 523: invalid price format"
```

**Operator Decision**: Row 523 has known data quality issue, already fixed in source system.

**Option 1: Complete Manually** (provide results for this batch):

```bash
TASK_UUID="abc-123-task-uuid"
STEP_UUID="uuid-worker-3"

curl -X PATCH http://localhost:8080/v1/tasks/${TASK_UUID}/workflow_steps/${STEP_UUID} \
  -H "Content-Type: application/json" \
  -d '{
    "action_type": "complete_manually",
    "completion_data": {
      "result": {
        "processed_count": 199,
        "total_inventory_value": 45232.50,
        "category_counts": {"electronics": 150, "clothing": 49},
        "batch_id": "003",
        "note": "Row 523 skipped due to data quality issue, manually verified totals"
      },
      "metadata": {
        "manually_verified": true,
        "verification_method": "manual_inspection",
        "skipped_rows": [523]
      }
    },
    "reason": "Manual completion after verifying corrected data in source system",
    "completed_by": "operator@example.com"
  }'
```

**Option 2: Resolve Manually** (skip this batch):

```bash
curl -X PATCH http://localhost:8080/v1/tasks/${TASK_UUID}/workflow_steps/${STEP_UUID} \
  -H "Content-Type: application/json" \
  -d '{
    "action_type": "resolve_manually",
    "resolved_by": "operator@example.com",
    "reason": "Non-critical batch containing known bad data, skipping 200 rows out of 1000 total"
  }'
```

**Result (Option 1)**:
- Worker 3 marked `Complete` with manual results
- Convergence step receives manual results in aggregation
- Task completes successfully with note about manual intervention

**Result (Option 2)**:
- Worker 3 marked `ResolvedManually` (no results provided)
- Convergence step detects missing results, adjusts aggregation
- Task completes with reduced total (800 rows instead of 1000)

#### Scenario 3: Long-Running Worker Needs Checkpoint

**Problem**: Worker 1 processing 10,000 rows, operator notices it's been running 90 minutes (threshold: 120 minutes).

**Investigation**:
```sql
-- Check last checkpoint
SELECT
    ws.name,
    ws.current_state,
    ws.results->>'last_checkpoint_cursor' as last_checkpoint,
    ws.results->>'checkpoint_timestamp' as checkpoint_time,
    NOW() - (ws.results->>'checkpoint_timestamp')::timestamptz as time_since_checkpoint
FROM tasker.workflow_steps ws
WHERE ws.workflow_step_uuid = 'uuid-worker-1';
```

**Result**:
```
name: process_large_batch_001
current_state: InProgress
last_checkpoint: 7850
checkpoint_time: 2025-01-15 11:30:00
time_since_checkpoint: 00:05:00
```

**Operator Action**: Worker is healthy and making progress (checkpointed 5 minutes ago at row 7850).

No action needed - worker will complete normally. Operator adds investigation note to DLQ entry:

```bash
DLQ_ENTRY_UUID="dlq-entry-uuid-here"

curl -X PATCH http://localhost:8080/v1/dlq/entry/${DLQ_ENTRY_UUID} \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "investigation_notes": "Worker healthy, last checkpoint at row 7850 (5 min ago), estimated 15 min remaining",
      "investigator": "operator@example.com",
      "timestamp": "2025-01-15T11:35:00Z",
      "action_taken": "none - monitoring"
    }
  }'
```

#### Scenario 4: All Workers Failed - Batch Strategy Issue

**Problem**: All 10 workers fail with "memory exhausted" error - batch size too large.

**Investigation via API**:
```bash
TASK_UUID="task-uuid-here"

# Get task details including all workflow steps
curl http://localhost:8080/v1/tasks/${TASK_UUID}/workflow_steps | jq '.[] | select(.name | startswith("process_large_batch_")) | {name, current_state, attempts, last_error}'
```

**Result**: All workers show `current_state: "Error"` with same OOM error in `last_error`.

**Operator Action**: Cancel entire task, will re-run with smaller batch size.

```bash
DLQ_ENTRY_UUID="dlq-entry-uuid-here"

# Cancel task (cancels all workers)
curl -X DELETE http://localhost:8080/v1/tasks/${TASK_UUID}

# Update DLQ entry to track resolution
curl -X PATCH http://localhost:8080/v1/dlq/entry/${DLQ_ENTRY_UUID} \
  -H "Content-Type: application/json" \
  -d '{
    "resolution_status": "permanently_failed",
    "resolution_notes": "Batch size too large causing OOM. Cancelled task and created new task with batch_size: 5000 instead of 10000",
    "resolved_by": "operator@example.com",
    "metadata": {
      "root_cause": "configuration_error",
      "corrective_action": "reduced_batch_size",
      "new_task_uuid": "new-task-uuid-here"
    }
  }'

# Create new task with corrected configuration
curl -X POST http://localhost:8080/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "namespace": "data_processing",
    "template_name": "large_dataset_processor",
    "context": {
      "dataset_id": "dataset-123",
      "batch_size": 5000,
      "max_workers": 20
    }
  }'
```

### DLQ Query Patterns for Batch Processing

**1. Find DLQ entry for a batch processing task**:
```sql
-- Get DLQ entry with task snapshot
SELECT
    dlq.dlq_entry_uuid,
    dlq.task_uuid,
    dlq.dlq_reason,
    dlq.resolution_status,
    dlq.dlq_timestamp,
    dlq.resolution_notes,
    dlq.resolved_by,
    dlq.task_snapshot->'namespace_name' as namespace,
    dlq.task_snapshot->'template_name' as template,
    dlq.task_snapshot->'current_state' as task_state
FROM tasker.tasks_dlq dlq
WHERE dlq.task_uuid = :task_uuid
  AND dlq.resolution_status = 'pending'
ORDER BY dlq.dlq_timestamp DESC
LIMIT 1;
```

**2. Check batch completion progress**:
```sql
SELECT
    COUNT(*) FILTER (WHERE ws.current_state = 'Complete') as completed_workers,
    COUNT(*) FILTER (WHERE ws.current_state = 'InProgress') as in_progress_workers,
    COUNT(*) FILTER (WHERE ws.current_state = 'Error') as failed_workers,
    COUNT(*) FILTER (WHERE ws.current_state = 'Pending') as pending_workers,
    COUNT(*) FILTER (WHERE ws.current_state = 'WaitingForRetry') as waiting_retry,
    COUNT(*) as total_workers
FROM tasker.workflow_steps ws
WHERE ws.task_uuid = :task_uuid
  AND ws.name LIKE 'process_%_batch_%';
```

**3. Find workers with stale checkpoints**:
```sql
SELECT
    ws.workflow_step_uuid,
    ws.name,
    ws.current_state,
    ws.results->>'last_checkpoint_cursor' as checkpoint_cursor,
    ws.results->>'checkpoint_timestamp' as checkpoint_time,
    NOW() - (ws.results->>'checkpoint_timestamp')::timestamptz as time_since_checkpoint,
    ws.attempts,
    ws.last_error
FROM tasker.workflow_steps ws
WHERE ws.task_uuid = :task_uuid
  AND ws.name LIKE 'process_%_batch_%'
  AND ws.current_state = 'InProgress'
  AND ws.results->>'checkpoint_timestamp' IS NOT NULL
  AND NOW() - (ws.results->>'checkpoint_timestamp')::timestamptz > interval '15 minutes'
ORDER BY time_since_checkpoint DESC;
```

**4. Get aggregated batch task health**:
```sql
SELECT
    t.task_uuid,
    t.namespace_name,
    t.template_name,
    t.current_state as task_state,
    t.execution_status,
    COUNT(DISTINCT ws.workflow_step_uuid) FILTER (WHERE ws.name LIKE 'process_%_batch_%') as worker_count,
    jsonb_object_agg(
        ws.current_state,
        COUNT(*)
    ) FILTER (WHERE ws.name LIKE 'process_%_batch_%') as worker_states,
    dlq.dlq_reason,
    dlq.resolution_status
FROM tasker.tasks t
JOIN tasker.workflow_steps ws ON ws.task_uuid = t.task_uuid
LEFT JOIN tasker.tasks_dlq dlq ON dlq.task_uuid = t.task_uuid
    AND dlq.resolution_status = 'pending'
WHERE t.task_uuid = :task_uuid
GROUP BY t.task_uuid, t.namespace_name, t.template_name, t.current_state, t.execution_status,
         dlq.dlq_reason, dlq.resolution_status;
```

**5. Find all batch tasks in DLQ**:
```sql
-- Find tasks with batch workers that are stuck
SELECT
    dlq.dlq_entry_uuid,
    dlq.task_uuid,
    dlq.dlq_reason,
    dlq.dlq_timestamp,
    t.namespace_name,
    t.template_name,
    t.current_state,
    COUNT(DISTINCT ws.workflow_step_uuid) FILTER (WHERE ws.name LIKE 'process_%_batch_%') as batch_worker_count,
    COUNT(DISTINCT ws.workflow_step_uuid) FILTER (WHERE ws.current_state = 'Error' AND ws.name LIKE 'process_%_batch_%') as failed_workers
FROM tasker.tasks_dlq dlq
JOIN tasker.tasks t ON t.task_uuid = dlq.task_uuid
JOIN tasker.workflow_steps ws ON ws.task_uuid = dlq.task_uuid
WHERE dlq.resolution_status = 'pending'
GROUP BY dlq.dlq_entry_uuid, dlq.task_uuid, dlq.dlq_reason, dlq.dlq_timestamp,
         t.namespace_name, t.template_name, t.current_state
HAVING COUNT(DISTINCT ws.workflow_step_uuid) FILTER (WHERE ws.name LIKE 'process_%_batch_%') > 0
ORDER BY dlq.dlq_timestamp DESC;
```

### Operator Dashboard Recommendations

For monitoring batch processing tasks, operators should have dashboards showing:

1. **Batch Progress**:
   - Total workers vs completed workers
   - Estimated completion time based on worker velocity
   - Current throughput (items/second across all workers)

2. **Stale Worker Alerts**:
   - Workers exceeding duration threshold
   - Workers with stale checkpoints
   - Workers with repeated failures

3. **Batch Health Metrics**:
   - Success rate per batch
   - Average processing time per worker
   - Resource utilization (CPU, memory)

4. **Resolution Actions**:
   - Recent operator interventions
   - Resolution action distribution (ResetForRetry vs ResolveManually)
   - Time to resolution for stale workers

---

## Code Examples

### Complete Working Example: CSV Product Inventory

This section shows a complete end-to-end implementation processing a 1000-row CSV file in 5 parallel batches.

#### Rust Implementation

**1. Batchable Handler**: `workers/rust/src/step_handlers/batch_processing_products_csv.rs:60-150`

```rust
pub struct CsvAnalyzerHandler;

#[async_trait]
impl StepHandler for CsvAnalyzerHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get CSV file path from task context
        let csv_file_path = step_data
            .task
            .context
            .get("csv_file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing csv_file_path in task context"))?;

        // Count total data rows (excluding header)
        let file = File::open(csv_file_path)?;
        let reader = BufReader::new(file);
        let total_rows = reader.lines().count().saturating_sub(1) as u64;

        info!("CSV Analysis: {} rows in {}", total_rows, csv_file_path);

        // Get batch configuration
        let handler_init = step_data.handler_initialization.as_object().unwrap();
        let batch_size = handler_init
            .get("batch_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(200);
        let max_workers = handler_init
            .get("max_workers")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        // Determine if batching needed
        if total_rows == 0 {
            let outcome = BatchProcessingOutcome::no_batches();
            let elapsed_ms = start_time.elapsed().as_millis() as u64;

            return Ok(success_result(
                step_uuid,
                json!({
                    "batch_processing_outcome": outcome.to_value(),
                    "reason": "empty_dataset",
                    "total_rows": 0
                }),
                elapsed_ms,
                None,
            ));
        }

        // Calculate worker count
        let worker_count = ((total_rows as f64 / batch_size as f64).ceil() as u64)
            .min(max_workers);

        // Generate cursor configurations
        let actual_batch_size = (total_rows as f64 / worker_count as f64).ceil() as u64;
        let mut cursor_configs = Vec::new();

        for i in 0..worker_count {
            let start_row = (i * actual_batch_size) + 1; // 1-indexed after header
            let end_row = ((i + 1) * actual_batch_size).min(total_rows) + 1;

            cursor_configs.push(CursorConfig {
                batch_id: format!("{:03}", i + 1),
                start_cursor: json!(start_row),
                end_cursor: json!(end_row),
                batch_size: (end_row - start_row) as u32,
            });
        }

        info!(
            "Creating {} batch workers for {} rows (batch_size: {})",
            worker_count, total_rows, actual_batch_size
        );

        // Return CreateBatches outcome
        let outcome = BatchProcessingOutcome::create_batches(
            "process_csv_batch".to_string(),
            worker_count as u32,
            cursor_configs,
            total_rows,
        );

        let elapsed_ms = start_time.elapsed().as_millis() as u64;

        Ok(success_result(
            step_uuid,
            json!({
                "batch_processing_outcome": outcome.to_value(),
                "worker_count": worker_count,
                "total_rows": total_rows,
                "csv_file_path": csv_file_path
            }),
            elapsed_ms,
            Some(json!({
                "batch_size": actual_batch_size,
                "file_path": csv_file_path
            })),
        ))
    }
}
```

**2. Batch Worker Handler**: `workers/rust/src/step_handlers/batch_processing_products_csv.rs:200-350`

```rust
pub struct CsvBatchProcessorHandler;

#[derive(Debug, Deserialize)]
struct Product {
    id: u32,
    title: String,
    category: String,
    price: f64,
    stock: u32,
    rating: f64,
}

#[async_trait]
impl StepHandler for CsvBatchProcessorHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract batch worker context using helper
        let context = BatchWorkerContext::from_step_data(step_data)?;

        // Check for no-op placeholder worker
        if context.is_no_op() {
            let elapsed_ms = start_time.elapsed().as_millis() as u64;
            return Ok(success_result(
                step_uuid,
                json!({
                    "no_op": true,
                    "reason": "NoBatches scenario - no items to process",
                    "batch_id": context.batch_id()
                }),
                elapsed_ms,
                None,
            ));
        }

        // Get CSV file path from dependency results
        let csv_file_path = step_data
            .dependency_results
            .get("analyze_csv")
            .and_then(|r| r.result.get("csv_file_path"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing csv_file_path from analyze_csv"))?;

        // Extract cursor range
        let start_row = context.start_position();
        let end_row = context.end_position();

        info!(
            "Processing batch {} (rows {}-{})",
            context.batch_id(),
            start_row,
            end_row
        );

        // Initialize metrics
        let mut processed_count = 0u64;
        let mut total_inventory_value = 0.0;
        let mut category_counts: HashMap<String, u32> = HashMap::new();
        let mut max_price = 0.0;
        let mut max_price_product = None;
        let mut total_rating = 0.0;

        // Open CSV and process rows in cursor range
        let file = File::open(Path::new(csv_file_path))?;
        let mut csv_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(BufReader::new(file));

        for (row_idx, result) in csv_reader.deserialize::<Product>().enumerate() {
            let data_row_num = row_idx + 1; // 1-indexed after header

            if data_row_num < start_row {
                continue; // Skip rows before our range
            }
            if data_row_num >= end_row {
                break; // Processed all our rows
            }

            let product: Product = result?;

            // Calculate inventory metrics
            let inventory_value = product.price * (product.stock as f64);
            total_inventory_value += inventory_value;

            *category_counts.entry(product.category.clone()).or_insert(0) += 1;

            if product.price > max_price {
                max_price = product.price;
                max_price_product = Some(product.title.clone());
            }

            total_rating += product.rating;
            processed_count += 1;

            // Checkpoint progress periodically
            if processed_count % context.checkpoint_interval() == 0 {
                debug!(
                    "Checkpoint: batch {} processed {} items",
                    context.batch_id(),
                    processed_count
                );
            }
        }

        let average_rating = if processed_count > 0 {
            total_rating / (processed_count as f64)
        } else {
            0.0
        };

        let elapsed_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "Batch {} complete: {} items processed",
            context.batch_id(),
            processed_count
        );

        Ok(success_result(
            step_uuid,
            json!({
                "processed_count": processed_count,
                "total_inventory_value": total_inventory_value,
                "category_counts": category_counts,
                "max_price": max_price,
                "max_price_product": max_price_product,
                "average_rating": average_rating,
                "batch_id": context.batch_id(),
                "start_row": start_row,
                "end_row": end_row
            }),
            elapsed_ms,
            None,
        ))
    }
}
```

**3. Convergence Handler**: `workers/rust/src/step_handlers/batch_processing_products_csv.rs:400-520`

```rust
pub struct CsvResultsAggregatorHandler;

#[async_trait]
impl StepHandler for CsvResultsAggregatorHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Detect scenario using helper
        let scenario = BatchAggregationScenario::detect(
            &step_data.dependency_results,
            "analyze_csv",
            "process_csv_batch_",
        )?;

        let (total_processed, total_inventory_value, category_counts, max_price, max_price_product, overall_avg_rating, worker_count) = match scenario {
            BatchAggregationScenario::NoBatches { batchable_result } => {
                // No batch workers - get dataset size from batchable step
                let total_rows = batchable_result
                    .result
                    .get("total_rows")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                info!("NoBatches scenario: {} rows (no processing needed)", total_rows);

                (total_rows, 0.0, HashMap::new(), 0.0, None, 0.0, 0)
            }

            BatchAggregationScenario::WithBatches {
                batch_results,
                worker_count,
            } => {
                info!("Aggregating results from {} batch workers", worker_count);

                let mut total_processed = 0u64;
                let mut total_inventory_value = 0.0;
                let mut global_category_counts: HashMap<String, u64> = HashMap::new();
                let mut max_price = 0.0;
                let mut max_price_product = None;
                let mut weighted_ratings = Vec::new();

                for (step_name, result) in batch_results {
                    // Sum processed counts
                    let count = result
                        .result
                        .get("processed_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    total_processed += count;

                    // Sum inventory values
                    let value = result
                        .result
                        .get("total_inventory_value")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    total_inventory_value += value;

                    // Merge category counts
                    if let Some(categories) = result
                        .result
                        .get("category_counts")
                        .and_then(|v| v.as_object())
                    {
                        for (category, cat_count) in categories {
                            *global_category_counts
                                .entry(category.clone())
                                .or_insert(0) += cat_count.as_u64().unwrap_or(0);
                        }
                    }

                    // Find global max price
                    let batch_max_price = result
                        .result
                        .get("max_price")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    if batch_max_price > max_price {
                        max_price = batch_max_price;
                        max_price_product = result
                            .result
                            .get("max_price_product")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }

                    // Collect ratings for weighted average
                    let avg_rating = result
                        .result
                        .get("average_rating")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    weighted_ratings.push((count, avg_rating));
                }

                // Calculate overall weighted average rating
                let total_items = weighted_ratings.iter().map(|(c, _)| c).sum::<u64>();
                let overall_avg_rating = if total_items > 0 {
                    weighted_ratings
                        .iter()
                        .map(|(count, avg)| (*count as f64) * avg)
                        .sum::<f64>()
                        / (total_items as f64)
                } else {
                    0.0
                };

                (
                    total_processed,
                    total_inventory_value,
                    global_category_counts,
                    max_price,
                    max_price_product,
                    overall_avg_rating,
                    worker_count,
                )
            }
        };

        let elapsed_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "Aggregation complete: {} total items processed by {} workers",
            total_processed, worker_count
        );

        Ok(success_result(
            step_uuid,
            json!({
                "total_processed": total_processed,
                "total_inventory_value": total_inventory_value,
                "category_counts": category_counts,
                "max_price": max_price,
                "max_price_product": max_price_product,
                "overall_average_rating": overall_avg_rating,
                "worker_count": worker_count
            }),
            elapsed_ms,
            None,
        ))
    }
}
```

#### Ruby Implementation

**1. Batchable Handler**: `workers/ruby/spec/handlers/examples/batch_processing/step_handlers/csv_analyzer_handler.rb`

```ruby
module BatchProcessing
  module StepHandlers
    # CSV Analyzer - Batchable Step
    class CsvAnalyzerHandler < TaskerCore::StepHandler::Batchable
      def call(task, _sequence, step)
        csv_file_path = task.context['csv_file_path']
        raise ArgumentError, 'Missing csv_file_path in task context' unless csv_file_path

        # Count CSV rows (excluding header)
        total_rows = count_csv_rows(csv_file_path)

        Rails.logger.info("CSV Analysis: #{total_rows} rows in #{csv_file_path}")

        # Get batch configuration from handler initialization
        batch_size = step_definition_initialization['batch_size'] || 200
        max_workers = step_definition_initialization['max_workers'] || 5

        # Calculate worker count
        worker_count = [(total_rows.to_f / batch_size).ceil, max_workers].min

        if worker_count.zero? || total_rows.zero?
          # Use helper for NoBatches outcome
          return no_batches_success(
            reason: 'empty_dataset',
            total_rows: total_rows
          )
        end

        # Generate cursor configs using helper
        cursor_configs = generate_cursor_configs(
          total_items: total_rows,
          worker_count: worker_count
        ) do |batch_idx, start_pos, end_pos, items_in_batch|
          # Adjust to 1-indexed row numbers (after header)
          {
            'batch_id' => format('%03d', batch_idx + 1),
            'start_cursor' => start_pos + 1,
            'end_cursor' => end_pos + 1,
            'batch_size' => items_in_batch
          }
        end

        Rails.logger.info("Creating #{worker_count} batch workers for #{total_rows} rows")

        # Use helper for CreateBatches outcome
        create_batches_success(
          worker_template_name: 'process_csv_batch',
          worker_count: worker_count,
          cursor_configs: cursor_configs,
          total_items: total_rows,
          additional_data: {
            'csv_file_path' => csv_file_path
          }
        )
      end

      private

      def count_csv_rows(csv_file_path)
        CSV.read(csv_file_path, headers: true).length
      end

      def step_definition_initialization
        @step_definition_initialization ||= {}
      end
    end
  end
end
```

**2. Batch Worker Handler**: `workers/ruby/spec/handlers/examples/batch_processing/step_handlers/csv_batch_processor_handler.rb`

```ruby
module BatchProcessing
  module StepHandlers
    # CSV Batch Processor - Batch Worker
    class CsvBatchProcessorHandler < TaskerCore::StepHandler::Batchable
      Product = Struct.new(
        :id, :title, :description, :category, :price,
        :discount_percentage, :rating, :stock, :brand, :sku, :weight,
        keyword_init: true
      )

      def call(context)
        # Extract batch context using helper
        batch_ctx = get_batch_context(context)

        # Use helper to check for no-op worker
        no_op_result = handle_no_op_worker(batch_ctx)
        return no_op_result if no_op_result

        # Get CSV file path from dependency results
        csv_file_path = context.get_dependency_result('analyze_csv')&.dig('csv_file_path')
        raise ArgumentError, 'Missing csv_file_path from analyze_csv' unless csv_file_path

        Rails.logger.info("Processing batch #{batch_ctx.batch_id} (rows #{batch_ctx.start_cursor}-#{batch_ctx.end_cursor})")

        # Process CSV rows in cursor range
        metrics = process_csv_batch(
          csv_file_path,
          batch_ctx.start_cursor,
          batch_ctx.end_cursor
        )

        Rails.logger.info("Batch #{batch_ctx.batch_id} complete: #{metrics[:processed_count]} items processed")

        # Return results for aggregation
        success(
          result_data: {
            'processed_count' => metrics[:processed_count],
            'total_inventory_value' => metrics[:total_inventory_value],
            'category_counts' => metrics[:category_counts],
            'max_price' => metrics[:max_price],
            'max_price_product' => metrics[:max_price_product],
            'average_rating' => metrics[:average_rating],
            'batch_id' => batch_ctx.batch_id,
            'start_row' => batch_ctx.start_cursor,
            'end_row' => batch_ctx.end_cursor
          }
        )
      end

      private

      def process_csv_batch(csv_file_path, start_row, end_row)
        metrics = {
          processed_count: 0,
          total_inventory_value: 0.0,
          category_counts: Hash.new(0),
          max_price: 0.0,
          max_price_product: nil,
          ratings: []
        }

        CSV.foreach(csv_file_path, headers: true).with_index(1) do |row, data_row_num|
          # Skip rows before our range
          next if data_row_num < start_row
          # Break when we've processed all our rows
          break if data_row_num >= end_row

          product = parse_product(row)

          # Calculate inventory metrics
          inventory_value = product.price * product.stock
          metrics[:total_inventory_value] += inventory_value

          metrics[:category_counts][product.category] += 1

          if product.price > metrics[:max_price]
            metrics[:max_price] = product.price
            metrics[:max_price_product] = product.title
          end

          metrics[:ratings] << product.rating
          metrics[:processed_count] += 1
        end

        # Calculate average rating
        metrics[:average_rating] = if metrics[:ratings].any?
                                      metrics[:ratings].sum / metrics[:ratings].size.to_f
                                    else
                                      0.0
                                    end

        metrics.except(:ratings)
      end

      def parse_product(row)
        Product.new(
          id: row['id'].to_i,
          title: row['title'],
          description: row['description'],
          category: row['category'],
          price: row['price'].to_f,
          discount_percentage: row['discountPercentage'].to_f,
          rating: row['rating'].to_f,
          stock: row['stock'].to_i,
          brand: row['brand'],
          sku: row['sku'],
          weight: row['weight'].to_i
        )
      end
    end
  end
end
```

**3. Convergence Handler**: `workers/ruby/spec/handlers/examples/batch_processing/step_handlers/csv_results_aggregator_handler.rb`

```ruby
module BatchProcessing
  module StepHandlers
    # CSV Results Aggregator - Deferred Convergence Step
    class CsvResultsAggregatorHandler < TaskerCore::StepHandler::Batchable
      def call(_task, sequence, _step)
        # Detect scenario using helper
        scenario = detect_aggregation_scenario(
          sequence,
          batchable_step_name: 'analyze_csv',
          batch_worker_prefix: 'process_csv_batch_'
        )

        # Use helper for aggregation with custom block
        aggregate_batch_worker_results(scenario) do |batch_results|
          aggregate_csv_metrics(batch_results)
        end
      end

      private

      def aggregate_csv_metrics(batch_results)
        total_processed = 0
        total_inventory_value = 0.0
        global_category_counts = Hash.new(0)
        max_price = 0.0
        max_price_product = nil
        weighted_ratings = []

        batch_results.each do |step_name, batch_result|
          result = batch_result['result'] || {}

          # Sum processed counts
          count = result['processed_count'] || 0
          total_processed += count

          # Sum inventory values
          total_inventory_value += result['total_inventory_value'] || 0.0

          # Merge category counts
          (result['category_counts'] || {}).each do |category, cat_count|
            global_category_counts[category] += cat_count
          end

          # Find global max price
          batch_max_price = result['max_price'] || 0.0
          if batch_max_price > max_price
            max_price = batch_max_price
            max_price_product = result['max_price_product']
          end

          # Collect ratings for weighted average
          avg_rating = result['average_rating'] || 0.0
          weighted_ratings << { count: count, avg: avg_rating }
        end

        # Calculate overall weighted average rating
        total_items = weighted_ratings.sum { |r| r[:count] }
        overall_avg_rating = if total_items.positive?
                               weighted_ratings.sum { |r| r[:avg] * r[:count] } / total_items.to_f
                             else
                               0.0
                             end

        Rails.logger.info("Aggregation complete: #{total_processed} total items processed by #{batch_results.size} workers")

        {
          'total_processed' => total_processed,
          'total_inventory_value' => total_inventory_value,
          'category_counts' => global_category_counts,
          'max_price' => max_price,
          'max_price_product' => max_price_product,
          'overall_average_rating' => overall_avg_rating,
          'worker_count' => batch_results.size
        }
      end
    end
  end
end
```

#### YAML Template

**File**: `tests/fixtures/task_templates/rust/batch_processing_products_csv.yaml`

```yaml
---
name: csv_product_inventory_analyzer
namespace_name: csv_processing
version: "1.0.0"
description: "Process CSV product data in parallel batches"
task_handler:
  callable: rust_handler
  initialization: {}

steps:
  # BATCHABLE STEP: CSV Analysis and Batch Planning
  - name: analyze_csv
    type: batchable
    dependencies: []
    handler:
      callable: CsvAnalyzerHandler
      initialization:
        batch_size: 200
        max_workers: 5

  # BATCH WORKER TEMPLATE: Single CSV Batch Processing
  # Orchestration creates N instances from this template
  - name: process_csv_batch
    type: batch_worker
    dependencies:
      - analyze_csv
    lifecycle:
      max_steps_in_process_minutes: 120
      max_retries: 3
      backoff_multiplier: 2.0
    handler:
      callable: CsvBatchProcessorHandler
      initialization:
        operation: "inventory_analysis"

  # DEFERRED CONVERGENCE STEP: CSV Results Aggregation
  - name: aggregate_csv_results
    type: deferred_convergence
    dependencies:
      - process_csv_batch  # Template dependency - resolves to all worker instances
    handler:
      callable: CsvResultsAggregatorHandler
      initialization:
        aggregation_type: "inventory_metrics"
```

---

## Best Practices

### 1. Batch Size Calculation

**Guideline**: Balance parallelism with overhead.

**Too Small**:
- Excessive orchestration overhead
- Too many database transactions
- Diminishing returns on parallelism

**Too Large**:
- Workers timeout or OOM
- Long retry times on failure
- Reduced parallelism

**Recommended Approach**:

```ruby
def calculate_optimal_batch_size(total_items, item_processing_time_ms)
  # Target: Each batch takes 5-10 minutes
  target_duration_ms = 7.5 * 60 * 1000

  # Calculate items per batch
  items_per_batch = (target_duration_ms / item_processing_time_ms).ceil

  # Enforce min/max bounds
  [[items_per_batch, 100].max, 10000].min
end
```

### 2. Worker Count Limits

**Guideline**: Limit concurrency based on system resources.

```yaml
handler:
  initialization:
    batch_size: 200
    max_workers: 10  # Prevents creating 100 workers for 20,000 items
```

**Considerations**:
- Database connection pool size
- Memory per worker
- External API rate limits
- CPU cores available

### 3. Cursor Design

**Guideline**: Use cursors that support resumability.

**Good Cursor Types**:
- ✅ Integer offsets: `start_cursor: 1000, end_cursor: 2000`
- ✅ Timestamps: `start_cursor: "2025-01-01T00:00:00Z"`
- ✅ Database IDs: `start_cursor: uuid_a, end_cursor: uuid_b`
- ✅ Composite keys: `{ date: "2025-01-01", partition: "US-WEST" }`

**Bad Cursor Types**:
- ❌ Page numbers (data can shift between pages)
- ❌ Non-deterministic ordering (random, relevance scores)
- ❌ Mutable values (last_modified_at can change)

### 4. Checkpoint Frequency

**Guideline**: Balance resumability with performance.

```rust
// Checkpoint every 100 items
if processed_count % 100 == 0 {
    checkpoint_progress(step_uuid, current_cursor).await?;
}
```

**Factors**:
- Item processing time (faster = higher frequency)
- Worker failure rate (higher = more frequent checkpoints)
- Database write overhead (less frequent = better performance)

**Recommended**:
- Fast items (< 10ms each): Checkpoint every 1000 items
- Medium items (10-100ms each): Checkpoint every 100 items
- Slow items (> 100ms each): Checkpoint every 10 items

### 5. Error Handling Strategies

**FailFast** (default):
```rust
FailureStrategy::FailFast
```
- Worker fails immediately on first error
- Suitable for: Data validation, schema violations
- Retry preserves cursor for retry

**ContinueOnFailure**:
```rust
FailureStrategy::ContinueOnFailure
```
- Worker processes all items, collects errors
- Suitable for: Best-effort processing, partial results acceptable
- Returns both results and error list

**IsolateFailed**:
```rust
FailureStrategy::IsolateFailed
```
- Failed items moved to separate queue
- Suitable for: Large batches with few expected failures
- Allows manual review of failed items

### 6. Aggregation Patterns

**Sum/Count**:
```rust
let total = batch_results.iter()
    .map(|(_, r)| r.result.get("count").unwrap().as_u64().unwrap())
    .sum::<u64>();
```

**Max/Min**:
```rust
let max_value = batch_results.iter()
    .filter_map(|(_, r)| r.result.get("max").and_then(|v| v.as_f64()))
    .max_by(|a, b| a.partial_cmp(b).unwrap())
    .unwrap_or(0.0);
```

**Weighted Average**:
```rust
let total_weight: u64 = weighted_values.iter().map(|(w, _)| w).sum();
let weighted_avg = weighted_values.iter()
    .map(|(weight, value)| (*weight as f64) * value)
    .sum::<f64>() / (total_weight as f64);
```

**Merge HashMaps**:
```rust
let mut merged = HashMap::new();
for (_, result) in batch_results {
    if let Some(counts) = result.get("counts").and_then(|v| v.as_object()) {
        for (key, count) in counts {
            *merged.entry(key.clone()).or_insert(0) += count.as_u64().unwrap();
        }
    }
}
```

### 7. Testing Strategies

**Unit Tests**: Test handler logic independently
```rust
#[test]
fn test_cursor_generation() {
    let configs = create_cursor_configs(1000, 5, 200);
    assert_eq!(configs.len(), 5);
    assert_eq!(configs[0].start_cursor, json!(0));
    assert_eq!(configs[0].end_cursor, json!(200));
}
```

**Integration Tests**: Test with small datasets
```rust
#[tokio::test]
async fn test_batch_processing_integration() {
    let task = create_task_with_csv("test_data_10_rows.csv").await;
    assert_eq!(task.current_state, TaskState::Complete);

    let steps = get_workflow_steps(task.task_uuid).await;
    let workers = steps.iter().filter(|s| s.step_type == "batch_worker").count();
    assert_eq!(workers, 1); // 10 rows = 1 worker with batch_size 200
}
```

**E2E Tests**: Test complete workflow with realistic data
```rust
#[tokio::test]
async fn test_csv_batch_processing_e2e() {
    let task = create_task_with_csv("products_1000_rows.csv").await;
    wait_for_completion(task.task_uuid, Duration::from_secs(60)).await;

    let results = get_aggregation_results(task.task_uuid).await;
    assert_eq!(results["total_processed"], 1000);
    assert_eq!(results["worker_count"], 5);
}
```

### 8. Monitoring and Observability

**Metrics to Track**:
- Worker creation time
- Individual worker duration
- Batch size distribution
- Retry rate per batch
- Aggregation duration

**Recommended Dashboards**:
```sql
-- Batch processing health
SELECT
    COUNT(*) FILTER (WHERE step_type = 'batch_worker') as total_workers,
    AVG(EXTRACT(EPOCH FROM (updated_at - created_at))) as avg_worker_duration_sec,
    MAX(EXTRACT(EPOCH FROM (updated_at - created_at))) as max_worker_duration_sec,
    COUNT(*) FILTER (WHERE current_state = 'Error') as failed_workers
FROM tasker.workflow_steps
WHERE task_uuid = :task_uuid
  AND step_type = 'batch_worker';
```

## Summary

Batch processing in Tasker provides a robust, production-ready pattern for parallel dataset processing:

**Key Strengths**:
- ✅ Builds on proven DAG, retry, and deferred convergence foundations
- ✅ No special recovery system needed (uses standard DLQ + retry)
- ✅ Transaction-based worker creation prevents corruption
- ✅ Cursor-based resumability enables long-running processing
- ✅ Language-agnostic design works across Rust and Ruby workers

**Integration Points**:
- **DAG**: Workers are full nodes with standard lifecycle
- **Retryability**: Uses `lifecycle.max_retries` and exponential backoff
- **Deferred Convergence**: Intersection semantics aggregate dynamic worker counts
- **DLQ**: Standard operator workflows with cursor preservation

**Production Readiness** (TAS-59 Complete):
- 908 tests passing (Ruby workers)
- Real-world CSV processing (1000 rows)
- Docker integration working
- Code review complete with recommended fixes
- Ready for PR submission

**For More Information**:
- **Implementation Details**: See [TAS-59](https://linear.app/tasker-systems/issue/TAS-59)
- **Conditional Workflows**: See `docs/conditional-workflows.md`
- **DLQ System**: See `docs/dlq-system.md`
- **Code Examples**: See `workers/rust/src/step_handlers/batch_processing_*.rs`

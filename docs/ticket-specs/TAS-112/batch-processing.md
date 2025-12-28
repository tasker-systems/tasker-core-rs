# TAS-112 Phase 6: Batch Processing Lifecycle Analysis

**Phase**: 6 of 9  
**Created**: 2025-12-27  
**Status**: Analysis Complete  
**Prerequisites**: Phase 5 (Batchable Handler Pattern)

---

## Executive Summary

This analysis examines the **batch processing lifecycle** - the data structures, orchestration flow, and cross-language boundaries that enable parallel batch processing. Unlike Phase 5 (handler helpers), this phase focuses on the **workflow orchestration** and **data structures at FFI boundaries** between workers and Rust orchestration.

**Key Findings**:
1. **✅ Excellent FFI Boundary Alignment** - Data structures serialize consistently across languages
2. **✅ CursorConfig Flexibility** - Rust uses `Value` for flexible cursor types (int, UUID, timestamp, etc.)
3. **⚠️ BatchWorkerInputs Structure** - Not fully documented in Python/Ruby, well-defined in TypeScript/Rust
4. **✅ Checkpoint Mechanism** - Consistent approach across languages using workflow_step.results
5. **✅ Aggregation Scenario Detection** - Ruby has helpers, Python/TypeScript need standardization

**Guiding Principle** (Zen of Python): *"Explicit is better than implicit."*

**Project Context**: Pre-alpha greenfield. FFI boundaries are critical - must maintain cross-language compatibility.

---

## Batch Processing Lifecycle Overview

### Three-Phase Pattern

```
Phase 1: ANALYSIS (Batchable Step)
├─ Handler analyzes dataset size
├─ Calculates worker count and cursor ranges
└─ Returns BatchProcessingOutcome

Phase 2: ORCHESTRATION (Rust Service)
├─ Receives BatchProcessingOutcome from worker
├─ Creates N worker instances from template
├─ Assigns CursorConfig to each worker
├─ Creates convergence step with intersection semantics
└─ Enqueues workers for parallel execution

Phase 3: AGGREGATION (Convergence Step)
├─ Waits for all workers (deferred convergence)
├─ Detects scenario (NoBatches vs WithBatches)
├─ Aggregates results from completed workers
└─ Returns combined metrics
```

**Key Insight**: Orchestration (Phase 2) is **100% Rust** - all other languages communicate via FFI-safe data structures embedded in `StepExecutionResult.result` field.

---

## Data Structures at FFI Boundaries

### BatchProcessingOutcome (Worker → Orchestration)

**Purpose**: Batchable handler returns this in `result` field to instruct orchestration.

**Location**: `tasker-shared/src/messaging/execution_types.rs`

#### Rust Definition

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchProcessingOutcome {
    /// No batches needed - create placeholder worker
    NoBranches,
    
    /// Create N batch workers with cursor configurations
    CreateBatches {
        worker_template_name: String,
        worker_count: u32,
        cursor_configs: Vec<CursorConfig>,
        total_items: u64,
    },
}
```

**JSON Serialization**:
```json
// NoBatches
{
  "type": "no_batches"
}

// CreateBatches
{
  "type": "create_batches",
  "worker_template_name": "process_csv_batch",
  "worker_count": 5,
  "cursor_configs": [
    {
      "batch_id": "001",
      "start_cursor": 0,
      "end_cursor": 200,
      "batch_size": 200
    },
    // ... more configs
  ],
  "total_items": 1000
}
```

#### Ruby Type Mirror

**Location**: `workers/ruby/lib/tasker_core/types/batch_processing_outcome.rb`

```ruby
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
  end
end
```

**Analysis**:
- ✅ **Identical serialization** - `to_h` matches Rust JSON
- ✅ **Dry-Struct validation** - constraints match Rust types
- ✅ **Factory methods** - `no_batches()`, `create_batches()`

#### Python Type Mirror

**Location**: `python/tasker_core/types.py`

```python
# No dedicated BatchProcessingOutcome class!
# Constructed inline in batchable.py helpers
```

**Pattern** (from Phase 5):
```python
def batch_analyzer_success(self, outcome, metadata=None, worker_template_name="batch_worker"):
    # Build batch_processing_outcome in format Rust expects
    batch_processing_outcome = {
        "type": "create_batches",
        "worker_template_name": worker_template_name,
        "worker_count": len(outcome.cursor_configs),
        "cursor_configs": formatted_configs,
        "total_items": outcome.total_items
    }
    result = {"batch_processing_outcome": batch_processing_outcome, ...}
    return self.success(result, metadata=metadata)
```

**Analysis**:
- ⚠️ **No type class** - constructed as dict inline
- ✅ **Correct serialization** - matches Rust structure
- ⚠️ **Less type safety** - no Pydantic validation for outcome itself

#### TypeScript Type Mirror

**Location**: `src/handler/batchable.ts`

```typescript
// Also no dedicated BatchProcessingOutcome interface!
// Constructed inline in helpers
```

**Pattern** (from Phase 5):
```typescript
batchSuccess(
  workerTemplateName: string,
  batchConfigs: BatchWorkerConfig[],
  metadata?: Record<string, unknown>
): BatchableResult {
  const result = {
    batch_processing_outcome: {
      type: 'create_batches',
      worker_template_name: workerTemplateName,
      worker_count: batchConfigs.length,
      cursor_configs: cursorConfigs,
      total_items: totalItems
    },
    ...(metadata || {})
  };
  
  return StepHandlerResult.success(result, metadata);
}
```

**Analysis**:
- ⚠️ **No type interface** - constructed as object inline
- ✅ **Correct serialization** - matches Rust structure
- ⚠️ **Less type safety** - relies on handler helper correctness

**Cross-Language Comparison**:

| Feature | Rust | Ruby | Python | TypeScript |
|---------|------|------|--------|------------|
| **Dedicated Type** | ✅ Enum | ✅ Dry::Struct classes | ❌ Inline dict | ❌ Inline object |
| **Validation** | Serde | Dry-Struct | ❌ None | ❌ None |
| **Factory Methods** | ✅ | ✅ | ❌ (via helpers) | ❌ (via helpers) |
| **Serialization Format** | ✅ Identical | ✅ Identical | ✅ Identical | ✅ Identical |
| **FFI Safety** | ✅ | ✅ | ✅ | ✅ |

**Recommendation**:
- ✅ **Python**: Create `BatchProcessingOutcome` Pydantic model for type safety
- ✅ **TypeScript**: Create `BatchProcessingOutcome` interface
- 📝 **Zen Alignment**: "Explicit is better than implicit" - dedicated types improve safety

---

### CursorConfig (Worker Boundary Configuration)

**Purpose**: Defines the data range each batch worker should process.

#### Rust Definition

**Location**: `tasker-shared/src/messaging/execution_types.rs`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Batch identifier (e.g., "001", "002")
    pub batch_id: String,
    
    /// Starting position (inclusive) - FLEXIBLE TYPE!
    /// Can be: int, string, timestamp, UUID, object, etc.
    pub start_cursor: Value,
    
    /// Ending position (exclusive) - FLEXIBLE TYPE!
    pub end_cursor: Value,
    
    /// Number of items in this batch
    pub batch_size: u32,
}
```

**Key Feature**: `start_cursor` and `end_cursor` are `serde_json::Value` - **any JSON type allowed!**

**Flexibility Examples**:
```rust
// Integer cursors (most common)
CursorConfig {
    batch_id: "001".to_string(),
    start_cursor: json!(0),
    end_cursor: json!(1000),
    batch_size: 1000
}

// UUID cursors (for UUID-partitioned data)
CursorConfig {
    batch_id: "001".to_string(),
    start_cursor: json!("00000000-0000-0000-0000-000000000000"),
    end_cursor: json!("3fffffff-ffff-ffff-ffff-ffffffffffff"),
    batch_size: 1000
}

// Timestamp cursors (for time-series data)
CursorConfig {
    batch_id: "001".to_string(),
    start_cursor: json!("2024-01-01T00:00:00Z"),
    end_cursor: json!("2024-01-08T00:00:00Z"),
    batch_size: 7  // 7 days
}

// Composite cursors (for multi-key partitions)
CursorConfig {
    batch_id: "001".to_string(),
    start_cursor: json!({"region": "us-west", "page": 0}),
    end_cursor: json!({"region": "us-west", "page": 10}),
    batch_size: 10
}
```

#### Language Type Definitions

**Ruby**:
```ruby
# No dedicated CursorConfig class
# Represented as Hash with flexible cursor values
{
  'batch_id' => '001',
  'start_cursor' => 0,  # Can be any JSON-serializable value
  'end_cursor' => 1000,
  'batch_size' => 1000
}
```

**Python**:
```python
class CursorConfig(BaseModel):
    start_cursor: int  # ⚠️ LIMITATION: int only!
    end_cursor: int    # ⚠️ Should be Any
    step_size: int = 1
    metadata: dict[str, Any] = Field(default_factory=dict)
```

**TypeScript**:
```typescript
interface CursorConfig {
  startCursor: number;  // ⚠️ LIMITATION: number only!
  endCursor: number;    // ⚠️ Should be unknown
  stepSize: number;
  metadata: Record<string, unknown>;
}
```

**Cross-Language Comparison**:

| Feature | Rust | Ruby | Python | TypeScript |
|---------|------|------|--------|------------|
| **Cursor Type** | ✅ `Value` (any JSON) | ✅ Any (via Hash) | ❌ `int` only | ❌ `number` only |
| **batch_id** | ✅ String | ✅ String | ❌ Missing | ❌ Missing |
| **batch_size** | ✅ u32 | ✅ Integer | ⚠️ Implicit (end-start) | ⚠️ Implicit |

**Recommendation** (from Phase 5):
- ✅ **Python**: Change `start_cursor`/`end_cursor` from `int` to `Any`
- ✅ **TypeScript**: Change from `number` to `unknown`
- ✅ **Python/TypeScript**: Add `batch_id` field (currently missing)
- 📝 **Zen Alignment**: "Special cases aren't special enough to break the rules" - support all cursor types

---

### BatchWorkerInputs (Orchestration → Worker)

**Purpose**: Rust orchestration creates this structure and stores it in `workflow_steps.inputs` for each worker.

#### Rust Definition

**Location**: `tasker-orchestration/src/orchestration/lifecycle/batch_processing/service.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWorkerInputs {
    /// Cursor configuration for this worker
    pub cursor: CursorConfig,
    
    /// Metadata from analyzer (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_metadata: Option<serde_json::Value>,
    
    /// Flag indicating this is a no-op placeholder worker
    pub is_no_op: bool,
}
```

**Example** (stored in database):
```json
{
  "cursor": {
    "batch_id": "001",
    "start_cursor": 0,
    "end_cursor": 200,
    "batch_size": 200
  },
  "batch_metadata": {
    "checkpoint_interval": 100,
    "cursor_field": "id",
    "failure_strategy": "halt"
  },
  "is_no_op": false
}
```

#### Language Access Patterns

**Ruby**:
```ruby
def get_batch_context(context)
  # Extracts from context.workflow_step
  BatchProcessing::BatchWorkerContext.from_step_data(context.workflow_step)
end

# BatchWorkerContext structure
class BatchWorkerContext
  attr_reader :batch_id, :start_cursor, :end_cursor, :batch_size, 
              :batch_metadata, :no_op
  
  def self.from_step_data(workflow_step)
    # Reads workflow_step.inputs JSON
    inputs = workflow_step.inputs || {}
    cursor = inputs['cursor'] || {}
    
    new(
      batch_id: cursor['batch_id'],
      start_cursor: cursor['start_cursor'],
      end_cursor: cursor['end_cursor'],
      batch_size: cursor['batch_size'],
      batch_metadata: inputs['batch_metadata'] || {},
      no_op: inputs['is_no_op'] || false
    )
  end
end
```

**Python**:
```python
class BatchWorkerContext(BaseModel):
    batch_id: str
    cursor_config: CursorConfig
    batch_index: int
    total_batches: int
    batch_metadata: dict[str, Any] = Field(default_factory=dict)
    
    @classmethod
    def from_step_context(cls, context: StepContext):
        # Reads from context.step_inputs
        # ⚠️ Structure differs from Rust BatchWorkerInputs!
        ...
```

**TypeScript**:
```typescript
export interface RustBatchWorkerInputs {
  cursor: {
    batch_id: string;
    start_cursor: number;
    end_cursor: number;
    batch_size: number;
  };
  batch_metadata: {
    checkpoint_interval: number;
    cursor_field: string;
    failure_strategy: string;
  };
  is_no_op: boolean;
}

function getBatchWorkerInputs(context: StepContext): Partial<RustBatchWorkerInputs> | null {
  if (!context.stepInputs || Object.keys(context.stepInputs).length === 0) {
    return null;
  }
  return context.stepInputs as Partial<RustBatchWorkerInputs>;
}
```

**Analysis**:
- ✅ **TypeScript**: Explicitly models Rust structure (best alignment)
- ⚠️ **Python**: `BatchWorkerContext` doesn't match Rust `BatchWorkerInputs` exactly
- ✅ **Ruby**: `from_step_data()` correctly reads Rust structure
- 📝 **Key Point**: This is an **FFI boundary** - must match Rust exactly

**Recommendation**:
- ✅ **Python**: Align `BatchWorkerContext` with Rust `BatchWorkerInputs` structure
- ✅ **All languages**: Document that `BatchWorkerInputs` is Rust-owned, immutable structure
- 📝 **Zen Alignment**: "Explicit is better than implicit" - FFI structures need precise documentation

---

## Orchestration Flow Analysis

### Step 1: Batchable Handler Execution

**Language**: Any (Ruby/Python/TypeScript/Rust)  
**Location**: Worker process

```
Handler analyzes dataset
   ↓
Calculates worker_count and cursor ranges
   ↓
Constructs BatchProcessingOutcome
   ↓
Embeds in StepExecutionResult.result:
{
  "result": {
    "batch_processing_outcome": {
      "type": "create_batches",
      "worker_template_name": "process_batch",
      "worker_count": 5,
      "cursor_configs": [...],
      "total_items": 1000
    },
    "analysis_metadata": {...}
  }
}
   ↓
Returns to orchestration via PGMQ
```

**Key Point**: Handler doesn't create workers - just returns **instructions** for orchestration.

---

### Step 2: Orchestration Processing (Rust Only)

**Language**: Rust  
**Location**: `tasker-orchestration` crate

```rust
// ResultProcessingService receives StepExecutionResult
ResultProcessingService::handle_step_result()
   ↓
// Detects batch_processing_outcome in result
extract_batch_processing_outcome()
   ↓
// Matches on outcome type
match outcome {
    BatchProcessingOutcome::CreateBatches { ... } => {
        // Delegates to BatchProcessingService
        BatchProcessingService::process_create_batches(
            tx,
            task_uuid,
            batchable_step,
            worker_template_name,
            cursor_configs,
            total_items
        ).await?
    }
    BatchProcessingOutcome::NoBatches => {
        // Creates single placeholder worker
        BatchProcessingService::create_no_op_worker(
            tx,
            task_uuid,
            worker_template
        ).await?
    }
}
```

**Transaction Boundary**:
```rust
let mut tx = pool.begin().await?;

// All operations in single transaction
for (i, cursor_config) in cursor_configs.iter().enumerate() {
    // 1. Create worker instance from template
    let worker_step = WorkflowStepCreator::create_from_template(
        &mut tx,
        task_uuid,
        &worker_template,
        &format!("{}_{:03}", worker_template_name, i + 1),
        Some(BatchWorkerInputs {
            cursor: cursor_config.clone(),
            batch_metadata: Some(batch_metadata.clone()),
            is_no_op: false
        })
    ).await?;
    
    // 2. Create edge: batchable → worker
    WorkflowStepEdge::create_with_transaction(
        &mut tx,
        batchable_step.workflow_step_uuid,
        worker_step.workflow_step_uuid,
        "batch_dependency"
    ).await?;
    
    // 3. Create edge: worker → convergence (if exists)
    if let Some(convergence) = &convergence_step {
        WorkflowStepEdge::create_with_transaction(
            &mut tx,
            worker_step.workflow_step_uuid,
            convergence.workflow_step_uuid,
            "batch_convergence_dependency"
        ).await?;
    }
}

tx.commit().await?; // All-or-nothing atomicity
```

**Key Guarantees**:
- ✅ **Atomic**: All N workers created or none
- ✅ **Consistent DAG**: All edges created in same transaction
- ✅ **Isolated**: Other transactions don't see partial state
- ✅ **Durable**: Committed to PostgreSQL

---

### Step 3: Worker Execution (Any Language)

**Access Pattern**:
```
Worker process claims step from queue
   ↓
Loads workflow_step from database
   ↓
Extracts workflow_step.inputs (contains BatchWorkerInputs)
   ↓
Handler calls get_batch_context()
   ↓
Processes data in cursor range
   ↓
Returns result with processing metrics
```

**Checkpoint Pattern** (Ruby example):
```ruby
def call(context)
  batch_ctx = get_batch_context(context)
  
  # Check for existing checkpoint
  checkpoint = context.workflow_step.results&.dig('checkpoint_cursor')
  start_cursor = checkpoint || batch_ctx.start_cursor
  
  results = []
  (start_cursor...batch_ctx.end_cursor).each do |cursor|
    # Process item
    results << process_item(cursor)
    
    # Checkpoint every N items
    if results.size % checkpoint_interval == 0
      update_checkpoint(cursor)  # Updates workflow_step.results
    end
  end
  
  batch_worker_success(
    items_processed: results.size,
    items_succeeded: results.count { |r| r[:success] },
    last_cursor: batch_ctx.end_cursor
  )
end
```

**Checkpoint Storage**:
- Stored in `workflow_steps.results` JSONB column
- Preserved during `ResetForRetry` action
- Worker resumes from checkpoint on retry

---

### Step 4: Convergence/Aggregation (Any Language)

**Deferred Convergence Pattern**:
```
Convergence step waits until:
  - ALL workers in declared_dependencies
  - AND actually created during runtime
  - have completed (intersection semantics)
   ↓
Convergence step becomes ready
   ↓
Worker claims and executes aggregator handler
```

**Aggregation Pattern** (Ruby):
```ruby
def call(context)
  # Detect scenario
  scenario = detect_aggregation_scenario(
    context.dependency_results,
    'analyze_csv',
    'process_csv_batch_'
  )
  
  if scenario.no_batches?
    # Use analyzer result directly
    return no_batches_aggregation_result(
      metadata: { total_processed: 0 }
    )
  end
  
  # Aggregate from all workers
  aggregate_batch_worker_results(scenario) do |batch_results|
    total_processed = 0
    total_value = 0.0
    
    batch_results.each_value do |result|
      total_processed += result['items_processed'] || 0
      total_value += result['total_value'] || 0.0
    end
    
    {
      'total_processed' => total_processed,
      'total_value' => total_value
    }
  end
end
```

**Aggregation Pattern** (Python):
```python
def call(self, context: StepContext) -> StepHandlerResult:
    # Get all worker results
    worker_results = [
        context.get_dependency_result(f"process_csv_batch_{i:03d}")
        for i in range(1, worker_count + 1)
    ]
    
    # Use static aggregation helper
    summary = Batchable.aggregate_worker_results(worker_results)
    
    return self.success(summary)
```

**Key Difference**:
- **Ruby**: Flexible block-based aggregation (supports sum, max, concat, merge, custom)
- **Python/TypeScript**: Static sum aggregation (most common case)

**Recommendation** (from Phase 5):
- ✅ **Python/TypeScript**: Add optional `aggregation_fn` parameter
- 📝 **Zen Alignment**: "Practicality beats purity" - default sum, allow custom

---

## Scenario Detection

### BatchAggregationScenario (Ruby Only)

**Location**: `workers/ruby/lib/tasker_core/batch_processing/batch_aggregation_scenario.rb`

```ruby
class BatchAggregationScenario
  attr_reader :scenario_type, :analyzer_result, :batch_results, :worker_count
  
  def self.detect(dependency_results, analyzer_step_name, batch_worker_prefix)
    analyzer_result = dependency_results.get_results(analyzer_step_name)
    
    # Find all batch worker results
    batch_results = dependency_results.steps.select do |step_name, _|
      step_name.start_with?(batch_worker_prefix)
    end
    
    if batch_results.empty?
      # NoBatches scenario
      NoBatchesScenario.new(analyzer_result)
    else
      # WithBatches scenario
      WithBatchesScenario.new(analyzer_result, batch_results)
    end
  end
  
  def no_batches?
    scenario_type == :no_batches
  end
  
  def with_batches?
    scenario_type == :with_batches
  end
end
```

**Usage**:
```ruby
scenario = BatchAggregationScenario.detect(
  context.dependency_results,
  'analyze_csv',
  'process_csv_batch_'
)

if scenario.no_batches?
  # Handle no-batches case
  analyzer_data = scenario.analyzer_result
  return success(result: { total: 0, data: analyzer_data })
else
  # Aggregate from workers
  aggregate_batch_worker_results(scenario) { |batch_results| ... }
end
```

**Analysis**:
- ✅ **Ruby Only**: Sophisticated scenario detection helper
- ❌ **Python/TypeScript Missing**: No equivalent helper class
- ⚠️ **Manual Detection**: Python/TypeScript handlers detect manually

**Recommendation**:
- ✅ **Python**: Add `BatchAggregationScenario` class
- ✅ **TypeScript**: Add `BatchAggregationScenario` class
- ✅ **Standardize**: Detection logic should be consistent
- 📝 **Zen Alignment**: "There should be one obvious way" - standardized detection

---

## Checkpoint Mechanisms

### Checkpoint Pattern

**Purpose**: Enable worker resumability on failure/retry.

**Storage**: `workflow_steps.results` JSONB column

**Preserved During**: `ResetForRetry` action (verified in TAS-59)

### Implementation Patterns

**Ruby**:
```ruby
def call(context)
  batch_ctx = get_batch_context(context)
  
  # Check for checkpoint
  checkpoint = context.workflow_step.results&.dig('checkpoint_cursor')
  start = checkpoint || batch_ctx.start_cursor
  
  (start...batch_ctx.end_cursor).each_slice(checkpoint_interval) do |batch|
    # Process batch
    results += process_batch(batch)
    
    # Save checkpoint
    update_workflow_step_results({
      'checkpoint_cursor' => batch.last,
      'processed_count' => results.size
    })
  end
end
```

**Python**:
```python
def call(self, context: StepContext) -> StepHandlerResult:
    batch_ctx = self.get_batch_context(context)
    
    # Check for checkpoint
    checkpoint = None
    if context.step_results:
        checkpoint = context.step_results.get('checkpoint_cursor')
    
    start = checkpoint if checkpoint is not None else batch_ctx.start_cursor
    
    for cursor in range(start, batch_ctx.end_cursor, checkpoint_interval):
        # Process items
        batch_results = self.process_batch(cursor, cursor + checkpoint_interval)
        
        # Save checkpoint (would need orchestration API)
        # NOTE: Python doesn't have update_results() helper!
        # Would need to implement or use custom metadata
```

**TypeScript**:
```typescript
async call(context: StepContext): Promise<StepHandlerResult> {
  const batchCtx = this.getBatchContext(context);
  
  // Check for checkpoint
  const checkpoint = context.stepResults?.checkpoint_cursor;
  const start = checkpoint ?? batchCtx.startCursor;
  
  for (let cursor = start; cursor < batchCtx.endCursor; cursor += checkpointInterval) {
    // Process items
    const batchResults = await this.processBatch(cursor, cursor + checkpointInterval);
    
    // Save checkpoint (would need orchestration API)
    // NOTE: TypeScript doesn't have update_results() helper!
  }
}
```

**Cross-Language Comparison**:

| Feature | Rust | Ruby | Python | TypeScript |
|---------|------|------|--------|------------|
| **Checkpoint Read** | ✅ Via step_data | ✅ Via `workflow_step.results` | ⚠️ Via `step_results` (limited) | ⚠️ Via `stepResults` (limited) |
| **Checkpoint Write** | ✅ DB update | ✅ `update_workflow_step_results()` | ❌ No helper | ❌ No helper |
| **Checkpoint Preservation** | ✅ `ResetForRetry` | ✅ Preserved | ✅ Preserved | ✅ Preserved |
| **Resumability** | ✅ Full support | ✅ Full support | ⚠️ Read-only | ⚠️ Read-only |

**Recommendation**:
- ✅ **Python/TypeScript**: Add `update_checkpoint()` or `update_step_results()` helper
- ✅ **Document**: Checkpoint preservation during retry is critical feature
- 📝 **Zen Alignment**: "Practicality beats purity" - checkpoints enable resilience

---

## Functional Gaps Summary

### Rust (Orchestration Complete)
1. ✅ **Full orchestration** - atomic worker creation
2. ✅ **Flexible cursors** - `Value` type supports all JSON types
3. ✅ **Transaction safety** - all-or-nothing guarantees
4. ✅ **Checkpoint preservation** - `ResetForRetry` verified

### Ruby (Most Complete Worker Support)
1. ✅ **BatchAggregationScenario** - sophisticated detection
2. ✅ **Checkpoint updates** - `update_workflow_step_results()`
3. ✅ **Type mirrors** - Dry-Struct matches Rust structures
4. ✅ **Flexible cursors** - via block customization

### Python (Good Core, Missing Advanced Features)
1. ⚠️ **No BatchProcessingOutcome type** - inline dict
2. ⚠️ **No BatchAggregationScenario** - manual detection
3. ❌ **No checkpoint write helper** - read-only
4. ⚠️ **Int-only cursors** - should support flexible types
5. ⚠️ **BatchWorkerContext mismatch** - doesn't align with Rust `BatchWorkerInputs`

### TypeScript (Good Core, Missing Advanced Features)
1. ⚠️ **No BatchProcessingOutcome interface** - inline object
2. ⚠️ **No BatchAggregationScenario** - manual detection
3. ❌ **No checkpoint write helper** - read-only
4. ⚠️ **Number-only cursors** - should support flexible types
5. ✅ **Best FFI alignment** - `RustBatchWorkerInputs` interface matches exactly

---

## Recommendations Summary

### Critical Changes (Implement Now)

#### 1. Type Safety at FFI Boundaries

**Python**:
- ✅ Create `BatchProcessingOutcome` Pydantic model
- ✅ Align `BatchWorkerContext` with Rust `BatchWorkerInputs` structure
- ✅ Change `CursorConfig` cursor types from `int` to `Any`
- ✅ Add `batch_id` field to `CursorConfig`

**TypeScript**:
- ✅ Create `BatchProcessingOutcome` interface
- ✅ Change `CursorConfig` cursor types from `number` to `unknown`
- ✅ Add `batch_id` field to `CursorConfig`
- ✅ Document `RustBatchWorkerInputs` as FFI boundary type

**Ruby**:
- ✅ Already excellent - keep current pattern
- ✅ Document Dry-Struct types as FFI mirrors

#### 2. Scenario Detection Standardization

**Python**:
- ✅ Create `BatchAggregationScenario` class
- ✅ Add `detect()` class method
- ✅ Support both `no_batches` and `with_batches` scenarios

**TypeScript**:
- ✅ Create `BatchAggregationScenario` class
- ✅ Add static `detect()` method
- ✅ Support both scenarios

**Ruby**:
- ✅ Already implemented - use as reference

#### 3. Checkpoint Write Support

**Python**:
- ✅ Add `update_checkpoint(cursor, metadata)` helper
- ✅ Or add `update_step_results(data)` general helper
- ✅ Document checkpoint preservation during retry

**TypeScript**:
- ✅ Add `updateCheckpoint(cursor, metadata)` helper
- ✅ Or add `updateStepResults(data)` general helper
- ✅ Document checkpoint preservation during retry

**Ruby**:
- ✅ Already has `update_workflow_step_results()` - document better

#### 4. Flexible Cursor Types (from Phase 5)

**All Languages**:
- ✅ Support int, string, timestamp, UUID, composite object cursors
- ✅ Document cursor flexibility with examples
- ✅ Add cursor type examples to batch processing docs

### Documentation Requirements

1. **FFI Boundary Reference**:
   - Document `BatchProcessingOutcome` as Rust-owned structure
   - Document `BatchWorkerInputs` as Rust-created, immutable
   - Show exact JSON serialization format
   - Explain that workers read, orchestration writes

2. **Checkpoint Mechanism Guide**:
   - Explain `workflow_steps.results` storage
   - Document `ResetForRetry` preservation
   - Show checkpoint read/write patterns per language
   - Add resumability examples

3. **Aggregation Pattern Guide**:
   - Document scenario detection patterns
   - Show NoBatches vs WithBatches handling
   - Provide aggregation examples (sum, max, concat, merge, custom)
   - Explain convergence step timing

4. **Flexible Cursor Examples**:
   - Integer cursors (most common)
   - UUID cursors (UUID-partitioned data)
   - Timestamp cursors (time-series data)
   - Composite cursors (multi-key partitions)
   - String cursors (alphabetical ranges)

---

## Implementation Checklist

### Python Enhancements
- [ ] Create `BatchProcessingOutcome` Pydantic model:
  - [ ] `NoBatches` and `CreateBatches` variants
  - [ ] Validation matching Rust constraints
  - [ ] Factory methods: `no_batches()`, `create_batches()`
- [ ] Create `BatchAggregationScenario` class:
  - [ ] `detect()` class method
  - [ ] `no_batches()` and `with_batches()` predicates
  - [ ] `analyzer_result` and `batch_results` accessors
- [ ] Align `BatchWorkerContext` with Rust `BatchWorkerInputs`
- [ ] Add `update_checkpoint()` or `update_step_results()` helper
- [ ] Change `CursorConfig` cursor types to `Any`
- [ ] Add `batch_id` field to `CursorConfig`

### TypeScript Enhancements
- [ ] Create `BatchProcessingOutcome` interface:
  - [ ] Union type for NoBatches | CreateBatches
  - [ ] Type guards: `isCreateBatches()`, `isNoBatches()`
  - [ ] Factory functions (optional)
- [ ] Create `BatchAggregationScenario` class:
  - [ ] Static `detect()` method
  - [ ] `noBatches()` and `withBatches()` predicates
  - [ ] `analyzerResult` and `batchResults` accessors
- [ ] Add `updateCheckpoint()` or `updateStepResults()` helper
- [ ] Change `CursorConfig` cursor types to `unknown`
- [ ] Add `batchId` field to `CursorConfig`
- [ ] Document `RustBatchWorkerInputs` as canonical FFI type

### Ruby Enhancements
- [ ] Document existing types as FFI mirrors
- [ ] Add flexible cursor examples to documentation
- [ ] Document `update_workflow_step_results()` for checkpoints
- [ ] Add cursor customization examples (UUID, timestamp, etc.)

### Documentation
- [ ] Create `docs/batch-processing-ffi-boundaries.md`:
  - [ ] Document all FFI structures
  - [ ] Show exact JSON serialization
  - [ ] Explain Rust ownership
- [ ] Update `docs/batch-processing.md`:
  - [ ] Add checkpoint mechanism section
  - [ ] Add flexible cursor examples
  - [ ] Add aggregation scenario detection
  - [ ] Add language-specific patterns
- [ ] Create `docs/batch-processing-checkpoints.md`:
  - [ ] Explain checkpoint storage
  - [ ] Document retry preservation
  - [ ] Show read/write patterns per language
- [ ] Add examples to `docs/worker-crates/*.md`

---

## Next Phase

**Phase 7: Domain Events Integration** will analyze how domain event publishing integrates with handlers (TAS-109). Key questions:
- Is event publisher registration consistent?
- Do all languages support post-execution publishing (TAS-65)?
- Are event routing patterns (durable vs fast paths) aligned?
- Should event publishing be a mixin/trait or base class feature?

---

## Metadata

**Document Version**: 1.0  
**Analysis Date**: 2025-12-27  
**Reviewers**: TBD  
**Approval Status**: Draft

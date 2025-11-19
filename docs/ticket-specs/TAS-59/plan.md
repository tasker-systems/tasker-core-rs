# TAS-59 Batch Processing Implementation Plan

**Last Updated**: 2025-11-11
**Status**: Ready for Implementation
**Integrates With**: TAS-49 (DLQ), TAS-53 (Conditional Workflows), TAS-46 (Actor Pattern)

---

## Executive Summary

Implement batch processing with cursor-based resumability, leveraging TAS-49's proven DLQ and manual resolution infrastructure. This approach **eliminates the need for a separate recovery system** by treating batch workers as standard steps with enhanced checkpoint-based health monitoring.

### Key Simplifications from Original Spec

✅ **Recovery Strategy**: TAS-49 retry + DLQ (no BatchRecoveryService)
✅ **Cursor Preservation**: Verified - `results` field preserved during `ResetForRetry`
✅ **Configuration**: Template `lifecycle` config (no duplicate batch retry settings)
✅ **Resumability**: Controlled by `lifecycle.max_retries` (standard pattern)
✅ **Manual Resolution**: Uses existing TAS-49 step APIs (no new endpoints)

### What Makes This Work

1. **TAS-49 Cursor Preservation**: `ResetForRetry` preserves `workflow_steps.results` field where cursor data is stored
2. **Standard Retry Mechanism**: Batch workers use existing backoff and retry logic
3. **DLQ Integration**: Batch workers enter DLQ via standard staleness detection
4. **Checkpoint-Based Health**: Additional health check based on cursor timestamps (additive to time-based checks)

---

## Architecture Overview

### High-Level Flow

```
Task Initialization
       ↓
Regular Step(s)
       ↓
Batchable Step ← Analyzes dataset, determines batch configuration
       ↓
   [Batch Split via BatchProcessingActor]
       ↓
   ┌───┴───┬───┬───┐
   ↓       ↓   ↓   ↓
Batch-1 Batch-2 ... Batch-N  ← Worker instances with cursor configs
   ↓       ↓   ↓   ↓
   └───┬───┴───┴───┘
       ↓
Convergence Step ← Deferred dependencies (intersection semantics)
       ↓
Task Complete
```

### Recovery Flow (TAS-49 Integration)

```
Batch Worker Fails
    ↓
Standard Retry Logic (lifecycle.max_retries)
    ↓
Cursor Preserved in results.batch_cursor
    ↓
Backoff Elapses → Retry Eligible
    ↓
Worker Re-executes → Loads Cursor → Resumes from Checkpoint
    ↓
If Retries Exhausted:
    ↓
Task Enters DLQ (TAS-49 Staleness Detection)
    ↓
Operator Investigation (TAS-49 Workflow)
    ↓
Operator Chooses Action:
    ├─ ResetForRetry (cursor preserved, resume from checkpoint)
    ├─ ResolveManually (skip batch, workflow continues)
    └─ CompleteManually (provide manual results)
```

---

## Phase 1: Data Structures & Types

### 1.1 BatchProcessingOutcome Enum

**File**: `tasker-shared/src/messaging/execution_types.rs`
**Pattern**: Parallel to `DecisionPointOutcome` (lines 510-552)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchProcessingOutcome {
    /// No batches needed - process as single step
    NoBatches,

    /// Create batch worker steps from template
    CreateBatches {
        /// Template step name to use for creating workers
        worker_template_name: String,
        /// Number of worker instances to create
        worker_count: u32,
        /// Initial cursor positions for each batch
        cursor_configs: Vec<CursorConfig>,
        /// Total items to process
        total_items: u64,
    },
}

impl BatchProcessingOutcome {
    pub fn no_batches() -> Self {
        Self::NoBatches
    }

    pub fn create_batches(
        worker_template_name: String,
        worker_count: u32,
        cursor_configs: Vec<CursorConfig>,
        total_items: u64,
    ) -> Self {
        Self::CreateBatches {
            worker_template_name,
            worker_count,
            cursor_configs,
            total_items,
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self)
            .expect("BatchProcessingOutcome serialization should not fail")
    }

    pub fn from_step_result(result: &serde_json::Value) -> Option<Self> {
        result
            .as_object()?
            .get("batch_processing_outcome")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}
```

### 1.2 CursorConfig Struct

**File**: `tasker-shared/src/messaging/execution_types.rs`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Batch identifier (e.g., "batch_001")
    pub batch_id: String,
    /// Starting position for this batch
    pub start_cursor: serde_json::Value,
    /// Ending position for this batch
    pub end_cursor: serde_json::Value,
    /// Items in this batch
    pub batch_size: u32,
}
```

**Cursor Storage Format** (in `workflow_steps.results`):

```json
{
  "batch_cursor": {
    "batch_id": "batch_001",
    "current_position": 5000,
    "end_position": 10000,
    "items_processed": 4500,
    "last_checkpoint": "2025-11-11T01:00:00Z",
    "checkpoint_interval": 100,
    "error_count": 0,
    "state": "processing"
  },
  "processing_metrics": {
    "items_per_second": 45.2,
    "estimated_completion": "2025-11-11T02:00:00Z"
  }
}
```

### 1.3 StepType Enum Extension

**File**: `tasker-shared/src/models/core/task_template.rs` (lines 209-232)

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    #[default]
    Standard,
    Decision,      // TAS-53
    Deferred,      // TAS-53
    Batchable,     // TAS-59: Triggers batch splitting
    BatchWorker,   // TAS-59: Template for worker instances
}
```

**Update `initial_step_set()` Logic**:
- Exclude descendants of `Decision` steps (existing)
- Exclude `BatchWorker` template steps (new)
- Include `Batchable` steps in initial set

### 1.4 BatchConfiguration Struct

**File**: `tasker-shared/src/models/core/task_template.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchConfiguration {
    /// Items per batch
    pub batch_size: u32,
    /// Maximum parallel batch workers
    pub parallelism: u32,
    /// Field to track progress (e.g., "record_id")
    pub cursor_field: String,
    /// Checkpoint every N items
    pub checkpoint_interval: u32,
    /// Template step name for workers
    pub worker_template: String,
    /// How to handle batch failures
    pub failure_strategy: FailureStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureStrategy {
    /// Continue processing other batches on failure
    ContinueOnFailure,
    /// First batch failure immediately fails entire task
    FailFast,
    /// Failed batches require manual investigation
    Isolate,
}
```

**Extend StepDefinition**:

```rust
pub struct StepDefinition {
    // ... existing fields ...

    #[serde(default)]
    pub batch_config: Option<BatchConfiguration>,
}
```

### 1.5 Configuration Struct

**File**: `tasker-shared/src/config/tasker/batch_processing.rs` (new file)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProcessingConfig {
    /// Enable batch processing feature
    pub enabled: bool,
    /// Maximum parallel batch workers across all tasks
    pub max_parallel_batches: u32,
    /// Default batch size if not specified in template
    pub default_batch_size: u32,
    /// Default checkpoint interval
    pub checkpoint_interval_default: u32,
    /// Minutes without checkpoint = stalled (checkpoint-based health check)
    pub checkpoint_stall_minutes: u32,
}

impl Default for BatchProcessingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_parallel_batches: 50,
            default_batch_size: 1000,
            checkpoint_interval_default: 100,
            checkpoint_stall_minutes: 15,
        }
    }
}
```

**TOML Configuration** (`config/tasker/base/orchestration.toml`):

```toml
[orchestration.batch_processing]
enabled = true
max_parallel_batches = 50
default_batch_size = 1000
checkpoint_interval_default = 100
checkpoint_stall_minutes = 15  # Checkpoint-based health check only
```

**Key Point**: No retry or backoff configuration in batch config. Use template `lifecycle` config:

```yaml
lifecycle:
  max_steps_in_process_minutes: 240  # Primary DLQ timeout (TAS-49)
  max_retries: 3                     # Standard retry limit
  backoff_multiplier: 2.0            # Exponential backoff
```

---

## Phase 2: Actor & Service Implementation

### 2.1 BatchProcessingService

**File**: `tasker-orchestration/src/orchestration/lifecycle/batch_processing/service.rs` (new)

```rust
pub struct BatchProcessingService {
    context: Arc<SystemContext>,
    step_creator: Arc<WorkflowStepCreator>,
}

impl BatchProcessingService {
    /// Analyze dataset and determine batch configuration
    pub async fn analyze_batch_requirements(
        &self,
        task_uuid: Uuid,
        batchable_step: &WorkflowStep,
    ) -> TaskerResult<BatchRequirements> {
        // 1. Extract batch_config from step metadata
        // 2. Analyze dataset size (from step handler result)
        // 3. Calculate optimal worker count based on parallelism
        // 4. Generate cursor configs for each worker
    }

    /// Create worker instances from template
    pub async fn create_worker_instances(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        template_name: &str,
        worker_count: u32,
        cursor_configs: Vec<CursorConfig>,
    ) -> TaskerResult<HashMap<String, Uuid>> {
        // 1. Find template step definition
        // 2. Generate unique worker names (e.g., batch_worker_001)
        // 3. Use WorkflowStepCreator.create_steps_batch()
        // 4. Initialize each worker's results with cursor config
        // 5. Create DAG edges: batchable_step -> workers
    }

    /// Validate template exists and is BatchWorker type
    pub async fn validate_template(
        &self,
        template_name: &str,
        task_template: &TaskTemplate,
    ) -> TaskerResult<()> {
        // 1. Find step definition by name
        // 2. Verify type = BatchWorker
        // 3. Validate handler configuration
    }

    /// Initialize cursor configs based on batch configuration
    fn initialize_cursor_configs(
        &self,
        batch_config: &BatchConfiguration,
        total_items: u64,
    ) -> Vec<CursorConfig> {
        // 1. Calculate items per worker
        // 2. Generate cursor ranges
        // 3. Create CursorConfig for each worker
    }
}
```

**Key Pattern**: Mirror `DecisionPointService` architecture
- Service handles business logic
- Actor delegates to service
- Uses existing `WorkflowStepCreator` for step creation
- Validates within transaction for atomicity

### 2.2 BatchProcessingActor

**File**: `tasker-orchestration/src/actors/batch_processing_actor.rs` (new)

```rust
use super::traits::{Handler, Message, OrchestrationActor};

pub struct BatchProcessingActor {
    context: Arc<SystemContext>,
    service: Arc<BatchProcessingService>,
}

#[derive(Debug, Clone)]
pub struct ProcessBatchSplitMessage {
    pub task_uuid: Uuid,
    pub batchable_step_uuid: Uuid,
    pub outcome: BatchProcessingOutcome,
}

impl Message for ProcessBatchSplitMessage {}

pub struct BatchProcessingResult {
    pub task_uuid: Uuid,
    pub workers_created: u32,
    pub worker_names: Vec<String>,
}

#[async_trait]
impl OrchestrationActor for BatchProcessingActor {
    fn name(&self) -> &str {
        "batch_processing_actor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    async fn started(&self) -> TaskerResult<()> {
        tracing::info!("BatchProcessingActor started");
        Ok(())
    }

    async fn stopped(&self) -> TaskerResult<()> {
        tracing::info!("BatchProcessingActor stopped");
        Ok(())
    }
}

#[async_trait]
impl Handler<ProcessBatchSplitMessage> for BatchProcessingActor {
    type Response = TaskerResult<BatchProcessingResult>;

    async fn handle(&self, msg: ProcessBatchSplitMessage) -> Self::Response {
        match msg.outcome {
            BatchProcessingOutcome::NoBatches => {
                // No workers needed
                Ok(BatchProcessingResult {
                    task_uuid: msg.task_uuid,
                    workers_created: 0,
                    worker_names: vec![],
                })
            }
            BatchProcessingOutcome::CreateBatches {
                worker_template_name,
                worker_count,
                cursor_configs,
                total_items,
            } => {
                // Delegate to service
                let worker_mapping = self.service.create_worker_instances(
                    // ... parameters
                ).await?;

                Ok(BatchProcessingResult {
                    task_uuid: msg.task_uuid,
                    workers_created: worker_count,
                    worker_names: worker_mapping.keys().cloned().collect(),
                })
            }
        }
    }
}
```

### 2.3 ActorRegistry Integration

**File**: `tasker-orchestration/src/actors/registry.rs`

```rust
pub struct ActorRegistry {
    // ... existing actors ...
    pub batch_processing_actor: Arc<BatchProcessingActor>,
}

impl ActorRegistry {
    pub fn build(context: Arc<SystemContext>) -> Self {
        // ... existing actor initialization ...

        let batch_processing_service = Arc::new(BatchProcessingService::new(
            context.clone(),
            // ... dependencies
        ));

        let batch_processing_actor = Arc::new(BatchProcessingActor {
            context: context.clone(),
            service: batch_processing_service,
        });

        Self {
            // ... existing actors ...
            batch_processing_actor,
        }
    }
}
```

---

## Phase 3: Checkpoint-Based Health Monitoring

### 3.1 Extend StalenessDetector

**File**: `tasker-orchestration/src/orchestration/staleness_detector.rs`

```rust
impl StalenessDetector {
    /// Check if batch worker is healthy based on checkpoint timestamps
    async fn is_batch_worker_healthy(
        &self,
        step: &WorkflowStep,
    ) -> TaskerResult<bool> {
        // Check if this is a batch worker
        if let Some(batch_cursor) = step.results.get("batch_cursor") {
            // Get last checkpoint timestamp
            let last_checkpoint = batch_cursor
                .get("last_checkpoint")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok());

            if let Some(checkpoint_time) = last_checkpoint {
                let elapsed = Utc::now() - checkpoint_time;
                let stall_threshold = self.get_batch_checkpoint_threshold(step);

                if elapsed > stall_threshold {
                    tracing::warn!(
                        step_uuid = %step.workflow_step_uuid,
                        elapsed_minutes = elapsed.num_minutes(),
                        threshold_minutes = stall_threshold.num_minutes(),
                        "Batch worker stalled - no recent checkpoint"
                    );
                    return Ok(false);
                }
            }

            // Also check overall duration
            let max_duration = self.get_batch_max_duration(step);
            if step.duration_in_state() > max_duration {
                return Ok(false);
            }

            Ok(true)  // Batch is healthy
        } else {
            // Regular step - use standard health check
            self.is_regular_step_healthy(step).await
        }
    }

    fn get_batch_checkpoint_threshold(&self, step: &WorkflowStep) -> chrono::Duration {
        // Get from batch_config.checkpoint_stall_minutes
        // Default to config default
        chrono::Duration::minutes(
            self.config.batch_processing.checkpoint_stall_minutes as i64
        )
    }

    fn get_batch_max_duration(&self, step: &WorkflowStep) -> chrono::Duration {
        // Get from lifecycle.max_steps_in_process_minutes (TAS-49 standard)
        // This is the primary DLQ timeout
        chrono::Duration::minutes(
            step.lifecycle_config.max_steps_in_process_minutes as i64
        )
    }
}
```

**Key Design Points**:
- **Two health checks**: Checkpoint-based + duration-based
- **Additive to TAS-49**: Doesn't replace standard staleness detection
- **Template-specific thresholds**: `lifecycle.max_steps_in_process_minutes` for DLQ, `batch_config.checkpoint_stall_minutes` for checkpoint health

### 3.2 Configuration

**File**: `config/tasker/base/orchestration.toml`

```toml
[orchestration.staleness_detection]
enabled = true
batch_size = 100
detection_interval_seconds = 300  # 5 minutes

[orchestration.staleness_detection.batch_awareness]
enabled = true
checkpoint_based_health = true  # Enable cursor health checks
```

---

## Phase 4: Result Processing Integration

### 4.1 Detect Batch Outcomes

**File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/service.rs`

```rust
impl OrchestrationResultProcessor {
    pub async fn process_step_result(
        &self,
        result: StepExecutionResult,
    ) -> TaskerResult<()> {
        // ... existing result processing ...

        // Check for batch processing outcome (after decision point check)
        if let Some(batch_outcome) = BatchProcessingOutcome::from_step_result(&result.result_data) {
            tracing::info!(
                step_uuid = %result.workflow_step_uuid,
                "Detected batch processing outcome, routing to BatchProcessingActor"
            );

            let msg = ProcessBatchSplitMessage {
                task_uuid: result.task_uuid,
                batchable_step_uuid: result.workflow_step_uuid,
                outcome: batch_outcome,
            };

            self.actors.batch_processing_actor.handle(msg).await?;
        }

        // ... continue with standard result processing ...
    }
}
```

### 4.2 Command Processor Integration

**File**: `tasker-orchestration/src/orchestration/command_processor.rs`

```rust
pub enum OrchestrationCommand {
    // ... existing commands ...
    ProcessBatchSplit {
        task_uuid: Uuid,
        batchable_step_uuid: Uuid,
        outcome: BatchProcessingOutcome,
    },
}

impl OrchestrationCommandProcessor {
    async fn handle_process_batch_split(
        &self,
        task_uuid: Uuid,
        batchable_step_uuid: Uuid,
        outcome: BatchProcessingOutcome,
    ) -> TaskerResult<()> {
        let msg = ProcessBatchSplitMessage {
            task_uuid,
            batchable_step_uuid,
            outcome,
        };

        self.actors.batch_processing_actor.handle(msg).await?;
        Ok(())
    }
}
```

---

## Phase 5: YAML Template Support

### 5.1 Template Parser Extension

**File**: `tasker-shared/src/models/core/task_template.rs`

Serde handles parsing automatically once we add the field to `StepDefinition`.

### 5.2 Example Template

```yaml
name: data_pipeline
namespace_name: batch_processing
version: 1.0.0

steps:
  - name: load_data
    type: standard
    handler:
      callable: DataPipeline::LoadDataHandler

  - name: process_records
    type: batchable  # New step type
    dependencies:
      - load_data
    batch_config:
      batch_size: 1000
      parallelism: 5
      cursor_field: record_id
      checkpoint_interval: 100
      worker_template: batch_worker_template
      failure_strategy: continue_on_failure
    handler:
      callable: DataPipeline::BatchSplitterHandler

  # Template for batch workers (system creates N instances)
  - name: batch_worker_template
    type: batch_worker  # Special type for template
    dependencies:
      - process_records  # Automatically depends on batchable step
    lifecycle:
      max_steps_in_process_minutes: 120  # Batch-specific timeout
      max_retries: 3
      backoff_multiplier: 2.0
    handler:
      callable: DataPipeline::BatchWorkerHandler

  - name: aggregate_results
    type: deferred  # Convergence point (existing type)
    dependencies:
      - batch_worker_template  # System resolves to all created instances
    handler:
      callable: DataPipeline::AggregateResultsHandler
```

**How Dynamic Worker Creation Works**:

1. **Analysis**: `process_records` handler analyzes dataset and determines need for 5 workers
2. **Worker Creation**: System creates 5 instances from `batch_worker_template`:
   - `batch_worker_001` with cursor for items 1-1000
   - `batch_worker_002` with cursor for items 1001-2000
   - ... (5 total)
3. **Dependency Resolution**: `aggregate_results` dependency on `batch_worker_template` automatically resolves to all 5 instances using intersection semantics (TAS-53)
4. **Execution**: Workers process in parallel, checkpointing progress
5. **Convergence**: `aggregate_results` waits for all workers to complete

---

## Phase 6: Ruby Handler Base Class

### 6.1 Create BatchWorker Base Class

**File**: `workers/ruby/lib/tasker_core/step_handler/batch_worker.rb` (new)

```ruby
module TaskerCore
  module StepHandler
    # Base class for batch worker handlers providing cursor management
    # and checkpoint-based resumability.
    #
    # Example:
    #   class MyBatchWorker < TaskerCore::StepHandler::BatchWorker
    #     def process_item(item, index)
    #       # Process single item
    #       MyService.process(item)
    #     end
    #   end
    class BatchWorker < Base
      def call(task, sequence, step)
        cursor = load_cursor(step)

        begin
          process_batch_with_checkpointing(cursor, step) do |item, index|
            # Call subclass implementation
            process_item(item, index)
          end

          success(
            result_data: {
              batch_id: cursor['batch_id'],
              items_processed: cursor['items_processed'],
              completion_status: 'complete'
            }
          )
        rescue => e
          # Save cursor position for recovery
          persist_cursor_state(step, cursor)

          # Return error with retry capability
          error(
            result_data: {
              batch_id: cursor['batch_id'],
              last_position: cursor['current_position'],
              error: e.message
            },
            retryable: recoverable_error?(e),
            backoff_seconds: calculate_backoff(step.attempts)
          )
        end
      end

      # Subclass must implement this method
      def process_item(item, index)
        raise NotImplementedError, "Subclass must implement process_item"
      end

      private

      def load_cursor(step)
        # Check for existing cursor (indicates retry)
        step.results['batch_cursor'] || initialize_cursor(step)
      end

      def initialize_cursor(step)
        # First execution - initialize from metadata
        {
          'batch_id' => step.metadata['batch_id'],
          'current_position' => step.metadata['start_position'],
          'end_position' => step.metadata['end_position'],
          'items_processed' => 0,
          'checkpoint_interval' => step.batch_config['checkpoint_interval'],
          'last_checkpoint' => Time.now.utc.iso8601,
          'state' => 'processing'
        }
      end

      def process_batch_with_checkpointing(cursor, step)
        start_pos = cursor['current_position']
        end_pos = cursor['end_position']

        (start_pos..end_pos).each do |position|
          item = fetch_item_at(position)
          yield item, position

          cursor['current_position'] = position + 1
          cursor['items_processed'] += 1

          # Checkpoint every N items
          if cursor['items_processed'] % cursor['checkpoint_interval'] == 0
            update_cursor_checkpoint(step, cursor)
          end
        end
      end

      def update_cursor_checkpoint(step, cursor)
        cursor['last_checkpoint'] = Time.now.utc.iso8601

        # Persist to workflow_steps.results
        UpdateStepCursor.call(
          workflow_step_uuid: step.workflow_step_uuid,
          cursor: cursor
        )
      end

      def persist_cursor_state(step, cursor)
        update_cursor_checkpoint(step, cursor)
      end

      def recoverable_error?(error)
        # Override in subclass for custom logic
        # Default: network errors, timeouts are recoverable
        error.is_a?(Timeout::Error) ||
          error.is_a?(Net::OpenTimeout) ||
          error.message.include?('connection')
      end

      def calculate_backoff(attempts)
        # Exponential backoff: 2^attempts seconds
        [2 ** attempts, 300].min  # Cap at 5 minutes
      end
    end
  end
end
```

### 6.2 Example Batch Worker Implementation

```ruby
module DataPipeline
  class BatchWorkerHandler < TaskerCore::StepHandler::BatchWorker
    def process_item(item, index)
      # Business logic for processing single item
      record = fetch_record(item)
      validate_record(record)
      transform_record(record)
      save_record(record)
    end

    private

    def fetch_record(id)
      Record.find(id)
    end

    def validate_record(record)
      raise ValidationError unless record.valid?
    end

    def transform_record(record)
      record.status = 'processed'
      record.processed_at = Time.now
    end

    def save_record(record)
      record.save!
    end
  end
end
```

---

## Phase 7: Testing & Validation

### 7.1 Unit Tests

**File**: `tasker-shared/tests/batch_processing_types_test.rs`

```rust
#[test]
fn test_batch_processing_outcome_serialization() {
    // Test NoBatches variant
    // Test CreateBatches variant
    // Test to_value() and from_step_result()
}

#[test]
fn test_cursor_config_creation() {
    // Test CursorConfig initialization
    // Test cursor range calculation
}
```

**File**: `tasker-orchestration/tests/unit/batch_processing_service_test.rs`

```rust
#[sqlx::test]
async fn test_worker_instance_creation(pool: PgPool) {
    // Test creating N workers from template
    // Verify unique names generated
    // Verify cursor configs assigned
}

#[sqlx::test]
async fn test_batch_requirements_analysis(pool: PgPool) {
    // Test analyzing dataset size
    // Test calculating worker count
    // Test respecting parallelism limits
}
```

### 7.2 Integration Tests

**File**: `tests/e2e/batch_processing_integration_test.rs`

```rust
#[sqlx::test]
async fn test_small_batch_single_worker(pool: PgPool) {
    // Dataset: 500 items, batch_size: 1000
    // Expected: 1 worker created
    // Verify: Single worker processes all items
}

#[sqlx::test]
async fn test_medium_batch_parallel_workers(pool: PgPool) {
    // Dataset: 3000 items, batch_size: 1000, parallelism: 5
    // Expected: 3 workers created
    // Verify: All workers complete, convergence step receives results
}

#[sqlx::test]
async fn test_large_batch_max_parallelism(pool: PgPool) {
    // Dataset: 10000 items, batch_size: 1000, parallelism: 5
    // Expected: 5 workers created (respecting parallelism limit)
    // Verify: Parallel execution, checkpoint progress
}

#[sqlx::test]
async fn test_worker_failure_with_checkpoint_resume(pool: PgPool) {
    // Worker processes 500/1000 items, then fails
    // Cursor saved at position 500
    // Retry executes
    // Verify: Resume from position 500, not restart from 0
}

#[sqlx::test]
async fn test_convergence_step_receives_all_results(pool: PgPool) {
    // Multiple workers complete with results
    // Convergence step uses deferred type
    // Verify: Convergence receives results from all workers
}
```

### 7.3 TAS-49 Integration Tests

**File**: `tests/integration/batch_dlq_integration_test.rs`

```rust
#[sqlx::test]
async fn test_batch_worker_enters_dlq_after_retries(pool: PgPool) {
    // Batch worker exhausts max_retries
    // Staleness detection runs
    // Verify: Task enters DLQ with appropriate reason
}

#[sqlx::test]
async fn test_reset_for_retry_preserves_cursor(pool: PgPool) {
    // Batch worker fails with cursor at position 500
    // Operator calls ResetForRetry
    // Verify: attempts = 0, cursor still at position 500
    // Worker re-executes and resumes from 500
}

#[sqlx::test]
async fn test_resolve_manually_skips_batch(pool: PgPool) {
    // Batch worker in error state
    // Operator calls ResolveManually
    // Verify: Step transitions to ResolvedManually
    // Convergence step proceeds with partial results
}

#[sqlx::test]
async fn test_complete_manually_with_results(pool: PgPool) {
    // Batch worker stuck
    // Operator manually processes items, calls CompleteManually
    // Verify: Step complete with manual results
    // Convergence step receives manual results
}
```

### 7.4 Cursor Preservation Test (Add to TAS-49 Suite)

**File**: `tests/integration/step_manual_resolution_test.rs`

```rust
#[sqlx::test]
async fn test_reset_for_retry_preserves_batch_cursor(pool: PgPool) -> Result<()> {
    // 1. Create batch worker step with cursor in results
    let step = create_batch_worker_with_cursor(
        &pool,
        batch_id: "test_001",
        current_position: 5000,
        end_position: 10000,
    ).await?;

    // 2. Transition to Error state
    step_state_machine.transition(StepEvent::Fail).await?;
    assert_eq!(step.state, WorkflowStepState::Error);
    assert_eq!(step.attempts, 3);

    // 3. Call ResetForRetry
    step_state_machine.transition(StepEvent::ResetForRetry).await?;

    // 4. Verify cursor data unchanged
    let updated_step = WorkflowStep::find_by_uuid(&pool, step.uuid).await?;
    assert_eq!(updated_step.state, WorkflowStepState::Pending);
    assert_eq!(updated_step.attempts, 0);  // ✅ Reset

    let cursor = updated_step.results["batch_cursor"];
    assert_eq!(cursor["current_position"], 5000);  // ✅ Preserved
    assert_eq!(cursor["end_position"], 10000);     // ✅ Preserved
    assert_eq!(cursor["batch_id"], "test_001");    // ✅ Preserved

    Ok(())
}
```

---

## Phase 8: Documentation

### 8.1 Update Existing Documentation

**File**: `docs/conditional-workflows.md`
- Add section: "Batch Processing with Dynamic Workers"
- Document template instantiation pattern
- Show batch processing examples

**File**: `docs/dlq-system.md`
- Add section: "Batch Worker Resolution"
- Document checkpoint preservation during ResetForRetry
- Show CLI examples for batch worker recovery

**File**: `docs/idempotency-and-atomicity.md`
- Add section: "Batch Processing Atomicity"
- Document cursor persistence guarantees
- Explain retry-safe batch resumption

### 8.2 Create Batch Processing Guide

**File**: `docs/batch-processing.md`

```markdown
# Batch Processing Guide

## When to Use Batch Processing

✅ **Use batch processing when:**
- Processing large datasets (>1000 items)
- Work can be parallelized
- Resumability after failures is important
- Progress tracking is needed

❌ **Don't use batch processing when:**
- Dataset is small (<100 items)
- Work must be strictly sequential
- Overhead of checkpointing not justified

## YAML Configuration

(Include full template example with annotations)

## Handler Implementation

(Include Ruby and Rust examples)

## Checkpoint Best Practices

1. Choose appropriate checkpoint intervals
2. Make processing idempotent
3. Handle partial failures gracefully

## Recovery Workflows

(Document TAS-49 integration with examples)

## Example: CSV Processing Pipeline

(Complete working example)
```

---

## What's NOT Included (Simplified from Original Spec)

❌ **BatchRecoveryService** - Using TAS-49 retry + DLQ instead
❌ **`detect_stalled_batch_workers()` SQL function** - Using existing staleness detector with checkpoint checks
❌ **Custom retry mechanism** - Using standard `lifecycle.max_retries`
❌ **`resumable` configuration flag** - Controlled by retry config
❌ **Separate `[orchestration.batch_recovery]` config** - Using `lifecycle` config
❌ **New manual resolution APIs** - Using existing TAS-49 step endpoints
❌ **Custom state transitions** - Using standard `StepEvent::Retry`

---

## Implementation Timeline

**Phase 1**: Data structures + Types (~2 days)
- BatchProcessingOutcome enum
- CursorConfig struct
- StepType extension
- BatchConfiguration struct
- Config integration

**Phase 2**: Actor + Service (~2 days)
- BatchProcessingService implementation
- BatchProcessingActor implementation
- ActorRegistry integration

**Phase 3**: Health Monitoring (~1 day)
- Extend StalenessDetector
- Checkpoint-based health checks
- Configuration integration

**Phase 4**: Result Processing (~1 day)
- Detect batch outcomes
- Command processor integration

**Phase 5**: YAML Support (~1 day)
- Template parsing
- Example templates

**Phase 6**: Ruby Handler (~1 day)
- BatchWorker base class
- Example implementation

**Phase 7**: Testing (~2 days)
- Unit tests
- Integration tests
- TAS-49 integration tests
- Cursor preservation test

**Phase 8**: Documentation (~1 day)
- Update existing docs
- Create batch processing guide
- Examples and tutorials

**Total: ~11 days** (vs 4 weeks in original spec)

---

## Success Criteria

✅ Batch processing with dynamic worker instantiation
✅ Cursor-based resumability after failures
✅ Checkpoint progress tracking
✅ Zero duplicate work after crashes
✅ TAS-49 DLQ integration for manual resolution
✅ Template-based configuration
✅ Full E2E test coverage
✅ No new APIs needed (leverages TAS-49)
✅ Unified configuration (no duplicate settings)
✅ Automatic recovery via standard retry mechanism

---

## Related Documentation

- **[TAS-49 DLQ System](../../../docs/dlq-system.md)** - Manual resolution integration
- **[TAS-53 Conditional Workflows](../../../docs/conditional-workflows.md)** - Decision point pattern (template instantiation)
- **[TAS-46 Actor Pattern](../../../docs/actors.md)** - Actor-based architecture
- **[Idempotency Guarantees](../../../docs/idempotency-and-atomicity.md)** - Cursor preservation and retry safety
- **[Original TAS-59 Spec](./original-spec.md)** - Pre-TAS-49 design

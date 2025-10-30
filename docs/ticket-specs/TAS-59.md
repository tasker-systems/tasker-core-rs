## TAS-59 Batch Processing Implementation Plan

Based on our review of the conditional workflows pattern (TAS-53) and the existing architecture, here's a comprehensive implementation plan for batch processing with resumability:

### High-Level Design

The batch processing system will follow the same pattern as conditional workflows, using:
1. **Special step type**: `batchable` - triggers batch splitting
2. **Dynamic step creation**: Similar to `DecisionPointOutcome`, we'll have `BatchProcessingOutcome`
3. **Cursor mechanism**: Stored in `workflow_steps.results` JSONB field
4. **Convergence pattern**: Using `deferred` step type for batch convergence
5. **Recovery system**: Detection and re-enqueueing of stalled batch workers

### Architecture Overview

```
Task Initialization
       ↓
Regular Step(s)
       ↓
Batchable Step ← Determines batch configuration
       ↓
   [Batch Split]
       ↓
   ┌───┴───┬───┬───┐
   ↓       ↓   ↓   ↓
Batch-1 Batch-2 ... Batch-N  ← Created dynamically with cursors
   ↓       ↓   ↓   ↓
   └───┬───┴───┴───┘
       ↓
Convergence Step ← Deferred dependencies (intersection semantics)
       ↓
Task Complete
```

### Core Components

#### 1. New Step Type: `batchable`

```yaml
steps:
  - name: process_large_dataset
    type: batchable
    dependencies:
      - data_validation
    batch_config:
      batch_size: 1000      # Items per batch
      parallelism: 10       # Max parallel batches
      cursor_field: id      # Field to track progress
      resumable: true       # Allow automatic recovery of stalled batches
      failure_strategy: isolate  # How to handle batch failures
    handler:
      callable: DataProcessing::BatchProcessorHandler
```

#### 2. BatchProcessingOutcome Enum (New)

```rust
// In tasker-shared/src/messaging/execution_types.rs

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Batch identifier
    batch_id: String,
    /// Starting position for this batch
    start_cursor: serde_json::Value,
    /// Ending position for this batch
    end_cursor: serde_json::Value,
    /// Items in this batch
    batch_size: u32,
}
```

#### 3. BatchProcessingActor (New)

Following the actor pattern from TAS-46:

```rust
// tasker-orchestration/src/actors/batch_processing_actor.rs

pub struct BatchProcessingActor {
    context: Arc<SystemContext>,
    service: Arc<BatchProcessingService>,
}

impl Handler<ProcessBatchSplitMessage> for BatchProcessingActor {
    type Response = BatchProcessingResult;

    async fn handle(&self, msg: ProcessBatchSplitMessage) -> TaskerResult<Self::Response> {
        // 1. Analyze dataset size and determine batch configuration
        // 2. Determine number of workers needed based on parallelism config
        // 3. Create worker instances from template with unique names (e.g., batch_worker_001, batch_worker_002)
        // 4. Store cursor metadata in each worker's workflow_steps.results
        // 5. Return outcome with template name and worker count
    }
}
```

#### 4. Cursor Storage Schema

Cursor information will be stored in the existing `workflow_steps.results` JSONB field:

```json
{
  "batch_cursor": {
    "batch_id": "batch_001",
    "current_position": 5000,
    "end_position": 10000,
    "items_processed": 4500,
    "last_checkpoint": "2025-10-30T01:00:00Z",
    "checkpoint_interval": 100,
    "error_count": 0,
    "state": "processing"  // processing, complete, failed, stalled
  },
  "processing_metrics": {
    "items_per_second": 45.2,
    "estimated_completion": "2025-10-30T02:00:00Z"
  }
}
```

#### 5. Batch Worker Step Implementation

```ruby
# Ruby batch worker implementation
class BatchWorkerHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    # Get cursor config from step results
    cursor = step.results['batch_cursor']

    # Process batch with checkpointing
    process_batch_with_cursor(cursor) do |item, index|
      # Process individual item
      process_item(item)

      # Update cursor every N items
      if index % cursor['checkpoint_interval'] == 0
        update_cursor_checkpoint(step, cursor, index)
      end
    end

    success(
      result_data: {
        batch_id: cursor['batch_id'],
        items_processed: cursor['items_processed'],
        completion_status: 'complete'
      }
    )
  rescue => e
    # Store cursor position for recovery
    update_cursor_position(step, cursor)
    raise
  end

  private

  def update_cursor_checkpoint(step, cursor, position)
    # Update cursor in database
    cursor['current_position'] = position
    cursor['last_checkpoint'] = Time.now.utc

    # Persist to workflow_steps.results
    UpdateStepCursor.call(
      workflow_step_uuid: step.workflow_step_uuid,
      cursor: cursor
    )
  end
end
```

#### 6. Stalled Batch Detection

Add a new SQL function for detecting stalled batches:

```sql
-- Migration: add_batch_stall_detection.sql

CREATE OR REPLACE FUNCTION detect_stalled_batch_workers(
    stall_timeout_seconds INTEGER DEFAULT 300,
    cursor_stall_threshold_seconds INTEGER DEFAULT 60
)
RETURNS TABLE(
    workflow_step_uuid UUID,
    task_uuid UUID,
    batch_id TEXT,
    last_checkpoint TIMESTAMPTZ,
    current_position BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        ws.workflow_step_uuid,
        ws.task_uuid,
        (ws.results->'batch_cursor'->>'batch_id')::TEXT as batch_id,
        (ws.results->'batch_cursor'->>'last_checkpoint')::TIMESTAMPTZ as last_checkpoint,
        (ws.results->'batch_cursor'->>'current_position')::BIGINT as current_position
    FROM tasker_workflow_steps ws
    JOIN tasker_workflow_step_transitions wst ON (
        wst.workflow_step_uuid = ws.workflow_step_uuid
        AND wst.most_recent = true
        AND wst.to_state = 'InProgress'
    )
    WHERE
        -- Has batch cursor
        ws.results ? 'batch_cursor'
        -- Is in progress
        AND NOT ws.processed
        AND ws.in_process
        -- Check for stalled conditions
        AND (
            -- No checkpoint for too long
            (ws.results->'batch_cursor'->>'last_checkpoint')::TIMESTAMPTZ
                < NOW() - INTERVAL '1 second' * cursor_stall_threshold_seconds
            OR
            -- Step in progress for too long without completion
            wst.created_at < NOW() - INTERVAL '1 second' * stall_timeout_seconds
        );
END;
$$ LANGUAGE plpgsql STABLE;
```

#### 7. Recovery System

```rust
// tasker-orchestration/src/orchestration/lifecycle/batch_recovery/service.rs

pub struct BatchRecoveryService {
    context: Arc<SystemContext>,
    step_enqueuer: Arc<StepEnqueuerService>,
}

impl BatchRecoveryService {
    pub async fn detect_and_recover_stalled_batches(&self) -> TaskerResult<RecoveryStats> {
        // 1. Query for stalled batch workers with resumability check
        let stalled_batches = self.get_resumable_stalled_batches().await?;
        
        let mut recovered = 0;
        let mut isolated = 0;
        
        for batch in stalled_batches {
            let batch_config = self.get_batch_config(batch.workflow_step_uuid).await?;
            
            // Check if this batch is configured for automatic recovery
            if !batch_config.resumable {
                tracing::warn!(
                    batch_id = %batch.batch_id,
                    "Stalled batch detected but automatic recovery disabled - requires manual intervention"
                );
                // Mark for manual resolution instead of auto-recovery
                self.mark_for_manual_resolution(batch.workflow_step_uuid).await?;
                isolated += 1;
                continue;
            }
            
            // 2. Transition step back to Pending (with retry increment)
            transition_step_for_recovery(batch.workflow_step_uuid).await?;
            
            // 3. Re-enqueue for processing
            self.step_enqueuer.enqueue_single_step(batch.workflow_step_uuid).await?;
            
            // 4. Log recovery event
            tracing::warn!(
                batch_id = %batch.batch_id,
                last_position = %batch.current_position,
                "Recovered stalled batch worker"
            );
            
            recovered += 1;
        }
        
        Ok(RecoveryStats { 
            recovered_batches: recovered,
            isolated_batches: isolated 
        })
    }
    
    async fn get_resumable_stalled_batches(&self) -> TaskerResult<Vec<StalledBatch>> {
        // Query only batches configured as resumable
        sqlx::query!(
            r#"
            SELECT DISTINCT
                ws.workflow_step_uuid,
                ws.task_uuid,
                (ws.results->'batch_cursor'->>'batch_id')::TEXT as batch_id,
                ns.metadata->'batch_config'->'resumable' as resumable
            FROM tasker_workflow_steps ws
            JOIN tasker_named_steps ns ON ws.named_step_uuid = ns.named_step_uuid
            WHERE 
                ws.results ? 'batch_cursor'
                AND (ns.metadata->'batch_config'->>'resumable')::boolean = true
                -- Additional stall detection logic...
            "#
        )
        .fetch_all(&self.context.db_pool)
        .await
    }
}
```

### YAML Configuration Example

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
      worker_template: batch_worker_template  # Reference to template step
      resumable: true                        # Enable automatic recovery
      failure_strategy: continue_on_failure   # Continue processing other batches
    handler:
      callable: DataPipeline::BatchSplitterHandler

  # Template for batch workers (system will create N instances)
  - name: batch_worker_template
    type: batch_worker  # Special type for batch worker template
    dependencies:
      - process_records  # Automatically depends on the batchable step
    handler:
      callable: DataPipeline::BatchWorkerHandler
    # This template will be instantiated as:
    # - batch_worker_001
    # - batch_worker_002
    # - ... up to batch_worker_N based on parallelism

  - name: aggregate_results
    type: deferred  # Convergence point
    dependencies:
      - batch_worker_template  # System resolves to all created instances
    handler:
      callable: DataPipeline::AggregateResultsHandler
```

#### How Dynamic Worker Creation Works

When the `process_records` batchable step executes:

1. **Analysis Phase**: The handler analyzes the dataset and determines optimal batch configuration
2. **Worker Creation**: Based on `parallelism: 5` and data size, it decides to create 5 workers
3. **Template Instantiation**: The system creates 5 instances from `batch_worker_template`:
   - `batch_worker_001` with cursor for items 1-1000
   - `batch_worker_002` with cursor for items 1001-2000
   - `batch_worker_003` with cursor for items 2001-3000
   - `batch_worker_004` with cursor for items 3001-4000
   - `batch_worker_005` with cursor for items 4001-5000
4. **Dependency Resolution**: The `aggregate_results` step's dependency on `batch_worker_template` 
   automatically resolves to all 5 created instances using intersection semantics
5. **Execution**: Workers process in parallel, checkpointing progress via cursors

The handler returns:
```rust
BatchProcessingOutcome::CreateBatches {
    worker_template_name: "batch_worker_template".to_string(),
    worker_count: 5,
    cursor_configs: vec![
        CursorConfig { batch_id: "001", start_cursor: 1, end_cursor: 1000, batch_size: 1000 },
        CursorConfig { batch_id: "002", start_cursor: 1001, end_cursor: 2000, batch_size: 1000 },
        // ... etc
    ],
    total_items: 5000,
}
```

This approach provides:
- **Flexibility**: Worker count determined at runtime based on actual data
- **Simplicity**: Single template definition instead of pre-enumerating workers
- **Scalability**: Can create 1 or 100 workers from the same template
- **Type Safety**: Template validation ensures worker configuration is correct

### Template Instantiation Mechanism

The system needs to support template-based step creation with the following capabilities:

#### 1. Template Step Definition

A template step is marked with `type: batch_worker` and serves as a blueprint:

```yaml
- name: batch_worker_template
  type: batch_worker
  dependencies:
    - process_records  # Parent batchable step
  handler:
    callable: DataPipeline::BatchWorkerHandler
  metadata:
    is_template: true
    instance_naming_pattern: "{base_name}_{index:03d}"  # e.g., batch_worker_001
```

#### 2. Instance Creation Process

```rust
// In BatchProcessingService
async fn create_worker_instances(
    &self,
    template: &StepDefinition,
    worker_count: u32,
    cursor_configs: Vec<CursorConfig>,
) -> TaskerResult<Vec<String>> {
    let mut instance_names = Vec::new();
    
    for i in 0..worker_count {
        // Generate unique instance name
        let instance_name = format!("{}_{:03}", template.name, i + 1);
        
        // Create step instance with cursor config
        let mut instance_def = template.clone();
        instance_def.name = instance_name.clone();
        instance_def.metadata.insert(
            "batch_cursor".to_string(),
            serde_json::to_value(&cursor_configs[i as usize])?
        );
        instance_def.metadata.insert(
            "template_source".to_string(),
            json!(template.name)
        );
        
        // Create the step in database
        self.step_creator.create_single_step(&instance_def).await?;
        instance_names.push(instance_name);
    }
    
    Ok(instance_names)
}
```

#### 3. Dependency Resolution for Templates

When a deferred step depends on a template, the system must resolve it to actual instances:

```rust
// In dependency resolution logic
fn resolve_template_dependencies(
    step: &WorkflowStep,
    created_steps: &HashSet<String>,
) -> Vec<String> {
    let mut resolved_deps = Vec::new();
    
    for dep in &step.dependencies {
        if is_template_step(dep) {
            // Find all instances created from this template
            let instances = created_steps
                .iter()
                .filter(|name| name.starts_with(&format!("{}_", dep)))
                .cloned()
                .collect::<Vec<_>>();
            
            resolved_deps.extend(instances);
        } else {
            // Regular dependency
            if created_steps.contains(dep) {
                resolved_deps.push(dep.clone());
            }
        }
    }
    
    resolved_deps
}
```

#### 4. Batch Worker Lifecycle

Each worker instance follows this lifecycle:

1. **Creation**: Instance created from template with unique name and cursor config
2. **Initialization**: Worker reads its cursor config from metadata
3. **Processing**: Executes batch with periodic checkpointing
4. **Recovery**: If failed, can resume from last checkpoint
5. **Completion**: Marks batch as complete, enables convergence step

#### 5. Template Validation Rules

The system should enforce these rules for batch worker templates:

- Template steps must have `type: batch_worker`
- Templates cannot be executed directly (only instances)
- Template names must be unique within a workflow
- Templates must depend on exactly one batchable step
- Instance naming must follow a deterministic pattern

### DAG Edge Management for Batch Workers

The batch processing system must properly integrate with the existing DAG infrastructure to ensure SQL functions for step readiness and enqueueing work correctly. This requires careful management of `WorkflowStepEdge` relationships.

#### 1. Edge Creation During Batch Instantiation

When creating batch worker instances, we must establish proper DAG edges:

```rust
// In BatchProcessingService
async fn create_worker_dag_edges(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batchable_step_uuid: Uuid,
    worker_instance_uuids: Vec<Uuid>,
    convergence_step_uuid: Option<Uuid>,
) -> TaskerResult<()> {
    // 1. Create edges from batchable step to each worker
    for worker_uuid in &worker_instance_uuids {
        WorkflowStepEdge::create_with_transaction(
            tx,
            NewWorkflowStepEdge {
                from_step_uuid: batchable_step_uuid,
                to_step_uuid: *worker_uuid,
                name: format!("batch_split_{}", worker_uuid),
            }
        ).await?;
    }
    
    // 2. If convergence step exists, create edges from workers to it
    if let Some(convergence_uuid) = convergence_step_uuid {
        for worker_uuid in &worker_instance_uuids {
            WorkflowStepEdge::create_with_transaction(
                tx,
                NewWorkflowStepEdge {
                    from_step_uuid: *worker_uuid,
                    to_step_uuid: convergence_uuid,
                    name: format!("batch_converge_{}", worker_uuid),
                }
            ).await?;
        }
    }
    
    Ok(())
}
```

#### 2. Dependency Resolution via SQL Functions

The existing `get_step_readiness_status()` and `get_step_transitive_dependencies()` functions will automatically work with dynamically created batch workers because:

1. **Step Readiness**: Each batch worker has an edge from the batchable step, so they become ready when the batchable step completes
2. **Convergence Readiness**: The convergence step has edges from all batch workers, so it waits for all to complete
3. **Transitive Dependencies**: The recursive CTE traverses the DAG correctly regardless of when edges were created

#### 3. Dynamic Dependency Resolution for Deferred Steps

When a deferred convergence step depends on a template, we need special handling:

```rust
// Enhanced dependency resolution for templates
async fn resolve_deferred_dependencies(
    &self,
    pool: &PgPool,
    deferred_step_uuid: Uuid,
    task_uuid: Uuid,
) -> TaskerResult<Vec<Uuid>> {
    // Get declared dependencies from named_step definition
    let declared_deps = self.get_declared_dependencies(deferred_step_uuid).await?;
    
    let mut resolved_deps = Vec::new();
    
    for dep_name in declared_deps {
        if self.is_template_step(&dep_name).await? {
            // Find all instances created from this template
            let instances = sqlx::query!(
                r#"
                SELECT ws.workflow_step_uuid
                FROM tasker_workflow_steps ws
                JOIN tasker_named_steps ns ON ws.named_step_uuid = ns.named_step_uuid
                WHERE ws.task_uuid = $1
                  AND ns.name LIKE $2 || '_%'
                  AND ns.metadata->>'template_source' = $2
                "#,
                task_uuid,
                dep_name
            )
            .fetch_all(pool)
            .await?;
            
            resolved_deps.extend(instances.iter().map(|r| r.workflow_step_uuid));
        } else {
            // Regular dependency - find by exact name
            let step = self.find_step_by_name(task_uuid, &dep_name).await?;
            if let Some(step_uuid) = step {
                resolved_deps.push(step_uuid);
            }
        }
    }
    
    resolved_deps
}
```

#### 4. SQL Function Compatibility

The batch system leverages existing SQL functions without modification:

**`get_step_readiness_status()`** - Works automatically because:
- Batch workers have proper parent edges from batchable step
- Convergence step has parent edges from all batch workers
- Function uses `workflow_step_edges` to determine dependencies

**`get_ready_steps()`** - Discovers batch workers when:
- Batchable step is complete
- Workers have no other blocking dependencies
- Not already in progress or complete

**`get_step_transitive_dependencies()`** - Traverses correctly:
- Follows edges regardless of creation time
- Batch workers appear in dependency chain
- Used by convergence step to gather all results

#### 5. Edge Validation and Cycle Detection

Before creating edges, validate DAG integrity:

```rust
async fn validate_batch_dag_structure(
    &self,
    pool: &PgPool,
    batchable_step_uuid: Uuid,
    worker_uuids: &[Uuid],
    convergence_step_uuid: Option<Uuid>,
) -> TaskerResult<()> {
    // Check for cycles (though unlikely with batch pattern)
    if let Some(convergence) = convergence_step_uuid {
        // Verify no path from convergence back to batchable
        if WorkflowStepEdge::would_create_cycle(
            pool,
            convergence,
            batchable_step_uuid
        ).await? {
            return Err(TaskerError::ValidationError(
                "Batch structure would create cycle in DAG".into()
            ));
        }
    }
    
    // Verify workers don't already have edges
    for worker_uuid in worker_uuids {
        let existing_deps = WorkflowStepEdge::find_dependencies(
            pool,
            *worker_uuid
        ).await?;
        
        if !existing_deps.is_empty() {
            return Err(TaskerError::ValidationError(
                format!("Worker {} already has dependencies", worker_uuid)
            ));
        }
    }
    
    Ok(())
}
```

#### 6. Complete DAG Integration Flow

1. **Batchable Step Executes**: Determines batch count and configuration
2. **Create Worker Instances**: Generate N workers from template with unique UUIDs
3. **Create DAG Edges**: 
   - Batchable → Worker edges (N edges)
   - Worker → Convergence edges (N edges)
4. **Update Convergence Dependencies**: Map template dependency to actual worker UUIDs
5. **SQL Functions Work**: Readiness, enqueueing, and dependency queries function normally
6. **Recovery Preserves DAG**: Failed workers maintain edges for proper re-enqueueing

This ensures the batch processing system is fully compatible with the existing orchestration infrastructure.

### Deferred Convergence with Dynamic Batch Workers

The convergence step uses the `deferred` type (from conditional workflows) to handle the dynamic nature of batch workers. This requires special handling since the workers don't exist at workflow definition time.

#### 1. Template Dependency in YAML

The convergence step declares a dependency on the template, not individual workers:

```yaml
- name: aggregate_results
  type: deferred
  dependencies:
    - process_records           # The batchable step
    - batch_worker_template     # Template, not instances
  handler:
    callable: DataPipeline::AggregateResultsHandler
```

#### 2. Runtime Dependency Resolution

When the orchestration system evaluates the deferred step's readiness:

```rust
// In StepReadinessAnalyzer
async fn evaluate_deferred_step_readiness(
    &self,
    step: &WorkflowStep,
    task_uuid: Uuid,
) -> TaskerResult<bool> {
    // Get declared dependencies from step definition
    let declared_deps = step.get_declared_dependencies();
    
    // Get actually created steps for this task
    let created_steps = self.get_created_steps_for_task(task_uuid).await?;
    
    // Resolve template dependencies to instances
    let mut effective_deps = Vec::new();
    for dep in declared_deps {
        if dep == "batch_worker_template" {
            // Find all instances created from this template
            let instances = created_steps.iter()
                .filter(|s| s.name.starts_with("batch_worker_"))
                .filter(|s| s.metadata.get("template_source") == Some("batch_worker_template"))
                .map(|s| s.workflow_step_uuid)
                .collect::<Vec<_>>();
            effective_deps.extend(instances);
        } else {
            // Regular dependency
            if let Some(step) = created_steps.iter().find(|s| s.name == dep) {
                effective_deps.push(step.workflow_step_uuid);
            }
        }
    }
    
    // Check if all effective dependencies are complete
    let all_complete = self.check_dependencies_complete(&effective_deps).await?;
    Ok(all_complete)
}
```

#### 3. Result Aggregation

The convergence step can access results from all batch workers:

```ruby
class AggregateResultsHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    # Get results from all batch workers
    batch_results = sequence.transitive_dependencies
      .select { |dep| dep.step_name.start_with?('batch_worker_') }
      .map { |dep| dep.results }
    
    # Aggregate batch results
    total_processed = batch_results.sum { |r| r['items_processed'] || 0 }
    errors = batch_results.flat_map { |r| r['errors'] || [] }
    
    # Check all batches completed successfully
    all_successful = batch_results.all? { |r| r['status'] == 'complete' }
    
    if all_successful
      success(
        result_data: {
          total_items_processed: total_processed,
          batch_count: batch_results.count,
          aggregation_complete: true
        }
      )
    else
      error(
        result_data: {
          total_items_processed: total_processed,
          errors: errors,
          failed_batches: batch_results.count { |r| r['status'] != 'complete' }
        }
      )
    end
  end
end
```

#### 4. SQL Function Integration

The `get_step_transitive_dependencies()` function automatically includes batch workers:

```sql
-- When convergence step queries its dependencies
SELECT * FROM get_step_transitive_dependencies(convergence_step_uuid)

-- Returns:
-- batch_worker_001 | {results...} | true | 1
-- batch_worker_002 | {results...} | true | 1
-- batch_worker_003 | {results...} | true | 1
-- process_records  | {batch_config...} | true | 2
```

#### 5. Edge Cases and Special Handling

**No Workers Created**: If the batchable step decides no batching is needed:
- Returns `BatchProcessingOutcome::NoBatches`
- No worker instances created
- Convergence step's template dependency resolves to empty set
- Convergence step becomes ready immediately after batchable step

**Partial Worker Failure**: Some workers fail while others succeed:
- Failed workers remain in DAG with error state
- Convergence step waits for all workers (success or failure)
- Can aggregate partial results and report overall failure
- Recovery can re-enqueue failed workers while preserving completed ones

**Dynamic Worker Count**: Different executions create different numbers of workers:
- Each task execution independently resolves dependencies
- Template → instance mapping is task-specific
- No interference between concurrent task executions

This approach ensures the convergence step correctly waits for all dynamically created batch workers while maintaining the flexibility of runtime batch sizing.

### Batch Failure Handling Strategies

Different batch processing scenarios require different failure handling approaches. The system supports configurable strategies:

#### 1. Failure Strategy Options

```yaml
batch_config:
  failure_strategy: <strategy>  # Choose one of the following
```

**`continue_on_failure`** (Default)
- Failed batches are isolated but don't block other batches
- Convergence step receives partial results
- Task can complete with some failed batches
- Use for: Best-effort processing, analytics, non-critical operations

**`fail_fast`**
- First batch failure immediately fails entire task
- Other running batches are cancelled
- No convergence step execution
- Use for: All-or-nothing operations, financial transactions

**`isolate`**
- Failed batches are marked for manual review
- Other batches continue processing
- Task blocks at convergence until manual resolution
- Use for: Sensitive operations requiring human oversight

**`retry_with_backoff`**
- Failed batches automatically retry with exponential backoff
- Configurable max retries before isolation
- Use for: Transient failures, API rate limiting

#### 2. Resumability Configuration

```yaml
batch_config:
  resumable: true|false  # Enable/disable automatic recovery
```

When `resumable: false`:
- Stalled batches are NOT automatically recovered
- System marks them for manual intervention
- Operators must manually resolve or restart
- Preserves audit trail and investigation capability

#### 3. Configuration Examples

**Financial Transaction Processing** (No auto-recovery, fail-fast):
```yaml
- name: process_payments
  type: batchable
  batch_config:
    batch_size: 100
    parallelism: 3
    resumable: false        # No automatic recovery
    failure_strategy: fail_fast  # Any failure fails entire task
    checkpoint_interval: 1   # Checkpoint every transaction
  handler:
    callable: Payments::BatchProcessor
```

**Data Analytics Pipeline** (Auto-recovery, continue on failure):
```yaml
- name: aggregate_metrics
  type: batchable
  batch_config:
    batch_size: 10000
    parallelism: 20
    resumable: true         # Automatic recovery enabled
    failure_strategy: continue_on_failure
    checkpoint_interval: 1000
  handler:
    callable: Analytics::BatchProcessor
```

**Compliance Report Generation** (Manual intervention required):
```yaml
- name: generate_compliance_reports
  type: batchable
  batch_config:
    batch_size: 500
    parallelism: 5
    resumable: false        # No automatic recovery
    failure_strategy: isolate  # Require manual review
    checkpoint_interval: 50
  handler:
    callable: Compliance::ReportBatchProcessor
```

#### 4. Failure Handling in Convergence Step

The convergence step can inspect failure strategies to determine behavior:

```ruby
class AggregateResultsHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    batch_config = task.context['batch_config']
    batch_results = get_batch_results(sequence)
    
    failed_batches = batch_results.select { |r| r['status'] == 'error' }
    
    case batch_config['failure_strategy']
    when 'fail_fast'
      # Should not reach here - task already failed
      error(result_data: { reason: 'Batch processing failed' })
      
    when 'isolate'
      if failed_batches.any?
        # Block until manual resolution
        pending(
          result_data: {
            isolated_batches: failed_batches.count,
            awaiting_manual_resolution: true
          }
        )
      else
        success(result_data: aggregate_results(batch_results))
      end
      
    when 'continue_on_failure'
      # Process partial results
      success(
        result_data: {
          **aggregate_results(batch_results),
          partial_failure: failed_batches.any?,
          failed_batch_count: failed_batches.count
        }
      )
    end
  end
end
```

#### 5. Manual Resolution Interface

For non-resumable batches requiring manual intervention:

```rust
// Manual resolution API
pub async fn resolve_batch_manually(
    &self,
    workflow_step_uuid: Uuid,
    resolution: ManualResolution,
) -> TaskerResult<()> {
    match resolution {
        ManualResolution::Retry => {
            // Reset cursor and re-enqueue
            self.reset_batch_cursor(workflow_step_uuid).await?;
            self.step_enqueuer.enqueue_single_step(workflow_step_uuid).await?;
        }
        ManualResolution::Skip => {
            // Mark as resolved_manually and continue
            self.mark_step_resolved_manually(workflow_step_uuid).await?;
        }
        ManualResolution::FailTask => {
            // Fail the entire task
            self.fail_task_from_batch(workflow_step_uuid).await?;
        }
    }
    Ok(())
}
```

This flexibility ensures the batch processing system can handle sensitive operations appropriately while still providing automation where desired.

### Critical Distinction: Batch Failure vs Step State Transitions

**Important**: Batch processing failures are distinct from workflow step failures. The failure strategy determines how the step state machine transitions, which affects DAG traversal and task completion.

#### Step State Transition Based on Failure Strategy

The batch worker's failure strategy determines the workflow step's final state:

**1. `fail_fast` Strategy → Step Transitions to Error State**
```ruby
class BatchWorkerHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    batch_config = step.metadata['batch_config']
    
    begin
      process_batch_with_cursor(cursor) do |item|
        # Process item...
      end
    rescue => e
      if batch_config['failure_strategy'] == 'fail_fast'
        # Step transitions to Error state via worker error path
        # This prevents convergence step from ever becoming ready
        error(
          result_data: { 
            batch_id: cursor['batch_id'],
            failure_reason: e.message 
          },
          retryable: false  # No retry for fail_fast
        )
      end
    end
  end
end
```
**Result**: Step state = `Error` → Convergence step dependencies never satisfied → Task blocked

**2. `continue_on_failure` Strategy → Step Transitions to Complete State**
```ruby
def call(task, sequence, step)
  batch_config = step.metadata['batch_config']
  
  begin
    process_batch_with_cursor(cursor) do |item|
      # Process item...
    end
  rescue => e
    if batch_config['failure_strategy'] == 'continue_on_failure'
      # Step completes successfully despite batch failure
      # This is "success of the effort" not "success of the operation"
      success(
        result_data: { 
          batch_id: cursor['batch_id'],
          partial_failure: true,
          items_processed: cursor['items_processed'],
          error: e.message
        }
      )
    end
  end
end
```
**Result**: Step state = `Complete` → Convergence step can proceed with partial results

**3. `isolate` Strategy → Step Transitions to ResolvedManually State**
```ruby
def call(task, sequence, step)
  batch_config = step.metadata['batch_config']
  
  begin
    process_batch_with_cursor(cursor) do |item|
      # Process item...
    end
  rescue => e
    if batch_config['failure_strategy'] == 'isolate'
      # Mark for manual intervention
      # Step will be transitioned to ResolvedManually by operator
      enqueue_for_orchestration(
        result_data: { 
          batch_id: cursor['batch_id'],
          requires_manual_resolution: true,
          error: e.message
        },
        action_required: 'manual_resolution'
      )
    end
  end
end
```
**Result**: Step state = `EnqueuedForOrchestration` → Awaits manual resolution → `ResolvedManually`

#### State Machine Integration

The step state transitions follow existing patterns from TAS-41:

1. **Error State Path** (`fail_fast`):
   - Worker returns error result
   - Step transitions: `InProgress` → `Error`
   - Task state may transition to `BlockedByFailures`
   - Convergence step never becomes ready

2. **Success State Path** (`continue_on_failure`):
   - Worker returns success result with partial failure metadata
   - Step transitions: `InProgress` → `Complete`
   - Convergence step dependencies satisfied
   - Convergence handler inspects metadata to determine overall outcome

3. **Manual Resolution Path** (`isolate`):
   - Worker enqueues for orchestration review
   - Step transitions: `InProgress` → `EnqueuedForOrchestration`
   - Operator intervenes and transitions to `ResolvedManually`
   - If resolved as success, convergence step can proceed
   - If resolved as failure, task may be blocked

#### DAG Traversal Implications

**Key Principle**: The convergence step's readiness depends on the batch workers' **step states**, not their processing outcomes:

```sql
-- Convergence step readiness check
SELECT COUNT(*) = COUNT(CASE WHEN satisfies_dependencies(state) THEN 1 END)
FROM workflow_steps
WHERE workflow_step_uuid IN (batch_worker_001, batch_worker_002, ...)

-- satisfies_dependencies returns true for:
-- Complete, Error (terminal), ResolvedManually
-- Returns false for:
-- InProgress, EnqueuedForOrchestration (non-terminal)
```

**Scenarios**:
- All workers `Complete` (even with partial failures) → Convergence ready
- Any worker `Error` with `fail_fast` → Convergence never ready, task blocked
- Some workers `ResolvedManually` → Convergence ready after resolution
- Any worker `InProgress` or `EnqueuedForOrchestration` → Convergence waits

#### Implementation Guidance

1. **Batch Worker Handlers** must return appropriate step results based on failure strategy
2. **Convergence Handlers** inspect metadata, not step states, to determine success
3. **Recovery Service** respects step states when determining eligibility
4. **Manual Resolution** transitions step to terminal state enabling workflow continuation

This architecture ensures batch processing integrates cleanly with the existing state machine and DAG traversal logic without special cases.

### Batch Resumability via Step Retry Mechanism

Batch resumability leverages the existing step retry and backoff infrastructure rather than creating a parallel recovery system. This ensures consistent behavior and reuses proven patterns.

#### Integration with Standard Retry Logic

When a batch worker encounters a recoverable failure, it uses the standard error-with-retry path:

**1. Batch Worker Signals Retryable Error**
```ruby
class BatchWorkerHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    cursor = step.results['batch_cursor'] || initialize_cursor(step)
    
    begin
      # Resume from cursor position
      process_batch_from_cursor(cursor) do |item, index|
        process_item(item)
        update_cursor_checkpoint(step, cursor, index) if index % 100 == 0
      end
      
      success(result_data: { items_processed: cursor['items_processed'] })
      
    rescue => e
      if recoverable_error?(e)
        # Save cursor state for resume
        persist_cursor_state(step, cursor)
        
        # Use standard retry mechanism with backoff
        error(
          result_data: {
            batch_id: cursor['batch_id'],
            last_position: cursor['current_position'],
            error: e.message
          },
          retryable: true,  # Triggers standard retry logic
          backoff_seconds: calculate_backoff(step.attempts)
        )
      else
        # Non-recoverable error - follow failure strategy
        handle_non_recoverable_error(e, cursor, step)
      end
    end
  end
  
  private
  
  def initialize_cursor(step)
    # First execution - initialize cursor
    {
      'batch_id' => step.metadata['batch_id'],
      'current_position' => step.metadata['start_position'],
      'end_position' => step.metadata['end_position'],
      'items_processed' => 0,
      'checkpoint_interval' => 100
    }
  end
  
  def process_batch_from_cursor(cursor, &block)
    # Key: Resume from cursor position, not from beginning
    start_pos = cursor['current_position'] || cursor['start_position']
    end_pos = cursor['end_position']
    
    (start_pos..end_pos).each do |position|
      yield(fetch_item_at(position), position)
      cursor['current_position'] = position + 1
      cursor['items_processed'] += 1
    end
  end
end
```

**2. Standard Retry Flow with Cursor Preservation**
```
Initial Execution:
  → Process items 1-500
  → Failure at item 501
  → Cursor saved: current_position=501
  → Step transitions to Error (retryable)
  → EnqueuedForOrchestration with backoff

Retry 1 (after backoff):
  → Load cursor: current_position=501
  → Resume from item 501 (not item 1!)
  → Process items 501-750
  → Failure at item 751
  → Cursor saved: current_position=751
  → Increased backoff

Retry 2 (after longer backoff):
  → Load cursor: current_position=751
  → Resume from item 751
  → Complete items 751-1000
  → Success
```

#### Configuration Alignment

The batch configuration must align with retry expectations:

```yaml
- name: batch_worker_template
  type: batch_worker
  handler:
    callable: DataPipeline::BatchWorkerHandler
  retry_config:
    max_attempts: 3          # Standard retry limit
    backoff_multiplier: 2.0  # Standard exponential backoff
    initial_backoff: 30      # Seconds before first retry
  batch_config:
    resumable: true          # Enables cursor-based resume
    checkpoint_interval: 100 # Frequency of cursor saves
```

#### Key Design Principles

1. **Cursor Persistence**: The cursor is stored in `workflow_steps.results` and survives retries
2. **Idempotent Checkpointing**: Cursor updates are idempotent - can safely re-save same position
3. **Backoff Calculation**: Uses existing step backoff logic, not custom batch backoff
4. **Retry Limits**: Respects standard step retry limits from configuration
5. **State Transitions**: Follows standard Error → WaitingForRetry → Enqueued flow

#### Resumability Decision Tree

```
Batch Failure Occurs
    ↓
Is error recoverable? (e.g., network timeout, rate limit)
    ├─ Yes → Return error(retryable: true)
    │         → Standard retry with backoff
    │         → Resume from cursor on retry
    └─ No → Check failure_strategy
            ├─ fail_fast → error(retryable: false)
            ├─ continue_on_failure → success(partial: true)
            └─ isolate → enqueue_for_orchestration()
```

#### Handler Implementation Guidelines

1. **Always Check for Existing Cursor**: On each execution, check if cursor exists (indicates retry)
2. **Make Processing Idempotent**: Ensure re-processing from checkpoint doesn't duplicate work
3. **Save Cursor Before Error**: Persist cursor state before returning error result
4. **Use Standard Backoff**: Let the framework handle backoff calculation via existing logic
5. **Clear Cursor on Success**: Clean up cursor data when batch completes successfully

#### Benefits of This Approach

- **No New Infrastructure**: Reuses existing retry/backoff mechanisms
- **Consistent Behavior**: Batch retries work like any other step retry
- **Observability**: Existing retry metrics and monitoring apply
- **Configuration Simplicity**: Single retry configuration for all step types
- **Recovery Transparency**: Cursor-based resume is invisible to orchestration layer

This integration ensures that batch resumability is not a special case but rather a natural extension of the existing retry mechanism, with the cursor providing the additional context needed for efficient resume operations.

### Compatibility with TAS-49 DLQ and Staleness Detection

The batch processing system must integrate carefully with TAS-49's Dead Letter Queue and staleness detection to avoid false positives and ensure proper lifecycle management.

#### 1. Timeout Configuration Hierarchy

Batch workers may legitimately run much longer than regular steps. The timeout hierarchy ensures proper detection:

```yaml
# Global defaults (orchestration.toml)
[orchestration.staleness_detection.thresholds]
waiting_for_dependencies_minutes = 60    # Regular steps
waiting_for_retry_minutes = 30          # Regular steps
steps_in_process_minutes = 30           # Regular steps

# Template-specific overrides (YAML)
lifecycle:
  max_steps_in_process_minutes: 120     # Template-wide override

# Batch-specific configuration
batch_config:
  max_batch_duration_minutes: 240       # Batch workers can run 4 hours
  checkpoint_stall_minutes: 15          # No checkpoint for 15min = stalled
```

**Resolution Order**:
1. Batch-specific timeout (if batch worker)
2. Template lifecycle config (if defined)
3. Global staleness thresholds (fallback)

#### 2. Staleness Detection for Batch Workers

The staleness detector must distinguish between batch workers and regular steps:

```rust
// In StalenessDetector
async fn is_batch_worker_healthy(
    &self,
    step: &WorkflowStep,
) -> TaskerResult<bool> {
    // Check if this is a batch worker
    if let Some(batch_cursor) = step.results.get("batch_cursor") {
        // Batch worker - check cursor checkpoint
        let last_checkpoint = batch_cursor["last_checkpoint"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok());
        
        if let Some(checkpoint_time) = last_checkpoint {
            let stall_threshold = self.get_batch_checkpoint_threshold(step);
            let elapsed = Utc::now() - checkpoint_time;
            
            if elapsed > stall_threshold {
                // Batch is stalled - no recent checkpoint
                return Ok(false);
            }
        }
        
        // Check overall batch duration
        let max_duration = self.get_batch_max_duration(step);
        if step.duration_in_state() > max_duration {
            return Ok(false);
        }
        
        Ok(true)  // Batch is healthy
    } else {
        // Regular step - use standard thresholds
        self.is_regular_step_healthy(step).await
    }
}
```

#### 3. DLQ Integration with Batch Failure Strategies

Batch failure strategies determine DLQ behavior:

```rust
// DLQ reason mapping for batch failures
pub enum BatchDlqReason {
    BatchStalledNoCheckpoint,     // No cursor progress
    BatchExceededMaxDuration,     // Overall timeout
    BatchFailedWithStrategy,      // Based on failure_strategy
}

// How failure strategies map to DLQ
match batch_config.failure_strategy {
    "fail_fast" => {
        // Step transitions to Error → Task to DLQ
        create_dlq_entry(
            task_uuid,
            DlqReason::BatchFailure,
            metadata: { strategy: "fail_fast", batch_id }
        )
    }
    "isolate" => {
        // Step awaits manual resolution → DLQ for investigation
        create_dlq_entry(
            task_uuid,
            DlqReason::ManualInterventionRequired,
            metadata: { strategy: "isolate", batch_id }
        )
    }
    "continue_on_failure" => {
        // Step completes → No DLQ (partial success logged)
        // DLQ only if convergence determines overall failure
    }
}
```

#### 4. Preventing False Positive Detections

Long-running batch workers must not trigger premature staleness detection:

```toml
# Batch-aware staleness configuration
[orchestration.staleness_detection.batch_awareness]
enabled = true
respect_batch_timeouts = true
checkpoint_based_health = true    # Use cursor checkpoints, not just duration

# Batch-specific exemptions
[orchestration.staleness_detection.exemptions]
batch_workers_use_checkpoint = true    # Don't use simple timeout
batch_max_duration_override = true     # Allow batch-specific max duration
```

#### 5. Recovery Service Coordination

The BatchRecoveryService and StalenessDetector must coordinate to avoid conflicts:

```rust
// Coordination between recovery services
impl BatchRecoveryService {
    async fn should_recover_batch(&self, step: &WorkflowStep) -> TaskerResult<bool> {
        // Check if already in DLQ
        if self.is_in_dlq(step.task_uuid).await? {
            // Let DLQ investigation handle it
            return Ok(false);
        }
        
        // Check batch-specific recovery criteria
        if !self.is_batch_stalled(step).await? {
            return Ok(false);
        }
        
        // Check if manual resolution required
        if step.metadata.get("failure_strategy") == Some("isolate") {
            // Don't auto-recover isolated batches
            return Ok(false);
        }
        
        Ok(true)
    }
}

// StalenessDetector respects batch configuration
impl StalenessDetector {
    async fn should_transition_to_dlq(&self, task: &Task) -> TaskerResult<bool> {
        // Check if any steps are batch workers with active checkpoints
        let has_active_batches = self.has_active_batch_workers(task.task_uuid).await?;
        
        if has_active_batches {
            // Don't transition task while batches are progressing
            return Ok(false);
        }
        
        // Apply standard staleness rules
        self.apply_standard_rules(task).await
    }
}
```

#### 6. Configuration Guidelines

To ensure compatibility, follow these configuration patterns:

**For Short-Running Batches** (< 30 minutes):
```yaml
batch_config:
  max_batch_duration_minutes: 30
  checkpoint_interval: 100
  resumable: true
  failure_strategy: retry_with_backoff
```

**For Long-Running Batches** (hours/days):
```yaml
batch_config:
  max_batch_duration_minutes: 1440  # 24 hours
  checkpoint_interval: 1000         # Less frequent checkpoints
  checkpoint_stall_minutes: 60      # Allow longer between checkpoints
  resumable: true
  failure_strategy: continue_on_failure

# Template lifecycle override
lifecycle:
  max_steps_in_process_minutes: 1500  # 25 hours (> batch duration)
```

**For Sensitive Batches** (financial):
```yaml
batch_config:
  max_batch_duration_minutes: 60
  checkpoint_interval: 10           # Frequent checkpoints
  resumable: false                  # No auto-recovery
  failure_strategy: isolate         # Manual investigation required
```

#### 7. Observability Integration

Unified metrics for both systems:

```rust
// Batch-aware DLQ metrics
dlq_entries_created_total{reason="batch_stalled"}
dlq_entries_created_total{reason="batch_checkpoint_timeout"}
batch_recovery_attempts_total{source="dlq_investigation"}
batch_recovery_attempts_total{source="auto_recovery"}

// Staleness detection with batch awareness
staleness_detection_excluded{reason="active_batch_checkpoint"}
staleness_detection_duration{step_type="batch_worker"}
```

This compatibility ensures batch processing and DLQ/staleness detection work together without interference, while maintaining appropriate oversight for both short-running and long-running batch operations.

### Alternative Approaches Considered

We evaluated several approaches before settling on the template-based design:

#### 1. Pre-enumerated Workers (Rejected)
**Approach**: Define all possible workers upfront in YAML (batch_worker_001 through batch_worker_N)
**Problems**:
- Inflexible - must know max parallelism at design time
- Verbose - YAML becomes unwieldy with many workers
- Wasteful - creates unused steps if fewer workers needed
- Maintenance burden - updating requires changing multiple definitions

#### 2. Dynamic Step Names Only (Considered)
**Approach**: Return step names to create, system generates definitions on the fly
**Problems**:
- No template validation at workflow design time
- Handler configuration must be passed through outcome
- Loses type safety and compile-time checking
- Similar to decision points but less structured

#### 3. Worker Pool Pattern (Interesting but Complex)
**Approach**: Pre-create a pool of generic workers that claim work from a queue
**Problems**:
- Requires additional queue infrastructure
- Harder to track individual batch progress
- Doesn't fit well with existing step state machine
- More complex recovery logic needed

#### 4. Template-Based Approach (Chosen)
**Approach**: Define single template, instantiate N times with cursors
**Benefits**:
- ✅ Clean YAML - single template definition
- ✅ Runtime flexibility - create 1 to N workers as needed
- ✅ Type safety - template validated at workflow creation
- ✅ Consistent with existing patterns (similar to decision points)
- ✅ Clear ownership - each worker owns its batch via cursor
- ✅ Simple recovery - standard step retry mechanisms work
- ✅ Dependency clarity - deferred steps reference template

The template approach provides the best balance of flexibility, simplicity, and consistency with the existing architecture. It extends naturally from the decision point pattern while addressing the specific needs of batch processing.

### Implementation Phases

#### Phase 1: Core Infrastructure (Week 1)
- [ ] Create `BatchProcessingOutcome` enum and types
- [ ] Add `batchable` step type support to task template parser
- [ ] Create `BatchProcessingActor` following TAS-46 patterns
- [ ] Implement `BatchProcessingService` for batch splitting logic
- [ ] Add cursor storage to workflow_steps.results

#### Phase 2: Batch Worker Support (Week 2)
- [ ] Implement cursor checkpoint mechanism
- [ ] Create Ruby `BatchWorker` base class
- [ ] Add cursor update SQL functions
- [ ] Implement batch progress tracking
- [ ] Create batch worker step templates

#### Phase 3: Recovery System (Week 3)
- [ ] Create stalled batch detection SQL function
- [ ] Implement `BatchRecoveryService`
- [ ] Add recovery loop to orchestration system
- [ ] Create recovery metrics and monitoring
- [ ] Add configuration for stall detection thresholds

#### Phase 4: Testing & Polish (Week 4)
- [ ] E2E integration tests with batch processing
- [ ] Stall simulation and recovery tests
- [ ] Performance benchmarks for large batches
- [ ] Documentation and examples
- [ ] Monitoring dashboard updates

### Configuration Changes

Add to `config/tasker/base/orchestration.toml`:

```toml
[orchestration.batch_processing]
enabled = true
max_parallelism = 20          # Maximum parallel batch workers
default_batch_size = 1000     # Default items per batch
checkpoint_interval = 100     # Checkpoint every N items
stall_timeout_seconds = 300   # Mark as stalled after 5 minutes
cursor_stall_seconds = 60     # No checkpoint for 1 minute = stalled
default_resumable = true      # Default resumability for batches
default_failure_strategy = "continue_on_failure"  # Default failure handling

[orchestration.batch_recovery]
enabled = true
check_interval_seconds = 30   # Check for stalled batches every 30s
max_retries = 3              # Max recovery attempts per batch
backoff_multiplier = 2.0     # Exponential backoff for retries
```

### Migration Strategy

1. **Database Migration**: Add indexes for efficient cursor queries
2. **Backward Compatibility**: Regular steps continue to work unchanged
3. **Feature Flag**: Use `orchestration.batch_processing.enabled` flag
4. **Gradual Rollout**: Test with small batches before scaling up

### Monitoring & Observability

Key metrics to track:
- Batch split decisions and sizes
- Cursor checkpoint frequency
- Stalled batch detection rate
- Recovery success rate
- Average batch processing time
- Items processed per second

### Example Use Cases

1. **Large CSV Processing**: Split 1M row CSV into 100 batches
2. **API Data Sync**: Paginate through external API with cursor
3. **Database Migration**: Process records in batches with checkpoints
4. **Report Generation**: Parallel processing of report segments
5. **Email Campaigns**: Send emails in batches with rate limiting

### Success Criteria

- Batch processing with automatic parallelization
- Cursor-based resumability after failures
- Zero data loss during worker crashes
- Automatic recovery of stalled batches
- Performance improvement for large datasets
- Clean integration with existing workflow patterns

This implementation plan leverages the established patterns from conditional workflows (TAS-53) and the actor architecture (TAS-46), ensuring consistency with the existing system while adding powerful batch processing capabilities.

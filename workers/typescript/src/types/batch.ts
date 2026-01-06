/**
 * Batch processing types for cursor-based batch operations.
 *
 * These types support the analyzer/worker pattern for processing
 * large datasets in parallel batches.
 *
 * Matches Python's batch processing types and Ruby's batch types (TAS-92 aligned).
 *
 * ## FFI Boundary Types (TAS-112/TAS-123)
 *
 * This module includes types that cross the Rust ↔ TypeScript FFI boundary:
 *
 * - `RustCursorConfig` - Cursor configuration with flexible cursor types
 * - `BatchProcessingOutcome` - Discriminated union for batch processing decisions
 * - `RustBatchWorkerInputs` - Worker initialization inputs from Rust orchestration
 * - `BatchMetadata` - Batch processing metadata from template configuration
 *
 * These types are serialized by Rust and deserialized by TypeScript workers.
 * They must remain compatible with Rust's serde serialization format.
 *
 * @module types/batch
 */

/**
 * Configuration for cursor-based batch processing.
 *
 * Defines a range of items to process in a batch worker step.
 */
export interface CursorConfig {
  /** Starting cursor position (inclusive) */
  startCursor: number;
  /** Ending cursor position (exclusive) */
  endCursor: number;
  /** Step size for iteration (usually 1) */
  stepSize: number;
  /** Additional metadata for this cursor range */
  metadata: Record<string, unknown>;
}

/**
 * Outcome from a batch analyzer handler.
 *
 * Batch analyzers return this to define the cursor ranges that will
 * spawn parallel batch worker steps.
 */
export interface BatchAnalyzerOutcome {
  /** List of cursor configurations for batch workers */
  cursorConfigs: CursorConfig[];
  /** Total number of items to process (for progress tracking) */
  totalItems: number | null;
  /** Metadata to pass to all batch workers */
  batchMetadata: Record<string, unknown>;
}

/**
 * Context for a batch worker step.
 *
 * Provides information about the specific batch this worker should process,
 * including checkpoint data from previous yields (TAS-125).
 */
export interface BatchWorkerContext {
  /** Unique identifier for this batch */
  batchId: string;
  /** Cursor configuration for this batch */
  cursorConfig: CursorConfig;
  /** Index of this batch (0-based) */
  batchIndex: number;
  /** Total number of batches */
  totalBatches: number;
  /** Metadata from the analyzer */
  batchMetadata: Record<string, unknown>;
  /** TAS-125: Checkpoint data from previous yields */
  checkpoint: Record<string, unknown>;
  /** Convenience accessor for start cursor */
  readonly startCursor: number;
  /** Convenience accessor for end cursor */
  readonly endCursor: number;
  /** Convenience accessor for step size */
  readonly stepSize: number;
  // TAS-125: Checkpoint accessor properties
  /** TAS-125: Get checkpoint cursor from previous yield */
  readonly checkpointCursor: number | string | Record<string, unknown> | undefined;
  /** TAS-125: Get accumulated results from previous checkpoint yield */
  readonly accumulatedResults: Record<string, unknown> | undefined;
  /** TAS-125: Get items processed count from checkpoint */
  readonly checkpointItemsProcessed: number;
  /** TAS-125: Check if checkpoint exists */
  hasCheckpoint(): boolean;
}

/**
 * Outcome from a batch worker step.
 *
 * Batch workers return this to report progress and results.
 */
export interface BatchWorkerOutcome {
  /** Total items processed in this batch */
  itemsProcessed: number;
  /** Items that succeeded */
  itemsSucceeded: number;
  /** Items that failed */
  itemsFailed: number;
  /** Items that were skipped */
  itemsSkipped: number;
  /** Individual item results */
  results: Array<Record<string, unknown>>;
  /** Individual item errors */
  errors: Array<Record<string, unknown>>;
  /** Last cursor position processed */
  lastCursor: number | null;
  /** Additional batch metadata */
  batchMetadata: Record<string, unknown>;
}

/**
 * Create a BatchWorkerContext from raw batch data.
 *
 * Handles both formats:
 * - Nested: { batch_id, cursor_config: { start_cursor, end_cursor, ... } }
 * - Flat: { batch_id, start_cursor, end_cursor, ... }
 *
 * TAS-125: Also extracts checkpoint data if present.
 *
 * @internal
 */
export function createBatchWorkerContext(
  batchData: Record<string, unknown>,
  checkpoint?: Record<string, unknown>
): BatchWorkerContext {
  // Handle both nested cursor_config and flat format
  const cursorData = (batchData.cursor_config as Record<string, unknown>) || batchData;
  const cursorConfig: CursorConfig = {
    startCursor: (cursorData.start_cursor as number) ?? 0,
    endCursor: (cursorData.end_cursor as number) ?? 0,
    stepSize: (cursorData.step_size as number) ?? 1,
    metadata: (cursorData.metadata as Record<string, unknown>) ?? {},
  };

  // TAS-125: Extract checkpoint data
  const checkpointData = checkpoint ?? {};

  return {
    batchId: (batchData.batch_id as string) ?? '',
    cursorConfig,
    batchIndex: (batchData.batch_index as number) ?? 0,
    totalBatches: (batchData.total_batches as number) ?? 1,
    batchMetadata: (batchData.batch_metadata as Record<string, unknown>) ?? {},
    checkpoint: checkpointData,
    get startCursor() {
      return this.cursorConfig.startCursor;
    },
    get endCursor() {
      return this.cursorConfig.endCursor;
    },
    get stepSize() {
      return this.cursorConfig.stepSize;
    },
    // TAS-125: Checkpoint accessor properties
    get checkpointCursor() {
      return this.checkpoint?.cursor as number | string | Record<string, unknown> | undefined;
    },
    get accumulatedResults() {
      return this.checkpoint?.accumulated_results as Record<string, unknown> | undefined;
    },
    get checkpointItemsProcessed() {
      return (this.checkpoint?.items_processed as number) ?? 0;
    },
    hasCheckpoint() {
      return Boolean(this.checkpoint && this.checkpoint.cursor !== undefined);
    },
  };
}

// =============================================================================
// FFI Boundary Types (TAS-112/TAS-123)
//
// These types match Rust structures that cross the FFI boundary.
// They are serialized by Rust and deserialized by TypeScript workers.
// =============================================================================

/**
 * Cursor configuration for a single batch's position and range.
 *
 * Matches Rust's `CursorConfig` in `tasker-shared/src/messaging/execution_types.rs`.
 *
 * ## Flexible Cursor Types
 *
 * Unlike the simpler `CursorConfig` interface (which uses `number`),
 * this type supports flexible cursor values that can be:
 * - Integer for record IDs: `123`
 * - String for timestamps: `"2025-11-01T00:00:00Z"`
 * - Object for composite keys: `{"page": 1, "offset": 0}`
 *
 * This enables cursor-based pagination across diverse data sources.
 *
 * @example
 * ```typescript
 * // Integer cursors (most common)
 * const intCursor: RustCursorConfig = {
 *   batch_id: "batch_001",
 *   start_cursor: 0,
 *   end_cursor: 1000,
 *   batch_size: 1000,
 * };
 *
 * // Timestamp cursors
 * const timestampCursor: RustCursorConfig = {
 *   batch_id: "batch_001",
 *   start_cursor: "2025-01-01T00:00:00Z",
 *   end_cursor: "2025-01-02T00:00:00Z",
 *   batch_size: 86400, // seconds in a day
 * };
 *
 * // Composite cursors
 * const compositeCursor: RustCursorConfig = {
 *   batch_id: "batch_001",
 *   start_cursor: { page: 1, offset: 0 },
 *   end_cursor: { page: 10, offset: 0 },
 *   batch_size: 1000,
 * };
 * ```
 */
export interface RustCursorConfig {
  /** Batch identifier (e.g., "batch_001", "batch_002") */
  batch_id: string;

  /**
   * Starting position for this batch (inclusive).
   *
   * Type depends on cursor strategy:
   * - `number` for record IDs
   * - `string` for timestamps or UUIDs
   * - `object` for composite keys
   */
  start_cursor: unknown;

  /**
   * Ending position for this batch (exclusive).
   *
   * Workers process items from `start_cursor` (inclusive)
   * up to but not including `end_cursor`.
   */
  end_cursor: unknown;

  /** Number of items in this batch (for progress reporting) */
  batch_size: number;
}

/**
 * Failure strategy for batch processing.
 *
 * Matches Rust's `FailureStrategy` enum in `task_template.rs`.
 */
export type FailureStrategy = 'continue_on_failure' | 'fail_fast' | 'isolate';

/**
 * Batch processing metadata from template configuration.
 *
 * Matches Rust's `BatchMetadata` in `tasker-shared/src/models/core/batch_worker.rs`.
 *
 * This structure extracts relevant template configuration that workers
 * need during execution. Workers don't need parallelism settings or
 * batch size calculation logic - just execution parameters.
 */
export interface BatchMetadata {
  // TAS-125: checkpoint_interval removed - handlers decide when to checkpoint

  /**
   * Database field name used for cursor-based pagination.
   *
   * Workers use this to construct queries like:
   * `WHERE cursor_field > start_cursor AND cursor_field <= end_cursor`
   *
   * Common values: "id", "created_at", "sequence_number"
   */
  cursor_field: string;

  /**
   * How this worker should handle failures during batch processing.
   *
   * - `continue_on_failure`: Log errors, continue processing remaining items
   * - `fail_fast`: Stop immediately on first error
   * - `isolate`: Mark batch for manual investigation
   */
  failure_strategy: FailureStrategy;
}

/**
 * Initialization inputs for batch worker instances.
 *
 * Matches Rust's `BatchWorkerInputs` in `tasker-shared/src/models/core/batch_worker.rs`.
 *
 * This structure is serialized to JSONB by Rust orchestration and stored
 * in `workflow_steps.inputs` for dynamically created batch workers.
 *
 * @example
 * ```typescript
 * // In a batch worker handler
 * async call(context: StepContext): Promise<StepHandlerResult> {
 *   const inputs = context.stepInputs as RustBatchWorkerInputs;
 *
 *   // Check for no-op placeholder first
 *   if (inputs.is_no_op) {
 *     return this.success({
 *       batch_id: inputs.cursor.batch_id,
 *       no_op: true,
 *       message: 'No batches to process',
 *     });
 *   }
 *
 *   // Process the batch using cursor bounds
 *   const { start_cursor, end_cursor } = inputs.cursor;
 *   // ... process items in range
 * }
 * ```
 */
export interface RustBatchWorkerInputs {
  /**
   * Cursor configuration defining this worker's processing range.
   *
   * Created by the batchable handler after analyzing dataset size.
   */
  cursor: RustCursorConfig;

  /**
   * Batch processing metadata from template configuration.
   *
   * Provides checkpointing frequency, cursor field, and failure strategy.
   */
  batch_metadata: BatchMetadata;

  /**
   * Explicit flag indicating if this is a no-op/placeholder worker.
   *
   * Set by orchestration based on BatchProcessingOutcome type:
   * - `true` for NoBatches outcome (placeholder worker)
   * - `false` for CreateBatches outcome (real worker with data)
   *
   * Workers should check this flag FIRST before any processing logic.
   * If `true`, immediately return success without processing.
   */
  is_no_op: boolean;
}

// =============================================================================
// BatchProcessingOutcome - Discriminated Union (TAS-112/TAS-123)
//
// Matches Rust's `BatchProcessingOutcome` enum with tagged serialization.
// Uses TypeScript discriminated unions for type-safe pattern matching.
// =============================================================================

/**
 * No batches needed - process as single step or skip.
 *
 * Returned when:
 * - Dataset is too small to warrant batching
 * - Data doesn't meet batching criteria
 * - Batch processing not applicable for this execution
 *
 * Serialization format: `{ "type": "no_batches" }`
 */
export interface NoBatchesOutcome {
  type: 'no_batches';
}

/**
 * Create batch worker steps from template.
 *
 * The orchestration system will:
 * 1. Instantiate N workers from the template step
 * 2. Assign each worker a unique cursor config
 * 3. Create DAG edges from batchable step to workers
 * 4. Enqueue workers for parallel execution
 *
 * Serialization format:
 * ```json
 * {
 *   "type": "create_batches",
 *   "worker_template_name": "batch_worker_template",
 *   "worker_count": 5,
 *   "cursor_configs": [...],
 *   "total_items": 5000
 * }
 * ```
 */
export interface CreateBatchesOutcome {
  type: 'create_batches';

  /**
   * Template step name to use for creating workers.
   *
   * Must match a step definition in the template with `type: batch_worker`.
   * The system creates multiple instances with generated names like:
   * - `{template_name}_001`
   * - `{template_name}_002`
   */
  worker_template_name: string;

  /**
   * Number of worker instances to create.
   *
   * Typically calculated based on dataset size / batch_size.
   */
  worker_count: number;

  /**
   * Initial cursor positions for each batch.
   *
   * Each worker receives one cursor config that defines its
   * processing boundaries. Length must equal `worker_count`.
   */
  cursor_configs: RustCursorConfig[];

  /**
   * Total items to process across all batches.
   *
   * Used for progress tracking and observability.
   */
  total_items: number;
}

/**
 * Outcome of a batchable step that determines batch worker creation.
 *
 * Matches Rust's `BatchProcessingOutcome` enum in
 * `tasker-shared/src/messaging/execution_types.rs`.
 *
 * This discriminated union enables type-safe pattern matching:
 *
 * @example
 * ```typescript
 * function handleOutcome(outcome: BatchProcessingOutcome): void {
 *   switch (outcome.type) {
 *     case 'no_batches':
 *       console.log('No batches needed');
 *       break;
 *     case 'create_batches':
 *       console.log(`Creating ${outcome.worker_count} workers`);
 *       console.log(`Total items: ${outcome.total_items}`);
 *       break;
 *     default: {
 *       const _exhaustive: never = outcome;
 *       throw new Error(`Unhandled outcome type: ${_exhaustive}`);
 *     }
 *   }
 * }
 * ```
 */
export type BatchProcessingOutcome = NoBatchesOutcome | CreateBatchesOutcome;

// =============================================================================
// Factory Functions for BatchProcessingOutcome
// =============================================================================

/**
 * Create a NoBatches outcome.
 *
 * Use when batching is not needed or applicable.
 *
 * @returns A NoBatchesOutcome object
 */
export function noBatches(): NoBatchesOutcome {
  return { type: 'no_batches' };
}

/**
 * Create a CreateBatches outcome with specified configuration.
 *
 * @param workerTemplateName - Name of the template step to instantiate
 * @param workerCount - Number of workers to create
 * @param cursorConfigs - Cursor configuration for each worker
 * @param totalItems - Total number of items to process
 * @returns A CreateBatchesOutcome object
 *
 * @example
 * ```typescript
 * const outcome = createBatches(
 *   'process_csv_batch',
 *   3,
 *   [
 *     { batch_id: '001', start_cursor: 0, end_cursor: 1000, batch_size: 1000 },
 *     { batch_id: '002', start_cursor: 1000, end_cursor: 2000, batch_size: 1000 },
 *     { batch_id: '003', start_cursor: 2000, end_cursor: 3000, batch_size: 1000 },
 *   ],
 *   3000
 * );
 * ```
 */
export function createBatches(
  workerTemplateName: string,
  workerCount: number,
  cursorConfigs: RustCursorConfig[],
  totalItems: number
): CreateBatchesOutcome {
  if (cursorConfigs.length !== workerCount) {
    throw new Error(
      `cursor_configs length (${cursorConfigs.length}) must equal worker_count (${workerCount})`
    );
  }

  return {
    type: 'create_batches',
    worker_template_name: workerTemplateName,
    worker_count: workerCount,
    cursor_configs: cursorConfigs,
    total_items: totalItems,
  };
}

/**
 * Type guard to check if an outcome is NoBatches.
 */
export function isNoBatches(outcome: BatchProcessingOutcome): outcome is NoBatchesOutcome {
  return outcome.type === 'no_batches';
}

/**
 * Type guard to check if an outcome is CreateBatches.
 */
export function isCreateBatches(outcome: BatchProcessingOutcome): outcome is CreateBatchesOutcome {
  return outcome.type === 'create_batches';
}

/**
 * Result from aggregating multiple batch worker results.
 *
 * Cross-language standard: matches Python's aggregate_worker_results output
 * and Ruby's aggregate_batch_worker_results.
 *
 * TAS-112: Standardized aggregation result structure.
 */
export interface BatchAggregationResult {
  /** Total items processed across all batches */
  total_processed: number;
  /** Total items that succeeded */
  total_succeeded: number;
  /** Total items that failed */
  total_failed: number;
  /** Total items that were skipped */
  total_skipped: number;
  /** Number of batch workers that ran */
  batch_count: number;
  /** Success rate (0.0 to 1.0) */
  success_rate: number;
  /** Collected errors from all batches (limited) */
  errors: Array<Record<string, unknown>>;
  /** Total error count (may exceed errors array length) */
  error_count: number;
}

/**
 * Aggregate results from multiple batch workers.
 *
 * Cross-language standard: matches Python's `Batchable.aggregate_worker_results`
 * and Ruby's `aggregate_batch_worker_results`.
 *
 * @param workerResults - Array of results from batch worker steps
 * @param maxErrors - Maximum number of errors to collect (default: 100)
 * @returns Aggregated summary of all batch processing
 *
 * @example
 * ```typescript
 * // In an aggregator handler
 * const workerResults = Object.values(context.previousResults)
 *   .filter(r => r?.batch_worker);
 * const summary = aggregateBatchResults(workerResults);
 * return this.success(summary);
 * ```
 */
export function aggregateBatchResults(
  workerResults: Array<Record<string, unknown> | null | undefined>,
  maxErrors = 100
): BatchAggregationResult {
  let totalProcessed = 0;
  let totalSucceeded = 0;
  let totalFailed = 0;
  let totalSkipped = 0;
  const allErrors: Array<Record<string, unknown>> = [];
  let batchCount = 0;

  for (const result of workerResults) {
    if (result === null || result === undefined) {
      continue;
    }

    batchCount++;
    totalProcessed += (result.items_processed as number) ?? 0;
    totalSucceeded += (result.items_succeeded as number) ?? 0;
    totalFailed += (result.items_failed as number) ?? 0;
    totalSkipped += (result.items_skipped as number) ?? 0;

    const errors = result.errors as Array<Record<string, unknown>> | undefined;
    if (errors && Array.isArray(errors)) {
      allErrors.push(...errors);
    }
  }

  return {
    total_processed: totalProcessed,
    total_succeeded: totalSucceeded,
    total_failed: totalFailed,
    total_skipped: totalSkipped,
    batch_count: batchCount,
    success_rate: totalProcessed > 0 ? totalSucceeded / totalProcessed : 0,
    errors: allErrors.slice(0, maxErrors),
    error_count: allErrors.length,
  };
}

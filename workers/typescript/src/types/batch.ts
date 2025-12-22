/**
 * Batch processing types for cursor-based batch operations.
 *
 * These types support the analyzer/worker pattern for processing
 * large datasets in parallel batches.
 *
 * Matches Python's batch processing types and Ruby's batch types (TAS-92 aligned).
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
 * Provides information about the specific batch this worker should process.
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
  /** Convenience accessor for start cursor */
  readonly startCursor: number;
  /** Convenience accessor for end cursor */
  readonly endCursor: number;
  /** Convenience accessor for step size */
  readonly stepSize: number;
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
 * @internal
 */
export function createBatchWorkerContext(
  batchData: Record<string, unknown>
): BatchWorkerContext {
  // Handle both nested cursor_config and flat format
  const cursorData = (batchData.cursor_config as Record<string, unknown>) || batchData;
  const cursorConfig: CursorConfig = {
    startCursor: (cursorData.start_cursor as number) ?? 0,
    endCursor: (cursorData.end_cursor as number) ?? 0,
    stepSize: (cursorData.step_size as number) ?? 1,
    metadata: (cursorData.metadata as Record<string, unknown>) ?? {},
  };

  return {
    batchId: (batchData.batch_id as string) ?? '',
    cursorConfig,
    batchIndex: (batchData.batch_index as number) ?? 0,
    totalBatches: (batchData.total_batches as number) ?? 1,
    batchMetadata: (batchData.batch_metadata as Record<string, unknown>) ?? {},
    get startCursor() {
      return this.cursorConfig.startCursor;
    },
    get endCursor() {
      return this.cursorConfig.endCursor;
    },
    get stepSize() {
      return this.cursorConfig.stepSize;
    },
  };
}

/**
 * Type definitions for the tasker-core TypeScript worker.
 *
 * Provides type-safe data models for the tasker-core FFI layer,
 * aligned with Python and Ruby worker implementations (TAS-92).
 *
 * @module types
 */

export type {
  BatchAggregationResult,
  BatchAnalyzerOutcome,
  BatchMetadata,
  BatchProcessingOutcome,
  BatchWorkerContext,
  BatchWorkerOutcome,
  CreateBatchesOutcome,
  CursorConfig,
  FailureStrategy,
  NoBatchesOutcome,
  RustBatchWorkerInputs,
  // FFI Boundary Types (TAS-112/TAS-123)
  RustCursorConfig,
} from './batch';
// Batch processing types (TAS-103)
export {
  aggregateBatchResults,
  createBatches,
  createBatchWorkerContext,
  isCreateBatches,
  isNoBatches,
  // FFI Boundary Type factories (TAS-112/TAS-123)
  noBatches,
} from './batch';
// Error types
export { ErrorType, isStandardErrorType, isTypicallyRetryable } from './error-type';
export type { StepContextParams } from './step-context';
// Step context
export { StepContext } from './step-context';
export type { StepHandlerResultParams } from './step-handler-result';
// Step handler result
export { StepHandlerResult } from './step-handler-result';

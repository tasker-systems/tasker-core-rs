/**
 * Type definitions for the tasker-core TypeScript worker.
 *
 * Provides type-safe data models for the tasker-core FFI layer,
 * aligned with Python and Ruby worker implementations (TAS-92).
 *
 * @module types
 */

// Error types
export { ErrorType, isStandardErrorType, isTypicallyRetryable } from './error-type';

// Step context
export { StepContext } from './step-context';
export type { StepContextParams } from './step-context';

// Step handler result
export { StepHandlerResult } from './step-handler-result';
export type { StepHandlerResultParams } from './step-handler-result';

// Batch processing types (TAS-103)
export { createBatchWorkerContext } from './batch';
export type {
  CursorConfig,
  BatchAnalyzerOutcome,
  BatchWorkerContext,
  BatchWorkerOutcome,
} from './batch';

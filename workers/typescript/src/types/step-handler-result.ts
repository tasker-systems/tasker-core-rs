import { ErrorType } from './error-type';

/**
 * Configuration for a batch worker instance.
 *
 * Used by batchable handlers to define the work partition for each worker.
 */
export interface BatchWorkerConfig {
  /** Unique identifier for this batch */
  batch_id: string;
  /** Starting cursor position (inclusive) */
  cursor_start: number;
  /** Ending cursor position (exclusive) */
  cursor_end: number;
  /** Number of rows/items in this batch */
  row_count: number;
  /** Index of this worker (0-based) */
  worker_index: number;
  /** Total number of workers processing this data */
  total_workers: number;
  /** Optional additional metadata for this batch */
  metadata?: Record<string, unknown>;
}

/**
 * Result type alias for batchable handlers.
 *
 * Batchable handlers return this type which is a StepHandlerResult
 * with batch-specific metadata.
 */
export type BatchableResult = StepHandlerResult;

/**
 * Parameters for constructing a StepHandlerResult.
 */
export interface StepHandlerResultParams {
  success: boolean;
  result?: Record<string, unknown> | null;
  errorMessage?: string | null;
  errorType?: string | null;
  errorCode?: string | null;
  retryable?: boolean;
  metadata?: Record<string, unknown>;
}

/**
 * Result from a step handler execution.
 *
 * Step handlers return this to indicate success or failure,
 * along with any output data or error details.
 *
 * Matches Python's StepHandlerResult and Ruby's StepHandlerCallResult.
 *
 * @example Success case
 * ```typescript
 * return StepHandlerResult.success({ processed: 100 });
 * ```
 *
 * @example Failure case
 * ```typescript
 * return StepHandlerResult.failure(
 *   'Validation failed',
 *   ErrorType.VALIDATION_ERROR,
 *   false
 * );
 * ```
 *
 * @example Failure with error code
 * ```typescript
 * return StepHandlerResult.failure(
 *   'Payment gateway timeout',
 *   ErrorType.TIMEOUT,
 *   true,
 *   { gateway: 'stripe' },
 *   'GATEWAY_TIMEOUT'
 * );
 * ```
 */
export class StepHandlerResult {
  /** Whether the handler executed successfully */
  public readonly success: boolean;

  /** Handler output data (success case) */
  public readonly result: Record<string, unknown> | null;

  /** Error message (failure case) */
  public readonly errorMessage: string | null;

  /** Error type/category for classification */
  public readonly errorType: string | null;

  /** Optional application-specific error code */
  public readonly errorCode: string | null;

  /** Whether the error is retryable */
  public readonly retryable: boolean;

  /** Additional execution metadata */
  public readonly metadata: Record<string, unknown>;

  constructor(params: StepHandlerResultParams) {
    this.success = params.success;
    this.result = params.result ?? null;
    this.errorMessage = params.errorMessage ?? null;
    this.errorType = params.errorType ?? null;
    this.errorCode = params.errorCode ?? null;
    this.retryable = params.retryable ?? true;
    this.metadata = params.metadata ?? {};
  }

  /**
   * Create a successful handler result.
   *
   * This is the primary factory method for creating success results.
   * Aligned with Ruby and Python worker APIs.
   *
   * @param result - The handler output data
   * @param metadata - Optional additional metadata
   * @returns A StepHandlerResult indicating success
   *
   * @example
   * ```typescript
   * return StepHandlerResult.success(
   *   { processed: 100, skipped: 5 }
   * );
   * ```
   */
  static success(
    result: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return new StepHandlerResult({
      success: true,
      result,
      metadata: metadata ?? {},
    });
  }

  /**
   * Create a failure handler result.
   *
   * @param message - Human-readable error message
   * @param errorType - Error type/category for classification. Use ErrorType enum.
   * @param retryable - Whether the error is retryable (default: true)
   * @param metadata - Optional additional metadata
   * @param errorCode - Optional application-specific error code
   * @returns A StepHandlerResult indicating failure
   *
   * @example
   * ```typescript
   * return StepHandlerResult.failure(
   *   'Invalid input format',
   *   ErrorType.VALIDATION_ERROR,
   *   false
   * );
   * ```
   *
   * @example With error code
   * ```typescript
   * return StepHandlerResult.failure(
   *   'Gateway timeout',
   *   ErrorType.TIMEOUT,
   *   true,
   *   { duration_ms: 30000 },
   *   'GATEWAY_TIMEOUT'
   * );
   * ```
   */
  static failure(
    message: string,
    errorType: ErrorType | string = ErrorType.HANDLER_ERROR,
    retryable = true,
    metadata?: Record<string, unknown>,
    errorCode?: string
  ): StepHandlerResult {
    return new StepHandlerResult({
      success: false,
      errorMessage: message,
      // ErrorType enum values are already strings, so this works directly
      errorType: errorType as string,
      errorCode: errorCode ?? null,
      retryable,
      metadata: metadata ?? {},
    });
  }

  /**
   * Check if this result indicates success.
   */
  isSuccess(): boolean {
    return this.success;
  }

  /**
   * Check if this result indicates failure.
   */
  isFailure(): boolean {
    return !this.success;
  }

  /**
   * Convert to JSON for serialization.
   *
   * Uses snake_case keys to match the Rust FFI contract.
   */
  toJSON(): Record<string, unknown> {
    return {
      success: this.success,
      result: this.result,
      error_message: this.errorMessage,
      error_type: this.errorType,
      error_code: this.errorCode,
      retryable: this.retryable,
      metadata: this.metadata,
    };
  }
}

/**
 * Standard error types for cross-language consistency.
 *
 * These values align with Ruby and Python worker implementations
 * and are used by the orchestration layer for retry decisions.
 *
 * @see TAS-92 Cross-Language API Alignment
 */
export enum ErrorType {
  /**
   * Permanent, non-recoverable failure.
   * Examples: invalid input, resource not found, authentication failure.
   */
  PERMANENT_ERROR = 'permanent_error',

  /**
   * Transient failure that may succeed on retry.
   * Examples: network timeout, service unavailable, rate limiting.
   */
  RETRYABLE_ERROR = 'retryable_error',

  /**
   * Input validation failure.
   * Examples: missing required field, invalid format, constraint violation.
   */
  VALIDATION_ERROR = 'validation_error',

  /**
   * Operation timed out.
   * Examples: HTTP request timeout, database query timeout.
   */
  TIMEOUT = 'timeout',

  /**
   * Failure within the step handler itself.
   * Examples: unhandled exception, handler misconfiguration.
   */
  HANDLER_ERROR = 'handler_error',
}

/**
 * Check if an error type is one of the standard values.
 *
 * @param errorType - The error type string to check
 * @returns True if the error type matches one of the standard values
 *
 * @example
 * isStandardErrorType('permanent_error'); // true
 * isStandardErrorType('custom_error');    // false
 */
export function isStandardErrorType(errorType: string): boolean {
  return Object.values(ErrorType).includes(errorType as ErrorType);
}

/**
 * Get the recommended retryable flag for a given error type.
 *
 * @param errorType - The error type string
 * @returns True if the error type is typically retryable
 *
 * @example
 * isTypicallyRetryable('timeout');          // true
 * isTypicallyRetryable('permanent_error');  // false
 */
export function isTypicallyRetryable(errorType: string): boolean {
  return [ErrorType.RETRYABLE_ERROR, ErrorType.TIMEOUT].includes(errorType as ErrorType);
}

/**
 * Structured logging API for TypeScript workers.
 *
 * Provides unified structured logging that integrates with Rust tracing
 * infrastructure via FFI. All log messages are forwarded to the Rust
 * tracing subscriber for consistent formatting and output.
 *
 * Matches Python's logging module and Ruby's tracing module (TAS-92 aligned).
 */

import { getTaskerRuntime } from '../ffi/runtime-factory.js';
import type { LogFields as FfiLogFields } from '../ffi/types.js';

/**
 * Structured logging fields.
 *
 * All fields are optional. Common fields include:
 * - component: Component/subsystem identifier (e.g., "handler", "registry")
 * - operation: Operation being performed (e.g., "process_payment")
 * - correlation_id: Distributed tracing correlation ID
 * - task_uuid: Task identifier
 * - step_uuid: Step identifier
 * - namespace: Task namespace
 * - error_message: Error message for error logs
 * - duration_ms: Execution duration for timed operations
 */
export interface LogFields {
  [key: string]: string | number | boolean | null | undefined;
}

/**
 * Convert LogFields to FFI-compatible format.
 * FFI expects all values as string | number | boolean | null.
 */
function toFfiFields(fields?: LogFields): FfiLogFields {
  if (!fields) {
    return {};
  }

  const result: FfiLogFields = {};
  for (const [key, value] of Object.entries(fields)) {
    if (value !== undefined) {
      result[key] = value as string | number | boolean | null;
    }
  }
  return result;
}

/**
 * Check if the FFI runtime is available and loaded.
 */
function isRuntimeAvailable(): boolean {
  try {
    const runtime = getTaskerRuntime();
    return runtime.isLoaded;
  } catch {
    return false;
  }
}

/**
 * Fallback console logging when FFI is not available.
 */
function fallbackLog(level: string, message: string, fields?: LogFields): void {
  const timestamp = new Date().toISOString();
  const fieldsStr = fields ? ` ${JSON.stringify(fields)}` : '';
  console.log(`[${timestamp}] ${level.toUpperCase()}: ${message}${fieldsStr}`);
}

/**
 * Log an ERROR level message with structured fields.
 *
 * Use this for unrecoverable failures that require intervention.
 *
 * @param message - The log message
 * @param fields - Optional structured fields for context
 *
 * @example
 * logError('Database connection failed', {
 *   component: 'database',
 *   operation: 'connect',
 *   error_message: 'Connection timeout',
 * });
 */
export function logError(message: string, fields?: LogFields): void {
  if (!isRuntimeAvailable()) {
    fallbackLog('error', message, fields);
    return;
  }

  try {
    const runtime = getTaskerRuntime();
    runtime.logError(message, toFfiFields(fields));
  } catch {
    fallbackLog('error', message, fields);
  }
}

/**
 * Log a WARN level message with structured fields.
 *
 * Use this for degraded operation or retryable failures.
 *
 * @param message - The log message
 * @param fields - Optional structured fields for context
 *
 * @example
 * logWarn('Retry attempt 3 of 5', {
 *   component: 'handler',
 *   operation: 'retry',
 *   attempt: 3,
 * });
 */
export function logWarn(message: string, fields?: LogFields): void {
  if (!isRuntimeAvailable()) {
    fallbackLog('warn', message, fields);
    return;
  }

  try {
    const runtime = getTaskerRuntime();
    runtime.logWarn(message, toFfiFields(fields));
  } catch {
    fallbackLog('warn', message, fields);
  }
}

/**
 * Log an INFO level message with structured fields.
 *
 * Use this for lifecycle events and state transitions.
 *
 * @param message - The log message
 * @param fields - Optional structured fields for context
 *
 * @example
 * logInfo('Task processing started', {
 *   component: 'handler',
 *   operation: 'process_payment',
 *   correlation_id: 'abc-123',
 *   task_uuid: 'task-456',
 * });
 */
export function logInfo(message: string, fields?: LogFields): void {
  if (!isRuntimeAvailable()) {
    fallbackLog('info', message, fields);
    return;
  }

  try {
    const runtime = getTaskerRuntime();
    runtime.logInfo(message, toFfiFields(fields));
  } catch {
    fallbackLog('info', message, fields);
  }
}

/**
 * Log a DEBUG level message with structured fields.
 *
 * Use this for detailed diagnostic information during development.
 *
 * @param message - The log message
 * @param fields - Optional structured fields for context
 *
 * @example
 * logDebug('Parsed request payload', {
 *   component: 'handler',
 *   payload_size: 1024,
 *   content_type: 'application/json',
 * });
 */
export function logDebug(message: string, fields?: LogFields): void {
  if (!isRuntimeAvailable()) {
    fallbackLog('debug', message, fields);
    return;
  }

  try {
    const runtime = getTaskerRuntime();
    runtime.logDebug(message, toFfiFields(fields));
  } catch {
    fallbackLog('debug', message, fields);
  }
}

/**
 * Log a TRACE level message with structured fields.
 *
 * Use this for very verbose logging, like function entry/exit.
 * This level is typically disabled in production.
 *
 * @param message - The log message
 * @param fields - Optional structured fields for context
 *
 * @example
 * logTrace('Entering process_step', {
 *   component: 'handler',
 *   step_uuid: 'step-789',
 * });
 */
export function logTrace(message: string, fields?: LogFields): void {
  if (!isRuntimeAvailable()) {
    fallbackLog('trace', message, fields);
    return;
  }

  try {
    const runtime = getTaskerRuntime();
    runtime.logTrace(message, toFfiFields(fields));
  } catch {
    fallbackLog('trace', message, fields);
  }
}

/**
 * Create a logger with preset fields.
 *
 * Useful for creating component-specific loggers that automatically
 * include common fields in every log message.
 *
 * @param defaultFields - Fields to include in every log message
 * @returns Logger object with log methods
 *
 * @example
 * const logger = createLogger({ component: 'payment_handler' });
 * logger.info('Processing payment', { amount: 100 });
 * // Logs: { component: 'payment_handler', amount: 100 }
 */
export function createLogger(defaultFields: LogFields) {
  const mergeFields = (fields?: LogFields): LogFields => ({
    ...defaultFields,
    ...fields,
  });

  return {
    error: (message: string, fields?: LogFields) => logError(message, mergeFields(fields)),
    warn: (message: string, fields?: LogFields) => logWarn(message, mergeFields(fields)),
    info: (message: string, fields?: LogFields) => logInfo(message, mergeFields(fields)),
    debug: (message: string, fields?: LogFields) => logDebug(message, mergeFields(fields)),
    trace: (message: string, fields?: LogFields) => logTrace(message, mergeFields(fields)),
  };
}

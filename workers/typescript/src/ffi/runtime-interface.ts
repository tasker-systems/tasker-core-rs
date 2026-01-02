/**
 * Runtime interface for FFI operations.
 *
 * This interface defines the contract that all runtime adapters must implement.
 * It provides strongly-typed access to the Rust FFI functions.
 */

import type {
  BootstrapConfig,
  BootstrapResult,
  FfiDispatchMetrics,
  FfiDomainEvent,
  FfiStepEvent,
  LogFields,
  StepExecutionResult,
  StopResult,
  WorkerStatus,
} from './types.js';

/**
 * Interface for runtime-specific FFI implementations.
 *
 * Each runtime (Node.js, Bun, Deno) implements this interface
 * using their native FFI mechanism.
 */
export interface TaskerRuntime {
  /**
   * Get the runtime name
   */
  readonly name: string;

  /**
   * Check if the FFI library is loaded
   */
  readonly isLoaded: boolean;

  /**
   * Load the native library from the given path
   */
  load(libraryPath: string): Promise<void>;

  /**
   * Unload the native library and release resources
   */
  unload(): void;

  // ============================================================================
  // Version and Health
  // ============================================================================

  /**
   * Get the version of the tasker-worker-ts package
   */
  getVersion(): string;

  /**
   * Get detailed Rust library version
   */
  getRustVersion(): string;

  /**
   * Check if the FFI module is functional
   */
  healthCheck(): boolean;

  // ============================================================================
  // Worker Lifecycle
  // ============================================================================

  /**
   * Bootstrap the worker with optional configuration
   */
  bootstrapWorker(config?: BootstrapConfig): BootstrapResult;

  /**
   * Check if the worker is currently running
   */
  isWorkerRunning(): boolean;

  /**
   * Get current worker status
   */
  getWorkerStatus(): WorkerStatus;

  /**
   * Stop the worker gracefully
   */
  stopWorker(): StopResult;

  /**
   * Transition to graceful shutdown mode
   */
  transitionToGracefulShutdown(): StopResult;

  // ============================================================================
  // Event Polling
  // ============================================================================

  /**
   * Poll for pending step events (non-blocking)
   *
   * @returns Step event if available, null otherwise
   */
  pollStepEvents(): FfiStepEvent | null;

  /**
   * Poll for in-process domain events (fast path, non-blocking)
   *
   * Used for real-time notifications that don't require guaranteed delivery
   * (e.g., metrics updates, logging, notifications).
   *
   * @returns Domain event if available, null otherwise
   */
  pollInProcessEvents(): FfiDomainEvent | null;

  /**
   * Complete a step event with the given result
   *
   * @param eventId The event ID to complete
   * @param result The step execution result
   * @returns true if successful, false otherwise
   */
  completeStepEvent(eventId: string, result: StepExecutionResult): boolean;

  // ============================================================================
  // Metrics and Monitoring
  // ============================================================================

  /**
   * Get FFI dispatch metrics
   */
  getFfiDispatchMetrics(): FfiDispatchMetrics;

  /**
   * Check for and log starvation warnings
   */
  checkStarvationWarnings(): void;

  /**
   * Cleanup timed-out events
   */
  cleanupTimeouts(): void;

  // ============================================================================
  // Logging
  // ============================================================================

  /**
   * Log an error message
   */
  logError(message: string, fields?: LogFields): void;

  /**
   * Log a warning message
   */
  logWarn(message: string, fields?: LogFields): void;

  /**
   * Log an info message
   */
  logInfo(message: string, fields?: LogFields): void;

  /**
   * Log a debug message
   */
  logDebug(message: string, fields?: LogFields): void;

  /**
   * Log a trace message
   */
  logTrace(message: string, fields?: LogFields): void;
}

/**
 * Base class with common functionality for all runtime implementations.
 *
 * Runtime-specific implementations extend this class and implement
 * the abstract methods using their native FFI mechanism.
 */
export abstract class BaseTaskerRuntime implements TaskerRuntime {
  abstract readonly name: string;
  abstract readonly isLoaded: boolean;

  abstract load(libraryPath: string): Promise<void>;
  abstract unload(): void;

  abstract getVersion(): string;
  abstract getRustVersion(): string;
  abstract healthCheck(): boolean;

  abstract bootstrapWorker(config?: BootstrapConfig): BootstrapResult;
  abstract isWorkerRunning(): boolean;
  abstract getWorkerStatus(): WorkerStatus;
  abstract stopWorker(): StopResult;
  abstract transitionToGracefulShutdown(): StopResult;

  abstract pollStepEvents(): FfiStepEvent | null;
  abstract pollInProcessEvents(): FfiDomainEvent | null;
  abstract completeStepEvent(eventId: string, result: StepExecutionResult): boolean;

  abstract getFfiDispatchMetrics(): FfiDispatchMetrics;
  abstract checkStarvationWarnings(): void;
  abstract cleanupTimeouts(): void;

  abstract logError(message: string, fields?: LogFields): void;
  abstract logWarn(message: string, fields?: LogFields): void;
  abstract logInfo(message: string, fields?: LogFields): void;
  abstract logDebug(message: string, fields?: LogFields): void;
  abstract logTrace(message: string, fields?: LogFields): void;

  /**
   * Helper to parse JSON string from FFI
   */
  protected parseJson<T>(jsonStr: string | null): T | null {
    if (jsonStr === null || jsonStr === '') {
      return null;
    }
    try {
      return JSON.parse(jsonStr) as T;
    } catch {
      return null;
    }
  }

  /**
   * Helper to stringify JSON for FFI
   */
  protected toJson(value: unknown): string {
    return JSON.stringify(value);
  }
}

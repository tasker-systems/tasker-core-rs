/**
 * Bootstrap API for TypeScript workers.
 *
 * High-level TypeScript API for worker lifecycle management.
 * Wraps FFI calls with type-safe interfaces and error handling.
 *
 * Matches Python's bootstrap.py and Ruby's bootstrap.rb (TAS-92 aligned).
 */

import { getCachedRuntime, getTaskerRuntime } from '../ffi/runtime-factory.js';
import type { BootstrapConfig, BootstrapResult, StopResult, WorkerStatus } from './types.js';
import {
  fromFfiBootstrapResult,
  fromFfiStopResult,
  fromFfiWorkerStatus,
  toFfiBootstrapConfig,
} from './types.js';

/**
 * Initialize the worker system.
 *
 * This function bootstraps the full tasker-worker system, including:
 * - Creating a Tokio runtime for async operations
 * - Connecting to the database
 * - Setting up the FFI dispatch channel for step events
 * - Subscribing to domain events
 *
 * @param config - Optional bootstrap configuration
 * @returns BootstrapResult with worker details and status
 * @throws Error if bootstrap fails critically
 *
 * @example Bootstrap with defaults
 * ```typescript
 * const result = await bootstrapWorker();
 * console.log(`Worker ${result.workerId} started`);
 * ```
 *
 * @example Bootstrap with custom config
 * ```typescript
 * const result = await bootstrapWorker({
 *   namespace: 'payments',
 *   logLevel: 'debug',
 * });
 * ```
 */
export async function bootstrapWorker(config?: BootstrapConfig): Promise<BootstrapResult> {
  try {
    const runtime = await getTaskerRuntime();

    if (!runtime.isLoaded) {
      return {
        success: false,
        status: 'error',
        message: 'Runtime not loaded. Ensure the FFI library is available.',
        error: 'Runtime not loaded',
      };
    }

    const ffiConfig = toFfiBootstrapConfig(config);
    const ffiResult = runtime.bootstrapWorker(ffiConfig);
    return fromFfiBootstrapResult(ffiResult);
  } catch (error) {
    return {
      success: false,
      status: 'error',
      message: `Bootstrap failed: ${error instanceof Error ? error.message : String(error)}`,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * Stop the worker system gracefully.
 *
 * This function stops the worker system and releases all resources.
 * Safe to call even if the worker is not running.
 *
 * @returns StopResult indicating the outcome
 *
 * @example
 * ```typescript
 * const result = stopWorker();
 * if (result.success) {
 *   console.log('Worker stopped successfully');
 * }
 * ```
 */
export function stopWorker(): StopResult {
  const runtime = getCachedRuntime();

  if (!runtime?.isLoaded) {
    return {
      success: true,
      status: 'not_running',
      message: 'Runtime not loaded',
    };
  }

  try {
    const ffiResult = runtime.stopWorker();
    return fromFfiStopResult(ffiResult);
  } catch (error) {
    return {
      success: false,
      status: 'error',
      message: `Stop failed: ${error instanceof Error ? error.message : String(error)}`,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * Get the current worker system status.
 *
 * Returns detailed information about the worker's current state,
 * including resource usage and operational status.
 *
 * @returns WorkerStatus with current state and metrics
 *
 * @example
 * ```typescript
 * const status = getWorkerStatus();
 * if (status.running) {
 *   console.log(`Pool size: ${status.databasePoolSize}`);
 * } else {
 *   console.log(`Worker not running`);
 * }
 * ```
 */
export function getWorkerStatus(): WorkerStatus {
  const runtime = getCachedRuntime();

  if (!runtime?.isLoaded) {
    return {
      success: false,
      running: false,
      status: 'stopped',
    };
  }

  try {
    const ffiStatus = runtime.getWorkerStatus();
    return fromFfiWorkerStatus(ffiStatus);
  } catch (_error) {
    return {
      success: false,
      running: false,
      status: 'stopped',
    };
  }
}

/**
 * Initiate graceful shutdown of the worker system.
 *
 * This function begins the graceful shutdown process, allowing
 * in-flight operations to complete before fully stopping.
 * Call stopWorker() after this to fully stop the worker.
 *
 * @returns StopResult indicating the transition status
 *
 * @example
 * ```typescript
 * // Start graceful shutdown
 * transitionToGracefulShutdown();
 *
 * // Wait for in-flight operations...
 * await new Promise(resolve => setTimeout(resolve, 5000));
 *
 * // Fully stop
 * stopWorker();
 * ```
 */
export function transitionToGracefulShutdown(): StopResult {
  const runtime = getCachedRuntime();

  if (!runtime?.isLoaded) {
    return {
      success: true,
      status: 'not_running',
      message: 'Runtime not loaded',
    };
  }

  try {
    const ffiResult = runtime.transitionToGracefulShutdown();
    return fromFfiStopResult(ffiResult);
  } catch (error) {
    return {
      success: false,
      status: 'error',
      message: `Graceful shutdown failed: ${error instanceof Error ? error.message : String(error)}`,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * Check if the worker system is currently running.
 *
 * Lightweight check that doesn't query the full status.
 *
 * @returns True if the worker is running
 *
 * @example
 * ```typescript
 * if (!isWorkerRunning()) {
 *   await bootstrapWorker();
 * }
 * ```
 */
export function isWorkerRunning(): boolean {
  const runtime = getCachedRuntime();

  if (!runtime?.isLoaded) {
    return false;
  }

  try {
    return runtime.isWorkerRunning();
  } catch {
    return false;
  }
}

/**
 * Get version information for the worker system.
 *
 * @returns Version string from the Rust library
 */
export function getVersion(): string {
  const runtime = getCachedRuntime();

  if (!runtime?.isLoaded) {
    return 'unknown (runtime not loaded)';
  }

  try {
    return runtime.getVersion();
  } catch {
    return 'unknown';
  }
}

/**
 * Get detailed Rust library version.
 *
 * @returns Detailed version information
 */
export function getRustVersion(): string {
  const runtime = getCachedRuntime();

  if (!runtime?.isLoaded) {
    return 'unknown (runtime not loaded)';
  }

  try {
    return runtime.getRustVersion();
  } catch {
    return 'unknown';
  }
}

/**
 * Perform a health check on the FFI module.
 *
 * @returns True if the FFI module is functional
 */
export function healthCheck(): boolean {
  const runtime = getCachedRuntime();

  if (!runtime?.isLoaded) {
    return false;
  }

  try {
    return runtime.healthCheck();
  } catch {
    return false;
  }
}

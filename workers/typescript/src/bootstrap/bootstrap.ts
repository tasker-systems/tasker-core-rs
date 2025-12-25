/**
 * Bootstrap API for TypeScript workers.
 *
 * High-level TypeScript API for worker lifecycle management.
 * Wraps FFI calls with type-safe interfaces and error handling.
 *
 * Matches Python's bootstrap.py and Ruby's bootstrap.rb (TAS-92 aligned).
 *
 * All functions require an explicit runtime parameter. Use FfiLayer to load
 * the runtime before calling these functions.
 */

import type { TaskerRuntime } from '../ffi/runtime-interface.js';
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
 * @param runtime - The loaded FFI runtime (required)
 * @returns BootstrapResult with worker details and status
 * @throws Error if bootstrap fails critically
 *
 * @example
 * ```typescript
 * const ffiLayer = new FfiLayer();
 * await ffiLayer.load();
 * const result = await bootstrapWorker({ namespace: 'payments' }, ffiLayer.getRuntime());
 * console.log(`Worker ${result.workerId} started`);
 * ```
 */
export async function bootstrapWorker(
  config: BootstrapConfig | undefined,
  runtime: TaskerRuntime
): Promise<BootstrapResult> {
  try {
    if (!runtime?.isLoaded) {
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
 * @param runtime - The loaded FFI runtime (optional - returns success if not loaded)
 * @returns StopResult indicating the outcome
 *
 * @example
 * ```typescript
 * const result = stopWorker(runtime);
 * if (result.success) {
 *   console.log('Worker stopped successfully');
 * }
 * ```
 */
export function stopWorker(runtime?: TaskerRuntime): StopResult {
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
 * @param runtime - The loaded FFI runtime (optional - returns stopped if not loaded)
 * @returns WorkerStatus with current state and metrics
 *
 * @example
 * ```typescript
 * const status = getWorkerStatus(runtime);
 * if (status.running) {
 *   console.log(`Pool size: ${status.databasePoolSize}`);
 * } else {
 *   console.log(`Worker not running`);
 * }
 * ```
 */
export function getWorkerStatus(runtime?: TaskerRuntime): WorkerStatus {
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
 * @param runtime - The loaded FFI runtime (optional - returns success if not loaded)
 * @returns StopResult indicating the transition status
 *
 * @example
 * ```typescript
 * // Start graceful shutdown
 * transitionToGracefulShutdown(runtime);
 *
 * // Wait for in-flight operations...
 * await new Promise(resolve => setTimeout(resolve, 5000));
 *
 * // Fully stop
 * stopWorker(runtime);
 * ```
 */
export function transitionToGracefulShutdown(runtime?: TaskerRuntime): StopResult {
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
 * @param runtime - The loaded FFI runtime (optional - returns false if not loaded)
 * @returns True if the worker is running
 *
 * @example
 * ```typescript
 * if (!isWorkerRunning(runtime)) {
 *   await bootstrapWorker(config, runtime);
 * }
 * ```
 */
export function isWorkerRunning(runtime?: TaskerRuntime): boolean {
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
 * @param runtime - The loaded FFI runtime (optional)
 * @returns Version string from the Rust library
 */
export function getVersion(runtime?: TaskerRuntime): string {
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
 * @param runtime - The loaded FFI runtime (optional)
 * @returns Detailed version information
 */
export function getRustVersion(runtime?: TaskerRuntime): string {
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
 * @param runtime - The loaded FFI runtime (optional - returns false if not loaded)
 * @returns True if the FFI module is functional
 */
export function healthCheck(runtime?: TaskerRuntime): boolean {
  if (!runtime?.isLoaded) {
    return false;
  }

  try {
    return runtime.healthCheck();
  } catch {
    return false;
  }
}

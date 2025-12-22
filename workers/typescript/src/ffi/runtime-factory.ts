/**
 * Runtime factory for TypeScript/JavaScript workers.
 *
 * Automatically detects the current runtime and returns the appropriate
 * FFI adapter. Uses a singleton pattern for efficiency.
 */

import { detectRuntime, type RuntimeType } from './runtime.js';
import type { TaskerRuntime } from './runtime-interface.js';

/**
 * Cached runtime instance (singleton)
 */
let cachedRuntime: TaskerRuntime | null = null;

/**
 * Get the TaskerRuntime for the current environment.
 *
 * This function detects the runtime (Bun, Node.js, or Deno) and returns
 * the appropriate FFI adapter. The adapter is cached for subsequent calls.
 *
 * @returns The appropriate TaskerRuntime for the current environment
 * @throws Error if running in an unsupported runtime
 */
export async function getTaskerRuntime(): Promise<TaskerRuntime> {
  if (cachedRuntime !== null) {
    return cachedRuntime;
  }

  const runtimeType = detectRuntime();
  cachedRuntime = await createRuntime(runtimeType);
  return cachedRuntime;
}

/**
 * Create a runtime adapter for a specific runtime type.
 *
 * @param runtimeType The runtime type to create an adapter for
 * @returns The appropriate TaskerRuntime
 * @throws Error if the runtime type is not supported
 */
export async function createRuntime(runtimeType: RuntimeType): Promise<TaskerRuntime> {
  switch (runtimeType) {
    case 'bun': {
      const { BunRuntime } = await import('./bun-runtime.js');
      return new BunRuntime();
    }
    case 'node': {
      const { NodeRuntime } = await import('./node-runtime.js');
      return new NodeRuntime();
    }
    case 'deno': {
      const { DenoRuntime } = await import('./deno-runtime.js');
      return new DenoRuntime();
    }
    default:
      throw new Error(
        `Unsupported runtime: ${runtimeType}. Tasker TypeScript worker requires Bun, Node.js, or Deno.`
      );
  }
}

/**
 * Clear the cached runtime instance.
 *
 * Useful for testing or when switching runtimes dynamically.
 */
export function clearRuntimeCache(): void {
  if (cachedRuntime?.isLoaded) {
    cachedRuntime.unload();
  }
  cachedRuntime = null;
}

/**
 * Check if a runtime is already cached.
 */
export function hasRuntimeCached(): boolean {
  return cachedRuntime !== null;
}

/**
 * Get the currently cached runtime without creating one.
 *
 * @returns The cached runtime or null if none is cached
 */
export function getCachedRuntime(): TaskerRuntime | null {
  return cachedRuntime;
}

/**
 * FfiLayer - Owns FFI runtime loading and lifecycle.
 *
 * This class encapsulates the FFI runtime management:
 * - Runtime detection (Bun, Node.js, Deno)
 * - Library path discovery
 * - Runtime loading and unloading
 *
 * Design principles:
 * - Explicit construction: No singleton pattern
 * - Clear ownership: Owns the runtime instance
 * - Explicit lifecycle: load() and unload() methods
 */

import { existsSync } from 'node:fs';
import { detectRuntime, type RuntimeType } from './runtime.js';
import type { TaskerRuntime } from './runtime-interface.js';

/**
 * Configuration for FfiLayer.
 */
export interface FfiLayerConfig {
  /** Override runtime detection */
  runtimeType?: RuntimeType;

  /** Custom library path (overrides discovery) */
  libraryPath?: string;
}

/**
 * Owns FFI runtime loading and lifecycle.
 *
 * Unlike RuntimeFactory, this class:
 * - Is NOT a singleton - created and passed explicitly
 * - Owns the runtime instance directly
 * - Has clear load/unload lifecycle
 *
 * @example
 * ```typescript
 * const ffiLayer = new FfiLayer();
 * await ffiLayer.load();
 * const runtime = ffiLayer.getRuntime();
 * // ... use runtime ...
 * await ffiLayer.unload();
 * ```
 */
export class FfiLayer {
  private runtime: TaskerRuntime | null = null;
  private libraryPath: string | null = null;
  private readonly runtimeType: RuntimeType;
  private readonly configuredLibraryPath: string | undefined;

  /**
   * Create a new FfiLayer.
   *
   * @param config - Optional configuration for runtime type and library path
   */
  constructor(config: FfiLayerConfig = {}) {
    this.runtimeType = config.runtimeType ?? detectRuntime();
    this.configuredLibraryPath = config.libraryPath;
  }

  /**
   * Load the FFI library.
   *
   * Discovers and loads the native library for the current runtime.
   *
   * @param customPath - Optional override for library path (takes precedence over config)
   * @throws Error if library not found or failed to load
   */
  async load(customPath?: string): Promise<void> {
    if (this.runtime?.isLoaded) {
      return; // Already loaded
    }

    const path = customPath ?? this.configuredLibraryPath ?? this.discoverLibraryPath();

    if (!path) {
      throw new Error(
        'FFI library not found. TASKER_FFI_LIBRARY_PATH environment variable must be set to the path of the compiled library.\n' +
          'Example: export TASKER_FFI_LIBRARY_PATH=/path/to/target/debug/libtasker_worker.dylib\n' +
          'Build the library with: cargo build -p tasker-worker-ts'
      );
    }

    this.runtime = await this.createRuntime();
    await this.runtime.load(path);
    this.libraryPath = path;
  }

  /**
   * Unload the FFI library and release resources.
   *
   * Safe to call even if not loaded.
   */
  async unload(): Promise<void> {
    if (this.runtime?.isLoaded) {
      this.runtime.unload();
    }
    this.runtime = null;
    this.libraryPath = null;
  }

  /**
   * Check if the FFI library is loaded.
   */
  isLoaded(): boolean {
    return this.runtime?.isLoaded ?? false;
  }

  /**
   * Get the loaded runtime.
   *
   * @throws Error if runtime is not loaded
   */
  getRuntime(): TaskerRuntime {
    if (!this.runtime?.isLoaded) {
      throw new Error('FFI not loaded. Call load() first.');
    }
    return this.runtime;
  }

  /**
   * Get the path to the loaded library.
   */
  getLibraryPath(): string | null {
    return this.libraryPath;
  }

  /**
   * Get the detected runtime type.
   */
  getRuntimeType(): RuntimeType {
    return this.runtimeType;
  }

  /**
   * Find the FFI library path.
   *
   * Static method for finding the library path without creating an instance.
   * Useful for test utilities and pre-flight checks.
   *
   * REQUIRES: TASKER_FFI_LIBRARY_PATH environment variable to be set.
   * This explicit requirement prevents confusion from automatic debug/release
   * library discovery and ensures intentional configuration at build/runtime.
   *
   * @param _callerDir Deprecated parameter, kept for API compatibility
   * @returns Path to the library if found and exists, null otherwise
   */
  static findLibraryPath(_callerDir?: string): string | null {
    const envPath = process.env.TASKER_FFI_LIBRARY_PATH;

    if (!envPath) {
      return null;
    }

    if (!existsSync(envPath)) {
      console.warn(`TASKER_FFI_LIBRARY_PATH is set to "${envPath}" but the file does not exist`);
      return null;
    }

    return envPath;
  }

  /**
   * Discover the FFI library path.
   *
   * Instance method that delegates to the static findLibraryPath.
   */
  private discoverLibraryPath(): string | null {
    return FfiLayer.findLibraryPath();
  }

  /**
   * Create a runtime adapter for the configured runtime type.
   *
   * NOTE: We use koffi (NodeRuntime) for both Node.js and Bun because:
   * - bun:ffi is experimental with known bugs (per Bun docs)
   * - koffi is stable and works with both Node.js and Bun via Node-API
   * - See: https://bun.sh/docs/runtime/node-api
   */
  private async createRuntime(): Promise<TaskerRuntime> {
    switch (this.runtimeType) {
      case 'bun':
      case 'node': {
        // Use koffi-based NodeRuntime for both Bun and Node.js
        // koffi is stable and Bun supports Node-API modules
        const { NodeRuntime } = await import('./node-runtime.js');
        return new NodeRuntime();
      }
      case 'deno': {
        const { DenoRuntime } = await import('./deno-runtime.js');
        return new DenoRuntime();
      }
      default:
        throw new Error(
          `Unsupported runtime: ${this.runtimeType}. Tasker TypeScript worker requires Bun, Node.js, or Deno.`
        );
    }
  }
}

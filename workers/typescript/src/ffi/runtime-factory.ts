/**
 * Runtime factory for TypeScript/JavaScript workers.
 *
 * Provides a singleton factory for creating and managing TaskerRuntime instances.
 * Handles library discovery, runtime detection, and caching.
 */

import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { detectRuntime, type RuntimeType } from './runtime.js';
import type { TaskerRuntime } from './runtime-interface.js';

/**
 * Factory for creating and managing TaskerRuntime instances.
 *
 * Uses singleton pattern for process-level runtime management.
 * Handles library discovery, runtime detection, instantiation, and caching.
 *
 * @example
 * ```typescript
 * const factory = RuntimeFactory.instance();
 * await factory.loadLibrary();
 * const runtime = factory.getCachedRuntime();
 * ```
 */
export class RuntimeFactory {
  private static _instance: RuntimeFactory | null = null;
  private _runtime: TaskerRuntime | null = null;
  private _libraryPath: string | null = null;
  private _runtimeType: RuntimeType | null = null;

  private constructor() {
    // Private constructor for singleton pattern
  }

  /**
   * Get the singleton RuntimeFactory instance.
   */
  static instance(): RuntimeFactory {
    if (RuntimeFactory._instance === null) {
      RuntimeFactory._instance = new RuntimeFactory();
    }
    return RuntimeFactory._instance;
  }

  /**
   * Reset the singleton instance.
   *
   * Useful for testing. Unloads any cached runtime before resetting.
   */
  static resetInstance(): void {
    if (RuntimeFactory._instance !== null) {
      RuntimeFactory._instance.unload();
      RuntimeFactory._instance = null;
    }
  }

  /**
   * Get or create the TaskerRuntime for the current environment.
   *
   * This method detects the runtime (Bun, Node.js, or Deno) and returns
   * the appropriate FFI adapter. The adapter is cached for subsequent calls.
   *
   * @returns The appropriate TaskerRuntime for the current environment
   * @throws Error if running in an unsupported runtime
   */
  async getRuntime(): Promise<TaskerRuntime> {
    if (this._runtime !== null) {
      return this._runtime;
    }

    this._runtimeType = detectRuntime();
    this._runtime = await this.createRuntimeAdapter(this._runtimeType);
    return this._runtime;
  }

  /**
   * Get the currently cached runtime without creating one.
   *
   * @returns The cached runtime or null if none is cached
   */
  getCachedRuntime(): TaskerRuntime | null {
    return this._runtime;
  }

  /**
   * Check if a runtime is cached and loaded.
   */
  isLoaded(): boolean {
    return this._runtime?.isLoaded ?? false;
  }

  /**
   * Get the loaded runtime, throwing if not loaded.
   *
   * Use this when you've already checked isLoaded() and need a non-null runtime.
   *
   * @throws Error if runtime is not loaded
   */
  getLoadedRuntime(): TaskerRuntime {
    if (!this._runtime?.isLoaded) {
      throw new Error('Runtime not loaded. Call loadLibrary() first.');
    }
    return this._runtime;
  }

  /**
   * Get the library path used for loading.
   *
   * @returns The path to the loaded library, or null if not loaded
   */
  getLibraryPath(): string | null {
    return this._libraryPath;
  }

  /**
   * Get the detected runtime type.
   *
   * @returns The runtime type (bun, node, deno) or null if not detected yet
   */
  getRuntimeType(): RuntimeType | null {
    return this._runtimeType;
  }

  /**
   * Find and load the FFI library.
   *
   * This method:
   * 1. Finds the library path (using customPath or auto-discovery)
   * 2. Creates the appropriate runtime adapter
   * 3. Loads the library into the runtime
   *
   * @param customPath Optional custom path to the FFI library
   * @returns The path to the loaded library
   * @throws Error if library not found or failed to load
   */
  async loadLibrary(customPath?: string): Promise<string> {
    const libraryPath = customPath ?? RuntimeFactory.findLibraryPath();

    if (!libraryPath) {
      throw new Error(
        'FFI library not found. Set TASKER_FFI_LIBRARY_PATH or build with: cargo build -p tasker-worker-ts --release'
      );
    }

    const runtime = await this.getRuntime();
    await runtime.load(libraryPath);
    this._libraryPath = libraryPath;

    return libraryPath;
  }

  /**
   * Unload the runtime and clear the cache.
   *
   * Safe to call even if no runtime is loaded.
   */
  unload(): void {
    if (this._runtime?.isLoaded) {
      this._runtime.unload();
    }
    this._runtime = null;
    this._libraryPath = null;
    this._runtimeType = null;
  }

  /**
   * Find the FFI library path.
   *
   * Searches for the native library in the following order:
   * 1. TASKER_FFI_LIBRARY_PATH environment variable (if set and file exists)
   * 2. CARGO_TARGET_DIR environment variable (release, then debug)
   * 3. Relative to caller's directory: ../../target/release/
   * 4. Relative to caller's directory: ../../target/debug/
   * 5. Relative to cwd: ../../target/release/
   * 6. Relative to cwd: ../../target/debug/
   *
   * @param callerDir Optional directory to search relative to (defaults to cwd)
   * @returns Path to the library if found, null otherwise
   */
  static findLibraryPath(callerDir?: string): string | null {
    // Environment variable takes precedence
    const envPath = process.env.TASKER_FFI_LIBRARY_PATH;
    if (envPath && existsSync(envPath)) {
      return envPath;
    }

    const libName =
      process.platform === 'darwin'
        ? 'libtasker_worker.dylib'
        : process.platform === 'win32'
          ? 'tasker_worker.dll'
          : 'libtasker_worker.so';

    const searchPaths: string[] = [];

    // Check CARGO_TARGET_DIR if set (custom target directory)
    const cargoTargetDir = process.env.CARGO_TARGET_DIR;
    if (cargoTargetDir) {
      searchPaths.push(join(cargoTargetDir, 'release', libName));
      searchPaths.push(join(cargoTargetDir, 'debug', libName));
    }

    // Build search paths relative to caller/cwd
    const baseDir = callerDir ?? process.cwd();
    searchPaths.push(
      // Relative to caller directory
      join(baseDir, '..', '..', 'target', 'release', libName),
      join(baseDir, '..', '..', 'target', 'debug', libName)
    );

    // Relative to cwd (if different from callerDir)
    if (callerDir) {
      searchPaths.push(
        join(process.cwd(), '..', '..', 'target', 'release', libName),
        join(process.cwd(), '..', '..', 'target', 'debug', libName)
      );
    }

    for (const path of searchPaths) {
      if (existsSync(path)) {
        return path;
      }
    }

    return null;
  }

  /**
   * Create a runtime adapter for a specific runtime type.
   *
   * @param runtimeType The runtime type to create an adapter for
   * @returns The appropriate TaskerRuntime
   * @throws Error if the runtime type is not supported
   */
  private async createRuntimeAdapter(runtimeType: RuntimeType): Promise<TaskerRuntime> {
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
}

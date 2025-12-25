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
import { join } from 'node:path';
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
        'FFI library not found. Set TASKER_FFI_LIBRARY_PATH or build with: cargo build -p tasker-worker-ts --release'
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
   * Searches for the native library in the following order:
   * 1. TASKER_FFI_LIBRARY_PATH environment variable (if set and file exists)
   * 2. CARGO_TARGET_DIR environment variable (release, then debug)
   * 3. Relative to cwd: ../../target/release/
   * 4. Relative to cwd: ../../target/debug/
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
      join(baseDir, '..', '..', 'target', 'release', libName),
      join(baseDir, '..', '..', 'target', 'debug', libName),
      join(baseDir, 'target', 'release', libName),
      join(baseDir, 'target', 'debug', libName)
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
   * Discover the FFI library path.
   *
   * Instance method that delegates to the static findLibraryPath.
   */
  private discoverLibraryPath(): string | null {
    return FfiLayer.findLibraryPath();
  }

  /**
   * Create a runtime adapter for the configured runtime type.
   */
  private async createRuntime(): Promise<TaskerRuntime> {
    switch (this.runtimeType) {
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
          `Unsupported runtime: ${this.runtimeType}. Tasker TypeScript worker requires Bun, Node.js, or Deno.`
        );
    }
  }
}

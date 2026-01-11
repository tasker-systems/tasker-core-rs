/**
 * Runtime detection for TypeScript/JavaScript workers.
 *
 * Detects whether the code is running in Bun, Node.js, Deno, or an unknown runtime.
 */

/**
 * Supported JavaScript/TypeScript runtimes
 */
export type RuntimeType = 'bun' | 'node' | 'deno' | 'unknown';

/**
 * Runtime information including version details
 */
export interface RuntimeInfo {
  type: RuntimeType;
  version: string;
  platform: string;
  arch: string;
}

/**
 * Cached runtime type for performance
 */
let cachedRuntimeType: RuntimeType | null = null;

/**
 * Detect the current JavaScript/TypeScript runtime.
 *
 * @returns The detected runtime type
 */
export function detectRuntime(): RuntimeType {
  if (cachedRuntimeType !== null) {
    return cachedRuntimeType;
  }

  // Check for Bun
  if (typeof globalThis !== 'undefined' && 'Bun' in globalThis) {
    cachedRuntimeType = 'bun';
    return 'bun';
  }

  // Check for Deno
  if (typeof globalThis !== 'undefined' && 'Deno' in globalThis) {
    cachedRuntimeType = 'deno';
    return 'deno';
  }

  // Check for Node.js
  if (typeof process !== 'undefined' && process.versions && process.versions.node) {
    cachedRuntimeType = 'node';
    return 'node';
  }

  cachedRuntimeType = 'unknown';
  return 'unknown';
}

/**
 * Check if running in Bun
 */
export function isBun(): boolean {
  return detectRuntime() === 'bun';
}

/**
 * Check if running in Node.js
 */
export function isNode(): boolean {
  return detectRuntime() === 'node';
}

/**
 * Check if running in Deno
 */
export function isDeno(): boolean {
  return detectRuntime() === 'deno';
}

/**
 * Get detailed runtime information
 */
export function getRuntimeInfo(): RuntimeInfo {
  const type = detectRuntime();

  switch (type) {
    case 'bun': {
      // biome-ignore lint/suspicious/noExplicitAny: Bun global is runtime-specific
      const Bun = (globalThis as any).Bun;
      return {
        type: 'bun',
        version: Bun?.version ?? 'unknown',
        platform: process?.platform ?? 'unknown',
        arch: process?.arch ?? 'unknown',
      };
    }
    case 'deno': {
      // biome-ignore lint/suspicious/noExplicitAny: Deno global is runtime-specific
      const Deno = (globalThis as any).Deno;
      return {
        type: 'deno',
        version: Deno?.version?.deno ?? 'unknown',
        platform: Deno?.build?.os ?? 'unknown',
        arch: Deno?.build?.arch ?? 'unknown',
      };
    }
    case 'node':
      return {
        type: 'node',
        version: process.versions.node,
        platform: process.platform,
        arch: process.arch,
      };
    default:
      return {
        type: 'unknown',
        version: 'unknown',
        platform: 'unknown',
        arch: 'unknown',
      };
  }
}

/**
 * Get the path to the native library based on runtime and platform
 *
 * @param basePath Optional base path to the library directory
 * @returns Path to the native library
 */
export function getLibraryPath(basePath?: string): string {
  const base = basePath ?? process?.cwd?.() ?? '.';
  const platform = process?.platform ?? 'unknown';

  // Library naming conventions by platform
  const libName = (() => {
    switch (platform) {
      case 'darwin':
        return 'libtasker_worker.dylib';
      case 'linux':
        return 'libtasker_worker.so';
      case 'win32':
        return 'tasker_worker.dll';
      default:
        throw new Error(`Unsupported platform: ${platform}`);
    }
  })();

  // Default to debug build path (matches sccache config for CI)
  return `${base}/target/debug/${libName}`;
}

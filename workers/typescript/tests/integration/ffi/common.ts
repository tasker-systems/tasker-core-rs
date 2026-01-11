/**
 * Common utilities for FFI integration tests.
 *
 * Provides shared helper functions for testing TaskerRuntime implementations
 * against the actual FFI library.
 */

import { FfiLayer } from '../../../src/ffi/ffi-layer.ts';

/**
 * Find the FFI library path for testing.
 *
 * Re-exports FfiLayer.findLibraryPath() for test convenience.
 * See FfiLayer for full search order documentation.
 */
export function findLibraryPath(): string | null {
  return FfiLayer.findLibraryPath();
}

/**
 * Check if DATABASE_URL is set.
 *
 * Note: This only checks if the environment variable is set, not if
 * the database is actually accessible.
 */
export function isDatabaseAvailable(): boolean {
  return typeof process.env.DATABASE_URL === 'string' && process.env.DATABASE_URL.length > 0;
}

/**
 * Check if bootstrap tests should run.
 *
 * Bootstrap tests require database connectivity and can timeout if the database
 * is not accessible. To avoid slow test failures, bootstrap tests only run when
 * explicitly enabled via FFI_BOOTSTRAP_TESTS=true environment variable.
 *
 * This is useful because DATABASE_URL may be set in the environment but the
 * database may not be running or accessible.
 */
export function shouldRunBootstrapTests(): boolean {
  return process.env.FFI_BOOTSTRAP_TESTS === 'true' && isDatabaseAvailable();
}

/**
 * Skip message for tests requiring bootstrap (database connectivity).
 */
export const SKIP_BOOTSTRAP_MESSAGE =
  'Skipping: Set FFI_BOOTSTRAP_TESTS=true and DATABASE_URL to run bootstrap tests';

/**
 * Skip message for tests requiring database connectivity.
 */
export const SKIP_DATABASE_MESSAGE = 'Skipping: DATABASE_URL not set';

/**
 * Skip message for tests requiring FFI library.
 */
export const SKIP_LIBRARY_MESSAGE =
  'Skipping: TASKER_FFI_LIBRARY_PATH not set. Set the environment variable and build with: cargo build -p tasker-worker-ts';

/**
 * Assert that a value is a non-empty string.
 */
export function assertNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0;
}

/**
 * Assert that a value looks like a semantic version string.
 */
export function assertVersionString(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  // Basic semver pattern: x.y.z with optional suffix
  return /^\d+\.\d+\.\d+/.test(value);
}

/**
 * Test configuration for FFI integration tests.
 */
export interface TestConfig {
  libraryPath: string;
  skipDatabase: boolean;
}

/**
 * Initialize test configuration.
 *
 * @throws Error if FFI library is not found
 */
export function initTestConfig(): TestConfig {
  const libraryPath = findLibraryPath();
  if (!libraryPath) {
    throw new Error(SKIP_LIBRARY_MESSAGE);
  }

  return {
    libraryPath,
    skipDatabase: !isDatabaseAvailable(),
  };
}

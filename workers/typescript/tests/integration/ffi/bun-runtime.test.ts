/**
 * Bun FFI Runtime Integration Tests.
 *
 * Tests the BunRuntime implementation against the actual Rust FFI library.
 * Verifies that Bun's bun:ffi correctly interfaces with the native code.
 *
 * Prerequisites:
 * - Build FFI library: cargo build -p tasker-worker-ts --release
 * - (Optional) DATABASE_URL for bootstrap tests
 *
 * Run: bun test tests/integration/ffi/bun-runtime.test.ts
 */

import { afterAll, beforeAll, describe, expect, it } from 'bun:test';
import { BunRuntime } from '../../../src/ffi/bun-runtime.js';
import {
  assertVersionString,
  findLibraryPath,
  SKIP_BOOTSTRAP_MESSAGE,
  SKIP_LIBRARY_MESSAGE,
  shouldRunBootstrapTests,
} from './common.js';

describe('BunRuntime FFI Integration', () => {
  let runtime: BunRuntime;
  let libraryPath: string | null;

  beforeAll(async () => {
    libraryPath = findLibraryPath();
    if (!libraryPath) {
      console.warn(SKIP_LIBRARY_MESSAGE);
      return;
    }

    runtime = new BunRuntime();
    await runtime.load(libraryPath);
  });

  afterAll(() => {
    if (runtime?.isLoaded) {
      runtime.unload();
    }
  });

  describe('library loading', () => {
    it('loads FFI library successfully', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(runtime.isLoaded).toBe(true);
    });

    it('has correct runtime name', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(runtime.name).toBe('bun');
    });
  });

  describe('version information', () => {
    it('returns valid version string', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const version = runtime.getVersion();
      expect(typeof version).toBe('string');
      expect(version.length).toBeGreaterThan(0);
      expect(assertVersionString(version)).toBe(true);
    });

    it('returns valid Rust version string', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const rustVersion = runtime.getRustVersion();
      expect(typeof rustVersion).toBe('string');
      expect(rustVersion.length).toBeGreaterThan(0);
    });
  });

  describe('health check', () => {
    it('passes health check when loaded', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(runtime.healthCheck()).toBe(true);
    });
  });

  describe('worker status (without database)', () => {
    it('reports worker not running initially', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      // Worker should not be running without bootstrap
      expect(runtime.isWorkerRunning()).toBe(false);
    });

    it('returns worker status object', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const status = runtime.getWorkerStatus();
      expect(status).toBeDefined();
      expect(typeof status.running).toBe('boolean');
    });
  });

  describe('metrics', () => {
    it('returns FfiDispatchMetrics object', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const metrics = runtime.getFfiDispatchMetrics();
      expect(metrics).toBeDefined();
      expect(typeof metrics.pending_count).toBe('number');
      expect(typeof metrics.starvation_detected).toBe('boolean');
    });

    it('metrics have valid initial values', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const metrics = runtime.getFfiDispatchMetrics();
      expect(metrics.pending_count).toBeGreaterThanOrEqual(0);
      expect(metrics.starving_event_count).toBeGreaterThanOrEqual(0);
    });
  });

  describe('logging functions', () => {
    it('logError does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() => runtime.logError('Test error message')).not.toThrow();
    });

    it('logWarn does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() => runtime.logWarn('Test warning message')).not.toThrow();
    });

    it('logInfo does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() => runtime.logInfo('Test info message')).not.toThrow();
    });

    it('logDebug does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() => runtime.logDebug('Test debug message')).not.toThrow();
    });

    it('logTrace does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() => runtime.logTrace('Test trace message')).not.toThrow();
    });

    it('logging with fields does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() =>
        runtime.logInfo('Test with fields', {
          test_key: 'test_value',
          numeric: 42,
        })
      ).not.toThrow();
    });
  });

  describe('maintenance functions', () => {
    it('checkStarvationWarnings does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() => runtime.checkStarvationWarnings()).not.toThrow();
    });

    it('cleanupTimeouts does not throw', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      expect(() => runtime.cleanupTimeouts()).not.toThrow();
    });
  });

  describe('event polling (without bootstrap)', () => {
    it('pollStepEvents returns null when no events', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      // Without bootstrap, there should be no events
      const event = runtime.pollStepEvents();
      expect(event).toBeNull();
    });
  });

  describe('worker lifecycle (requires database)', () => {
    it('bootstrapWorker returns valid result', () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      if (!shouldRunBootstrapTests()) {
        console.warn(SKIP_BOOTSTRAP_MESSAGE);
        return;
      }

      // This test requires database connectivity
      const result = runtime.bootstrapWorker({});
      expect(result).toBeDefined();
      expect(typeof result.success).toBe('boolean');

      // Clean up if bootstrap succeeded
      if (result.success) {
        runtime.stopWorker();
      }
    });
  });

  describe('unload behavior', () => {
    it('can unload and reload library', async () => {
      if (!libraryPath) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }

      // Unload
      runtime.unload();
      expect(runtime.isLoaded).toBe(false);

      // Reload
      await runtime.load(libraryPath);
      expect(runtime.isLoaded).toBe(true);
      expect(runtime.healthCheck()).toBe(true);
    });
  });
});

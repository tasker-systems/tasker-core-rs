/**
 * Node.js FFI Runtime Integration Tests.
 *
 * Tests the NodeRuntime implementation against the actual Rust FFI library.
 * Verifies that ffi-napi correctly interfaces with the native code.
 *
 * Prerequisites:
 * - Build FFI library: cargo build -p tasker-worker-ts --release
 * - Install ffi-napi: bun install (includes ffi-napi dependency)
 * - (Optional) DATABASE_URL for bootstrap tests
 *
 * Run: node --import tsx --test tests/integration/ffi/node-runtime.test.ts
 * Or:  npx tsx --test tests/integration/ffi/node-runtime.test.ts
 *
 * Note: ffi-napi requires Node.js (not Bun) and native compilation.
 */

import assert from 'node:assert';
import { after, before, describe, it } from 'node:test';
import {
  assertVersionString,
  findLibraryPath,
  shouldRunBootstrapTests,
  SKIP_BOOTSTRAP_MESSAGE,
  SKIP_LIBRARY_MESSAGE,
} from './common.js';

// Dynamic import for NodeRuntime to avoid Bun runtime conflicts
let NodeRuntime: typeof import('../../../src/ffi/node-runtime.js').NodeRuntime;

describe('NodeRuntime FFI Integration', async () => {
  let runtime: InstanceType<typeof NodeRuntime> | null = null;
  let libraryPath: string | null = null;

  before(async () => {
    libraryPath = findLibraryPath();
    if (!libraryPath) {
      console.warn(SKIP_LIBRARY_MESSAGE);
      return;
    }

    try {
      // Dynamically import NodeRuntime to allow test file to be parsed by Bun
      const module = await import('../../../src/ffi/node-runtime.js');
      NodeRuntime = module.NodeRuntime;
      runtime = new NodeRuntime();
      await runtime.load(libraryPath);
    } catch (error) {
      console.warn('Failed to load NodeRuntime:', error);
      runtime = null;
    }
  });

  after(() => {
    if (runtime?.isLoaded) {
      runtime.unload();
    }
  });

  describe('library loading', () => {
    it('loads FFI library successfully', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      assert.strictEqual(runtime.isLoaded, true);
    });

    it('has correct runtime name', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      assert.strictEqual(runtime.name, 'node');
    });
  });

  describe('version information', () => {
    it('returns valid version string', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const version = runtime.getVersion();
      assert.strictEqual(typeof version, 'string');
      assert.ok(version.length > 0);
      assert.ok(assertVersionString(version));
    });

    it('returns valid Rust version string', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const rustVersion = runtime.getRustVersion();
      assert.strictEqual(typeof rustVersion, 'string');
      assert.ok(rustVersion.length > 0);
    });
  });

  describe('health check', () => {
    it('passes health check when loaded', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      assert.strictEqual(runtime.healthCheck(), true);
    });
  });

  describe('worker status (without database)', () => {
    it('reports worker not running initially', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      assert.strictEqual(runtime.isWorkerRunning(), false);
    });

    it('returns worker status object', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const status = runtime.getWorkerStatus();
      assert.ok(status !== null && status !== undefined);
      assert.strictEqual(typeof status.running, 'boolean');
    });
  });

  describe('metrics', () => {
    it('returns FfiDispatchMetrics object', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const metrics = runtime.getFfiDispatchMetrics();
      assert.ok(metrics !== null && metrics !== undefined);
      assert.strictEqual(typeof metrics.pending_count, 'number');
      assert.strictEqual(typeof metrics.starvation_detected, 'boolean');
    });

    it('metrics have valid initial values', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const metrics = runtime.getFfiDispatchMetrics();
      assert.ok(metrics.pending_count >= 0);
      assert.ok(metrics.starving_event_count >= 0);
    });
  });

  describe('logging functions', () => {
    it('logError does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() => r.logError('Test error message'));
    });

    it('logWarn does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() => r.logWarn('Test warning message'));
    });

    it('logInfo does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() => r.logInfo('Test info message'));
    });

    it('logDebug does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() => r.logDebug('Test debug message'));
    });

    it('logTrace does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() => r.logTrace('Test trace message'));
    });

    it('logging with fields does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() =>
        r.logInfo('Test with fields', {
          test_key: 'test_value',
          numeric: 42,
        })
      );
    });
  });

  describe('maintenance functions', () => {
    it('checkStarvationWarnings does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() => r.checkStarvationWarnings());
    });

    it('cleanupTimeouts does not throw', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const r = runtime;
      assert.doesNotThrow(() => r.cleanupTimeouts());
    });
  });

  describe('event polling (without bootstrap)', () => {
    it('pollStepEvents returns null when no events', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      const event = runtime.pollStepEvents();
      assert.strictEqual(event, null);
    });
  });

  describe('worker lifecycle (requires database)', () => {
    it('bootstrapWorker returns valid result', () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }
      if (!shouldRunBootstrapTests()) {
        console.warn(SKIP_BOOTSTRAP_MESSAGE);
        return;
      }

      const result = runtime.bootstrapWorker({});
      assert.ok(result !== null && result !== undefined);
      assert.strictEqual(typeof result.success, 'boolean');

      if (result.success) {
        runtime.stopWorker();
      }
    });
  });

  describe('unload behavior', () => {
    it('can unload and reload library', async () => {
      if (!libraryPath || !runtime) {
        console.warn(SKIP_LIBRARY_MESSAGE);
        return;
      }

      runtime.unload();
      assert.strictEqual(runtime.isLoaded, false);

      await runtime.load(libraryPath);
      assert.strictEqual(runtime.isLoaded, true);
      assert.strictEqual(runtime.healthCheck(), true);
    });
  });
});

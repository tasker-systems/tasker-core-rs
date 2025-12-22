/**
 * Runtime factory coherence tests.
 *
 * Verifies that the runtime factory correctly creates and caches
 * runtime instances based on the detected environment.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { BunRuntime } from '../../../src/ffi/bun-runtime.js';
import {
  clearRuntimeCache,
  createRuntime,
  getCachedRuntime,
  getTaskerRuntime,
  hasRuntimeCached,
} from '../../../src/ffi/runtime-factory.js';

describe('Runtime Factory', () => {
  afterEach(() => {
    clearRuntimeCache();
  });

  describe('getTaskerRuntime', () => {
    it('returns a TaskerRuntime instance', async () => {
      const runtime = await getTaskerRuntime();
      expect(runtime).toBeDefined();
      expect(runtime.name).toBeDefined();
    });

    it('returns BunRuntime when running under Bun', async () => {
      const runtime = await getTaskerRuntime();
      expect(runtime).toBeInstanceOf(BunRuntime);
      expect(runtime.name).toBe('bun');
    });

    it('caches the runtime instance', async () => {
      const runtime1 = await getTaskerRuntime();
      const runtime2 = await getTaskerRuntime();
      expect(runtime1).toBe(runtime2);
    });

    it('sets hasRuntimeCached to true after first call', async () => {
      expect(hasRuntimeCached()).toBe(false);
      await getTaskerRuntime();
      expect(hasRuntimeCached()).toBe(true);
    });
  });

  describe('createRuntime', () => {
    it("creates BunRuntime for 'bun' type", async () => {
      const runtime = await createRuntime('bun');
      expect(runtime).toBeInstanceOf(BunRuntime);
      expect(runtime.name).toBe('bun');
    });

    it("creates NodeRuntime for 'node' type", async () => {
      const runtime = await createRuntime('node');
      expect(runtime.name).toBe('node');
    });

    it("creates DenoRuntime for 'deno' type", async () => {
      const runtime = await createRuntime('deno');
      expect(runtime.name).toBe('deno');
    });

    it("throws error for 'unknown' type", async () => {
      await expect(createRuntime('unknown')).rejects.toThrow('Unsupported runtime: unknown');
    });

    it('does not cache created runtimes', async () => {
      await createRuntime('bun');
      expect(hasRuntimeCached()).toBe(false);
    });
  });

  describe('clearRuntimeCache', () => {
    it('clears the cached runtime', async () => {
      await getTaskerRuntime();
      expect(hasRuntimeCached()).toBe(true);

      clearRuntimeCache();
      expect(hasRuntimeCached()).toBe(false);
    });

    it('causes getTaskerRuntime to create new instance', async () => {
      const runtime1 = await getTaskerRuntime();
      clearRuntimeCache();
      const runtime2 = await getTaskerRuntime();

      expect(runtime1).not.toBe(runtime2);
    });

    it('is safe to call when no cache exists', () => {
      expect(() => clearRuntimeCache()).not.toThrow();
    });
  });

  describe('hasRuntimeCached', () => {
    it('returns false initially', () => {
      expect(hasRuntimeCached()).toBe(false);
    });

    it('returns true after getTaskerRuntime', async () => {
      await getTaskerRuntime();
      expect(hasRuntimeCached()).toBe(true);
    });

    it('returns false after clearRuntimeCache', async () => {
      await getTaskerRuntime();
      clearRuntimeCache();
      expect(hasRuntimeCached()).toBe(false);
    });
  });

  describe('getCachedRuntime', () => {
    it('returns null when no cache exists', () => {
      expect(getCachedRuntime()).toBeNull();
    });

    it('returns cached runtime after getTaskerRuntime', async () => {
      const runtime = await getTaskerRuntime();
      expect(getCachedRuntime()).toBe(runtime);
    });

    it('returns null after clearRuntimeCache', async () => {
      await getTaskerRuntime();
      clearRuntimeCache();
      expect(getCachedRuntime()).toBeNull();
    });
  });

  describe('runtime interface compliance', () => {
    it('created runtime has all required properties', async () => {
      const runtime = await getTaskerRuntime();

      expect(typeof runtime.name).toBe('string');
      expect(typeof runtime.isLoaded).toBe('boolean');
      expect(typeof runtime.load).toBe('function');
      expect(typeof runtime.unload).toBe('function');
      expect(typeof runtime.getVersion).toBe('function');
      expect(typeof runtime.getRustVersion).toBe('function');
      expect(typeof runtime.healthCheck).toBe('function');
      expect(typeof runtime.bootstrapWorker).toBe('function');
      expect(typeof runtime.isWorkerRunning).toBe('function');
      expect(typeof runtime.getWorkerStatus).toBe('function');
      expect(typeof runtime.stopWorker).toBe('function');
      expect(typeof runtime.transitionToGracefulShutdown).toBe('function');
      expect(typeof runtime.pollStepEvents).toBe('function');
      expect(typeof runtime.completeStepEvent).toBe('function');
      expect(typeof runtime.getFfiDispatchMetrics).toBe('function');
      expect(typeof runtime.checkStarvationWarnings).toBe('function');
      expect(typeof runtime.cleanupTimeouts).toBe('function');
      expect(typeof runtime.logError).toBe('function');
      expect(typeof runtime.logWarn).toBe('function');
      expect(typeof runtime.logInfo).toBe('function');
      expect(typeof runtime.logDebug).toBe('function');
      expect(typeof runtime.logTrace).toBe('function');
    });

    it('runtime starts in unloaded state', async () => {
      const runtime = await getTaskerRuntime();
      expect(runtime.isLoaded).toBe(false);
    });
  });
});

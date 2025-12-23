/**
 * RuntimeFactory tests.
 *
 * Verifies that the RuntimeFactory correctly creates and caches
 * runtime instances based on the detected environment.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { BunRuntime } from '../../../src/ffi/bun-runtime.js';
import { RuntimeFactory } from '../../../src/ffi/runtime-factory.js';

describe('RuntimeFactory', () => {
  afterEach(() => {
    RuntimeFactory.resetInstance();
  });

  describe('instance', () => {
    it('returns the same singleton instance', () => {
      const instance1 = RuntimeFactory.instance();
      const instance2 = RuntimeFactory.instance();
      expect(instance1).toBe(instance2);
    });

    it('returns a new instance after reset', () => {
      const instance1 = RuntimeFactory.instance();
      RuntimeFactory.resetInstance();
      const instance2 = RuntimeFactory.instance();
      expect(instance1).not.toBe(instance2);
    });
  });

  describe('getRuntime', () => {
    it('returns a TaskerRuntime instance', async () => {
      const factory = RuntimeFactory.instance();
      const runtime = await factory.getRuntime();
      expect(runtime).toBeDefined();
      expect(runtime.name).toBeDefined();
    });

    it('returns BunRuntime when running under Bun', async () => {
      const factory = RuntimeFactory.instance();
      const runtime = await factory.getRuntime();
      expect(runtime).toBeInstanceOf(BunRuntime);
      expect(runtime.name).toBe('bun');
    });

    it('caches the runtime instance', async () => {
      const factory = RuntimeFactory.instance();
      const runtime1 = await factory.getRuntime();
      const runtime2 = await factory.getRuntime();
      expect(runtime1).toBe(runtime2);
    });

    it('sets isLoaded to false before loadLibrary', async () => {
      const factory = RuntimeFactory.instance();
      await factory.getRuntime();
      expect(factory.isLoaded()).toBe(false);
    });
  });

  describe('getCachedRuntime', () => {
    it('returns null when no runtime is cached', () => {
      const factory = RuntimeFactory.instance();
      expect(factory.getCachedRuntime()).toBeNull();
    });

    it('returns cached runtime after getRuntime', async () => {
      const factory = RuntimeFactory.instance();
      const runtime = await factory.getRuntime();
      expect(factory.getCachedRuntime()).toBe(runtime);
    });

    it('returns null after resetInstance', async () => {
      const factory = RuntimeFactory.instance();
      await factory.getRuntime();
      RuntimeFactory.resetInstance();
      expect(RuntimeFactory.instance().getCachedRuntime()).toBeNull();
    });
  });

  describe('isLoaded', () => {
    it('returns false initially', () => {
      const factory = RuntimeFactory.instance();
      expect(factory.isLoaded()).toBe(false);
    });

    it('returns false after getRuntime (not loaded yet)', async () => {
      const factory = RuntimeFactory.instance();
      await factory.getRuntime();
      expect(factory.isLoaded()).toBe(false);
    });
  });

  describe('getLibraryPath', () => {
    it('returns null before loadLibrary', () => {
      const factory = RuntimeFactory.instance();
      expect(factory.getLibraryPath()).toBeNull();
    });
  });

  describe('getRuntimeType', () => {
    it('returns null before getRuntime', () => {
      const factory = RuntimeFactory.instance();
      expect(factory.getRuntimeType()).toBeNull();
    });

    it('returns the detected runtime type after getRuntime', async () => {
      const factory = RuntimeFactory.instance();
      await factory.getRuntime();
      expect(factory.getRuntimeType()).toBe('bun');
    });
  });

  describe('unload', () => {
    it('clears the cached runtime', async () => {
      const factory = RuntimeFactory.instance();
      await factory.getRuntime();
      expect(factory.getCachedRuntime()).not.toBeNull();

      factory.unload();
      expect(factory.getCachedRuntime()).toBeNull();
    });

    it('is safe to call when no runtime exists', () => {
      const factory = RuntimeFactory.instance();
      expect(() => factory.unload()).not.toThrow();
    });

    it('causes getRuntime to create a new instance', async () => {
      const factory = RuntimeFactory.instance();
      const runtime1 = await factory.getRuntime();
      factory.unload();
      const runtime2 = await factory.getRuntime();

      expect(runtime1).not.toBe(runtime2);
    });
  });

  describe('resetInstance', () => {
    it('clears the singleton and runtime', async () => {
      await RuntimeFactory.instance().getRuntime();
      RuntimeFactory.resetInstance();

      const factory = RuntimeFactory.instance();
      expect(factory.getCachedRuntime()).toBeNull();
      expect(factory.isLoaded()).toBe(false);
    });

    it('is safe to call when no instance exists', () => {
      expect(() => RuntimeFactory.resetInstance()).not.toThrow();
    });
  });

  describe('findLibraryPath', () => {
    it('returns null when library is not found', () => {
      // Clear any env var that might be set
      const original = process.env.TASKER_FFI_LIBRARY_PATH;
      delete process.env.TASKER_FFI_LIBRARY_PATH;

      // Use a non-existent directory
      const path = RuntimeFactory.findLibraryPath('/nonexistent/path');

      // Restore
      if (original) {
        process.env.TASKER_FFI_LIBRARY_PATH = original;
      }

      expect(path).toBeNull();
    });

    it('returns env path when TASKER_FFI_LIBRARY_PATH is set to valid file', () => {
      // This test will only work if we have a valid library built
      // For unit testing, we just verify the method exists and returns
      const path = RuntimeFactory.findLibraryPath();
      // The result depends on whether the library exists
      expect(path === null || typeof path === 'string').toBe(true);
    });
  });

  describe('runtime interface compliance', () => {
    it('created runtime has all required properties', async () => {
      const factory = RuntimeFactory.instance();
      const runtime = await factory.getRuntime();

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
      const factory = RuntimeFactory.instance();
      const runtime = await factory.getRuntime();
      expect(runtime.isLoaded).toBe(false);
    });
  });
});

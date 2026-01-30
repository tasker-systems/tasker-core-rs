/**
 * FfiLayer tests.
 *
 * Tests FfiLayer state management, load/unload lifecycle,
 * getRuntime error handling, and findLibraryPath static method.
 */

import { afterEach, describe, expect, test } from 'bun:test';
import { FfiLayer } from '../../../src/ffi/ffi-layer.js';

// =============================================================================
// Tests
// =============================================================================

describe('FfiLayer', () => {
  describe('constructor', () => {
    test('should create with default runtime detection', () => {
      const layer = new FfiLayer();

      // Bun test environment should detect 'bun'
      const runtimeType = layer.getRuntimeType();
      expect(['bun', 'node', 'deno', 'unknown']).toContain(runtimeType);
    });

    test('should accept runtimeType override', () => {
      const layer = new FfiLayer({ runtimeType: 'node' });

      expect(layer.getRuntimeType()).toBe('node');
    });

    test('should accept libraryPath override', () => {
      const layer = new FfiLayer({ libraryPath: '/custom/path.dylib' });

      expect(layer.getRuntimeType()).toBeDefined();
      // libraryPath is used during load(), not accessible directly before load
    });
  });

  describe('isLoaded', () => {
    test('should be false before load', () => {
      const layer = new FfiLayer();

      expect(layer.isLoaded()).toBe(false);
    });
  });

  describe('getRuntime', () => {
    test('should throw when not loaded', () => {
      const layer = new FfiLayer();

      expect(() => layer.getRuntime()).toThrow('FFI not loaded. Call load() first.');
    });
  });

  describe('getLibraryPath', () => {
    test('should return null before load', () => {
      const layer = new FfiLayer();

      expect(layer.getLibraryPath()).toBeNull();
    });
  });

  describe('getRuntimeType', () => {
    test('should return configured type', () => {
      const layer = new FfiLayer({ runtimeType: 'deno' });

      expect(layer.getRuntimeType()).toBe('deno');
    });

    test('should return detected type when no override', () => {
      const layer = new FfiLayer();

      const runtimeType = layer.getRuntimeType();
      expect(typeof runtimeType).toBe('string');
      expect(runtimeType.length).toBeGreaterThan(0);
    });
  });

  describe('load', () => {
    test('should throw when no library path found', async () => {
      // No configured path, no env var, no discoverable path
      const originalEnv = process.env.TASKER_FFI_LIBRARY_PATH;
      delete process.env.TASKER_FFI_LIBRARY_PATH;

      try {
        const layer = new FfiLayer();

        await expect(layer.load()).rejects.toThrow('FFI library not found');
      } finally {
        if (originalEnv !== undefined) {
          process.env.TASKER_FFI_LIBRARY_PATH = originalEnv;
        }
      }
    });
  });

  describe('unload', () => {
    test('should be safe to call when not loaded', async () => {
      const layer = new FfiLayer();

      // Should not throw
      await layer.unload();

      expect(layer.isLoaded()).toBe(false);
      expect(layer.getLibraryPath()).toBeNull();
    });

    test('should clear state after unload', async () => {
      const layer = new FfiLayer();

      await layer.unload();

      expect(layer.isLoaded()).toBe(false);
      expect(layer.getLibraryPath()).toBeNull();
    });
  });
});

// =============================================================================
// findLibraryPath (static method)
// =============================================================================

describe('FfiLayer.findLibraryPath', () => {
  const originalEnv = process.env.TASKER_FFI_LIBRARY_PATH;

  afterEach(() => {
    if (originalEnv === undefined) {
      delete process.env.TASKER_FFI_LIBRARY_PATH;
    } else {
      process.env.TASKER_FFI_LIBRARY_PATH = originalEnv;
    }
  });

  test('should return null when TASKER_FFI_LIBRARY_PATH is not set', () => {
    delete process.env.TASKER_FFI_LIBRARY_PATH;

    const result = FfiLayer.findLibraryPath();

    expect(result).toBeNull();
  });

  test('should return null when path does not exist', () => {
    process.env.TASKER_FFI_LIBRARY_PATH = '/nonexistent/path/to/lib.dylib';

    const result = FfiLayer.findLibraryPath();

    expect(result).toBeNull();
  });

  test('should return path when env var is set and file exists', () => {
    // Use a file that we know exists (this test file itself)
    const existingFile = import.meta.path;
    process.env.TASKER_FFI_LIBRARY_PATH = existingFile;

    const result = FfiLayer.findLibraryPath();

    expect(result).toBe(existingFile);
  });

  test('should accept deprecated callerDir parameter', () => {
    delete process.env.TASKER_FFI_LIBRARY_PATH;

    // callerDir is deprecated and ignored, should still return null
    const result = FfiLayer.findLibraryPath('/some/dir');

    expect(result).toBeNull();
  });
});

/**
 * Runtime detection coherence tests.
 *
 * Verifies that runtime detection works correctly and returns
 * consistent, expected values.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import {
  detectRuntime,
  getLibraryPath,
  getRuntimeInfo,
  isBun,
  isDeno,
  isNode,
  type RuntimeType,
} from '../../../src/ffi/runtime.js';

describe('Runtime Detection', () => {
  describe('detectRuntime', () => {
    it('returns a valid runtime type', () => {
      const runtime = detectRuntime();
      const validRuntimes: RuntimeType[] = ['bun', 'node', 'deno', 'unknown'];
      expect(validRuntimes).toContain(runtime);
    });

    it("returns 'bun' when running under Bun", () => {
      // Since we're running these tests with Bun, this should be true
      expect(detectRuntime()).toBe('bun');
    });

    it('caches the runtime type for performance', () => {
      const first = detectRuntime();
      const second = detectRuntime();
      expect(first).toBe(second);
    });
  });

  describe('runtime helper functions', () => {
    it('isBun returns true when running under Bun', () => {
      expect(isBun()).toBe(true);
    });

    it('isNode returns false when running under Bun', () => {
      expect(isNode()).toBe(false);
    });

    it('isDeno returns false when running under Bun', () => {
      expect(isDeno()).toBe(false);
    });

    it('helper functions are consistent with detectRuntime', () => {
      const runtime = detectRuntime();
      expect(isBun()).toBe(runtime === 'bun');
      expect(isNode()).toBe(runtime === 'node');
      expect(isDeno()).toBe(runtime === 'deno');
    });
  });

  describe('getRuntimeInfo', () => {
    it('returns complete runtime information', () => {
      const info = getRuntimeInfo();

      expect(info).toHaveProperty('type');
      expect(info).toHaveProperty('version');
      expect(info).toHaveProperty('platform');
      expect(info).toHaveProperty('arch');
    });

    it('returns correct type for Bun runtime', () => {
      const info = getRuntimeInfo();
      expect(info.type).toBe('bun');
    });

    it('returns a valid version string', () => {
      const info = getRuntimeInfo();
      expect(typeof info.version).toBe('string');
      expect(info.version).not.toBe('unknown');
      // Bun versions are semver-ish (e.g., "1.0.0", "1.1.38")
      expect(info.version).toMatch(/^\d+\.\d+/);
    });

    it('returns valid platform information', () => {
      const info = getRuntimeInfo();
      const validPlatforms = ['darwin', 'linux', 'win32', 'unknown'];
      expect(validPlatforms).toContain(info.platform);
    });

    it('returns valid architecture information', () => {
      const info = getRuntimeInfo();
      const validArchs = ['x64', 'arm64', 'arm', 'ia32', 'unknown'];
      expect(validArchs).toContain(info.arch);
    });
  });

  describe('getLibraryPath', () => {
    const originalEnv = process.env.TASKER_FFI_LIBRARY_PATH;

    afterEach(() => {
      // Restore original env
      if (originalEnv !== undefined) {
        process.env.TASKER_FFI_LIBRARY_PATH = originalEnv;
      } else {
        delete process.env.TASKER_FFI_LIBRARY_PATH;
      }
    });

    it('returns TASKER_FFI_LIBRARY_PATH when set', () => {
      const testPath = '/test/path/libtasker_worker.dylib';
      process.env.TASKER_FFI_LIBRARY_PATH = testPath;
      const path = getLibraryPath();
      expect(path).toBe(testPath);
    });

    it('uses provided base path when env not set', () => {
      delete process.env.TASKER_FFI_LIBRARY_PATH;
      const basePath = '/custom/path';
      const path = getLibraryPath(basePath);
      expect(path).toStartWith(basePath);
      expect(path).toContain('tasker_worker');
    });

    it('includes correct library extension when using base path', () => {
      delete process.env.TASKER_FFI_LIBRARY_PATH;
      const info = getRuntimeInfo();
      const path = getLibraryPath('/test');

      switch (info.platform) {
        case 'darwin':
          expect(path).toContain('.dylib');
          break;
        case 'linux':
          expect(path).toContain('.so');
          break;
        case 'win32':
          expect(path).toContain('.dll');
          break;
      }
    });

    it('throws error when no env var and no base path', () => {
      delete process.env.TASKER_FFI_LIBRARY_PATH;
      expect(() => getLibraryPath()).toThrow('TASKER_FFI_LIBRARY_PATH');
    });

    it('prefers env var over base path', () => {
      const envPath = '/env/path/libtasker_worker.dylib';
      process.env.TASKER_FFI_LIBRARY_PATH = envPath;
      const path = getLibraryPath('/ignored/base');
      expect(path).toBe(envPath);
    });
  });
});

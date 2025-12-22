/**
 * Tests for the bootstrap API.
 *
 * Note: These tests run without the FFI library loaded, testing fallback behavior.
 */

import { describe, test, expect } from 'bun:test';
import {
  bootstrapWorker,
  stopWorker,
  getWorkerStatus,
  transitionToGracefulShutdown,
  isWorkerRunning,
  getVersion,
  getRustVersion,
  healthCheck,
} from '../../../src/bootstrap/bootstrap';

describe('Bootstrap API (No FFI)', () => {
  describe('bootstrapWorker', () => {
    test('should return error result when runtime not loaded', () => {
      const result = bootstrapWorker();

      expect(result.success).toBe(false);
      expect(result.status).toBe('error');
      expect(result.error).toBeDefined();
    });

    test('should accept config options', () => {
      const result = bootstrapWorker({
        namespace: 'test',
        logLevel: 'debug',
      });

      // Still fails (no runtime), but should not throw
      expect(result.success).toBe(false);
    });

    test('should handle all config options', () => {
      const result = bootstrapWorker({
        workerId: 'test-worker-1',
        namespace: 'payments',
        configPath: '/path/to/config.toml',
        logLevel: 'trace',
        databaseUrl: 'postgresql://localhost/test',
      });

      expect(result.success).toBe(false);
      expect(result.status).toBe('error');
    });
  });

  describe('stopWorker', () => {
    test('should return not_running when runtime not loaded', () => {
      const result = stopWorker();

      expect(result.success).toBe(true);
      expect(result.status).toBe('not_running');
    });
  });

  describe('getWorkerStatus', () => {
    test('should return stopped status when runtime not loaded', () => {
      const status = getWorkerStatus();

      expect(status.success).toBe(false);
      expect(status.running).toBe(false);
    });
  });

  describe('transitionToGracefulShutdown', () => {
    test('should return not_running when runtime not loaded', () => {
      const result = transitionToGracefulShutdown();

      expect(result.success).toBe(true);
      expect(result.status).toBe('not_running');
    });
  });

  describe('isWorkerRunning', () => {
    test('should return false when runtime not loaded', () => {
      const running = isWorkerRunning();

      expect(running).toBe(false);
    });
  });

  describe('getVersion', () => {
    test('should return unknown when runtime not loaded', () => {
      const version = getVersion();

      expect(version).toContain('unknown');
    });
  });

  describe('getRustVersion', () => {
    test('should return unknown when runtime not loaded', () => {
      const version = getRustVersion();

      expect(version).toContain('unknown');
    });
  });

  describe('healthCheck', () => {
    test('should return false when runtime not loaded', () => {
      const healthy = healthCheck();

      expect(healthy).toBe(false);
    });
  });
});

describe('Bootstrap API Return Types', () => {
  test('BootstrapResult should have required fields', () => {
    const result = bootstrapWorker();

    expect(typeof result.success).toBe('boolean');
    expect(typeof result.status).toBe('string');
    expect(typeof result.message).toBe('string');
    // Optional fields may be undefined
    expect(result.workerId === undefined || typeof result.workerId === 'string').toBe(true);
    expect(result.error === undefined || typeof result.error === 'string').toBe(true);
  });

  test('StopResult should have required fields', () => {
    const result = stopWorker();

    expect(typeof result.success).toBe('boolean');
    expect(typeof result.status).toBe('string');
    expect(typeof result.message).toBe('string');
  });

  test('WorkerStatus should have required fields', () => {
    const status = getWorkerStatus();

    expect(typeof status.success).toBe('boolean');
    expect(typeof status.running).toBe('boolean');
  });
});

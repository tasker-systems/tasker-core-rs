/**
 * WorkerServer tests.
 *
 * Unit tests for the WorkerServer singleton class.
 * Integration tests that require FFI are in the integration test suite.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { WorkerServer } from '../../../src/server/worker-server.js';

describe('WorkerServer', () => {
  afterEach(async () => {
    await WorkerServer.resetInstance();
  });

  describe('singleton', () => {
    it('returns the same instance', () => {
      const instance1 = WorkerServer.instance();
      const instance2 = WorkerServer.instance();
      expect(instance1).toBe(instance2);
    });

    it('returns a new instance after reset', async () => {
      const instance1 = WorkerServer.instance();
      await WorkerServer.resetInstance();
      const instance2 = WorkerServer.instance();
      expect(instance1).not.toBe(instance2);
    });
  });

  describe('initial state', () => {
    it('starts in initialized state', () => {
      const server = WorkerServer.instance();
      expect(server.state).toBe('initialized');
    });

    it('is not running initially', () => {
      const server = WorkerServer.instance();
      expect(server.running()).toBe(false);
    });

    it('has no workerId initially', () => {
      const server = WorkerServer.instance();
      expect(server.workerId).toBeNull();
    });
  });

  describe('status', () => {
    it('returns status in initialized state', () => {
      const server = WorkerServer.instance();
      const status = server.status();

      expect(status.state).toBe('initialized');
      expect(status.workerId).toBeNull();
      expect(status.running).toBe(false);
      expect(status.processedCount).toBe(0);
      expect(status.errorCount).toBe(0);
      expect(status.activeHandlers).toBe(0);
      expect(status.uptimeMs).toBe(0);
    });
  });

  describe('healthCheck', () => {
    it('returns unhealthy when not running', () => {
      const server = WorkerServer.instance();
      const health = server.healthCheck();

      expect(health.healthy).toBe(false);
      expect(health.error).toContain('not running');
    });
  });

  describe('onShutdown', () => {
    it('accepts shutdown handlers', () => {
      const server = WorkerServer.instance();
      expect(() => {
        server.onShutdown(() => {
          // Handler registered
        });
      }).not.toThrow();
    });
  });

  describe('shutdown', () => {
    it('is safe to call when not running', async () => {
      const server = WorkerServer.instance();
      await expect(server.shutdown()).resolves.toBeUndefined();
    });
  });
});

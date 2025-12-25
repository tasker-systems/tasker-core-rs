/**
 * WorkerServer tests.
 *
 * Unit tests for the WorkerServer class.
 * Integration tests that require FFI are in the integration test suite.
 */

import { describe, expect, it } from 'bun:test';
import { WorkerServer } from '../../../src/server/worker-server.js';

describe('WorkerServer', () => {
  describe('initial state', () => {
    it('starts in initialized state', () => {
      const server = new WorkerServer();
      expect(server.getState()).toBe('initialized');
    });

    it('is not running initially', () => {
      const server = new WorkerServer();
      expect(server.isRunning()).toBe(false);
    });

    it('has no workerId initially', () => {
      const server = new WorkerServer();
      expect(server.getWorkerId()).toBeNull();
    });
  });

  describe('status', () => {
    it('returns status in initialized state', () => {
      const server = new WorkerServer();
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
      const server = new WorkerServer();
      const health = server.healthCheck();

      expect(health.healthy).toBe(false);
      expect(health.error).toContain('not running');
    });
  });

  describe('onShutdown', () => {
    it('accepts shutdown handlers', () => {
      const server = new WorkerServer();
      expect(() => {
        server.onShutdown(() => {
          // Handler registered
        });
      }).not.toThrow();
    });
  });

  describe('shutdown', () => {
    it('is safe to call when not running', async () => {
      const server = new WorkerServer();
      await expect(server.shutdown()).resolves.toBeUndefined();
    });
  });

  describe('handler system', () => {
    it('provides access to handler system for registration', () => {
      const server = new WorkerServer();
      const handlerSystem = server.getHandlerSystem();
      expect(handlerSystem).toBeDefined();
      expect(handlerSystem.handlerCount()).toBe(0);
    });
  });

  describe('event system', () => {
    it('returns null event system before start', () => {
      const server = new WorkerServer();
      expect(server.getEventSystem()).toBeNull();
    });
  });
});

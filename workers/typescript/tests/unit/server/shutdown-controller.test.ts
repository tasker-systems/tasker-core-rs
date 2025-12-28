/**
 * ShutdownController tests.
 *
 * Verifies graceful shutdown coordination behavior.
 */

import { afterEach, describe, expect, it, mock } from 'bun:test';
import { ShutdownController } from '../../../src/server/shutdown-controller.js';

describe('ShutdownController', () => {
  let controller: ShutdownController;

  afterEach(() => {
    controller?.reset();
  });

  describe('initial state', () => {
    it('starts with isRequested false', () => {
      controller = new ShutdownController();
      expect(controller.isRequested).toBe(false);
    });

    it('starts with signal null', () => {
      controller = new ShutdownController();
      expect(controller.signal).toBeNull();
    });

    it('provides a promise', () => {
      controller = new ShutdownController();
      expect(controller.promise).toBeInstanceOf(Promise);
    });
  });

  describe('trigger', () => {
    it('sets isRequested to true', () => {
      controller = new ShutdownController();
      controller.trigger('SIGTERM');
      expect(controller.isRequested).toBe(true);
    });

    it('stores the signal', () => {
      controller = new ShutdownController();
      controller.trigger('SIGINT');
      expect(controller.signal).toBe('SIGINT');
    });

    it('resolves the promise', async () => {
      controller = new ShutdownController();
      const resolved = mock(() => {});

      controller.promise.then(resolved);
      controller.trigger('SIGTERM');

      // Give the promise time to resolve
      await new Promise((resolve) => setTimeout(resolve, 10));
      expect(resolved).toHaveBeenCalled();
    });

    it('ignores subsequent triggers', () => {
      controller = new ShutdownController();
      controller.trigger('SIGTERM');
      controller.trigger('SIGINT');

      expect(controller.signal).toBe('SIGTERM');
    });
  });

  describe('onShutdown', () => {
    it('registers handlers', async () => {
      controller = new ShutdownController();
      const handler = mock(() => {});

      controller.onShutdown(handler);
      await controller.executeHandlers();

      expect(handler).toHaveBeenCalled();
    });

    it('executes handlers in order', async () => {
      controller = new ShutdownController();
      const order: number[] = [];

      controller.onShutdown(() => order.push(1));
      controller.onShutdown(() => order.push(2));
      controller.onShutdown(() => order.push(3));

      await controller.executeHandlers();

      expect(order).toEqual([1, 2, 3]);
    });

    it('continues after handler error', async () => {
      controller = new ShutdownController();
      const order: number[] = [];

      controller.onShutdown(() => order.push(1));
      controller.onShutdown(() => {
        throw new Error('Handler failed');
      });
      controller.onShutdown(() => order.push(3));

      await controller.executeHandlers();

      expect(order).toEqual([1, 3]);
    });

    it('supports async handlers', async () => {
      controller = new ShutdownController();
      const order: number[] = [];

      controller.onShutdown(async () => {
        await new Promise((resolve) => setTimeout(resolve, 10));
        order.push(1);
      });
      controller.onShutdown(() => order.push(2));

      await controller.executeHandlers();

      expect(order).toEqual([1, 2]);
    });
  });

  describe('reset', () => {
    it('clears isRequested', () => {
      controller = new ShutdownController();
      controller.trigger('SIGTERM');
      controller.reset();
      expect(controller.isRequested).toBe(false);
    });

    it('clears signal', () => {
      controller = new ShutdownController();
      controller.trigger('SIGTERM');
      controller.reset();
      expect(controller.signal).toBeNull();
    });

    it('clears handlers', async () => {
      controller = new ShutdownController();
      const handler = mock(() => {});

      controller.onShutdown(handler);
      controller.reset();
      await controller.executeHandlers();

      expect(handler).not.toHaveBeenCalled();
    });
  });
});

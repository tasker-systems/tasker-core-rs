/**
 * HandlerRegistry tests.
 *
 * Verifies handler registration, resolution, and lifecycle.
 */

import { describe, expect, it, spyOn } from 'bun:test';
import { StepHandler } from '../../../src/handler/base.js';
import { HandlerRegistry } from '../../../src/handler/registry.js';
import { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

// Test handlers
class OrderHandler extends StepHandler {
  static handlerName = 'order_handler';
  static handlerVersion = '1.0.0';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'order' });
  }
}

class PaymentHandler extends StepHandler {
  static handlerName = 'payment_handler';
  static handlerVersion = '2.0.0';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'payment' });
  }
}

class FailingConstructorHandler extends StepHandler {
  static handlerName = 'failing_constructor';

  constructor() {
    super();
    throw new Error('Constructor failed!');
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({});
  }
}

describe('HandlerRegistry', () => {
  describe('register', () => {
    it('registers a handler class', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);

      expect(registry.isRegistered('order_handler')).toBe(true);
    });

    it('registers multiple handlers', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);
      registry.register('payment_handler', PaymentHandler);

      expect(registry.handlerCount()).toBe(2);
    });

    it('throws error for empty name', () => {
      const registry = new HandlerRegistry();

      expect(() => registry.register('', OrderHandler)).toThrow(
        'Handler name must be a non-empty string'
      );
    });

    it('throws error for non-string name', () => {
      const registry = new HandlerRegistry();

      // @ts-expect-error Testing runtime validation
      expect(() => registry.register(null, OrderHandler)).toThrow(
        'Handler name must be a non-empty string'
      );
    });

    it('throws error for non-function handler class', () => {
      const registry = new HandlerRegistry();

      // @ts-expect-error Testing runtime validation
      expect(() => registry.register('test', 'not a function')).toThrow(
        'handlerClass must be a StepHandler subclass'
      );
    });

    it('warns when overwriting existing handler', () => {
      const registry = new HandlerRegistry();
      const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});

      registry.register('order_handler', OrderHandler);
      registry.register('order_handler', PaymentHandler);

      expect(warnSpy).toHaveBeenCalledWith(
        expect.stringContaining('Overwriting existing handler: order_handler')
      );

      warnSpy.mockRestore();
    });

    it('logs info message on registration', () => {
      const registry = new HandlerRegistry();
      const infoSpy = spyOn(console, 'info').mockImplementation(() => {});

      registry.register('order_handler', OrderHandler);

      expect(infoSpy).toHaveBeenCalledWith(
        expect.stringContaining('Registered handler: order_handler')
      );

      infoSpy.mockRestore();
    });
  });

  describe('unregister', () => {
    it('removes registered handler', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);

      const result = registry.unregister('order_handler');

      expect(result).toBe(true);
      expect(registry.isRegistered('order_handler')).toBe(false);
    });

    it('returns false for non-existent handler', () => {
      const registry = new HandlerRegistry();

      const result = registry.unregister('nonexistent');

      expect(result).toBe(false);
    });
  });

  describe('resolve', () => {
    it('instantiates registered handler', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);

      const handler = registry.resolve('order_handler');

      expect(handler).not.toBeNull();
      expect(handler?.name).toBe('order_handler');
      expect(handler?.version).toBe('1.0.0');
    });

    it('returns new instance each time', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);

      const handler1 = registry.resolve('order_handler');
      const handler2 = registry.resolve('order_handler');

      expect(handler1).not.toBe(handler2);
    });

    it('returns null for unregistered handler', () => {
      const registry = new HandlerRegistry();
      const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});

      const handler = registry.resolve('nonexistent');

      expect(handler).toBeNull();
      expect(warnSpy).toHaveBeenCalledWith(
        expect.stringContaining('Handler not found: nonexistent')
      );

      warnSpy.mockRestore();
    });

    it('returns null when constructor throws', () => {
      const registry = new HandlerRegistry();
      const errorSpy = spyOn(console, 'error').mockImplementation(() => {});

      registry.register('failing_constructor', FailingConstructorHandler);
      const handler = registry.resolve('failing_constructor');

      expect(handler).toBeNull();
      expect(errorSpy).toHaveBeenCalledWith(
        expect.stringContaining('Failed to instantiate handler'),
        expect.any(Error)
      );

      errorSpy.mockRestore();
    });
  });

  describe('getHandlerClass', () => {
    it('returns handler class for registered handler', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);

      const handlerClass = registry.getHandlerClass('order_handler');

      expect(handlerClass).toBe(OrderHandler);
    });

    it('returns undefined for unregistered handler', () => {
      const registry = new HandlerRegistry();

      const handlerClass = registry.getHandlerClass('nonexistent');

      expect(handlerClass).toBeUndefined();
    });
  });

  describe('isRegistered', () => {
    it('returns true for registered handler', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);

      expect(registry.isRegistered('order_handler')).toBe(true);
    });

    it('returns false for unregistered handler', () => {
      const registry = new HandlerRegistry();

      expect(registry.isRegistered('nonexistent')).toBe(false);
    });
  });

  describe('listHandlers', () => {
    it('returns empty array when no handlers registered', () => {
      const registry = new HandlerRegistry();

      expect(registry.listHandlers()).toEqual([]);
    });

    it('returns array of registered handler names', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);
      registry.register('payment_handler', PaymentHandler);

      const handlers = registry.listHandlers();

      expect(handlers).toContain('order_handler');
      expect(handlers).toContain('payment_handler');
      expect(handlers).toHaveLength(2);
    });
  });

  describe('handlerCount', () => {
    it('returns 0 when empty', () => {
      const registry = new HandlerRegistry();

      expect(registry.handlerCount()).toBe(0);
    });

    it('returns correct count', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);
      registry.register('payment_handler', PaymentHandler);

      expect(registry.handlerCount()).toBe(2);
    });
  });

  describe('clear', () => {
    it('removes all handlers', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);
      registry.register('payment_handler', PaymentHandler);

      registry.clear();

      expect(registry.handlerCount()).toBe(0);
      expect(registry.listHandlers()).toEqual([]);
    });

    it('logs debug message', () => {
      const registry = new HandlerRegistry();
      const debugSpy = spyOn(console, 'debug').mockImplementation(() => {});

      registry.clear();

      expect(debugSpy).toHaveBeenCalledWith(expect.stringContaining('Cleared all handlers'));

      debugSpy.mockRestore();
    });
  });

  describe('debugInfo', () => {
    it('returns empty state when no handlers', () => {
      const registry = new HandlerRegistry();

      const info = registry.debugInfo();

      expect(info.handlerCount).toBe(0);
      expect(info.handlers).toEqual({});
    });

    it('returns handler names and class names', () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);
      registry.register('payment_handler', PaymentHandler);

      const info = registry.debugInfo();

      expect(info.handlerCount).toBe(2);
      expect(info.handlers).toEqual({
        order_handler: 'OrderHandler',
        payment_handler: 'PaymentHandler',
      });
    });
  });

  describe('integration', () => {
    it('supports full workflow: register, resolve, call', async () => {
      const registry = new HandlerRegistry();
      registry.register('order_handler', OrderHandler);

      const handler = registry.resolve('order_handler');
      expect(handler).not.toBeNull();

      // Create a minimal context for testing
      const mockEvent = {
        event_id: 'evt-1',
        task_uuid: 'task-1',
        step_uuid: 'step-1',
        correlation_id: 'corr-1',
        trace_id: null,
        span_id: null,
        task_correlation_id: 'task-corr-1',
        parent_correlation_id: null,
        task: {
          task_uuid: 'task-1',
          named_task_uuid: 'named-1',
          name: 'Test',
          namespace: 'test',
          version: '1.0.0',
          context: null,
          correlation_id: 'corr-1',
          parent_correlation_id: null,
          complete: false,
          priority: 0,
          initiator: null,
          source_system: null,
          reason: null,
          tags: null,
          identity_hash: 'hash',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          requested_at: new Date().toISOString(),
        },
        workflow_step: {
          workflow_step_uuid: 'step-1',
          task_uuid: 'task-1',
          named_step_uuid: 'named-step-1',
          name: 'Step',
          template_step_name: 'step',
          retryable: true,
          max_attempts: 3,
          attempts: 0,
          in_process: false,
          processed: false,
          skippable: false,
          inputs: null,
          results: null,
          backoff_request_seconds: null,
          processed_at: null,
          last_attempted_at: null,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
        step_definition: {
          name: 'step',
          description: 'Test step',
          handler: { callable: 'OrderHandler', initialization: {} },
          system_dependency: null,
          dependencies: [],
          timeout_seconds: 30,
          retry: {
            retryable: true,
            max_attempts: 3,
            backoff: 'exponential',
            backoff_base_ms: 1000,
            max_backoff_ms: 30000,
          },
        },
        dependency_results: {},
      };

      const context = StepContext.fromFfiEvent(mockEvent, 'order_handler');

      // Handler was already asserted to be non-null above
      if (!handler) {
        throw new Error('Handler should not be null');
      }
      const result = await handler.call(context);

      expect(result.success).toBe(true);
      expect(result.result).toEqual({ type: 'order' });
    });
  });
});

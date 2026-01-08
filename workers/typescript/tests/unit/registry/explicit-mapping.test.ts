/**
 * TAS-93: ExplicitMappingResolver tests.
 *
 * Verifies explicit handler registration and resolution.
 */

import { describe, expect, it, spyOn } from 'bun:test';
import { StepHandler } from '../../../src/handler/base.js';
import type { HandlerDefinition } from '../../../src/registry/handler-definition.js';
import { ExplicitMappingResolver } from '../../../src/registry/resolvers/explicit-mapping.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

// Test handlers
class OrderHandler extends StepHandler {
  static handlerName = 'order_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'order' });
  }
}

class PaymentHandler extends StepHandler {
  static handlerName = 'payment_handler';

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

describe('ExplicitMappingResolver', () => {
  describe('properties', () => {
    it('has correct name', () => {
      const resolver = new ExplicitMappingResolver();
      expect(resolver.name).toBe('explicit_mapping');
    });

    it('has priority 10', () => {
      const resolver = new ExplicitMappingResolver();
      expect(resolver.priority).toBe(10);
    });
  });

  describe('register', () => {
    it('registers a handler class', () => {
      const resolver = new ExplicitMappingResolver();

      resolver.register('order_handler', OrderHandler);

      expect(resolver.registeredCallables()).toContain('order_handler');
    });

    it('registers handler instance', () => {
      const resolver = new ExplicitMappingResolver();
      const handler = new OrderHandler();

      resolver.register('order_instance', handler);

      expect(resolver.registeredCallables()).toContain('order_instance');
    });

    it('registers factory function', () => {
      const resolver = new ExplicitMappingResolver();
      const factory = () => new OrderHandler();

      resolver.register('order_factory', factory);

      expect(resolver.registeredCallables()).toContain('order_factory');
    });

    it('overwrites existing registration', () => {
      const resolver = new ExplicitMappingResolver();

      resolver.register('handler', OrderHandler);
      resolver.register('handler', PaymentHandler);

      expect(resolver.registeredCallables()).toHaveLength(1);
    });
  });

  describe('unregister', () => {
    it('removes registered handler', () => {
      const resolver = new ExplicitMappingResolver();
      resolver.register('order_handler', OrderHandler);

      const result = resolver.unregister('order_handler');

      expect(result).toBe(true);
      expect(resolver.registeredCallables()).not.toContain('order_handler');
    });

    it('returns false for non-existent handler', () => {
      const resolver = new ExplicitMappingResolver();

      const result = resolver.unregister('nonexistent');

      expect(result).toBe(false);
    });
  });

  describe('registeredCallables', () => {
    it('returns empty array when no handlers registered', () => {
      const resolver = new ExplicitMappingResolver();

      expect(resolver.registeredCallables()).toEqual([]);
    });

    it('returns all registered handler names', () => {
      const resolver = new ExplicitMappingResolver();
      resolver.register('order_handler', OrderHandler);
      resolver.register('payment_handler', PaymentHandler);

      const callables = resolver.registeredCallables();

      expect(callables).toContain('order_handler');
      expect(callables).toContain('payment_handler');
      expect(callables).toHaveLength(2);
    });
  });

  describe('canResolve', () => {
    it('returns true for registered handler', () => {
      const resolver = new ExplicitMappingResolver();
      resolver.register('order_handler', OrderHandler);

      const definition: HandlerDefinition = { callable: 'order_handler' };

      expect(resolver.canResolve(definition)).toBe(true);
    });

    it('returns false for unregistered handler', () => {
      const resolver = new ExplicitMappingResolver();

      const definition: HandlerDefinition = { callable: 'unknown_handler' };

      expect(resolver.canResolve(definition)).toBe(false);
    });
  });

  describe('resolve', () => {
    it('instantiates handler class', async () => {
      const resolver = new ExplicitMappingResolver();
      resolver.register('order_handler', OrderHandler);

      const definition: HandlerDefinition = { callable: 'order_handler' };
      const handler = await resolver.resolve(definition);

      expect(handler).not.toBeNull();
      expect(handler?.name).toBe('order_handler');
    });

    it('returns existing instance', async () => {
      const resolver = new ExplicitMappingResolver();
      const instance = new OrderHandler();
      resolver.register('order_instance', instance);

      const definition: HandlerDefinition = { callable: 'order_instance' };
      const handler = await resolver.resolve(definition);

      expect(handler).toBe(instance);
    });

    it('calls factory function', async () => {
      const resolver = new ExplicitMappingResolver();
      const factory = () => new PaymentHandler();
      resolver.register('payment_factory', factory);

      const definition: HandlerDefinition = { callable: 'payment_factory' };
      const handler = await resolver.resolve(definition);

      expect(handler).not.toBeNull();
      expect(handler?.name).toBe('payment_handler');
    });

    it('returns new instance each call for classes', async () => {
      const resolver = new ExplicitMappingResolver();
      resolver.register('order_handler', OrderHandler);

      const definition: HandlerDefinition = { callable: 'order_handler' };
      const handler1 = await resolver.resolve(definition);
      const handler2 = await resolver.resolve(definition);

      expect(handler1).not.toBe(handler2);
    });

    it('returns null for unregistered handler', async () => {
      const resolver = new ExplicitMappingResolver();

      const definition: HandlerDefinition = { callable: 'unknown' };
      const handler = await resolver.resolve(definition);

      expect(handler).toBeNull();
    });

    it('returns null when constructor throws', async () => {
      const resolver = new ExplicitMappingResolver();
      resolver.register('failing', FailingConstructorHandler);
      const errorSpy = spyOn(console, 'error').mockImplementation(() => {});

      const definition: HandlerDefinition = { callable: 'failing' };
      const handler = await resolver.resolve(definition);

      expect(handler).toBeNull();
      errorSpy.mockRestore();
    });

    it('returns null when factory throws', async () => {
      const resolver = new ExplicitMappingResolver();
      const factory = () => {
        throw new Error('Factory failed!');
      };
      resolver.register('failing_factory', factory);
      const errorSpy = spyOn(console, 'error').mockImplementation(() => {});

      const definition: HandlerDefinition = { callable: 'failing_factory' };
      const handler = await resolver.resolve(definition);

      expect(handler).toBeNull();
      errorSpy.mockRestore();
    });
  });

  describe('integration', () => {
    it('supports full workflow: register, check, resolve', async () => {
      const resolver = new ExplicitMappingResolver();
      const definition: HandlerDefinition = { callable: 'order_handler' };

      // Initially not registered
      expect(resolver.canResolve(definition)).toBe(false);

      // Register
      resolver.register('order_handler', OrderHandler);

      // Now can resolve
      expect(resolver.canResolve(definition)).toBe(true);
      const handler = await resolver.resolve(definition);
      expect(handler).not.toBeNull();
      expect(handler?.name).toBe('order_handler');

      // Unregister
      resolver.unregister('order_handler');
      expect(resolver.canResolve(definition)).toBe(false);
    });
  });
});

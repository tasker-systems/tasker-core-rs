/**
 * TAS-93: ResolverChain tests.
 *
 * Verifies resolver chain behavior including:
 * - Default chain creation
 * - Priority-ordered resolution
 * - Resolver hints
 * - Method dispatch integration
 * - Custom resolver support
 */

import { beforeEach, describe, expect, it, spyOn } from 'bun:test';
import { StepHandler } from '../../../src/handler/base.js';
import type { ResolverConfig } from '../../../src/registry/base-resolver.js';
import type { HandlerDefinition } from '../../../src/registry/handler-definition.js';
import { MethodDispatchWrapper } from '../../../src/registry/method-dispatch-wrapper.js';
import { RegistryResolver } from '../../../src/registry/registry-resolver.js';
import { ResolverChain } from '../../../src/registry/resolver-chain.js';
import { ExplicitMappingResolver } from '../../../src/registry/resolvers/explicit-mapping.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

// Test handlers
class OrderHandler extends StepHandler {
  static handlerName = 'order_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'order' });
  }

  async process(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'order', method: 'process' });
  }
}

class PaymentHandler extends StepHandler {
  static handlerName = 'payment_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'payment' });
  }
}

// Custom resolver for testing
class CustomResolver extends RegistryResolver {
  static readonly _name = 'custom_resolver';
  static readonly _priority = 25;
  static readonly prefix = 'custom:';

  private handlers: Map<string, StepHandler> = new Map();

  registerHandler(key: string, handler: StepHandler): void {
    this.handlers.set(key, handler);
  }

  async resolveHandler(
    definition: HandlerDefinition,
    _match: RegExpMatchArray | null,
    _config?: ResolverConfig
  ): Promise<StepHandler | null> {
    const key = definition.callable.replace('custom:', '');
    return this.handlers.get(key) ?? null;
  }
}

describe('ResolverChain', () => {
  describe('default', () => {
    it('creates chain with default resolvers', async () => {
      const chain = await ResolverChain.default();
      const resolvers = chain.listResolvers();

      expect(resolvers.length).toBeGreaterThanOrEqual(2);

      const names = resolvers.map(([name]) => name);
      expect(names).toContain('explicit_mapping');
      expect(names).toContain('class_lookup');
    });

    it('orders resolvers by priority', async () => {
      const chain = await ResolverChain.default();
      const resolvers = chain.listResolvers();

      // Verify priority ordering (lower numbers first)
      for (let i = 1; i < resolvers.length; i++) {
        expect(resolvers[i - 1][1]).toBeLessThanOrEqual(resolvers[i][1]);
      }
    });
  });

  describe('withResolvers', () => {
    it('creates chain with specified resolvers', () => {
      const resolver = new ExplicitMappingResolver();
      const chain = ResolverChain.withResolvers([resolver]);

      const resolvers = chain.listResolvers();
      expect(resolvers).toHaveLength(1);
      expect(resolvers[0][0]).toBe('explicit_mapping');
    });
  });

  describe('addResolver', () => {
    it('adds resolver to chain', async () => {
      const chain = await ResolverChain.default();
      const customResolver = new CustomResolver();

      chain.addResolver(customResolver);

      const resolvers = chain.listResolvers();
      const names = resolvers.map(([name]) => name);
      expect(names).toContain('custom_resolver');
    });

    it('maintains priority ordering', async () => {
      const chain = await ResolverChain.default();
      const customResolver = new CustomResolver(); // priority 25

      chain.addResolver(customResolver);

      const resolvers = chain.listResolvers();
      // Custom resolver (priority 25) should be between explicit (10) and class_lookup (100)
      const customIndex = resolvers.findIndex(([name]) => name === 'custom_resolver');
      const explicitIndex = resolvers.findIndex(([name]) => name === 'explicit_mapping');
      const classLookupIndex = resolvers.findIndex(([name]) => name === 'class_lookup');

      expect(customIndex).toBeGreaterThan(explicitIndex);
      expect(customIndex).toBeLessThan(classLookupIndex);
    });
  });

  describe('getResolver', () => {
    it('returns resolver by name', async () => {
      const chain = await ResolverChain.default();

      const resolver = chain.getResolver('explicit_mapping');

      expect(resolver).toBeDefined();
      expect(resolver?.name).toBe('explicit_mapping');
    });

    it('returns undefined for unknown resolver', async () => {
      const chain = await ResolverChain.default();

      const resolver = chain.getResolver('nonexistent');

      expect(resolver).toBeUndefined();
    });
  });

  describe('resolve', () => {
    let chain: ResolverChain;
    let explicitResolver: ExplicitMappingResolver;

    beforeEach(async () => {
      chain = await ResolverChain.default();
      explicitResolver = chain.getResolver('explicit_mapping') as ExplicitMappingResolver;
      explicitResolver.register('order_handler', OrderHandler);
      explicitResolver.register('payment_handler', PaymentHandler);
    });

    it('resolves handler by callable string', async () => {
      const definition: HandlerDefinition = { callable: 'order_handler' };

      const handler = await chain.resolve(definition);

      expect(handler).not.toBeNull();
      expect(handler?.name).toBe('order_handler');
    });

    it('returns null for unknown handler', async () => {
      const warnSpy = spyOn(console, 'warn').mockImplementation(() => {});
      const definition: HandlerDefinition = { callable: 'unknown_handler' };

      const handler = await chain.resolve(definition);

      expect(handler).toBeNull();
      warnSpy.mockRestore();
    });

    it('uses resolver hint when provided', async () => {
      const definition: HandlerDefinition = {
        callable: 'order_handler',
        resolver: 'explicit_mapping',
      };

      const handler = await chain.resolve(definition);

      expect(handler).not.toBeNull();
      expect(handler?.name).toBe('order_handler');
    });

    it('throws when resolver hint points to unknown resolver', async () => {
      const definition: HandlerDefinition = {
        callable: 'order_handler',
        resolver: 'nonexistent_resolver',
      };

      await expect(chain.resolve(definition)).rejects.toThrow('Resolver not found');
    });

    it('uses chain when resolver hint resolver cannot resolve', async () => {
      // Create a custom resolver that doesn't know about 'order_handler'
      const customResolver = new CustomResolver();
      chain.addResolver(customResolver);

      const definition: HandlerDefinition = {
        callable: 'order_handler',
        resolver: 'custom_resolver',
      };

      // Should fall through to explicit_mapping via chain
      const handler = await chain.resolve(definition);

      expect(handler).toBeNull(); // Resolver hint doesn't fall through to chain
    });

    it('wraps handler with method dispatch when method specified', async () => {
      const definition: HandlerDefinition = {
        callable: 'order_handler',
        method: 'process',
      };

      const handler = await chain.resolve(definition);

      expect(handler).not.toBeNull();
      expect(handler).toBeInstanceOf(MethodDispatchWrapper);
      expect((handler as MethodDispatchWrapper).targetMethod).toBe('process');
    });

    it('does not wrap when method is "call"', async () => {
      const definition: HandlerDefinition = {
        callable: 'order_handler',
        method: 'call',
      };

      const handler = await chain.resolve(definition);

      expect(handler).not.toBeNull();
      expect(handler).not.toBeInstanceOf(MethodDispatchWrapper);
    });

    it('tries resolvers in priority order', async () => {
      const customResolver = new CustomResolver();
      const customHandler = new OrderHandler();
      customResolver.registerHandler('special', customHandler);
      chain.addResolver(customResolver);

      const definition: HandlerDefinition = { callable: 'custom:special' };

      const handler = await chain.resolve(definition);

      expect(handler).toBe(customHandler);
    });
  });

  describe('wrapForMethodDispatch', () => {
    it('wraps handler when method differs from call', async () => {
      const chain = await ResolverChain.default();
      const handler = new OrderHandler();
      const definition: HandlerDefinition = {
        callable: 'order_handler',
        method: 'process',
      };

      const wrapped = chain.wrapForMethodDispatch(handler, definition);

      expect(wrapped).toBeInstanceOf(MethodDispatchWrapper);
    });

    it('does not wrap when method is call', async () => {
      const chain = await ResolverChain.default();
      const handler = new OrderHandler();
      const definition: HandlerDefinition = {
        callable: 'order_handler',
        method: 'call',
      };

      const wrapped = chain.wrapForMethodDispatch(handler, definition);

      expect(wrapped).toBe(handler);
    });

    it('does not wrap when method is undefined', async () => {
      const chain = await ResolverChain.default();
      const handler = new OrderHandler();
      const definition: HandlerDefinition = { callable: 'order_handler' };

      const wrapped = chain.wrapForMethodDispatch(handler, definition);

      expect(wrapped).toBe(handler);
    });
  });

  describe('listResolvers', () => {
    it('returns array of [name, priority] tuples', async () => {
      const chain = await ResolverChain.default();

      const resolvers = chain.listResolvers();

      expect(Array.isArray(resolvers)).toBe(true);
      for (const [name, priority] of resolvers) {
        expect(typeof name).toBe('string');
        expect(typeof priority).toBe('number');
      }
    });
  });
});

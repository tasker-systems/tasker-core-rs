/**
 * TAS-93: RegistryResolver tests.
 *
 * Verifies the developer-friendly base class for custom resolvers.
 */

import { describe, expect, it } from 'bun:test';
import { StepHandler } from '../../../src/handler/base.js';
import type { ResolverConfig } from '../../../src/registry/base-resolver.js';
import type { HandlerDefinition } from '../../../src/registry/handler-definition.js';
import { RegistryResolver } from '../../../src/registry/registry-resolver.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

// Test handlers
class PaymentHandler extends StepHandler {
  static handlerName = 'payment_handler';
  provider: string;
  action: string;

  constructor(provider: string = 'stripe', action: string = 'charge') {
    super();
    this.provider = provider;
    this.action = action;
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ provider: this.provider, action: this.action });
  }
}

// Custom resolver using pattern matching
class PaymentResolver extends RegistryResolver {
  static readonly _name = 'payment_resolver';
  static readonly _priority = 20;
  static readonly pattern = /^payments:(?<provider>\w+):(?<action>\w+)$/;

  async resolveHandler(
    _definition: HandlerDefinition,
    match: RegExpMatchArray | null,
    _config?: ResolverConfig
  ): Promise<StepHandler | null> {
    if (!match?.groups) return null;

    const { provider, action } = match.groups;
    return new PaymentHandler(provider, action);
  }
}

// Custom resolver using prefix matching
class WebhookResolver extends RegistryResolver {
  static readonly _name = 'webhook_resolver';
  static readonly _priority = 30;
  static readonly prefix = 'webhook:';

  private handlers: Map<string, StepHandler> = new Map();

  registerHandler(key: string, handler: StepHandler): void {
    this.handlers.set(key, handler);
  }

  async resolveHandler(
    definition: HandlerDefinition,
    _match: RegExpMatchArray | null,
    _config?: ResolverConfig
  ): Promise<StepHandler | null> {
    const key = definition.callable.replace('webhook:', '');
    return this.handlers.get(key) ?? null;
  }
}

// Resolver without pattern or prefix
class FallbackResolver extends RegistryResolver {
  static readonly _name = 'fallback_resolver';
  static readonly _priority = 200;

  async resolveHandler(
    _definition: HandlerDefinition,
    _match: RegExpMatchArray | null,
    _config?: ResolverConfig
  ): Promise<StepHandler | null> {
    return null;
  }
}

// Resolver with only defaults
class DefaultsResolver extends RegistryResolver {
  async resolveHandler(
    _definition: HandlerDefinition,
    _match: RegExpMatchArray | null,
    _config?: ResolverConfig
  ): Promise<StepHandler | null> {
    return null;
  }
}

describe('RegistryResolver', () => {
  describe('static properties', () => {
    it('exposes name from static _name', () => {
      const resolver = new PaymentResolver();
      expect(resolver.name).toBe('payment_resolver');
    });

    it('exposes priority from static _priority', () => {
      const resolver = new PaymentResolver();
      expect(resolver.priority).toBe(20);
    });

    it('uses default name when _name not defined', () => {
      const resolver = new DefaultsResolver();
      expect(resolver.name).toBe('custom_resolver');
    });

    it('uses default priority when _priority not defined', () => {
      const resolver = new DefaultsResolver();
      expect(resolver.priority).toBe(50);
    });
  });

  describe('pattern matching', () => {
    it('matches callable against pattern', () => {
      const resolver = new PaymentResolver();

      const matchingDef: HandlerDefinition = { callable: 'payments:stripe:charge' };
      const nonMatchingDef: HandlerDefinition = { callable: 'other:handler' };

      expect(resolver.canResolve(matchingDef)).toBe(true);
      expect(resolver.canResolve(nonMatchingDef)).toBe(false);
    });

    it('passes match groups to resolveHandler', async () => {
      const resolver = new PaymentResolver();
      const definition: HandlerDefinition = { callable: 'payments:paypal:refund' };

      const handler = await resolver.resolve(definition);

      expect(handler).not.toBeNull();
      expect((handler as PaymentHandler).provider).toBe('paypal');
      expect((handler as PaymentHandler).action).toBe('refund');
    });

    it('extracts named groups from pattern', async () => {
      const resolver = new PaymentResolver();
      const definition: HandlerDefinition = { callable: 'payments:stripe:charge' };

      const handler = await resolver.resolve(definition);

      expect(handler).not.toBeNull();
      expect((handler as PaymentHandler).provider).toBe('stripe');
      expect((handler as PaymentHandler).action).toBe('charge');
    });
  });

  describe('prefix matching', () => {
    it('matches callable starting with prefix', () => {
      const resolver = new WebhookResolver();

      const matchingDef: HandlerDefinition = { callable: 'webhook:github' };
      const nonMatchingDef: HandlerDefinition = { callable: 'other:handler' };

      expect(resolver.canResolve(matchingDef)).toBe(true);
      expect(resolver.canResolve(nonMatchingDef)).toBe(false);
    });

    it('does not pass match to resolveHandler for prefix', async () => {
      const resolver = new WebhookResolver();
      const handler = new PaymentHandler();
      resolver.registerHandler('github', handler);

      const definition: HandlerDefinition = { callable: 'webhook:github' };
      const resolved = await resolver.resolve(definition);

      expect(resolved).toBe(handler);
    });
  });

  describe('without pattern or prefix', () => {
    it('canResolve returns false', () => {
      const resolver = new FallbackResolver();

      const definition: HandlerDefinition = { callable: 'any:handler' };

      expect(resolver.canResolve(definition)).toBe(false);
    });
  });

  describe('resolve', () => {
    it('calls resolveHandler with definition and match', async () => {
      const resolver = new PaymentResolver();
      const definition: HandlerDefinition = { callable: 'payments:stripe:charge' };

      const handler = await resolver.resolve(definition);

      expect(handler).not.toBeNull();
    });

    it('passes null match when no pattern', async () => {
      const resolver = new WebhookResolver();
      const handler = new PaymentHandler();
      resolver.registerHandler('test', handler);

      const definition: HandlerDefinition = { callable: 'webhook:test' };
      const resolved = await resolver.resolve(definition);

      expect(resolved).toBe(handler);
    });
  });

  describe('integration example: domain resolver', () => {
    // This demonstrates how a developer would create a domain-specific resolver
    class DomainResolver extends RegistryResolver {
      static readonly _name = 'domain_resolver';
      static readonly _priority = 15;
      static readonly pattern = /^(?<domain>orders|inventory|shipping):(?<action>\w+)$/;

      private domainHandlers: Map<string, Map<string, StepHandler>> = new Map();

      constructor() {
        super();
        // Initialize domain maps
        this.domainHandlers.set('orders', new Map());
        this.domainHandlers.set('inventory', new Map());
        this.domainHandlers.set('shipping', new Map());
      }

      registerDomainHandler(domain: string, action: string, handler: StepHandler): void {
        const domainMap = this.domainHandlers.get(domain);
        if (domainMap) {
          domainMap.set(action, handler);
        }
      }

      async resolveHandler(
        _definition: HandlerDefinition,
        match: RegExpMatchArray | null,
        _config?: ResolverConfig
      ): Promise<StepHandler | null> {
        if (!match?.groups) return null;

        const { domain, action } = match.groups;
        const domainMap = this.domainHandlers.get(domain);
        return domainMap?.get(action) ?? null;
      }
    }

    it('resolves handlers by domain and action', async () => {
      const resolver = new DomainResolver();
      const orderHandler = new PaymentHandler('orders', 'create');
      const inventoryHandler = new PaymentHandler('inventory', 'check');

      resolver.registerDomainHandler('orders', 'create', orderHandler);
      resolver.registerDomainHandler('inventory', 'check', inventoryHandler);

      const orderDef: HandlerDefinition = { callable: 'orders:create' };
      const inventoryDef: HandlerDefinition = { callable: 'inventory:check' };

      const resolvedOrder = await resolver.resolve(orderDef);
      const resolvedInventory = await resolver.resolve(inventoryDef);

      expect(resolvedOrder).toBe(orderHandler);
      expect(resolvedInventory).toBe(inventoryHandler);
    });

    it('returns null for unregistered domain action', async () => {
      const resolver = new DomainResolver();

      const definition: HandlerDefinition = { callable: 'orders:delete' };
      const handler = await resolver.resolve(definition);

      expect(handler).toBeNull();
    });
  });
});

/**
 * TAS-93: Developer-friendly base class for custom resolvers.
 *
 * Provides pattern and prefix matching capabilities for implementing
 * custom domain-specific resolvers.
 *
 * @example
 * ```typescript
 * class PaymentResolver extends RegistryResolver {
 *   static readonly _name = 'payment_resolver';
 *   static readonly _priority = 20;
 *   static readonly pattern = /^payments:(?<provider>\w+):(?<action>\w+)$/;
 *
 *   async resolveHandler(
 *     definition: HandlerDefinition,
 *     match: RegExpMatchArray | null
 *   ): Promise<StepHandler | null> {
 *     if (!match?.groups) return null;
 *
 *     const { provider, action } = match.groups;
 *     return PaymentHandlers.get(provider, action);
 *   }
 * }
 *
 * // Register with chain
 * chain.addResolver(new PaymentResolver());
 *
 * // Resolves: { callable: 'payments:stripe:refund' }
 * ```
 */

import type { StepHandler } from '../handler/base.js';
import type { BaseResolver, ResolverConfig } from './base-resolver.js';
import type { HandlerDefinition } from './handler-definition.js';

/**
 * Static configuration for RegistryResolver subclasses.
 */
export interface RegistryResolverStatic {
  /** Resolver name (required) */
  readonly _name: string;

  /** Priority in chain (default: 50) */
  readonly _priority?: number;

  /** Regex pattern to match callables (optional) */
  readonly pattern?: RegExp;

  /** String prefix to match callables (optional) */
  readonly prefix?: string;
}

/**
 * Developer-friendly base class for custom resolvers.
 *
 * Subclasses should:
 * 1. Set static `_name` and optionally `_priority`
 * 2. Set static `pattern` or `prefix` for matching
 * 3. Implement `resolveHandler()` for resolution logic
 */
export abstract class RegistryResolver implements BaseResolver {
  /**
   * Get the resolver name from static property.
   */
  get name(): string {
    const ctor = this.constructor as unknown as RegistryResolverStatic;
    return ctor._name ?? 'custom_resolver';
  }

  /**
   * Get the resolver priority from static property.
   */
  get priority(): number {
    const ctor = this.constructor as unknown as RegistryResolverStatic;
    return ctor._priority ?? 50;
  }

  /**
   * Check if this resolver can handle the definition.
   *
   * Uses pattern or prefix matching from static properties.
   *
   * @param definition - Handler definition
   * @param _config - Unused, part of interface
   * @returns True if callable matches
   */
  canResolve(definition: HandlerDefinition, _config?: ResolverConfig): boolean {
    const ctor = this.constructor as unknown as RegistryResolverStatic;
    const callable = definition.callable;

    if (ctor.pattern) {
      return ctor.pattern.test(callable);
    }

    if (ctor.prefix) {
      return callable.startsWith(ctor.prefix);
    }

    return false;
  }

  /**
   * Resolve the handler.
   *
   * Delegates to resolveHandler() with pattern match if applicable.
   *
   * @param definition - Handler definition
   * @param config - Resolver configuration
   * @returns Handler instance or null
   */
  async resolve(
    definition: HandlerDefinition,
    config?: ResolverConfig
  ): Promise<StepHandler | null> {
    const ctor = this.constructor as unknown as RegistryResolverStatic;
    const match = ctor.pattern ? definition.callable.match(ctor.pattern) : null;

    return this.resolveHandler(definition, match, config);
  }

  /**
   * Override this in subclasses for custom resolution logic.
   *
   * @param definition - Handler definition
   * @param match - Regex match result (if pattern was used)
   * @param config - Resolver configuration
   * @returns Handler instance or null
   */
  abstract resolveHandler(
    definition: HandlerDefinition,
    match: RegExpMatchArray | null,
    config?: ResolverConfig
  ): Promise<StepHandler | null>;
}

/**
 * TAS-93: Resolver chain for step handler resolution.
 *
 * Orchestrates multiple resolvers in priority order to find handlers.
 * Supports resolver hints to bypass the chain and method dispatch wrapping.
 *
 * @example
 * ```typescript
 * // Create chain with default resolvers
 * const chain = ResolverChain.default();
 *
 * // Register a handler
 * const explicit = chain.getResolver('explicit_mapping') as ExplicitMappingResolver;
 * explicit.register('my_handler', MyHandler);
 *
 * // Resolve handler
 * const definition: HandlerDefinition = { callable: 'my_handler' };
 * const handler = await chain.resolve(definition);
 *
 * // Resolve with method dispatch
 * const definition2: HandlerDefinition = {
 *   callable: 'my_handler',
 *   method: 'process',
 * };
 * const handler2 = await chain.resolve(definition2);
 * // handler2.call() will invoke handler.process()
 * ```
 */

import type { StepHandler } from '../handler/base.js';
import type { BaseResolver, ResolverConfig } from './base-resolver.js';
import { ResolverNotFoundError } from './errors.js';
import {
  effectiveMethod,
  type HandlerDefinition,
  hasResolverHint,
  usesMethodDispatch,
} from './handler-definition.js';
import { MethodDispatchWrapper } from './method-dispatch-wrapper.js';

// Lazy imports to avoid circular dependencies
let ExplicitMappingResolver: typeof import('./resolvers/explicit-mapping.js').ExplicitMappingResolver;
let ClassLookupResolver: typeof import('./resolvers/class-lookup.js').ClassLookupResolver;

/**
 * Priority-ordered chain of resolvers for handler resolution.
 */
export class ResolverChain {
  private resolvers: BaseResolver[] = [];
  private resolversByName: Map<string, BaseResolver> = new Map();

  /**
   * Create a resolver chain with default resolvers.
   *
   * Default resolvers:
   * - ExplicitMappingResolver (priority 10)
   * - ClassLookupResolver (priority 100)
   *
   * @returns Configured resolver chain
   */
  static async default(): Promise<ResolverChain> {
    const chain = new ResolverChain();

    // Lazy load to avoid circular dependencies
    if (!ExplicitMappingResolver) {
      const mod = await import('./resolvers/explicit-mapping.js');
      ExplicitMappingResolver = mod.ExplicitMappingResolver;
    }
    if (!ClassLookupResolver) {
      const mod = await import('./resolvers/class-lookup.js');
      ClassLookupResolver = mod.ClassLookupResolver;
    }

    chain.addResolver(new ExplicitMappingResolver());
    chain.addResolver(new ClassLookupResolver());

    return chain;
  }

  /**
   * Create a resolver chain synchronously with provided resolvers.
   *
   * Use this when you want to avoid async initialization.
   *
   * @param resolvers - Resolvers to add to the chain
   * @returns Configured resolver chain
   */
  static withResolvers(resolvers: BaseResolver[]): ResolverChain {
    const chain = new ResolverChain();
    for (const resolver of resolvers) {
      chain.addResolver(resolver);
    }
    return chain;
  }

  /**
   * Add a resolver to the chain.
   *
   * Resolvers are automatically sorted by priority (lowest first).
   *
   * @param resolver - Resolver to add
   */
  addResolver(resolver: BaseResolver): void {
    this.resolvers.push(resolver);
    this.resolversByName.set(resolver.name, resolver);
    this.sortResolvers();
  }

  /**
   * Get a resolver by name.
   *
   * @param name - Resolver name
   * @returns Resolver or undefined if not found
   */
  getResolver(name: string): BaseResolver | undefined {
    return this.resolversByName.get(name);
  }

  /**
   * List all resolvers with their priorities.
   *
   * @returns Array of [name, priority] tuples, sorted by priority
   */
  listResolvers(): Array<[string, number]> {
    return this.resolvers.map((r) => [r.name, r.priority]);
  }

  /**
   * Resolve a handler from a definition.
   *
   * Resolution process:
   * 1. If resolver hint is present, use only that resolver
   * 2. Otherwise, try resolvers in priority order
   * 3. If method dispatch is needed, wrap the handler
   *
   * @param definition - Handler definition to resolve
   * @param config - Optional resolver configuration
   * @returns Handler instance (possibly wrapped) or null
   */
  async resolve(
    definition: HandlerDefinition,
    config?: ResolverConfig
  ): Promise<StepHandler | null> {
    let handler: StepHandler | null;

    if (hasResolverHint(definition)) {
      handler = await this.resolveWithHint(definition, config);
    } else {
      handler = await this.resolveWithChain(definition, config);
    }

    if (!handler) {
      return null;
    }

    return this.wrapForMethodDispatch(handler, definition);
  }

  /**
   * Wrap a handler for method dispatch if needed.
   *
   * @param handler - Handler to potentially wrap
   * @param definition - Handler definition with method info
   * @returns Original handler or wrapped handler
   */
  wrapForMethodDispatch(handler: StepHandler, definition: HandlerDefinition): StepHandler {
    if (!usesMethodDispatch(definition)) {
      return handler;
    }

    const method = effectiveMethod(definition);

    // Check if handler has the method
    const handlerWithMethod = handler as unknown as Record<string, unknown>;
    if (typeof handlerWithMethod[method] !== 'function') {
      console.warn(`[ResolverChain] Handler '${handler.name}' does not have method '${method}'`);
      return handler;
    }

    // Cast to StepHandler since MethodDispatchWrapper is structurally compatible
    return new MethodDispatchWrapper(handler, method) as unknown as StepHandler;
  }

  /**
   * Resolve using a specific resolver (from hint).
   */
  private async resolveWithHint(
    definition: HandlerDefinition,
    config?: ResolverConfig
  ): Promise<StepHandler | null> {
    // Resolver hint is guaranteed to exist when this method is called
    // (checked by hasResolverHint), but we handle the edge case safely
    const resolverName = definition.resolver ?? '';
    const resolver = this.resolversByName.get(resolverName);

    if (!resolver) {
      throw new ResolverNotFoundError(resolverName);
    }

    return resolver.resolve(definition, config);
  }

  /**
   * Resolve by trying resolvers in priority order.
   */
  private async resolveWithChain(
    definition: HandlerDefinition,
    config?: ResolverConfig
  ): Promise<StepHandler | null> {
    for (const resolver of this.resolvers) {
      if (resolver.canResolve(definition, config)) {
        const handler = await resolver.resolve(definition, config);
        if (handler) {
          return handler;
        }
      }
    }
    return null;
  }

  /**
   * Sort resolvers by priority (ascending).
   */
  private sortResolvers(): void {
    this.resolvers.sort((a, b) => a.priority - b.priority);
  }
}

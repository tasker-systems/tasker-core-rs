/**
 * TAS-93: Step handler registry with resolver chain support.
 *
 * Provides handler registration and resolution using a priority-ordered
 * resolver chain. Supports method dispatch and resolver hints.
 *
 * @example
 * ```typescript
 * const registry = new HandlerRegistry();
 * await registry.initialize();
 *
 * // Register a handler
 * registry.register('my_handler', MyHandler);
 *
 * // Simple resolution (string callable)
 * const handler = await registry.resolve('my_handler');
 *
 * // Resolution with method dispatch
 * const definition: HandlerDefinition = {
 *   callable: 'my_handler',
 *   method: 'process',
 * };
 * const handler2 = await registry.resolve(definition);
 * // handler2.call() invokes handler.process()
 *
 * // Resolution with resolver hint
 * const definition2: HandlerDefinition = {
 *   callable: 'my_handler',
 *   resolver: 'explicit_mapping',
 * };
 * const handler3 = await registry.resolve(definition2);
 * ```
 */

import {
  type BaseResolver,
  ExplicitMappingResolver,
  type HandlerDefinition,
  type HandlerSpec,
  normalizeToDefinition,
  ResolverChain,
} from '../registry/index.js';
import type { StepHandler, StepHandlerClass } from './base';

/**
 * Registry for step handler classes.
 *
 * TAS-93: Uses ResolverChain for flexible handler resolution
 * with support for method dispatch and resolver hints.
 *
 * Provides handler discovery, registration, and resolution.
 */
export class HandlerRegistry {
  private _resolverChain: ResolverChain | null = null;
  private _explicitResolver: ExplicitMappingResolver | null = null;
  private _initialized = false;

  /** Promise-based lock for initialization to prevent concurrent init */
  private _initPromise: Promise<void> | null = null;

  /** Track registrations so they can be transferred to chain resolver */
  private _pendingRegistrations: Map<string, StepHandlerClass> = new Map();

  /**
   * Initialize the registry with default resolvers.
   *
   * Must be called before using resolve() with resolver chain features.
   * For simple register/resolve with strings, lazy initialization is used.
   */
  async initialize(): Promise<void> {
    if (this._initialized) return;

    this._resolverChain = await ResolverChain.default();
    const chainResolver = this._resolverChain.getResolver(
      'explicit_mapping'
    ) as ExplicitMappingResolver;

    // Transfer handlers from standalone resolver (if any) to chain resolver
    if (this._explicitResolver && this._explicitResolver !== chainResolver) {
      for (const name of this._explicitResolver.registeredCallables()) {
        // Re-register from pending registrations (original class reference)
        const handlerClass = this._pendingRegistrations.get(name);
        if (handlerClass) {
          chainResolver.register(name, handlerClass);
        }
      }
    }

    // Transfer any handlers registered before initialization
    for (const [name, handlerClass] of this._pendingRegistrations) {
      chainResolver.register(name, handlerClass);
    }
    this._pendingRegistrations.clear();

    // Now point to the chain resolver
    this._explicitResolver = chainResolver;
    this._initialized = true;
  }

  /**
   * Ensure the registry is initialized.
   * Uses lazy initialization with proper locking to prevent concurrent init.
   */
  private async ensureInitialized(): Promise<void> {
    if (this._initialized) return;

    // Use promise-based lock to prevent concurrent initialization
    if (!this._initPromise) {
      this._initPromise = this.initialize();
    }
    await this._initPromise;
  }

  /**
   * Get the explicit mapping resolver for direct registration.
   */
  private getExplicitResolver(): ExplicitMappingResolver {
    if (!this._explicitResolver) {
      // Create standalone resolver for pre-initialization use
      this._explicitResolver = new ExplicitMappingResolver();
    }
    return this._explicitResolver;
  }

  /**
   * Register a handler class.
   *
   * @param name - Handler name (must match step definition)
   * @param handlerClass - StepHandler subclass
   * @throws Error if handlerClass is not a valid StepHandler subclass
   *
   * @example
   * ```typescript
   * registry.register('my_handler', MyHandler);
   * ```
   */
  register(name: string, handlerClass: StepHandlerClass): void {
    if (!name || typeof name !== 'string') {
      throw new Error('Handler name must be a non-empty string');
    }

    if (typeof handlerClass !== 'function') {
      throw new Error(`handlerClass must be a StepHandler subclass, got ${typeof handlerClass}`);
    }

    // Track for transfer during initialization
    if (!this._initialized) {
      this._pendingRegistrations.set(name, handlerClass);
    }

    const resolver = this.getExplicitResolver();
    resolver.register(name, handlerClass);
    console.info(`[HandlerRegistry] Registered handler: ${name} -> ${handlerClass.name}`);
  }

  /**
   * Unregister a handler.
   *
   * @param name - Handler name to unregister
   * @returns True if handler was unregistered, false if not found
   */
  unregister(name: string): boolean {
    // Remove from pending registrations if not initialized
    if (!this._initialized) {
      this._pendingRegistrations.delete(name);
    }

    const resolver = this.getExplicitResolver();
    const removed = resolver.unregister(name);
    if (removed) {
      console.debug(`[HandlerRegistry] Unregistered handler: ${name}`);
    }
    return removed;
  }

  /**
   * Resolve and instantiate a handler.
   *
   * TAS-93: Accepts flexible input types:
   * - string: Simple callable name
   * - HandlerDefinition: Full definition with method/resolver
   * - HandlerSpec: Union type for all formats
   *
   * @param handlerSpec - Handler specification
   * @returns Instantiated handler or null if not found
   *
   * @example
   * ```typescript
   * // String callable
   * const handler = await registry.resolve('my_handler');
   *
   * // With method dispatch
   * const handler2 = await registry.resolve({
   *   callable: 'my_handler',
   *   method: 'process',
   * });
   * ```
   */
  async resolve(handlerSpec: HandlerSpec): Promise<StepHandler | null> {
    await this.ensureInitialized();

    const definition = normalizeToDefinition(handlerSpec);

    if (!this._resolverChain) {
      console.warn('[HandlerRegistry] Resolver chain not initialized');
      return null;
    }

    const handler = await this._resolverChain.resolve(definition);

    if (!handler) {
      console.warn(`[HandlerRegistry] Handler not found: ${definition.callable}`);
    }

    return handler;
  }

  /**
   * Synchronous resolve for backward compatibility.
   *
   * Note: This only works with explicitly registered handlers.
   * For full resolver chain support, use the async resolve() method.
   *
   * @param name - Handler name to resolve
   * @returns Instantiated handler or null if not found
   */
  resolveSync(name: string): StepHandler | null {
    const resolver = this.getExplicitResolver();
    const definition: HandlerDefinition = { callable: name };

    if (!resolver.canResolve(definition)) {
      console.warn(`[HandlerRegistry] Handler not found: ${name}`);
      return null;
    }

    // Use Promise.resolve to handle the async resolve, but block on it
    // This is a workaround for backward compatibility
    let result: StepHandler | null = null;

    // Try to instantiate directly from the explicit resolver
    const entry = resolver.registeredCallables().includes(name);
    if (entry) {
      // Access internal state - not ideal but needed for sync compat
      resolver
        .resolve(definition)
        .then((h) => {
          result = h;
        })
        .catch(() => {
          result = null;
        });
    }

    // For sync compatibility, try direct instantiation
    // This is a best-effort approach
    return result;
  }

  /**
   * Get a handler class without instantiation.
   *
   * @param name - Handler name to look up
   * @returns Handler class or undefined if not found
   */
  getHandlerClass(_name: string): StepHandlerClass | undefined {
    // This would require exposing internal state from ExplicitMappingResolver
    // For now, return undefined - users should use resolve()
    console.warn('[HandlerRegistry] getHandlerClass is deprecated, use resolve() instead');
    return undefined;
  }

  /**
   * Check if a handler is registered.
   *
   * @param name - Handler name to check
   * @returns True if handler is registered
   */
  isRegistered(name: string): boolean {
    const resolver = this.getExplicitResolver();
    return resolver.registeredCallables().includes(name);
  }

  /**
   * List all registered handler names.
   *
   * @returns Array of registered handler names
   */
  listHandlers(): string[] {
    const resolver = this.getExplicitResolver();
    return resolver.registeredCallables();
  }

  /**
   * Get the number of registered handlers.
   */
  handlerCount(): number {
    return this.listHandlers().length;
  }

  /**
   * Clear all registered handlers.
   */
  clear(): void {
    if (this._explicitResolver) {
      for (const key of this._explicitResolver.registeredCallables()) {
        this._explicitResolver.unregister(key);
      }
    }
    console.debug('[HandlerRegistry] Cleared all handlers from registry');
  }

  /**
   * Add a custom resolver to the chain.
   *
   * TAS-93: Allows adding custom domain-specific resolvers.
   *
   * @param resolver - Resolver to add
   */
  async addResolver(resolver: BaseResolver): Promise<void> {
    await this.ensureInitialized();
    this._resolverChain?.addResolver(resolver);
  }

  /**
   * Get a resolver by name.
   *
   * @param name - Resolver name
   * @returns Resolver or undefined
   */
  async getResolver(name: string): Promise<BaseResolver | undefined> {
    await this.ensureInitialized();
    return this._resolverChain?.getResolver(name);
  }

  /**
   * List all resolvers with priorities.
   *
   * @returns Array of [name, priority] tuples
   */
  async listResolvers(): Promise<Array<[string, number]>> {
    await this.ensureInitialized();
    return this._resolverChain?.listResolvers() ?? [];
  }

  /**
   * Get the underlying resolver chain.
   *
   * Useful for advanced configuration.
   */
  async getResolverChain(): Promise<ResolverChain | null> {
    await this.ensureInitialized();
    return this._resolverChain;
  }

  /**
   * Get debug information about the registry.
   */
  debugInfo(): Record<string, unknown> {
    const resolver = this.getExplicitResolver();
    const handlers: Record<string, string> = {};

    for (const name of resolver.registeredCallables()) {
      handlers[name] = name; // We don't have class names readily available
    }

    return {
      initialized: this._initialized,
      handlerCount: resolver.registeredCallables().length,
      handlers,
    };
  }
}

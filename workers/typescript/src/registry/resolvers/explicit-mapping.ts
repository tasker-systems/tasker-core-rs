/**
 * TAS-93: Explicit mapping resolver (priority 10).
 *
 * Resolves handlers from explicitly registered mappings.
 * This is the highest priority resolver in the default chain.
 *
 * Supports registering:
 * - Handler classes (instantiated on resolve)
 * - Handler instances (returned directly)
 * - Factory functions (called with config on resolve)
 *
 * @example
 * ```typescript
 * const resolver = new ExplicitMappingResolver();
 *
 * // Register a class
 * resolver.register('my_handler', MyHandler);
 *
 * // Register an instance
 * resolver.register('shared_handler', new SharedHandler());
 *
 * // Register a factory
 * resolver.register('configurable_handler', (config) => new ConfigHandler(config));
 *
 * // Resolve
 * const definition: HandlerDefinition = { callable: 'my_handler' };
 * const handler = await resolver.resolve(definition);
 * ```
 */

import type { StepHandler, StepHandlerClass } from '../../handler/base.js';
import type { BaseResolver, ResolverConfig } from '../base-resolver.js';
import type { HandlerDefinition } from '../handler-definition.js';

/**
 * Factory function for creating handlers.
 */
export type HandlerFactory = (config: ResolverConfig) => StepHandler;

/**
 * Entry types that can be registered.
 */
export type HandlerEntry = StepHandlerClass | StepHandler | HandlerFactory;

/**
 * Resolver for explicitly registered handlers.
 *
 * Priority 10 - checked first in the default chain.
 */
export class ExplicitMappingResolver implements BaseResolver {
  readonly name: string;
  readonly priority = 10;

  private handlers: Map<string, HandlerEntry> = new Map();

  /**
   * Create an explicit mapping resolver.
   *
   * @param name - Resolver name (default: 'explicit_mapping')
   */
  constructor(name = 'explicit_mapping') {
    this.name = name;
  }

  /**
   * Check if a handler is registered for this callable.
   *
   * @param definition - Handler definition
   * @param _config - Unused, part of interface
   * @returns True if handler is registered
   */
  canResolve(definition: HandlerDefinition, _config?: ResolverConfig): boolean {
    return this.handlers.has(definition.callable);
  }

  /**
   * Resolve and instantiate a registered handler.
   *
   * @param definition - Handler definition
   * @param config - Configuration passed to factories
   * @returns Handler instance or null if not registered
   */
  async resolve(
    definition: HandlerDefinition,
    config?: ResolverConfig
  ): Promise<StepHandler | null> {
    const entry = this.handlers.get(definition.callable);
    if (!entry) {
      return null;
    }

    return this.instantiateHandler(entry, definition, config ?? {});
  }

  /**
   * Register a handler.
   *
   * @param key - Handler identifier (matched against definition.callable)
   * @param handler - Handler class, instance, or factory function
   */
  register(key: string, handler: HandlerEntry): void {
    this.handlers.set(key, handler);
  }

  /**
   * Unregister a handler.
   *
   * @param key - Handler identifier to remove
   * @returns True if handler was removed, false if not found
   */
  unregister(key: string): boolean {
    return this.handlers.delete(key);
  }

  /**
   * Get all registered callable keys.
   *
   * @returns Array of registered keys
   */
  registeredCallables(): string[] {
    return Array.from(this.handlers.keys());
  }

  /**
   * Instantiate a handler from a registered entry.
   */
  private instantiateHandler(
    entry: HandlerEntry,
    definition: HandlerDefinition,
    config: ResolverConfig
  ): StepHandler | null {
    // Check if it's a class (has prototype and is a function)
    if (this.isHandlerClass(entry)) {
      return this.instantiateClass(entry, definition);
    }

    // Check if it's a factory function (function but not a class)
    if (typeof entry === 'function') {
      try {
        return entry(config);
      } catch (error) {
        console.error(`[ExplicitMappingResolver] Factory failed:`, error);
        return null;
      }
    }

    // Otherwise it's an instance - return directly
    return entry as StepHandler;
  }

  /**
   * Check if entry is a handler class.
   */
  private isHandlerClass(entry: HandlerEntry): entry is StepHandlerClass {
    return typeof entry === 'function' && entry.prototype !== undefined && 'handlerName' in entry;
  }

  /**
   * Instantiate a handler class.
   */
  private instantiateClass(
    handlerClass: StepHandlerClass,
    _definition: HandlerDefinition
  ): StepHandler | null {
    try {
      return new handlerClass();
    } catch (error) {
      console.error(`[ExplicitMappingResolver] Failed to instantiate ${handlerClass.name}:`, error);
      return null;
    }
  }
}

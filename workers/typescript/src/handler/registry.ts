import type { StepHandler, StepHandlerClass } from './base';

/**
 * Registry for step handler classes.
 *
 * Provides handler registration and resolution using the singleton pattern
 * for global handler management.
 *
 * Matches Python's HandlerRegistry and Ruby's HandlerRegistry.
 *
 * @example
 * ```typescript
 * // Get singleton instance
 * const registry = HandlerRegistry.instance();
 *
 * // Register a handler
 * registry.register('my_handler', MyHandler);
 *
 * // Check if registered
 * if (registry.isRegistered('my_handler')) {
 *   // Resolve and get instance
 *   const handler = registry.resolve('my_handler');
 *   if (handler) {
 *     const result = await handler.call(context);
 *   }
 * }
 *
 * // List all handlers
 * const handlers = registry.listHandlers();
 * console.log('Registered handlers:', handlers);
 * ```
 */
export class HandlerRegistry {
  private static _instance: HandlerRegistry | null = null;

  private _handlers: Map<string, StepHandlerClass> = new Map();

  /**
   * Private constructor - use instance() to get singleton.
   */
  private constructor() {}

  /**
   * Get the singleton registry instance.
   *
   * @returns The singleton HandlerRegistry instance
   *
   * @example
   * ```typescript
   * const registry = HandlerRegistry.instance();
   * ```
   */
  static instance(): HandlerRegistry {
    if (!HandlerRegistry._instance) {
      HandlerRegistry._instance = new HandlerRegistry();
    }
    return HandlerRegistry._instance;
  }

  /**
   * Reset the singleton instance.
   *
   * This is primarily for testing to ensure a clean state between tests.
   *
   * @example
   * ```typescript
   * // In test setup
   * HandlerRegistry.resetInstance();
   * ```
   */
  static resetInstance(): void {
    HandlerRegistry._instance = null;
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

    // Validate it's a constructor function
    if (typeof handlerClass !== 'function') {
      throw new Error(`handlerClass must be a StepHandler subclass, got ${typeof handlerClass}`);
    }

    // Log warning if overwriting
    if (this._handlers.has(name)) {
      console.warn(`[HandlerRegistry] Overwriting existing handler: ${name}`);
    }

    this._handlers.set(name, handlerClass);
    console.info(`[HandlerRegistry] Registered handler: ${name} -> ${handlerClass.name}`);
  }

  /**
   * Unregister a handler.
   *
   * @param name - Handler name to unregister
   * @returns True if handler was unregistered, false if not found
   *
   * @example
   * ```typescript
   * if (registry.unregister('old_handler')) {
   *   console.log('Handler removed');
   * }
   * ```
   */
  unregister(name: string): boolean {
    if (this._handlers.has(name)) {
      this._handlers.delete(name);
      console.debug(`[HandlerRegistry] Unregistered handler: ${name}`);
      return true;
    }
    return false;
  }

  /**
   * Resolve and instantiate a handler by name.
   *
   * @param name - Handler name to resolve
   * @returns Instantiated handler or null if not found or instantiation fails
   *
   * @example
   * ```typescript
   * const handler = registry.resolve('my_handler');
   * if (handler) {
   *   const result = await handler.call(context);
   * }
   * ```
   */
  resolve(name: string): StepHandler | null {
    const handlerClass = this._handlers.get(name);
    if (!handlerClass) {
      console.warn(`[HandlerRegistry] Handler not found: ${name}`);
      return null;
    }

    try {
      return new handlerClass();
    } catch (error) {
      console.error(`[HandlerRegistry] Failed to instantiate handler ${name}:`, error);
      return null;
    }
  }

  /**
   * Get a handler class without instantiation.
   *
   * @param name - Handler name to look up
   * @returns Handler class or undefined if not found
   *
   * @example
   * ```typescript
   * const handlerClass = registry.getHandlerClass('my_handler');
   * if (handlerClass) {
   *   console.log(`Handler version: ${handlerClass.handlerVersion}`);
   * }
   * ```
   */
  getHandlerClass(name: string): StepHandlerClass | undefined {
    return this._handlers.get(name);
  }

  /**
   * Check if a handler is registered.
   *
   * @param name - Handler name to check
   * @returns True if handler is registered
   *
   * @example
   * ```typescript
   * if (registry.isRegistered('my_handler')) {
   *   const handler = registry.resolve('my_handler');
   * }
   * ```
   */
  isRegistered(name: string): boolean {
    return this._handlers.has(name);
  }

  /**
   * List all registered handler names.
   *
   * @returns Array of registered handler names
   *
   * @example
   * ```typescript
   * const handlers = registry.listHandlers();
   * console.log(`Registered handlers: ${handlers.join(', ')}`);
   * ```
   */
  listHandlers(): string[] {
    return Array.from(this._handlers.keys());
  }

  /**
   * Get the number of registered handlers.
   *
   * @returns Number of registered handlers
   */
  handlerCount(): number {
    return this._handlers.size;
  }

  /**
   * Clear all registered handlers.
   *
   * Primarily for testing.
   */
  clear(): void {
    this._handlers.clear();
    console.debug('[HandlerRegistry] Cleared all handlers from registry');
  }

  /**
   * Get debug information about the registry.
   *
   * @returns Object with registry state
   */
  debugInfo(): Record<string, unknown> {
    const handlers: Record<string, string> = {};
    for (const [name, handlerClass] of this._handlers) {
      handlers[name] = handlerClass.name;
    }

    return {
      handlerCount: this._handlers.size,
      handlers,
    };
  }
}

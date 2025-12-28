/**
 * HandlerSystem - Owns handler registration and discovery.
 *
 * This class encapsulates handler management:
 * - Owns the HandlerRegistry (no singleton)
 * - Provides handler discovery from directories
 * - Manages handler registration lifecycle
 *
 * Design principles:
 * - Explicit construction: No singleton pattern
 * - Clear ownership: Owns the registry instance
 * - Encapsulated discovery: Handler loading logic contained here
 */

import { existsSync } from 'node:fs';
import { readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { createLogger } from '../logging/index.js';
import type { StepHandler, StepHandlerClass } from './base.js';

const log = createLogger({ component: 'handler-system' });

/**
 * Internal registry for step handler classes.
 *
 * This is an internal implementation used by HandlerSystem.
 * Unlike HandlerRegistry, it's not a singleton.
 */
class InternalRegistry {
  private handlers: Map<string, StepHandlerClass> = new Map();

  register(name: string, handlerClass: StepHandlerClass): void {
    if (!name || typeof name !== 'string') {
      throw new Error('Handler name must be a non-empty string');
    }

    if (typeof handlerClass !== 'function') {
      throw new Error(`handlerClass must be a StepHandler subclass, got ${typeof handlerClass}`);
    }

    if (this.handlers.has(name)) {
      log.warn(`Overwriting existing handler: ${name}`, { operation: 'register' });
    }

    this.handlers.set(name, handlerClass);
    log.info(`Registered handler: ${name} -> ${handlerClass.name}`, { operation: 'register' });
  }

  unregister(name: string): boolean {
    if (this.handlers.has(name)) {
      this.handlers.delete(name);
      log.debug(`Unregistered handler: ${name}`, { operation: 'unregister' });
      return true;
    }
    return false;
  }

  resolve(name: string): StepHandler | null {
    const handlerClass = this.handlers.get(name);
    if (!handlerClass) {
      log.warn(`Handler not found: ${name}`, { operation: 'resolve' });
      return null;
    }

    try {
      return new handlerClass();
    } catch (error) {
      log.error(`Failed to instantiate handler ${name}: ${error}`, { operation: 'resolve' });
      return null;
    }
  }

  getHandlerClass(name: string): StepHandlerClass | undefined {
    return this.handlers.get(name);
  }

  isRegistered(name: string): boolean {
    return this.handlers.has(name);
  }

  listHandlers(): string[] {
    return Array.from(this.handlers.keys());
  }

  handlerCount(): number {
    return this.handlers.size;
  }

  clear(): void {
    this.handlers.clear();
    log.debug('Cleared all handlers from registry', { operation: 'clear' });
  }
}

/**
 * Configuration for HandlerSystem.
 */
export interface HandlerSystemConfig {
  /** Path to directory containing handlers (can also use TYPESCRIPT_HANDLER_PATH env var) */
  handlerPath?: string;
}

/**
 * Owns handler registration and discovery.
 *
 * Unlike the singleton HandlerRegistry, this class:
 * - Is NOT a singleton - created and passed explicitly
 * - Owns the registry instance directly
 * - Encapsulates handler discovery logic
 *
 * @example
 * ```typescript
 * const handlerSystem = new HandlerSystem();
 *
 * // Register individual handlers
 * handlerSystem.register('my_handler', MyHandler);
 *
 * // Or load from directory
 * await handlerSystem.loadFromPath('./handlers');
 *
 * // Resolve handler
 * const handler = handlerSystem.resolve('my_handler');
 * ```
 */
export class HandlerSystem {
  private readonly registry: InternalRegistry;

  /**
   * Create a new HandlerSystem.
   */
  constructor() {
    this.registry = new InternalRegistry();
  }

  /**
   * Load handlers from a directory path.
   *
   * Searches for handler modules in the directory and registers them.
   * Supports both index file exports and directory scanning.
   *
   * @param path - Path to directory containing handlers
   * @returns Number of handlers loaded
   */
  async loadFromPath(path: string): Promise<number> {
    if (!existsSync(path)) {
      log.warn(`Handler path does not exist: ${path}`, { operation: 'load_from_path' });
      return 0;
    }

    log.info(`Loading handlers from: ${path}`, { operation: 'load_from_path' });

    try {
      // Try to find and import an index file first
      const indexResult = await this.tryImportIndexFile(path);
      if (indexResult.module) {
        return this.registerHandlersFromModule(indexResult.module, indexResult.path);
      }

      // Fallback: scan for handler files
      log.debug('No index file found, scanning for handler files...', {
        operation: 'load_from_path',
      });
      return this.scanAndImportHandlers(path);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      log.error(`Failed to load handlers from path: ${errorMessage}`, {
        operation: 'load_from_path',
        error_message: errorMessage,
      });
      return 0;
    }
  }

  /**
   * Load handlers from TYPESCRIPT_HANDLER_PATH environment variable.
   *
   * @returns Number of handlers loaded
   */
  async loadFromEnv(): Promise<number> {
    const handlerPath = process.env.TYPESCRIPT_HANDLER_PATH;
    if (!handlerPath) {
      log.debug('TYPESCRIPT_HANDLER_PATH not set, skipping handler import', {
        operation: 'load_from_env',
      });
      return 0;
    }

    return this.loadFromPath(handlerPath);
  }

  /**
   * Register a handler class.
   *
   * @param name - Handler name (must match step definition)
   * @param handlerClass - StepHandler subclass
   */
  register(name: string, handlerClass: StepHandlerClass): void {
    this.registry.register(name, handlerClass);
  }

  /**
   * Unregister a handler.
   *
   * @param name - Handler name to unregister
   * @returns True if handler was unregistered
   */
  unregister(name: string): boolean {
    return this.registry.unregister(name);
  }

  /**
   * Resolve and instantiate a handler by name.
   *
   * @param name - Handler name to resolve
   * @returns Instantiated handler or null if not found
   */
  resolve(name: string): StepHandler | null {
    return this.registry.resolve(name);
  }

  /**
   * Get a handler class without instantiation.
   *
   * @param name - Handler name to look up
   * @returns Handler class or undefined
   */
  getHandlerClass(name: string): StepHandlerClass | undefined {
    return this.registry.getHandlerClass(name);
  }

  /**
   * Check if a handler is registered.
   */
  isRegistered(name: string): boolean {
    return this.registry.isRegistered(name);
  }

  /**
   * List all registered handler names.
   */
  listHandlers(): string[] {
    return this.registry.listHandlers();
  }

  /**
   * Get the number of registered handlers.
   */
  handlerCount(): number {
    return this.registry.handlerCount();
  }

  /**
   * Clear all registered handlers.
   */
  clear(): void {
    this.registry.clear();
  }

  /**
   * Get a compatible registry interface.
   *
   * Returns an object that conforms to the registry interface expected
   * by other components (StepExecutionSubscriber, etc.).
   */
  getRegistry(): {
    resolve(name: string): StepHandler | null;
    isRegistered(name: string): boolean;
    listHandlers(): string[];
  } {
    return {
      resolve: (name: string) => this.registry.resolve(name),
      isRegistered: (name: string) => this.registry.isRegistered(name),
      listHandlers: () => this.registry.listHandlers(),
    };
  }

  // ==========================================================================
  // Private Methods
  // ==========================================================================

  /**
   * Try to import an index file from the handler path.
   */
  private async tryImportIndexFile(
    handlerPath: string
  ): Promise<{ module: Record<string, unknown> | null; path: string | null }> {
    const indexPaths = [
      join(handlerPath, 'examples', 'index.js'),
      join(handlerPath, 'examples', 'index.ts'),
      join(handlerPath, 'index.js'),
      join(handlerPath, 'index.ts'),
    ];

    for (const indexPath of indexPaths) {
      if (existsSync(indexPath)) {
        const module = (await import(`file://${indexPath}`)) as Record<string, unknown>;
        return { module, path: indexPath };
      }
    }

    return { module: null, path: null };
  }

  /**
   * Register handlers from a module's exports.
   */
  private registerHandlersFromModule(
    module: Record<string, unknown>,
    importPath: string | null
  ): number {
    log.info(`Loaded handler module from: ${importPath}`, { operation: 'import_handlers' });

    // Check for ALL_EXAMPLE_HANDLERS array (preferred pattern)
    if (Array.isArray(module.ALL_EXAMPLE_HANDLERS)) {
      return this.registerFromHandlerArray(module.ALL_EXAMPLE_HANDLERS);
    }

    // Fallback: scan module exports for handler classes
    return this.registerFromModuleExports(module);
  }

  /**
   * Register handlers from ALL_EXAMPLE_HANDLERS array.
   */
  private registerFromHandlerArray(handlers: unknown[]): number {
    let count = 0;
    for (const handlerClass of handlers) {
      if (this.isValidHandlerClass(handlerClass)) {
        this.registry.register(handlerClass.handlerName, handlerClass);
        count++;
      }
    }
    log.info(`Registered ${count} handlers from ALL_EXAMPLE_HANDLERS`, {
      operation: 'import_handlers',
    });
    return count;
  }

  /**
   * Register handlers from module exports.
   */
  private registerFromModuleExports(module: Record<string, unknown>): number {
    let count = 0;
    for (const [exportName, exported] of Object.entries(module)) {
      if (this.isValidHandlerClass(exported)) {
        this.registry.register(exported.handlerName, exported);
        count++;
        log.debug(`Registered handler from export: ${exportName}`, {
          operation: 'import_handlers',
        });
      }
    }
    log.info(`Registered ${count} handlers from module exports`, {
      operation: 'import_handlers',
    });
    return count;
  }

  /**
   * Check if a value is a valid handler class.
   */
  private isValidHandlerClass(value: unknown): value is StepHandlerClass {
    return (
      value !== null &&
      typeof value === 'function' &&
      'handlerName' in value &&
      typeof (value as StepHandlerClass).handlerName === 'string'
    );
  }

  /**
   * Recursively scan a directory for handler files and import them.
   */
  private async scanAndImportHandlers(dirPath: string): Promise<number> {
    let count = 0;

    try {
      const entries = await readdir(dirPath, { withFileTypes: true });

      for (const entry of entries) {
        const fullPath = join(dirPath, entry.name);
        count += await this.processDirectoryEntry(entry, fullPath);
      }
    } catch (error) {
      log.debug(`Could not scan ${dirPath}: ${error}`, { operation: 'import_handlers' });
    }

    return count;
  }

  /**
   * Process a single directory entry for handler import.
   */
  private async processDirectoryEntry(
    entry: { isDirectory(): boolean; isFile(): boolean; name: string },
    fullPath: string
  ): Promise<number> {
    if (this.shouldScanDirectory(entry)) {
      return this.scanAndImportHandlers(fullPath);
    }

    if (this.isHandlerFile(entry)) {
      return this.importHandlerFile(fullPath);
    }

    return 0;
  }

  /**
   * Check if a directory should be scanned for handlers.
   */
  private shouldScanDirectory(entry: { isDirectory(): boolean; name: string }): boolean {
    return entry.isDirectory() && !entry.name.startsWith('_') && entry.name !== 'node_modules';
  }

  /**
   * Check if a file might contain handlers.
   */
  private isHandlerFile(entry: { isFile(): boolean; name: string }): boolean {
    const name = entry.name;
    return (
      entry.isFile() &&
      (name.endsWith('.ts') || name.endsWith('.js')) &&
      !name.startsWith('_') &&
      !name.endsWith('.d.ts') &&
      !name.endsWith('.test.ts') &&
      !name.endsWith('.spec.ts')
    );
  }

  /**
   * Import handlers from a single file.
   */
  private async importHandlerFile(fullPath: string): Promise<number> {
    let count = 0;

    try {
      const module = (await import(`file://${fullPath}`)) as Record<string, unknown>;

      for (const [, exported] of Object.entries(module)) {
        if (this.isValidHandlerClass(exported)) {
          this.registry.register(exported.handlerName, exported);
          count++;
        }
      }
    } catch (importError) {
      log.debug(`Could not import ${fullPath}: ${importError}`, { operation: 'import_handlers' });
    }

    return count;
  }
}

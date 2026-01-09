/**
 * TAS-93: Class lookup resolver (priority 100).
 *
 * Infers handler classes from callable strings using dynamic imports.
 * This resolver handles module path formats.
 *
 * Supports callable formats:
 * - "./path/to/handler.js" - Relative module path
 * - "../parent/handler.js" - Parent-relative module path
 * - "@scope/package/handler" - Scoped package path
 *
 * NOT supported (security):
 * - "/absolute/path/to/handler.js" - Absolute paths are blocked to prevent
 *   loading arbitrary code in shared hosting environments.
 *
 * The module must export a class with a static `handlerName` property
 * matching the expected handler interface.
 *
 * Note: This resolver is lower priority (100) because dynamic imports
 * are more expensive than explicit mappings.
 *
 * @example
 * ```typescript
 * const resolver = new ClassLookupResolver();
 *
 * // Resolve from module path
 * const definition: HandlerDefinition = {
 *   callable: './handlers/payment-handler.js',
 * };
 * const handler = await resolver.resolve(definition);
 * ```
 */

import type { StepHandler, StepHandlerClass } from '../../handler/base.js';
import type { BaseResolver, ResolverConfig } from '../base-resolver.js';
import type { HandlerDefinition } from '../handler-definition.js';

/**
 * Pattern to match importable paths.
 *
 * Matches:
 * - Relative paths: ./foo, ../foo
 * - Package paths: @scope/foo
 *
 * Note: Absolute paths (/foo/bar) are intentionally NOT supported
 * to prevent loading arbitrary code from the filesystem in shared
 * hosting environments.
 */
const IMPORTABLE_PATTERN = /^(\.\.?\/|@[\w-]+\/)/;

/**
 * Resolver that infers handlers from module paths via dynamic import.
 *
 * Priority 100 - checked last in the default chain (inferential).
 */
export class ClassLookupResolver implements BaseResolver {
  readonly name = 'class_lookup';
  readonly priority = 100;

  /**
   * Check if callable looks like an importable path.
   *
   * @param definition - Handler definition
   * @param _config - Unused, part of interface
   * @returns True if callable matches importable pattern
   */
  canResolve(definition: HandlerDefinition, _config?: ResolverConfig): boolean {
    return IMPORTABLE_PATTERN.test(definition.callable);
  }

  /**
   * Resolve handler by dynamically importing the module.
   *
   * Looks for:
   * 1. Default export that is a handler class
   * 2. Named exports that are handler classes
   *
   * @param definition - Handler definition with module path
   * @param _config - Unused, part of interface
   * @returns Handler instance or null if not found
   */
  async resolve(
    definition: HandlerDefinition,
    _config?: ResolverConfig
  ): Promise<StepHandler | null> {
    // Security check: only allow paths that pass canResolve
    if (!this.canResolve(definition)) {
      return null;
    }

    const modulePath = definition.callable;

    try {
      const module = await import(modulePath);
      const handlerClass = this.findHandlerClass(module);

      if (!handlerClass) {
        return null;
      }

      return this.instantiateHandler(handlerClass);
    } catch (error) {
      // Import failed - log and return null to allow chain to continue
      console.debug(
        `[ClassLookupResolver] Failed to import '${modulePath}':`,
        error instanceof Error ? error.message : error
      );
      return null;
    }
  }

  /**
   * Find a handler class in a module's exports.
   */
  private findHandlerClass(module: Record<string, unknown>): StepHandlerClass | null {
    // Try default export first
    if (this.isHandlerClass(module.default)) {
      return module.default;
    }

    // Look through named exports
    for (const [, exported] of Object.entries(module)) {
      if (this.isHandlerClass(exported)) {
        return exported;
      }
    }

    return null;
  }

  /**
   * Check if a value is a valid handler class.
   */
  private isHandlerClass(value: unknown): value is StepHandlerClass {
    return (
      value !== null &&
      typeof value === 'function' &&
      'handlerName' in value &&
      typeof (value as StepHandlerClass).handlerName === 'string'
    );
  }

  /**
   * Instantiate a handler class.
   */
  private instantiateHandler(handlerClass: StepHandlerClass): StepHandler | null {
    try {
      return new handlerClass();
    } catch (error) {
      console.error(`[ClassLookupResolver] Failed to instantiate ${handlerClass.name}:`, error);
      return null;
    }
  }
}

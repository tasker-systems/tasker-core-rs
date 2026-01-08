/**
 * TAS-93: Base resolver interface for step handler resolution.
 *
 * Defines the contract that all resolvers must implement.
 * Resolvers are ordered by priority (lower = checked first).
 *
 * @example
 * ```typescript
 * class MyResolver implements BaseResolver {
 *   readonly name = 'my_resolver';
 *   readonly priority = 50;
 *
 *   canResolve(definition: HandlerDefinition): boolean {
 *     return definition.callable.startsWith('my:');
 *   }
 *
 *   async resolve(definition: HandlerDefinition): Promise<StepHandler | null> {
 *     if (!this.canResolve(definition)) return null;
 *     return new MyHandler();
 *   }
 * }
 * ```
 */

import type { StepHandler } from '../handler/base.js';
import type { HandlerDefinition } from './handler-definition.js';

/**
 * Configuration passed to resolvers during resolution.
 */
export interface ResolverConfig {
  /** Additional context for resolution */
  [key: string]: unknown;
}

/**
 * Base interface for step handler resolvers.
 *
 * Resolvers are responsible for finding and instantiating handlers
 * based on their callable format. Multiple resolvers form a chain,
 * ordered by priority.
 */
export interface BaseResolver {
  /**
   * Unique name for this resolver.
   * Used for resolver hints and debugging.
   */
  readonly name: string;

  /**
   * Priority in the resolver chain.
   * Lower values are checked first.
   *
   * Suggested ranges:
   * - 1-20: Explicit mappings (highest priority)
   * - 21-50: Custom domain resolvers
   * - 51-99: Pattern-based resolvers
   * - 100+: Inferential resolvers (lowest priority)
   */
  readonly priority: number;

  /**
   * Check if this resolver can handle the given definition.
   *
   * This is a quick check without actually resolving.
   * Used to determine if resolve() should be called.
   *
   * @param definition - Handler definition to check
   * @param config - Optional resolver configuration
   * @returns True if this resolver can attempt resolution
   */
  canResolve(definition: HandlerDefinition, config?: ResolverConfig): boolean;

  /**
   * Resolve a handler from the definition.
   *
   * Should return null if resolution fails (allows chain to continue).
   * Should throw only for unrecoverable errors.
   *
   * @param definition - Handler definition to resolve
   * @param config - Optional resolver configuration
   * @returns Handler instance or null if not found
   */
  resolve(definition: HandlerDefinition, config?: ResolverConfig): Promise<StepHandler | null>;
}

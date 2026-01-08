/**
 * TAS-93: Step handler resolver chain infrastructure.
 *
 * This module provides a priority-ordered resolver chain for flexible
 * handler resolution. Key components:
 *
 * - `ResolverChain`: Orchestrates multiple resolvers in priority order
 * - `BaseResolver`: Interface that all resolvers implement
 * - `RegistryResolver`: Developer-friendly base class for custom resolvers
 * - `ExplicitMappingResolver`: For explicitly registered handlers (priority 10)
 * - `ClassLookupResolver`: Infers handlers from module paths (priority 100)
 * - `MethodDispatchWrapper`: Redirects .call() to specified method
 * - `HandlerDefinition`: Configuration type for handler resolution
 *
 * @example Basic usage
 * ```typescript
 * import {
 *   ResolverChain,
 *   ExplicitMappingResolver,
 *   HandlerDefinition,
 * } from './registry';
 *
 * // Create chain with default resolvers
 * const chain = await ResolverChain.default();
 *
 * // Get explicit mapping resolver and register handler
 * const explicit = chain.getResolver('explicit_mapping') as ExplicitMappingResolver;
 * explicit.register('my_handler', MyHandler);
 *
 * // Resolve handler
 * const definition: HandlerDefinition = { callable: 'my_handler' };
 * const handler = await chain.resolve(definition);
 * ```
 *
 * @example Method dispatch
 * ```typescript
 * // Resolve with method dispatch
 * const definition: HandlerDefinition = {
 *   callable: 'my_handler',
 *   method: 'process',
 * };
 * const handler = await chain.resolve(definition);
 * // handler.call() will invoke handler.process()
 * ```
 *
 * @example Custom resolver
 * ```typescript
 * import { RegistryResolver, HandlerDefinition } from './registry';
 *
 * class PaymentResolver extends RegistryResolver {
 *   static readonly _name = 'payment_resolver';
 *   static readonly _priority = 20;
 *   static readonly pattern = /^payments:(\w+):(\w+)$/;
 *
 *   async resolveHandler(definition, match) {
 *     if (!match) return null;
 *     const [, provider, action] = match;
 *     return PaymentHandlers.get(provider, action);
 *   }
 * }
 *
 * chain.addResolver(new PaymentResolver());
 * ```
 */

// Resolver interface
export type { BaseResolver, ResolverConfig } from './base-resolver.js';
// Errors
export {
  MethodDispatchError,
  NoResolverMatchError,
  ResolutionError,
  ResolverNotFoundError,
} from './errors.js';
// Core types
export {
  effectiveMethod,
  fromCallable,
  fromDto,
  type HandlerDefinition,
  type HandlerSpec,
  hasResolverHint,
  normalizeToDefinition,
  usesMethodDispatch,
} from './handler-definition.js';

// Method dispatch
export { MethodDispatchWrapper } from './method-dispatch-wrapper.js';

// Developer-friendly base class
export { RegistryResolver, type RegistryResolverStatic } from './registry-resolver.js';
// Chain
export { ResolverChain } from './resolver-chain.js';
// Built-in resolvers
export {
  ClassLookupResolver,
  ExplicitMappingResolver,
  type HandlerEntry,
  type HandlerFactory,
} from './resolvers/index.js';

/**
 * TAS-93: Built-in resolvers for step handler resolution.
 */

export { ClassLookupResolver } from './class-lookup.js';
export {
  ExplicitMappingResolver,
  type HandlerEntry,
  type HandlerFactory,
} from './explicit-mapping.js';

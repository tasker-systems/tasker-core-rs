/**
 * TAS-93: Error types for resolver chain.
 */

/**
 * Base error class for resolution errors.
 */
export class ResolutionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ResolutionError';
  }
}

/**
 * Error thrown when a specified resolver is not found.
 */
export class ResolverNotFoundError extends ResolutionError {
  readonly resolverName: string;

  constructor(resolverName: string) {
    super(`Resolver not found: '${resolverName}'`);
    this.name = 'ResolverNotFoundError';
    this.resolverName = resolverName;
  }
}

/**
 * Error thrown when no resolver can handle a callable.
 */
export class NoResolverMatchError extends ResolutionError {
  readonly callable: string;
  readonly triedResolvers: string[];

  constructor(callable: string, triedResolvers: string[]) {
    const tried = triedResolvers.length > 0 ? triedResolvers.join(', ') : 'none';
    super(`No resolver could resolve callable '${callable}'. Tried: ${tried}`);
    this.name = 'NoResolverMatchError';
    this.callable = callable;
    this.triedResolvers = triedResolvers;
  }
}

/**
 * Error thrown when method dispatch fails.
 */
export class MethodDispatchError extends ResolutionError {
  readonly handlerName: string;
  readonly methodName: string;

  constructor(handlerName: string, methodName: string) {
    super(`Handler '${handlerName}' does not have method '${methodName}'`);
    this.name = 'MethodDispatchError';
    this.handlerName = handlerName;
    this.methodName = methodName;
  }
}

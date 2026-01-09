/**
 * TAS-93: Method dispatch wrapper for step handlers.
 *
 * Wraps a handler to redirect .call() invocations to a specified method.
 * This enables the `handler_method` field to work transparently.
 *
 * @example
 * ```typescript
 * class PaymentHandler extends StepHandler {
 *   static handlerName = 'payment_handler';
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     return this.success({ action: 'default' });
 *   }
 *
 *   async refund(context: StepContext): Promise<StepHandlerResult> {
 *     return this.success({ action: 'refund' });
 *   }
 * }
 *
 * // Without wrapper: handler.call(ctx) returns { action: 'default' }
 * // With wrapper:
 * const wrapped = new MethodDispatchWrapper(handler, 'refund');
 * await wrapped.call(ctx); // returns { action: 'refund' }
 * ```
 */

import type { ExecutableHandler, StepHandler } from '../handler/base.js';
import type { StepContext } from '../types/step-context.js';
import type { StepHandlerResult } from '../types/step-handler-result.js';
import { MethodDispatchError } from './errors.js';

/**
 * Wrapper that redirects .call() to a specified method.
 *
 * Implements ExecutableHandler to be type-safe when used in place of
 * a regular StepHandler. This avoids unsafe type casting while providing
 * the same public interface.
 */
export class MethodDispatchWrapper implements ExecutableHandler {
  /** The wrapped handler instance */
  readonly handler: StepHandler;

  /** The method to invoke instead of call() */
  readonly targetMethod: string;

  /** The bound method function */
  private readonly boundMethod: (context: StepContext) => Promise<StepHandlerResult>;

  /**
   * Create a new method dispatch wrapper.
   *
   * @param handler - The handler to wrap
   * @param targetMethod - The method name to invoke
   * @throws MethodDispatchError if handler doesn't have the method
   */
  constructor(handler: StepHandler, targetMethod: string) {
    this.handler = handler;
    this.targetMethod = targetMethod;

    // Validate method exists on handler
    const method = (handler as unknown as Record<string, unknown>)[targetMethod];
    if (typeof method !== 'function') {
      throw new MethodDispatchError(handler.name, targetMethod);
    }

    // Bind the method to the handler
    this.boundMethod = method.bind(handler) as (context: StepContext) => Promise<StepHandlerResult>;
  }

  /**
   * Get the handler name.
   * Delegates to wrapped handler.
   */
  get name(): string {
    return this.handler.name;
  }

  /**
   * Get the handler version.
   * Delegates to wrapped handler.
   */
  get version(): string {
    return this.handler.version;
  }

  /**
   * Get the handler capabilities.
   * Delegates to wrapped handler.
   */
  get capabilities(): string[] {
    return this.handler.capabilities;
  }

  /**
   * Execute the step by calling the target method.
   *
   * @param context - Step execution context
   * @returns Handler result from the target method
   */
  async call(context: StepContext): Promise<StepHandlerResult> {
    return this.boundMethod(context);
  }

  /**
   * Get config schema from wrapped handler.
   */
  configSchema(): Record<string, unknown> | null {
    return this.handler.configSchema();
  }

  /**
   * Get the unwrapped handler.
   *
   * Useful for testing and debugging.
   *
   * @returns The original handler instance
   */
  unwrap(): StepHandler {
    return this.handler;
  }

  /**
   * String representation for debugging.
   */
  toString(): string {
    return `MethodDispatchWrapper(handler=${this.handler.name}, method=${this.targetMethod})`;
  }
}

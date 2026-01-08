/**
 * TAS-93 Phase 5: Multi-Method Handler for Resolver Chain E2E Testing.
 *
 * This handler demonstrates the method dispatch feature of the resolver chain.
 * It has multiple entry points beyond the default `call()` method, allowing
 * YAML templates to specify `method: "validate"` or `method: "process"` etc.
 *
 * Example YAML configuration:
 * ```yaml
 * handler:
 *   callable: ResolverTests.StepHandlers.MultiMethodHandler
 *   method: validate  # Invokes validate() instead of call()
 * ```
 *
 * @see docs/ticket-specs/TAS-93/implementation-plan.md
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Multi-method handler demonstrating method dispatch.
 *
 * Available methods:
 * - call: Default entry point (standard processing)
 * - validate: Validation-only path
 * - process: Processing-specific path
 * - refund: Refund-specific path (payment domain example)
 *
 * Each method returns a result with `invoked_method` so tests can verify
 * which method was actually called.
 */
export class MultiMethodHandler extends StepHandler {
  static handlerName = 'ResolverTests.StepHandlers.MultiMethodHandler';
  static handlerVersion = '1.0.0';

  /**
   * Default entry point - standard processing.
   */
  async call(context: StepContext): Promise<StepHandlerResult> {
    const input = context.getInput<Record<string, unknown>>('data') ?? {};

    return this.success({
      invoked_method: 'call',
      handler: 'MultiMethodHandler',
      message: 'Default call method invoked',
      input_received: input,
      step_uuid: context.stepUuid,
    });
  }

  /**
   * Validation-only entry point.
   *
   * Can be invoked with: `method: "validate"`
   */
  async validate(context: StepContext): Promise<StepHandlerResult> {
    const input = context.getInput<Record<string, unknown>>('data') ?? {};

    // Simple validation logic for testing
    const hasRequiredFields = input.amount !== undefined;

    if (!hasRequiredFields) {
      return this.failure(
        'Validation failed: missing required field "amount"',
        'validation_error',
        false
      );
    }

    return this.success({
      invoked_method: 'validate',
      handler: 'MultiMethodHandler',
      message: 'Validation completed successfully',
      validated: true,
      input_validated: input,
      step_uuid: context.stepUuid,
    });
  }

  /**
   * Processing entry point.
   *
   * Can be invoked with: `method: "process"`
   */
  async process(context: StepContext): Promise<StepHandlerResult> {
    const input = context.getInput<Record<string, unknown>>('data') ?? {};
    const amount = (input.amount as number) ?? 0;

    // Simple processing logic for testing
    const processedAmount = amount * 1.1; // Add 10% processing fee

    return this.success({
      invoked_method: 'process',
      handler: 'MultiMethodHandler',
      message: 'Processing completed',
      original_amount: amount,
      processed_amount: processedAmount,
      processing_fee: processedAmount - amount,
      step_uuid: context.stepUuid,
    });
  }

  /**
   * Refund entry point.
   *
   * Can be invoked with: `method: "refund"`
   * Demonstrates payment domain method dispatch pattern.
   */
  async refund(context: StepContext): Promise<StepHandlerResult> {
    const input = context.getInput<Record<string, unknown>>('data') ?? {};
    const amount = (input.amount as number) ?? 0;
    const reason = (input.reason as string) ?? 'not_specified';

    return this.success({
      invoked_method: 'refund',
      handler: 'MultiMethodHandler',
      message: 'Refund processed',
      refund_amount: amount,
      refund_reason: reason,
      refund_id: `refund_${Date.now()}`,
      step_uuid: context.stepUuid,
    });
  }
}

/**
 * Second multi-method handler for testing resolver chain with different handlers.
 *
 * This handler is used to verify that the resolver chain can find
 * handlers by different callable addresses.
 */
export class AlternateMethodHandler extends StepHandler {
  static handlerName = 'ResolverTests.StepHandlers.AlternateMethodHandler';
  static handlerVersion = '1.0.0';

  /**
   * Default entry point.
   */
  async call(context: StepContext): Promise<StepHandlerResult> {
    return this.success({
      invoked_method: 'call',
      handler: 'AlternateMethodHandler',
      message: 'Alternate handler default method',
      step_uuid: context.stepUuid,
    });
  }

  /**
   * Custom action method.
   *
   * Can be invoked with: `method: "execute_action"`
   */
  async execute_action(context: StepContext): Promise<StepHandlerResult> {
    const action = context.getInput<string>('action_type') ?? 'default_action';

    return this.success({
      invoked_method: 'execute_action',
      handler: 'AlternateMethodHandler',
      message: 'Custom action executed',
      action_type: action,
      step_uuid: context.stepUuid,
    });
  }
}

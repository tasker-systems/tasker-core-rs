/**
 * Error Testing Handlers for E2E Testing.
 *
 * Provides handlers for testing different error scenarios:
 * - SuccessHandler: Always succeeds
 * - PermanentErrorHandler: Fails with non-retryable error
 * - RetryableErrorHandler: Fails with retryable error
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import { ErrorType } from '../../../../../src/types/error-type.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Handler that always succeeds.
 */
export class SuccessHandler extends StepHandler {
  static handlerName = 'TestErrors.StepHandlers.SuccessHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const message = context.getInput<string>('message') ?? 'Success!';
    const timestamp = new Date().toISOString();

    return this.success({
      message,
      timestamp,
      scenario: 'success',
      handler: 'SuccessHandler',
    });
  }
}

/**
 * Handler that always fails with a permanent (non-retryable) error.
 *
 * Used to test that the orchestration system correctly handles
 * permanent failures without retry attempts.
 */
export class PermanentErrorHandler extends StepHandler {
  static handlerName = 'TestErrors.StepHandlers.PermanentErrorHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const message = context.getInput<string>('message') ?? 'Permanent error - no retry allowed';

    return this.failure(
      message,
      ErrorType.PERMANENT_FAILURE,
      false, // Not retryable
      {
        scenario: 'permanent_error',
        handler: 'PermanentErrorHandler',
        timestamp: new Date().toISOString(),
      }
    );
  }
}

/**
 * Handler that always fails with a retryable error.
 *
 * Used to test that the orchestration system correctly handles
 * retryable failures with exponential backoff until retry limit.
 */
export class RetryableErrorHandler extends StepHandler {
  static handlerName = 'TestErrors.StepHandlers.RetryableErrorHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const message = context.getInput<string>('message') ?? 'Retryable error - will be retried';

    return this.failure(
      message,
      ErrorType.RETRYABLE_FAILURE,
      true, // Retryable
      {
        scenario: 'retryable_error',
        handler: 'RetryableErrorHandler',
        timestamp: new Date().toISOString(),
        attempt: context.step?.attempts ?? 1,
      }
    );
  }
}

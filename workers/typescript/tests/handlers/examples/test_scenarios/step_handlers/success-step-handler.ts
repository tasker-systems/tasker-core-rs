/**
 * Success Step Handler for E2E Testing.
 *
 * Simple handler that always succeeds - used for basic integration testing.
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Simple step handler that always succeeds.
 *
 * Used for testing basic workflow execution without error handling complexity.
 */
export class SuccessStepHandler extends StepHandler {
  static handlerName = 'TestScenarios.StepHandlers.SuccessStepHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const message = context.getInput<string>('message') ?? 'Hello from TypeScript!';
    const timestamp = new Date().toISOString();

    return this.success({
      message,
      timestamp,
      handler: 'SuccessStepHandler',
      language: 'typescript',
    });
  }
}

/**
 * Test fixture: Module with both default and named exports.
 * Used to verify default export takes priority.
 */

import { StepHandler } from '../../../src/handler/base.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

class PrimaryHandler extends StepHandler {
  static handlerName = 'primary_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'primary' });
  }
}

export class SecondaryHandler extends StepHandler {
  static handlerName = 'secondary_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'secondary' });
  }
}

export default PrimaryHandler;

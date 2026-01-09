/**
 * Test fixture: Handler whose constructor throws.
 */

import { StepHandler } from '../../../src/handler/base.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

class FailingConstructorHandler extends StepHandler {
  static handlerName = 'failing_constructor_handler';

  constructor() {
    super();
    throw new Error('Constructor intentionally failed for testing!');
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({});
  }
}

export default FailingConstructorHandler;

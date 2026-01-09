/**
 * Test fixture: Handler with default export.
 */

import { StepHandler } from '../../../src/handler/base.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

class DefaultExportHandler extends StepHandler {
  static handlerName = 'default_export_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'default_export' });
  }
}

export default DefaultExportHandler;

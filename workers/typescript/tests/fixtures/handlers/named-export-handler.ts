/**
 * Test fixture: Handler with named export.
 */

import { StepHandler } from '../../../src/handler/base.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

export class NamedExportHandler extends StepHandler {
  static handlerName = 'named_export_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ type: 'named_export' });
  }
}

// Export a non-handler item to test filtering
export const notAHandler = 'this is not a handler';

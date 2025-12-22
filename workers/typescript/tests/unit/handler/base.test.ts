/**
 * StepHandler base class tests.
 *
 * Verifies abstract base class functionality and helper methods.
 */

import { describe, expect, it } from 'bun:test';
import { StepHandler } from '../../../src/handler/base.js';
import { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';
import { ErrorType } from '../../../src/types/error-type.js';
import type { FfiStepEvent } from '../../../src/ffi/types.js';

// Concrete implementation for testing
class TestHandler extends StepHandler {
  static handlerName = 'test_handler';
  static handlerVersion = '2.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const input = context.getInput<string>('key');
    return this.success({ received: input });
  }
}

class MinimalHandler extends StepHandler {
  static handlerName = 'minimal_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({});
  }
}

class FailingHandler extends StepHandler {
  static handlerName = 'failing_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.failure('Something went wrong', ErrorType.HANDLER_ERROR, true);
  }
}

class CustomCapabilitiesHandler extends StepHandler {
  static handlerName = 'custom_capabilities';

  get capabilities(): string[] {
    return ['process', 'transform', 'validate'];
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({});
  }
}

class ConfiguredHandler extends StepHandler {
  static handlerName = 'configured_handler';

  configSchema(): Record<string, unknown> {
    return {
      type: 'object',
      properties: {
        api_endpoint: { type: 'string' },
        timeout: { type: 'number' },
      },
      required: ['api_endpoint'],
    };
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({});
  }
}

describe('StepHandler', () => {
  describe('static properties', () => {
    it('has handlerName static property', () => {
      expect(TestHandler.handlerName).toBe('test_handler');
    });

    it('has handlerVersion static property with custom value', () => {
      expect(TestHandler.handlerVersion).toBe('2.0.0');
    });

    it('defaults handlerVersion to 1.0.0', () => {
      expect(MinimalHandler.handlerVersion).toBe('1.0.0');
    });
  });

  describe('instance properties', () => {
    it('name getter returns static handlerName', () => {
      const handler = new TestHandler();
      expect(handler.name).toBe('test_handler');
    });

    it('version getter returns static handlerVersion', () => {
      const handler = new TestHandler();
      expect(handler.version).toBe('2.0.0');
    });

    it('version getter returns default version', () => {
      const handler = new MinimalHandler();
      expect(handler.version).toBe('1.0.0');
    });
  });

  describe('capabilities', () => {
    it('returns default capabilities', () => {
      const handler = new TestHandler();
      expect(handler.capabilities).toEqual(['process']);
    });

    it('returns custom capabilities when overridden', () => {
      const handler = new CustomCapabilitiesHandler();
      expect(handler.capabilities).toEqual(['process', 'transform', 'validate']);
    });
  });

  describe('configSchema', () => {
    it('returns null by default', () => {
      const handler = new TestHandler();
      expect(handler.configSchema()).toBeNull();
    });

    it('returns custom schema when overridden', () => {
      const handler = new ConfiguredHandler();
      const schema = handler.configSchema();

      expect(schema).not.toBeNull();
      expect(schema?.type).toBe('object');
      expect(schema?.properties).toBeDefined();
    });
  });

  describe('success helper', () => {
    it('creates success result', async () => {
      const handler = new TestHandler();
      const context = createValidStepContext();
      const result = await handler.call(context);

      expect(result.success).toBe(true);
      expect(result.isSuccess()).toBe(true);
    });

    it('passes result data through', async () => {
      const handler = new TestHandler();
      const context = createValidStepContext({ inputData: { key: 'value' } });
      const result = await handler.call(context);

      expect(result.result).toEqual({ received: 'value' });
    });
  });

  describe('failure helper', () => {
    it('creates failure result', async () => {
      const handler = new FailingHandler();
      const context = createValidStepContext();
      const result = await handler.call(context);

      expect(result.success).toBe(false);
      expect(result.isFailure()).toBe(true);
    });

    it('passes error details through', async () => {
      const handler = new FailingHandler();
      const context = createValidStepContext();
      const result = await handler.call(context);

      expect(result.errorMessage).toBe('Something went wrong');
      expect(result.errorType).toBe('handler_error');
      expect(result.retryable).toBe(true);
    });
  });

  describe('toString', () => {
    it('returns formatted string representation', () => {
      const handler = new TestHandler();
      const str = handler.toString();

      expect(str).toBe('TestHandler(name=test_handler, version=2.0.0)');
    });

    it('uses default version in string', () => {
      const handler = new MinimalHandler();
      const str = handler.toString();

      expect(str).toBe('MinimalHandler(name=minimal_handler, version=1.0.0)');
    });
  });

  describe('call method', () => {
    it('receives context with input data', async () => {
      const handler = new TestHandler();
      const context = createValidStepContext({
        inputData: { key: 'test_value' },
      });
      const result = await handler.call(context);

      expect(result.result).toEqual({ received: 'test_value' });
    });

    it('is async and returns Promise', () => {
      const handler = new TestHandler();
      const context = createValidStepContext();
      const resultPromise = handler.call(context);

      expect(resultPromise).toBeInstanceOf(Promise);
    });
  });
});

// Test helpers

function createValidFfiStepEvent(): FfiStepEvent {
  return {
    event_id: 'event-123',
    task_uuid: 'task-456',
    step_uuid: 'step-789',
    correlation_id: 'corr-001',
    trace_id: null,
    span_id: null,
    task_correlation_id: 'task-corr-001',
    parent_correlation_id: null,
    task: {
      task_uuid: 'task-456',
      named_task_uuid: 'named-task-001',
      name: 'TestTask',
      namespace: 'test',
      version: '1.0.0',
      context: null,
      correlation_id: 'corr-001',
      parent_correlation_id: null,
      complete: false,
      priority: 0,
      initiator: null,
      source_system: null,
      reason: null,
      tags: null,
      identity_hash: 'hash-123',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      requested_at: new Date().toISOString(),
    },
    workflow_step: {
      workflow_step_uuid: 'step-789',
      task_uuid: 'task-456',
      named_step_uuid: 'named-step-001',
      name: 'TestStep',
      template_step_name: 'test_step',
      retryable: true,
      max_attempts: 3,
      attempts: 0,
      in_process: false,
      processed: false,
      skippable: false,
      inputs: null,
      results: null,
      backoff_request_seconds: null,
      processed_at: null,
      last_attempted_at: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    },
    step_definition: {
      name: 'test_step',
      description: 'A test step',
      handler: {
        callable: 'TestHandler',
        initialization: {},
      },
      system_dependency: null,
      dependencies: [],
      timeout_seconds: 30,
      retry: {
        retryable: true,
        max_attempts: 3,
        backoff: 'exponential',
        backoff_base_ms: 1000,
        max_backoff_ms: 30000,
      },
    },
    dependency_results: {},
  };
}

function createValidStepContext(
  overrides: Partial<{
    inputData: Record<string, unknown>;
    dependencyResults: Record<string, unknown>;
    stepConfig: Record<string, unknown>;
    stepInputs: Record<string, unknown>;
  }> = {}
): StepContext {
  const event = createValidFfiStepEvent();
  return new StepContext({
    event,
    taskUuid: 'task-123',
    stepUuid: 'step-456',
    correlationId: 'corr-789',
    handlerName: 'test_handler',
    inputData: overrides.inputData ?? {},
    dependencyResults: overrides.dependencyResults ?? {},
    stepConfig: overrides.stepConfig ?? {},
    stepInputs: overrides.stepInputs ?? {},
    retryCount: 0,
    maxRetries: 3,
  });
}

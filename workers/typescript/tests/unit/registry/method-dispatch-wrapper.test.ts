/**
 * TAS-93: MethodDispatchWrapper tests.
 *
 * Verifies method dispatch wrapper behavior for redirecting
 * handler calls to non-default methods.
 */

import { describe, expect, it } from 'bun:test';
import { StepHandler } from '../../../src/handler/base.js';
import { MethodDispatchError } from '../../../src/registry/errors.js';
import { MethodDispatchWrapper } from '../../../src/registry/method-dispatch-wrapper.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

// Test handler with multiple methods
class MultiMethodHandler extends StepHandler {
  static handlerName = 'multi_method_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ method: 'call' });
  }

  async process(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ method: 'process' });
  }

  async validate(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ method: 'validate' });
  }
}

// Handler with a method that throws
class ErrorMethodHandler extends StepHandler {
  static handlerName = 'error_method_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({});
  }

  async failingMethod(_context: StepContext): Promise<StepHandlerResult> {
    throw new Error('Method failed!');
  }
}

// Create a minimal mock context for testing
function createMockContext(): StepContext {
  return {
    handlerCallable: 'test_handler',
    task: { task_uuid: 'task-1' } as unknown as StepContext['task'],
    workflowStep: { workflow_step_uuid: 'step-1' } as unknown as StepContext['workflowStep'],
    stepDefinition: { name: 'step' } as unknown as StepContext['stepDefinition'],
    dependencyResults: {},
    eventId: 'evt-1',
    correlationId: 'corr-1',
    taskCorrelationId: 'task-corr-1',
    stepUuid: 'step-1',
    taskUuid: 'task-1',
    namedStepUuid: 'named-step-1',
    stepName: 'step',
    raw: {} as unknown as StepContext['raw'],
  } as StepContext;
}

describe('MethodDispatchWrapper', () => {
  describe('constructor', () => {
    it('wraps handler with target method', () => {
      const handler = new MultiMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'process');

      expect(wrapper.handler).toBe(handler);
      expect(wrapper.targetMethod).toBe('process');
    });

    it('throws error if target method does not exist', () => {
      const handler = new MultiMethodHandler();

      expect(() => new MethodDispatchWrapper(handler, 'nonexistent')).toThrow(MethodDispatchError);
    });

    it('throws error if target method is not a function', () => {
      const handler = new MultiMethodHandler();
      // @ts-expect-error - name is not a function
      (handler as unknown as { name: string }).someProperty = 'not a function';

      expect(() => new MethodDispatchWrapper(handler, 'someProperty' as string)).toThrow(
        MethodDispatchError
      );
    });
  });

  describe('call', () => {
    it('delegates to target method', async () => {
      const handler = new MultiMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'process');
      const context = createMockContext();

      const result = await wrapper.call(context);

      expect(result.success).toBe(true);
      expect(result.result).toEqual({ method: 'process' });
    });

    it('can dispatch to different methods', async () => {
      const handler = new MultiMethodHandler();
      const context = createMockContext();

      const processWrapper = new MethodDispatchWrapper(handler, 'process');
      const validateWrapper = new MethodDispatchWrapper(handler, 'validate');

      const processResult = await processWrapper.call(context);
      const validateResult = await validateWrapper.call(context);

      expect(processResult.result).toEqual({ method: 'process' });
      expect(validateResult.result).toEqual({ method: 'validate' });
    });

    it('propagates errors from target method', async () => {
      const handler = new ErrorMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'failingMethod');
      const context = createMockContext();

      await expect(wrapper.call(context)).rejects.toThrow('Method failed!');
    });
  });

  describe('handler delegation', () => {
    it('exposes handler name', () => {
      const handler = new MultiMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'process');

      expect(wrapper.name).toBe('multi_method_handler');
    });

    it('exposes handler version', () => {
      const handler = new MultiMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'process');

      expect(wrapper.version).toBe(handler.version);
    });

    it('exposes handler capabilities', () => {
      const handler = new MultiMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'process');

      expect(wrapper.capabilities).toEqual(handler.capabilities);
    });

    it('delegates configSchema', () => {
      const handler = new MultiMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'process');

      expect(wrapper.configSchema()).toEqual(handler.configSchema());
    });
  });

  describe('unwrap', () => {
    it('returns the original handler', () => {
      const handler = new MultiMethodHandler();
      const wrapper = new MethodDispatchWrapper(handler, 'process');

      const unwrapped = wrapper.unwrap();

      expect(unwrapped).toBe(handler);
    });
  });

  describe('wrapping already wrapped handlers', () => {
    it('can wrap a wrapper (uses call method)', () => {
      const handler = new MultiMethodHandler();
      const wrapper1 = new MethodDispatchWrapper(handler, 'process');
      // Wrapping a wrapper is valid - it calls wrapper1.call()
      const wrapper2 = new MethodDispatchWrapper(wrapper1, 'call');

      // The inner wrapper is preserved
      expect(wrapper2.handler).toBe(wrapper1);
    });

    it('unwrap returns immediate wrapped handler', () => {
      const handler = new MultiMethodHandler();
      const wrapper1 = new MethodDispatchWrapper(handler, 'process');
      const wrapper2 = new MethodDispatchWrapper(wrapper1, 'call');

      // unwrap returns the immediate wrapped handler
      expect(wrapper2.unwrap()).toBe(wrapper1);
      expect(wrapper1.unwrap()).toBe(handler);
    });
  });
});

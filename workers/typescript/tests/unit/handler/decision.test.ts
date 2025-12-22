import { describe, expect, test, beforeEach } from 'bun:test';
import { DecisionHandler, DecisionType } from '../../../src/handler/decision';
import type { DecisionPointOutcome } from '../../../src/handler/decision';
import { StepContext } from '../../../src/types/step-context';
import type { StepHandlerResult } from '../../../src/types/step-handler-result';

// =============================================================================
// Test Fixtures
// =============================================================================

/**
 * Concrete implementation of DecisionHandler for testing.
 */
class TestDecisionHandler extends DecisionHandler {
  static handlerName = 'test_decision_handler';

  // Expose protected methods for testing
  testDecisionSuccess(
    steps: string[],
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ) {
    return this.decisionSuccess(steps, routingContext, metadata);
  }

  testDecisionSuccessWithOutcome(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ) {
    return this.decisionSuccessWithOutcome(outcome, metadata);
  }

  testDecisionNoBranches(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ) {
    return this.decisionNoBranches(outcome, metadata);
  }

  testSkipBranches(
    reason: string,
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ) {
    return this.skipBranches(reason, routingContext, metadata);
  }

  testDecisionFailure(
    message: string,
    errorType = 'decision_error',
    retryable = false,
    metadata?: Record<string, unknown>
  ) {
    return this.decisionFailure(message, errorType, retryable, metadata);
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    // Simple routing based on context for testing
    return this.decisionSuccess(['process_step']);
  }
}

// =============================================================================
// DecisionType Tests
// =============================================================================

describe('DecisionType', () => {
  test('should have CREATE_STEPS value', () => {
    expect(DecisionType.CREATE_STEPS).toBe('create_steps');
  });

  test('should have NO_BRANCHES value', () => {
    expect(DecisionType.NO_BRANCHES).toBe('no_branches');
  });
});

// =============================================================================
// DecisionHandler Tests
// =============================================================================

describe('DecisionHandler', () => {
  let handler: TestDecisionHandler;

  test('should have correct static handler name', () => {
    expect(TestDecisionHandler.handlerName).toBe('test_decision_handler');
  });

  describe('capabilities', () => {
    test('should include decision capabilities', () => {
      handler = new TestDecisionHandler();
      const caps = handler.capabilities;

      expect(caps).toContain('process');
      expect(caps).toContain('decision');
      expect(caps).toContain('routing');
    });
  });

  describe('decisionSuccess', () => {
    beforeEach(() => {
      handler = new TestDecisionHandler();
    });

    test('should create success result with step names', () => {
      const result = handler.testDecisionSuccess(['step_a', 'step_b']);

      expect(result.success).toBe(true);
      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.type).toBe(DecisionType.CREATE_STEPS);
      expect(outcome.step_names).toEqual(['step_a', 'step_b']);
    });

    test('should include routing context', () => {
      const result = handler.testDecisionSuccess(['process_order'], { tier: 'premium' });

      expect(result.result?.routing_context).toEqual({ tier: 'premium' });
    });

    test('should include decision handler metadata', () => {
      const result = handler.testDecisionSuccess(['step_a']);

      expect(result.metadata?.decision_handler).toBe('test_decision_handler');
      expect(result.metadata?.decision_version).toBe('1.0.0');
    });

    test('should merge custom metadata', () => {
      const result = handler.testDecisionSuccess(['step_a'], undefined, { custom: 'value' });

      expect(result.metadata?.custom).toBe('value');
      expect(result.metadata?.decision_handler).toBe('test_decision_handler');
    });

    test('should work with single step', () => {
      const result = handler.testDecisionSuccess(['only_step']);

      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.step_names).toEqual(['only_step']);
    });

    test('should work with empty routing context', () => {
      const result = handler.testDecisionSuccess(['step_a'], {});

      // Empty routing context should not be included
      expect(result.result?.routing_context).toBeUndefined();
    });
  });

  describe('decisionSuccessWithOutcome', () => {
    beforeEach(() => {
      handler = new TestDecisionHandler();
    });

    test('should create success result from full outcome', () => {
      const outcome: DecisionPointOutcome = {
        decisionType: DecisionType.CREATE_STEPS,
        nextStepNames: ['step_a', 'step_b'],
        routingContext: { source: 'test' },
      };

      const result = handler.testDecisionSuccessWithOutcome(outcome);

      expect(result.success).toBe(true);
      const dpOutcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(dpOutcome.type).toBe(DecisionType.CREATE_STEPS);
      expect(dpOutcome.step_names).toEqual(['step_a', 'step_b']);
      expect(result.result?.routing_context).toEqual({ source: 'test' });
    });

    test('should include dynamic steps when provided', () => {
      const outcome: DecisionPointOutcome = {
        decisionType: DecisionType.CREATE_STEPS,
        nextStepNames: ['dynamic_step'],
        dynamicSteps: [
          { name: 'dynamic_step', handler: 'process_item' },
        ],
        routingContext: {},
      };

      const result = handler.testDecisionSuccessWithOutcome(outcome);

      expect(result.result?.dynamic_steps).toEqual([
        { name: 'dynamic_step', handler: 'process_item' },
      ]);
    });
  });

  describe('decisionNoBranches', () => {
    beforeEach(() => {
      handler = new TestDecisionHandler();
    });

    test('should create success result with no_branches type', () => {
      const outcome: DecisionPointOutcome = {
        decisionType: DecisionType.NO_BRANCHES,
        nextStepNames: [],
        reason: 'No items to process',
        routingContext: {},
      };

      const result = handler.testDecisionNoBranches(outcome);

      expect(result.success).toBe(true);
      const dpOutcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(dpOutcome.type).toBe(DecisionType.NO_BRANCHES);
    });

    test('should include reason when provided', () => {
      const outcome: DecisionPointOutcome = {
        decisionType: DecisionType.NO_BRANCHES,
        nextStepNames: [],
        reason: 'All items already processed',
        routingContext: {},
      };

      const result = handler.testDecisionNoBranches(outcome);

      expect(result.result?.reason).toBe('All items already processed');
    });
  });

  describe('skipBranches', () => {
    beforeEach(() => {
      handler = new TestDecisionHandler();
    });

    test('should create success result with no_branches type', () => {
      const result = handler.testSkipBranches('No items require processing');

      expect(result.success).toBe(true);
      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.type).toBe(DecisionType.NO_BRANCHES);
    });

    test('should include reason', () => {
      const result = handler.testSkipBranches('Empty batch');

      expect(result.result?.reason).toBe('Empty batch');
    });

    test('should include routing context when provided', () => {
      const result = handler.testSkipBranches('Filtered out', { filter: 'active' });

      expect(result.result?.routing_context).toEqual({ filter: 'active' });
    });

    test('should include metadata', () => {
      const result = handler.testSkipBranches('Skip reason', undefined, { skip_count: 10 });

      expect(result.metadata?.skip_count).toBe(10);
      expect(result.metadata?.decision_handler).toBe('test_decision_handler');
    });
  });

  describe('decisionFailure', () => {
    beforeEach(() => {
      handler = new TestDecisionHandler();
    });

    test('should create failure result', () => {
      const result = handler.testDecisionFailure('Missing required field');

      expect(result.success).toBe(false);
      expect(result.errorMessage).toBe('Missing required field');
      expect(result.errorType).toBe('decision_error');
      expect(result.retryable).toBe(false);
    });

    test('should use custom error type', () => {
      const result = handler.testDecisionFailure('Validation failed', 'validation_error');

      expect(result.errorType).toBe('validation_error');
    });

    test('should allow marking as retryable', () => {
      const result = handler.testDecisionFailure('Temporary error', 'decision_error', true);

      expect(result.retryable).toBe(true);
    });

    test('should include decision handler metadata', () => {
      const result = handler.testDecisionFailure('Error message');

      expect(result.metadata?.decision_handler).toBe('test_decision_handler');
      expect(result.metadata?.decision_version).toBe('1.0.0');
    });

    test('should merge custom metadata', () => {
      const result = handler.testDecisionFailure('Error', 'decision_error', false, {
        attempted_routes: ['route_a', 'route_b'],
      });

      expect(result.metadata?.attempted_routes).toEqual(['route_a', 'route_b']);
    });
  });

  describe('call method', () => {
    test('should execute and return decision result', async () => {
      handler = new TestDecisionHandler();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
      });

      const result = await handler.call(context);

      expect(result.success).toBe(true);
      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.step_names).toEqual(['process_step']);
    });
  });
});

// =============================================================================
// Real-world Scenarios
// =============================================================================

describe('DecisionHandler real-world scenarios', () => {
  describe('CustomerTierRouter', () => {
    class CustomerTierRouter extends DecisionHandler {
      static handlerName = 'route_by_tier';

      async call(context: StepContext): Promise<StepHandlerResult> {
        const tier = context.inputData.customer_tier as string;

        if (tier === 'enterprise') {
          return this.decisionSuccess(
            ['enterprise_validation', 'enterprise_processing'],
            { tier }
          );
        }
        if (tier === 'premium') {
          return this.decisionSuccess(['premium_processing'], { tier });
        }
        if (tier === 'standard') {
          return this.decisionSuccess(['standard_processing'], { tier });
        }

        return this.decisionFailure(`Unknown tier: ${tier}`, 'unknown_tier');
      }
    }

    test('should route enterprise customers correctly', async () => {
      const router = new CustomerTierRouter();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        inputData: { customer_tier: 'enterprise' },
      });

      const result = await router.call(context);

      expect(result.success).toBe(true);
      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.step_names).toEqual(['enterprise_validation', 'enterprise_processing']);
      expect(result.result?.routing_context).toEqual({ tier: 'enterprise' });
    });

    test('should route premium customers correctly', async () => {
      const router = new CustomerTierRouter();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        inputData: { customer_tier: 'premium' },
      });

      const result = await router.call(context);

      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.step_names).toEqual(['premium_processing']);
    });

    test('should fail for unknown tier', async () => {
      const router = new CustomerTierRouter();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        inputData: { customer_tier: 'unknown' },
      });

      const result = await router.call(context);

      expect(result.success).toBe(false);
      expect(result.errorType).toBe('unknown_tier');
    });
  });

  describe('BatchFilterDecision', () => {
    class BatchFilterDecision extends DecisionHandler {
      static handlerName = 'filter_batch';

      async call(context: StepContext): Promise<StepHandlerResult> {
        const items = context.inputData.items as unknown[];

        if (!items || items.length === 0) {
          return this.skipBranches('No items to process', { itemCount: 0 });
        }

        return this.decisionSuccess(['process_batch'], { itemCount: items.length });
      }
    }

    test('should skip branches when no items', async () => {
      const handler = new BatchFilterDecision();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        inputData: { items: [] },
      });

      const result = await handler.call(context);

      expect(result.success).toBe(true);
      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.type).toBe(DecisionType.NO_BRANCHES);
      expect(result.result?.reason).toBe('No items to process');
    });

    test('should proceed when items exist', async () => {
      const handler = new BatchFilterDecision();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        inputData: { items: [1, 2, 3] },
      });

      const result = await handler.call(context);

      expect(result.success).toBe(true);
      const outcome = result.result?.decision_point_outcome as Record<string, unknown>;
      expect(outcome.type).toBe(DecisionType.CREATE_STEPS);
      expect(outcome.step_names).toEqual(['process_batch']);
    });
  });
});

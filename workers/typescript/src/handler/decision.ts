/**
 * Decision handler for workflow routing.
 *
 * TAS-112: Composition Pattern (DEPRECATED CLASS)
 *
 * This module provides the DecisionHandler class for backward compatibility.
 * For new code, use the mixin pattern:
 *
 * @example Using DecisionMixin
 * ```typescript
 * import { StepHandler } from './base';
 * import { DecisionMixin, DecisionCapable, applyDecision } from './mixins/decision';
 *
 * class RouteOrderHandler extends StepHandler implements DecisionCapable {
 *   static handlerName = 'route_order';
 *
 *   constructor() {
 *     super();
 *     applyDecision(this);
 *   }
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const orderType = context.inputData['order_type'];
 *     if (orderType === 'premium') {
 *       return this.decisionSuccess(
 *         ['validate_premium', 'process_premium'],
 *         { order_type: orderType }
 *       );
 *     }
 *     return this.decisionSuccess(['process_standard']);
 *   }
 * }
 * ```
 *
 * @module handler/decision
 */

import type { StepHandlerResult } from '../types/step-handler-result.js';
import { StepHandler } from './base.js';
import { DecisionMixin, type DecisionPointOutcome } from './mixins/decision.js';

// Re-export types from mixin for backward compatibility
export { type DecisionPointOutcome, DecisionType } from './mixins/decision.js';

/**
 * Base class for decision point step handlers.
 *
 * TAS-112: This class is provided for backward compatibility.
 * For new code, prefer using DecisionMixin directly with applyDecision().
 *
 * Decision handlers are used to make routing decisions in workflows.
 * They evaluate conditions and determine which steps should execute next.
 *
 * @example
 * ```typescript
 * class CustomerTierRouter extends DecisionHandler {
 *   static handlerName = 'route_by_tier';
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const tier = context.inputData['customer_tier'];
 *     if (tier === 'enterprise') {
 *       return this.decisionSuccess(
 *         ['enterprise_validation', 'enterprise_processing'],
 *         { tier }
 *       );
 *     } else if (tier === 'premium') {
 *       return this.decisionSuccess(['premium_processing']);
 *     } else {
 *       return this.decisionSuccess(['standard_processing']);
 *     }
 *   }
 * }
 * ```
 */
export abstract class DecisionHandler extends StepHandler {
  private readonly _decisionMixin = new DecisionMixin();

  get capabilities(): string[] {
    return ['process', 'decision', 'routing'];
  }

  /**
   * Simplified decision success helper (cross-language standard API).
   *
   * Use this when routing to one or more steps based on a decision.
   * This is the recommended method for most decision handlers.
   */
  protected decisionSuccess(
    steps: string[],
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return this._decisionMixin.decisionSuccess.call(this, steps, routingContext, metadata);
  }

  /**
   * Create a success result with a DecisionPointOutcome.
   *
   * Use this for complex decision outcomes that require dynamic steps
   * or advanced routing. For simple step routing, use `decisionSuccess()`.
   */
  protected decisionSuccessWithOutcome(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return this._decisionMixin.decisionSuccessWithOutcome.call(this, outcome, metadata);
  }

  /**
   * Create a success result for a decision with no branches.
   *
   * Use this when the decision results in no additional steps being executed.
   */
  protected decisionNoBranches(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return this._decisionMixin.decisionNoBranches.call(this, outcome, metadata);
  }

  /**
   * Convenience method to skip all branches.
   */
  protected skipBranches(
    reason: string,
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return this._decisionMixin.skipBranches.call(this, reason, routingContext, metadata);
  }

  /**
   * Create a failure result for a decision that could not be made.
   */
  protected decisionFailure(
    message: string,
    errorType = 'decision_error',
    retryable = false,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return this._decisionMixin.decisionFailure.call(this, message, errorType, retryable, metadata);
  }
}

/**
 * Alias for DecisionHandler for backwards compatibility.
 *
 * @deprecated Use DecisionHandler instead
 */
export { DecisionHandler as DecisionStepHandler };

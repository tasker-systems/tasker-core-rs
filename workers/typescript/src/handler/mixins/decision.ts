/**
 * Decision mixin for workflow routing.
 *
 * TAS-112: Composition Pattern - Decision Mixin
 *
 * This module provides the DecisionMixin class for step handlers that make
 * routing decisions. Use via interface implementation with method binding.
 *
 * @example
 * ```typescript
 * class RouteOrderHandler extends StepHandler implements DecisionCapable {
 *   static handlerName = 'route_order';
 *
 *   // Bind DecisionMixin methods
 *   decisionSuccess = DecisionMixin.prototype.decisionSuccess.bind(this);
 *   skipBranches = DecisionMixin.prototype.skipBranches.bind(this);
 *   // ... other required methods
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
 * @module handler/mixins/decision
 */

import { StepHandlerResult } from '../../types/step-handler-result.js';

/**
 * Type of decision point outcome.
 */
export enum DecisionType {
  CREATE_STEPS = 'create_steps',
  NO_BRANCHES = 'no_branches',
}

/**
 * Outcome from a decision point handler.
 *
 * Decision handlers return this to indicate which branch(es) of a workflow
 * to execute.
 */
export interface DecisionPointOutcome {
  /** Type of decision made */
  decisionType: DecisionType;
  /** Names of steps to execute next */
  nextStepNames: string[];
  /** Optional dynamically created steps */
  dynamicSteps?: Array<Record<string, unknown>>;
  /** Human-readable reason for the decision */
  reason?: string;
  /** Context data for routing */
  routingContext: Record<string, unknown>;
}

/**
 * Interface for decision-capable handlers.
 *
 * Implement this interface and bind DecisionMixin methods to get decision functionality.
 */
export interface DecisionCapable {
  /** Handler name (from StepHandler) */
  name: string;
  /** Handler version (from StepHandler) */
  version: string;

  // Decision methods
  decisionSuccess(
    steps: string[],
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ): StepHandlerResult;

  decisionSuccessWithOutcome(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult;

  decisionNoBranches(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult;

  skipBranches(
    reason: string,
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ): StepHandlerResult;

  decisionFailure(
    message: string,
    errorType?: string,
    retryable?: boolean,
    metadata?: Record<string, unknown>
  ): StepHandlerResult;
}

/**
 * Implementation of decision methods.
 *
 * TAS-112: Use via interface implementation with method binding.
 *
 * Decision handlers are used to make routing decisions in workflows.
 * They evaluate conditions and determine which steps should execute next.
 */
export class DecisionMixin implements DecisionCapable {
  // These should be defined on the handler class
  get name(): string {
    return (this.constructor as { handlerName?: string }).handlerName || this.constructor.name;
  }

  get version(): string {
    return (this.constructor as { handlerVersion?: string }).handlerVersion || '1.0.0';
  }

  /**
   * Simplified decision success helper (cross-language standard API).
   *
   * Use this when routing to one or more steps based on a decision.
   * This is the recommended method for most decision handlers.
   *
   * @param steps - List of step names to activate
   * @param routingContext - Optional context for routing decisions
   * @param metadata - Optional additional metadata
   * @returns A success StepHandlerResult with the decision outcome
   */
  decisionSuccess(
    steps: string[],
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    const outcome: DecisionPointOutcome = {
      decisionType: DecisionType.CREATE_STEPS,
      nextStepNames: steps,
      routingContext: routingContext || {},
    };

    return this.decisionSuccessWithOutcome(outcome, metadata);
  }

  /**
   * Create a success result with a DecisionPointOutcome.
   *
   * Use this for complex decision outcomes that require dynamic steps
   * or advanced routing. For simple step routing, use `decisionSuccess()`.
   */
  decisionSuccessWithOutcome(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    // Build decision_point_outcome in format Rust expects
    const decisionPointOutcome: Record<string, unknown> = {
      type: outcome.decisionType,
      step_names: outcome.nextStepNames,
    };

    const result: Record<string, unknown> = {
      decision_point_outcome: decisionPointOutcome,
    };

    if (outcome.dynamicSteps) {
      result.dynamic_steps = outcome.dynamicSteps;
    }

    if (outcome.routingContext && Object.keys(outcome.routingContext).length > 0) {
      result.routing_context = outcome.routingContext;
    }

    const combinedMetadata: Record<string, unknown> = { ...(metadata || {}) };
    combinedMetadata.decision_handler = this.name;
    combinedMetadata.decision_version = this.version;

    return StepHandlerResult.success(result, combinedMetadata);
  }

  /**
   * Create a success result for a decision with no branches.
   *
   * Use this when the decision results in no additional steps being executed.
   * This is still a successful outcome - the decision was made correctly,
   * it just doesn't require any follow-up steps.
   */
  decisionNoBranches(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    const decisionPointOutcome: Record<string, unknown> = {
      type: outcome.decisionType,
    };

    const result: Record<string, unknown> = {
      decision_point_outcome: decisionPointOutcome,
    };

    if (outcome.reason) {
      result.reason = outcome.reason;
    }

    if (outcome.routingContext && Object.keys(outcome.routingContext).length > 0) {
      result.routing_context = outcome.routingContext;
    }

    const combinedMetadata: Record<string, unknown> = { ...(metadata || {}) };
    combinedMetadata.decision_handler = this.name;
    combinedMetadata.decision_version = this.version;

    return StepHandlerResult.success(result, combinedMetadata);
  }

  /**
   * Convenience method to skip all branches.
   *
   * @param reason - Human-readable reason for skipping branches
   * @param routingContext - Optional context data
   * @param metadata - Optional additional metadata
   * @returns A success StepHandlerResult indicating no branches
   */
  skipBranches(
    reason: string,
    routingContext?: Record<string, unknown>,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    const outcome: DecisionPointOutcome = {
      decisionType: DecisionType.NO_BRANCHES,
      nextStepNames: [],
      reason,
      routingContext: routingContext || {},
    };

    return this.decisionNoBranches(outcome, metadata);
  }

  /**
   * Create a failure result for a decision that could not be made.
   *
   * Use this when the handler cannot determine the appropriate routing,
   * typically due to invalid input data or missing required information.
   *
   * Decision failures are usually NOT retryable.
   */
  decisionFailure(
    message: string,
    errorType = 'decision_error',
    retryable = false,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    const combinedMetadata: Record<string, unknown> = { ...(metadata || {}) };
    combinedMetadata.decision_handler = this.name;
    combinedMetadata.decision_version = this.version;

    return StepHandlerResult.failure(message, errorType, retryable, combinedMetadata);
  }
}

/**
 * Helper function to apply decision methods to a handler instance.
 *
 * @example
 * ```typescript
 * class MyDecisionHandler extends StepHandler {
 *   constructor() {
 *     super();
 *     applyDecision(this);
 *   }
 * }
 * ```
 */
export function applyDecision<T extends { name: string; version: string }>(
  target: T
): T & DecisionCapable {
  const mixin = new DecisionMixin();

  (target as T & DecisionCapable).decisionSuccess = mixin.decisionSuccess.bind(target);
  (target as T & DecisionCapable).decisionSuccessWithOutcome =
    mixin.decisionSuccessWithOutcome.bind(target);
  (target as T & DecisionCapable).decisionNoBranches = mixin.decisionNoBranches.bind(target);
  (target as T & DecisionCapable).skipBranches = mixin.skipBranches.bind(target);
  (target as T & DecisionCapable).decisionFailure = mixin.decisionFailure.bind(target);

  return target as T & DecisionCapable;
}

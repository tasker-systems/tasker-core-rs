import type { StepHandlerResult } from '../types/step-handler-result';
import { StepHandler } from './base';

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
 * Base class for decision point step handlers.
 *
 * Decision handlers are used to make routing decisions in workflows.
 * They evaluate conditions and determine which steps should execute next.
 *
 * Matches Python's DecisionHandler and Ruby's DecisionHandler (TAS-92 aligned).
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
  get capabilities(): string[] {
    return ['process', 'decision', 'routing'];
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
   *
   * @example Simple routing
   * ```typescript
   * return this.decisionSuccess(['process_order']);
   * ```
   *
   * @example With routing context
   * ```typescript
   * return this.decisionSuccess(
   *   ['validate_premium', 'process_premium'],
   *   { tier: 'premium' }
   * );
   * ```
   */
  protected decisionSuccess(
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
  protected decisionSuccessWithOutcome(
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

    return this.success(result, combinedMetadata);
  }

  /**
   * Create a success result for a decision with no branches.
   *
   * Use this when the decision results in no additional steps being executed.
   * This is still a successful outcome - the decision was made correctly,
   * it just doesn't require any follow-up steps.
   */
  protected decisionNoBranches(
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

    return this.success(result, combinedMetadata);
  }

  /**
   * Convenience method to skip all branches.
   *
   * @param reason - Human-readable reason for skipping branches
   * @param routingContext - Optional context data
   * @param metadata - Optional additional metadata
   * @returns A success StepHandlerResult indicating no branches
   *
   * @example
   * ```typescript
   * if (!itemsToProcess.length) {
   *   return this.skipBranches('No items require processing');
   * }
   * ```
   */
  protected skipBranches(
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
   *
   * @example
   * ```typescript
   * if (!('order_type' in context.inputData)) {
   *   return this.decisionFailure(
   *     'Missing required field: order_type',
   *     'missing_field'
   *   );
   * }
   * ```
   */
  protected decisionFailure(
    message: string,
    errorType = 'decision_error',
    retryable = false,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    const combinedMetadata: Record<string, unknown> = { ...(metadata || {}) };
    combinedMetadata.decision_handler = this.name;
    combinedMetadata.decision_version = this.version;

    return this.failure(message, errorType, retryable, combinedMetadata);
  }
}

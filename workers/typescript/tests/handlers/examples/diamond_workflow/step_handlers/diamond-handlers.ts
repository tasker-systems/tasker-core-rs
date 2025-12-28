/**
 * Diamond Workflow Step Handlers for E2E Testing.
 *
 * Implements the diamond pattern workflow:
 * 1. Start: Square the input (n → n²)
 * 2. Branch B: Add 25 to squared result (n² + 25)
 * 3. Branch C: Multiply squared result by 2 (n² × 2)
 * 4. End: Average both branch results ((b + c) / 2)
 *
 * Diamond Pattern:
 *     [start]
 *       / \
 *    [B]   [C]
 *       \ /
 *      [end]
 *
 * Matches Ruby and Python diamond workflow implementations for testing parity.
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Diamond Start: Square the initial even number.
 *
 * Input: even_number from task context
 * Output: { squared_value: n² }
 */
export class DiamondStartHandler extends StepHandler {
  static handlerName = 'diamond_workflow.step_handlers.DiamondStartHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const evenNumber = context.getInput<number>('even_number');

    if (evenNumber === undefined || evenNumber === null) {
      return this.failure('Missing required input: even_number', 'validation_error', false);
    }

    if (evenNumber % 2 !== 0) {
      return this.failure(
        `Input must be an even number, got: ${evenNumber}`,
        'validation_error',
        false
      );
    }

    const squaredValue = evenNumber * evenNumber;

    return this.success({
      squared_value: squaredValue,
      operation: 'square',
      input: evenNumber,
    });
  }
}

/**
 * Diamond Branch B: Add constant to squared result.
 *
 * Input: squared_value from diamond_start dependency
 * Output: { branch_b_value: n² + 25 }
 */
export class DiamondBranchBHandler extends StepHandler {
  static handlerName = 'diamond_workflow.step_handlers.DiamondBranchBHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const startResult = context.getDependencyResult('diamond_start_ts') as Record<
      string,
      unknown
    > | null;

    if (!startResult) {
      return this.failure(
        'Missing dependency result from diamond_start_ts',
        'dependency_error',
        false
      );
    }

    const squaredValue = startResult.squared_value as number;
    const constant = 25;
    const branchBValue = squaredValue + constant;

    return this.success({
      branch_b_value: branchBValue,
      operation: 'add',
      constant,
      input: squaredValue,
    });
  }
}

/**
 * Diamond Branch C: Multiply squared result by factor.
 *
 * Input: squared_value from diamond_start dependency
 * Output: { branch_c_value: n² × 2 }
 */
export class DiamondBranchCHandler extends StepHandler {
  static handlerName = 'diamond_workflow.step_handlers.DiamondBranchCHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const startResult = context.getDependencyResult('diamond_start_ts') as Record<
      string,
      unknown
    > | null;

    if (!startResult) {
      return this.failure(
        'Missing dependency result from diamond_start_ts',
        'dependency_error',
        false
      );
    }

    const squaredValue = startResult.squared_value as number;
    const factor = 2;
    const branchCValue = squaredValue * factor;

    return this.success({
      branch_c_value: branchCValue,
      operation: 'multiply',
      factor,
      input: squaredValue,
    });
  }
}

/**
 * Diamond End: Average results from both branches (convergence).
 *
 * Input: branch_b_value and branch_c_value from both branches
 * Output: { final_value: (b + c) / 2 }
 */
export class DiamondEndHandler extends StepHandler {
  static handlerName = 'diamond_workflow.step_handlers.DiamondEndHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const branchBResult = context.getDependencyResult('diamond_branch_b_ts') as Record<
      string,
      unknown
    > | null;
    const branchCResult = context.getDependencyResult('diamond_branch_c_ts') as Record<
      string,
      unknown
    > | null;

    if (!branchBResult) {
      return this.failure(
        'Missing dependency result from diamond_branch_b_ts',
        'dependency_error',
        false
      );
    }

    if (!branchCResult) {
      return this.failure(
        'Missing dependency result from diamond_branch_c_ts',
        'dependency_error',
        false
      );
    }

    const branchBValue = branchBResult.branch_b_value as number;
    const branchCValue = branchCResult.branch_c_value as number;
    const finalValue = (branchBValue + branchCValue) / 2;

    return this.success({
      final_value: finalValue,
      operation: 'average',
      branch_b_value: branchBValue,
      branch_c_value: branchCValue,
    });
  }
}

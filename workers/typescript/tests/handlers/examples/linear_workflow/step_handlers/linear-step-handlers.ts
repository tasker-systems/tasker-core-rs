/**
 * Linear Workflow Step Handlers for E2E Testing.
 *
 * Implements the mathematical sequence workflow:
 * 1. Step 1: Square the input (n → n²)
 * 2. Step 2: Add constant (n² + 10)
 * 3. Step 3: Multiply by factor ((n² + 10) × 3)
 * 4. Step 4: Divide for final result (((n² + 10) × 3) ÷ 2)
 *
 * Matches Ruby and Python linear workflow implementations for testing parity.
 *
 * TAS-137 Best Practices Demonstrated:
 * - getInput<T>() for task context field access
 * - getDependencyResult() for upstream step results (auto-unwraps {"result": value})
 * - getDependencyField() for nested field extraction from dependencies
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Step 1: Square the initial even number.
 *
 * Input: even_number from task context
 * Output: { squared_value: n² }
 *
 * TAS-137 Best Practices:
 * - Uses getInput<T>() for task context access (cross-language standard)
 */
export class LinearStep1Handler extends StepHandler {
  static handlerName = 'LinearWorkflow.StepHandlers.LinearStep1Handler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getInput() for task context access (cross-language standard)
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
 * Step 2: Add constant to squared result.
 *
 * Input: squared_value from Step 1 dependency
 * Output: { added_value: n² + 10 }
 *
 * TAS-137 Best Practices:
 * - Uses getDependencyField() for nested field extraction from dependencies
 */
export class LinearStep2Handler extends StepHandler {
  static handlerName = 'LinearWorkflow.StepHandlers.LinearStep2Handler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getDependencyField() for nested field extraction
    // This replaces the two-step pattern: getDependencyResult().field
    const squaredValue = context.getDependencyField('linear_step_1', 'squared_value') as
      | number
      | null;

    if (squaredValue === null || squaredValue === undefined) {
      return this.failure(
        'Missing dependency result from linear_step_1',
        'dependency_error',
        false
      );
    }
    const constant = 10;
    const addedValue = squaredValue + constant;

    return this.success({
      added_value: addedValue,
      operation: 'add',
      constant,
      input: squaredValue,
    });
  }
}

/**
 * Step 3: Multiply by factor.
 *
 * Input: added_value from Step 2 dependency
 * Output: { multiplied_value: (n² + 10) × 3 }
 *
 * TAS-137 Best Practices:
 * - Uses getDependencyField() for nested field extraction from dependencies
 */
export class LinearStep3Handler extends StepHandler {
  static handlerName = 'LinearWorkflow.StepHandlers.LinearStep3Handler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getDependencyField() for nested field extraction
    const addedValue = context.getDependencyField('linear_step_2', 'added_value') as number | null;

    if (addedValue === null || addedValue === undefined) {
      return this.failure(
        'Missing dependency result from linear_step_2',
        'dependency_error',
        false
      );
    }
    const factor = 3;
    const multipliedValue = addedValue * factor;

    return this.success({
      multiplied_value: multipliedValue,
      operation: 'multiply',
      factor,
      input: addedValue,
    });
  }
}

/**
 * Step 4: Divide for final result.
 *
 * Input: multiplied_value from Step 3 dependency
 * Output: { final_value: ((n² + 10) × 3) ÷ 2 }
 *
 * TAS-137 Best Practices:
 * - Uses getDependencyField() for nested field extraction from dependencies
 */
export class LinearStep4Handler extends StepHandler {
  static handlerName = 'LinearWorkflow.StepHandlers.LinearStep4Handler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getDependencyField() for nested field extraction
    const multipliedValue = context.getDependencyField('linear_step_3', 'multiplied_value') as
      | number
      | null;

    if (multipliedValue === null || multipliedValue === undefined) {
      return this.failure(
        'Missing dependency result from linear_step_3',
        'dependency_error',
        false
      );
    }
    const divisor = 2;
    const finalValue = multipliedValue / divisor;

    return this.success({
      final_value: finalValue,
      operation: 'divide',
      divisor,
      input: multipliedValue,
    });
  }
}

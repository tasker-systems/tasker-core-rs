import type { FfiStepEvent } from '../ffi/types';

/**
 * Parameters for constructing a StepContext.
 */
export interface StepContextParams {
  event: FfiStepEvent;
  taskUuid: string;
  stepUuid: string;
  correlationId: string;
  handlerName: string;
  inputData: Record<string, unknown>;
  dependencyResults: Record<string, unknown>;
  stepConfig: Record<string, unknown>;
  stepInputs: Record<string, unknown>;
  retryCount: number;
  maxRetries: number;
}

/**
 * Context provided to step handlers during execution.
 *
 * Contains all information needed for a step handler to execute,
 * including input data, dependency results, and configuration.
 *
 * Matches Python's StepContext and Ruby's StepContext (post-TAS-96).
 *
 * @example
 * ```typescript
 * class MyHandler extends StepHandler {
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const orderId = context.inputData['order_id'];
 *     const previousResult = context.getDependencyResult('step_1');
 *     // ... handler logic ...
 *     return this.success({ processed: true });
 *   }
 * }
 * ```
 */
export class StepContext {
  /** The original FFI step event */
  public readonly event: FfiStepEvent;

  /** Task UUID */
  public readonly taskUuid: string;

  /** Step UUID */
  public readonly stepUuid: string;

  /** Correlation ID for tracing */
  public readonly correlationId: string;

  /** Name of the handler being executed */
  public readonly handlerName: string;

  /** Input data for the handler (from task context) */
  public readonly inputData: Record<string, unknown>;

  /** Results from dependent steps */
  public readonly dependencyResults: Record<string, unknown>;

  /** Handler-specific configuration (from step_definition.handler.initialization) */
  public readonly stepConfig: Record<string, unknown>;

  /** Step-specific inputs (from workflow_step.inputs, used for batch cursor config) */
  public readonly stepInputs: Record<string, unknown>;

  /** Current retry attempt number */
  public readonly retryCount: number;

  /** Maximum retry attempts allowed */
  public readonly maxRetries: number;

  constructor(params: StepContextParams) {
    this.event = params.event;
    this.taskUuid = params.taskUuid;
    this.stepUuid = params.stepUuid;
    this.correlationId = params.correlationId;
    this.handlerName = params.handlerName;
    this.inputData = params.inputData;
    this.dependencyResults = params.dependencyResults;
    this.stepConfig = params.stepConfig;
    this.stepInputs = params.stepInputs;
    this.retryCount = params.retryCount;
    this.maxRetries = params.maxRetries;
  }

  /**
   * Create a StepContext from an FFI event.
   *
   * Extracts input data, dependency results, and configuration from
   * the task_sequence_step payload.
   *
   * The FFI data structure mirrors the Ruby TaskSequenceStepWrapper:
   * - task.context -> inputData (task context with user inputs)
   * - dependency_results -> results from parent steps
   * - step_definition.handler.initialization -> stepConfig
   * - workflow_step.attempts -> retryCount
   * - workflow_step.max_attempts -> maxRetries
   * - workflow_step.inputs -> stepInputs
   *
   * @param event - The FFI step event
   * @param handlerName - Name of the handler to execute
   * @returns A StepContext populated from the event
   */
  static fromFfiEvent(event: FfiStepEvent, handlerName: string): StepContext {
    // Extract task context (inputData) from the task structure
    // The task field contains task metadata including context
    const task = event.task ?? {};
    const inputData = (task.context as Record<string, unknown>) ?? {};

    // Extract dependency results
    const dependencyResults = (event.dependency_results as Record<string, unknown>) ?? {};

    // Extract step config from handler initialization
    const stepDefinition = event.step_definition ?? {};
    const handlerConfig = stepDefinition.handler ?? {};
    const stepConfig = (handlerConfig.initialization as Record<string, unknown>) ?? {};

    // Extract retry information and step inputs from workflow_step
    const workflowStep = event.workflow_step ?? {};
    const retryCount = workflowStep.attempts ?? 0;
    const maxRetries = workflowStep.max_attempts ?? 3;
    const stepInputs = (workflowStep.inputs as Record<string, unknown>) ?? {};

    return new StepContext({
      event,
      taskUuid: event.task_uuid,
      stepUuid: event.step_uuid,
      correlationId: event.correlation_id,
      handlerName,
      inputData,
      dependencyResults,
      stepConfig,
      stepInputs,
      retryCount,
      maxRetries,
    });
  }

  /**
   * Get the computed result value from a dependency step.
   *
   * This method extracts the actual computed value from a dependency result,
   * unwrapping any nested structure. Matches Python's get_dependency_result().
   *
   * The dependency result structure can be:
   * - {"result": actual_value} - unwraps to actual_value
   * - primitive value - returns as-is
   *
   * @param stepName - Name of the dependency step
   * @returns The computed result value, or null if not found
   *
   * @example
   * ```typescript
   * // Instead of:
   * const step1Result = context.dependencyResults['step_1'] || {};
   * const value = step1Result.result;  // Might be nested!
   *
   * // Use:
   * const value = context.getDependencyResult('step_1');  // Unwrapped
   * ```
   */
  getDependencyResult(stepName: string): unknown {
    const resultHash = this.dependencyResults[stepName];
    if (resultHash === undefined || resultHash === null) {
      return null;
    }

    // If it's an object with a 'result' key, extract that value
    if (typeof resultHash === 'object' && resultHash !== null && 'result' in resultHash) {
      return (resultHash as Record<string, unknown>).result;
    }

    // Otherwise return the whole thing (might be a primitive value)
    return resultHash;
  }

  /**
   * Get a value from the input data.
   *
   * @param key - The key to look up in inputData
   * @returns The value or undefined if not found
   */
  getInput<T = unknown>(key: string): T | undefined {
    return this.inputData[key] as T | undefined;
  }

  /**
   * Get a value from the step configuration.
   *
   * @param key - The key to look up in stepConfig
   * @returns The value or undefined if not found
   */
  getConfig<T = unknown>(key: string): T | undefined {
    return this.stepConfig[key] as T | undefined;
  }

  /**
   * Check if this is a retry attempt.
   *
   * @returns True if retryCount > 0
   */
  isRetry(): boolean {
    return this.retryCount > 0;
  }

  /**
   * Check if this is the last allowed retry attempt.
   *
   * @returns True if retryCount >= maxRetries - 1
   */
  isLastRetry(): boolean {
    return this.retryCount >= this.maxRetries - 1;
  }

  /**
   * Get a value from the input data with a default.
   *
   * @param key - The key to look up in inputData
   * @param defaultValue - Value to return if key not found or undefined
   * @returns The value or default if not found/undefined
   *
   * @example
   * ```typescript
   * const batchSize = context.getInputOr('batch_size', 100);
   * ```
   */
  getInputOr<T = unknown>(key: string, defaultValue: T): T {
    const value = this.inputData[key];
    return value === undefined ? defaultValue : (value as T);
  }

  /**
   * Extract a nested field from a dependency result.
   *
   * Useful when dependency results are complex objects and you need
   * to extract a specific nested value without manual object traversal.
   *
   * @param stepName - Name of the dependency step
   * @param path - Path elements to traverse into the result
   * @returns The nested value, or null if not found
   *
   * @example
   * ```typescript
   * // Extract nested field from dependency result
   * const csvPath = context.getDependencyField('analyze_csv', 'csv_file_path');
   * // Multiple levels deep
   * const value = context.getDependencyField('step_1', 'data', 'items');
   * ```
   */
  getDependencyField(stepName: string, ...path: string[]): unknown {
    let result = this.getDependencyResult(stepName);
    if (result === null || result === undefined) {
      return null;
    }
    for (const key of path) {
      if (typeof result !== 'object' || result === null) {
        return null;
      }
      result = (result as Record<string, unknown>)[key];
    }
    return result;
  }

  // ===========================================================================
  // CHECKPOINT ACCESSORS (TAS-125 Batch Processing Support)
  // ===========================================================================

  /**
   * Get the raw checkpoint data from the workflow step.
   *
   * @returns The checkpoint data object or null if not set
   */
  get checkpoint(): Record<string, unknown> | null {
    const workflowStep = this.event.workflow_step ?? {};
    return (workflowStep.checkpoint as Record<string, unknown>) ?? null;
  }

  /**
   * Get the checkpoint cursor position.
   *
   * The cursor represents the current position in batch processing,
   * allowing handlers to resume from where they left off.
   *
   * @returns The cursor value (number, string, or object) or null if not set
   *
   * @example
   * ```typescript
   * const cursor = context.checkpointCursor;
   * const startFrom = cursor ?? 0;
   * ```
   */
  get checkpointCursor(): unknown {
    return this.checkpoint?.cursor ?? null;
  }

  /**
   * Get the number of items processed in the current batch run.
   *
   * @returns Number of items processed (0 if no checkpoint)
   */
  get checkpointItemsProcessed(): number {
    return (this.checkpoint?.items_processed as number) ?? 0;
  }

  /**
   * Get the accumulated results from batch processing.
   *
   * Accumulated results allow handlers to maintain running totals
   * or aggregated state across checkpoint boundaries.
   *
   * @returns The accumulated results object or null if not set
   *
   * @example
   * ```typescript
   * const totals = context.accumulatedResults ?? {};
   * const currentSum = totals.sum ?? 0;
   * ```
   */
  get accumulatedResults(): Record<string, unknown> | null {
    return (this.checkpoint?.accumulated_results as Record<string, unknown>) ?? null;
  }

  /**
   * Check if a checkpoint exists for this step.
   *
   * @returns True if a checkpoint cursor exists
   *
   * @example
   * ```typescript
   * if (context.hasCheckpoint()) {
   *   console.log(`Resuming from cursor: ${context.checkpointCursor}`);
   * }
   * ```
   */
  hasCheckpoint(): boolean {
    return this.checkpointCursor !== null;
  }

  /**
   * Get all dependency result keys.
   *
   * @returns Array of step names that have dependency results
   */
  getDependencyResultKeys(): string[] {
    return Object.keys(this.dependencyResults);
  }

  /**
   * Get all dependency results matching a step name prefix.
   *
   * This is useful for batch processing where multiple worker steps
   * share a common prefix (e.g., "process_batch_001", "process_batch_002").
   *
   * Returns the unwrapped result values (same as getDependencyResult).
   *
   * @param prefix - Step name prefix to match
   * @returns Array of unwrapped result values from matching steps
   *
   * @example
   * ```typescript
   * // For batch worker results named: process_batch_001, process_batch_002, etc.
   * const batchResults = context.getAllDependencyResults('process_batch_');
   * const total = batchResults.reduce((sum, r) => sum + r.count, 0);
   * ```
   */
  getAllDependencyResults(prefix: string): unknown[] {
    const results: unknown[] = [];

    for (const key of Object.keys(this.dependencyResults)) {
      if (key.startsWith(prefix)) {
        const result = this.getDependencyResult(key);
        if (result !== null) {
          results.push(result);
        }
      }
    }

    return results;
  }
}

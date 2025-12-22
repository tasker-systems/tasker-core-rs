import type { StepContext } from "../types/step-context";
import { StepHandlerResult } from "../types/step-handler-result";
import { ErrorType } from "../types/error-type";

/**
 * Interface for step handler class metadata.
 *
 * Handler classes must implement these static properties.
 */
export interface StepHandlerClass {
  /** Unique identifier for this handler. Must match step definition. */
  handlerName: string;

  /** Version string for the handler (default: "1.0.0") */
  handlerVersion?: string;

  /** Constructor that creates a handler instance */
  new (): StepHandler;
}

/**
 * Abstract base class for step handlers.
 *
 * All step handlers must extend this class and implement
 * the `call` method. The handlerName static property must be set
 * to a unique identifier for the handler.
 *
 * Matches Python's StepHandler and Ruby's StepHandler base classes.
 *
 * @example
 * ```typescript
 * class ProcessOrderHandler extends StepHandler {
 *   static handlerName = 'process_order';
 *   static handlerVersion = '1.0.0';
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const orderId = context.getInput<string>('order_id');
 *     // Process the order...
 *     return this.success({ order_id: orderId, status: 'processed' });
 *   }
 * }
 * ```
 */
export abstract class StepHandler {
  /**
   * Unique identifier for this handler.
   * Must be set by subclasses and match the step definition.
   */
  static handlerName: string;

  /**
   * Version string for the handler.
   * Default: "1.0.0"
   */
  static handlerVersion = "1.0.0";

  /**
   * Execute the step handler logic.
   *
   * This method is called by the execution subscriber when a step
   * event is received that matches this handler's name.
   *
   * @param context - Execution context with input data, dependency results,
   *                  and configuration
   * @returns Promise resolving to StepHandlerResult indicating success or failure
   *
   * @example
   * ```typescript
   * async call(context: StepContext): Promise<StepHandlerResult> {
   *   try {
   *     const result = await processData(context.inputData);
   *     return this.success(result);
   *   } catch (error) {
   *     return this.failure(
   *       error.message,
   *       ErrorType.HANDLER_ERROR,
   *       true
   *     );
   *   }
   * }
   * ```
   */
  abstract call(context: StepContext): Promise<StepHandlerResult>;

  /**
   * Get the handler name.
   *
   * @returns The handlerName static property, or the class name if not set
   */
  get name(): string {
    const ctor = this.constructor as typeof StepHandler;
    return ctor.handlerName || ctor.name;
  }

  /**
   * Get the handler version.
   *
   * @returns The handlerVersion static property
   */
  get version(): string {
    const ctor = this.constructor as typeof StepHandler;
    return ctor.handlerVersion;
  }

  /**
   * Return handler capabilities.
   *
   * Override this to advertise specific capabilities for handler selection.
   *
   * @returns List of capability strings (default: ["process"])
   */
  get capabilities(): string[] {
    return ["process"];
  }

  /**
   * Return JSON schema for handler configuration.
   *
   * Override this to provide a schema for validating step_config.
   *
   * @returns JSON schema object, or null if no schema is defined
   */
  configSchema(): Record<string, unknown> | null {
    return null;
  }

  /**
   * Create a success result.
   *
   * Convenience method for creating success results.
   *
   * @param result - Result data object
   * @param metadata - Optional metadata object
   * @returns StepHandlerResult with success=true
   *
   * @example
   * ```typescript
   * return this.success({ processed: 100 });
   * ```
   */
  protected success(
    result: Record<string, unknown>,
    metadata?: Record<string, unknown>,
  ): StepHandlerResult {
    return StepHandlerResult.success(result, metadata);
  }

  /**
   * Create a failure result.
   *
   * Convenience method for creating failure results.
   *
   * @param message - Error message
   * @param errorType - Error type classification. Use ErrorType enum for consistency.
   * @param retryable - Whether the error is retryable (default: true)
   * @param metadata - Optional metadata object
   * @param errorCode - Optional application-specific error code
   * @returns StepHandlerResult with success=false
   *
   * @example
   * ```typescript
   * return this.failure(
   *   'Invalid input',
   *   ErrorType.VALIDATION_ERROR,
   *   false
   * );
   * ```
   */
  protected failure(
    message: string,
    errorType: ErrorType | string = ErrorType.HANDLER_ERROR,
    retryable = true,
    metadata?: Record<string, unknown>,
    errorCode?: string,
  ): StepHandlerResult {
    return StepHandlerResult.failure(
      message,
      errorType,
      retryable,
      metadata,
      errorCode,
    );
  }

  /**
   * Get a string representation of the handler.
   */
  toString(): string {
    return `${this.constructor.name}(name=${this.name}, version=${this.version})`;
  }
}

/**
 * Step execution subscriber for TypeScript workers.
 *
 * Subscribes to step execution events from the EventPoller and dispatches
 * them to the appropriate handlers via the HandlerRegistry.
 *
 * Matches Python's StepExecutionSubscriber pattern (TAS-92 aligned).
 */

import pino, { type Logger, type LoggerOptions } from 'pino';
import type { StepExecutionReceivedPayload, TaskerEventEmitter } from '../events/event-emitter.js';
import { StepEventNames } from '../events/event-names.js';
import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import type { CheckpointYieldData, FfiStepEvent, StepExecutionResult } from '../ffi/types.js';
import type { StepHandler } from '../handler/base.js';
import { logDebug, logError, logInfo, logWarn } from '../logging/index.js';
import { StepContext } from '../types/step-context.js';
import type { StepHandlerResult } from '../types/step-handler-result.js';

// Create a pino logger for the subscriber (for debugging)
const loggerOptions: LoggerOptions = {
  name: 'step-subscriber',
  level: process.env.RUST_LOG ?? 'info',
};

// Add pino-pretty transport in non-production environments
if (process.env.TASKER_ENV !== 'production') {
  loggerOptions.transport = {
    target: 'pino-pretty',
    options: { colorize: true },
  };
}

const pinoLog: Logger = pino(loggerOptions);

/**
 * Interface for handler registry required by StepExecutionSubscriber.
 */
export interface HandlerRegistryInterface {
  /** Resolve and instantiate a handler by name */
  resolve(name: string): StepHandler | null;
}

/**
 * Configuration for the step execution subscriber.
 */
export interface StepExecutionSubscriberConfig {
  /** Worker ID for result attribution */
  workerId?: string;

  /** Maximum concurrent handler executions (default: 10) */
  maxConcurrent?: number;

  /** Handler execution timeout in milliseconds (default: 300000 = 5 minutes) */
  handlerTimeoutMs?: number;
}

/**
 * Subscribes to step execution events and dispatches them to handlers.
 *
 * This is the critical component that connects the FFI event stream
 * to TypeScript handler execution. It:
 * 1. Listens for step events from the EventPoller via EventEmitter
 * 2. Resolves the appropriate handler from the HandlerRegistry
 * 3. Creates a StepContext from the FFI event
 * 4. Executes the handler
 * 5. Submits the result back to Rust via FFI
 *
 * @example
 * ```typescript
 * const subscriber = new StepExecutionSubscriber(
 *   eventEmitter,
 *   handlerRegistry,
 *   runtime,
 *   { workerId: 'worker-1' }
 * );
 *
 * subscriber.start();
 *
 * // Later...
 * subscriber.stop();
 * ```
 */
export class StepExecutionSubscriber {
  private readonly emitter: TaskerEventEmitter;
  private readonly registry: HandlerRegistryInterface;
  private readonly runtime: TaskerRuntime;
  private readonly workerId: string;
  private readonly maxConcurrent: number;
  private readonly handlerTimeoutMs: number;

  private running = false;
  private activeHandlers = 0;
  private processedCount = 0;
  private errorCount = 0;

  /**
   * Create a new StepExecutionSubscriber.
   *
   * @param emitter - The event emitter to subscribe to (required, no fallback)
   * @param registry - The handler registry for resolving step handlers
   * @param runtime - The FFI runtime for submitting results (required, no fallback)
   * @param config - Optional configuration for execution behavior
   */
  constructor(
    emitter: TaskerEventEmitter,
    registry: HandlerRegistryInterface,
    runtime: TaskerRuntime,
    config: StepExecutionSubscriberConfig = {}
  ) {
    this.emitter = emitter;
    this.registry = registry;
    this.runtime = runtime;
    this.workerId = config.workerId ?? `typescript-worker-${process.pid}`;
    this.maxConcurrent = config.maxConcurrent ?? 10;
    this.handlerTimeoutMs = config.handlerTimeoutMs ?? 300000;
  }

  /**
   * Start subscribing to step execution events.
   */
  start(): void {
    pinoLog.info(
      { component: 'subscriber', emitterInstanceId: this.emitter.getInstanceId() },
      'StepExecutionSubscriber.start() called'
    );

    if (this.running) {
      logWarn('StepExecutionSubscriber already running', {
        component: 'subscriber',
      });
      return;
    }

    this.running = true;
    this.processedCount = 0;
    this.errorCount = 0;

    // Subscribe to step events
    pinoLog.info(
      {
        component: 'subscriber',
        eventName: StepEventNames.STEP_EXECUTION_RECEIVED,
        emitterInstanceId: this.emitter.getInstanceId(),
      },
      'Registering event listener on emitter'
    );

    this.emitter.on(
      StepEventNames.STEP_EXECUTION_RECEIVED,
      (payload: StepExecutionReceivedPayload) => {
        try {
          pinoLog.info(
            {
              component: 'subscriber',
              eventId: payload.event.event_id,
              stepUuid: payload.event.step_uuid,
            },
            'Received step event in subscriber callback!'
          );
          // Extract the event from the payload wrapper
          pinoLog.info({ component: 'subscriber' }, 'About to call handleEvent from callback');
          this.handleEvent(payload.event);
          pinoLog.info({ component: 'subscriber' }, 'handleEvent returned from callback');
        } catch (error) {
          pinoLog.error(
            {
              component: 'subscriber',
              error: error instanceof Error ? error.message : String(error),
              stack: error instanceof Error ? error.stack : undefined,
            },
            'EXCEPTION in event listener callback!'
          );
        }
      }
    );

    pinoLog.info(
      { component: 'subscriber', workerId: this.workerId },
      'StepExecutionSubscriber started successfully'
    );

    logInfo('StepExecutionSubscriber started', {
      component: 'subscriber',
      operation: 'start',
      worker_id: this.workerId,
    });
  }

  /**
   * Stop subscribing to step execution events.
   *
   * Note: Does not wait for in-flight handlers to complete.
   * Use waitForCompletion() if you need to wait.
   */
  stop(): void {
    if (!this.running) {
      return;
    }

    this.running = false;
    this.emitter.removeAllListeners(StepEventNames.STEP_EXECUTION_RECEIVED);

    logInfo('StepExecutionSubscriber stopped', {
      component: 'subscriber',
      operation: 'stop',
      processed_count: String(this.processedCount),
      error_count: String(this.errorCount),
    });
  }

  /**
   * Check if the subscriber is running.
   */
  isRunning(): boolean {
    return this.running;
  }

  /**
   * Get the count of events processed.
   */
  getProcessedCount(): number {
    return this.processedCount;
  }

  /**
   * Get the count of errors encountered.
   */
  getErrorCount(): number {
    return this.errorCount;
  }

  /**
   * Get the count of currently active handlers.
   */
  getActiveHandlers(): number {
    return this.activeHandlers;
  }

  /**
   * Wait for all active handlers to complete.
   *
   * @param timeoutMs - Maximum time to wait (default: 30000)
   * @returns True if all handlers completed, false if timeout
   */
  async waitForCompletion(timeoutMs = 30000): Promise<boolean> {
    const startTime = Date.now();
    const checkInterval = 100;

    while (this.activeHandlers > 0) {
      if (Date.now() - startTime > timeoutMs) {
        logWarn('Timeout waiting for handlers to complete', {
          component: 'subscriber',
          active_handlers: String(this.activeHandlers),
        });
        return false;
      }
      await new Promise((resolve) => setTimeout(resolve, checkInterval));
    }

    return true;
  }

  /**
   * Handle a step execution event.
   */
  private handleEvent(event: FfiStepEvent): void {
    pinoLog.info(
      {
        component: 'subscriber',
        eventId: event.event_id,
        running: this.running,
        activeHandlers: this.activeHandlers,
        maxConcurrent: this.maxConcurrent,
      },
      'handleEvent() called'
    );

    if (!this.running) {
      pinoLog.warn(
        { component: 'subscriber', eventId: event.event_id },
        'Received event while stopped, ignoring'
      );
      return;
    }

    // Check concurrency limit
    if (this.activeHandlers >= this.maxConcurrent) {
      pinoLog.warn(
        {
          component: 'subscriber',
          activeHandlers: this.activeHandlers,
          maxConcurrent: this.maxConcurrent,
        },
        'Max concurrent handlers reached, event will be re-polled'
      );
      // Don't process - event stays in FFI queue and will be re-polled
      return;
    }

    pinoLog.info(
      { component: 'subscriber', eventId: event.event_id },
      'About to call processEvent()'
    );

    // Process asynchronously
    this.processEvent(event).catch((error) => {
      pinoLog.error(
        {
          component: 'subscriber',
          eventId: event.event_id,
          error: error instanceof Error ? error.message : String(error),
          stack: error instanceof Error ? error.stack : undefined,
        },
        'Unhandled error in processEvent'
      );
    });
  }

  /**
   * Process a step execution event.
   */
  private async processEvent(event: FfiStepEvent): Promise<void> {
    pinoLog.info({ component: 'subscriber', eventId: event.event_id }, 'processEvent() starting');

    this.activeHandlers++;
    const startTime = Date.now();

    try {
      // Extract handler name from step definition
      const handlerName = this.extractHandlerName(event);
      pinoLog.info(
        { component: 'subscriber', eventId: event.event_id, handlerName },
        'Extracted handler name'
      );

      if (!handlerName) {
        pinoLog.error(
          { component: 'subscriber', eventId: event.event_id },
          'No handler name found!'
        );
        await this.submitErrorResult(event, 'No handler name found in step definition', startTime);
        return;
      }

      pinoLog.info(
        {
          component: 'subscriber',
          eventId: event.event_id,
          stepUuid: event.step_uuid,
          handlerName,
        },
        'Processing step event'
      );

      // Emit started event
      this.emitter.emit(StepEventNames.STEP_EXECUTION_STARTED, {
        eventId: event.event_id,
        stepUuid: event.step_uuid,
        handlerName,
        timestamp: new Date(),
      });

      // Resolve handler from registry
      pinoLog.info({ component: 'subscriber', handlerName }, 'Resolving handler from registry...');
      const handler = this.registry.resolve(handlerName);
      pinoLog.info(
        { component: 'subscriber', handlerName, handlerFound: !!handler },
        'Handler resolution result'
      );

      if (!handler) {
        pinoLog.error({ component: 'subscriber', handlerName }, 'Handler not found in registry!');
        await this.submitErrorResult(event, `Handler not found: ${handlerName}`, startTime);
        return;
      }

      // Create context from FFI event
      pinoLog.info({ component: 'subscriber', handlerName }, 'Creating StepContext from FFI event');
      const context = StepContext.fromFfiEvent(event, handlerName);
      pinoLog.info(
        { component: 'subscriber', handlerName },
        'StepContext created, executing handler'
      );

      // Execute handler with timeout
      const result = await this.executeWithTimeout(
        () => handler.call(context),
        this.handlerTimeoutMs
      );

      pinoLog.info(
        { component: 'subscriber', handlerName, success: result.success },
        'Handler execution completed'
      );

      const executionTimeMs = Date.now() - startTime;

      // Submit result to Rust
      await this.submitResult(event, result, executionTimeMs);

      // Emit completed/failed event
      if (result.success) {
        this.emitter.emit(StepEventNames.STEP_EXECUTION_COMPLETED, {
          eventId: event.event_id,
          stepUuid: event.step_uuid,
          handlerName,
          executionTimeMs,
          timestamp: new Date(),
        });
      } else {
        this.emitter.emit(StepEventNames.STEP_EXECUTION_FAILED, {
          eventId: event.event_id,
          stepUuid: event.step_uuid,
          handlerName,
          error: result.errorMessage,
          executionTimeMs,
          timestamp: new Date(),
        });
      }

      this.processedCount++;
    } catch (error) {
      this.errorCount++;
      const errorMessage = error instanceof Error ? error.message : String(error);

      logError('Handler execution failed', {
        component: 'subscriber',
        event_id: event.event_id,
        step_uuid: event.step_uuid,
        error_message: errorMessage,
      });

      await this.submitErrorResult(event, errorMessage, startTime);

      this.emitter.emit(StepEventNames.STEP_EXECUTION_FAILED, {
        eventId: event.event_id,
        stepUuid: event.step_uuid,
        error: errorMessage,
        executionTimeMs: Date.now() - startTime,
        timestamp: new Date(),
      });
    } finally {
      this.activeHandlers--;
    }
  }

  /**
   * Execute a function with a timeout.
   */
  private async executeWithTimeout<T>(fn: () => Promise<T>, timeoutMs: number): Promise<T> {
    return Promise.race([
      fn(),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error(`Handler execution timed out after ${timeoutMs}ms`)),
          timeoutMs
        )
      ),
    ]);
  }

  /**
   * Extract handler name from FFI event.
   *
   * The handler name is in step_definition.handler.callable
   */
  private extractHandlerName(event: FfiStepEvent): string | null {
    const stepDefinition = event.step_definition;
    if (!stepDefinition) {
      return null;
    }

    const handler = stepDefinition.handler;
    if (!handler) {
      return null;
    }

    return handler.callable || null;
  }

  /**
   * Submit a handler result via FFI.
   *
   * TAS-125: Detects checkpoint yields and routes them to checkpointYieldStepEvent
   * instead of the normal completion path.
   */
  private async submitResult(
    event: FfiStepEvent,
    result: StepHandlerResult,
    executionTimeMs: number
  ): Promise<void> {
    pinoLog.info(
      { component: 'subscriber', eventId: event.event_id, runtimeLoaded: this.runtime.isLoaded },
      'submitResult() called'
    );

    if (!this.runtime.isLoaded) {
      pinoLog.error(
        { component: 'subscriber', eventId: event.event_id },
        'Cannot submit result: runtime not loaded!'
      );
      return;
    }

    // TAS-125: Check for checkpoint yield in metadata
    if (result.metadata?.checkpoint_yield === true) {
      await this.submitCheckpointYield(event, result);
      return;
    }

    const executionResult = this.buildExecutionResult(event, result, executionTimeMs);
    await this.sendCompletionViaFfi(event, executionResult, result.success);
  }

  /**
   * TAS-125: Submit a checkpoint yield via FFI.
   *
   * Called when a handler returns a checkpoint_yield result.
   * This persists the checkpoint and re-dispatches the step.
   */
  private async submitCheckpointYield(
    event: FfiStepEvent,
    result: StepHandlerResult
  ): Promise<void> {
    pinoLog.info(
      { component: 'subscriber', eventId: event.event_id },
      'submitCheckpointYield() called - handler yielded checkpoint'
    );

    // Extract checkpoint data from the result
    const resultData = result.result ?? {};
    const checkpointData: CheckpointYieldData = {
      step_uuid: event.step_uuid,
      cursor: resultData.cursor ?? 0,
      items_processed: (resultData.items_processed as number) ?? 0,
    };

    // Only set accumulated_results if it exists
    const accumulatedResults = resultData.accumulated_results as
      | Record<string, unknown>
      | undefined;
    if (accumulatedResults !== undefined) {
      checkpointData.accumulated_results = accumulatedResults;
    }

    try {
      const success = this.runtime.checkpointYieldStepEvent(event.event_id, checkpointData);

      if (success) {
        pinoLog.info(
          {
            component: 'subscriber',
            eventId: event.event_id,
            cursor: checkpointData.cursor,
            itemsProcessed: checkpointData.items_processed,
          },
          'Checkpoint yield submitted successfully - step will be re-dispatched'
        );

        this.emitter.emit(StepEventNames.STEP_CHECKPOINT_YIELD_SENT, {
          eventId: event.event_id,
          stepUuid: event.step_uuid,
          cursor: checkpointData.cursor,
          itemsProcessed: checkpointData.items_processed,
          timestamp: new Date(),
        });

        logInfo('Checkpoint yield submitted', {
          component: 'subscriber',
          event_id: event.event_id,
          step_uuid: event.step_uuid,
          cursor: String(checkpointData.cursor),
          items_processed: String(checkpointData.items_processed),
        });
      } else {
        pinoLog.error(
          { component: 'subscriber', eventId: event.event_id },
          'Checkpoint yield rejected by Rust - event may not be in pending map'
        );
        logError('Checkpoint yield rejected', {
          component: 'subscriber',
          event_id: event.event_id,
          step_uuid: event.step_uuid,
        });
      }
    } catch (error) {
      pinoLog.error(
        {
          component: 'subscriber',
          eventId: event.event_id,
          error: error instanceof Error ? error.message : String(error),
        },
        'Checkpoint yield failed with error'
      );
      logError('Failed to submit checkpoint yield', {
        component: 'subscriber',
        event_id: event.event_id,
        error_message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Submit an error result via FFI (for handler resolution/execution failures).
   */
  private async submitErrorResult(
    event: FfiStepEvent,
    errorMessage: string,
    startTime: number
  ): Promise<void> {
    if (!this.runtime.isLoaded) {
      logError('Cannot submit error result: runtime not available', {
        component: 'subscriber',
        event_id: event.event_id,
      });
      return;
    }

    const executionTimeMs = Date.now() - startTime;
    const executionResult = this.buildErrorExecutionResult(event, errorMessage, executionTimeMs);

    const accepted = await this.sendCompletionViaFfi(event, executionResult, false);
    if (accepted) {
      this.errorCount++;
    }
  }

  /**
   * Build a StepExecutionResult from a handler result.
   *
   * IMPORTANT: metadata.retryable must be set for Rust's is_retryable() to work correctly.
   */
  private buildExecutionResult(
    event: FfiStepEvent,
    result: StepHandlerResult,
    executionTimeMs: number
  ): StepExecutionResult {
    const executionResult: StepExecutionResult = {
      step_uuid: event.step_uuid,
      success: result.success,
      result: result.result ?? {},
      metadata: {
        execution_time_ms: executionTimeMs,
        worker_id: this.workerId,
        handler_name: this.extractHandlerName(event) ?? 'unknown',
        attempt_number: event.workflow_step?.attempts ?? 1,
        retryable: result.retryable ?? false,
        ...result.metadata,
      },
      status: result.success ? 'completed' : 'failed',
    };

    // Only add error field when not successful
    if (!result.success) {
      executionResult.error = {
        message: result.errorMessage ?? 'Unknown error',
        error_type: result.errorType ?? 'handler_error',
        retryable: result.retryable,
        status_code: null,
        backtrace: null,
      };
    }

    return executionResult;
  }

  /**
   * Build an error StepExecutionResult for handler resolution/execution failures.
   *
   * IMPORTANT: metadata.retryable must be set for Rust's is_retryable() to work correctly.
   */
  private buildErrorExecutionResult(
    event: FfiStepEvent,
    errorMessage: string,
    executionTimeMs: number
  ): StepExecutionResult {
    return {
      step_uuid: event.step_uuid,
      success: false,
      result: {},
      metadata: {
        execution_time_ms: executionTimeMs,
        worker_id: this.workerId,
        retryable: true,
      },
      status: 'error',
      error: {
        message: errorMessage,
        error_type: 'handler_error',
        retryable: true,
        status_code: null,
        backtrace: null,
      },
    };
  }

  /**
   * Send a completion result to Rust via FFI and handle the response.
   *
   * @returns true if the completion was accepted by Rust, false otherwise
   */
  private async sendCompletionViaFfi(
    event: FfiStepEvent,
    executionResult: StepExecutionResult,
    isSuccess: boolean
  ): Promise<boolean> {
    pinoLog.info(
      {
        component: 'subscriber',
        eventId: event.event_id,
        stepUuid: event.step_uuid,
        resultJson: JSON.stringify(executionResult),
      },
      'About to call runtime.completeStepEvent()'
    );

    try {
      const ffiResult = this.runtime.completeStepEvent(event.event_id, executionResult);

      if (ffiResult) {
        this.handleFfiSuccess(event, executionResult, isSuccess);
        return true;
      }
      this.handleFfiRejection(event);
      return false;
    } catch (error) {
      this.handleFfiError(event, error);
      return false;
    }
  }

  /**
   * Handle successful FFI completion submission.
   */
  private handleFfiSuccess(
    event: FfiStepEvent,
    executionResult: StepExecutionResult,
    isSuccess: boolean
  ): void {
    pinoLog.info(
      { component: 'subscriber', eventId: event.event_id, success: isSuccess },
      'completeStepEvent() returned TRUE - completion accepted by Rust'
    );

    this.emitter.emit(StepEventNames.STEP_COMPLETION_SENT, {
      eventId: event.event_id,
      stepUuid: event.step_uuid,
      success: isSuccess,
      timestamp: new Date(),
    });

    logDebug('Step result submitted', {
      component: 'subscriber',
      event_id: event.event_id,
      step_uuid: event.step_uuid,
      success: String(isSuccess),
      execution_time_ms: String(executionResult.metadata.execution_time_ms),
    });
  }

  /**
   * Handle FFI completion rejection (event not in pending map).
   */
  private handleFfiRejection(event: FfiStepEvent): void {
    pinoLog.error(
      {
        component: 'subscriber',
        eventId: event.event_id,
        stepUuid: event.step_uuid,
      },
      'completeStepEvent() returned FALSE - completion REJECTED by Rust! Event may not be in pending map.'
    );
    logError('FFI completion rejected', {
      component: 'subscriber',
      event_id: event.event_id,
      step_uuid: event.step_uuid,
    });
  }

  /**
   * Handle FFI completion error.
   */
  private handleFfiError(event: FfiStepEvent, error: unknown): void {
    pinoLog.error(
      {
        component: 'subscriber',
        eventId: event.event_id,
        error: error instanceof Error ? error.message : String(error),
        stack: error instanceof Error ? error.stack : undefined,
      },
      'completeStepEvent() THREW AN ERROR!'
    );
    logError('Failed to submit step result', {
      component: 'subscriber',
      event_id: event.event_id,
      error_message: error instanceof Error ? error.message : String(error),
    });
  }
}

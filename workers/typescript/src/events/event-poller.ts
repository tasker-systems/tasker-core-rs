/**
 * Event poller for TypeScript workers.
 *
 * Provides a polling loop that retrieves step events from the Rust FFI layer
 * and dispatches them to registered handlers. Uses a 10ms polling interval
 * matching other language workers.
 */

import pino, { type Logger, type LoggerOptions } from 'pino';
import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import type { FfiDispatchMetrics, FfiStepEvent } from '../ffi/types.js';
import type { TaskerEventEmitter } from './event-emitter.js';
import { MetricsEventNames, PollerEventNames, StepEventNames } from './event-names.js';

// Create a pino logger for the event poller
const loggerOptions: LoggerOptions = {
  name: 'event-poller',
  level: process.env.RUST_LOG ?? 'info',
};

// Add pino-pretty transport in non-production environments
if (process.env.TASKER_ENV !== 'production') {
  loggerOptions.transport = {
    target: 'pino-pretty',
    options: { colorize: true },
  };
}

const log: Logger = pino(loggerOptions);

/**
 * Configuration for the event poller
 */
export interface EventPollerConfig {
  /** Polling interval in milliseconds (default: 10) */
  pollIntervalMs?: number;

  /** Number of polls between starvation checks (default: 100) */
  starvationCheckInterval?: number;

  /** Number of polls between cleanup operations (default: 1000) */
  cleanupInterval?: number;

  /** Number of polls between metrics emissions (default: 100) */
  metricsInterval?: number;

  /** Maximum events to process per poll cycle (default: 100) */
  maxEventsPerCycle?: number;
}

/**
 * Callback for step event handling
 */
export type StepEventCallback = (event: FfiStepEvent) => Promise<void>;

/**
 * Callback for error handling
 */
export type ErrorCallback = (error: Error) => void;

/**
 * Callback for metrics handling
 */
export type MetricsCallback = (metrics: FfiDispatchMetrics) => void;

/**
 * Event poller state
 */
export type PollerState = 'stopped' | 'running' | 'stopping';

/**
 * Event poller for retrieving and dispatching step events from the FFI layer.
 *
 * The poller runs a continuous loop that:
 * 1. Polls for step events at 10ms intervals
 * 2. Dispatches received events to registered handlers
 * 3. Periodically checks for starvation conditions
 * 4. Performs cleanup of timed-out events
 * 5. Emits metrics for monitoring
 */
export class EventPoller {
  private readonly runtime: TaskerRuntime;
  private readonly config: Required<EventPollerConfig>;
  private readonly emitter: TaskerEventEmitter;

  private state: PollerState = 'stopped';
  private pollCount = 0;
  private cycleCount = 0;
  private intervalId: ReturnType<typeof setInterval> | null = null;

  private stepEventCallback: StepEventCallback | null = null;
  private errorCallback: ErrorCallback | null = null;
  private metricsCallback: MetricsCallback | null = null;

  /**
   * Create a new EventPoller.
   *
   * @param runtime - The FFI runtime for polling events
   * @param emitter - The event emitter to dispatch events to (required, no fallback)
   * @param config - Optional configuration for polling behavior
   */
  constructor(runtime: TaskerRuntime, emitter: TaskerEventEmitter, config: EventPollerConfig = {}) {
    this.runtime = runtime;
    this.emitter = emitter;
    this.config = {
      pollIntervalMs: config.pollIntervalMs ?? 10,
      starvationCheckInterval: config.starvationCheckInterval ?? 100,
      cleanupInterval: config.cleanupInterval ?? 1000,
      metricsInterval: config.metricsInterval ?? 100,
      maxEventsPerCycle: config.maxEventsPerCycle ?? 100,
    };
  }

  /**
   * Get the current poller state
   */
  getState(): PollerState {
    return this.state;
  }

  /**
   * Check if the poller is running
   */
  isRunning(): boolean {
    return this.state === 'running';
  }

  /**
   * Get the total number of polls executed
   */
  getPollCount(): number {
    return this.pollCount;
  }

  /**
   * Get the total number of cycles completed
   */
  getCycleCount(): number {
    return this.cycleCount;
  }

  /**
   * Register a callback for step events
   */
  onStepEvent(callback: StepEventCallback): this {
    this.stepEventCallback = callback;
    return this;
  }

  /**
   * Register a callback for errors
   */
  onError(callback: ErrorCallback): this {
    this.errorCallback = callback;
    return this;
  }

  /**
   * Register a callback for metrics
   */
  onMetrics(callback: MetricsCallback): this {
    this.metricsCallback = callback;
    return this;
  }

  /**
   * Start the polling loop
   */
  start(): void {
    log.info(
      { component: 'event-poller', operation: 'start', currentState: this.state },
      'EventPoller start() called'
    );

    if (this.state === 'running') {
      log.debug({ component: 'event-poller' }, 'Already running, returning early');
      return; // Already running
    }

    log.debug(
      { component: 'event-poller', runtimeLoaded: this.runtime.isLoaded },
      'Checking runtime.isLoaded'
    );
    if (!this.runtime.isLoaded) {
      throw new Error('Runtime not loaded. Call runtime.load() first.');
    }

    this.state = 'running';
    this.pollCount = 0;
    this.cycleCount = 0;

    this.emitter.emit(PollerEventNames.POLLER_STARTED, {
      timestamp: new Date(),
      message: 'Event poller started',
    });

    log.info(
      { component: 'event-poller', intervalMs: this.config.pollIntervalMs },
      'Setting up setInterval for polling'
    );

    // Start the polling loop
    this.intervalId = setInterval(() => {
      this.poll();
    }, this.config.pollIntervalMs);

    log.info(
      { component: 'event-poller', intervalId: String(this.intervalId) },
      'setInterval created, polling active'
    );
  }

  /**
   * Stop the polling loop
   */
  async stop(): Promise<void> {
    if (this.state === 'stopped') {
      return; // Already stopped
    }

    this.state = 'stopping';

    // Clear the interval
    if (this.intervalId !== null) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }

    this.state = 'stopped';

    this.emitter.emit(PollerEventNames.POLLER_STOPPED, {
      timestamp: new Date(),
      message: `Event poller stopped after ${this.pollCount} polls`,
    });
  }

  /**
   * Execute a single poll cycle
   */
  private poll(): void {
    // Log first poll at info level to confirm interval is working
    if (this.pollCount === 0) {
      log.info(
        { component: 'event-poller', state: this.state },
        'First poll() call - setInterval is working'
      );
    }

    // Log every 100th poll to avoid spam
    if (this.pollCount % 100 === 0) {
      log.debug(
        { component: 'event-poller', pollCount: this.pollCount, state: this.state },
        'poll() cycle'
      );
    }

    if (this.state !== 'running') {
      return;
    }

    this.pollCount++;

    try {
      // Poll for events (non-blocking)
      let eventsProcessed = 0;

      for (let i = 0; i < this.config.maxEventsPerCycle; i++) {
        const event = this.runtime.pollStepEvents();
        if (event === null) {
          break; // No more events
        }

        eventsProcessed++;
        const handlerCallable = event.step_definition.handler.callable;
        log.info(
          {
            component: 'event-poller',
            operation: 'event_received',
            stepUuid: event.step_uuid,
            handlerCallable,
            eventIndex: i,
          },
          `Received step event for handler: ${handlerCallable}`
        );
        this.handleStepEvent(event);
      }

      // Periodic starvation check
      if (this.pollCount % this.config.starvationCheckInterval === 0) {
        this.checkStarvation();
      }

      // Periodic cleanup
      if (this.pollCount % this.config.cleanupInterval === 0) {
        this.runtime.cleanupTimeouts();
      }

      // Periodic metrics
      if (this.pollCount % this.config.metricsInterval === 0) {
        this.emitMetrics();
      }

      // Emit cycle complete
      this.cycleCount++;
      if (eventsProcessed > 0) {
        this.emitter.emit(PollerEventNames.POLLER_CYCLE_COMPLETE, {
          eventsProcessed,
          cycleNumber: this.cycleCount,
          timestamp: new Date(),
        });
      }
    } catch (error) {
      this.handleError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  /**
   * Handle a step event
   */
  private handleStepEvent(event: FfiStepEvent): void {
    const handlerCallable = event.step_definition.handler.callable;
    log.debug(
      {
        component: 'event-poller',
        operation: 'handle_step_event',
        stepUuid: event.step_uuid,
        handlerCallable,
        hasCallback: !!this.stepEventCallback,
      },
      'Handling step event'
    );

    // Log emitter instance for debugging
    const listenerCountBefore = this.emitter.listenerCount(StepEventNames.STEP_EXECUTION_RECEIVED);
    log.info(
      {
        component: 'event-poller',
        emitterInstanceId: this.emitter.getInstanceId(),
        stepUuid: event.step_uuid,
        listenerCount: listenerCountBefore,
        eventName: StepEventNames.STEP_EXECUTION_RECEIVED,
      },
      `About to emit ${StepEventNames.STEP_EXECUTION_RECEIVED} event`
    );

    // Emit the event through the event emitter with error handling
    try {
      const emitResult = this.emitter.emit(StepEventNames.STEP_EXECUTION_RECEIVED, {
        event,
        receivedAt: new Date(),
      });
      log.info(
        {
          component: 'event-poller',
          stepUuid: event.step_uuid,
          emitResult,
          listenerCountAfter: this.emitter.listenerCount(StepEventNames.STEP_EXECUTION_RECEIVED),
          eventName: StepEventNames.STEP_EXECUTION_RECEIVED,
        },
        `Emit returned: ${emitResult} (true means listeners were called)`
      );
    } catch (emitError) {
      log.error(
        {
          component: 'event-poller',
          stepUuid: event.step_uuid,
          error: emitError instanceof Error ? emitError.message : String(emitError),
          stack: emitError instanceof Error ? emitError.stack : undefined,
        },
        'Error during emit'
      );
    }

    // Call the registered callback if present
    if (this.stepEventCallback) {
      log.debug(
        { component: 'event-poller', stepUuid: event.step_uuid },
        'Invoking step event callback'
      );
      this.stepEventCallback(event).catch((error) => {
        this.handleError(error instanceof Error ? error : new Error(String(error)));
      });
    } else {
      log.warn(
        { component: 'event-poller', stepUuid: event.step_uuid },
        'No step event callback registered!'
      );
    }
  }

  /**
   * Check for starvation conditions
   */
  private checkStarvation(): void {
    try {
      this.runtime.checkStarvationWarnings();

      const metrics = this.runtime.getFfiDispatchMetrics();
      if (metrics.starvation_detected) {
        this.emitter.emitStarvationDetected(metrics);
      }
    } catch (error) {
      // Log but don't fail the poll loop
      this.handleError(error instanceof Error ? error : new Error(String(error)));
    }
  }

  /**
   * Emit current metrics
   */
  private emitMetrics(): void {
    try {
      const metrics = this.runtime.getFfiDispatchMetrics();
      this.emitter.emitMetricsUpdated(metrics);

      if (this.metricsCallback) {
        this.metricsCallback(metrics);
      }
    } catch (error) {
      this.emitter.emit(MetricsEventNames.METRICS_ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        timestamp: new Date(),
      });
    }
  }

  /**
   * Handle an error
   */
  private handleError(error: Error): void {
    this.emitter.emit(PollerEventNames.POLLER_ERROR, {
      error,
      timestamp: new Date(),
    });

    if (this.errorCallback) {
      this.errorCallback(error);
    }
  }
}

/**
 * Create an event poller with the given runtime, emitter, and configuration
 */
export function createEventPoller(
  runtime: TaskerRuntime,
  emitter: TaskerEventEmitter,
  config?: EventPollerConfig
): EventPoller {
  return new EventPoller(runtime, emitter, config);
}

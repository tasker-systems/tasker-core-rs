/**
 * EventSystem - Unified event processing system for TypeScript workers.
 *
 * This class owns and manages the complete event flow:
 * - TaskerEventEmitter: Event bus for step events
 * - EventPoller: Polls FFI for step events, emits to emitter
 * - StepExecutionSubscriber: Subscribes to emitter, dispatches to handlers
 *
 * By owning all three components, EventSystem guarantees they share the
 * same emitter instance, eliminating reference sharing bugs.
 *
 * Design principles:
 * - Explicit construction: All dependencies injected via constructor
 * - Clear ownership: This class owns the emitter lifecycle
 * - Explicit lifecycle: start() and stop() methods with defined phases
 */

import pino, { type Logger, type LoggerOptions } from 'pino';
import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import {
  type HandlerRegistryInterface,
  StepExecutionSubscriber,
} from '../subscriber/step-execution-subscriber.js';
import type { StepExecutionReceivedPayload } from './event-emitter.js';
import { TaskerEventEmitter } from './event-emitter.js';
import { StepEventNames } from './event-names.js';
import { EventPoller } from './event-poller.js';

// Re-export for backwards compatibility
export type { HandlerRegistryInterface };

// Create a pino logger for the event system (for debugging)
const loggerOptions: LoggerOptions = {
  name: 'event-system',
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
 * Configuration for EventPoller within EventSystem.
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
 * Configuration for StepExecutionSubscriber within EventSystem.
 */
export interface SubscriberConfig {
  /** Unique identifier for this worker (default: typescript-worker-{pid}) */
  workerId?: string;

  /** Maximum number of concurrent handler executions (default: 10) */
  maxConcurrent?: number;

  /** Timeout for individual handler execution in milliseconds (default: 300000) */
  handlerTimeoutMs?: number;
}

/**
 * Complete configuration for EventSystem.
 */
export interface EventSystemConfig {
  /** Configuration for the event poller */
  poller?: EventPollerConfig;

  /** Configuration for the step execution subscriber */
  subscriber?: SubscriberConfig;
}

/**
 * Statistics about the event system's operation.
 */
export interface EventSystemStats {
  /** Whether the system is currently running */
  running: boolean;

  /** Total events processed by the subscriber */
  processedCount: number;

  /** Total errors encountered during processing */
  errorCount: number;

  /** Number of currently active handler executions */
  activeHandlers: number;

  /** Total poll cycles executed */
  pollCount: number;
}

/**
 * Unified event processing system.
 *
 * Owns the complete event flow: emitter → poller → subscriber.
 * Guarantees all components share the same emitter instance.
 *
 * @example
 * ```typescript
 * const eventSystem = new EventSystem(runtime, registry, {
 *   poller: { pollIntervalMs: 10 },
 *   subscriber: { workerId: 'worker-1', maxConcurrent: 10 },
 * });
 *
 * eventSystem.start();
 * // ... processing events ...
 * await eventSystem.stop();
 * ```
 */
export class EventSystem {
  private readonly emitter: TaskerEventEmitter;
  private readonly poller: EventPoller;
  private readonly subscriber: StepExecutionSubscriber;
  private running: boolean = false;

  /**
   * Create a new EventSystem.
   *
   * @param runtime - The FFI runtime for polling events and submitting results
   * @param registry - The handler registry for resolving step handlers
   * @param config - Optional configuration for poller and subscriber
   */
  constructor(
    runtime: TaskerRuntime,
    registry: HandlerRegistryInterface,
    config: EventSystemConfig = {}
  ) {
    // Create a single emitter instance owned by this class
    this.emitter = new TaskerEventEmitter();

    // Create poller with explicit emitter (no global fallback)
    this.poller = new EventPoller(runtime, this.emitter, config.poller);

    // Create subscriber with explicit emitter and runtime
    this.subscriber = new StepExecutionSubscriber(
      this.emitter,
      registry,
      runtime,
      config.subscriber
    );
  }

  /**
   * Start the event system.
   *
   * Starts the subscriber first (to register listeners), then the poller.
   * This ensures no events are missed.
   */
  start(): void {
    log.info(
      { component: 'event-system', emitterInstanceId: this.emitter.getInstanceId() },
      'EventSystem.start() called'
    );

    if (this.running) {
      log.warn({ component: 'event-system' }, 'EventSystem already running');
      return;
    }

    // Add a debug listener BEFORE subscriber to trace event flow
    log.info(
      { component: 'event-system', eventName: StepEventNames.STEP_EXECUTION_RECEIVED },
      `Adding debug listener BEFORE subscriber for ${StepEventNames.STEP_EXECUTION_RECEIVED}...`
    );
    this.emitter.on(
      StepEventNames.STEP_EXECUTION_RECEIVED,
      (payload: StepExecutionReceivedPayload) => {
        log.info(
          {
            component: 'event-system',
            debugListener: true,
            eventId: payload.event?.event_id,
            stepUuid: payload.event?.step_uuid,
            eventName: StepEventNames.STEP_EXECUTION_RECEIVED,
          },
          `🔍 DEBUG LISTENER: Received ${StepEventNames.STEP_EXECUTION_RECEIVED} event!`
        );
      }
    );
    log.info(
      {
        component: 'event-system',
        listenerCountAfterDebug: this.emitter.listenerCount(StepEventNames.STEP_EXECUTION_RECEIVED),
        eventName: StepEventNames.STEP_EXECUTION_RECEIVED,
      },
      'Debug listener added'
    );

    // Start subscriber first to register listeners
    log.info({ component: 'event-system' }, 'Starting subscriber first...');
    this.subscriber.start();

    log.info(
      {
        component: 'event-system',
        listenerCountAfterSubscriber: this.emitter.listenerCount(
          StepEventNames.STEP_EXECUTION_RECEIVED
        ),
        eventName: StepEventNames.STEP_EXECUTION_RECEIVED,
      },
      'Subscriber started, checking listener count'
    );

    // Then start poller to begin receiving events
    log.info({ component: 'event-system' }, 'Starting poller...');
    this.poller.start();

    this.running = true;
    log.info(
      {
        component: 'event-system',
        emitterInstanceId: this.emitter.getInstanceId(),
        listenerCount: this.emitter.listenerCount(StepEventNames.STEP_EXECUTION_RECEIVED),
        eventName: StepEventNames.STEP_EXECUTION_RECEIVED,
      },
      'EventSystem started successfully'
    );
  }

  /**
   * Stop the event system gracefully.
   *
   * Stops ingress first (poller), waits for in-flight handlers,
   * then stops the subscriber.
   *
   * @param drainTimeoutMs - Maximum time to wait for in-flight handlers (default: 30000)
   */
  async stop(drainTimeoutMs: number = 30000): Promise<void> {
    if (!this.running) {
      return;
    }

    // Stop ingress first
    await this.poller.stop();

    // Wait for in-flight handlers to complete
    await this.subscriber.waitForCompletion(drainTimeoutMs);

    // Stop subscriber
    this.subscriber.stop();

    this.running = false;
  }

  /**
   * Check if the event system is running.
   */
  isRunning(): boolean {
    return this.running;
  }

  /**
   * Get the event emitter (for testing or advanced use cases).
   */
  getEmitter(): TaskerEventEmitter {
    return this.emitter;
  }

  /**
   * Get current statistics about the event system.
   */
  getStats(): EventSystemStats {
    return {
      running: this.running,
      processedCount: this.subscriber.getProcessedCount(),
      errorCount: this.subscriber.getErrorCount(),
      activeHandlers: this.subscriber.getActiveHandlers(),
      pollCount: this.poller.getPollCount(),
    };
  }
}

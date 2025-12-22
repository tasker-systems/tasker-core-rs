/**
 * Event poller for TypeScript workers.
 *
 * Provides a polling loop that retrieves step events from the Rust FFI layer
 * and dispatches them to registered handlers. Uses a 10ms polling interval
 * matching other language workers.
 */

import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import type { FfiDispatchMetrics, FfiStepEvent } from '../ffi/types.js';
import { type TaskerEventEmitter, getGlobalEmitter } from './event-emitter.js';

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

  /** Custom event emitter (uses global emitter if not provided) */
  eventEmitter?: TaskerEventEmitter;
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

  constructor(runtime: TaskerRuntime, config: EventPollerConfig = {}) {
    this.runtime = runtime;
    this.config = {
      pollIntervalMs: config.pollIntervalMs ?? 10,
      starvationCheckInterval: config.starvationCheckInterval ?? 100,
      cleanupInterval: config.cleanupInterval ?? 1000,
      metricsInterval: config.metricsInterval ?? 100,
      maxEventsPerCycle: config.maxEventsPerCycle ?? 100,
      eventEmitter: config.eventEmitter ?? getGlobalEmitter(),
    };
    this.emitter = this.config.eventEmitter;
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
    if (this.state === 'running') {
      return; // Already running
    }

    if (!this.runtime.isLoaded) {
      throw new Error('Runtime not loaded. Call runtime.load() first.');
    }

    this.state = 'running';
    this.pollCount = 0;
    this.cycleCount = 0;

    this.emitter.emit('poller.started', {
      timestamp: new Date(),
      message: 'Event poller started',
    });

    // Start the polling loop
    this.intervalId = setInterval(() => {
      this.poll();
    }, this.config.pollIntervalMs);
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

    this.emitter.emit('poller.stopped', {
      timestamp: new Date(),
      message: `Event poller stopped after ${this.pollCount} polls`,
    });
  }

  /**
   * Execute a single poll cycle
   */
  private poll(): void {
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
        this.emitter.emit('poller.cycle.complete', {
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
    // Emit the event through the event emitter
    this.emitter.emitStepReceived(event);

    // Call the registered callback if present
    if (this.stepEventCallback) {
      this.stepEventCallback(event).catch((error) => {
        this.handleError(error instanceof Error ? error : new Error(String(error)));
      });
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
      this.emitter.emit('metrics.error', {
        error: error instanceof Error ? error : new Error(String(error)),
        timestamp: new Date(),
      });
    }
  }

  /**
   * Handle an error
   */
  private handleError(error: Error): void {
    this.emitter.emit('poller.error', {
      error,
      timestamp: new Date(),
    });

    if (this.errorCallback) {
      this.errorCallback(error);
    }
  }
}

/**
 * Create an event poller with the given runtime and configuration
 */
export function createEventPoller(runtime: TaskerRuntime, config?: EventPollerConfig): EventPoller {
  return new EventPoller(runtime, config);
}

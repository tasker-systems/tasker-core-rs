/**
 * Standard event names for the TypeScript worker.
 *
 * These constants provide type-safe event names that match
 * the event system used by other language workers.
 */

/**
 * Event names for step execution lifecycle
 */
export const StepEventNames = {
  /** Emitted when a step execution event is received from the FFI layer */
  STEP_EXECUTION_RECEIVED: 'step.execution.received',

  /** Emitted when a step handler starts executing */
  STEP_EXECUTION_STARTED: 'step.execution.started',

  /** Emitted when a step handler completes successfully */
  STEP_EXECUTION_COMPLETED: 'step.execution.completed',

  /** Emitted when a step handler fails */
  STEP_EXECUTION_FAILED: 'step.execution.failed',

  /** Emitted when a step completion is sent back to Rust */
  STEP_COMPLETION_SENT: 'step.completion.sent',

  /** Emitted when a step handler times out */
  STEP_EXECUTION_TIMEOUT: 'step.execution.timeout',

  /** TAS-125: Emitted when a checkpoint yield is sent back to Rust */
  STEP_CHECKPOINT_YIELD_SENT: 'step.checkpoint_yield.sent',
} as const;

/**
 * Event names for worker lifecycle
 */
export const WorkerEventNames = {
  /** Emitted when the worker starts up */
  WORKER_STARTED: 'worker.started',

  /** Emitted when the worker is ready to process events */
  WORKER_READY: 'worker.ready',

  /** Emitted when graceful shutdown begins */
  WORKER_SHUTDOWN_STARTED: 'worker.shutdown.started',

  /** Emitted when the worker has fully stopped */
  WORKER_STOPPED: 'worker.stopped',

  /** Emitted when the worker encounters an error */
  WORKER_ERROR: 'worker.error',
} as const;

/**
 * Event names for polling lifecycle
 */
export const PollerEventNames = {
  /** Emitted when the poller starts */
  POLLER_STARTED: 'poller.started',

  /** Emitted when the poller stops */
  POLLER_STOPPED: 'poller.stopped',

  /** Emitted when a poll cycle completes */
  POLLER_CYCLE_COMPLETE: 'poller.cycle.complete',

  /** Emitted when starvation is detected */
  POLLER_STARVATION_DETECTED: 'poller.starvation.detected',

  /** Emitted when the poller encounters an error */
  POLLER_ERROR: 'poller.error',
} as const;

/**
 * Event names for metrics
 */
export const MetricsEventNames = {
  /** Emitted periodically with FFI dispatch metrics */
  METRICS_UPDATED: 'metrics.updated',

  /** Emitted when metrics collection fails */
  METRICS_ERROR: 'metrics.error',
} as const;

/**
 * All event names combined
 */
export const EventNames = {
  ...StepEventNames,
  ...WorkerEventNames,
  ...PollerEventNames,
  ...MetricsEventNames,
} as const;

/**
 * Type representing all possible event names
 */
export type EventName = (typeof EventNames)[keyof typeof EventNames];

/**
 * Type representing step event names
 */
export type StepEventName = (typeof StepEventNames)[keyof typeof StepEventNames];

/**
 * Type representing worker event names
 */
export type WorkerEventName = (typeof WorkerEventNames)[keyof typeof WorkerEventNames];

/**
 * Type representing poller event names
 */
export type PollerEventName = (typeof PollerEventNames)[keyof typeof PollerEventNames];

/**
 * Type representing metrics event names
 */
export type MetricsEventName = (typeof MetricsEventNames)[keyof typeof MetricsEventNames];

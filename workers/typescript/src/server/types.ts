/**
 * Server module types.
 *
 * Defines types for WorkerServer lifecycle and configuration.
 */

import type { TaskerEventEmitter } from '../events/event-emitter.js';
import type { EventPoller } from '../events/event-poller.js';
import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import type { HandlerRegistry } from '../handler/registry.js';
import type { StepExecutionSubscriber } from '../subscriber/step-execution-subscriber.js';

/**
 * Server lifecycle states.
 *
 * State transitions:
 * - initialized -> starting -> running -> shutting_down -> stopped
 * - Any state can transition to 'error' on fatal failures
 */
export type ServerState =
  | 'initialized'
  | 'starting'
  | 'running'
  | 'shutting_down'
  | 'stopped'
  | 'error';

/**
 * Worker server configuration.
 *
 * All fields are optional with sensible defaults.
 */
export interface WorkerServerConfig {
  /** Task namespace to handle (default: "default") */
  namespace?: string;

  /** Log level for Rust tracing (default: "info") */
  logLevel?: 'trace' | 'debug' | 'info' | 'warn' | 'error';

  /** Path to TOML configuration file */
  configPath?: string;

  /** PostgreSQL database connection URL */
  databaseUrl?: string;

  /** Path to the FFI library (overrides auto-discovery) */
  libraryPath?: string;

  /** Maximum concurrent handler executions (default: 10) */
  maxConcurrentHandlers?: number;

  /** Handler execution timeout in milliseconds (default: 300000 = 5 minutes) */
  handlerTimeoutMs?: number;

  /** Event polling interval in milliseconds (default: 10) */
  pollIntervalMs?: number;

  /** Starvation check interval in poll cycles (default: 100) */
  starvationCheckInterval?: number;

  /** Cleanup interval in poll cycles (default: 1000) */
  cleanupInterval?: number;

  /** Metrics reporting interval in poll cycles (default: 100) */
  metricsInterval?: number;
}

/**
 * Health check result.
 */
export interface HealthCheckResult {
  /** Whether the worker is healthy */
  healthy: boolean;

  /** Optional status details when healthy */
  status?: ServerStatus;

  /** Error message when unhealthy */
  error?: string;
}

/**
 * Server status information.
 */
export interface ServerStatus {
  /** Current server state */
  state: ServerState;

  /** Worker identifier */
  workerId: string | null;

  /** Whether the worker is actively running */
  running: boolean;

  /** Number of events processed */
  processedCount: number;

  /** Number of errors encountered */
  errorCount: number;

  /** Number of currently active handlers */
  activeHandlers: number;

  /** Server uptime in milliseconds */
  uptimeMs: number;
}

/**
 * Internal server components.
 *
 * Used by WorkerServer to manage lifecycle of all components.
 */
export interface ServerComponents {
  /** FFI runtime instance */
  runtime: TaskerRuntime;

  /** Event emitter for step events */
  emitter: TaskerEventEmitter;

  /** Handler registry */
  registry: HandlerRegistry;

  /** Event poller for FFI events */
  eventPoller: EventPoller;

  /** Step execution subscriber */
  stepSubscriber: StepExecutionSubscriber;

  /** Worker identifier from bootstrap */
  workerId: string;
}

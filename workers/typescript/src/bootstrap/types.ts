/**
 * Bootstrap configuration and result types.
 *
 * These types extend the FFI types with TypeScript-friendly interfaces
 * for worker lifecycle management.
 */

import type {
  BootstrapConfig as FfiBootstrapConfig,
  BootstrapResult as FfiBootstrapResult,
  WorkerStatus as FfiWorkerStatus,
  StopResult as FfiStopResult,
} from '../ffi/types.js';

// Re-export FFI types for convenience
export type { FfiBootstrapConfig, FfiBootstrapResult, FfiWorkerStatus, FfiStopResult };

/**
 * Configuration for worker bootstrap.
 *
 * Matches Python's BootstrapConfig and Ruby's bootstrap options.
 */
export interface BootstrapConfig {
  /** Optional worker ID. Auto-generated if not provided. */
  workerId?: string;

  /** Task namespace this worker handles (default: "default"). */
  namespace?: string;

  /** Path to custom configuration file (TOML). */
  configPath?: string;

  /** Log level: trace, debug, info, warn, error (default: "info"). */
  logLevel?: 'trace' | 'debug' | 'info' | 'warn' | 'error';

  /** Database URL override. */
  databaseUrl?: string;
}

/**
 * Result from worker bootstrap.
 *
 * Contains information about the bootstrapped worker instance.
 */
export interface BootstrapResult {
  /** Whether bootstrap was successful. */
  success: boolean;

  /** Current status (started, already_running, error). */
  status: 'started' | 'already_running' | 'error';

  /** Human-readable status message. */
  message: string;

  /** Unique identifier for this worker instance. */
  workerId?: string;

  /** Error message if bootstrap failed. */
  error?: string;
}

/**
 * Current worker status.
 *
 * Contains detailed information about the worker's state and resources.
 */
export interface WorkerStatus {
  /** Whether the status query succeeded. */
  success: boolean;

  /** Whether the worker is currently running. */
  running: boolean;

  /** Current status string. */
  status?: string;

  /** Worker ID if running. */
  workerId?: string;

  /** Current environment (test, development, production). */
  environment?: string;

  /** Internal worker core status. */
  workerCoreStatus?: string;

  /** Whether the web API is enabled. */
  webApiEnabled?: boolean;

  /** List of task namespaces this worker handles. */
  supportedNamespaces?: string[];

  /** Total database connection pool size. */
  databasePoolSize?: number;

  /** Number of idle database connections. */
  databasePoolIdle?: number;
}

/**
 * Result from stopping the worker.
 */
export interface StopResult {
  /** Whether the stop was successful. */
  success: boolean;

  /** Current status (stopped, not_running, error). */
  status: 'stopped' | 'not_running' | 'error';

  /** Human-readable status message. */
  message: string;

  /** Worker ID that was stopped. */
  workerId?: string;

  /** Error message if stop failed. */
  error?: string;
}

/**
 * Convert TypeScript BootstrapConfig to FFI format.
 */
export function toFfiBootstrapConfig(config?: BootstrapConfig): FfiBootstrapConfig {
  if (!config) {
    return {};
  }

  return {
    worker_id: config.workerId,
    log_level: config.logLevel,
    database_url: config.databaseUrl,
    // Additional config can be passed through
    namespace: config.namespace,
    config_path: config.configPath,
  };
}

/**
 * Convert FFI BootstrapResult to TypeScript format.
 */
export function fromFfiBootstrapResult(result: FfiBootstrapResult): BootstrapResult {
  return {
    success: result.success,
    status: result.status,
    message: result.message,
    workerId: result.worker_id,
    error: result.error,
  };
}

/**
 * Convert FFI WorkerStatus to TypeScript format.
 */
export function fromFfiWorkerStatus(status: FfiWorkerStatus): WorkerStatus {
  return {
    success: status.success,
    running: status.running,
    status: status.status,
    workerId: status.worker_id,
    environment: status.environment,
    workerCoreStatus: status.worker_core_status,
    webApiEnabled: status.web_api_enabled,
    supportedNamespaces: status.supported_namespaces,
    databasePoolSize: status.database_pool_size,
    databasePoolIdle: status.database_pool_idle,
  };
}

/**
 * Convert FFI StopResult to TypeScript format.
 */
export function fromFfiStopResult(result: FfiStopResult): StopResult {
  return {
    success: result.success,
    status: result.status,
    message: result.message,
    workerId: result.worker_id,
    error: result.error,
  };
}

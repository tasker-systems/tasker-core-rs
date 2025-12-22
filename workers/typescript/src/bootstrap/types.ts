/**
 * Bootstrap configuration and result types.
 *
 * These types extend the FFI types with TypeScript-friendly interfaces
 * for worker lifecycle management.
 */

import type {
  BootstrapConfig as FfiBootstrapConfig,
  BootstrapResult as FfiBootstrapResult,
  StopResult as FfiStopResult,
  WorkerStatus as FfiWorkerStatus,
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

  const result: FfiBootstrapConfig = {};

  if (config.workerId !== undefined) {
    result.worker_id = config.workerId;
  }
  if (config.logLevel !== undefined) {
    result.log_level = config.logLevel;
  }
  if (config.databaseUrl !== undefined) {
    result.database_url = config.databaseUrl;
  }
  if (config.namespace !== undefined) {
    result.namespace = config.namespace;
  }
  if (config.configPath !== undefined) {
    result.config_path = config.configPath;
  }

  return result;
}

/**
 * Convert FFI BootstrapResult to TypeScript format.
 */
export function fromFfiBootstrapResult(result: FfiBootstrapResult): BootstrapResult {
  const converted: BootstrapResult = {
    success: result.success,
    status: result.status,
    message: result.message,
  };

  if (result.worker_id !== undefined) {
    converted.workerId = result.worker_id;
  }
  if (result.error !== undefined) {
    converted.error = result.error;
  }

  return converted;
}

/**
 * Convert FFI WorkerStatus to TypeScript format.
 */
export function fromFfiWorkerStatus(status: FfiWorkerStatus): WorkerStatus {
  const converted: WorkerStatus = {
    success: status.success,
    running: status.running,
  };

  if (status.status !== undefined) {
    converted.status = status.status;
  }
  if (status.worker_id !== undefined) {
    converted.workerId = status.worker_id;
  }
  if (status.environment !== undefined) {
    converted.environment = status.environment;
  }
  if (status.worker_core_status !== undefined) {
    converted.workerCoreStatus = status.worker_core_status;
  }
  if (status.web_api_enabled !== undefined) {
    converted.webApiEnabled = status.web_api_enabled;
  }
  if (status.supported_namespaces !== undefined) {
    converted.supportedNamespaces = status.supported_namespaces;
  }
  if (status.database_pool_size !== undefined) {
    converted.databasePoolSize = status.database_pool_size;
  }
  if (status.database_pool_idle !== undefined) {
    converted.databasePoolIdle = status.database_pool_idle;
  }

  return converted;
}

/**
 * Convert FFI StopResult to TypeScript format.
 */
export function fromFfiStopResult(result: FfiStopResult): StopResult {
  const converted: StopResult = {
    success: result.success,
    status: result.status,
    message: result.message,
  };

  if (result.worker_id !== undefined) {
    converted.workerId = result.worker_id;
  }
  if (result.error !== undefined) {
    converted.error = result.error;
  }

  return converted;
}

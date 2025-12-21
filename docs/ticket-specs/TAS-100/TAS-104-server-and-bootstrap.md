# TAS-104: Server and Bootstrap

**Priority**: Medium  
**Estimated Effort**: 1-2 days  
**Dependencies**: TAS-101 (FFI Bridge), TAS-102 (Handler API), TAS-103 (Specialized Handlers)  
**Parent**: [TAS-100](./README.md)  
**Linear**: [TAS-104](https://linear.app/tasker-systems/issue/TAS-104)  
**Branch**: `jcoletaylor/tas-104-typescript-server-bootstrap`  
**Status**: Detailed Specification

---

## Objective

Create the complete TypeScript worker server including bootstrap process, lifecycle management, signal handling, and graceful shutdown. This brings together all previous components (FFI bridge, handlers, event system) into a production-ready server.

**Key Goal**: Provide a TypeScript worker that matches the Python/Ruby production patterns for bootstrapping, health checks, and graceful shutdown.

---

## Architecture Overview

### Bootstrap Sequence

```
1. Show Banner & Environment Info
2. Discover & Register Handlers
3. Create Bootstrap Configuration
4. Bootstrap Rust Worker via FFI
   ├── Initialize Tokio runtime
   ├── Connect to PostgreSQL
   ├── Setup PGMQ channels
   └── Start internal services
5. Start Python/TypeScript Event System
   ├── EventBridge (pub/sub)
   ├── StepExecutionSubscriber (dispatch to handlers)
   └── EventPoller (poll FFI → publish events)
6. Register Signal Handlers (SIGTERM, SIGINT, SIGUSR1)
7. Main Loop with Periodic Health Checks
8. Graceful Shutdown on Signal
```

### Shutdown Sequence

```
1. EventPoller.stop()         ← Stop new events from arriving
2. StepExecutionSubscriber.stop() ← Stop handler dispatch
3. EventBridge.stop()          ← Clear all subscriptions
4. transition_to_graceful_shutdown() ← Tell Rust to finish in-flight
5. stop_worker()               ← Stop Rust foundation
```

---

## Reference Implementations

### Python (Most Complete)
- `workers/python/bin/server.py` - Complete server lifecycle (lines 1-356)
- `workers/python/python/tasker_core/bootstrap.py` - Bootstrap API (lines 1-168)
- `workers/python/python/tasker_core/event_poller.py` - FFI polling
- `workers/python/python/tasker_core/step_execution_subscriber.py` - Handler dispatch

### Ruby
- `workers/ruby/bin/server.rb` - Server entry point
- `workers/ruby/lib/tasker_core/bootstrap.rb` - Bootstrap orchestration

---

## Detailed Component Specifications

### 1. Structured Logging API

**File**: `src/logging/index.ts`

**Purpose**: Unified structured logging that integrates with Rust tracing infrastructure via FFI.

**Python reference**: `workers/python/python/tasker_core/logging.py`  
**Ruby reference**: `workers/ruby/lib/tasker_core/tracing.rb`

```typescript
import { getTaskerRuntime } from '../ffi/runtime-factory';

/**
 * Structured logging fields.
 * 
 * All fields are optional. Common fields include:
 * - component: Component/subsystem identifier
 * - operation: Operation being performed
 * - correlation_id: Distributed tracing correlation ID
 * - task_uuid: Task identifier
 * - step_uuid: Step identifier
 * - namespace: Task namespace
 * - error_message: Error message for error logs
 */
export interface LogFields {
  [key: string]: string;
}

/**
 * Log an ERROR level message with structured fields.
 * 
 * Use this for unrecoverable failures that require intervention.
 * 
 * @param message - The log message
 * @param fields - Optional structured fields for context
 * 
 * @example
 * logError('Database connection failed', {
 *   component: 'database',
 *   operation: 'connect',
 *   error_message: 'Connection timeout',
 * });
 */
export function logError(message: string, fields?: LogFields): void {
  const runtime = getTaskerRuntime();
  runtime.logError(message, fields || {});
}

/**
 * Log a WARN level message with structured fields.
 * 
 * Use this for degraded operation or retryable failures.
 * 
 * @param message - The log message
 * @param fields - Optional structured fields for context
 * 
 * @example
 * logWarn('Retry attempt 3 of 5', {
 *   component: 'handler',
 *   operation: 'retry',
 *   attempt: '3',
 * });
 */
export function logWarn(message: string, fields?: LogFields): void {
  const runtime = getTaskerRuntime();
  runtime.logWarn(message, fields || {});
}

/**
 * Log an INFO level message with structured fields.
 * 
 * Use this for lifecycle events and state transitions.
 * 
 * @param message - The log message
 * @param fields - Optional structured fields for context
 * 
 * @example
 * logInfo('Task processing started', {
 *   component: 'handler',
 *   operation: 'process_payment',
 *   correlation_id: 'abc-123',
 *   task_uuid: 'task-456',
 * });
 */
export function logInfo(message: string, fields?: LogFields): void {
  const runtime = getTaskerRuntime();
  runtime.logInfo(message, fields || {});
}

/**
 * Log a DEBUG level message with structured fields.
 * 
 * Use this for detailed diagnostic information during development.
 * 
 * @param message - The log message
 * @param fields - Optional structured fields for context
 * 
 * @example
 * logDebug('Parsed request payload', {
 *   component: 'handler',
 *   payload_size: '1024',
 *   content_type: 'application/json',
 * });
 */
export function logDebug(message: string, fields?: LogFields): void {
  const runtime = getTaskerRuntime();
  runtime.logDebug(message, fields || {});
}

/**
 * Log a TRACE level message with structured fields.
 * 
 * Use this for very verbose logging, like function entry/exit.
 * This level is typically disabled in production.
 * 
 * @param message - The log message
 * @param fields - Optional structured fields for context
 * 
 * @example
 * logTrace('Entering process_step', {
 *   component: 'handler',
 *   step_uuid: 'step-789',
 * });
 */
export function logTrace(message: string, fields?: LogFields): void {
  const runtime = getTaskerRuntime();
  runtime.logTrace(message, fields || {});
}
```

**Structured Field Conventions** (matching TAS-29 Phase 6.2):

**Required fields** (when applicable):
- `correlation_id`: Always include for distributed tracing
- `task_uuid`: Include for task-level operations
- `step_uuid`: Include for step-level operations
- `namespace`: Include for namespace-specific operations
- `operation`: Operation identifier (e.g., "validate_inventory")
- `component`: Component/subsystem identifier (e.g., "handler", "registry", "event_poller")

**Optional fields**:
- `duration_ms`: For timed operations
- `error_message`: For error context
- `error_class`: For error type information
- `retry_count`: For retryable operations

**Log Level Guidelines** (matching TAS-29 Phase 6.1):
- **ERROR**: Unrecoverable failures requiring intervention
- **WARN**: Degraded operation, retryable failures
- **INFO**: Lifecycle events, state transitions
- **DEBUG**: Detailed diagnostic information
- **TRACE**: Very verbose, hot-path entry/exit

---

### 2. Bootstrap Configuration Types

**File**: `src/bootstrap/BootstrapConfig.ts`

**Purpose**: Configuration options for worker initialization.

```typescript
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
}

/**
 * Result from worker bootstrap.
 * 
 * Contains information about the bootstrapped worker instance.
 */
export interface BootstrapResult {
  /** Whether bootstrap was successful. */
  success: boolean;

  /** Unique identifier for this worker instance. */
  handleId: string;

  /** Full worker identifier string. */
  workerId: string;

  /** Current status (started, already_running). */
  status: string;

  /** Human-readable status message. */
  message?: string;

  /** Error message if bootstrap failed. */
  errorMessage?: string;
}

/**
 * Current worker status.
 * 
 * Contains detailed information about the worker's state and resources.
 */
export interface WorkerStatus {
  /** Whether the worker is currently running. */
  running: boolean;

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

  /** Error message if worker is not running. */
  error?: string;
}
```

---

### 2. Bootstrap API

**File**: `src/bootstrap/bootstrap.ts`

**Purpose**: High-level TypeScript API for worker lifecycle management.

**Python reference**: `workers/python/python/tasker_core/bootstrap.py`

```typescript
import { getTaskerRuntime } from '../ffi/runtime-factory';
import type { BootstrapConfig, BootstrapResult, WorkerStatus } from './BootstrapConfig';

/**
 * Initialize the worker system.
 * 
 * This function bootstraps the full tasker-worker system, including:
 * - Creating a Tokio runtime for async operations
 * - Connecting to the database
 * - Setting up the FFI dispatch channel for step events
 * - Subscribing to domain events
 * 
 * @param config - Optional bootstrap configuration
 * @returns BootstrapResult with worker details and status
 * @throws Error if bootstrap fails or worker is already running
 * 
 * @example Bootstrap with defaults
 * const result = bootstrapWorker();
 * console.log(`Worker ${result.workerId} started`);
 * 
 * @example Bootstrap with custom config
 * const result = bootstrapWorker({
 *   namespace: 'payments',
 *   logLevel: 'debug',
 * });
 */
export function bootstrapWorker(config?: BootstrapConfig): BootstrapResult {
  const runtime = getTaskerRuntime();
  
  // Convert TypeScript config to format expected by FFI
  const rustConfig = config ? {
    namespace: config.namespace || 'default',
    log_level: config.logLevel || 'info',
  } : {};

  // Call strongly-typed bootstrap
  const result = runtime.bootstrapWorker(rustConfig);

  // Map result to TypeScript interface
  return {
    success: result.status === 'started',
    handleId: result.handle_id || '',
    workerId: result.worker_id || '',
    status: result.status || '',
    message: result.message,
    errorMessage: undefined,
  };
}

/**
 * Stop the worker system gracefully.
 * 
 * This function stops the worker system and releases all resources.
 * Safe to call even if the worker is not running.
 * 
 * @returns Status message indicating the result
 * @throws Error if worker cannot be stopped
 * 
 * @example
 * stopWorker();
 * console.log('Worker stopped');
 */
export function stopWorker(): string {
  const runtime = getTaskerRuntime();
  return runtime.stopWorker();
}

/**
 * Get the current worker system status.
 * 
 * Returns detailed information about the worker's current state,
 * including resource usage and operational status.
 * 
 * @returns WorkerStatus with current state and metrics
 * 
 * @example
 * const status = getWorkerStatus();
 * if (status.running) {
 *   console.log(`Pool size: ${status.databasePoolSize}`);
 * } else {
 *   console.log(`Error: ${status.error}`);
 * }
 */
export function getWorkerStatus(): WorkerStatus {
  const runtime = getTaskerRuntime();
  const status = runtime.getWorkerStatus();

  return {
    running: status.running,
    environment: undefined,
    workerCoreStatus: status.worker_core_status,
    webApiEnabled: undefined,
    supportedNamespaces: undefined,
    databasePoolSize: status.database_pool_size,
    databasePoolIdle: status.database_pool_available,
    error: undefined,
  };
}

/**
 * Initiate graceful shutdown of the worker system.
 * 
 * This function begins the graceful shutdown process, allowing
 * in-flight operations to complete before fully stopping.
 * 
 * @returns Status message indicating the transition
 * @throws Error if worker is not running
 * 
 * @example
 * transitionToGracefulShutdown();
 * // Wait for in-flight operations...
 * stopWorker();
 */
export function transitionToGracefulShutdown(): string {
  const runtime = getTaskerRuntime();
  return runtime.transitionToGracefulShutdown();
}

/**
 * Check if the worker system is currently running.
 * 
 * Lightweight check that doesn't query the full status.
 * 
 * @returns True if the worker is running
 * 
 * @example
 * if (!isWorkerRunning()) {
 *   bootstrapWorker();
 * }
 */
export function isWorkerRunning(): boolean {
  const runtime = getTaskerRuntime();
  return runtime.isWorkerRunning();
}
```

---

### 3. Server Entry Point

**File**: `bin/server.ts`

**Purpose**: Production-ready server script with full lifecycle management.

**Python reference**: `workers/python/bin/server.py` (lines 1-356)

```typescript
#!/usr/bin/env bun
/**
 * TypeScript Worker Server.
 * 
 * Production-ready server script that bootstraps Rust foundation via FFI
 * and manages TypeScript handler execution for workflow orchestration.
 */

import { EventEmitter } from 'events';
import {
  bootstrapWorker,
  getWorkerStatus,
  isWorkerRunning,
  stopWorker,
  transitionToGracefulShutdown,
  type BootstrapConfig,
} from '../src/bootstrap/bootstrap';
import { EventPoller, EventNames } from '../src/events/EventPoller';
import { HandlerRegistry } from '../src/registry/HandlerRegistry';
import { StepExecutionSubscriber } from '../src/subscriber/StepExecutionSubscriber';
import type { FfiStepEvent } from '../src/ffi/types';
import * as Logging from '../src/logging';

/**
 * Display startup banner.
 */
function showBanner(): void {
  Logging.logInfo('='.repeat(60));
  Logging.logInfo('Starting TypeScript Worker Bootstrap System');
  Logging.logInfo('='.repeat(60));
  Logging.logInfo(`Environment: ${process.env.TASKER_ENV || 'development'}`, {
    component: 'bootstrap',
    operation: 'banner',
  });
  Logging.logInfo(`Runtime: ${process.versions.bun ? 'Bun ' + process.versions.bun : 'Node.js ' + process.version}`, {
    component: 'bootstrap',
    runtime: process.versions.bun ? 'bun' : 'node',
  });
  Logging.logInfo(`Database URL: ${process.env.DATABASE_URL ? '[REDACTED]' : 'Not set'}`, {
    component: 'bootstrap',
  });
  Logging.logInfo(`Template Path: ${process.env.TASKER_TEMPLATE_PATH || 'Not set'}`, {
    component: 'bootstrap',
  });
  Logging.logInfo(`Web Bind Address: ${process.env.TASKER_WEB_BIND_ADDRESS || 'Not set'}`, {
    component: 'bootstrap',
  });
  Logging.logInfo(`Config Path: ${process.env.TASKER_CONFIG_PATH || 'Not set'}`, {
    component: 'bootstrap',
  });

  Logging.logInfo('Configuration:');
  Logging.logInfo('  TypeScript will initialize Rust foundation via FFI');
  Logging.logInfo('  Worker will process tasks by calling TypeScript handlers');
  if (process.env.TASKER_ENV === 'production') {
    Logging.logInfo('  Production optimizations: Enabled', {
      component: 'bootstrap',
      environment: 'production',
    });
  }
}

/**
 * Perform health check on the worker.
 */
function healthCheck(): { healthy: boolean; status?: any; error?: string } {
  try {
    const status = getWorkerStatus();
    return {
      healthy: status.running,
      status,
    };
  } catch (error) {
    return {
      healthy: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * Initialize handler registry with automatic discovery.
 * 
 * Handler discovery is handled automatically by HandlerRegistry based on:
 * 1. Test environment: Scans for preloaded handlers
 * 2. Template-driven discovery from TASKER_TEMPLATE_PATH or workspace
 * 
 * @returns Number of handlers registered
 */
function initializeHandlerRegistry(): number {
  // HandlerRegistry auto-bootstraps on first instance() call
  // This uses template-driven discovery (matching Python pattern)
  const registry = HandlerRegistry.instance();

  const handlerCount = registry.handlerCount();
  Logging.logInfo(`Handler registry initialized: ${handlerCount} handlers registered`, {
    component: 'registry',
    operation: 'initialize',
    handler_count: String(handlerCount),
  });
  Logging.logInfo(`Registered handlers: ${registry.listHandlers().join(', ')}`, {
    component: 'registry',
    handlers: registry.listHandlers().join(','),
  });

  return handlerCount;
}

/**
 * Main server function.
 */
async function main(): Promise<number> {
  showBanner();

  // Shutdown coordination
  let shutdownRequested = false;
  const shutdownPromise = new Promise<void>((resolve) => {
    const handleShutdown = (signal: string) => {
      Logging.logInfo(`Received ${signal} signal, initiating shutdown...`, {
        component: 'server',
        operation: 'shutdown',
        signal,
      });
      shutdownRequested = true;
      resolve();
    };

    const handleStatus = () => {
      Logging.logInfo('Received SIGUSR1 signal, reporting worker status...', {
        component: 'server',
        operation: 'status_check',
      });
      try {
        const status = getWorkerStatus();
        Logging.logInfo(`Worker Status: ${JSON.stringify(status, null, 2)}`, {
          component: 'server',
          operation: 'status_check',
        });
      } catch (error) {
        Logging.logError(`Failed to get worker status: ${error}`, {
          component: 'server',
          operation: 'status_check',
        });
      }
    };

    // Set up signal handlers
    process.on('SIGTERM', () => handleShutdown('SIGTERM'));
    process.on('SIGINT', () => handleShutdown('SIGINT'));
    process.on('SIGUSR1', handleStatus);
  });

  try {
    Logging.logInfo('Initializing TypeScript Worker Bootstrap...', {
      component: 'bootstrap',
      operation: 'initialize',
    });

    // Discover and register handlers before bootstrap
    Logging.logInfo('Discovering TypeScript handlers...', {
      component: 'registry',
      operation: 'discover',
    });
    const handlerCount = initializeHandlerRegistry();
    Logging.logInfo(`Handler discovery complete: ${handlerCount} handlers registered`, {
      component: 'registry',
      operation: 'discover',
      handler_count: String(handlerCount),
    });

    // Create bootstrap configuration
    const config: BootstrapConfig = {
      namespace: process.env.TASKER_NAMESPACE || 'default',
      logLevel: (process.env.RUST_LOG as any) || 'info',
    };

    // Bootstrap the worker
    const result = bootstrapWorker(config);

    if (!result.success) {
      Logging.logError(`Failed to bootstrap worker: ${result.message}`, {
        component: 'bootstrap',
        operation: 'bootstrap_worker',
        error_message: result.message || 'Unknown error',
      });
      return 3;
    }

    // =================================================================
    // Initialize TypeScript event dispatch system
    // This is the key integration that routes FFI events to TypeScript handlers
    // =================================================================

    Logging.logInfo('Starting TypeScript event dispatch system...', {
      component: 'event_system',
      operation: 'initialize',
    });

    // 1. Start the EventBridge (pub/sub for step events)
    const eventBridge = new EventEmitter();
    eventBridge.setMaxListeners(100); // Increase for production
    Logging.logInfo('  EventBridge: Started', {
      component: 'event_bridge',
      operation: 'start',
    });

    // 2. Create and start StepExecutionSubscriber (routes events to handlers)
    const handlerRegistry = HandlerRegistry.instance();
    const workerId = process.env.HOSTNAME || 'typescript-worker';
    const stepSubscriber = new StepExecutionSubscriber(
      eventBridge,
      handlerRegistry,
      workerId
    );
    stepSubscriber.start();
    Logging.logInfo(`  StepExecutionSubscriber: Started (worker_id=${workerId})`, {
      component: 'subscriber',
      operation: 'start',
      worker_id: workerId,
    });

    // 3. Create EventPoller with callback that publishes to EventBridge
    const onStepEvent = (event: FfiStepEvent) => {
      // Forward polled events to EventBridge for handler dispatch
      eventBridge.emit(EventNames.STEP_EXECUTION_RECEIVED, event);
    };

    const onPollerError = (error: Error) => {
      // Log poller errors
      Logging.logError(`EventPoller error: ${error.message}`, {
        component: 'event_poller',
        operation: 'poll',
        error_message: error.message,
      });
      eventBridge.emit(EventNames.POLLER_ERROR, error);
    };

    const eventPoller = new EventPoller({
      pollingIntervalMs: 10,  // 10ms polling interval
      starvationCheckInterval: 100,  // Check every ~1 second
      cleanupInterval: 1000,  // Cleanup every ~10 seconds
    });
    eventPoller.onStepEvent(onStepEvent);
    eventPoller.onError(onPollerError);

    // 4. Start the poller (runs in background)
    eventPoller.start();
    Logging.logInfo('  EventPoller: Started (10ms polling interval)', {
      component: 'event_poller',
      operation: 'start',
      polling_interval_ms: '10',
    });

    Logging.logInfo('TypeScript event dispatch system ready');
    Logging.logInfo('='.repeat(60));

    Logging.logInfo('TypeScript worker system started successfully', {
      component: 'server',
      operation: 'started',
    });
    Logging.logInfo('  Rust foundation: Bootstrapped via FFI');
    Logging.logInfo('  TypeScript handlers: Registered and ready');
    Logging.logInfo('  Event dispatch: Active (polling -> handlers -> completion)');
    Logging.logInfo('  Worker status: Running and processing tasks');
    Logging.logInfo('Worker ready and processing tasks');
    Logging.logInfo('='.repeat(60));

    // Main worker loop
    const sleepInterval = process.env.TASKER_ENV === 'production' ? 5000 : 1000;
    let loopCount = 0;

    const mainLoop = async () => {
      while (!shutdownRequested) {
        await new Promise(resolve => setTimeout(resolve, sleepInterval));

        if (shutdownRequested) {
          break;
        }

        // Periodic health check (every 60 iterations)
        loopCount++;
        if (loopCount % 60 === 0) {
          try {
            const healthStatus = healthCheck();
            if (healthStatus.healthy) {
              Logging.logDebug(`Health check #${loopCount / 60}: OK`, {
                component: 'server',
                operation: 'health_check',
              });
            } else {
              Logging.logWarn(
                `Health check #${loopCount / 60}: UNHEALTHY - ${healthStatus.error}`,
                {
                  component: 'server',
                  operation: 'health_check',
                  error: healthStatus.error || 'unknown',
                }
              );
            }
          } catch (error) {
            Logging.logWarn(`Health check failed: ${error}`, {
              component: 'server',
              operation: 'health_check',
            });
          }
        }
      }
    };

    // Race between main loop and shutdown signal
    await Promise.race([mainLoop(), shutdownPromise]);

    // Shutdown sequence
    Logging.logInfo('Starting shutdown sequence...', {
      component: 'server',
      operation: 'shutdown',
    });

    try {
      // 1. Stop EventPoller first (stops new events from being polled)
      Logging.logInfo('  Stopping EventPoller...', {
        component: 'event_poller',
        operation: 'stop',
      });
      await eventPoller.stop(5000);
      Logging.logInfo('  EventPoller stopped', {
        component: 'event_poller',
        operation: 'stop',
      });

      // 2. Stop StepExecutionSubscriber (stops event routing)
      Logging.logInfo('  Stopping StepExecutionSubscriber...', {
        component: 'subscriber',
        operation: 'stop',
      });
      stepSubscriber.stop();
      Logging.logInfo('  StepExecutionSubscriber stopped', {
        component: 'subscriber',
        operation: 'stop',
      });

      // 3. Clear EventBridge (remove all listeners)
      Logging.logInfo('  Stopping EventBridge...', {
        component: 'event_bridge',
        operation: 'stop',
      });
      eventBridge.removeAllListeners();
      Logging.logInfo('  EventBridge stopped', {
        component: 'event_bridge',
        operation: 'stop',
      });

      // 4. Stop Rust worker foundation
      if (isWorkerRunning()) {
        Logging.logInfo('  Transitioning to graceful shutdown...', {
          component: 'worker',
          operation: 'graceful_shutdown',
        });
        transitionToGracefulShutdown();
        Logging.logInfo('  Stopping Rust worker...', {
          component: 'worker',
          operation: 'stop',
        });
        stopWorker();
        Logging.logInfo('  Rust worker stopped', {
          component: 'worker',
          operation: 'stop',
        });
      }

      Logging.logInfo('TypeScript worker shutdown completed successfully', {
        component: 'server',
        operation: 'shutdown',
      });
    } catch (error) {
      Logging.logError(`Error during shutdown: ${error}`, {
        component: 'server',
        operation: 'shutdown',
        error_message: error instanceof Error ? error.message : String(error),
      });
      return 1;
    }

    Logging.logInfo('TypeScript Worker Server terminated gracefully', {
      component: 'server',
      operation: 'shutdown',
    });
    return 0;

  } catch (error) {
    Logging.logError(`Unexpected error during startup: ${error}`, {
      component: 'server',
      operation: 'startup',
      error_message: error instanceof Error ? error.message : String(error),
    });
    if (error instanceof Error) {
      Logging.logError(error.stack || '', {
        component: 'server',
        operation: 'startup',
      });
    }
    return 4;
  }
}

// Run the server
main().then((exitCode) => {
  process.exit(exitCode);
}).catch((error) => {
  Logging.logError(`Unhandled error: ${error}`, {
    component: 'server',
    operation: 'fatal',
    error_message: error instanceof Error ? error.message : String(error),
  });
  process.exit(5);
});
```

---

### 4. EventPoller Implementation

**File**: `src/events/EventPoller.ts`

**Purpose**: Poll FFI for step events and publish them to EventBridge.

**Python reference**: `workers/python/python/tasker_core/event_poller.py`

```typescript
import { getTaskerRuntime } from '../ffi/runtime-factory';
import type { FfiStepEvent } from '../ffi/types';

export enum EventNames {
  STEP_EXECUTION_RECEIVED = 'step:execution:received',
  POLLER_ERROR = 'poller:error',
  POLLER_STOPPED = 'poller:stopped',
}

export interface EventPollerConfig {
  /** Polling interval in milliseconds (default: 10). */
  pollingIntervalMs?: number;

  /** How often to check for starvation (default: 100 iterations). */
  starvationCheckInterval?: number;

  /** How often to run cleanup (default: 1000 iterations). */
  cleanupInterval?: number;
}

/**
 * Event poller that polls FFI for step events.
 * 
 * Runs in a background interval, polling poll_step_events() and
 * invoking callbacks for each event received.
 * 
 * Matches Python's EventPoller pattern.
 */
export class EventPoller {
  private config: Required<EventPollerConfig>;
  private running: boolean = false;
  private intervalId: NodeJS.Timeout | null = null;
  private stepEventCallback: ((event: FfiStepEvent) => void) | null = null;
  private errorCallback: ((error: Error) => void) | null = null;

  constructor(config: EventPollerConfig = {}) {
    this.config = {
      pollingIntervalMs: config.pollingIntervalMs ?? 10,
      starvationCheckInterval: config.starvationCheckInterval ?? 100,
      cleanupInterval: config.cleanupInterval ?? 1000,
    };
  }

  /**
   * Register callback for step events.
   */
  onStepEvent(callback: (event: FfiStepEvent) => void): void {
    this.stepEventCallback = callback;
  }

  /**
   * Register callback for errors.
   */
  onError(callback: (error: Error) => void): void {
    this.errorCallback = callback;
  }

  /**
   * Start polling for events.
   */
  start(): void {
    if (this.running) {
      return;
    }

    this.running = true;
    this.intervalId = setInterval(() => {
      this.poll();
    }, this.config.pollingIntervalMs);
  }

  /**
   * Stop polling for events.
   * 
   * @param timeoutMs - How long to wait for graceful stop
   */
  async stop(timeoutMs: number = 5000): Promise<void> {
    if (!this.running) {
      return;
    }

    this.running = false;

    if (this.intervalId) {
      clearInterval(this.intervalId);
      this.intervalId = null;
    }

    // Wait a bit for in-flight polls to complete
    await new Promise(resolve => setTimeout(resolve, Math.min(100, timeoutMs)));
  }

  /**
   * Poll FFI for step events.
   */
  private poll(): void {
    if (!this.running) {
      return;
    }

    try {
      const runtime = getTaskerRuntime();
      const events = runtime.pollStepEvents();

      // Process each event
      for (const event of events) {
        if (this.stepEventCallback) {
          this.stepEventCallback(event);
        }
      }
    } catch (error) {
      if (this.errorCallback) {
        this.errorCallback(error instanceof Error ? error : new Error(String(error)));
      }
    }
  }
}
```

---

### 5. StepExecutionSubscriber Implementation

**File**: `src/subscriber/StepExecutionSubscriber.ts`

**Purpose**: Subscribe to step events and dispatch them to handlers.

**Python reference**: `workers/python/python/tasker_core/step_execution_subscriber.py`

```typescript
import type { EventEmitter } from 'events';
import { StepContext } from '../types/StepContext';
import { getTaskerRuntime } from '../ffi/runtime-factory';
import type { TaskerRuntime, StepExecutionResult } from '../ffi/runtime-interface';
import type { HandlerRegistry } from '../registry/HandlerRegistry';
import type { FfiStepEvent } from '../ffi/types';
import { EventNames } from '../events/EventPoller';

/**
 * Subscribes to step execution events and dispatches them to handlers.
 * 
 * This is the critical component that connects the FFI event stream
 * to TypeScript handler execution.
 * 
 * Matches Python's StepExecutionSubscriber pattern.
 */
export class StepExecutionSubscriber {
  private eventBridge: EventEmitter;
  private handlerRegistry: HandlerRegistry;
  private workerId: string;
  private running: boolean = false;

  constructor(
    eventBridge: EventEmitter,
    handlerRegistry: HandlerRegistry,
    workerId: string
  ) {
    this.eventBridge = eventBridge;
    this.handlerRegistry = handlerRegistry;
    this.workerId = workerId;
  }

  /**
   * Start subscribing to events.
   */
  start(): void {
    if (this.running) {
      return;
    }

    this.running = true;
    this.eventBridge.on(EventNames.STEP_EXECUTION_RECEIVED, (event: FfiStepEvent) => {
      this.processEvent(event);
    });
  }

  /**
   * Stop subscribing to events.
   */
  stop(): void {
    if (!this.running) {
      return;
    }

    this.running = false;
    this.eventBridge.removeAllListeners(EventNames.STEP_EXECUTION_RECEIVED);
  }

  /**
   * Process a single step event.
   */
  private async processEvent(event: FfiStepEvent): Promise<void> {
    const startTime = Date.now();

    try {
      // Extract handler name from event
      const handlerName = this.extractHandlerName(event);
      if (!handlerName) {
        await this.submitErrorResult(
          event,
          'No handler name found in event',
          Date.now() - startTime
        );
        return;
      }

      // Resolve handler
      const handler = this.handlerRegistry.resolve(handlerName);
      if (!handler) {
        await this.submitErrorResult(
          event,
          `Handler not found: ${handlerName}`,
          Date.now() - startTime
        );
        return;
      }

      // Create context
      const context = StepContext.fromFfiEvent(event, handlerName);

      // Execute handler
      const result = await handler.call(context);
      const executionTimeMs = Date.now() - startTime;

      // Submit result
      await this.submitResult(event, result, executionTimeMs);

    } catch (error) {
      const executionTimeMs = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      await this.submitErrorResult(event, errorMessage, executionTimeMs);
    }
  }

  /**
   * Extract handler name from FFI event.
   */
  private extractHandlerName(event: FfiStepEvent): string | null {
    const tss = event.task_sequence_step;
    if (!tss) {
      return null;
    }

    const stepDefinition = tss.step_definition || {};
    const handler = stepDefinition.handler || {};
    return handler.callable || null;
  }

  /**
   * Submit success result via FFI.
   */
  private async submitResult(
    event: FfiStepEvent,
    result: any,
    executionTimeMs: number
  ): Promise<void> {
    const runtime = getTaskerRuntime();

    const executionResult: StepExecutionResult = {
      step_uuid: event.step_uuid,
      task_uuid: event.task_uuid,
      success: result.success,
      status: result.success ? 'completed' : 'error',
      execution_time_ms: executionTimeMs,
      result: result.result,
      error: result.success ? undefined : {
        message: result.errorMessage,
        error_type: result.errorType,
        error_code: result.errorCode,
        retryable: result.retryable,
      },
      worker_id: this.workerId,
      correlation_id: event.correlation_id,
      metadata: result.metadata || {},
    };

    runtime.completeStepEvent(event.event_id, executionResult);
  }

  /**
   * Submit error result via FFI.
   */
  private async submitErrorResult(
    event: FfiStepEvent,
    errorMessage: string,
    executionTimeMs: number
  ): Promise<void> {
    const runtime = getTaskerRuntime();

    const executionResult: StepExecutionResult = {
      step_uuid: event.step_uuid,
      task_uuid: event.task_uuid,
      success: false,
      status: 'error',
      execution_time_ms: executionTimeMs,
      error: {
        message: errorMessage,
        error_type: 'handler_error',
        retryable: true,
      },
      worker_id: this.workerId,
      correlation_id: event.correlation_id,
      metadata: {},
    };

    runtime.completeStepEvent(event.event_id, executionResult);
  }
}
```

---

## File Structure

```
workers/typescript/
├── bin/
│   └── server.ts                           # Server entry point
├── src/
│   ├── bootstrap/
│   │   ├── BootstrapConfig.ts              # Bootstrap types
│   │   ├── bootstrap.ts                    # Bootstrap API
│   │   └── index.ts                        # Exports
│   ├── events/
│   │   ├── EventPoller.ts                  # FFI event polling
│   │   └── index.ts                        # Exports
│   ├── subscriber/
│   │   ├── StepExecutionSubscriber.ts      # Handler dispatch
│   │   └── index.ts                        # Exports
│   ├── logging/
│   │   └── index.ts                        # Structured logging via FFI
│   ├── ffi/                                # From TAS-101
│   ├── types/                              # From TAS-102
│   ├── handlers/                           # From TAS-102/103
│   ├── registry/                           # From TAS-102
│   └── index.ts                            # Main package exports
├── package.json
├── tsconfig.json
└── README.md
```

---

## Package.json Configuration

```json
{
  "name": "@tasker/worker-typescript",
  "version": "0.1.0",
  "type": "module",
  "bin": {
    "tasker-worker": "./bin/server.ts"
  },
  "scripts": {
    "dev": "bun run bin/server.ts",
    "build": "tsc",
    "test": "bun test",
    "lint": "eslint src/ bin/",
    "typecheck": "tsc --noEmit"
  },
  "main": "./src/index.ts",
  "exports": {
    ".": {
      "import": "./src/index.ts",
      "types": "./src/index.ts"
    }
  },
  "dependencies": {
    "ffi-napi": "^4.0.3",
    "ref-napi": "^3.0.3"
  },
  "devDependencies": {
    "@types/node": "^20.0.0",
    "typescript": "^5.3.0"
  },
  "engines": {
    "node": ">=18.0.0"
  }
}
```

---

## Main Package Exports

**File**: `src/index.ts`

```typescript
// Bootstrap API
export {
  bootstrapWorker,
  stopWorker,
  getWorkerStatus,
  transitionToGracefulShutdown,
  isWorkerRunning,
} from './bootstrap/bootstrap';

export type {
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
} from './bootstrap/BootstrapConfig';

// Logging
export {
  logError,
  logWarn,
  logInfo,
  logDebug,
  logTrace,
} from './logging';
export type { LogFields } from './logging';

// Types
export type { StepContext } from './types/StepContext';
export type { StepHandlerResult } from './types/StepHandlerResult';
export { ErrorType } from './types/ErrorType';

// Handlers
export { StepHandler } from './handlers/StepHandler';
export { ApiHandler, ApiResponse } from './handlers/ApiHandler';
export { DecisionHandler, DecisionType } from './handlers/DecisionHandler';
export { BatchableMixin } from './handlers/Batchable';
export type { Batchable } from './handlers/Batchable';

// Registry
export { HandlerRegistry } from './registry/HandlerRegistry';

// Events
export { EventPoller, EventNames } from './events/EventPoller';

// Subscriber
export { StepExecutionSubscriber } from './subscriber/StepExecutionSubscriber';
```

---

## Usage Examples

### Example 1: Running the Server

```bash
# Development mode
export DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_development"
export TASKER_ENV="development"
export RUST_LOG="info"

bun run bin/server.ts

# Production mode
export TASKER_ENV="production"
export RUST_LOG="warn"
bun run bin/server.ts
```

### Example 2: Embedded/Headless Mode

```typescript
import {
  bootstrapWorker,
  stopWorker,
  getWorkerStatus,
} from '@tasker/worker-typescript';
import { HandlerRegistry } from '@tasker/worker-typescript';
import { MyCustomHandler } from './handlers/MyCustomHandler';

// Register custom handlers
const registry = HandlerRegistry.instance(true); // Skip auto-bootstrap
registry.register('my_handler', MyCustomHandler);

// Bootstrap worker
const result = bootstrapWorker({
  namespace: 'my_namespace',
  logLevel: 'debug',
});

console.log(`Worker started: ${result.workerId}`);

// ... application logic ...

// Check status
const status = getWorkerStatus();
console.log(`Worker running: ${status.running}`);

// Graceful shutdown
process.on('SIGTERM', () => {
  console.log('Shutting down...');
  stopWorker();
  process.exit(0);
});
```

### Example 3: Custom Event Handling

```typescript
import { EventEmitter } from 'events';
import { EventPoller, EventNames } from '@tasker/worker-typescript';

const eventBridge = new EventEmitter();

// Custom event handler
eventBridge.on(EventNames.STEP_EXECUTION_RECEIVED, (event) => {
  console.log(`Received step event: ${event.step_uuid}`);
  // Custom processing...
});

const poller = new EventPoller({ pollingIntervalMs: 10 });
poller.onStepEvent((event) => {
  eventBridge.emit(EventNames.STEP_EXECUTION_RECEIVED, event);
});

poller.start();

// Later...
await poller.stop();
```

---

## Success Criteria

- [ ] `bin/server.ts` starts worker successfully
- [ ] Bootstrap API (`bootstrapWorker`, `stopWorker`, etc.) working
- [ ] Signal handlers (SIGTERM, SIGINT, SIGUSR1) trigger appropriate actions
- [ ] EventPoller successfully polls FFI and publishes events
- [ ] StepExecutionSubscriber dispatches events to handlers
- [ ] Graceful shutdown sequence executes correctly
- [ ] Health checks run periodically
- [ ] Worker status can be queried
- [ ] Headless/embedded mode works without server
- [ ] Integration test: full lifecycle (start → process step → shutdown)
- [ ] Works in both Bun and Node.js 18+

---

## Dependencies

**Requires**:
- TAS-101: FFI Bridge and strongly-typed `TaskerRuntime` interface
- TAS-102: Handler API and Registry
- TAS-103: Specialized Handlers (optional, for examples)

**Blocks**:
- TAS-105: Testing and Examples
- TAS-107: Documentation

**Key Dependency Notes**:
- All FFI calls use the strongly-typed `TaskerRuntime` interface from TAS-101
- This provides full type safety: `runtime.bootstrapWorker(config)` returns `BootstrapResult`
- No generic `callFfiFunction()` calls - every method has explicit types
- See TAS-101 Phase 4-7 for `TaskerRuntime` interface and implementations

---

## Estimated Breakdown

| Task | Estimated Time |
|------|----------------|
| Structured logging API | 1 hour |
| Bootstrap config types | 0.5 hours |
| Bootstrap API implementation | 1 hour |
| EventPoller implementation | 1.5 hours |
| StepExecutionSubscriber implementation | 2 hours |
| Server entry point (`bin/server.ts`) | 2 hours |
| Signal handling and shutdown | 1 hour |
| Health check implementation | 0.5 hours |
| Package.json and exports | 0.5 hours |
| Integration testing | 2 hours |
| Documentation | 1 hour |
| **Total** | **13 hours (~1.5-2 days)** |

---

## Implementation Notes

### Signal Handling

Both Bun and Node.js support `process.on('SIGTERM', ...)` for signal handling. The implementation is runtime-agnostic.

### EventEmitter

Use Node.js built-in `EventEmitter` (works in both Bun and Node.js) for the event bridge. Alternative: `eventemitter3` for better performance.

### Shutdown Order

Critical to follow this exact order to prevent data loss:
1. Stop EventPoller (no new events)
2. Stop StepExecutionSubscriber (no new handler invocations)
3. Clear EventBridge (cleanup subscriptions)
4. Transition Rust to graceful shutdown
5. Stop Rust worker

### Health Checks

Health checks should be lightweight and not block the main loop. Run periodically (every 60 iterations in production).

### Error Handling

All FFI calls should be wrapped in try/catch. Errors during handler execution should be converted to step failure results and submitted via FFI.

---

## Testing Strategy

### Integration Test Example

```typescript
import { describe, test, expect, beforeAll, afterAll } from 'bun:test';
import {
  bootstrapWorker,
  stopWorker,
  getWorkerStatus,
  isWorkerRunning,
} from '../src/bootstrap/bootstrap';

describe('Worker Lifecycle', () => {
  test('full lifecycle', async () => {
    // Bootstrap
    const result = bootstrapWorker({ namespace: 'test' });
    expect(result.success).toBe(true);
    expect(isWorkerRunning()).toBe(true);

    // Check status
    const status = getWorkerStatus();
    expect(status.running).toBe(true);

    // ... process step events ...

    // Shutdown
    stopWorker();
    expect(isWorkerRunning()).toBe(false);
  });
});
```

---

## Next Steps

After TAS-104 completion:

1. **TAS-105**: Add comprehensive tests and example handlers
2. **TAS-107**: Complete documentation with deployment guides
3. **Production Hardening**: Add metrics, monitoring, error reporting

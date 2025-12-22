#!/usr/bin/env bun

/**
 * TypeScript Worker Server.
 *
 * Production-ready server script that bootstraps Rust foundation via FFI
 * and manages TypeScript handler execution for workflow orchestration.
 *
 * Usage:
 *   bun run bin/server.ts
 *   TASKER_ENV=production bun run bin/server.ts
 *
 * Environment Variables:
 *   DATABASE_URL        - PostgreSQL connection string (required)
 *   TASKER_ENV          - Environment: development, test, production
 *   RUST_LOG            - Log level: trace, debug, info, warn, error
 *   TASKER_NAMESPACE    - Task namespace to handle (default: "default")
 *   TASKER_CONFIG_PATH  - Path to TOML configuration file
 *   TASKER_TEMPLATE_PATH - Path to task template directory
 */

import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import {
  bootstrapWorker,
  getVersion,
  getWorkerStatus,
  healthCheck,
  isWorkerRunning,
  stopWorker,
  transitionToGracefulShutdown,
} from '../src/bootstrap/bootstrap.js';
import type { BootstrapConfig, BootstrapResult } from '../src/bootstrap/types.js';
import { getGlobalEmitter, type TaskerEventEmitter } from '../src/events/event-emitter.js';
import { createEventPoller, type EventPoller } from '../src/events/event-poller.js';
import { getTaskerRuntime, type TaskerRuntime } from '../src/ffi/runtime-factory.js';
import { HandlerRegistry } from '../src/handler/registry.js';
import { createLogger, logDebug, logWarn } from '../src/logging/index.js';
import { StepExecutionSubscriber } from '../src/subscriber/step-execution-subscriber.js';

// Server-specific logger
const log = createLogger({ component: 'server' });

// Environment helpers
const isProduction = (): boolean => process.env.TASKER_ENV === 'production';
const getNamespace = (): string => process.env.TASKER_NAMESPACE || 'default';
const getLogLevel = (): BootstrapConfig['logLevel'] =>
  (process.env.RUST_LOG as BootstrapConfig['logLevel']) || 'info';

/**
 * Shutdown controller for coordinating graceful shutdown.
 */
class ShutdownController {
  private shutdownRequested = false;
  private resolver: (() => void) | null = null;
  readonly promise: Promise<void>;

  constructor() {
    this.promise = new Promise<void>((resolve) => {
      this.resolver = resolve;
    });
  }

  get isRequested(): boolean {
    return this.shutdownRequested;
  }

  trigger(signal: string): void {
    log.info(`Received ${signal} signal, initiating shutdown...`, {
      operation: 'shutdown',
      signal,
    });
    this.shutdownRequested = true;
    this.resolver?.();
  }
}

/**
 * Server components for lifecycle management.
 */
interface ServerComponents {
  runtime: TaskerRuntime;
  emitter: TaskerEventEmitter;
  registry: HandlerRegistry;
  eventPoller: EventPoller;
  stepSubscriber: StepExecutionSubscriber;
  workerId: string;
}

// =============================================================================
// Initialization Functions
// =============================================================================

/**
 * Find the FFI library path.
 */
function findLibraryPath(): string | null {
  const libName =
    process.platform === 'darwin'
      ? 'libtasker_worker.dylib'
      : process.platform === 'win32'
        ? 'tasker_worker.dll'
        : 'libtasker_worker.so';

  // Search paths in order of preference
  const searchPaths = [
    // Relative to bin directory (development)
    join(dirname(import.meta.dir), '..', '..', 'target', 'release', libName),
    join(dirname(import.meta.dir), '..', '..', 'target', 'debug', libName),
    // Workspace target (running from workers/typescript)
    join(process.cwd(), '..', '..', 'target', 'release', libName),
    join(process.cwd(), '..', '..', 'target', 'debug', libName),
    // Environment variable override
    process.env.TASKER_FFI_LIBRARY_PATH ?? '',
  ].filter(Boolean);

  for (const path of searchPaths) {
    if (existsSync(path)) {
      return path;
    }
  }

  return null;
}

/**
 * Display startup banner.
 */
function showBanner(): void {
  console.log('='.repeat(60));
  console.log('Starting TypeScript Worker Bootstrap System');
  console.log('='.repeat(60));

  const info = [
    `Environment: ${process.env.TASKER_ENV || 'development'}`,
    `Runtime: ${process.versions.bun ? `Bun ${process.versions.bun}` : `Node.js ${process.version}`}`,
    `PID: ${process.pid}`,
    `Database URL: ${process.env.DATABASE_URL ? '[REDACTED]' : 'Not set'}`,
    `Template Path: ${process.env.TASKER_TEMPLATE_PATH || 'Not set'}`,
    `Config Path: ${process.env.TASKER_CONFIG_PATH || 'Not set'}`,
    `Namespace: ${getNamespace()}`,
  ];

  for (const line of info) {
    console.log(`  ${line}`);
  }

  console.log('');
  console.log('Configuration:');
  console.log('  TypeScript will initialize Rust foundation via FFI');
  console.log('  Worker will process tasks by calling TypeScript handlers');

  if (isProduction()) {
    console.log('  Production optimizations: Enabled');
  }

  console.log('='.repeat(60));
}

/**
 * Load FFI library and initialize runtime.
 */
async function loadFfiLibrary(): Promise<{ runtime: TaskerRuntime; path: string } | null> {
  log.info('Loading FFI library...', { operation: 'load_ffi' });
  const libraryPath = findLibraryPath();

  if (!libraryPath) {
    log.error('FFI library not found. Build with: cargo build -p tasker-worker-ts --release', {
      operation: 'load_ffi',
    });
    return null;
  }

  log.info(`FFI library found: ${libraryPath}`, { operation: 'load_ffi' });

  const runtime = await getTaskerRuntime();
  await runtime.load(libraryPath);

  log.info(`FFI library loaded successfully (version: ${getVersion()})`, {
    operation: 'load_ffi',
  });

  return { runtime, path: libraryPath };
}

/**
 * Initialize the handler registry.
 */
function initializeHandlerRegistry(): { registry: HandlerRegistry; count: number } {
  log.info('Discovering TypeScript handlers...', { operation: 'discover_handlers' });

  const registry = HandlerRegistry.instance();
  const count = registry.handlerCount();

  log.info(`Handler registry initialized: ${count} handlers registered`, {
    operation: 'initialize_registry',
    handler_count: String(count),
  });

  if (count > 0) {
    log.info(`Registered handlers: ${registry.listHandlers().join(', ')}`, {
      operation: 'list_handlers',
    });
  } else {
    log.warn('No handlers registered. Did you register handlers before starting?', {
      operation: 'discover_handlers',
    });
  }

  return { registry, count };
}

/**
 * Bootstrap the Rust worker.
 */
async function bootstrapRustWorker(): Promise<BootstrapResult> {
  const config: BootstrapConfig = {
    namespace: getNamespace(),
    logLevel: getLogLevel(),
    configPath: process.env.TASKER_CONFIG_PATH,
    databaseUrl: process.env.DATABASE_URL,
  };

  log.info('Bootstrapping Rust worker...', { operation: 'bootstrap' });
  return bootstrapWorker(config);
}

/**
 * Start the TypeScript event dispatch system.
 */
function startEventSystem(
  runtime: TaskerRuntime,
  registry: HandlerRegistry,
  workerId: string
): { emitter: TaskerEventEmitter; eventPoller: EventPoller; stepSubscriber: StepExecutionSubscriber } {
  log.info('Starting TypeScript event dispatch system...', {
    operation: 'start_event_system',
  });

  // 1. Get the global event emitter
  const emitter = getGlobalEmitter();
  log.info('  EventEmitter: Ready', { operation: 'start_event_system' });

  // 2. Create and start StepExecutionSubscriber
  const stepSubscriber = new StepExecutionSubscriber(emitter, registry, {
    workerId,
    maxConcurrent: 10,
    handlerTimeoutMs: 300000, // 5 minutes
  });
  stepSubscriber.start();
  log.info(`  StepExecutionSubscriber: Started (worker_id=${workerId})`, {
    operation: 'start_event_system',
  });

  // 3. Create and start EventPoller
  const eventPoller = createEventPoller(runtime, {
    pollIntervalMs: 10,
    starvationCheckInterval: 100,
    cleanupInterval: 1000,
    metricsInterval: 100,
  });
  eventPoller.start();
  log.info('  EventPoller: Started (10ms polling interval)', {
    operation: 'start_event_system',
  });

  return { emitter, eventPoller, stepSubscriber };
}

/**
 * Display success banner after startup.
 */
function showSuccessBanner(): void {
  console.log('');
  console.log('='.repeat(60));
  console.log('TypeScript worker system started successfully');
  console.log('  Rust foundation: Bootstrapped via FFI');
  console.log('  TypeScript handlers: Registered and ready');
  console.log('  Event dispatch: Active (polling -> handlers -> completion)');
  console.log('  Worker status: Running and processing tasks');
  console.log('');
  console.log('Worker ready and processing tasks');
  console.log('Press Ctrl+C to stop');
  console.log('='.repeat(60));
  console.log('');
}

// =============================================================================
// Runtime Functions
// =============================================================================

/**
 * Perform a health check on the worker.
 */
function performHealthCheck(): { healthy: boolean; status?: unknown; error?: string } {
  try {
    const ffiHealthy = healthCheck();
    if (!ffiHealthy) {
      return { healthy: false, error: 'FFI health check failed' };
    }

    const status = getWorkerStatus();
    return { healthy: status.running, status };
  } catch (error) {
    return {
      healthy: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * Run the main worker loop with periodic health checks.
 */
async function runMainLoop(shutdown: ShutdownController): Promise<void> {
  const sleepInterval = isProduction() ? 5000 : 1000;
  const healthCheckInterval = isProduction() ? 300 : 60;
  let loopCount = 0;

  while (!shutdown.isRequested) {
    await new Promise((resolve) => setTimeout(resolve, sleepInterval));

    if (shutdown.isRequested) {
      break;
    }

    loopCount++;
    if (loopCount % healthCheckInterval === 0) {
      const checkNumber = loopCount / healthCheckInterval;
      const healthStatus = performHealthCheck();

      if (healthStatus.healthy) {
        logDebug(`Health check #${checkNumber}: OK`, {
          component: 'server',
          operation: 'health_check',
        });
      } else {
        logWarn(`Health check #${checkNumber}: UNHEALTHY - ${healthStatus.error}`, {
          component: 'server',
          operation: 'health_check',
          error: healthStatus.error ?? 'unknown',
        });
      }
    }
  }
}

// =============================================================================
// Shutdown Functions
// =============================================================================

/**
 * Execute the graceful shutdown sequence.
 */
async function executeShutdown(components: ServerComponents): Promise<void> {
  console.log('');
  log.info('Starting shutdown sequence...', { operation: 'shutdown' });

  // 1. Stop EventPoller (no new events)
  log.info('  Stopping EventPoller...', { operation: 'shutdown' });
  await components.eventPoller.stop();
  log.info('  EventPoller stopped', { operation: 'shutdown' });

  // 2. Stop StepExecutionSubscriber (no new handler invocations)
  log.info('  Stopping StepExecutionSubscriber...', { operation: 'shutdown' });
  components.stepSubscriber.stop();
  await components.stepSubscriber.waitForCompletion(5000);
  log.info('  StepExecutionSubscriber stopped', { operation: 'shutdown' });

  // 3. Clear EventEmitter listeners
  log.info('  Clearing event listeners...', { operation: 'shutdown' });
  components.emitter.removeAllListeners();
  log.info('  Event listeners cleared', { operation: 'shutdown' });

  // 4. Transition to graceful shutdown and stop worker
  if (isWorkerRunning()) {
    log.info('  Transitioning to graceful shutdown...', { operation: 'shutdown' });
    transitionToGracefulShutdown();

    log.info('  Stopping Rust worker...', { operation: 'shutdown' });
    stopWorker();
    log.info('  Rust worker stopped', { operation: 'shutdown' });
  }

  log.info('TypeScript worker shutdown completed successfully', {
    operation: 'shutdown',
  });

  console.log('');
  console.log('TypeScript Worker Server terminated gracefully');
}

/**
 * Cleanup on error.
 */
async function cleanupOnError(
  eventPoller: EventPoller | null,
  stepSubscriber: StepExecutionSubscriber | null
): Promise<void> {
  try {
    if (eventPoller) {
      await eventPoller.stop();
    }
    if (stepSubscriber) {
      stepSubscriber.stop();
    }
    if (isWorkerRunning()) {
      stopWorker();
    }
  } catch {
    // Ignore cleanup errors
  }
}

// =============================================================================
// Main Entry Point
// =============================================================================

/**
 * Main server function.
 */
async function main(): Promise<number> {
  showBanner();

  // Set up shutdown controller
  const shutdown = new ShutdownController();

  // Set up signal handlers
  process.on('SIGTERM', () => shutdown.trigger('SIGTERM'));
  process.on('SIGINT', () => shutdown.trigger('SIGINT'));
  process.on('SIGUSR1', () => {
    log.info('Received SIGUSR1 signal, reporting worker status...', {
      operation: 'status_check',
    });
    try {
      const status = getWorkerStatus();
      console.log('\nWorker Status:');
      console.log(JSON.stringify(status, null, 2));
      console.log('');
    } catch (error) {
      log.error(`Failed to get worker status: ${error}`, {
        operation: 'status_check',
      });
    }
  });

  // Track components for cleanup
  let eventPoller: EventPoller | null = null;
  let stepSubscriber: StepExecutionSubscriber | null = null;

  try {
    // Load FFI library
    const ffiResult = await loadFfiLibrary();
    if (!ffiResult) {
      return 2;
    }

    // Initialize handler registry
    const { registry } = initializeHandlerRegistry();

    // Bootstrap Rust worker
    const result = await bootstrapRustWorker();
    if (!result.success) {
      log.error(`Failed to bootstrap worker: ${result.message}`, {
        operation: 'bootstrap',
        error_message: result.error ?? result.message,
      });
      return 3;
    }

    const workerId = result.workerId ?? `typescript-worker-${process.pid}`;
    log.info(`Worker bootstrapped successfully (ID: ${workerId})`, {
      operation: 'bootstrap',
      worker_id: workerId,
    });

    // Start event system
    const eventSystem = startEventSystem(ffiResult.runtime, registry, workerId);
    eventPoller = eventSystem.eventPoller;
    stepSubscriber = eventSystem.stepSubscriber;

    showSuccessBanner();

    // Run main loop until shutdown
    await Promise.race([runMainLoop(shutdown), shutdown.promise]);

    // Execute shutdown sequence
    await executeShutdown({
      runtime: ffiResult.runtime,
      emitter: eventSystem.emitter,
      registry,
      eventPoller,
      stepSubscriber,
      workerId,
    });

    return 0;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    log.error(`Unexpected error during startup: ${errorMessage}`, {
      operation: 'startup',
      error_message: errorMessage,
    });

    if (error instanceof Error && error.stack) {
      console.error(error.stack);
    }

    await cleanupOnError(eventPoller, stepSubscriber);
    return 4;
  }
}

// Run the server
main()
  .then((exitCode) => {
    process.exit(exitCode);
  })
  .catch((error) => {
    console.error('Unhandled error:', error);
    process.exit(5);
  });

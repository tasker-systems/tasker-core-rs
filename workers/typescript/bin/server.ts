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

import { getTaskerRuntime, setFfiLibraryPath } from '../src/ffi/runtime-factory.js';
import {
  bootstrapWorker,
  stopWorker,
  getWorkerStatus,
  transitionToGracefulShutdown,
  isWorkerRunning,
  healthCheck,
  getVersion,
} from '../src/bootstrap/bootstrap.js';
import type { BootstrapConfig } from '../src/bootstrap/types.js';
import { EventPoller, createEventPoller } from '../src/events/event-poller.js';
import { getGlobalEmitter } from '../src/events/event-emitter.js';
import { StepExecutionSubscriber } from '../src/subscriber/step-execution-subscriber.js';
import { HandlerRegistry } from '../src/handler/registry.js';
import { logError, logInfo, logWarn, logDebug, createLogger } from '../src/logging/index.js';
import { join, dirname } from 'node:path';
import { existsSync } from 'node:fs';

// Server-specific logger
const log = createLogger({ component: 'server' });

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
    `Namespace: ${process.env.TASKER_NAMESPACE || 'default'}`,
  ];

  for (const line of info) {
    console.log(`  ${line}`);
  }

  console.log('');
  console.log('Configuration:');
  console.log('  TypeScript will initialize Rust foundation via FFI');
  console.log('  Worker will process tasks by calling TypeScript handlers');

  if (process.env.TASKER_ENV === 'production') {
    console.log('  Production optimizations: Enabled');
  }

  console.log('='.repeat(60));
}

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
 * Initialize the handler registry.
 *
 * Handlers should be registered before starting the server.
 * For now, we just get the singleton instance which may auto-discover handlers.
 */
function initializeHandlerRegistry(): number {
  const registry = HandlerRegistry.instance();
  const handlerCount = registry.handlerCount();

  log.info(`Handler registry initialized: ${handlerCount} handlers registered`, {
    operation: 'initialize_registry',
    handler_count: String(handlerCount),
  });

  if (handlerCount > 0) {
    log.info(`Registered handlers: ${registry.listHandlers().join(', ')}`, {
      operation: 'list_handlers',
    });
  }

  return handlerCount;
}

/**
 * Main server function.
 */
async function main(): Promise<number> {
  showBanner();

  // Track shutdown state
  let shutdownRequested = false;
  let shutdownResolver: (() => void) | null = null;

  const shutdownPromise = new Promise<void>((resolve) => {
    shutdownResolver = resolve;
  });

  // Signal handlers
  const handleShutdown = (signal: string): void => {
    log.info(`Received ${signal} signal, initiating shutdown...`, {
      operation: 'shutdown',
      signal,
    });
    shutdownRequested = true;
    if (shutdownResolver) {
      shutdownResolver();
    }
  };

  const handleStatus = (): void => {
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
  };

  // Set up signal handlers
  process.on('SIGTERM', () => handleShutdown('SIGTERM'));
  process.on('SIGINT', () => handleShutdown('SIGINT'));
  process.on('SIGUSR1', handleStatus);

  // Components for cleanup
  let eventPoller: EventPoller | null = null;
  let stepSubscriber: StepExecutionSubscriber | null = null;

  try {
    // Find and load FFI library
    log.info('Loading FFI library...', { operation: 'load_ffi' });
    const libraryPath = findLibraryPath();

    if (!libraryPath) {
      log.error('FFI library not found. Build with: cargo build -p tasker-worker-ts --release', {
        operation: 'load_ffi',
      });
      return 2;
    }

    log.info(`FFI library found: ${libraryPath}`, { operation: 'load_ffi' });
    setFfiLibraryPath(libraryPath);

    const runtime = getTaskerRuntime();
    await runtime.load(libraryPath);

    log.info(`FFI library loaded successfully (version: ${getVersion()})`, {
      operation: 'load_ffi',
    });

    // Initialize handler registry
    log.info('Discovering TypeScript handlers...', { operation: 'discover_handlers' });
    const handlerCount = initializeHandlerRegistry();

    if (handlerCount === 0) {
      log.warn('No handlers registered. Did you register handlers before starting?', {
        operation: 'discover_handlers',
      });
    }

    // Create bootstrap configuration
    const config: BootstrapConfig = {
      namespace: process.env.TASKER_NAMESPACE || 'default',
      logLevel: (process.env.RUST_LOG as BootstrapConfig['logLevel']) || 'info',
      configPath: process.env.TASKER_CONFIG_PATH,
      databaseUrl: process.env.DATABASE_URL,
    };

    // Bootstrap the Rust worker
    log.info('Bootstrapping Rust worker...', { operation: 'bootstrap' });
    const result = bootstrapWorker(config);

    if (!result.success) {
      log.error(`Failed to bootstrap worker: ${result.message}`, {
        operation: 'bootstrap',
        error_message: result.error ?? result.message,
      });
      return 3;
    }

    log.info(`Worker bootstrapped successfully (ID: ${result.workerId})`, {
      operation: 'bootstrap',
      worker_id: result.workerId ?? 'unknown',
    });

    // Initialize TypeScript event dispatch system
    log.info('Starting TypeScript event dispatch system...', {
      operation: 'start_event_system',
    });

    // 1. Get the global event emitter
    const emitter = getGlobalEmitter();
    log.info('  EventEmitter: Ready', { operation: 'start_event_system' });

    // 2. Create and start StepExecutionSubscriber
    const registry = HandlerRegistry.instance();
    stepSubscriber = new StepExecutionSubscriber(emitter, registry, {
      workerId: result.workerId ?? `typescript-worker-${process.pid}`,
      maxConcurrent: 10,
      handlerTimeoutMs: 300000, // 5 minutes
    });
    stepSubscriber.start();
    log.info(`  StepExecutionSubscriber: Started (worker_id=${result.workerId})`, {
      operation: 'start_event_system',
    });

    // 3. Create and start EventPoller
    eventPoller = createEventPoller(runtime, {
      pollIntervalMs: 10,
      starvationCheckInterval: 100,
      cleanupInterval: 1000,
      metricsInterval: 100,
    });
    eventPoller.start();
    log.info('  EventPoller: Started (10ms polling interval)', {
      operation: 'start_event_system',
    });

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

    // Main worker loop
    const sleepInterval = process.env.TASKER_ENV === 'production' ? 5000 : 1000;
    let loopCount = 0;

    const mainLoop = async (): Promise<void> => {
      while (!shutdownRequested) {
        await new Promise((resolve) => setTimeout(resolve, sleepInterval));

        if (shutdownRequested) {
          break;
        }

        // Periodic health check (every 60 iterations in development, every 300 in production)
        loopCount++;
        const healthCheckInterval = process.env.TASKER_ENV === 'production' ? 300 : 60;

        if (loopCount % healthCheckInterval === 0) {
          const healthStatus = performHealthCheck();
          if (healthStatus.healthy) {
            logDebug(`Health check #${loopCount / healthCheckInterval}: OK`, {
              component: 'server',
              operation: 'health_check',
            });
          } else {
            logWarn(
              `Health check #${loopCount / healthCheckInterval}: UNHEALTHY - ${healthStatus.error}`,
              {
                component: 'server',
                operation: 'health_check',
                error: healthStatus.error ?? 'unknown',
              }
            );
          }
        }
      }
    };

    // Race between main loop and shutdown signal
    await Promise.race([mainLoop(), shutdownPromise]);

    // Shutdown sequence
    console.log('');
    log.info('Starting shutdown sequence...', { operation: 'shutdown' });

    // 1. Stop EventPoller (no new events)
    log.info('  Stopping EventPoller...', { operation: 'shutdown' });
    if (eventPoller) {
      await eventPoller.stop();
    }
    log.info('  EventPoller stopped', { operation: 'shutdown' });

    // 2. Stop StepExecutionSubscriber (no new handler invocations)
    log.info('  Stopping StepExecutionSubscriber...', { operation: 'shutdown' });
    if (stepSubscriber) {
      stepSubscriber.stop();
      // Wait for in-flight handlers
      await stepSubscriber.waitForCompletion(5000);
    }
    log.info('  StepExecutionSubscriber stopped', { operation: 'shutdown' });

    // 3. Clear EventEmitter listeners
    log.info('  Clearing event listeners...', { operation: 'shutdown' });
    emitter.removeAllListeners();
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

    // Cleanup on error
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

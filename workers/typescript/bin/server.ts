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

import pino from 'pino';
import { ShutdownController, WorkerServer, type WorkerServerConfig } from '../src/server/index.js';

// =============================================================================
// Logging
// =============================================================================

const log = pino({
  name: 'tasker-worker',
  level: process.env.RUST_LOG ?? 'info',
  transport:
    process.env.TASKER_ENV !== 'production'
      ? {
          target: 'pino-pretty',
          options: { colorize: true },
        }
      : undefined,
});

// =============================================================================
// Health State Machine
// =============================================================================

/**
 * Health states for the worker.
 *
 * Transitions:
 * - Healthy -> Degraded: First failure
 * - Degraded -> Unhealthy: N consecutive failures
 * - Degraded -> Healthy: Recovery (first success after failures)
 * - Unhealthy -> Healthy: Recovery (first success after errors)
 */
type HealthState = 'healthy' | 'degraded' | 'unhealthy';

interface HealthTracker {
  state: HealthState;
  consecutiveFailures: number;
  totalFailures: number;
  lastError: string | null;
}

/** Number of consecutive failures before escalating from WARN to ERROR */
const UNHEALTHY_THRESHOLD = 3;

function createHealthTracker(): HealthTracker {
  return {
    state: 'healthy',
    consecutiveFailures: 0,
    totalFailures: 0,
    lastError: null,
  };
}

function updateHealthState(tracker: HealthTracker, healthy: boolean, error?: string): void {
  const previousState = tracker.state;

  if (healthy) {
    // Recovery path
    if (previousState !== 'healthy') {
      log.info(
        {
          component: 'health',
          previousState,
          consecutiveFailures: tracker.consecutiveFailures,
          totalFailures: tracker.totalFailures,
        },
        'Worker health recovered'
      );
    }
    tracker.state = 'healthy';
    tracker.consecutiveFailures = 0;
    tracker.lastError = null;
  } else {
    // Failure path
    tracker.consecutiveFailures++;
    tracker.totalFailures++;
    tracker.lastError = error ?? 'Unknown error';

    if (tracker.consecutiveFailures >= UNHEALTHY_THRESHOLD) {
      tracker.state = 'unhealthy';
      log.error(
        {
          component: 'health',
          consecutiveFailures: tracker.consecutiveFailures,
          totalFailures: tracker.totalFailures,
          error: tracker.lastError,
        },
        'Worker health check failed (unhealthy)'
      );
    } else {
      tracker.state = 'degraded';
      log.warn(
        {
          component: 'health',
          consecutiveFailures: tracker.consecutiveFailures,
          totalFailures: tracker.totalFailures,
          error: tracker.lastError,
        },
        'Worker health check failed (degraded)'
      );
    }
  }
}

// =============================================================================
// Configuration
// =============================================================================

const isProduction = (): boolean => process.env.TASKER_ENV === 'production';

function getConfig(): WorkerServerConfig {
  return {
    namespace: process.env.TASKER_NAMESPACE ?? 'default',
    logLevel: (process.env.RUST_LOG as WorkerServerConfig['logLevel']) ?? 'info',
    configPath: process.env.TASKER_CONFIG_PATH,
    databaseUrl: process.env.DATABASE_URL,
  };
}

// =============================================================================
// Banner Display
// =============================================================================

function showBanner(): void {
  const banner = ['='.repeat(60), 'Starting TypeScript Worker Bootstrap System', '='.repeat(60)];

  const info = {
    environment: process.env.TASKER_ENV ?? 'development',
    runtime: process.versions.bun ? `Bun ${process.versions.bun}` : `Node.js ${process.version}`,
    pid: process.pid,
    databaseUrl: process.env.DATABASE_URL ? '[REDACTED]' : 'Not set',
    templatePath: process.env.TASKER_TEMPLATE_PATH ?? 'Not set',
    configPath: process.env.TASKER_CONFIG_PATH ?? 'Not set',
    namespace: process.env.TASKER_NAMESPACE ?? 'default',
    productionOptimizations: isProduction(),
  };

  log.info({ component: 'server', ...info }, banner.join('\n'));
}

function showSuccessBanner(): void {
  log.info(
    { component: 'server', operation: 'startup' },
    [
      '',
      '='.repeat(60),
      'TypeScript worker system started successfully',
      '  Rust foundation: Bootstrapped via FFI',
      '  TypeScript handlers: Registered and ready',
      '  Event dispatch: Active (polling -> handlers -> completion)',
      '  Worker status: Running and processing tasks',
      '',
      'Worker ready and processing tasks',
      'Press Ctrl+C to stop',
      '='.repeat(60),
    ].join('\n')
  );
}

// =============================================================================
// Main Loop
// =============================================================================

async function runMainLoop(server: WorkerServer, shutdown: ShutdownController): Promise<void> {
  const sleepInterval = isProduction() ? 5000 : 1000;
  const healthCheckInterval = isProduction() ? 300 : 60;
  const healthTracker = createHealthTracker();
  let loopCount = 0;

  while (!shutdown.isRequested && server.running()) {
    await new Promise((resolve) => setTimeout(resolve, sleepInterval));

    if (shutdown.isRequested) {
      break;
    }

    loopCount++;
    if (loopCount % healthCheckInterval === 0) {
      const healthStatus = server.healthCheck();
      updateHealthState(healthTracker, healthStatus.healthy, healthStatus.error);
    }
  }
}

// =============================================================================
// Main Entry Point
// =============================================================================

async function main(): Promise<number> {
  showBanner();

  const server = WorkerServer.instance();
  const shutdown = new ShutdownController();

  // Set up signal handlers
  process.on('SIGTERM', () => {
    log.info({ component: 'server', signal: 'SIGTERM' }, 'Received SIGTERM signal');
    shutdown.trigger('SIGTERM');
  });
  process.on('SIGINT', () => {
    log.info({ component: 'server', signal: 'SIGINT' }, 'Received SIGINT signal');
    shutdown.trigger('SIGINT');
  });
  process.on('SIGUSR1', () => {
    const status = server.status();
    log.info({ component: 'server', operation: 'status', ...status }, 'Worker status requested');
  });

  try {
    // Start the server
    await server.start(getConfig());
    showSuccessBanner();

    // Run main loop until shutdown
    await Promise.race([runMainLoop(server, shutdown), shutdown.promise]);

    // Shutdown gracefully
    await server.shutdown();

    log.info(
      { component: 'server', operation: 'shutdown' },
      'TypeScript Worker Server terminated gracefully'
    );
    return 0;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    log.error(
      {
        component: 'server',
        operation: 'startup',
        error: errorMessage,
        stack: error instanceof Error ? error.stack : undefined,
      },
      `Unexpected error: ${errorMessage}`
    );

    return 1;
  }
}

// Run the server
main()
  .then((exitCode) => {
    process.exit(exitCode);
  })
  .catch((error) => {
    log.fatal({ component: 'server', error }, 'Unhandled error');
    process.exit(2);
  });

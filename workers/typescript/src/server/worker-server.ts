/**
 * WorkerServer - Orchestrates the TypeScript worker lifecycle.
 *
 * Manages the complete lifecycle of a TypeScript worker:
 * - FFI library loading (via FfiLayer)
 * - Handler registration (via HandlerSystem)
 * - Rust worker bootstrapping
 * - Event processing (via EventSystem)
 * - Graceful shutdown
 *
 * Design principles:
 * - Explicit construction: No singleton pattern - caller creates and manages
 * - Clear ownership: Owns FfiLayer, HandlerSystem, EventSystem
 * - Explicit lifecycle: Clear 3-phase startup and shutdown
 *
 * @example
 * ```typescript
 * const server = new WorkerServer();
 *
 * await server.start({
 *   namespace: 'payments',
 *   logLevel: 'debug',
 * });
 *
 * // Server is now running and processing tasks
 *
 * await server.shutdown();
 * ```
 */

import {
  bootstrapWorker,
  healthCheck as ffiHealthCheck,
  getVersion,
  getWorkerStatus,
  isWorkerRunning,
  stopWorker,
  transitionToGracefulShutdown,
} from '../bootstrap/bootstrap.js';
import type { BootstrapConfig, BootstrapResult } from '../bootstrap/types.js';
import { EventSystem, type EventSystemConfig } from '../events/event-system.js';
import { FfiLayer, type FfiLayerConfig } from '../ffi/ffi-layer.js';
import { HandlerSystem } from '../handler/handler-system.js';
import { createLogger, setLoggingRuntime } from '../logging/index.js';
import {
  type HealthCheckResult,
  type ServerState,
  ServerStates,
  type ServerStatus,
  type WorkerServerConfig,
} from './types.js';

const log = createLogger({ component: 'server' });

/**
 * Worker server class.
 *
 * Unlike the previous singleton pattern, this class is instantiated directly
 * by the caller (typically bin/server.ts). This provides explicit lifecycle
 * control and clear dependency ownership.
 */
export class WorkerServer {
  private readonly ffiLayer: FfiLayer;
  private readonly handlerSystem: HandlerSystem;
  private eventSystem: EventSystem | null = null;

  private state: ServerState = ServerStates.INITIALIZED;
  private config: WorkerServerConfig | null = null;
  private workerId: string | null = null;
  private startTime: number | null = null;
  private shutdownHandlers: Array<() => void | Promise<void>> = [];

  /**
   * Create a new WorkerServer.
   *
   * @param ffiConfig - Optional FFI layer configuration
   */
  constructor(ffiConfig?: FfiLayerConfig) {
    this.ffiLayer = new FfiLayer(ffiConfig);
    this.handlerSystem = new HandlerSystem();
  }

  // ==========================================================================
  // Properties
  // ==========================================================================

  /**
   * Get the current server state.
   */
  getState(): ServerState {
    return this.state;
  }

  /**
   * Get the worker identifier.
   */
  getWorkerId(): string | null {
    return this.workerId;
  }

  /**
   * Check if the server is currently running.
   */
  isRunning(): boolean {
    return this.state === ServerStates.RUNNING;
  }

  /**
   * Get the handler system for external handler registration.
   */
  getHandlerSystem(): HandlerSystem {
    return this.handlerSystem;
  }

  /**
   * Get the event system (available after start).
   */
  getEventSystem(): EventSystem | null {
    return this.eventSystem;
  }

  // ==========================================================================
  // Lifecycle Methods
  // ==========================================================================

  /**
   * Start the worker server.
   *
   * Three-phase startup:
   * 1. Initialize: Load FFI, load handlers
   * 2. Bootstrap: Initialize Rust worker
   * 3. Start: Begin event processing
   *
   * @param config - Optional server configuration
   * @returns The server instance for chaining
   * @throws Error if server is already running or fails to start
   */
  async start(config?: WorkerServerConfig): Promise<this> {
    if (this.state === ServerStates.RUNNING) {
      throw new Error('WorkerServer is already running');
    }

    if (this.state === ServerStates.STARTING) {
      throw new Error('WorkerServer is already starting');
    }

    this.state = ServerStates.STARTING;
    this.config = config ?? {};
    this.startTime = Date.now();

    try {
      // Phase 1: Initialize
      await this.initializePhase();

      // Phase 2: Bootstrap Rust worker
      const bootstrapResult = await this.bootstrapPhase();
      this.workerId = bootstrapResult.workerId ?? `typescript-worker-${process.pid}`;

      log.info(`Worker bootstrapped successfully (ID: ${this.workerId})`, {
        operation: 'bootstrap',
        worker_id: this.workerId,
      });

      // Phase 3: Start event processing
      await this.startEventProcessingPhase();

      this.state = ServerStates.RUNNING;

      log.info('WorkerServer started successfully', {
        operation: 'start',
        worker_id: this.workerId,
        version: getVersion(this.ffiLayer.getRuntime()),
      });

      return this;
    } catch (error) {
      this.state = ServerStates.ERROR;
      const errorMessage = error instanceof Error ? error.message : String(error);

      log.error(`WorkerServer failed to start: ${errorMessage}`, {
        operation: 'start',
        error_message: errorMessage,
      });

      // Cleanup any partially initialized components
      await this.cleanupOnError();

      throw error;
    }
  }

  /**
   * Shutdown the worker server gracefully.
   *
   * Three-phase shutdown:
   * 1. Stop event processing
   * 2. Stop Rust worker
   * 3. Unload FFI
   */
  async shutdown(): Promise<void> {
    if (this.state === ServerStates.SHUTTING_DOWN) {
      log.warn('Shutdown already in progress', { operation: 'shutdown' });
      return;
    }

    if (this.state !== ServerStates.RUNNING) {
      log.info('Server not running, nothing to shutdown', {
        operation: 'shutdown',
        state: this.state,
      });
      return;
    }

    this.state = ServerStates.SHUTTING_DOWN;

    log.info('Starting shutdown sequence...', { operation: 'shutdown' });

    try {
      // Phase 1: Stop event processing
      if (this.eventSystem) {
        log.info('  Stopping event system...', { operation: 'shutdown' });
        await this.eventSystem.stop();
        this.eventSystem = null;
        log.info('  Event system stopped', { operation: 'shutdown' });
      }

      // Phase 2: Stop Rust worker
      const runtime = this.ffiLayer.isLoaded() ? this.ffiLayer.getRuntime() : undefined;
      if (isWorkerRunning(runtime)) {
        log.info('  Transitioning to graceful shutdown...', {
          operation: 'shutdown',
        });
        transitionToGracefulShutdown(runtime);

        log.info('  Stopping Rust worker...', { operation: 'shutdown' });
        stopWorker(runtime);
        log.info('  Rust worker stopped', { operation: 'shutdown' });
      }

      // Phase 3: Unload FFI
      log.info('  Unloading FFI...', { operation: 'shutdown' });
      await this.ffiLayer.unload();
      log.info('  FFI unloaded', { operation: 'shutdown' });

      // Execute registered shutdown handlers
      for (const handler of this.shutdownHandlers) {
        try {
          await handler();
        } catch (error) {
          log.error(
            `Shutdown handler failed: ${error instanceof Error ? error.message : String(error)}`,
            {
              operation: 'shutdown',
            }
          );
        }
      }

      this.state = ServerStates.STOPPED;

      log.info('WorkerServer shutdown completed successfully', {
        operation: 'shutdown',
      });
    } catch (error) {
      this.state = ServerStates.ERROR;
      const errorMessage = error instanceof Error ? error.message : String(error);

      log.error(`Shutdown failed: ${errorMessage}`, {
        operation: 'shutdown',
        error_message: errorMessage,
      });

      throw error;
    }
  }

  /**
   * Register a handler to be called during shutdown.
   *
   * @param handler - Function to execute during shutdown
   */
  onShutdown(handler: () => void | Promise<void>): void {
    this.shutdownHandlers.push(handler);
  }

  // ==========================================================================
  // Status Methods
  // ==========================================================================

  /**
   * Perform a health check on the worker.
   */
  healthCheck(): HealthCheckResult {
    if (this.state !== ServerStates.RUNNING) {
      return {
        healthy: false,
        error: `Server not running (state: ${this.state})`,
      };
    }

    try {
      const runtime = this.ffiLayer.isLoaded() ? this.ffiLayer.getRuntime() : undefined;
      const ffiHealthy = ffiHealthCheck(runtime);
      if (!ffiHealthy) {
        return { healthy: false, error: 'FFI health check failed' };
      }

      const workerStatus = getWorkerStatus(runtime);
      if (!workerStatus.running) {
        return { healthy: false, error: 'Worker not running' };
      }

      return {
        healthy: true,
        status: this.status(),
      };
    } catch (error) {
      return {
        healthy: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  /**
   * Get detailed server status.
   */
  status(): ServerStatus {
    const stats = this.eventSystem?.getStats();

    return {
      state: this.state,
      workerId: this.workerId,
      running: this.state === ServerStates.RUNNING,
      processedCount: stats?.processedCount ?? 0,
      errorCount: stats?.errorCount ?? 0,
      activeHandlers: stats?.activeHandlers ?? 0,
      uptimeMs: this.startTime ? Date.now() - this.startTime : 0,
    };
  }

  // ==========================================================================
  // Private Phase Methods
  // ==========================================================================

  /**
   * Phase 1: Initialize FFI and handlers.
   */
  private async initializePhase(): Promise<void> {
    // Load handlers from environment
    const handlerCount = await this.handlerSystem.loadFromEnv();
    log.info(`Loaded ${handlerCount} handlers from TYPESCRIPT_HANDLER_PATH`, {
      operation: 'initialize',
    });

    // Load FFI library
    log.info('Loading FFI library...', { operation: 'initialize' });
    await this.ffiLayer.load(this.config?.libraryPath);
    log.info(`FFI library loaded: ${this.ffiLayer.getLibraryPath()}`, {
      operation: 'initialize',
    });

    // Install the runtime for logging (enables Rust tracing integration)
    setLoggingRuntime(this.ffiLayer.getRuntime());

    // Log handler info
    const totalHandlers = this.handlerSystem.handlerCount();
    if (totalHandlers > 0) {
      log.info(`Handler registry: ${totalHandlers} handlers registered`, {
        operation: 'initialize',
        handler_count: String(totalHandlers),
      });
      log.info(`Registered handlers: ${this.handlerSystem.listHandlers().join(', ')}`, {
        operation: 'initialize',
      });
    } else {
      log.warn('No handlers registered. Did you register handlers before starting?', {
        operation: 'initialize',
      });
    }
  }

  /**
   * Phase 2: Bootstrap Rust worker.
   */
  private async bootstrapPhase(): Promise<BootstrapResult> {
    // Build config, only setting properties that have values
    const bootstrapConfig: BootstrapConfig = {
      namespace: this.config?.namespace ?? process.env.TASKER_NAMESPACE ?? 'default',
      logLevel:
        this.config?.logLevel ?? (process.env.RUST_LOG as BootstrapConfig['logLevel']) ?? 'info',
    };

    const configPath = this.config?.configPath ?? process.env.TASKER_CONFIG_PATH;
    if (configPath) {
      bootstrapConfig.configPath = configPath;
    }

    const databaseUrl = this.config?.databaseUrl ?? process.env.DATABASE_URL;
    if (databaseUrl) {
      bootstrapConfig.databaseUrl = databaseUrl;
    }

    log.info('Bootstrapping Rust worker...', { operation: 'bootstrap' });
    const runtime = this.ffiLayer.getRuntime();
    const result = await bootstrapWorker(bootstrapConfig, runtime);

    if (!result.success) {
      throw new Error(`Bootstrap failed: ${result.message}`);
    }

    return result;
  }

  /**
   * Phase 3: Start event processing.
   */
  private async startEventProcessingPhase(): Promise<void> {
    log.info('Starting event processing system...', {
      operation: 'start_events',
    });

    const runtime = this.ffiLayer.getRuntime();

    // Build event system config
    const eventConfig: EventSystemConfig = {
      poller: {
        pollIntervalMs: this.config?.pollIntervalMs ?? 10,
        starvationCheckInterval: this.config?.starvationCheckInterval ?? 100,
        cleanupInterval: this.config?.cleanupInterval ?? 1000,
        metricsInterval: this.config?.metricsInterval ?? 100,
      },
      subscriber: {
        maxConcurrent: this.config?.maxConcurrentHandlers ?? 10,
        handlerTimeoutMs: this.config?.handlerTimeoutMs ?? 300000,
      },
    };

    // Only add workerId if it's set (exactOptionalPropertyTypes compatibility)
    if (this.workerId) {
      eventConfig.subscriber = {
        ...eventConfig.subscriber,
        workerId: this.workerId,
      };
    }

    // Create EventSystem with explicit dependencies
    this.eventSystem = new EventSystem(runtime, this.handlerSystem.getRegistry(), eventConfig);

    // Start the event system
    this.eventSystem.start();

    log.info('Event processing system started', {
      operation: 'start_events',
      worker_id: this.workerId,
    });
  }

  /**
   * Cleanup on error during startup.
   */
  private async cleanupOnError(): Promise<void> {
    try {
      if (this.eventSystem) {
        await this.eventSystem.stop();
        this.eventSystem = null;
      }
      if (isWorkerRunning()) {
        stopWorker();
      }
      await this.ffiLayer.unload();
    } catch {
      // Ignore cleanup errors
    }
  }
}

// ==========================================================================
// Legacy Compatibility (Deprecated)
// ==========================================================================

/**
 * @deprecated Use `new WorkerServer()` instead. Will be removed in future version.
 *
 * This function provides backwards compatibility during migration.
 * New code should create WorkerServer directly.
 */
export function createWorkerServer(ffiConfig?: FfiLayerConfig): WorkerServer {
  return new WorkerServer(ffiConfig);
}

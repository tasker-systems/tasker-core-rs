/**
 * Worker server for TypeScript workers.
 *
 * Provides a singleton class for managing the complete worker lifecycle:
 * - FFI library loading and runtime initialization
 * - Handler registry setup
 * - Rust worker bootstrapping
 * - Event polling and step execution
 * - Graceful shutdown
 *
 * Matches Ruby's Bootstrap pattern (TAS-92 aligned).
 *
 * @example
 * ```typescript
 * const server = WorkerServer.instance();
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
import { getGlobalEmitter, type TaskerEventEmitter } from '../events/event-emitter.js';
import { createEventPoller, type EventPoller } from '../events/event-poller.js';
import { RuntimeFactory } from '../ffi/runtime-factory.js';
import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import { HandlerRegistry } from '../handler/registry.js';
import { createLogger } from '../logging/index.js';
import { StepExecutionSubscriber } from '../subscriber/step-execution-subscriber.js';
import type {
  HealthCheckResult,
  ServerComponents,
  ServerState,
  ServerStatus,
  WorkerServerConfig,
} from './types.js';

const log = createLogger({ component: 'server' });

/**
 * Singleton worker server class.
 *
 * Manages the complete lifecycle of a TypeScript worker including
 * FFI initialization, handler registration, and event processing.
 */
export class WorkerServer {
  private static _instance: WorkerServer | null = null;

  private _state: ServerState = 'initialized';
  private _config: WorkerServerConfig | null = null;
  private _components: ServerComponents | null = null;
  private _startTime: number | null = null;
  private _shutdownHandlers: Array<() => void | Promise<void>> = [];

  private constructor() {}

  // ==========================================================================
  // Singleton Access
  // ==========================================================================

  /**
   * Get the singleton WorkerServer instance.
   */
  static instance(): WorkerServer {
    if (!WorkerServer._instance) {
      WorkerServer._instance = new WorkerServer();
    }
    return WorkerServer._instance;
  }

  /**
   * Reset the singleton instance.
   *
   * Primarily for testing. Will stop the server if running.
   */
  static async resetInstance(): Promise<void> {
    if (WorkerServer._instance) {
      if (WorkerServer._instance._state === 'running') {
        await WorkerServer._instance.shutdown();
      }
      WorkerServer._instance = null;
    }
  }

  // ==========================================================================
  // Properties
  // ==========================================================================

  /**
   * Get the current server state.
   */
  get state(): ServerState {
    return this._state;
  }

  /**
   * Get the worker identifier.
   */
  get workerId(): string | null {
    return this._components?.workerId ?? null;
  }

  /**
   * Check if the server is currently running.
   */
  running(): boolean {
    return this._state === 'running';
  }

  // ==========================================================================
  // Lifecycle Methods
  // ==========================================================================

  /**
   * Start the worker server.
   *
   * This method:
   * 1. Loads the FFI library
   * 2. Initializes the handler registry
   * 3. Bootstraps the Rust worker
   * 4. Starts the event processing system
   *
   * @param config - Optional server configuration
   * @returns The server instance for chaining
   * @throws Error if server is already running or fails to start
   */
  async start(config?: WorkerServerConfig): Promise<this> {
    if (this._state === 'running') {
      throw new Error('WorkerServer is already running');
    }

    if (this._state === 'starting') {
      throw new Error('WorkerServer is already starting');
    }

    this._state = 'starting';
    this._config = config ?? {};
    this._startTime = Date.now();

    try {
      // 1. Load FFI library
      const runtime = await this.loadFfiLibrary();

      // 2. Initialize handler registry
      const registry = this.initializeHandlerRegistry();

      // 3. Bootstrap Rust worker
      const bootstrapResult = await this.bootstrapRustWorker();
      if (!bootstrapResult.success) {
        throw new Error(`Bootstrap failed: ${bootstrapResult.message}`);
      }

      const workerId = bootstrapResult.workerId ?? `typescript-worker-${process.pid}`;
      log.info(`Worker bootstrapped successfully (ID: ${workerId})`, {
        operation: 'bootstrap',
        worker_id: workerId,
      });

      // 4. Start event processing
      const eventComponents = this.startEventSystem(runtime, registry, workerId);

      // Store components
      this._components = {
        runtime,
        registry,
        workerId,
        ...eventComponents,
      };

      this._state = 'running';

      log.info('WorkerServer started successfully', {
        operation: 'start',
        worker_id: workerId,
        version: getVersion(),
      });

      return this;
    } catch (error) {
      this._state = 'error';
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
   * This method:
   * 1. Stops the event poller
   * 2. Waits for in-flight handlers to complete
   * 3. Clears event listeners
   * 4. Stops the Rust worker
   * 5. Executes registered shutdown handlers
   */
  async shutdown(): Promise<void> {
    if (this._state === 'shutting_down') {
      log.warn('Shutdown already in progress', { operation: 'shutdown' });
      return;
    }

    if (this._state !== 'running') {
      log.info('Server not running, nothing to shutdown', {
        operation: 'shutdown',
        state: this._state,
      });
      return;
    }

    this._state = 'shutting_down';

    log.info('Starting shutdown sequence...', { operation: 'shutdown' });

    try {
      const components = this._components;
      if (components) {
        // 1. Stop EventPoller (no new events)
        log.info('  Stopping EventPoller...', { operation: 'shutdown' });
        await components.eventPoller.stop();
        log.info('  EventPoller stopped', { operation: 'shutdown' });

        // 2. Stop StepExecutionSubscriber (no new handler invocations)
        log.info('  Stopping StepExecutionSubscriber...', {
          operation: 'shutdown',
        });
        components.stepSubscriber.stop();
        await components.stepSubscriber.waitForCompletion(5000);
        log.info('  StepExecutionSubscriber stopped', {
          operation: 'shutdown',
        });

        // 3. Clear EventEmitter listeners
        log.info('  Clearing event listeners...', { operation: 'shutdown' });
        components.emitter.removeAllListeners();
        log.info('  Event listeners cleared', { operation: 'shutdown' });

        // 4. Transition to graceful shutdown and stop worker
        if (isWorkerRunning()) {
          log.info('  Transitioning to graceful shutdown...', {
            operation: 'shutdown',
          });
          transitionToGracefulShutdown();

          log.info('  Stopping Rust worker...', { operation: 'shutdown' });
          stopWorker();
          log.info('  Rust worker stopped', { operation: 'shutdown' });
        }
      }

      // 5. Execute registered shutdown handlers
      for (const handler of this._shutdownHandlers) {
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

      this._components = null;
      this._state = 'stopped';

      log.info('WorkerServer shutdown completed successfully', {
        operation: 'shutdown',
      });
    } catch (error) {
      this._state = 'error';
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
    this._shutdownHandlers.push(handler);
  }

  // ==========================================================================
  // Status Methods
  // ==========================================================================

  /**
   * Perform a health check on the worker.
   */
  healthCheck(): HealthCheckResult {
    if (this._state !== 'running') {
      return {
        healthy: false,
        error: `Server not running (state: ${this._state})`,
      };
    }

    try {
      const ffiHealthy = ffiHealthCheck();
      if (!ffiHealthy) {
        return { healthy: false, error: 'FFI health check failed' };
      }

      const workerStatus = getWorkerStatus();
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
    const subscriber = this._components?.stepSubscriber;

    return {
      state: this._state,
      workerId: this._components?.workerId ?? null,
      running: this._state === 'running',
      processedCount: subscriber?.getProcessedCount() ?? 0,
      errorCount: subscriber?.getErrorCount() ?? 0,
      activeHandlers: subscriber?.getActiveHandlers() ?? 0,
      uptimeMs: this._startTime ? Date.now() - this._startTime : 0,
    };
  }

  // ==========================================================================
  // Private Initialization Methods
  // ==========================================================================

  /**
   * Load FFI library and get runtime.
   */
  private async loadFfiLibrary(): Promise<TaskerRuntime> {
    log.info('Loading FFI library...', { operation: 'load_ffi' });

    const factory = RuntimeFactory.instance();

    try {
      const libraryPath = await factory.loadLibrary();
      log.info(`FFI library loaded: ${libraryPath}`, { operation: 'load_ffi' });

      const runtime = factory.getLoadedRuntime();
      log.info(`FFI library loaded successfully (version: ${getVersion()})`, {
        operation: 'load_ffi',
      });

      return runtime;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      log.error(
        `FFI library not found. Build with: cargo build -p tasker-worker-ts --release. Error: ${errorMessage}`,
        { operation: 'load_ffi' }
      );
      throw new Error(`Failed to load FFI library: ${errorMessage}`);
    }
  }

  /**
   * Initialize the handler registry.
   */
  private initializeHandlerRegistry(): HandlerRegistry {
    log.info('Discovering TypeScript handlers...', {
      operation: 'discover_handlers',
    });

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

    return registry;
  }

  /**
   * Bootstrap the Rust worker.
   */
  private async bootstrapRustWorker(): Promise<BootstrapResult> {
    // Build config, only setting properties that have values
    // (required for exactOptionalPropertyTypes: true)
    const config: BootstrapConfig = {
      namespace: this._config?.namespace ?? process.env.TASKER_NAMESPACE ?? 'default',
      logLevel:
        this._config?.logLevel ?? (process.env.RUST_LOG as BootstrapConfig['logLevel']) ?? 'info',
    };

    const configPath = this._config?.configPath ?? process.env.TASKER_CONFIG_PATH;
    if (configPath) {
      config.configPath = configPath;
    }

    const databaseUrl = this._config?.databaseUrl ?? process.env.DATABASE_URL;
    if (databaseUrl) {
      config.databaseUrl = databaseUrl;
    }

    log.info('Bootstrapping Rust worker...', { operation: 'bootstrap' });
    return bootstrapWorker(config);
  }

  /**
   * Start the event processing system.
   */
  private startEventSystem(
    runtime: TaskerRuntime,
    registry: HandlerRegistry,
    workerId: string
  ): {
    emitter: TaskerEventEmitter;
    eventPoller: EventPoller;
    stepSubscriber: StepExecutionSubscriber;
  } {
    log.info('Starting TypeScript event dispatch system...', {
      operation: 'start_event_system',
    });

    // 1. Get the global event emitter
    const emitter = getGlobalEmitter();
    log.info('  EventEmitter: Ready', { operation: 'start_event_system' });

    // 2. Create and start StepExecutionSubscriber
    const stepSubscriber = new StepExecutionSubscriber(emitter, registry, {
      workerId,
      maxConcurrent: this._config?.maxConcurrentHandlers ?? 10,
      handlerTimeoutMs: this._config?.handlerTimeoutMs ?? 300000,
    });
    stepSubscriber.start();
    log.info(`  StepExecutionSubscriber: Started (worker_id=${workerId})`, {
      operation: 'start_event_system',
    });

    // 3. Create and start EventPoller
    const eventPoller = createEventPoller(runtime, {
      pollIntervalMs: this._config?.pollIntervalMs ?? 10,
      starvationCheckInterval: this._config?.starvationCheckInterval ?? 100,
      cleanupInterval: this._config?.cleanupInterval ?? 1000,
      metricsInterval: this._config?.metricsInterval ?? 100,
    });
    eventPoller.start();
    log.info(`  EventPoller: Started (${this._config?.pollIntervalMs ?? 10}ms polling interval)`, {
      operation: 'start_event_system',
    });

    return { emitter, eventPoller, stepSubscriber };
  }

  /**
   * Cleanup on error during startup.
   */
  private async cleanupOnError(): Promise<void> {
    try {
      const components = this._components;
      if (components) {
        await components.eventPoller?.stop();
        components.stepSubscriber?.stop();
      }
      if (isWorkerRunning()) {
        stopWorker();
      }
    } catch {
      // Ignore cleanup errors
    }
    this._components = null;
  }
}

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

import { existsSync } from 'node:fs';
import { readdir } from 'node:fs/promises';
import { join } from 'node:path';
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
import type { StepHandlerClass } from '../handler/base.js';
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
   * 1. Imports handlers from TYPESCRIPT_HANDLER_PATH (if set)
   * 2. Loads the FFI library
   * 3. Initializes the handler registry
   * 4. Bootstraps the Rust worker
   * 5. Starts the event processing system
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
      // 1. Import handlers from TYPESCRIPT_HANDLER_PATH (if set)
      await this.importHandlersFromPath();

      // 2. Load FFI library
      const runtime = await this.loadFfiLibrary();

      // 3. Initialize handler registry
      const registry = this.initializeHandlerRegistry();

      // 4. Bootstrap Rust worker
      const bootstrapResult = await this.bootstrapRustWorker();
      if (!bootstrapResult.success) {
        throw new Error(`Bootstrap failed: ${bootstrapResult.message}`);
      }

      const workerId = bootstrapResult.workerId ?? `typescript-worker-${process.pid}`;
      log.info(`Worker bootstrapped successfully (ID: ${workerId})`, {
        operation: 'bootstrap',
        worker_id: workerId,
      });

      // 5. Start event processing
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
   * Import handlers from TYPESCRIPT_HANDLER_PATH environment variable.
   *
   * Similar to Python's PYTHON_HANDLER_PATH and Ruby's RUBY_HANDLER_PATH,
   * this method dynamically imports handler modules from a specified directory
   * and registers them with the HandlerRegistry.
   *
   * The handler path should contain an index.ts/index.js that exports
   * ALL_EXAMPLE_HANDLERS (array of handler classes) or individual handler exports.
   */
  private async importHandlersFromPath(): Promise<void> {
    const handlerPath = process.env.TYPESCRIPT_HANDLER_PATH;
    if (!handlerPath) {
      log.debug('TYPESCRIPT_HANDLER_PATH not set, skipping handler import', {
        operation: 'import_handlers',
      });
      return;
    }

    if (!existsSync(handlerPath)) {
      log.warn(`TYPESCRIPT_HANDLER_PATH does not exist: ${handlerPath}`, {
        operation: 'import_handlers',
      });
      return;
    }

    log.info(`Importing handlers from TYPESCRIPT_HANDLER_PATH: ${handlerPath}`, {
      operation: 'import_handlers',
    });

    try {
      const importedCount = await this.importHandlersFromDirectory(handlerPath);
      log.info(`Handler import complete: ${importedCount} handlers registered`, {
        operation: 'import_handlers',
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      log.error(`Failed to import handlers from path: ${errorMessage}`, {
        operation: 'import_handlers',
        error_message: errorMessage,
      });
    }
  }

  /**
   * Import handlers from a directory, trying index files first.
   */
  private async importHandlersFromDirectory(handlerPath: string): Promise<number> {
    const registry = HandlerRegistry.instance();

    // Try to find and import an index file
    const indexResult = await this.tryImportIndexFile(handlerPath);
    if (indexResult.module) {
      return this.registerHandlersFromModule(indexResult.module, indexResult.path, registry);
    }

    // Fallback: scan for handler files
    log.warn('No index.ts/js found in handler path, scanning for handler files...', {
      operation: 'import_handlers',
    });
    return this.scanAndImportHandlers(handlerPath, registry);
  }

  /**
   * Try to import an index file from the handler path.
   */
  private async tryImportIndexFile(
    handlerPath: string
  ): Promise<{ module: Record<string, unknown> | null; path: string | null }> {
    const indexPaths = [
      join(handlerPath, 'examples', 'index.js'),
      join(handlerPath, 'examples', 'index.ts'),
      join(handlerPath, 'index.js'),
      join(handlerPath, 'index.ts'),
    ];

    for (const indexPath of indexPaths) {
      if (existsSync(indexPath)) {
        const module = (await import(`file://${indexPath}`)) as Record<string, unknown>;
        return { module, path: indexPath };
      }
    }

    return { module: null, path: null };
  }

  /**
   * Register handlers from a module's exports.
   */
  private registerHandlersFromModule(
    module: Record<string, unknown>,
    importPath: string | null,
    registry: HandlerRegistry
  ): number {
    log.info(`Loaded handler module from: ${importPath}`, { operation: 'import_handlers' });

    // Check for ALL_EXAMPLE_HANDLERS array (preferred pattern)
    if (Array.isArray(module.ALL_EXAMPLE_HANDLERS)) {
      return this.registerFromHandlerArray(module.ALL_EXAMPLE_HANDLERS, registry);
    }

    // Fallback: scan module exports for handler classes
    return this.registerFromModuleExports(module, registry);
  }

  /**
   * Register handlers from ALL_EXAMPLE_HANDLERS array.
   */
  private registerFromHandlerArray(handlers: unknown[], registry: HandlerRegistry): number {
    let count = 0;
    for (const handlerClass of handlers) {
      if (this.isValidHandlerClass(handlerClass)) {
        registry.register(handlerClass.handlerName, handlerClass);
        count++;
      }
    }
    log.info(`Registered ${count} handlers from ALL_EXAMPLE_HANDLERS`, {
      operation: 'import_handlers',
    });
    return count;
  }

  /**
   * Register handlers from module exports.
   */
  private registerFromModuleExports(
    module: Record<string, unknown>,
    registry: HandlerRegistry
  ): number {
    let count = 0;
    for (const [exportName, exported] of Object.entries(module)) {
      if (this.isValidHandlerClass(exported)) {
        registry.register(exported.handlerName, exported);
        count++;
        log.debug(`Registered handler from export: ${exportName}`, {
          operation: 'import_handlers',
        });
      }
    }
    log.info(`Registered ${count} handlers from module exports`, {
      operation: 'import_handlers',
    });
    return count;
  }

  /**
   * Check if a value is a valid handler class.
   */
  private isValidHandlerClass(value: unknown): value is StepHandlerClass {
    return (
      value !== null &&
      typeof value === 'function' &&
      'handlerName' in value &&
      typeof (value as StepHandlerClass).handlerName === 'string'
    );
  }

  /**
   * Recursively scan a directory for handler files and import them.
   */
  private async scanAndImportHandlers(dirPath: string, registry: HandlerRegistry): Promise<number> {
    let count = 0;

    try {
      const entries = await readdir(dirPath, { withFileTypes: true });

      for (const entry of entries) {
        const fullPath = join(dirPath, entry.name);
        count += await this.processDirectoryEntry(entry, fullPath, registry);
      }
    } catch (error) {
      log.debug(`Could not scan ${dirPath}: ${error}`, { operation: 'import_handlers' });
    }

    return count;
  }

  /**
   * Process a single directory entry for handler import.
   */
  private async processDirectoryEntry(
    entry: { isDirectory(): boolean; isFile(): boolean; name: string },
    fullPath: string,
    registry: HandlerRegistry
  ): Promise<number> {
    if (this.shouldScanDirectory(entry)) {
      return this.scanAndImportHandlers(fullPath, registry);
    }

    if (this.isHandlerFile(entry)) {
      return this.importHandlerFile(fullPath, registry);
    }

    return 0;
  }

  /**
   * Check if a directory should be scanned for handlers.
   */
  private shouldScanDirectory(entry: { isDirectory(): boolean; name: string }): boolean {
    return entry.isDirectory() && !entry.name.startsWith('_') && entry.name !== 'node_modules';
  }

  /**
   * Check if a file might contain handlers.
   */
  private isHandlerFile(entry: { isFile(): boolean; name: string }): boolean {
    const name = entry.name;
    return (
      entry.isFile() &&
      (name.endsWith('.ts') || name.endsWith('.js')) &&
      !name.startsWith('_') &&
      !name.endsWith('.d.ts') &&
      !name.endsWith('.test.ts') &&
      !name.endsWith('.spec.ts')
    );
  }

  /**
   * Import handlers from a single file.
   */
  private async importHandlerFile(fullPath: string, registry: HandlerRegistry): Promise<number> {
    let count = 0;

    try {
      const module = (await import(`file://${fullPath}`)) as Record<string, unknown>;

      for (const [, exported] of Object.entries(module)) {
        if (this.isValidHandlerClass(exported)) {
          registry.register(exported.handlerName, exported);
          count++;
        }
      }
    } catch (importError) {
      log.debug(`Could not import ${fullPath}: ${importError}`, { operation: 'import_handlers' });
    }

    return count;
  }

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

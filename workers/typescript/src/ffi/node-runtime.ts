/// <reference types="node" />
/**
 * Node.js FFI runtime adapter using koffi.
 *
 * This adapter uses the koffi package to interface with the Rust native library.
 * Koffi is a modern, actively maintained FFI library with prebuilt binaries.
 *
 * Install: npm install koffi
 */

import { BaseTaskerRuntime } from './runtime-interface.js';
import type {
  BootstrapConfig,
  BootstrapResult,
  FfiDispatchMetrics,
  FfiDomainEvent,
  FfiStepEvent,
  LogFields,
  StepExecutionResult,
  StopResult,
  WorkerStatus,
} from './types.js';

// Koffi library type
interface KoffiLib {
  get_version: () => unknown;
  get_rust_version: () => unknown;
  health_check: () => number;
  is_worker_running: () => number;
  bootstrap_worker: (configJson: string | null) => unknown;
  get_worker_status: () => unknown;
  stop_worker: () => unknown;
  transition_to_graceful_shutdown: () => unknown;
  poll_step_events: () => unknown;
  poll_in_process_events: () => unknown;
  complete_step_event: (eventId: string, resultJson: string) => number;
  get_ffi_dispatch_metrics: () => unknown;
  check_starvation_warnings: () => void;
  cleanup_timeouts: () => void;
  log_error: (message: string, fieldsJson: string | null) => void;
  log_warn: (message: string, fieldsJson: string | null) => void;
  log_info: (message: string, fieldsJson: string | null) => void;
  log_debug: (message: string, fieldsJson: string | null) => void;
  log_trace: (message: string, fieldsJson: string | null) => void;
  free_rust_string: (ptr: unknown) => void;
}

/**
 * Node.js FFI runtime implementation using koffi
 */
export class NodeRuntime extends BaseTaskerRuntime {
  readonly name = 'node';
  private lib: KoffiLib | null = null;
  // biome-ignore lint/suspicious/noExplicitAny: koffi module type
  private koffi: any = null;

  get isLoaded(): boolean {
    return this.lib !== null;
  }

  async load(libraryPath: string): Promise<void> {
    if (this.lib !== null) {
      return; // Already loaded
    }

    // Dynamically import koffi
    const koffiModule = await import('koffi');
    this.koffi = koffiModule.default ?? koffiModule;

    // Load the native library
    const lib = this.koffi.load(libraryPath);

    // Define FFI functions
    // Functions returning strings return pointers that we need to free
    this.lib = {
      get_version: lib.func('void* get_version()'),
      get_rust_version: lib.func('void* get_rust_version()'),
      health_check: lib.func('int health_check()'),
      is_worker_running: lib.func('int is_worker_running()'),
      bootstrap_worker: lib.func('void* bootstrap_worker(str)'),
      get_worker_status: lib.func('void* get_worker_status()'),
      stop_worker: lib.func('void* stop_worker()'),
      transition_to_graceful_shutdown: lib.func('void* transition_to_graceful_shutdown()'),
      poll_step_events: lib.func('void* poll_step_events()'),
      poll_in_process_events: lib.func('void* poll_in_process_events()'),
      complete_step_event: lib.func('int complete_step_event(str, str)'),
      get_ffi_dispatch_metrics: lib.func('void* get_ffi_dispatch_metrics()'),
      check_starvation_warnings: lib.func('void check_starvation_warnings()'),
      cleanup_timeouts: lib.func('void cleanup_timeouts()'),
      log_error: lib.func('void log_error(str, str)'),
      log_warn: lib.func('void log_warn(str, str)'),
      log_info: lib.func('void log_info(str, str)'),
      log_debug: lib.func('void log_debug(str, str)'),
      log_trace: lib.func('void log_trace(str, str)'),
      free_rust_string: lib.func('void free_rust_string(void*)'),
    };
  }

  unload(): void {
    this.lib = null;
    this.koffi = null;
  }

  private ensureLoaded(): KoffiLib {
    if (!this.lib) {
      throw new Error('Native library not loaded. Call load() first.');
    }
    return this.lib;
  }

  /**
   * Read a C string from a pointer and free the Rust-allocated memory.
   *
   * Uses koffi.decode with 'char' type and -1 length for null-terminated strings.
   */
  private readAndFreeRustString(ptr: unknown): string | null {
    if (!ptr) return null;
    const lib = this.ensureLoaded();

    // Decode the null-terminated C string from pointer
    // Using 'char' with -1 length reads until null terminator
    const str = this.koffi.decode(ptr, 'char', -1);

    // Free the Rust-allocated memory
    lib.free_rust_string(ptr);

    return str;
  }

  getVersion(): string {
    const lib = this.ensureLoaded();
    const ptr = lib.get_version();
    return this.readAndFreeRustString(ptr) ?? 'unknown';
  }

  getRustVersion(): string {
    const lib = this.ensureLoaded();
    const ptr = lib.get_rust_version();
    return this.readAndFreeRustString(ptr) ?? 'unknown';
  }

  healthCheck(): boolean {
    const lib = this.ensureLoaded();
    return lib.health_check() === 1;
  }

  bootstrapWorker(config?: BootstrapConfig): BootstrapResult {
    const lib = this.ensureLoaded();
    const configJson = config ? this.toJson(config) : null;
    const ptr = lib.bootstrap_worker(configJson);
    const jsonStr = this.readAndFreeRustString(ptr);

    const parsed = this.parseJson<BootstrapResult>(jsonStr);
    return (
      parsed ?? {
        success: false,
        status: 'error',
        message: 'Failed to parse bootstrap result',
        error: 'Invalid JSON response',
      }
    );
  }

  isWorkerRunning(): boolean {
    const lib = this.ensureLoaded();
    return lib.is_worker_running() === 1;
  }

  getWorkerStatus(): WorkerStatus {
    const lib = this.ensureLoaded();
    const ptr = lib.get_worker_status();
    const jsonStr = this.readAndFreeRustString(ptr);

    const parsed = this.parseJson<WorkerStatus>(jsonStr);
    return parsed ?? { success: false, running: false };
  }

  stopWorker(): StopResult {
    const lib = this.ensureLoaded();
    const ptr = lib.stop_worker();
    const jsonStr = this.readAndFreeRustString(ptr);

    const parsed = this.parseJson<StopResult>(jsonStr);
    return (
      parsed ?? {
        success: false,
        status: 'error',
        message: 'Failed to parse stop result',
        error: 'Invalid JSON response',
      }
    );
  }

  transitionToGracefulShutdown(): StopResult {
    const lib = this.ensureLoaded();
    const ptr = lib.transition_to_graceful_shutdown();
    const jsonStr = this.readAndFreeRustString(ptr);

    const parsed = this.parseJson<StopResult>(jsonStr);
    return (
      parsed ?? {
        success: false,
        status: 'error',
        message: 'Failed to parse shutdown result',
        error: 'Invalid JSON response',
      }
    );
  }

  pollStepEvents(): FfiStepEvent | null {
    const lib = this.ensureLoaded();
    const ptr = lib.poll_step_events();
    if (!ptr) return null;

    const jsonStr = this.readAndFreeRustString(ptr);
    return this.parseJson<FfiStepEvent>(jsonStr);
  }

  pollInProcessEvents(): FfiDomainEvent | null {
    const lib = this.ensureLoaded();
    const ptr = lib.poll_in_process_events();
    if (!ptr) return null;

    const jsonStr = this.readAndFreeRustString(ptr);
    return this.parseJson<FfiDomainEvent>(jsonStr);
  }

  completeStepEvent(eventId: string, result: StepExecutionResult): boolean {
    const lib = this.ensureLoaded();
    return lib.complete_step_event(eventId, this.toJson(result)) === 1;
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    const lib = this.ensureLoaded();
    const ptr = lib.get_ffi_dispatch_metrics();
    const jsonStr = this.readAndFreeRustString(ptr);

    const parsed = this.parseJson<FfiDispatchMetrics>(jsonStr);
    // Check if we got a valid metrics object (not an error response)
    if (parsed && typeof parsed.pending_count === 'number') {
      return parsed;
    }
    // Return default metrics when worker not initialized or error
    return {
      pending_count: 0,
      starvation_detected: false,
      starving_event_count: 0,
      oldest_pending_age_ms: null,
      newest_pending_age_ms: null,
      oldest_event_id: null,
    };
  }

  checkStarvationWarnings(): void {
    const lib = this.ensureLoaded();
    lib.check_starvation_warnings();
  }

  cleanupTimeouts(): void {
    const lib = this.ensureLoaded();
    lib.cleanup_timeouts();
  }

  logError(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    lib.log_error(message, fields ? this.toJson(fields) : null);
  }

  logWarn(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    lib.log_warn(message, fields ? this.toJson(fields) : null);
  }

  logInfo(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    lib.log_info(message, fields ? this.toJson(fields) : null);
  }

  logDebug(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    lib.log_debug(message, fields ? this.toJson(fields) : null);
  }

  logTrace(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    lib.log_trace(message, fields ? this.toJson(fields) : null);
  }
}

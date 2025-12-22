/**
 * Deno FFI runtime adapter using Deno.dlopen.
 *
 * This adapter uses Deno's built-in FFI to interface with the Rust native library.
 * It requires --unstable-ffi and --allow-ffi flags.
 */

import { BaseTaskerRuntime } from './runtime-interface.js';
import type {
  BootstrapConfig,
  BootstrapResult,
  FfiDispatchMetrics,
  FfiStepEvent,
  LogFields,
  StepExecutionResult,
  StopResult,
  WorkerStatus,
} from './types.js';

// Deno FFI types - using generic pointer type for build compatibility
// biome-ignore lint/suspicious/noExplicitAny: Deno global is runtime-specific
declare const Deno: any;

// Generic pointer type (Deno.PointerValue is bigint | null at runtime)
type PointerValue = bigint | null;

// FFI symbol result type
interface DenoFfiSymbols {
  get_version: { call: () => PointerValue };
  get_rust_version: { call: () => PointerValue };
  health_check: { call: () => number };
  is_worker_running: { call: () => number };
  bootstrap_worker: { call: (configJson: PointerValue) => PointerValue };
  get_worker_status: { call: () => PointerValue };
  stop_worker: { call: () => PointerValue };
  transition_to_graceful_shutdown: { call: () => PointerValue };
  poll_step_events: { call: () => PointerValue };
  complete_step_event: { call: (eventId: PointerValue, resultJson: PointerValue) => number };
  get_ffi_dispatch_metrics: { call: () => PointerValue };
  check_starvation_warnings: { call: () => void };
  cleanup_timeouts: { call: () => void };
  log_error: { call: (message: PointerValue, fieldsJson: PointerValue) => void };
  log_warn: { call: (message: PointerValue, fieldsJson: PointerValue) => void };
  log_info: { call: (message: PointerValue, fieldsJson: PointerValue) => void };
  log_debug: { call: (message: PointerValue, fieldsJson: PointerValue) => void };
  log_trace: { call: (message: PointerValue, fieldsJson: PointerValue) => void };
  free_rust_string: { call: (ptr: PointerValue) => void };
}

// Deno dynamic library handle
interface DenoFfiLibrary {
  symbols: DenoFfiSymbols;
  close(): void;
}

/**
 * Deno FFI runtime implementation using Deno.dlopen
 */
export class DenoRuntime extends BaseTaskerRuntime {
  readonly name = 'deno';
  private lib: DenoFfiLibrary | null = null;
  private encoder: TextEncoder = new TextEncoder();

  get isLoaded(): boolean {
    return this.lib !== null;
  }

  async load(libraryPath: string): Promise<void> {
    if (this.lib !== null) {
      return; // Already loaded
    }

    // Check if Deno is available
    if (typeof Deno === 'undefined') {
      throw new Error('Deno runtime not detected');
    }

    // Define FFI symbols
    this.lib = Deno.dlopen(libraryPath, {
      get_version: {
        parameters: [],
        result: 'pointer',
      },
      get_rust_version: {
        parameters: [],
        result: 'pointer',
      },
      health_check: {
        parameters: [],
        result: 'i32',
      },
      is_worker_running: {
        parameters: [],
        result: 'i32',
      },
      bootstrap_worker: {
        parameters: ['pointer'],
        result: 'pointer',
      },
      get_worker_status: {
        parameters: [],
        result: 'pointer',
      },
      stop_worker: {
        parameters: [],
        result: 'pointer',
      },
      transition_to_graceful_shutdown: {
        parameters: [],
        result: 'pointer',
      },
      poll_step_events: {
        parameters: [],
        result: 'pointer',
      },
      complete_step_event: {
        parameters: ['pointer', 'pointer'],
        result: 'i32',
      },
      get_ffi_dispatch_metrics: {
        parameters: [],
        result: 'pointer',
      },
      check_starvation_warnings: {
        parameters: [],
        result: 'void',
      },
      cleanup_timeouts: {
        parameters: [],
        result: 'void',
      },
      log_error: {
        parameters: ['pointer', 'pointer'],
        result: 'void',
      },
      log_warn: {
        parameters: ['pointer', 'pointer'],
        result: 'void',
      },
      log_info: {
        parameters: ['pointer', 'pointer'],
        result: 'void',
      },
      log_debug: {
        parameters: ['pointer', 'pointer'],
        result: 'void',
      },
      log_trace: {
        parameters: ['pointer', 'pointer'],
        result: 'void',
      },
      free_rust_string: {
        parameters: ['pointer'],
        result: 'void',
      },
    }) as DenoFfiLibrary;
  }

  unload(): void {
    if (this.lib) {
      this.lib.close();
      this.lib = null;
    }
  }

  private ensureLoaded(): DenoFfiSymbols {
    if (!this.lib) {
      throw new Error('Native library not loaded. Call load() first.');
    }
    return this.lib.symbols;
  }

  // biome-ignore lint/suspicious/noExplicitAny: Deno PointerValue type
  private toCString(str: string): any {
    // Create null-terminated C string buffer
    const bytes = this.encoder.encode(`${str}\0`);
    return Deno.UnsafePointer.of(bytes);
  }

  // biome-ignore lint/suspicious/noExplicitAny: Deno PointerValue type
  private fromCString(ptr: any): string | null {
    if (ptr === null || Deno.UnsafePointer.equals(ptr, null)) {
      return null;
    }
    // Read C string from pointer using Deno's pointer view
    const view = new Deno.UnsafePointerView(ptr);
    return view.getCString();
  }

  getVersion(): string {
    const symbols = this.ensureLoaded();
    const result = symbols.get_version.call();
    const version = this.fromCString(result) ?? 'unknown';
    if (result !== null) symbols.free_rust_string.call(result);
    return version;
  }

  getRustVersion(): string {
    const symbols = this.ensureLoaded();
    const result = symbols.get_rust_version.call();
    const version = this.fromCString(result) ?? 'unknown';
    if (result !== null) symbols.free_rust_string.call(result);
    return version;
  }

  healthCheck(): boolean {
    const symbols = this.ensureLoaded();
    return symbols.health_check.call() === 1;
  }

  bootstrapWorker(config?: BootstrapConfig): BootstrapResult {
    const symbols = this.ensureLoaded();
    const configPtr = config ? this.toCString(this.toJson(config)) : null;
    const result = symbols.bootstrap_worker.call(configPtr);
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string.call(result);

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
    const symbols = this.ensureLoaded();
    return symbols.is_worker_running.call() === 1;
  }

  getWorkerStatus(): WorkerStatus {
    const symbols = this.ensureLoaded();
    const result = symbols.get_worker_status.call();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string.call(result);

    const parsed = this.parseJson<WorkerStatus>(jsonStr);
    return parsed ?? { success: false, running: false };
  }

  stopWorker(): StopResult {
    const symbols = this.ensureLoaded();
    const result = symbols.stop_worker.call();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string.call(result);

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
    const symbols = this.ensureLoaded();
    const result = symbols.transition_to_graceful_shutdown.call();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string.call(result);

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
    const symbols = this.ensureLoaded();
    const result = symbols.poll_step_events.call();
    if (result === null || Deno.UnsafePointer.equals(result, null)) {
      return null;
    }

    const jsonStr = this.fromCString(result);
    symbols.free_rust_string.call(result);

    return this.parseJson<FfiStepEvent>(jsonStr);
  }

  completeStepEvent(eventId: string, result: StepExecutionResult): boolean {
    const symbols = this.ensureLoaded();
    const eventIdPtr = this.toCString(eventId);
    const resultJsonPtr = this.toCString(this.toJson(result));
    return symbols.complete_step_event.call(eventIdPtr, resultJsonPtr) === 1;
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    const symbols = this.ensureLoaded();
    const result = symbols.get_ffi_dispatch_metrics.call();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string.call(result);

    const parsed = this.parseJson<FfiDispatchMetrics>(jsonStr);
    return (
      parsed ?? {
        pending_count: 0,
        starvation_detected: false,
        starving_event_count: 0,
        oldest_pending_age_ms: null,
        newest_pending_age_ms: null,
      }
    );
  }

  checkStarvationWarnings(): void {
    const symbols = this.ensureLoaded();
    symbols.check_starvation_warnings.call();
  }

  cleanupTimeouts(): void {
    const symbols = this.ensureLoaded();
    symbols.cleanup_timeouts.call();
  }

  logError(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_error.call(msgPtr, fieldsPtr);
  }

  logWarn(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_warn.call(msgPtr, fieldsPtr);
  }

  logInfo(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_info.call(msgPtr, fieldsPtr);
  }

  logDebug(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_debug.call(msgPtr, fieldsPtr);
  }

  logTrace(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_trace.call(msgPtr, fieldsPtr);
  }
}

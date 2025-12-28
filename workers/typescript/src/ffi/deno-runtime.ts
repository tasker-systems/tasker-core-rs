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

// Buffer type for 'buffer' parameters (Uint8Array or null)
type BufferValue = Uint8Array | null;

// FFI symbol result type - Deno 2.x uses direct function calls
interface DenoFfiSymbols {
  get_version: () => PointerValue;
  get_rust_version: () => PointerValue;
  health_check: () => number;
  is_worker_running: () => number;
  bootstrap_worker: (configJson: BufferValue) => PointerValue;
  get_worker_status: () => PointerValue;
  stop_worker: () => PointerValue;
  transition_to_graceful_shutdown: () => PointerValue;
  poll_step_events: () => PointerValue;
  complete_step_event: (eventId: BufferValue, resultJson: BufferValue) => number;
  get_ffi_dispatch_metrics: () => PointerValue;
  check_starvation_warnings: () => void;
  cleanup_timeouts: () => void;
  log_error: (message: BufferValue, fieldsJson: BufferValue) => void;
  log_warn: (message: BufferValue, fieldsJson: BufferValue) => void;
  log_info: (message: BufferValue, fieldsJson: BufferValue) => void;
  log_debug: (message: BufferValue, fieldsJson: BufferValue) => void;
  log_trace: (message: BufferValue, fieldsJson: BufferValue) => void;
  free_rust_string: (ptr: PointerValue) => void;
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
        parameters: ['buffer'],
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
        parameters: ['buffer', 'buffer'],
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
        parameters: ['buffer', 'buffer'],
        result: 'void',
      },
      log_warn: {
        parameters: ['buffer', 'buffer'],
        result: 'void',
      },
      log_info: {
        parameters: ['buffer', 'buffer'],
        result: 'void',
      },
      log_debug: {
        parameters: ['buffer', 'buffer'],
        result: 'void',
      },
      log_trace: {
        parameters: ['buffer', 'buffer'],
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

  /**
   * Creates a null-terminated C string buffer.
   * With 'buffer' FFI type, we return Uint8Array directly.
   */
  private toCString(str: string): Uint8Array {
    return this.encoder.encode(`${str}\0`);
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
    const result = symbols.get_version();
    const version = this.fromCString(result) ?? 'unknown';
    if (result !== null) symbols.free_rust_string(result);
    return version;
  }

  getRustVersion(): string {
    const symbols = this.ensureLoaded();
    const result = symbols.get_rust_version();
    const version = this.fromCString(result) ?? 'unknown';
    if (result !== null) symbols.free_rust_string(result);
    return version;
  }

  healthCheck(): boolean {
    const symbols = this.ensureLoaded();
    return symbols.health_check() === 1;
  }

  bootstrapWorker(config?: BootstrapConfig): BootstrapResult {
    const symbols = this.ensureLoaded();
    const configBuf = config ? this.toCString(this.toJson(config)) : null;
    const result = symbols.bootstrap_worker(configBuf);
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string(result);

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
    return symbols.is_worker_running() === 1;
  }

  getWorkerStatus(): WorkerStatus {
    const symbols = this.ensureLoaded();
    const result = symbols.get_worker_status();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string(result);

    const parsed = this.parseJson<WorkerStatus>(jsonStr);
    return parsed ?? { success: false, running: false };
  }

  stopWorker(): StopResult {
    const symbols = this.ensureLoaded();
    const result = symbols.stop_worker();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string(result);

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
    const result = symbols.transition_to_graceful_shutdown();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string(result);

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
    const result = symbols.poll_step_events();
    if (result === null || Deno.UnsafePointer.equals(result, null)) {
      return null;
    }

    const jsonStr = this.fromCString(result);
    symbols.free_rust_string(result);

    return this.parseJson<FfiStepEvent>(jsonStr);
  }

  completeStepEvent(eventId: string, result: StepExecutionResult): boolean {
    const symbols = this.ensureLoaded();
    const eventIdBuf = this.toCString(eventId);
    const resultJsonBuf = this.toCString(this.toJson(result));
    return symbols.complete_step_event(eventIdBuf, resultJsonBuf) === 1;
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    const symbols = this.ensureLoaded();
    const result = symbols.get_ffi_dispatch_metrics();
    const jsonStr = this.fromCString(result);
    if (result !== null) symbols.free_rust_string(result);

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
    const symbols = this.ensureLoaded();
    symbols.check_starvation_warnings();
  }

  cleanupTimeouts(): void {
    const symbols = this.ensureLoaded();
    symbols.cleanup_timeouts();
  }

  logError(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_error(msgBuf, fieldsBuf);
  }

  logWarn(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_warn(msgBuf, fieldsBuf);
  }

  logInfo(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_info(msgBuf, fieldsBuf);
  }

  logDebug(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_debug(msgBuf, fieldsBuf);
  }

  logTrace(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    symbols.log_trace(msgBuf, fieldsBuf);
  }
}

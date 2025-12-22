/**
 * Node.js FFI runtime adapter using ffi-napi.
 *
 * This adapter uses the ffi-napi package to interface with the Rust native library.
 * It's designed to work with Node.js and provides the same interface as other runtimes.
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

// FFI type definitions for ffi-napi
interface FfiLibrary {
  get_version: () => Buffer;
  get_rust_version: () => Buffer;
  health_check: () => number;
  is_worker_running: () => number;
  bootstrap_worker: (configJson: Buffer | null) => Buffer;
  get_worker_status: () => Buffer;
  stop_worker: () => Buffer;
  transition_to_graceful_shutdown: () => Buffer;
  poll_step_events: () => Buffer | null;
  complete_step_event: (eventId: Buffer, resultJson: Buffer) => number;
  get_ffi_dispatch_metrics: () => Buffer;
  check_starvation_warnings: () => void;
  cleanup_timeouts: () => void;
  log_error: (message: Buffer, fieldsJson: Buffer | null) => void;
  log_warn: (message: Buffer, fieldsJson: Buffer | null) => void;
  log_info: (message: Buffer, fieldsJson: Buffer | null) => void;
  log_debug: (message: Buffer, fieldsJson: Buffer | null) => void;
  log_trace: (message: Buffer, fieldsJson: Buffer | null) => void;
  free_rust_string: (ptr: Buffer) => void;
}

/**
 * Node.js FFI runtime implementation using ffi-napi
 */
export class NodeRuntime extends BaseTaskerRuntime {
  readonly name = 'node';
  private lib: FfiLibrary | null = null;
  private ffi: typeof import('ffi-napi') | null = null;
  private ref: typeof import('ref-napi') | null = null;

  get isLoaded(): boolean {
    return this.lib !== null;
  }

  async load(libraryPath: string): Promise<void> {
    if (this.lib !== null) {
      return; // Already loaded
    }

    // Dynamically import ffi-napi
    const [ffiModule, refModule] = await Promise.all([import('ffi-napi'), import('ref-napi')]);

    this.ffi = ffiModule;
    this.ref = refModule;

    const CString = this.ref.types.CString;
    const int = this.ref.types.int;
    const voidType = this.ref.types.void;

    // Define the FFI bindings
    this.lib = this.ffi.Library(libraryPath, {
      get_version: [CString, []],
      get_rust_version: [CString, []],
      health_check: [int, []],
      is_worker_running: [int, []],
      bootstrap_worker: [CString, [CString]],
      get_worker_status: [CString, []],
      stop_worker: [CString, []],
      transition_to_graceful_shutdown: [CString, []],
      poll_step_events: [CString, []],
      complete_step_event: [int, [CString, CString]],
      get_ffi_dispatch_metrics: [CString, []],
      check_starvation_warnings: [voidType, []],
      cleanup_timeouts: [voidType, []],
      log_error: [voidType, [CString, CString]],
      log_warn: [voidType, [CString, CString]],
      log_info: [voidType, [CString, CString]],
      log_debug: [voidType, [CString, CString]],
      log_trace: [voidType, [CString, CString]],
      free_rust_string: [voidType, [CString]],
    }) as unknown as FfiLibrary;
  }

  unload(): void {
    this.lib = null;
  }

  private ensureLoaded(): FfiLibrary {
    if (!this.lib) {
      throw new Error('Native library not loaded. Call load() first.');
    }
    return this.lib;
  }

  private toCString(str: string): Buffer {
    return Buffer.from(`${str}\0`, 'utf8');
  }

  private fromCString(buf: Buffer | null): string | null {
    if (!buf) return null;
    // Find null terminator and convert to string
    const nullIndex = buf.indexOf(0);
    if (nullIndex >= 0) {
      return buf.slice(0, nullIndex).toString('utf8');
    }
    return buf.toString('utf8');
  }

  getVersion(): string {
    const lib = this.ensureLoaded();
    const result = lib.get_version();
    const version = this.fromCString(result) ?? 'unknown';
    if (result) lib.free_rust_string(result);
    return version;
  }

  getRustVersion(): string {
    const lib = this.ensureLoaded();
    const result = lib.get_rust_version();
    const version = this.fromCString(result) ?? 'unknown';
    if (result) lib.free_rust_string(result);
    return version;
  }

  healthCheck(): boolean {
    const lib = this.ensureLoaded();
    return lib.health_check() === 1;
  }

  bootstrapWorker(config?: BootstrapConfig): BootstrapResult {
    const lib = this.ensureLoaded();
    const configJson = config ? this.toCString(this.toJson(config)) : null;
    const result = lib.bootstrap_worker(configJson);
    const jsonStr = this.fromCString(result);
    if (result) lib.free_rust_string(result);

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
    const result = lib.get_worker_status();
    const jsonStr = this.fromCString(result);
    if (result) lib.free_rust_string(result);

    const parsed = this.parseJson<WorkerStatus>(jsonStr);
    return parsed ?? { success: false, running: false };
  }

  stopWorker(): StopResult {
    const lib = this.ensureLoaded();
    const result = lib.stop_worker();
    const jsonStr = this.fromCString(result);
    if (result) lib.free_rust_string(result);

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
    const result = lib.transition_to_graceful_shutdown();
    const jsonStr = this.fromCString(result);
    if (result) lib.free_rust_string(result);

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
    const result = lib.poll_step_events();
    if (!result) return null;

    const jsonStr = this.fromCString(result);
    lib.free_rust_string(result);

    return this.parseJson<FfiStepEvent>(jsonStr);
  }

  completeStepEvent(eventId: string, result: StepExecutionResult): boolean {
    const lib = this.ensureLoaded();
    const eventIdBuf = this.toCString(eventId);
    const resultJsonBuf = this.toCString(this.toJson(result));
    return lib.complete_step_event(eventIdBuf, resultJsonBuf) === 1;
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    const lib = this.ensureLoaded();
    const result = lib.get_ffi_dispatch_metrics();
    const jsonStr = this.fromCString(result);
    if (result) lib.free_rust_string(result);

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
    const lib = this.ensureLoaded();
    lib.check_starvation_warnings();
  }

  cleanupTimeouts(): void {
    const lib = this.ensureLoaded();
    lib.cleanup_timeouts();
  }

  logError(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    lib.log_error(msgBuf, fieldsBuf);
  }

  logWarn(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    lib.log_warn(msgBuf, fieldsBuf);
  }

  logInfo(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    lib.log_info(msgBuf, fieldsBuf);
  }

  logDebug(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    lib.log_debug(msgBuf, fieldsBuf);
  }

  logTrace(message: string, fields?: LogFields): void {
    const lib = this.ensureLoaded();
    const msgBuf = this.toCString(message);
    const fieldsBuf = fields ? this.toCString(this.toJson(fields)) : null;
    lib.log_trace(msgBuf, fieldsBuf);
  }
}

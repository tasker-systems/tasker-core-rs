/**
 * Bun FFI runtime adapter using bun:ffi.
 *
 * This adapter uses Bun's built-in FFI to interface with the Rust native library.
 * It's designed for high performance and works natively with Bun.
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

// FFI symbol definitions for Bun
type FfiSymbols = {
  get_version: () => bigint;
  get_rust_version: () => bigint;
  health_check: () => number;
  is_worker_running: () => number;
  bootstrap_worker: (configJson: bigint) => bigint;
  get_worker_status: () => bigint;
  stop_worker: () => bigint;
  transition_to_graceful_shutdown: () => bigint;
  poll_step_events: () => bigint;
  poll_in_process_events: () => bigint;
  complete_step_event: (eventId: bigint, resultJson: bigint) => number;
  get_ffi_dispatch_metrics: () => bigint;
  check_starvation_warnings: () => void;
  cleanup_timeouts: () => void;
  log_error: (message: bigint, fieldsJson: bigint) => void;
  log_warn: (message: bigint, fieldsJson: bigint) => void;
  log_info: (message: bigint, fieldsJson: bigint) => void;
  log_debug: (message: bigint, fieldsJson: bigint) => void;
  log_trace: (message: bigint, fieldsJson: bigint) => void;
  free_rust_string: (ptr: bigint) => void;
};

// Bun FFI library handle
interface BunFfiLibrary {
  symbols: FfiSymbols;
  close(): void;
}

/**
 * Bun FFI runtime implementation using bun:ffi
 */
export class BunRuntime extends BaseTaskerRuntime {
  readonly name = 'bun';
  private lib: BunFfiLibrary | null = null;

  get isLoaded(): boolean {
    return this.lib !== null;
  }

  async load(libraryPath: string): Promise<void> {
    if (this.lib !== null) {
      return; // Already loaded
    }

    // Dynamically import bun:ffi
    const { dlopen, FFIType } = await import('bun:ffi');

    // Define FFI symbols
    this.lib = dlopen(libraryPath, {
      get_version: {
        args: [],
        returns: FFIType.ptr,
      },
      get_rust_version: {
        args: [],
        returns: FFIType.ptr,
      },
      health_check: {
        args: [],
        returns: FFIType.i32,
      },
      is_worker_running: {
        args: [],
        returns: FFIType.i32,
      },
      bootstrap_worker: {
        args: [FFIType.ptr],
        returns: FFIType.ptr,
      },
      get_worker_status: {
        args: [],
        returns: FFIType.ptr,
      },
      stop_worker: {
        args: [],
        returns: FFIType.ptr,
      },
      transition_to_graceful_shutdown: {
        args: [],
        returns: FFIType.ptr,
      },
      poll_step_events: {
        args: [],
        returns: FFIType.ptr,
      },
      poll_in_process_events: {
        args: [],
        returns: FFIType.ptr,
      },
      complete_step_event: {
        args: [FFIType.ptr, FFIType.ptr],
        returns: FFIType.i32,
      },
      get_ffi_dispatch_metrics: {
        args: [],
        returns: FFIType.ptr,
      },
      check_starvation_warnings: {
        args: [],
        returns: FFIType.void,
      },
      cleanup_timeouts: {
        args: [],
        returns: FFIType.void,
      },
      log_error: {
        args: [FFIType.ptr, FFIType.ptr],
        returns: FFIType.void,
      },
      log_warn: {
        args: [FFIType.ptr, FFIType.ptr],
        returns: FFIType.void,
      },
      log_info: {
        args: [FFIType.ptr, FFIType.ptr],
        returns: FFIType.void,
      },
      log_debug: {
        args: [FFIType.ptr, FFIType.ptr],
        returns: FFIType.void,
      },
      log_trace: {
        args: [FFIType.ptr, FFIType.ptr],
        returns: FFIType.void,
      },
      free_rust_string: {
        args: [FFIType.ptr],
        returns: FFIType.void,
      },
    }) as unknown as BunFfiLibrary;
  }

  unload(): void {
    if (this.lib) {
      this.lib.close();
      this.lib = null;
    }
  }

  private ensureLoaded(): FfiSymbols {
    if (!this.lib) {
      throw new Error('Native library not loaded. Call load() first.');
    }
    return this.lib.symbols;
  }

  private toCString(str: string): bigint {
    // Create null-terminated C string buffer using Buffer (which has .ptr in Bun)
    // Note: TextEncoder.encode() returns Uint8Array backed by ArrayBuffer,
    // but ArrayBuffer doesn't have .ptr in Bun - only Buffer does!
    const buffer = Buffer.from(`${str}\0`, 'utf-8');
    // biome-ignore lint/suspicious/noExplicitAny: Bun FFI requires Buffer.ptr access
    return BigInt((buffer as any).ptr ?? 0);
  }

  private fromCString(ptrVal: bigint): string | null {
    if (ptrVal === 0n) return null;
    // Read C string from pointer using Bun's CString
    const { CString } = require('bun:ffi');
    return new CString(Number(ptrVal)).toString();
  }

  getVersion(): string {
    const symbols = this.ensureLoaded();
    const result = symbols.get_version();
    const version = this.fromCString(result) ?? 'unknown';
    if (result !== 0n) symbols.free_rust_string(result);
    return version;
  }

  getRustVersion(): string {
    const symbols = this.ensureLoaded();
    const result = symbols.get_rust_version();
    const version = this.fromCString(result) ?? 'unknown';
    if (result !== 0n) symbols.free_rust_string(result);
    return version;
  }

  healthCheck(): boolean {
    const symbols = this.ensureLoaded();
    return symbols.health_check() === 1;
  }

  bootstrapWorker(config?: BootstrapConfig): BootstrapResult {
    const symbols = this.ensureLoaded();
    const configPtr = config ? this.toCString(this.toJson(config)) : 0n;
    const result = symbols.bootstrap_worker(configPtr);
    const jsonStr = this.fromCString(result);
    if (result !== 0n) symbols.free_rust_string(result);

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
    if (result !== 0n) symbols.free_rust_string(result);

    const parsed = this.parseJson<WorkerStatus>(jsonStr);
    return parsed ?? { success: false, running: false };
  }

  stopWorker(): StopResult {
    const symbols = this.ensureLoaded();
    const result = symbols.stop_worker();
    const jsonStr = this.fromCString(result);
    if (result !== 0n) symbols.free_rust_string(result);

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
    if (result !== 0n) symbols.free_rust_string(result);

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
    if (result === 0n) return null;

    const jsonStr = this.fromCString(result);
    symbols.free_rust_string(result);

    return this.parseJson<FfiStepEvent>(jsonStr);
  }

  pollInProcessEvents(): FfiDomainEvent | null {
    const symbols = this.ensureLoaded();
    const result = symbols.poll_in_process_events();
    if (result === 0n) return null;

    const jsonStr = this.fromCString(result);
    symbols.free_rust_string(result);

    return this.parseJson<FfiDomainEvent>(jsonStr);
  }

  completeStepEvent(eventId: string, result: StepExecutionResult): boolean {
    const symbols = this.ensureLoaded();
    const eventIdPtr = this.toCString(eventId);
    const resultJsonPtr = this.toCString(this.toJson(result));
    return symbols.complete_step_event(eventIdPtr, resultJsonPtr) === 1;
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    const symbols = this.ensureLoaded();
    const result = symbols.get_ffi_dispatch_metrics();
    const jsonStr = this.fromCString(result);
    if (result !== 0n) symbols.free_rust_string(result);

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
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : 0n;
    symbols.log_error(msgPtr, fieldsPtr);
  }

  logWarn(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : 0n;
    symbols.log_warn(msgPtr, fieldsPtr);
  }

  logInfo(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : 0n;
    symbols.log_info(msgPtr, fieldsPtr);
  }

  logDebug(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : 0n;
    symbols.log_debug(msgPtr, fieldsPtr);
  }

  logTrace(message: string, fields?: LogFields): void {
    const symbols = this.ensureLoaded();
    const msgPtr = this.toCString(message);
    const fieldsPtr = fields ? this.toCString(this.toJson(fields)) : 0n;
    symbols.log_trace(msgPtr, fieldsPtr);
  }
}

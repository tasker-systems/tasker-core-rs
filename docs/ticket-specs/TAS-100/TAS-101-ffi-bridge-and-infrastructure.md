# TAS-101: TypeScript Worker FFI Bridge and Core Infrastructure

**Parent**: [TAS-100](./README.md)
**Linear**: [TAS-101](https://linear.app/tasker-systems/issue/TAS-101)
**Branch**: `jcoletaylor/tas-101-typescript-ffi-bridge`
**Priority**: High
**Status**: Todo

## Objective

Build the foundational FFI bridge layer and core infrastructure for the TypeScript worker, supporting both Bun and Node.js runtimes through a unified adapter interface. This ticket establishes the communication channel between TypeScript and the Rust `tasker-worker` foundation.

## Summary of Changes

| Area | Implementation | Effort |
|------|----------------|--------|
| **Runtime Detection** | Auto-detect Bun vs Node.js | Low |
| **FFI Adapter Interface** | `FfiAdapter` interface with implementations | Medium |
| **Bun FFI Integration** | `BunFfiAdapter` using `bun:ffi` | Medium |
| **Node FFI Integration** | `NodeFfiAdapter` using `ffi-napi` | Medium |
| **FFI Type Definitions** | TypeScript types for FFI boundary | Low |
| **EventEmitter** | Wrapper around `eventemitter3` | Low |
| **EventPoller** | 10ms polling loop with FFI calls | Medium |
| **Event Names** | Constants for standard events | Low |

## Reference Implementations

**Ruby**: `workers/ruby/lib/tasker_core/bootstrap.rb` (Magnus FFI)
**Python**: `workers/python/python/tasker_core/bootstrap.py` (PyO3 FFI)

Both use the same FFI contract:
- `poll_step_events()` → Returns JSON string or null
- `complete_step_event(event_id, result_json)` → Sends completion
- `bootstrap_worker(config_json)` → Initializes worker
- `get_worker_status()` → Returns status JSON

## Implementation Plan

### Phase 1: Project Setup

**Create directory structure:**
```
workers/typescript/
├── package.json
├── tsconfig.json
├── .gitignore
├── bin/
│   └── (TAS-104)
├── src/
│   ├── index.ts
│   ├── ffi/
│   │   ├── adapter.ts
│   │   ├── bun.ts
│   │   ├── node.ts
│   │   ├── types.ts
│   │   └── runtime.ts
│   └── events/
│       ├── event-emitter.ts
│       ├── event-names.ts
│       └── event-poller.ts
└── tests/
    └── unit/
        └── ffi/
```

**package.json:**
```json
{
  "name": "@tasker-systems/worker",
  "version": "0.1.0",
  "description": "TypeScript worker for tasker-core workflow orchestration",
  "type": "module",
  "main": "./dist/index.js",
  "types": "./dist/index.d.ts",
  "exports": {
    ".": {
      "import": "./dist/index.js",
      "require": "./dist/index.cjs",
      "types": "./dist/index.d.ts"
    }
  },
  "scripts": {
    "build": "tsup src/index.ts --format esm,cjs --dts --clean",
    "test": "vitest",
    "test:bun": "bun test",
    "lint": "eslint src/**/*.ts",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": {
    "eventemitter3": "^5.0.1"
  },
  "devDependencies": {
    "@types/node": "^20.10.0",
    "bun-types": "^1.0.0",
    "typescript": "^5.3.0",
    "tsup": "^8.0.0",
    "vitest": "^1.0.0",
    "eslint": "^8.55.0",
    "@typescript-eslint/eslint-plugin": "^6.15.0",
    "@typescript-eslint/parser": "^6.15.0"
  },
  "peerDependencies": {
    "ffi-napi": "^4.0.3",
    "ref-napi": "^3.0.3"
  },
  "peerDependenciesMeta": {
    "ffi-napi": {
      "optional": true
    },
    "ref-napi": {
      "optional": true
    }
  },
  "engines": {
    "node": ">=18.0.0"
  }
}
```

**tsconfig.json:**
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "lib": ["ES2022"],
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "types": ["node", "bun-types"]
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "tests"]
}
```

### Phase 2: Runtime Detection

**File**: `src/ffi/runtime.ts`

```typescript
/**
 * Detect the current JavaScript runtime
 */
export type RuntimeType = 'bun' | 'node' | 'deno' | 'unknown';

export function detectRuntime(): RuntimeType {
  // Check for Bun
  if (typeof Bun !== 'undefined') {
    return 'bun';
  }
  
  // Check for Deno
  if (typeof Deno !== 'undefined') {
    return 'deno';
  }
  
  // Check for Node.js
  if (typeof process !== 'undefined' && process.versions?.node) {
    return 'node';
  }
  
  return 'unknown';
}

export function isRuntimeSupported(runtime: RuntimeType): boolean {
  return runtime === 'bun' || runtime === 'node';
}

export function assertSupportedRuntime(): RuntimeType {
  const runtime = detectRuntime();
  if (!isRuntimeSupported(runtime)) {
    throw new Error(
      `Unsupported runtime: ${runtime}. ` +
      `@tasker-systems/worker requires Bun or Node.js.`
    );
  }
  return runtime;
}
```

### Phase 3: FFI Type Definitions

**File**: `src/ffi/types.ts`

```typescript
/**
 * FFI type definitions matching Rust tasker-worker exports
 * 
 * References:
 * - Ruby: workers/ruby/lib/tasker_core/models.rb
 * - Python: workers/python/python/tasker_core/types.py
 */

/**
 * Step event from FFI poll_step_events()
 */
export interface FfiStepEvent {
  event_id: string;
  task_uuid: string;
  step_uuid: string;
  correlation_id?: string;
  task_sequence_step: TaskSequenceStep;
}

/**
 * Task sequence step data (matches Rust TaskSequenceStep)
 */
export interface TaskSequenceStep {
  task: Task;
  workflow_step: WorkflowStep;
  dependency_results: DependencyResults;
  step_definition: StepDefinition;
}

/**
 * Task metadata
 */
export interface Task {
  task_uuid: string;
  context: Record<string, any>;
  namespace_name: string;
  template_name: string;
  status: string;
}

/**
 * Workflow step execution state
 */
export interface WorkflowStep {
  workflow_step_uuid: string;
  name: string;
  inputs: Record<string, any>;
  results: Record<string, any> | null;
  state: string;
  attempts: number;
  max_attempts: number;
  retryable: boolean;
}

/**
 * Dependency results from parent steps
 */
export interface DependencyResults {
  results: Record<string, any>;
}

/**
 * Step definition from template
 */
export interface StepDefinition {
  name: string;
  handler: HandlerConfig | null;
  dependencies: string[];
}

/**
 * Handler configuration from template
 */
export interface HandlerConfig {
  handler_class: string;
  initialization?: Record<string, any>;
}

/**
 * Bootstrap configuration
 */
export interface BootstrapConfig {
  namespace?: string;
  log_level?: string;
}

/**
 * Bootstrap result
 */
export interface BootstrapResult {
  status: 'started' | 'already_running';
  handle_id: string;
  worker_id: string;
  message?: string;
}

/**
 * Worker status
 */
export interface WorkerStatus {
  running: boolean;
  worker_id?: string;
  database_pool_size?: number;
  database_pool_available?: number;
  worker_core_status?: string;
}

/**
 * Dispatch metrics from FFI
 */
export interface FfiDispatchMetrics {
  pending_count: number;
  in_flight_count: number;
  completed_count: number;
  failed_count: number;
  starvation_detected: boolean;
  starving_event_count: number;
}
```

### Phase 4: FFI Adapter Interface

**File**: `src/ffi/adapter.ts`

```typescript
import type {
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  FfiDispatchMetrics,
} from './types.js';

/**
 * Unified FFI adapter interface for all runtimes
 * 
 * This interface abstracts over Bun's dlopen and Node's ffi-napi,
 * providing a consistent API for interacting with libtasker_worker.
 */
export interface FfiAdapter {
  /**
   * Poll for the next step event from Rust
   * Returns null if no events are pending
   */
  pollStepEvents(): FfiStepEvent | null;

  /**
   * Send step completion result back to Rust
   */
  completeStepEvent(eventId: string, resultJson: string): void;

  /**
   * Bootstrap the worker system
   */
  bootstrapWorker(config: BootstrapConfig): BootstrapResult;

  /**
   * Get current worker status
   */
  getWorkerStatus(): WorkerStatus;

  /**
   * Stop the worker system
   */
  stopWorker(): string;

  /**
   * Transition to graceful shutdown
   */
  transitionToGracefulShutdown(): string;

  /**
   * Check if worker is running
   */
  isWorkerRunning(): boolean;

  /**
   * Get FFI dispatch metrics
   */
  getFfiDispatchMetrics(): FfiDispatchMetrics;

  /**
   * Check for starvation warnings
   */
  checkStarvationWarnings(): void;

  /**
   * Clean up timed-out events
   */
  cleanupTimeouts(): void;

  /**
   * Close the FFI library handle
   */
  close(): void;
}

/**
 * Base error for FFI operations
 */
export class FfiError extends Error {
  constructor(message: string, public cause?: unknown) {
    super(message);
    this.name = 'FfiError';
  }
}

/**
 * Error when FFI library cannot be loaded
 */
export class FfiLibraryError extends FfiError {
  constructor(message: string, cause?: unknown) {
    super(message, cause);
    this.name = 'FfiLibraryError';
  }
}

/**
 * Error when FFI call fails
 */
export class FfiCallError extends FfiError {
  constructor(message: string, cause?: unknown) {
    super(message, cause);
    this.name = 'FfiCallError';
  }
}
```

### Phase 5: Bun FFI Adapter

**File**: `src/ffi/bun.ts`

```typescript
import { dlopen, FFIType, suffix, type Pointer } from 'bun:ffi';
import type { FfiAdapter } from './adapter.js';
import type {
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  FfiDispatchMetrics,
} from './types.js';
import { FfiLibraryError, FfiCallError } from './adapter.js';

/**
 * Bun FFI adapter using bun:ffi dlopen
 * 
 * References:
 * - Bun FFI docs: https://bun.com/docs/runtime/ffi
 * - Ruby Magnus FFI: workers/ruby/ext/tasker_worker_rb/src/lib.rs
 */
export class BunFfiAdapter implements FfiAdapter {
  private lib: ReturnType<typeof dlopen>;

  constructor(libraryPath?: string) {
    const path = libraryPath || this.findLibraryPath();
    
    try {
      this.lib = dlopen(path, {
        // Step execution functions
        poll_step_events: {
          args: [],
          returns: FFIType.ptr, // JSON string or null
        },
        complete_step_event: {
          args: [FFIType.cstring, FFIType.cstring],
          returns: FFIType.void,
        },
        
        // Bootstrap functions
        bootstrap_worker: {
          args: [FFIType.cstring],
          returns: FFIType.ptr, // JSON string
        },
        get_worker_status: {
          args: [],
          returns: FFIType.ptr, // JSON string
        },
        stop_worker: {
          args: [],
          returns: FFIType.ptr, // JSON string
        },
        transition_to_graceful_shutdown: {
          args: [],
          returns: FFIType.ptr, // JSON string
        },
        is_worker_running: {
          args: [],
          returns: FFIType.bool,
        },
        
        // FFI dispatch functions
        get_ffi_dispatch_metrics: {
          args: [],
          returns: FFIType.ptr, // JSON string
        },
        check_starvation_warnings: {
          args: [],
          returns: FFIType.void,
        },
        cleanup_timeouts: {
          args: [],
          returns: FFIType.void,
        },
        
        // Memory management
        free_rust_string: {
          args: [FFIType.ptr],
          returns: FFIType.void,
        },
      });
    } catch (error) {
      throw new FfiLibraryError(
        `Failed to load tasker-worker library from ${path}`,
        error
      );
    }
  }

  private findLibraryPath(): string {
    // Try common library locations
    const libName = `libtasker_worker.${suffix}`;
    const searchPaths = [
      `./target/release/${libName}`,
      `./target/debug/${libName}`,
      `/usr/local/lib/${libName}`,
      libName, // Let system resolve
    ];

    // In production, could check TASKER_LIBRARY_PATH env var
    const envPath = process.env.TASKER_LIBRARY_PATH;
    if (envPath) {
      searchPaths.unshift(envPath);
    }

    // Return first path that exists (Bun will throw if none work)
    return searchPaths[0];
  }

  private parseJsonPointer<T>(ptr: Pointer | null): T | null {
    if (ptr === null) {
      return null;
    }

    try {
      // Read C string from pointer
      const cString = new CString(ptr);
      const jsonStr = cString.toString();
      
      // Free Rust-allocated string
      this.lib.symbols.free_rust_string(ptr);
      
      return JSON.parse(jsonStr) as T;
    } catch (error) {
      throw new FfiCallError('Failed to parse JSON from FFI', error);
    }
  }

  pollStepEvents(): FfiStepEvent | null {
    try {
      const ptr = this.lib.symbols.poll_step_events();
      return this.parseJsonPointer<FfiStepEvent>(ptr);
    } catch (error) {
      throw new FfiCallError('poll_step_events failed', error);
    }
  }

  completeStepEvent(eventId: string, resultJson: string): void {
    try {
      this.lib.symbols.complete_step_event(eventId, resultJson);
    } catch (error) {
      throw new FfiCallError('complete_step_event failed', error);
    }
  }

  bootstrapWorker(config: BootstrapConfig): BootstrapResult {
    try {
      const configJson = JSON.stringify(config);
      const ptr = this.lib.symbols.bootstrap_worker(configJson);
      const result = this.parseJsonPointer<BootstrapResult>(ptr);
      if (!result) {
        throw new Error('bootstrap_worker returned null');
      }
      return result;
    } catch (error) {
      throw new FfiCallError('bootstrap_worker failed', error);
    }
  }

  getWorkerStatus(): WorkerStatus {
    try {
      const ptr = this.lib.symbols.get_worker_status();
      const status = this.parseJsonPointer<WorkerStatus>(ptr);
      if (!status) {
        throw new Error('get_worker_status returned null');
      }
      return status;
    } catch (error) {
      throw new FfiCallError('get_worker_status failed', error);
    }
  }

  stopWorker(): string {
    try {
      const ptr = this.lib.symbols.stop_worker();
      const result = this.parseJsonPointer<string>(ptr);
      return result || 'Worker stopped';
    } catch (error) {
      throw new FfiCallError('stop_worker failed', error);
    }
  }

  transitionToGracefulShutdown(): string {
    try {
      const ptr = this.lib.symbols.transition_to_graceful_shutdown();
      const result = this.parseJsonPointer<string>(ptr);
      return result || 'Transitioning to graceful shutdown';
    } catch (error) {
      throw new FfiCallError('transition_to_graceful_shutdown failed', error);
    }
  }

  isWorkerRunning(): boolean {
    try {
      return this.lib.symbols.is_worker_running();
    } catch (error) {
      throw new FfiCallError('is_worker_running failed', error);
    }
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    try {
      const ptr = this.lib.symbols.get_ffi_dispatch_metrics();
      const metrics = this.parseJsonPointer<FfiDispatchMetrics>(ptr);
      if (!metrics) {
        throw new Error('get_ffi_dispatch_metrics returned null');
      }
      return metrics;
    } catch (error) {
      throw new FfiCallError('get_ffi_dispatch_metrics failed', error);
    }
  }

  checkStarvationWarnings(): void {
    try {
      this.lib.symbols.check_starvation_warnings();
    } catch (error) {
      throw new FfiCallError('check_starvation_warnings failed', error);
    }
  }

  cleanupTimeouts(): void {
    try {
      this.lib.symbols.cleanup_timeouts();
    } catch (error) {
      throw new FfiCallError('cleanup_timeouts failed', error);
    }
  }

  close(): void {
    this.lib.close();
  }
}

// CString helper for Bun
class CString {
  constructor(private ptr: Pointer) {}

  toString(): string {
    // Bun provides ptr.toString() for C strings
    return this.ptr.toString();
  }
}
```

### Phase 6: Node.js FFI Adapter

**File**: `src/ffi/node.ts`

```typescript
import ffi from 'ffi-napi';
import ref from 'ref-napi';
import type { FfiAdapter } from './adapter.js';
import type {
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  FfiDispatchMetrics,
} from './types.js';
import { FfiLibraryError, FfiCallError } from './adapter.js';

/**
 * Node.js FFI adapter using ffi-napi
 * 
 * References:
 * - Node FFI: https://github.com/node-ffi/node-ffi
 * - Python PyO3: workers/python/src/lib.rs
 */
export class NodeFfiAdapter implements FfiAdapter {
  private lib: any;
  private stringType = ref.types.CString;
  private voidPtrType = ref.refType(ref.types.void);

  constructor(libraryPath?: string) {
    const path = libraryPath || this.findLibraryPath();

    try {
      this.lib = ffi.Library(path, {
        // Step execution functions
        poll_step_events: [this.voidPtrType, []],
        complete_step_event: ['void', ['string', 'string']],

        // Bootstrap functions
        bootstrap_worker: [this.voidPtrType, ['string']],
        get_worker_status: [this.voidPtrType, []],
        stop_worker: [this.voidPtrType, []],
        transition_to_graceful_shutdown: [this.voidPtrType, []],
        is_worker_running: ['bool', []],

        // FFI dispatch functions
        get_ffi_dispatch_metrics: [this.voidPtrType, []],
        check_starvation_warnings: ['void', []],
        cleanup_timeouts: ['void', []],

        // Memory management
        free_rust_string: ['void', [this.voidPtrType]],
      });
    } catch (error) {
      throw new FfiLibraryError(
        `Failed to load tasker-worker library from ${path}`,
        error
      );
    }
  }

  private findLibraryPath(): string {
    const platform = process.platform;
    const ext = platform === 'win32' ? 'dll' : platform === 'darwin' ? 'dylib' : 'so';
    const libName = `libtasker_worker.${ext}`;

    const searchPaths = [
      `./target/release/${libName}`,
      `./target/debug/${libName}`,
      `/usr/local/lib/${libName}`,
    ];

    const envPath = process.env.TASKER_LIBRARY_PATH;
    if (envPath) {
      searchPaths.unshift(envPath);
    }

    return searchPaths[0];
  }

  private parseJsonPointer<T>(ptr: Buffer | null): T | null {
    if (!ptr || ptr.isNull()) {
      return null;
    }

    try {
      const cString = ref.readCString(ptr, 0);
      this.lib.free_rust_string(ptr);
      return JSON.parse(cString) as T;
    } catch (error) {
      throw new FfiCallError('Failed to parse JSON from FFI', error);
    }
  }

  pollStepEvents(): FfiStepEvent | null {
    try {
      const ptr = this.lib.poll_step_events();
      return this.parseJsonPointer<FfiStepEvent>(ptr);
    } catch (error) {
      throw new FfiCallError('poll_step_events failed', error);
    }
  }

  completeStepEvent(eventId: string, resultJson: string): void {
    try {
      this.lib.complete_step_event(eventId, resultJson);
    } catch (error) {
      throw new FfiCallError('complete_step_event failed', error);
    }
  }

  bootstrapWorker(config: BootstrapConfig): BootstrapResult {
    try {
      const configJson = JSON.stringify(config);
      const ptr = this.lib.bootstrap_worker(configJson);
      const result = this.parseJsonPointer<BootstrapResult>(ptr);
      if (!result) {
        throw new Error('bootstrap_worker returned null');
      }
      return result;
    } catch (error) {
      throw new FfiCallError('bootstrap_worker failed', error);
    }
  }

  getWorkerStatus(): WorkerStatus {
    try {
      const ptr = this.lib.get_worker_status();
      const status = this.parseJsonPointer<WorkerStatus>(ptr);
      if (!status) {
        throw new Error('get_worker_status returned null');
      }
      return status;
    } catch (error) {
      throw new FfiCallError('get_worker_status failed', error);
    }
  }

  stopWorker(): string {
    try {
      const ptr = this.lib.stop_worker();
      const result = this.parseJsonPointer<string>(ptr);
      return result || 'Worker stopped';
    } catch (error) {
      throw new FfiCallError('stop_worker failed', error);
    }
  }

  transitionToGracefulShutdown(): string {
    try {
      const ptr = this.lib.transition_to_graceful_shutdown();
      const result = this.parseJsonPointer<string>(ptr);
      return result || 'Transitioning to graceful shutdown';
    } catch (error) {
      throw new FfiCallError('transition_to_graceful_shutdown failed', error);
    }
  }

  isWorkerRunning(): boolean {
    try {
      return this.lib.is_worker_running();
    } catch (error) {
      throw new FfiCallError('is_worker_running failed', error);
    }
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    try {
      const ptr = this.lib.get_ffi_dispatch_metrics();
      const metrics = this.parseJsonPointer<FfiDispatchMetrics>(ptr);
      if (!metrics) {
        throw new Error('get_ffi_dispatch_metrics returned null');
      }
      return metrics;
    } catch (error) {
      throw new FfiCallError('get_ffi_dispatch_metrics failed', error);
    }
  }

  checkStarvationWarnings(): void {
    try {
      this.lib.check_starvation_warnings();
    } catch (error) {
      throw new FfiCallError('check_starvation_warnings failed', error);
    }
  }

  cleanupTimeouts(): void {
    try {
      this.lib.cleanup_timeouts();
    } catch (error) {
      throw new FfiCallError('cleanup_timeouts failed', error);
    }
  }

  close(): void {
    // Node FFI doesn't require explicit close
  }
}
```

### Phase 7: EventEmitter Wrapper

**File**: `src/events/event-emitter.ts`

```typescript
import EventEmitter from 'eventemitter3';

/**
 * EventEmitter wrapper providing consistent pub/sub across runtimes
 * 
 * References:
 * - Ruby dry-events: workers/ruby/lib/tasker_core/event_bridge.rb
 * - Python pyee: workers/python/python/tasker_core/event_bridge.py
 */
export class TaskerEventEmitter extends EventEmitter {
  private _active: boolean = false;

  start(): void {
    this._active = true;
  }

  stop(): void {
    this._active = false;
    this.removeAllListeners();
  }

  isActive(): boolean {
    return this._active;
  }

  publish(event: string, ...args: any[]): void {
    if (!this._active) {
      console.warn(`EventEmitter not active, dropping event: ${event}`);
      return;
    }
    this.emit(event, ...args);
  }

  subscribe(event: string, handler: (...args: any[]) => void): void {
    this.on(event, handler);
  }

  unsubscribe(event: string, handler: (...args: any[]) => void): void {
    this.off(event, handler);
  }
}
```

**File**: `src/events/event-names.ts`

```typescript
/**
 * Standard event names used across the worker system
 * 
 * Matches Ruby and Python event naming conventions
 */
export const EventNames = {
  STEP_EXECUTION_RECEIVED: 'step.execution.received',
  STEP_COMPLETION_SENT: 'step.completion.sent',
  HANDLER_REGISTERED: 'handler.registered',
  HANDLER_ERROR: 'handler.error',
  POLLER_METRICS: 'poller.metrics',
  POLLER_ERROR: 'poller.error',
  WORKER_STARTED: 'worker.started',
  WORKER_STOPPED: 'worker.stopped',
} as const;

export type EventName = (typeof EventNames)[keyof typeof EventNames];
```

### Phase 8: EventPoller

**File**: `src/events/event-poller.ts`

```typescript
import type { FfiAdapter } from '../ffi/adapter.js';
import type { FfiStepEvent, FfiDispatchMetrics } from '../ffi/types.js';

/**
 * EventPoller polls Rust FFI for step events at 10ms intervals
 * 
 * References:
 * - Ruby: workers/ruby/lib/tasker_core/worker/event_poller.rb
 * - Python: workers/python/python/tasker_core/event_poller.py
 */
export class EventPoller {
  private intervalHandle: Timer | NodeJS.Timeout | null = null;
  private active = false;
  private pollCount = 0;

  private onEventCallback: ((event: FfiStepEvent) => void) | null = null;
  private onErrorCallback: ((error: Error) => void) | null = null;
  private onMetricsCallback: ((metrics: FfiDispatchMetrics) => void) | null = null;

  constructor(
    private ffiAdapter: FfiAdapter,
    private pollingIntervalMs: number = 10,
    private starvationCheckInterval: number = 100,
    private cleanupInterval: number = 1000
  ) {}

  /**
   * Register callback for step events
   */
  onStepEvent(callback: (event: FfiStepEvent) => void): void {
    this.onEventCallback = callback;
  }

  /**
   * Register callback for errors
   */
  onError(callback: (error: Error) => void): void {
    this.onErrorCallback = callback;
  }

  /**
   * Register callback for metrics updates
   */
  onMetrics(callback: (metrics: FfiDispatchMetrics) => void): void {
    this.onMetricsCallback = callback;
  }

  /**
   * Start polling
   */
  start(): void {
    if (this.active) {
      return;
    }

    this.active = true;
    this.intervalHandle = setInterval(() => {
      this.poll();
    }, this.pollingIntervalMs);
  }

  /**
   * Stop polling
   */
  stop(timeout: number = 5000): void {
    if (!this.active) {
      return;
    }

    this.active = false;
    if (this.intervalHandle) {
      clearInterval(this.intervalHandle);
      this.intervalHandle = null;
    }
  }

  /**
   * Check if poller is active
   */
  isActive(): boolean {
    return this.active;
  }

  private poll(): void {
    if (!this.active) {
      return;
    }

    try {
      // Poll for step event
      const event = this.ffiAdapter.pollStepEvents();
      if (event && this.onEventCallback) {
        this.onEventCallback(event);
      }

      this.pollCount++;

      // Periodic maintenance
      if (this.pollCount % this.starvationCheckInterval === 0) {
        this.ffiAdapter.checkStarvationWarnings();
      }

      if (this.pollCount % this.cleanupInterval === 0) {
        this.ffiAdapter.cleanupTimeouts();
      }

      // Periodic metrics (every 100 polls = ~1 second)
      if (this.pollCount % 100 === 0 && this.onMetricsCallback) {
        const metrics = this.ffiAdapter.getFfiDispatchMetrics();
        this.onMetricsCallback(metrics);
      }
    } catch (error) {
      if (this.onErrorCallback) {
        this.onErrorCallback(error as Error);
      } else {
        console.error('EventPoller error:', error);
      }
    }
  }
}
```

## Files Summary

### New Files
| File | Purpose |
|------|---------|
| `src/ffi/runtime.ts` | Runtime detection (Bun/Node/Deno) |
| `src/ffi/types.ts` | TypeScript types for FFI boundary |
| `src/ffi/adapter.ts` | FfiAdapter interface and errors |
| `src/ffi/bun.ts` | Bun FFI adapter implementation |
| `src/ffi/node.ts` | Node.js FFI adapter implementation |
| `src/events/event-emitter.ts` | EventEmitter wrapper |
| `src/events/event-names.ts` | Event name constants |
| `src/events/event-poller.ts` | 10ms polling loop |
| `package.json` | Package configuration |
| `tsconfig.json` | TypeScript configuration |

## Verification Checklist

- [ ] Runtime detection correctly identifies Bun, Node.js, Deno
- [ ] Bun FFI adapter loads `libtasker_worker` successfully
- [ ] Node FFI adapter loads `libtasker_worker` successfully
- [ ] `pollStepEvents()` returns null when no events pending
- [ ] `pollStepEvents()` returns valid `FfiStepEvent` when events available
- [ ] `completeStepEvent()` sends completion without errors
- [ ] `bootstrapWorker()` initializes worker successfully
- [ ] `getWorkerStatus()` returns valid status
- [ ] `stopWorker()` shuts down cleanly
- [ ] EventEmitter start/stop lifecycle works
- [ ] EventPoller polls at 10ms intervals
- [ ] EventPoller calls starvation check every 100 polls
- [ ] EventPoller calls cleanup every 1000 polls
- [ ] Memory is properly freed (no leaks)
- [ ] Type safety enforced at FFI boundary
- [ ] Unit tests pass for both Bun and Node adapters

## Testing Strategy

### Unit Tests (TAS-105)
```typescript
// tests/unit/ffi/runtime.test.ts
describe('Runtime Detection', () => {
  it('detects Bun runtime');
  it('detects Node runtime');
  it('throws on unsupported runtime');
});

// tests/unit/ffi/bun.test.ts (run with Bun)
describe('BunFfiAdapter', () => {
  it('loads library successfully');
  it('polls step events');
  it('completes step event');
  it('handles null pointers safely');
});

// tests/unit/ffi/node.test.ts (run with Node)
describe('NodeFfiAdapter', () => {
  it('loads library successfully');
  it('polls step events');
  it('completes step event');
  it('handles null pointers safely');
});
```

## Risk Assessment

**Medium Risk**:
- Bun FFI experimental with known bugs
- Memory management at FFI boundary
- Runtime-specific FFI differences

**Mitigation**:
- Start with Node.js adapter (mature)
- Comprehensive null pointer handling
- Extensive test coverage per runtime
- Clear error messages at FFI boundary

## Estimated Scope

- **Setup**: ~2 hours (package.json, tsconfig)
- **Runtime Detection**: ~1 hour
- **FFI Types**: ~2 hours
- **Adapter Interface**: ~1 hour
- **Bun Adapter**: ~4 hours
- **Node Adapter**: ~4 hours
- **EventEmitter**: ~2 hours
- **EventPoller**: ~3 hours
- **Testing**: ~6 hours
- **Total**: ~25 hours (~3-4 days)

## Dependencies

- None (foundational ticket)

## Next Steps

After TAS-101 completion:
- **TAS-102**: Build handler API and registry on top of FFI bridge
- **TAS-103**: Implement specialized handlers
- **TAS-104**: Create server and bootstrap

---

**Reference Commits**:
- Ruby FFI Bootstrap: See `workers/ruby/lib/tasker_core/bootstrap.rb`
- Python FFI Bootstrap: See `workers/python/python/tasker_core/bootstrap.py`

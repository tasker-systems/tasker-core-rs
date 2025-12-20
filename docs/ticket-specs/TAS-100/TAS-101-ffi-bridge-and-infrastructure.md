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

/**
 * Structured logging fields
 */
export interface LogFields {
  [key: string]: string;
}
```

### Phase 4: Strongly-Typed Runtime Interface

**File**: `src/ffi/runtime-interface.ts`

```typescript
import type {
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  FfiDispatchMetrics,
  LogFields,
} from './types.ts';

/**
 * Strongly-typed runtime interface for FFI calls.
 * 
 * This interface provides type-safe access to all Rust FFI functions,
 * replacing the generic callFfiFunction pattern with explicit methods
 * that have proper TypeScript return types.
 * 
 * Benefits:
 * - Full type safety for all FFI calls
 * - IDE autocomplete support
 * - Clear API surface
 * - Runtime abstraction (Bun/Node/Deno)
 */
export interface TaskerRuntime {
  // ============================================================
  // Step Execution
  // ============================================================

  /**
   * Poll for pending step events from the FFI dispatch channel.
   * 
   * Returns an array of step events ready for handler execution.
   * Returns empty array if no events are pending.
   * 
   * @returns Array of step events
   */
  pollStepEvents(): FfiStepEvent[];

  /**
   * Complete a step event by sending the execution result back to Rust.
   * 
   * @param eventId - The event ID from the FfiStepEvent
   * @param result - Execution result (will be JSON serialized)
   */
  completeStepEvent(eventId: string, result: StepExecutionResult): void;

  // ============================================================
  // Worker Lifecycle
  // ============================================================

  /**
   * Bootstrap the Rust worker foundation.
   * 
   * Initializes Tokio runtime, database connections, PGMQ channels,
   * and starts internal worker services.
   * 
   * @param config - Bootstrap configuration
   * @returns Bootstrap result with worker details
   * @throws Error if bootstrap fails or worker already running
   */
  bootstrapWorker(config: BootstrapConfig): BootstrapResult;

  /**
   * Get current worker status and metrics.
   * 
   * @returns Worker status including resource usage
   */
  getWorkerStatus(): WorkerStatus;

  /**
   * Stop the worker system gracefully.
   * 
   * Safe to call even if worker is not running.
   * 
   * @returns Status message
   */
  stopWorker(): string;

  /**
   * Initiate graceful shutdown transition.
   * 
   * Allows in-flight operations to complete before full shutdown.
   * 
   * @returns Status message
   */
  transitionToGracefulShutdown(): string;

  /**
   * Check if the worker system is currently running.
   * 
   * Lightweight check without full status query.
   * 
   * @returns True if worker is running
   */
  isWorkerRunning(): boolean;

  // ============================================================
  // Structured Logging
  // ============================================================

  /**
   * Log an ERROR level message via Rust tracing.
   * 
   * @param message - Log message
   * @param fields - Optional structured fields
   */
  logError(message: string, fields?: LogFields): void;

  /**
   * Log a WARN level message via Rust tracing.
   * 
   * @param message - Log message
   * @param fields - Optional structured fields
   */
  logWarn(message: string, fields?: LogFields): void;

  /**
   * Log an INFO level message via Rust tracing.
   * 
   * @param message - Log message
   * @param fields - Optional structured fields
   */
  logInfo(message: string, fields?: LogFields): void;

  /**
   * Log a DEBUG level message via Rust tracing.
   * 
   * @param message - Log message
   * @param fields - Optional structured fields
   */
  logDebug(message: string, fields?: LogFields): void;

  /**
   * Log a TRACE level message via Rust tracing.
   * 
   * @param message - Log message
   * @param fields - Optional structured fields
   */
  logTrace(message: string, fields?: LogFields): void;

  // ============================================================
  // FFI Dispatch Metrics
  // ============================================================

  /**
   * Get FFI dispatch channel metrics.
   * 
   * @returns Metrics including pending/in-flight/completed counts
   */
  getFfiDispatchMetrics(): FfiDispatchMetrics;

  /**
   * Check for event starvation and log warnings if detected.
   * 
   * Should be called periodically (e.g., every 100 poll iterations).
   */
  checkStarvationWarnings(): void;

  /**
   * Clean up timed-out events from the dispatch channel.
   * 
   * Should be called periodically (e.g., every 1000 poll iterations).
   */
  cleanupTimeouts(): void;

  // ============================================================
  // Resource Management
  // ============================================================

  /**
   * Close the FFI library handle and release resources.
   * 
   * Should be called during shutdown.
   */
  close(): void;
}

/**
 * Step execution result sent back to Rust.
 */
export interface StepExecutionResult {
  step_uuid: string;
  task_uuid: string;
  success: boolean;
  status: 'completed' | 'error';
  execution_time_ms: number;
  result?: any;
  error?: {
    message: string;
    error_type: string;
    error_code?: string;
    retryable: boolean;
  };
  worker_id: string;
  correlation_id?: string;
  metadata: Record<string, any>;
}
```

---

### Phase 5: Runtime Adapter Factory

**File**: `src/ffi/runtime-factory.ts`

```typescript
import type { TaskerRuntime } from './runtime-interface.ts';
import { BunTaskerRuntime } from './bun-runtime.ts';
import { NodeTaskerRuntime } from './node-runtime.ts';
import { detectRuntime, assertSupportedRuntime } from './runtime.ts';

/**
 * Singleton instance of the runtime
 */
let runtimeInstance: TaskerRuntime | null = null;

/**
 * Get the TaskerRuntime instance for the current JavaScript runtime.
 * 
 * Automatically detects Bun vs Node.js and returns the appropriate
 * implementation.
 * 
 * @param libraryPath - Optional path to libtasker_worker (auto-detected if not provided)
 * @returns TaskerRuntime instance
 * @throws Error if runtime is not supported
 * 
 * @example
 * const runtime = getTaskerRuntime();
 * const result = runtime.bootstrapWorker({ namespace: 'payments' });
 */
export function getTaskerRuntime(libraryPath?: string): TaskerRuntime {
  if (runtimeInstance) {
    return runtimeInstance;
  }

  const runtime = assertSupportedRuntime();

  if (runtime === 'bun') {
    runtimeInstance = new BunTaskerRuntime(libraryPath);
  } else if (runtime === 'node') {
    runtimeInstance = new NodeTaskerRuntime(libraryPath);
  } else {
    throw new Error(`Unsupported runtime: ${runtime}`);
  }

  return runtimeInstance;
}

/**
 * Reset the runtime instance (primarily for testing).
 */
export function resetTaskerRuntime(): void {
  if (runtimeInstance) {
    runtimeInstance.close();
    runtimeInstance = null;
  }
}
```

---

### Phase 6: Bun Implementation

**File**: `src/ffi/bun-runtime.ts`

```typescript
import { dlopen, FFIType, suffix, type Pointer } from 'bun:ffi';
import type { TaskerRuntime, StepExecutionResult } from './runtime-interface.ts';
import type {
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  FfiDispatchMetrics,
  LogFields,
} from './types.ts';

/**
 * Bun implementation of TaskerRuntime using bun:ffi.
 */
export class BunTaskerRuntime implements TaskerRuntime {
  private lib: ReturnType<typeof dlopen>;

  constructor(libraryPath?: string) {
    const path = libraryPath || this.findLibraryPath();
    
    this.lib = dlopen(path, {
      // Step execution
      poll_step_events: {
        args: [],
        returns: FFIType.ptr,
      },
      complete_step_event: {
        args: [FFIType.cstring, FFIType.cstring],
        returns: FFIType.void,
      },
      
      // Worker lifecycle
      bootstrap_worker: {
        args: [FFIType.cstring],
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
      is_worker_running: {
        args: [],
        returns: FFIType.bool,
      },
      
      // Logging
      log_error: {
        args: [FFIType.cstring, FFIType.cstring],
        returns: FFIType.void,
      },
      log_warn: {
        args: [FFIType.cstring, FFIType.cstring],
        returns: FFIType.void,
      },
      log_info: {
        args: [FFIType.cstring, FFIType.cstring],
        returns: FFIType.void,
      },
      log_debug: {
        args: [FFIType.cstring, FFIType.cstring],
        returns: FFIType.void,
      },
      log_trace: {
        args: [FFIType.cstring, FFIType.cstring],
        returns: FFIType.void,
      },
      
      // FFI dispatch metrics
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
      
      // Memory management
      free_rust_string: {
        args: [FFIType.ptr],
        returns: FFIType.void,
      },
    });
  }

  pollStepEvents(): FfiStepEvent[] {
    const ptr = this.lib.symbols.poll_step_events() as Pointer;
    if (!ptr) {
      return [];
    }
    
    const json = new CString(ptr);
    const events = JSON.parse(json);
    this.lib.symbols.free_rust_string(ptr);
    return events;
  }

  completeStepEvent(eventId: string, result: StepExecutionResult): void {
    const resultJson = JSON.stringify(result);
    this.lib.symbols.complete_step_event(eventId, resultJson);
  }

  bootstrapWorker(config: BootstrapConfig): BootstrapResult {
    const configJson = JSON.stringify(config);
    const ptr = this.lib.symbols.bootstrap_worker(configJson) as Pointer;
    const json = new CString(ptr);
    const result = JSON.parse(json);
    this.lib.symbols.free_rust_string(ptr);
    return result;
  }

  getWorkerStatus(): WorkerStatus {
    const ptr = this.lib.symbols.get_worker_status() as Pointer;
    const json = new CString(ptr);
    const status = JSON.parse(json);
    this.lib.symbols.free_rust_string(ptr);
    return status;
  }

  stopWorker(): string {
    const ptr = this.lib.symbols.stop_worker() as Pointer;
    const json = new CString(ptr);
    this.lib.symbols.free_rust_string(ptr);
    return json;
  }

  transitionToGracefulShutdown(): string {
    const ptr = this.lib.symbols.transition_to_graceful_shutdown() as Pointer;
    const json = new CString(ptr);
    this.lib.symbols.free_rust_string(ptr);
    return json;
  }

  isWorkerRunning(): boolean {
    return this.lib.symbols.is_worker_running() as boolean;
  }

  logError(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    this.lib.symbols.log_error(message, fieldsJson);
  }

  logWarn(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    this.lib.symbols.log_warn(message, fieldsJson);
  }

  logInfo(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    this.lib.symbols.log_info(message, fieldsJson);
  }

  logDebug(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    this.lib.symbols.log_debug(message, fieldsJson);
  }

  logTrace(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    this.lib.symbols.log_trace(message, fieldsJson);
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    const ptr = this.lib.symbols.get_ffi_dispatch_metrics() as Pointer;
    const json = new CString(ptr);
    const metrics = JSON.parse(json);
    this.lib.symbols.free_rust_string(ptr);
    return metrics;
  }

  checkStarvationWarnings(): void {
    this.lib.symbols.check_starvation_warnings();
  }

  cleanupTimeouts(): void {
    this.lib.symbols.cleanup_timeouts();
  }

  close(): void {
    this.lib.close();
  }

  private findLibraryPath(): string {
    // Auto-detect library path based on platform
    const platform = process.platform;
    const ext = suffix; // From bun:ffi
    
    // Try common locations
    const paths = [
      `./target/debug/libtasker_worker.${ext}`,
      `./target/release/libtasker_worker.${ext}`,
      `/usr/local/lib/libtasker_worker.${ext}`,
    ];
    
    for (const path of paths) {
      if (Bun.file(path).exists) {
        return path;
      }
    }
    
    throw new Error(
      'Could not find libtasker_worker. ' +
      'Set TASKER_LIBRARY_PATH or ensure library is in target/debug or target/release'
    );
  }
}
```

---

### Phase 7: Usage Examples

**Strongly-typed bootstrap:**
```typescript
import { getTaskerRuntime } from './ffi/runtime-factory';

const runtime = getTaskerRuntime();

// Type-safe bootstrap with full return type
const result = runtime.bootstrapWorker({
  namespace: 'payments',
  log_level: 'info',
});

// result.status is typed as 'started' | 'already_running'
if (result.status === 'started') {
  console.log(`Worker ${result.worker_id} started`);
}
```

**Strongly-typed logging:**
```typescript
const runtime = getTaskerRuntime();

// Full type safety and autocomplete
runtime.logInfo('Processing payment', {
  component: 'payment_handler',
  operation: 'process',
  correlation_id: 'abc-123',
  task_uuid: 'task-456',
});
```

**Strongly-typed step polling:**
```typescript
const runtime = getTaskerRuntime();

// pollStepEvents() returns FfiStepEvent[] - fully typed
const events = runtime.pollStepEvents();

for (const event of events) {
  // event.task_uuid is string
  // event.task_sequence_step is TaskSequenceStep
  // Full IDE autocomplete available
  console.log(`Processing step ${event.step_uuid}`);
}
```

**Strongly-typed metrics:**
```typescript
const runtime = getTaskerRuntime();

// getFfiDispatchMetrics() returns FfiDispatchMetrics - fully typed
const metrics = runtime.getFfiDispatchMetrics();

if (metrics.starvation_detected) {
  console.warn(`Starvation detected: ${metrics.starving_event_count} events`);
}
```

---

### Benefits of TaskerRuntime Interface

1. **Type Safety**: Every FFI call has explicit TypeScript types
   - Before: `adapter.callFfiFunction('bootstrap_worker', config)` → `any`
   - After: `runtime.bootstrapWorker(config)` → `BootstrapResult`

2. **IDE Support**: Full autocomplete for all FFI functions
   - IntelliSense shows all available methods
   - Parameter types and return types are clear

3. **Refactoring Safety**: Compiler catches breaking changes
   - Rename a method → TypeScript finds all usages
   - Change return type → TypeScript validates all call sites

4. **Runtime Abstraction**: Easy to add Deno support
   - Create `DenoTaskerRuntime implements TaskerRuntime`
   - No changes needed to calling code

5. **Clear API Surface**: One place to see all FFI functions
   - `TaskerRuntime` interface documents the entire API
   - No need to grep for `callFfiFunction` calls

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
import type { FfiAdapter } from './adapter.ts';
import type {
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  FfiDispatchMetrics,
} from './types.ts';
import { FfiLibraryError, FfiCallError } from './adapter.ts';

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
import type { FfiAdapter } from './adapter.ts';
import type {
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  FfiDispatchMetrics,
} from './types.ts';
import { FfiLibraryError, FfiCallError } from './adapter.ts';

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
import type { FfiAdapter } from '../ffi/adapter.ts';
import type { FfiStepEvent, FfiDispatchMetrics } from '../ffi/types.ts';

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

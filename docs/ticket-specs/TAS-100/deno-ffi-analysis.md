# Deno FFI Support Analysis

**Date**: 2025-12-20  
**Context**: TAS-100 TypeScript Worker Implementation  
**Related**: TAS-101 (FFI Bridge and Core Infrastructure)

## Executive Summary

Adding Deno support to the TypeScript worker is **trivial** - Deno's FFI API (`Deno.dlopen`) is nearly identical to Bun's `bun:ffi`. The primary differences are:

1. **API surface**: `Deno.dlopen` vs `dlopen` from `bun:ffi`
2. **Type definitions**: `Deno.ForeignFunction` vs Bun's `FFIType`
3. **Pointer handling**: `Deno.UnsafePointer` vs Bun's `Pointer`
4. **Permission model**: Requires `--allow-ffi` flag

**Recommendation**: Include Deno support in TAS-101 as part of the initial implementation. The incremental cost is minimal (~2-3 hours).

---

## API Comparison

### Bun FFI
```typescript
import { dlopen, FFIType, suffix } from 'bun:ffi';

const lib = dlopen('./libtasker_worker.dylib', {
  poll_step_events: {
    args: [],
    returns: FFIType.ptr,
  },
  bootstrap_worker: {
    args: [FFIType.cstring],
    returns: FFIType.ptr,
  },
  free_rust_string: {
    args: [FFIType.ptr],
    returns: FFIType.void,
  },
});

const ptr = lib.symbols.poll_step_events();
if (ptr) {
  const json = new CString(ptr);
  lib.symbols.free_rust_string(ptr);
}
```

### Deno FFI
```typescript
const lib = Deno.dlopen('./libtasker_worker.dylib', {
  poll_step_events: {
    parameters: [],
    result: 'pointer',
  },
  bootstrap_worker: {
    parameters: ['pointer'],
    result: 'pointer',
  },
  free_rust_string: {
    parameters: ['pointer'],
    result: 'void',
  },
});

const ptr = lib.symbols.poll_step_events();
if (ptr) {
  const view = new Deno.UnsafePointerView(ptr);
  const json = view.getCString();
  lib.symbols.free_rust_string(ptr);
}
```

### Key Differences

| Aspect | Bun | Deno |
|--------|-----|------|
| **Import** | `import { dlopen } from 'bun:ffi'` | `Deno.dlopen` (global) |
| **Function signature key** | `args` / `returns` | `parameters` / `result` |
| **String type** | `FFIType.cstring` | `'pointer'` |
| **Pointer type** | `FFIType.ptr` | `'pointer'` |
| **Void type** | `FFIType.void` | `'void'` |
| **Bool type** | `FFIType.bool` | `'bool'` |
| **Pointer reading** | `new CString(ptr)` | `new Deno.UnsafePointerView(ptr).getCString()` |
| **Library close** | `lib.close()` | `lib.close()` |
| **Permission flag** | None (default allowed) | `--allow-ffi` required |

---

## Implementation Strategy

### 1. Runtime Detection (Already Planned)

**File**: `src/ffi/runtime.ts`

```typescript
export type RuntimeType = 'bun' | 'node' | 'deno' | 'unknown';

export function detectRuntime(): RuntimeType {
  // Check for Deno (add this first)
  if (typeof Deno !== 'undefined') {
    return 'deno';
  }
  
  // Check for Bun
  if (typeof Bun !== 'undefined') {
    return 'bun';
  }
  
  // Check for Node.js
  if (typeof process !== 'undefined' && process.versions?.node) {
    return 'node';
  }
  
  return 'unknown';
}

export function isRuntimeSupported(runtime: RuntimeType): boolean {
  return runtime === 'bun' || runtime === 'node' || runtime === 'deno';
}
```

### 2. Deno TaskerRuntime Implementation

**File**: `src/ffi/deno-runtime.ts`

```typescript
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
 * Deno implementation of TaskerRuntime using Deno.dlopen.
 */
export class DenoTaskerRuntime implements TaskerRuntime {
  private lib: Deno.DynamicLibrary<typeof symbols>;

  constructor(libraryPath?: string) {
    const path = libraryPath || this.findLibraryPath();
    
    this.lib = Deno.dlopen(path, symbols);
  }

  pollStepEvents(): FfiStepEvent[] {
    const ptr = this.lib.symbols.poll_step_events();
    if (!ptr) {
      return [];
    }
    
    const view = new Deno.UnsafePointerView(ptr);
    const json = view.getCString();
    const events = JSON.parse(json);
    this.lib.symbols.free_rust_string(ptr);
    return events;
  }

  completeStepEvent(eventId: string, result: StepExecutionResult): void {
    const resultJson = JSON.stringify(result);
    const eventIdPtr = Deno.UnsafePointer.of(new TextEncoder().encode(eventId + '\0'));
    const resultPtr = Deno.UnsafePointer.of(new TextEncoder().encode(resultJson + '\0'));
    this.lib.symbols.complete_step_event(eventIdPtr, resultPtr);
  }

  bootstrapWorker(config: BootstrapConfig): BootstrapResult {
    const configJson = JSON.stringify(config);
    const configPtr = Deno.UnsafePointer.of(new TextEncoder().encode(configJson + '\0'));
    const ptr = this.lib.symbols.bootstrap_worker(configPtr);
    const view = new Deno.UnsafePointerView(ptr);
    const json = view.getCString();
    const result = JSON.parse(json);
    this.lib.symbols.free_rust_string(ptr);
    return result;
  }

  getWorkerStatus(): WorkerStatus {
    const ptr = this.lib.symbols.get_worker_status();
    const view = new Deno.UnsafePointerView(ptr);
    const json = view.getCString();
    const status = JSON.parse(json);
    this.lib.symbols.free_rust_string(ptr);
    return status;
  }

  stopWorker(): string {
    const ptr = this.lib.symbols.stop_worker();
    const view = new Deno.UnsafePointerView(ptr);
    const result = view.getCString();
    this.lib.symbols.free_rust_string(ptr);
    return result;
  }

  transitionToGracefulShutdown(): string {
    const ptr = this.lib.symbols.transition_to_graceful_shutdown();
    const view = new Deno.UnsafePointerView(ptr);
    const result = view.getCString();
    this.lib.symbols.free_rust_string(ptr);
    return result;
  }

  isWorkerRunning(): boolean {
    return this.lib.symbols.is_worker_running();
  }

  logError(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    const msgPtr = Deno.UnsafePointer.of(new TextEncoder().encode(message + '\0'));
    const fieldsPtr = fieldsJson 
      ? Deno.UnsafePointer.of(new TextEncoder().encode(fieldsJson + '\0'))
      : null;
    this.lib.symbols.log_error(msgPtr, fieldsPtr);
  }

  logWarn(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    const msgPtr = Deno.UnsafePointer.of(new TextEncoder().encode(message + '\0'));
    const fieldsPtr = fieldsJson 
      ? Deno.UnsafePointer.of(new TextEncoder().encode(fieldsJson + '\0'))
      : null;
    this.lib.symbols.log_warn(msgPtr, fieldsPtr);
  }

  logInfo(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    const msgPtr = Deno.UnsafePointer.of(new TextEncoder().encode(message + '\0'));
    const fieldsPtr = fieldsJson 
      ? Deno.UnsafePointer.of(new TextEncoder().encode(fieldsJson + '\0'))
      : null;
    this.lib.symbols.log_info(msgPtr, fieldsPtr);
  }

  logDebug(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    const msgPtr = Deno.UnsafePointer.of(new TextEncoder().encode(message + '\0'));
    const fieldsPtr = fieldsJson 
      ? Deno.UnsafePointer.of(new TextEncoder().encode(fieldsJson + '\0'))
      : null;
    this.lib.symbols.log_debug(msgPtr, fieldsPtr);
  }

  logTrace(message: string, fields?: LogFields): void {
    const fieldsJson = fields ? JSON.stringify(fields) : null;
    const msgPtr = Deno.UnsafePointer.of(new TextEncoder().encode(message + '\0'));
    const fieldsPtr = fieldsJson 
      ? Deno.UnsafePointer.of(new TextEncoder().encode(fieldsJson + '\0'))
      : null;
    this.lib.symbols.log_trace(msgPtr, fieldsPtr);
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    const ptr = this.lib.symbols.get_ffi_dispatch_metrics();
    const view = new Deno.UnsafePointerView(ptr);
    const json = view.getCString();
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
    const platform = Deno.build.os;
    const ext = platform === 'darwin' ? 'dylib' : platform === 'windows' ? 'dll' : 'so';
    
    // Try common locations
    const paths = [
      `./target/debug/libtasker_worker.${ext}`,
      `./target/release/libtasker_worker.${ext}`,
      `/usr/local/lib/libtasker_worker.${ext}`,
    ];
    
    for (const path of paths) {
      try {
        Deno.statSync(path);
        return path;
      } catch {
        // File doesn't exist, try next
      }
    }
    
    throw new Error(
      'Could not find libtasker_worker. ' +
      'Set TASKER_LIBRARY_PATH or ensure library is in target/debug or target/release'
    );
  }
}

// FFI symbol definitions
const symbols = {
  // Step execution
  poll_step_events: {
    parameters: [],
    result: 'pointer',
  },
  complete_step_event: {
    parameters: ['pointer', 'pointer'],
    result: 'void',
  },
  
  // Worker lifecycle
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
  is_worker_running: {
    parameters: [],
    result: 'bool',
  },
  
  // Logging
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
  
  // FFI dispatch metrics
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
  
  // Memory management
  free_rust_string: {
    parameters: ['pointer'],
    result: 'void',
  },
} as const;
```

### 3. Update Runtime Factory

**File**: `src/ffi/runtime-factory.ts`

```typescript
import type { TaskerRuntime } from './runtime-interface.ts';
import { BunTaskerRuntime } from './bun-runtime.ts';
import { NodeTaskerRuntime } from './node-runtime.ts';
import { DenoTaskerRuntime } from './deno-runtime.ts';
import { detectRuntime, assertSupportedRuntime } from './runtime.ts';

let runtimeInstance: TaskerRuntime | null = null;

export function getTaskerRuntime(libraryPath?: string): TaskerRuntime {
  if (runtimeInstance) {
    return runtimeInstance;
  }

  const runtime = assertSupportedRuntime();

  if (runtime === 'deno') {
    runtimeInstance = new DenoTaskerRuntime(libraryPath);
  } else if (runtime === 'bun') {
    runtimeInstance = new BunTaskerRuntime(libraryPath);
  } else if (runtime === 'node') {
    runtimeInstance = new NodeTaskerRuntime(libraryPath);
  } else {
    throw new Error(`Unsupported runtime: ${runtime}`);
  }

  return runtimeInstance;
}
```

---

## Testing Strategy

### Unit Tests

**File**: `tests/unit/ffi/deno-runtime.test.ts`

Run with:
```bash
deno test --allow-ffi tests/unit/ffi/deno-runtime.test.ts
```

### Integration Tests

Add Deno to CI/CD pipeline:
```yaml
test-deno:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v3
    - uses: denoland/setup-deno@v1
      with:
        deno-version: v1.x
    - run: deno test --allow-ffi
```

---

## Benefits of Including Deno

1. **Broader Runtime Support**: Deno is growing in popularity, especially for edge computing
2. **Security Model**: Deno's explicit permissions align well with production security
3. **TypeScript Native**: No transpilation needed, better developer experience
4. **Edge Deployment**: Deno Deploy support opens serverless opportunities
5. **Minimal Cost**: ~2-3 hours of implementation time for full support

---

## Usage Examples

### Running with Deno

```bash
# Run worker with Deno
deno run --allow-ffi --allow-net --allow-read --allow-env bin/tasker-worker.ts --namespace payments

# Development mode with auto-reload
deno run --allow-ffi --allow-net --allow-read --allow-env --watch bin/tasker-worker.ts
```

### Permission Model

Deno requires explicit permissions:
- `--allow-ffi`: Required for FFI calls to Rust
- `--allow-net`: Required for health check/metrics HTTP server
- `--allow-read`: Required for reading handler files and configuration
- `--allow-env`: Required for environment variable configuration

---

## Documentation Updates

### README.md

Add Deno to supported runtimes:

```markdown
## Supported Runtimes

- **Bun** 1.0+ (recommended for development)
- **Node.js** 18+ (recommended for production)
- **Deno** 1.25+ (recommended for edge/serverless)
```

### Installation

```markdown
### Deno

```bash
# Run directly from URL (Deno-style)
deno run --allow-ffi https://deno.land/x/tasker_worker/mod.ts

# Or with deno.json
{
  "imports": {
    "@tasker-systems/worker": "https://deno.land/x/tasker_worker/mod.ts"
  }
}
```

---

## Recommendation

**Include Deno support in TAS-101** as part of the initial FFI bridge implementation. The implementation is nearly identical to Bun, and the incremental effort is minimal (~2-3 hours) while providing significant value:

- Expands target audience (Deno users)
- Demonstrates runtime flexibility
- Enables edge deployment scenarios
- Future-proofs the worker system

The only additional work is:
1. Create `src/ffi/deno-runtime.ts` (copy Bun, adjust API calls)
2. Update `runtime.ts` to detect Deno
3. Update `runtime-factory.ts` to instantiate `DenoTaskerRuntime`
4. Add Deno unit tests
5. Update documentation

**Total estimated effort**: 2-3 hours
**Value**: High (enables entire new deployment model)

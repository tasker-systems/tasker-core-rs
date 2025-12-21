# TypeScript Worker Threading and Event System Analysis

**Created**: 2025-12-20  
**Context**: TAS-100 TypeScript Worker Implementation  
**Related**: TAS-106 (Runtime Optimizations), `docs/worker-event-systems.md`

---

## Executive Summary

This document analyzes threading models and event systems for the TypeScript worker, comparing approaches used in Ruby (dry-events), Python (pyee), and Rust (mpsc channels), and evaluating whether lightweight threading (Worker Threads in Node.js, `spawn()` in Bun) would provide performance benefits for handler execution.

**Key Findings**:
1. **N-API** is an alternative to FFI but adds complexity without clear benefits for our use case
2. **EventEmitter is sufficient** for the main event loop coordination
3. **Worker Threads could provide value** for CPU-intensive handlers but require careful design
4. **Current single-threaded approach is correct** for initial implementation

---

## Part 1: N-API vs FFI

### What is N-API?

N-API (Node-API) is a C API for building native Node.js addons that are ABI-stable across Node.js versions.

**Traditional Native Addons**:
```cpp
// V8-specific code (breaks across Node versions)
#include <node.h>
#include <v8.h>

void MyFunction(const v8::FunctionCallbackInfo<v8::Value>& args) {
  v8::Isolate* isolate = args.GetIsolate();
  // V8 API calls...
}
```

**N-API Addons**:
```cpp
// ABI-stable code (works across Node versions)
#include <node_api.h>

napi_value MyFunction(napi_env env, napi_callback_info info) {
  // N-API calls (version-stable)
}
```

### N-API vs FFI Comparison

| Aspect | FFI (ffi-napi) | N-API Addon |
|--------|----------------|-------------|
| **Implementation** | Dynamic library loading | Compiled native module |
| **Memory Management** | Manual (allocate/free) | Automatic via N-API |
| **Cross-Runtime** | Works with Bun, Node, Deno | Node.js only |
| **Build Complexity** | No build step | Requires C++ compilation |
| **ABI Stability** | Not relevant | Stable across Node versions |
| **Performance** | Slightly slower (dynamic linking) | Slightly faster (direct linking) |
| **Type Safety** | Manual type conversion | Manual type conversion |
| **Best For** | Cross-runtime support | Node-only, complex APIs |

### Our Use Case Analysis

**Why FFI is Better for Us**:

1. **Cross-Runtime Requirement**: We support Bun and Node.js
   - FFI works with both
   - N-API is Node.js-only

2. **Simple API Surface**: Our FFI contract is straightforward
   - String passing (JSON serialization)
   - Pointer management
   - No complex C++ objects

3. **No Build Step**: FFI is pure JavaScript
   - Faster development cycle
   - No toolchain requirements
   - Works with any `libtasker_worker.{so,dylib,dll}`

4. **Maintenance Burden**: N-API requires C++ knowledge
   - FFI is TypeScript/JavaScript only
   - Easier for contributors

**When N-API Would Make Sense**:
- Node.js-only deployment
- Very high-frequency FFI calls (>10k/sec)
- Complex C++ object lifetimes
- Need for V8 garbage collector integration

### Recommendation

**Stick with FFI (ffi-napi for Node, dlopen for Bun)**

N-API adds complexity without meaningful benefits for our use case. Our FFI contract is already optimized (JSON over strings, minimal call frequency).

---

## Part 2: Event Systems Comparison

### Current Implementations

#### Ruby: dry-events
```ruby
class EventBridge
  include Dry::Events::Publisher[:tasker_core]
  
  # Register events
  register_event('step.execution.received')
  register_event('step.completion.sent')
  
  # Publish
  publish('step.execution.received', event)
  
  # Subscribe
  subscribe('step.execution.received') do |event|
    # Handler code
  end
end
```

**Features**:
- Thread-safe pub/sub
- Event schema registration
- Middleware support
- Ruby GIL handles synchronization

#### Python: pyee
```python
from pyee.base import EventEmitter

class EventBridge:
    def __init__(self):
        self._emitter = EventEmitter()
    
    def subscribe(self, event, handler):
        self._emitter.on(event, handler)
    
    def publish(self, event, *args):
        self._emitter.emit(event, *args)
```

**Features**:
- Node.js EventEmitter API
- Async/await support
- Simple, lightweight
- GIL handles synchronization

#### Rust: mpsc Channels + Actors
```rust
// Actor-based with bounded channels
let (tx, rx) = mpsc::channel(1000);

// Send
tx.send(WorkerCommand::ExecuteStep(step)).await?;

// Receive
while let Some(cmd) = rx.recv().await {
    process_command(cmd).await;
}
```

**Features**:
- Truly concurrent (no GIL)
- Bounded channels for backpressure
- Type-safe message passing
- Actor pattern for isolation

### TypeScript Options

#### Option 1: Node.js EventEmitter (Current)
```typescript
import { EventEmitter } from 'events';

const bridge = new EventEmitter();
bridge.on('step:execution:received', (event) => {
  // Handle event
});
bridge.emit('step:execution:received', event);
```

**Pros**:
- Built-in, no dependencies
- Familiar API
- Works in Bun and Node
- Single-threaded (no race conditions)

**Cons**:
- Not type-safe by default
- No backpressure control
- Synchronous by default

#### Option 2: eventemitter3 (Popular Alternative)
```typescript
import EventEmitter from 'eventemitter3';

const bridge = new EventEmitter();
// Same API as Node's EventEmitter
```

**Pros**:
- Faster than Node's EventEmitter
- Works in browsers
- Same API

**Cons**:
- External dependency
- Marginal performance gain for our use case

#### Option 3: Custom Channel System (Rust-like)
```typescript
class Channel<T> {
  private buffer: T[] = [];
  private maxSize: number;
  private waiting: Array<(value: T) => void> = [];
  
  async send(value: T): Promise<void> {
    if (this.buffer.length >= this.maxSize) {
      // Backpressure: wait for space
      await new Promise(resolve => this.waiting.push(resolve));
    }
    this.buffer.push(value);
  }
  
  async recv(): Promise<T> {
    if (this.buffer.length === 0) {
      // Wait for data
      return new Promise(resolve => {
        const check = setInterval(() => {
          if (this.buffer.length > 0) {
            clearInterval(check);
            resolve(this.buffer.shift()!);
          }
        }, 10);
      });
    }
    return this.buffer.shift()!;
  }
}
```

**Pros**:
- Explicit backpressure
- Type-safe
- Rust-like semantics

**Cons**:
- More code to maintain
- Overkill for single-threaded execution
- EventEmitter is simpler

### Recommendation

**Use Node.js EventEmitter (Current Approach)**

Reasons:
1. **Single-threaded execution**: No need for channel-style backpressure
2. **Simplicity**: EventEmitter is well-understood and works everywhere
3. **Type safety**: We can wrap it in strongly-typed methods (see TAS-104)
4. **Ruby/Python alignment**: Similar patterns to pyee/dry-events

If performance becomes an issue, consider `eventemitter3`, but current approach is fine.

---

## Part 3: Threading Models Analysis

### JavaScript Threading Options

#### Node.js Worker Threads
```typescript
import { Worker } from 'worker_threads';

const worker = new Worker('./handler-worker.js', {
  workerData: { stepEvent }
});

worker.on('message', (result) => {
  // Handler completed
});
```

**Characteristics**:
- Separate V8 isolate per worker
- ~2MB memory overhead per worker
- Message passing via `postMessage()` (structured clone)
- No shared memory by default (can use SharedArrayBuffer)
- Worker pool pattern typical

#### Bun spawn()
```typescript
const proc = Bun.spawn(['bun', 'run', 'handler.ts'], {
  stdin: 'pipe',
  stdout: 'pipe',
});

proc.stdin.write(JSON.stringify(stepEvent));
const result = await proc.stdout.text();
```

**Characteristics**:
- Lightweight subprocess (faster than Node)
- Process-level isolation
- ~1MB overhead
- Communication via stdio or IPC
- No shared memory

### Memory Model Considerations

**Key Constraint**: TaskerRuntime is NOT thread-safe

```typescript
// WRONG: Sharing TaskerRuntime across threads
const runtime = getTaskerRuntime(); // Main thread

const worker = new Worker('./handler.js');
worker.postMessage({ runtime }); // ERROR: Can't transfer
```

**Why**:
1. FFI pointers are process-specific
2. Rust `tokio::Runtime` is single-threaded
3. `libtasker_worker` assumes single-threaded access

**Correct Pattern**: Event-based signaling

```
Main Thread (with TaskerRuntime)    Worker Thread (Handler)
         │                                │
         │  postMessage(stepEvent)        │
         ├───────────────────────────────►│
         │                                │ handler.call()
         │                                │
         │  postMessage(result)           │
         │◄───────────────────────────────┤
         │                                │
         │ runtime.completeStepEvent()    │
         ▼                                │
```

This is **exactly the pattern** Python and Ruby use:
- Python: Main thread polls FFI, spawns async tasks for handlers
- Ruby: Main thread polls FFI, handlers run in GIL-protected threads

### Threading Architecture Proposal

```
┌─────────────────────────────────────────────────────────────────┐
│                        Main Thread                               │
│                                                                  │
│  ┌──────────────────┐         ┌─────────────────────┐           │
│  │ TaskerRuntime    │         │   EventEmitter      │           │
│  │                  │         │                     │           │
│  │ pollStepEvents() │────────►│ emit('step:event')  │           │
│  │                  │         │                     │           │
│  │completeStepEvent │◄────────│ on('step:complete') │           │
│  └──────────────────┘         └──────────┬──────────┘           │
│                                          │                       │
└──────────────────────────────────────────┼───────────────────────┘
                                           │
                    postMessage(stepEvent) │
                                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Worker Thread Pool                           │
│                                                                  │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐ │
│  │  Worker 1        │  │  Worker 2        │  │  Worker N     │ │
│  │                  │  │                  │  │               │ │
│  │ handler.call()   │  │ handler.call()   │  │handler.call() │ │
│  │      │           │  │      │           │  │      │        │ │
│  └──────┼───────────┘  └──────┼───────────┘  └──────┼────────┘ │
│         │                     │                     │           │
└─────────┼─────────────────────┼─────────────────────┼───────────┘
          │ postMessage(result) │                     │
          └─────────────────────┴─────────────────────┘
                           │
                           ▼
                     Main Thread
              runtime.completeStepEvent()
```

### Implementation Sketch

**Main Thread** (`src/events/EventPoller.ts`):
```typescript
class EventPoller {
  private workerPool: WorkerPool;
  
  private poll(): void {
    const runtime = getTaskerRuntime();
    const events = runtime.pollStepEvents();
    
    for (const event of events) {
      // Dispatch to worker thread
      this.workerPool.execute(event, (result) => {
        // Back on main thread
        runtime.completeStepEvent(event.event_id, result);
      });
    }
  }
}
```

**Worker Thread** (`src/workers/handler-worker.ts`):
```typescript
import { parentPort, workerData } from 'worker_threads';

parentPort.on('message', async (stepEvent: FfiStepEvent) => {
  try {
    const handler = resolveHandler(stepEvent);
    const context = StepContext.fromFfiEvent(stepEvent);
    const result = await handler.call(context);
    
    parentPort.postMessage({ success: true, result });
  } catch (error) {
    parentPort.postMessage({ success: false, error });
  }
});
```

### When Threading Makes Sense

**Benefits**:
1. **CPU-intensive handlers**: Image processing, data transformation
2. **Parallel execution**: Multiple handlers running simultaneously
3. **Isolation**: Handler crash doesn't kill main process

**Costs**:
1. **Complexity**: Worker pool management, message serialization
2. **Memory overhead**: ~2MB per worker
3. **Serialization cost**: JSON cloning for message passing
4. **Startup latency**: Worker creation takes ~50ms

### Benchmark Targets

Threading is worth it if:
- Handler execution > 100ms (amortizes overhead)
- High concurrency (>10 concurrent steps)
- CPU-bound work (not I/O-bound)

For I/O-bound handlers (API calls, database queries), single-threaded async is better.

---

## Recommendations

### Phase 1 (TAS-100): Single-Threaded
**Status**: Implement as specified

- Use Node.js EventEmitter for coordination
- Single-threaded async handler execution
- Matches Python/Ruby patterns
- Simpler, easier to debug

```typescript
// Main thread only
const runtime = getTaskerRuntime();
const events = runtime.pollStepEvents();

for (const event of events) {
  const handler = registry.resolve(event.handler_name);
  const result = await handler.call(context); // Async, but single-threaded
  runtime.completeStepEvent(event.event_id, result);
}
```

### Phase 2 (TAS-106): Optional Threading
**Status**: Future optimization

Add opt-in worker thread pool for CPU-intensive handlers:

```typescript
// Opt-in via config
const config = {
  workerThreads: {
    enabled: true,
    poolSize: 4,
    // Only use for these handlers
    handlers: ['ImageProcessingHandler', 'DataTransformHandler']
  }
};
```

Implementation:
1. Create worker pool abstraction
2. Route CPU-intensive handlers to workers
3. Keep I/O-bound handlers on main thread
4. Fallback to single-threaded if workers unavailable

### Phase 3 (Post-TAS-100): Bun-Specific Optimization
**Status**: Research

Investigate Bun's `spawn()` for handler isolation:
- Faster than Node Worker Threads
- Process-level isolation
- May require different architecture

---

## Conclusion

1. **N-API**: Not needed, FFI is sufficient and cross-runtime
2. **EventEmitter**: Current choice is correct, no need for custom channels
3. **Threading**: Start single-threaded, add worker threads as opt-in optimization
4. **Event Pattern**: Our approach matches Python (pyee) and Ruby (dry-events)

The single-threaded approach with async handlers is the right starting point. Worker threads can be added later for CPU-intensive workloads without changing the core architecture.

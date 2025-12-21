# TAS-100: Bun and Node.js TypeScript Worker

**Status**: Planning
**Branch**: `jcoletaylor/tas-100-typescript-worker-parent`
**Linear**: [TAS-100](https://linear.app/tasker-systems/issue/TAS-100)
**Priority**: Medium

## Overview

This parent ticket coordinates the implementation of a TypeScript worker for tasker-core using FFI (Foreign Function Interface) to match the proven patterns established in TAS-92 (Ruby) and TAS-95 (Python). The TypeScript worker will support both **Bun** and **Node.js** runtimes, providing a consistent developer experience across all four worker implementations (Rust, Ruby, Python, TypeScript).

**Pre-alpha status**: No backward compatibility required - we can evolve the API as needed.

## Strategic Context

### Why TypeScript/JavaScript?

The JavaScript/TypeScript ecosystem is ubiquitous in modern web development:
- **Bun**: Modern, fast runtime with excellent TypeScript support
- **Node.js**: Dominant runtime with massive ecosystem
- **Universal adoption**: Most teams have JavaScript expertise

### Why FFI Instead of WASM?

After analyzing WASM/WASI maturity (as of December 2025):

**WASM Limitations**:
- No mature PostgreSQL client for WASI
- No direct PGMQ integration without host functions
- Single-threaded execution model incompatible with our Tokio runtime
- Framework lock-in (Spin, Wasmer require specific APIs)
- Cannot port existing Axum web server or async patterns

**FFI Advantages**:
- **Pattern Consistency**: Matches Ruby (Magnus) and Python (PyO3)
- **Proven Architecture**: Dual-channel (dispatch + completion) works perfectly
- **Production Ready**: Bun FFI experimental but stabilizing; Node FFI mature
- **Direct Integration**: Same `poll_step_events()` / `complete_step_event()` contract
- **Implementation Speed**: 2-3 weeks vs 2-3 months for WASM

**WASM Future**: TAS-109 reserved for WASM research when WASI 0.3+ matures. The vision of serverless WASM handlers (TAS-150+) remains valid for stateless, compute-heavy workflows.

## Analysis

See [analysis.md](./analysis.md) for detailed comparison of FFI vs WASM approaches, runtime landscape, and architectural decisions.

## Child Tickets

| Ticket | Title | Priority | Status |
|--------|-------|----------|--------|
| [TAS-101](./TAS-101-ffi-bridge-and-infrastructure.md) | TypeScript Worker FFI Bridge and Core Infrastructure | High | Todo |
| [TAS-102](./TAS-102-handler-api-and-registry.md) | TypeScript Handler API and Registry | High | Todo |
| [TAS-103](./TAS-103-specialized-handlers.md) | TypeScript Specialized Handlers | Medium | Todo |
| [TAS-104](./TAS-104-server-and-bootstrap.md) | TypeScript Worker Server and Bootstrap | Medium | Todo |
| [TAS-105](./TAS-105-testing-and-examples.md) | TypeScript Testing and Examples | Medium | Todo |
| [TAS-106](./TAS-106-runtime-optimizations.md) | Runtime-Specific Optimizations (Optional) | Low | Todo |
| [TAS-107](./TAS-107-documentation.md) | TypeScript Worker Documentation | Low | Todo |

## Execution Order

The recommended execution order (following TAS-92 pattern):

1. **TAS-101** (FFI Bridge) - Foundation layer, runtime detection
2. **TAS-102** (Handler API) - Core handler patterns aligned with TAS-92
3. **TAS-103** (Specialized Handlers) - ApiHandler, DecisionHandler, Batchable
4. **TAS-104** (Server/Bootstrap) - Entry points and lifecycle management
5. **TAS-105** (Testing) - Comprehensive test coverage
6. **TAS-106** (Optimizations) - Optional runtime-specific enhancements
7. **TAS-107** (Documentation) - Must be done LAST after all code changes

## Architecture Overview

### FFI-Based Design

```
┌─────────────────────────────────────────────────────────────┐
│              TYPESCRIPT WORKER ARCHITECTURE                  │
└─────────────────────────────────────────────────────────────┘

                  tasker-worker (Rust core)
                           │
                           │ FFI boundary
                           ▼
          ┌────────────────────────────────┐
          │   FFI Adapter (runtime-agnostic)│
          │   ├─ poll_step_events()         │
          │   ├─ complete_step_event()      │
          │   ├─ bootstrap_worker()         │
          │   └─ get_worker_status()        │
          └────────────────────────────────┘
                    │           │
        ┌───────────┴───────────┴──────────┐
        │                                  │
        ▼                                  ▼
  ┌──────────┐                      ┌──────────┐
  │   Bun    │                      │   Node   │
  │  dlopen  │                      │  ffi-napi│
  └──────────┘                      └──────────┘
        │                                  │
        └───────────┬──────────────────────┘
                    │
                    ▼
         ┌─────────────────────┐
         │  EventPoller        │
         │  (10ms polling)     │
         └─────────────────────┘
                    │
                    ▼
         ┌─────────────────────┐
         │  EventEmitter       │
         │  (Node compat)      │
         └─────────────────────┘
                    │
                    ▼
         ┌─────────────────────┐
         │  HandlerRegistry    │
         └─────────────────────┘
                    │
                    ▼
         ┌─────────────────────┐
         │  TypeScript Handler │
         │  handler.call(ctx)  │
         └─────────────────────┘
```

### Runtime Adapter Pattern

The worker will support multiple runtimes through a unified adapter:

```typescript
// workers/typescript/src/ffi/adapter.ts
export interface FfiAdapter {
  pollStepEvents(): FfiStepEvent | null;
  completeStepEvent(eventId: string, result: string): void;
  bootstrapWorker(config: string): string;
  getWorkerStatus(): string;
  // ... other FFI methods
}

// Auto-detect runtime and select adapter
const adapter: FfiAdapter = detectRuntime() === 'bun' 
  ? new BunFfiAdapter() 
  : new NodeFfiAdapter();
```

## Key Alignment Goals (TAS-92 Consistency)

### A. Handler Signatures → `call(context)`

| Language | Current | Target |
|----------|---------|--------|
| Ruby | `call(task, sequence, step)` → `call(context)` | TAS-96 ✅ |
| Python | `call(context)` | Already aligned ✅ |
| Rust | `call(&TaskSequenceStep)` | Already aligned ✅ |
| **TypeScript** | N/A | `call(context: StepContext)` |

### B. Result Factories → `success()` / `failure()`

| Language | Success Method | Failure Method |
|----------|----------------|----------------|
| Ruby | `success(result:, metadata:)` | `failure(message:, error_type:, ...)` |
| Python | `success(result, metadata)` | `failure(message, error_type, ...)` |
| Rust | `StepExecutionResult::success(...)` | `StepExecutionResult::failure(...)` |
| **TypeScript** | `success(result, metadata?)` | `failure(message, errorType, ...)` |

### C. Registry API Parity

Target methods for all languages:
- `register(name, handlerClass)`
- `isRegistered(name) -> boolean`
- `resolve(name) -> handlerInstance`
- `listHandlers() -> string[]`

### D. Specialized Handlers

- **API Handler**: `get/post/put/delete` convenience methods
- **Decision Handler**: `decisionSuccess(steps, routingContext)` helper
- **Batchable**: `batchWorkerSuccess()` and `getBatchContext()`

### E. Error Fields

Standardize across all languages:
- `errorMessage`, `errorType`, `errorCode` (optional), `retryable`
- Recommended `errorType` values: `permanent_error`, `retryable_error`, `validation_error`, `timeout`, `handler_error`

## Directory Structure

```
workers/typescript/
├── package.json                  # Dependencies and scripts
├── tsconfig.json                 # TypeScript configuration
├── bun.lockb / package-lock.json # Lock files
├── bin/
│   └── server.ts                 # Server entry point (TAS-104)
├── src/
│   ├── index.ts                  # Public API exports
│   ├── ffi/
│   │   ├── adapter.ts            # Runtime adapter interface (TAS-101)
│   │   ├── bun.ts                # Bun dlopen implementation (TAS-101)
│   │   ├── node.ts               # Node ffi-napi implementation (TAS-101)
│   │   └── types.ts              # FFI type definitions (TAS-101)
│   ├── bootstrap/
│   │   ├── bootstrap.ts          # Worker bootstrap (TAS-104)
│   │   └── config.ts             # Configuration types (TAS-104)
│   ├── events/
│   │   ├── event-emitter.ts      # EventEmitter wrapper (TAS-101)
│   │   ├── event-names.ts        # Event constants (TAS-101)
│   │   └── event-poller.ts       # FFI polling (TAS-101)
│   ├── handler/
│   │   ├── base.ts               # Base handler (TAS-102)
│   │   ├── registry.ts           # Handler registry (TAS-102)
│   │   ├── api.ts                # API handler (TAS-103)
│   │   ├── decision.ts           # Decision handler (TAS-103)
│   │   └── batchable.ts          # Batch mixin (TAS-103)
│   ├── types/
│   │   ├── context.ts            # StepContext (TAS-102)
│   │   ├── result.ts             # StepHandlerResult (TAS-102)
│   │   ├── errors.ts             # Error types (TAS-102)
│   │   └── index.ts              # Type exports
│   └── subscriber/
│       └── step-execution.ts     # Step subscriber (TAS-102)
├── tests/
│   ├── unit/                     # Unit tests (TAS-105)
│   ├── integration/              # Integration tests (TAS-105)
│   └── fixtures/                 # Test fixtures (TAS-105)
└── examples/
    └── handlers/                 # Example handlers (TAS-105)
```

## Runtime Support Matrix

| Runtime | FFI Library | Status | Priority |
|---------|-------------|--------|----------|
| **Bun** | `bun:ffi` | Experimental with known bugs | High (Phase 1) |
| **Node.js** | `node-ffi-napi` | Stable, widely used | High (Phase 1) |
| **Deno** | `Deno.dlopen` | Production-ready as of 1.25+ | Low (TAS-108 or later) |

### Bun Considerations

- FFI is experimental in Bun 1.x
- Known limitations documented at https://bun.com/docs/runtime/ffi
- Fast-moving project, FFI stability improving rapidly
- Excellent TypeScript support and performance

### Node.js Considerations

- `node-ffi-napi` is mature and battle-tested
- Wider adoption and ecosystem
- Slower runtime than Bun but proven reliability

## Dependencies

### Core Dependencies

```json
{
  "dependencies": {
    "eventemitter3": "^5.0.1",  // EventEmitter (Node compat)
    "@ltd/j-toml": "^1.38.0"    // TOML config parsing (optional)
  },
  "devDependencies": {
    "typescript": "^5.3.0",
    "bun-types": "^1.0.0",       // Bun type definitions
    "@types/node": "^20.0.0",    // Node type definitions
    "vitest": "^1.0.0"           // Testing framework
  },
  "peerDependencies": {
    "ffi-napi": "^4.0.3"         // For Node.js runtime
  }
}
```

### FFI Dependencies

**Bun**: Built-in `bun:ffi` module
**Node**: Requires `ffi-napi` package

## Configuration

TypeScript worker will use the same TOML configuration as Ruby/Python:

```toml
# config/tasker/base/worker.toml
[worker]
namespace = "default"
polling_interval_ms = 10
starvation_check_interval = 100
cleanup_interval = 1000

[worker.mpsc_channels.ffi_dispatch]
dispatch_buffer_size = 1000
completion_timeout_ms = 30000
starvation_warning_threshold_ms = 10000

[web]
enabled = false  # Headless mode for embedded usage
```

## Validation Plan

See [validation-plan.md](./validation-plan.md) for the comprehensive verification checklist to be executed after all child tickets are complete.

## Open Questions

| Question | Status |
|----------|--------|
| Package distribution: Single `@tasker/worker` or separate `@tasker/worker-bun` / `@tasker/worker-node`? | **Decision: Single package with runtime detection** |
| TypeScript vs JavaScript: TypeScript-first with compiled JS, or dual-mode? | **Decision: TypeScript-first, compile to both ESM and CommonJS** |
| Deno support: Include in TAS-100 or defer? | **Decision: Defer to TAS-108** |
| EventEmitter library: Native Node.js EventEmitter or third-party? | **Decision: eventemitter3 for consistency across runtimes** |

## Files Structure

```
docs/ticket-specs/TAS-100/
├── README.md                                    # This file
├── analysis.md                                  # FFI vs WASM analysis
├── TAS-101-ffi-bridge-and-infrastructure.md    # FFI adapter layer
├── TAS-102-handler-api-and-registry.md         # Handler API
├── TAS-103-specialized-handlers.md             # Specialized handlers
├── TAS-104-server-and-bootstrap.md             # Server and bootstrap
├── TAS-105-testing-and-examples.md             # Testing strategy
├── TAS-106-runtime-optimizations.md            # Optional optimizations
├── TAS-107-documentation.md                    # Documentation updates
└── validation-plan.md                           # Final validation checklist
```

## Success Criteria

- [ ] TypeScript worker supports Bun runtime via `bun:ffi`
- [ ] TypeScript worker supports Node.js runtime via `ffi-napi`
- [ ] Handler API matches TAS-92 alignment (`call(context)`, `success()`, `failure()`)
- [ ] Specialized handlers (API, Decision, Batchable) implemented
- [ ] EventPoller with 10ms polling interval working
- [ ] EventEmitter integration for step coordination
- [ ] Server mode with graceful shutdown
- [ ] Comprehensive test suite (unit + integration)
- [ ] Example handlers demonstrating all patterns
- [ ] Documentation in `docs/worker-crates/typescript.md`
- [ ] Cross-language comparison matrix updated

## Risk Assessment

**Medium Risk**:
- Bun FFI is experimental with known bugs
- Node FFI is mature but adds dependency overhead
- Runtime detection adds complexity
- TypeScript compilation target compatibility

**Mitigation**:
- Start with Node.js for stability, add Bun as enhancement
- Comprehensive test coverage across both runtimes
- Runtime adapter pattern isolates FFI differences
- Clear documentation of runtime-specific limitations

## Estimated Scope

- **Phase 1 (TAS-101-102)**: ~1-2 weeks (FFI bridge + handler API)
- **Phase 2 (TAS-103-104)**: ~1 week (specialized handlers + server)
- **Phase 3 (TAS-105-107)**: ~1 week (testing + documentation)
- **Total**: ~3-4 weeks for full implementation

## Future Work (Out of Scope)

- **TAS-108**: Deno runtime support
- **TAS-109**: WASM worker research (WASI 0.3+ evaluation)
- **TAS-150+**: Serverless WASM handler runtime for compute-heavy workflows

---

**Next Steps**: Review this plan, then proceed with TAS-101 (FFI Bridge) implementation.

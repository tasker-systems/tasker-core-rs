# TAS-100 Analysis: FFI vs WASM for TypeScript Worker

**Last Updated**: 2024-12-19
**Status**: Decision Made - FFI Approach

## Executive Summary

After analyzing WASM/WASI maturity and comparing it to the proven FFI pattern from Ruby/Python workers, **we are proceeding with FFI for TAS-100**. WASM will be revisited in TAS-109 when WASI 0.3+ stabilizes, with a future vision for serverless WASM handlers (TAS-150+) for compute-intensive workflows.

## Analysis Approach

We evaluated three dimensions:
1. **Technical Feasibility**: Can we implement it with current tooling?
2. **Pattern Consistency**: Does it match our Ruby/Python architecture?
3. **Production Readiness**: Is it stable enough for real workloads?

---

## FFI Approach Analysis

### Technical Architecture

```
┌──────────────────────────────────────────────────────────┐
│          FFI WORKER ARCHITECTURE (PROVEN)                 │
└──────────────────────────────────────────────────────────┘

  TypeScript/JavaScript Runtime (Bun / Node.js)
              │
              │ FFI calls (C ABI)
              ▼
  ┌────────────────────────────────────┐
  │   libtasker_worker.dylib/.so       │
  │   ─────────────────────────────    │
  │   Exported Functions:              │
  │   • poll_step_events()             │
  │   • complete_step_event()          │
  │   • bootstrap_worker()             │
  │   • get_worker_status()            │
  └────────────────────────────────────┘
              │
              ▼
  tasker-worker Rust crate (full features)
  ├─ Tokio async runtime
  ├─ PostgreSQL/PGMQ integration
  ├─ HandlerDispatchService
  ├─ CompletionProcessorService
  └─ Domain event system
```

### Advantages

**1. Pattern Consistency** (Highest Priority)
- Ruby uses Magnus FFI → same `poll_step_events()` contract ✅
- Python uses PyO3 FFI → same completion flow ✅
- TypeScript FFI → identical pull-based polling ✅
- Single Rust codebase serves all four workers

**2. Production Readiness**
- **Node.js**: `node-ffi-napi` is mature (10+ years, widely deployed)
- **Bun**: FFI experimental but fast-stabilizing, Bun team prioritizing it
- **Deno**: `Deno.dlopen` production-ready since 1.25 (2022)
- All use same C ABI, minimal runtime-specific code

**3. Implementation Speed**
- Estimated 2-3 weeks for full TypeScript worker
- Reuses existing `libtasker_worker` dylib
- No new compile targets or Rust refactoring
- Well-understood patterns from Ruby/Python

**4. Full Feature Access**
- Direct PostgreSQL connection via existing Rust code
- PGMQ queue operations (atomic claiming, visibility)
- Tokio async runtime for multi-threaded dispatch
- HandlerDispatchService with semaphore-bounded concurrency
- Domain event system with fire-and-forget guarantees

**5. Debugging and Observability**
- Standard debugging tools (lldb, gdb for FFI boundary)
- Structured logging across Rust ↔ TypeScript boundary
- Familiar stack traces on both sides
- Performance profiling with native tools

### Disadvantages

**1. FFI Safety Concerns**
- Unsafe boundary: incorrect types = segfaults
- Memory management responsibility (allocation/deallocation)
- Null pointer handling

**Mitigation**: Type-safe wrapper layer, comprehensive tests, validated contracts

**2. Runtime Compatibility**
- Different FFI semantics between Bun and Node
- Bun FFI experimental with known bugs
- Need runtime detection and adapter pattern

**Mitigation**: Adapter interface isolates differences, test coverage per runtime

**3. Dependency on Native Libraries**
- Must distribute compiled `.dylib` / `.so` / `.dll` per platform
- Platform-specific builds (macOS, Linux, Windows)
- Larger package size

**Mitigation**: Pre-built binaries via GitHub releases, optional compilation

### FFI Implementation Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Bun FFI instability | Medium | Start with Node.js, add Bun incrementally |
| Platform build matrix | Low | Use GitHub Actions for multi-platform builds |
| Type safety at boundary | Medium | Comprehensive FFI wrapper with validation |
| Segfaults in development | Low | Strict contract enforcement, extensive testing |

---

## WASM Approach Analysis

### Technical Architecture

```
┌──────────────────────────────────────────────────────────┐
│         WASM WORKER ARCHITECTURE (FUTURE)                 │
└──────────────────────────────────────────────────────────┘

  TypeScript/JavaScript Runtime
              │
              │ WebAssembly instance
              ▼
  ┌────────────────────────────────────┐
  │   tasker_worker.wasm               │
  │   ─────────────────────────────    │
  │   Compiled from Rust to wasm32     │
  │   WASI imports required:           │
  │   • sock_open (networking)         │
  │   • fd_read/fd_write (I/O)         │
  │   • poll_oneoff (async)            │
  └────────────────────────────────────┘
              │
              │ BUT: No direct PostgreSQL!
              ▼
  Host Functions (JS-provided)
  ├─ pgmq_poll_messages() ← We must implement
  ├─ pgmq_send_message()  ← We must implement
  ├─ async runtime shim   ← Tokio won't work
  └─ domain events shim   ← Need JS bridge
```

### Advantages

**1. Sandboxed Execution**
- Handlers run in isolated WASM sandbox
- Memory safety guaranteed by runtime
- No segfaults from user code

**2. Portability**
- Single `.wasm` binary runs everywhere
- No platform-specific builds
- Smaller package size (WASM is compact)

**3. Fast Cold Starts**
- WASM instantiation: ~1-5ms
- Could enable serverless handler execution (future TAS-150+)

**4. Future-Proof**
- WASM/WASI is the future of portable compute
- Component model will improve interop
- Growing ecosystem

### Disadvantages (Deal-Breakers for TAS-100)

**1. No Mature PostgreSQL Client**
- `wasm32-wasi` target lacks production Postgres drivers
- `tokio-postgres` doesn't compile to WASM (epoll, kqueue dependencies)
- Would need pure-WASI socket implementation

**Workaround Required**: Host functions for all database operations
- `pgmq_poll()`, `pgmq_send()`, `pg_query()` etc.
- Defeats the purpose of Rust worker - logic split between WASM and host

**2. Single-Threaded Execution**
- WASM is single-threaded (threads proposal not stabilized)
- Our `HandlerDispatchService` relies on Tokio's multi-threaded runtime
- Semaphore-bounded concurrency wouldn't work

**Workaround Required**: Rewrite orchestration for single-threaded async
- Major refactoring of `tasker-worker` crate
- Lose performance benefits of parallel handler execution

**3. WASI Maturity Gap**
- **WASI Preview 1** (current): No async, no sockets
- **WASI Preview 2** (2023): Async model exists, but adoption low
- **WASI 0.3+** (future): Will stabilize networking, still evolving

**Ecosystem Status**:
- Bun WASI support: Limited, focuses on pure compute
- Node WASI support: Basic, not production-tested for networking
- Deno WASI support: Better, but still experimental for async I/O

**4. Framework Lock-In**
- **Spin Framework**: Requires Spin APIs, can't use Axum/Tower
- **Wasmer**: Different host function semantics
- **Wasmtime**: Another set of host APIs

To avoid lock-in, we'd need to write our own host function layer anyway.

**5. Async Runtime Incompatibility**
- Tokio doesn't compile to `wasm32-wasi` target
- Would need WASI-compatible async runtime (not production-ready)
- Or poll-based state machine (massive refactoring)

**6. Development Complexity**
- Debug WASM with limited tooling (wasm-objdump, wasmtime)
- Stack traces harder to interpret
- Performance profiling immature

### WASM Implementation Risks

| Risk | Severity | Impact |
|------|----------|--------|
| No Postgres client for WASI | **Critical** | Must implement host functions for all DB ops |
| Single-threaded limitation | **High** | Lose parallel handler execution |
| Tokio incompatibility | **High** | Rewrite async runtime layer |
| WASI instability | **Medium** | Preview 2 adoption still low |
| Debugging difficulty | **Medium** | Slower development iteration |
| Framework lock-in | **Medium** | Can't reuse Axum/Tower patterns |

---

## Decision Matrix

| Criteria | FFI | WASM | Winner |
|----------|-----|------|--------|
| **Pattern Consistency** | ✅ Matches Ruby/Python | ❌ Requires new architecture | FFI |
| **Production Readiness** | ✅ Node FFI mature, Bun stabilizing | ❌ WASI networking immature | FFI |
| **Implementation Speed** | ✅ 2-3 weeks | ❌ 2-3 months + unknowns | FFI |
| **PostgreSQL Access** | ✅ Native via Rust | ❌ Needs host functions | FFI |
| **Multi-threading** | ✅ Full Tokio support | ❌ Single-threaded WASM | FFI |
| **Async Runtime** | ✅ Tokio works | ❌ Incompatible | FFI |
| **Debugging** | ✅ Standard tools | ⚠️ Limited tooling | FFI |
| **Portability** | ⚠️ Platform builds | ✅ Single binary | WASM |
| **Sandboxing** | ❌ Unsafe boundary | ✅ Guaranteed safe | WASM |
| **Future Serverless** | ❌ Not ideal | ✅ Perfect fit | WASM |

**Score**: FFI 8/10, WASM 3/10 for **current requirements** (TAS-100)

---

## Runtime Landscape

### Bun FFI

**Status**: Experimental (Bun 1.x)
**Documentation**: https://bun.com/docs/runtime/ffi

**Capabilities**:
- `dlopen()` for loading shared libraries
- C type mapping (i8, u32, pointer, buffer, etc.)
- Function pointers and callbacks
- Struct passing by value

**Limitations**:
- Experimental with known bugs (documented in Bun tracker)
- Callback thread safety issues
- Pointer alignment edge cases

**Production Use**: Use with caution, test extensively

**Example**:
```typescript
import { dlopen, FFIType, suffix } from "bun:ffi";

const lib = dlopen(`libtasker_worker.${suffix}`, {
  poll_step_events: {
    args: [],
    returns: FFIType.ptr, // JSON string pointer or null
  },
  complete_step_event: {
    args: [FFIType.cstring, FFIType.cstring],
    returns: FFIType.void,
  },
});
```

### Node.js FFI

**Status**: Mature (via `node-ffi-napi`)
**Repository**: https://github.com/node-ffi/node-ffi

**Capabilities**:
- Proven in production (10+ years)
- Full C type support
- Callback support (JS → C)
- Buffer and pointer management

**Limitations**:
- Performance overhead vs native calls
- Requires compilation (node-gyp, prebuild)
- Memory management manual

**Production Use**: Battle-tested, widely deployed

**Example**:
```typescript
import ffi from 'ffi-napi';
import ref from 'ref-napi';

const lib = ffi.Library('libtasker_worker', {
  poll_step_events: ['pointer', []],
  complete_step_event: ['void', ['string', 'string']],
});
```

### Deno FFI

**Status**: Production-ready (since Deno 1.25+)
**Documentation**: https://docs.deno.com/runtime/fundamentals/ffi/

**Capabilities**:
- Native integration (no third-party package)
- Performance-optimized
- Type-safe with TypeScript
- Struct support, callbacks

**Limitations**:
- Requires `--allow-ffi` flag (security)
- Deno ecosystem smaller than Node

**Production Use**: Recommended for Deno users

**Example**:
```typescript
const lib = Deno.dlopen("libtasker_worker.dylib", {
  poll_step_events: { parameters: [], result: "pointer" },
  complete_step_event: { parameters: ["buffer", "buffer"], result: "void" },
});
```

---

## Recommendation

**Proceed with FFI for TAS-100** with the following implementation strategy:

### Phase 1: Node.js (Stable Foundation)
- Start with `node-ffi-napi` for proven reliability
- Comprehensive test coverage
- Reference implementation for Bun

### Phase 2: Bun (Performance Enhancement)
- Add Bun FFI adapter after Node.js validation
- Document known limitations
- Feature flag for Bun-specific optimizations

### Phase 3: Deno (Future)
- Defer to TAS-108 if demand exists
- Straightforward port from Node/Bun

### Future: WASM Research (TAS-109)
Reserve TAS-109 for WASM feasibility study when:
- WASI 0.3+ stabilizes with networking
- Production Postgres client available for `wasm32-wasi`
- Async WASM (component model) matures

### Vision: Serverless WASM Handlers (TAS-150+)
Long-term goal (6-12 months post-TAS-100):
- Compile **individual handlers** to WASM
- Deploy to serverless platforms (AWS Lambda, Cloudflare Workers)
- Cold start optimization (1ms vs 100ms)
- Extreme scalability for compute-heavy workflows

**Separation of concerns**:
- **Orchestration**: Stays Rust (PostgreSQL, PGMQ, state machines)
- **Handlers**: Optionally WASM (stateless compute units)

---

## Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Is Bun FFI stable enough? | **No**, but improving fast. Start with Node, add Bun in parallel. |
| Can we do PGMQ in WASM? | **No**, no mature client. Would need host functions (defeats purpose). |
| Should we wait for WASI 0.3? | **No**, timeline uncertain (6+ months). FFI works today. |
| What about Spin framework? | **Framework lock-in**. Requires Spin APIs, can't reuse Axum patterns. |

---

## References

- Ruby Worker: `workers/ruby/lib/tasker_core/bootstrap.rb`
- Python Worker: `workers/python/python/tasker_core/bootstrap.py`
- Bun FFI Docs: https://bun.com/docs/runtime/ffi
- Node FFI: https://github.com/node-ffi/node-ffi
- Deno FFI: https://docs.deno.com/runtime/fundamentals/ffi/
- Spin Framework: https://spinframework.dev (WASM framework example)
- WASI Status: https://github.com/WebAssembly/WASI (Preview 2 adoption tracker)

---

**Next Steps**: Proceed with TAS-101 (FFI Bridge implementation).

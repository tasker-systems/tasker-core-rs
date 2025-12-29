# ADR: FFI Over WASM for Language Workers

**Status**: Accepted
**Date**: 2025-12
**Ticket**: [TAS-100](https://linear.app/tasker-systems/issue/TAS-100)

## Context

For the TypeScript worker implementation, we needed to decide between two integration approaches:
1. **FFI (Foreign Function Interface)**: Direct C ABI calls to compiled Rust library
2. **WASM (WebAssembly)**: Compile Rust to wasm32-wasi target

Ruby (Magnus) and Python (PyO3) workers already used FFI successfully.

## Decision

**Proceed with FFI for all language workers.** Reserve WASM for future serverless handler execution (TAS-150+).

**Decision Matrix**:

| Criteria | FFI | WASM |
|----------|-----|------|
| Pattern Consistency | Matches Ruby/Python | Requires new architecture |
| Production Readiness | Node FFI mature, Bun stabilizing | WASI networking immature |
| Implementation Speed | 2-3 weeks | 2-3 months + unknowns |
| PostgreSQL Access | Native via Rust | Needs host functions |
| Multi-threading | Full Tokio support | Single-threaded WASM |
| Async Runtime | Tokio works | Incompatible |
| Debugging | Standard tools | Limited tooling |

**Score**: FFI 8/10, WASM 3/10 for current requirements.

**WASM Deal-Breakers**:
1. No mature PostgreSQL client for `wasm32-wasi`
2. Single-threaded execution (our `HandlerDispatchService` relies on Tokio multi-threading)
3. Tokio doesn't compile to `wasm32-wasi` target
4. WASI networking still experimental (Preview 2 adoption low)

## Consequences

### Positive

- **Pattern consistency**: Single Rust codebase serves all four workers
- **Proven approach**: Ruby/Python FFI already validated
- **Full feature access**: PostgreSQL, PGMQ, Tokio, domain events all work
- **Standard debugging**: lldb, gdb, structured logging across boundary
- **Fast implementation**: Estimated 2-3 weeks for TypeScript worker

### Negative

- **FFI safety concerns**: Incorrect types can cause segfaults
- **Platform builds**: Must distribute `.dylib`/`.so`/`.dll` per platform
- **Runtime compatibility**: Different FFI semantics between Bun and Node

### Neutral

- Bun FFI experimental but fast-stabilizing
- Pre-built binaries via GitHub releases address distribution

## Future Vision

**WASM Research (TAS-109)**: Revisit when WASI 0.3+ stabilizes with networking.

**Serverless WASM Handlers (TAS-150+)**:
- Compile individual handlers to WASM (not orchestration)
- Deploy to serverless platforms (AWS Lambda, Cloudflare Workers)
- Cold start optimization (1ms vs 100ms)
- Extreme scalability for compute-heavy workflows

**Separation of Concerns**:
- **Orchestration**: Stays Rust (PostgreSQL, PGMQ, state machines)
- **Handlers**: Optionally WASM (stateless compute units)

## Alternatives Considered

### Alternative 1: WASM with Host Functions

Implement database operations as host functions.

**Rejected**: Defeats the purpose - logic split between WASM and host, loses Rust benefits.

### Alternative 2: Wait for WASI 0.3

Delay TypeScript worker until WASI matures.

**Rejected**: Timeline uncertain (6+ months); FFI works today.

### Alternative 3: Spin Framework

Use Spin's WASM abstraction layer.

**Rejected**: Framework lock-in; requires Spin APIs, can't reuse Axum/Tower patterns.

## References

- [TAS-100 Specification](../ticket-specs/TAS-100/analysis.md) - Full analysis
- [Cross-Language Consistency](../principles/cross-language-consistency.md) - API philosophy
- [Workers Documentation](../workers/) - Language-specific implementation guides

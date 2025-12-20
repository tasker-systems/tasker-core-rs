# TAS-106: Runtime-Specific Optimizations (Optional)

**Parent**: [TAS-100](./README.md)
**Linear**: [TAS-106](https://linear.app/tasker-systems/issue/TAS-106)
**Branch**: `jcoletaylor/tas-106-typescript-runtime-optimizations`
**Priority**: Low
**Status**: Todo (Optional)
**Depends On**: TAS-101, TAS-102, TAS-104

## Objective

Optional runtime-specific optimizations leveraging Bun-specific APIs and Node.js performance features. This ticket is **optional** and can be deferred if time-constrained.

**IMPORTANT**: See `threading-analysis.md` for detailed analysis of:
- N-API vs FFI (conclusion: stick with FFI)
- EventEmitter vs custom channels (conclusion: EventEmitter is sufficient)
- Worker Threads for parallel execution (conclusion: opt-in for CPU-intensive handlers)

## Bun-Specific Optimizations

### 1. Bun.file() for Fast File I/O
- Zero-copy file reading for handlers that process files
- Faster than `fs.readFile()` in Node

### 2. Bun.spawn() for Subprocess Handlers
- Native subprocess spawning (faster than `child_process`)
- Useful for handlers that shell out to external tools

### 3. Bun.serve() for Embedded HTTP
- Fast HTTP server for handlers that need to expose webhooks
- Alternative to Express/Fastify

### 4. FFI Performance Tuning
- Investigate Bun FFI callback optimizations
- Memory pooling for frequent FFI calls

## Node.js-Specific Optimizations

### 1. Worker Threads (Opt-in)
- Offload CPU-intensive handlers to worker threads
- Keep main thread free for FFI polling
- Pattern: Main thread owns TaskerRuntime, workers execute handlers
- Communication via postMessage (event-based)
- See `threading-analysis.md` for architecture proposal

### 2. Native Addons (NOT RECOMMENDED)
- N-API wrapper would be Node.js-only (breaks Bun compatibility)
- FFI is sufficient for our use case
- See `threading-analysis.md` for detailed N-API vs FFI comparison

### 3. Performance Hooks
- Use `perf_hooks` for detailed handler profiling
- Integrate with APM tools (DataDog, New Relic)

## Files to Create

```
src/
├── runtime/
│   ├── bun/
│   │   ├── file-handler.ts      # Bun.file() helpers
│   │   ├── subprocess.ts        # Bun.spawn() helpers
│   │   └── http-server.ts       # Bun.serve() helpers
│   └── node/
│       ├── worker-threads.ts    # Worker thread helpers
│       └── perf-hooks.ts        # Performance profiling
```

## Research Needed

1. Bun-specific API stability (experimental features)
2. Performance benchmarks: Bun vs Node for worker operations
3. Worker thread overhead vs benefits for handler execution
4. FFI call frequency analysis (is optimization needed?)

## Success Criteria

- [ ] Bun optimizations provide measurable performance gain (>10%)
- [ ] Node.js optimizations don't break compatibility
- [ ] Runtime-specific features are opt-in (not required)
- [ ] Documentation explains when to use each optimization
- [ ] Benchmarks demonstrate improvements

## Estimated Scope

~1-2 days (10-15 hours) - **Optional, defer if needed**

---

**Note**: This ticket is **optional** and can be tackled post-TAS-100 if time allows. Focus on TAS-101-105 first.

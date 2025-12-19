# TAS-104: TypeScript Worker Server and Bootstrap

**Parent**: [TAS-100](./README.md)
**Linear**: [TAS-104](https://linear.app/tasker-systems/issue/TAS-104)
**Branch**: `jcoletaylor/tas-104-typescript-server-bootstrap`
**Priority**: Medium
**Status**: Todo
**Depends On**: TAS-102

## Objective

Create the server entry point and bootstrap process for TypeScript workers, supporting both server mode and headless/embedded mode. Includes graceful shutdown and signal handling.

## Bootstrap Process (matches Ruby/Python)

1. Initialize FFI adapter (runtime detection)
2. Create EventEmitter and EventPoller
3. Bootstrap Rust worker via FFI
4. Start StepExecutionSubscriber
5. Register signal handlers (SIGTERM, SIGINT)
6. Main loop with periodic health checks
7. Graceful shutdown on signal

## Reference Implementations

**Ruby**: `workers/ruby/bin/server.rb` + `workers/ruby/lib/tasker_core/bootstrap.rb`
**Python**: `workers/python/bin/server.py` + `workers/python/python/tasker_core/bootstrap.py`

## Files to Create

```
bin/
└── server.ts                    # Server entry point

src/
├── bootstrap/
│   ├── bootstrap.ts             # Bootstrap orchestration
│   ├── config.ts                # BootstrapConfig types
│   └── index.ts                 # Exports
└── index.ts                     # Public API (bootstrap_worker, etc.)
```

## Key Features

### Server Mode
- Entry point: `bin/server.ts`
- Signal handlers: SIGTERM, SIGINT, SIGUSR1 (status)
- Periodic health checks
- Graceful shutdown sequence

### Headless/Embedded Mode
- Configuration via TOML: `web.enabled = false`
- Import and call `bootstrapWorker()` directly
- No HTTP server

### Bootstrap API
```typescript
export function bootstrapWorker(config?: BootstrapConfig): BootstrapResult
export function stopWorker(): string
export function getWorkerStatus(): WorkerStatus
export function isWorkerRunning(): boolean
```

## Research Needed

1. Process signal handling in Bun vs Node.js
2. TOML configuration loading (use `@ltd/j-toml` or read via FFI?)
3. Shutdown sequence order (matches Ruby/Python exactly)
4. Health check implementation

## Success Criteria

- [ ] `bin/server.ts` starts worker successfully
- [ ] Signal handlers trigger graceful shutdown
- [ ] Bootstrap API exported from package
- [ ] Headless mode works without server
- [ ] Configuration loaded from TOML
- [ ] Integration test: full lifecycle (start → process step → shutdown)

## Estimated Scope

~1-2 days (10-15 hours)

---

**To be expanded**: Detailed implementation after TAS-102 completion.

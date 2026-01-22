# TAS-71: Future Work and Deferred Items

## Validation: tokio-console Integration (TAS-158)

**Status:** Validated 2026-01-21

### Validation Steps Performed

1. **Build with console support:**
   ```bash
   cargo make build-console
   ```
   Result: ✅ Builds successfully with `tokio-console` feature

2. **Start orchestration with console:**
   ```bash
   cargo make run-console-orchestration
   ```
   Result: ✅ Logs show `tokio_console_enabled=true`

3. **Verify gRPC server:**
   ```bash
   lsof -i :6669  # Console-subscriber default port
   nc -zv localhost 6669
   ```
   Result: ✅ Server listening and accepting connections on port 6669

4. **Connect tokio-console** (requires interactive terminal):
   ```bash
   cargo make console  # or: tokio-console
   ```
   Result: ✅ Connects successfully (validated manually)

### Expected Named Tasks in tokio-console

When connected, you should see these named tasks:
- `orchestration_command_processor`
- `staleness_detector`
- `orchestration_web_server`
- `orchestration_shutdown_handler`
- `orchestration_queue_listener`

For Rust worker (`cargo make run-console-worker-rust`):
- `worker_command_processor`
- `worker_domain_event_system`
- `worker_completion_processor`
- `worker_web_server`
- `worker_shutdown_handler`
- `worker_queue_listener`
- `worker_completion_event_listener`
- `worker_step_dispatch_listener`

---

## FFI Worker tokio-console Integration

**Status:** Deferred
**Rationale:** For initial profiling and optimization work, orchestration and `worker-rust` binaries provide sufficient coverage of fundamental hot paths. All shared worker behavior is expressed in the `tasker-worker` foundation crate, which the Rust worker exercises directly.

### Current State

tokio-console support is available for:
- `tasker-orchestration` (tasker-server binary)
- `workers/rust` (rust-worker binary)

Usage:
```bash
cargo make build-console
cargo make run-console-orchestration  # or: cargo make rco
cargo make run-console-worker-rust    # or: cargo make rcw
cargo make console  # in another terminal
```

### Deferred: FFI Worker Support

**When to revisit:** When we need to evaluate in-process event loop polling behavior across the FFI boundary (Ruby/Python/TypeScript).

**What's needed for each FFI worker:**

| Worker | Build System | Changes Required |
|--------|--------------|------------------|
| Ruby | magnus / rake | Modify `Rakefile` to pass `RUSTFLAGS="--cfg tokio_unstable"` during `rake compile` |
| Python | maturin / pyo3 | Configure maturin to pass RUSTFLAGS, possibly via `pyproject.toml` or env |
| TypeScript | napi-rs | Modify build config to include RUSTFLAGS |

**Technical considerations:**
- Console-subscriber runs a gRPC server inside the Rust runtime
- For FFI workers, the tokio runtime runs inside the compiled dylib
- The host language process (Ruby/Python/Node) loads the dylib
- gRPC server should still be accessible, but initialization timing may differ
- Need to ensure console-subscriber initializes before tokio runtime starts

**Alternative for FFI profiling:**
- OpenTelemetry tracing is already integrated and works with compiled dylibs
- Consider tracing-based analysis for FFI-specific hot paths
- `tracing-flame` can generate flamegraphs from existing instrumentation

### Related Work

- TAS-158: tokio-console integration (completed for orchestration + rust worker)
- TAS-159: Expand E2E benchmarks (pending)
- TAS-161: Profile system and identify optimization targets (pending)

# AGENTS.md - TypeScript Worker

**Status**: TAS-100/TAS-101 In Progress | 122 coherence tests | Bun + Node.js + Deno support

---

## Quick Reference

### Build Commands
```bash
# Full build (Rust FFI + TypeScript)
make build

# Individual targets
make build-ffi      # Rust cdylib only (release)
make build-ffi-debug # Rust cdylib (debug, faster)
make build-ts       # TypeScript only

# Testing
make test           # Run all tests
make check          # lint + typecheck + test
bun test            # Direct test run

# Linting
make lint           # Biome lint
make typecheck      # TypeScript type check
```

### Build Output Location

Build artifacts go to `$CARGO_TARGET_DIR` if set, otherwise `../../target/`:

```bash
# Check your current target directory
echo ${CARGO_TARGET_DIR:-../../target}

# FFI library location
ls ${CARGO_TARGET_DIR:-../../target}/release/libtasker_worker.*
```

**Local Development Note**: If using external cache (see `~/bin/development_cache_init.sh`),
`CARGO_TARGET_DIR` points to `/Volumes/Expansion/Development/Cache/cargo-targets/`.
This keeps large build caches off the main drive.

---

## Architecture

### Directory Structure
```
workers/typescript/
├── Cargo.toml          # Rust cdylib crate definition
├── Makefile            # Unified build system
├── package.json        # TypeScript package (Bun/npm)
├── src-rust/           # Rust FFI implementation
│   ├── lib.rs          # FFI exports (#[no_mangle] extern "C")
│   ├── bridge.rs       # TypeScriptBridgeHandle, global state
│   ├── conversions.rs  # JSON type conversions
│   ├── error.rs        # Error types
│   └── ffi_logging.rs  # Structured logging FFI
├── src/                # TypeScript source
│   ├── ffi/            # Runtime detection, FFI types
│   ├── events/         # Event emitter, poller
│   └── index.ts        # Package entry point
├── tests/              # Test suites
│   └── unit/           # Coherence tests (no FFI mocking)
└── dist/               # Built TypeScript output
```

### Runtime Support

The TypeScript worker supports three runtimes with a unified API:

| Runtime | FFI Method | Status |
|---------|------------|--------|
| Bun | `bun:ffi` dlopen | Primary |
| Node.js | `ffi-napi` | Supported |
| Deno | `Deno.dlopen` | Supported |

Runtime detection happens automatically via `src/ffi/runtime.ts`.

### FFI Library

The Rust cdylib (`libtasker_worker.{dylib,so,dll}`) exports:

- `get_version()` / `get_rust_version()` - Version info
- `health_check()` - Library health
- `bootstrap_worker(config_json)` - Start worker
- `stop_worker()` / `get_worker_status()` - Lifecycle
- `poll_step_events()` - Pull events from dispatch channel
- `complete_step_event(event_id, result_json)` - Return results
- `get_ffi_dispatch_metrics()` - Queue metrics
- `log_error/warn/info/debug/trace()` - Structured logging

---

## Testing Strategy

### Coherence Tests (Current - TAS-101)
Unit tests that verify TypeScript logic without FFI:
- Type coherence and JSON serialization
- Event emitter functionality
- Runtime detection
- Event poller with mock runtime

```bash
bun test                    # Run all tests
bun test tests/unit/ffi/    # FFI type tests only
```

### Integration Tests (Future - TAS-105)
FFI integration tests that load the actual Rust library:
- Bun FFI compatibility
- Node.js FFI compatibility
- Deno FFI compatibility

---

## CI Integration

The TypeScript worker is part of the CI pipeline:

1. **build-workers.yml**: `make build` compiles Rust FFI + TypeScript
2. **test-typescript-framework.yml**: Runs `bun test` for coherence tests
3. Artifacts uploaded: `dist/`, `libtasker_worker.{so,dylib}`

---

## Development Workflow

### Adding a New FFI Function

1. Add `#[no_mangle] pub extern "C" fn` in `src-rust/lib.rs`
2. Add type definition in `src/ffi/types.ts`
3. Add runtime binding in `src/ffi/bun-runtime.ts` (and node/deno)
4. Add to `TaskerRuntime` interface in `src/ffi/runtime-interface.ts`
5. Add coherence test in `tests/unit/ffi/`

### Adding a New Event

1. Add event name constant in `src/events/event-names.ts`
2. Add payload type in `src/events/event-emitter.ts`
3. Add emit helper method to `TaskerEventEmitter`
4. Add test in `tests/unit/events/event-emitter.test.ts`

---

## Troubleshooting

### "Library not found" errors
```bash
# Check library exists
ls ${CARGO_TARGET_DIR:-../../target}/release/libtasker_worker.*

# Rebuild if missing
make build-ffi
```

### "Lockfile had changes" in CI
```bash
# Update lockfile locally
bun install
git add bun.lock
```

### Type errors after FFI changes
```bash
# Regenerate types and rebuild
make clean-ts
make build-ts
```

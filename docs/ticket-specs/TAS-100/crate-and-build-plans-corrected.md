# TypeScript Worker Crate and Build Plans (CORRECTED)

**Date**: 2025-12-20  
**Context**: TAS-100 TypeScript Worker Implementation  
**Related**: TAS-101 (FFI Bridge and Core Infrastructure)

---

## Executive Summary

**CORRECTION**: After reviewing `workers/python/src/*.rs` and `workers/ruby/ext/tasker_core/src/*.rs`, it's clear that **TypeScript DOES need a Rust FFI bridge crate**, just like the others!

All worker types follow the same pattern:
- **Ruby**: `workers/ruby/ext/tasker_core/Cargo.toml` (Magnus FFI) → cdylib `.bundle`
- **Python**: `workers/python/Cargo.toml` (PyO3 FFI) → cdylib `_tasker_core.so`
- **TypeScript**: `workers/typescript/Cargo.toml` (extern "C" FFI) → cdylib `libtasker_worker_ts.{so,dylib,dll}`

The TypeScript worker has **TWO components**:
1. **Rust FFI bridge** (`workers/typescript/Cargo.toml`) - Exposes `#[no_mangle]` extern "C" functions
2. **TypeScript code** (`workers/typescript/package.json`) - Calls the C FFI via bun:ffi/ffi-napi/Deno.dlopen

**Key Insight**: TypeScript needs a thin Rust FFI wrapper around `tasker-worker` that exposes C ABI functions, just like Ruby/Python wrap `tasker-worker` with language-specific FFI.

---

## Architecture Overview (CORRECTED)

```
┌─────────────────────────────────────────────────────────────┐
│                   tasker-core Workspace                      │
│                      (Rust + Cargo)                          │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  tasker-worker (foundation library)                          │
│                  │                                            │
│                  ├──► Ruby FFI Bridge (Magnus)               │
│                  │    └─► tasker_core.bundle                 │
│                  │                                            │
│                  ├──► Python FFI Bridge (PyO3)               │
│                  │    └─► _tasker_core.so                    │
│                  │                                            │
│                  └──► TypeScript FFI Bridge (extern "C")     │
│                       └─► libtasker_worker_ts.so             │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                        │
                        │ FFI at runtime
                        │
        ┌───────────────┼───────────────┬───────────────┐
        │               │               │               │
        ▼               ▼               ▼               ▼
   Ruby Code       Python Code     Bun Runtime      Node Runtime
   (calls .bundle) (calls .so)    (bun:ffi)       (ffi-napi)
                                        │               │
                                        └───────┬───────┘
                                                │
                                          Deno Runtime
                                         (Deno.dlopen)
                                                │
                                                ▼
                                       libtasker_worker_ts.so
```

### Comparison: All Worker Types

| Aspect | Ruby | Python | TypeScript |
|--------|------|--------|------------|
| **Rust Crate** | ✓ Magnus FFI | ✓ PyO3 FFI | ✓ extern "C" FFI |
| **Build System** | Cargo → Rakefile | Cargo → Maturin | Cargo → npm/bun/deno |
| **FFI Type** | Language-specific (Magnus) | Language-specific (PyO3) | C ABI (universal) |
| **Compilation** | Rust → .bundle | Rust → .so | Rust → .so + TS → .js |
| **Distribution** | gem (includes .bundle) | wheel (includes .so) | npm + Rust lib separate |
| **Workspace Member** | ✓ | ✓ | ✓ |
| **cdylib name** | tasker_core | _tasker_core | tasker_worker_ts |
| **FFI Functions** | Magnus wrapper fns | PyO3 `#[pyfunction]` | `#[no_mangle] extern "C"` |

---

## Directory Structure (CORRECTED)

```
tasker-core/
├── Cargo.toml                         # Rust workspace (INCLUDES workers/typescript)
├── workers/
│   ├── ruby/
│   │   ├── ext/tasker_core/
│   │   │   ├── Cargo.toml             # Magnus cdylib crate
│   │   │   └── src/
│   │   │       ├── lib.rs             # Magnus FFI exports
│   │   │       ├── bridge.rs          # RubyBridgeHandle
│   │   │       └── conversions.rs     # Ruby type conversions
│   │   ├── lib/                       # Ruby code
│   │   └── Rakefile                   # Ruby build (calls cargo)
│   │
│   ├── python/
│   │   ├── Cargo.toml                 # PyO3 cdylib crate
│   │   ├── src/
│   │   │   ├── lib.rs                 # PyO3 FFI exports
│   │   │   ├── bridge.rs              # PythonBridgeHandle
│   │   │   └── conversions.rs         # Python type conversions
│   │   ├── python/tasker_core/        # Python code
│   │   └── pyproject.toml             # Maturin build (calls cargo)
│   │
│   ├── rust/
│   │   ├── Cargo.toml                 # Pure Rust worker
│   │   └── src/
│   │
│   └── typescript/                    # NEW: TypeScript worker
│       ├── Cargo.toml                 # Rust FFI bridge crate!
│       ├── src-rust/                  # Rust FFI bridge source
│       │   ├── lib.rs                 # extern "C" FFI exports
│       │   ├── bridge.rs              # TypeScriptBridgeHandle
│       │   ├── conversions.rs         # JSON type conversions
│       │   └── ffi_logging.rs         # Logging FFI
│       ├── package.json               # npm/bun/deno package
│       ├── deno.json                  # Deno configuration
│       ├── tsconfig.json              # TypeScript config
│       ├── tsup.config.ts             # Build config
│       ├── biome.json                 # Linter/formatter
│       ├── bin/
│       │   └── tasker-worker.ts       # Executable
│       ├── src/                       # TypeScript source
│       │   ├── index.ts               # Main entry
│       │   ├── ffi/                   # FFI adapters (call Rust C ABI)
│       │   ├── types/                 # Type definitions
│       │   ├── handlers/              # Handler classes
│       │   ├── logging/               # Logging
│       │   ├── bootstrap/             # Bootstrap
│       │   ├── subscriber/            # Execution subscriber
│       │   └── server/                # Server components
│       ├── tests/
│       │   ├── unit/
│       │   ├── integration/
│       │   └── fixtures/
│       └── examples/
│           └── handlers/
```

---

## Cargo.toml Changes (CORRECTED)

### Root Cargo.toml

**YES** - add `workers/typescript` as a workspace member!

```toml
[workspace]
members = [
  ".",
  "tasker-pgmq",
  "tasker-client",
  "tasker-orchestration",
  "tasker-shared",
  "tasker-worker",
  "workers/ruby/ext/tasker_core",
  "workers/rust",
  "workers/python",
  "workers/typescript",  # NEW: TypeScript FFI bridge crate
]
```

### workers/typescript/Cargo.toml (NEW)

```toml
[package]
name = "tasker-worker-ts"
version = "0.1.0"
edition = "2021"
description = "C FFI bridge for tasker-core TypeScript worker (Bun/Node/Deno)"
repository = "https://github.com/tasker-systems/tasker-core"
license = "MIT"
keywords = ["ffi", "typescript", "bun", "deno", "node"]
categories = ["api-bindings", "development-tools::ffi"]

[lib]
crate-type = ["cdylib"]
name = "tasker_worker_ts"
path = "src-rust/lib.rs"

[dependencies]
# Error handling
anyhow = { workspace = true }
# Serialization for JSON FFI boundary
serde = { workspace = true }
serde_json = { workspace = true }
# Workspace dependencies for worker integration
tasker-shared = { path = "../../tasker-shared" }
tasker-worker = { path = "../../tasker-worker" }
thiserror = { workspace = true }
# Async runtime
tokio = { workspace = true, features = ["rt-multi-thread", "sync"] }
# Logging
tracing = { workspace = true }
# UUID
uuid = { workspace = true }

[lints]
workspace = true
```

---

## Rust FFI Bridge Implementation

### src-rust/lib.rs

```rust
//! C FFI bridge for TypeScript worker (Bun/Node/Deno)
//!
//! This module provides C ABI-compatible FFI functions that can be called
//! from TypeScript via bun:ffi, ffi-napi, or Deno.dlopen.
//!
//! Unlike Ruby (Magnus) and Python (PyO3) which use language-specific FFI,
//! TypeScript uses the universal C ABI for maximum compatibility.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

mod bridge;
mod conversions;
mod ffi_logging;

/// Get the version of the TypeScript FFI bridge
#[no_mangle]
pub extern "C" fn get_version() -> *mut c_char {
    let version = env!("CARGO_PKG_VERSION");
    CString::new(version).unwrap().into_raw()
}

/// Health check - returns 1 if FFI is working
#[no_mangle]
pub extern "C" fn health_check() -> i32 {
    1
}

/// Bootstrap the worker system
///
/// # Arguments
/// * `config_json` - JSON string with bootstrap configuration
///
/// # Returns
/// * JSON string with bootstrap result (must be freed with free_rust_string)
#[no_mangle]
pub extern "C" fn bootstrap_worker(config_json: *const c_char) -> *mut c_char {
    // Safety: Caller must ensure config_json is a valid C string
    let config_str = unsafe {
        if config_json.is_null() {
            return std::ptr::null_mut();
        }
        CStr::from_ptr(config_json).to_str().unwrap_or("")
    };

    match bridge::bootstrap_worker(config_str) {
        Ok(result) => {
            let result_json = serde_json::to_string(&result).unwrap();
            CString::new(result_json).unwrap().into_raw()
        }
        Err(e) => {
            let error_json = format!(r#"{{"error": "{}"}}"#, e);
            CString::new(error_json).unwrap().into_raw()
        }
    }
}

/// Get worker status
///
/// # Returns
/// * JSON string with worker status (must be freed with free_rust_string)
#[no_mangle]
pub extern "C" fn get_worker_status() -> *mut c_char {
    match bridge::get_worker_status() {
        Ok(status) => {
            let status_json = serde_json::to_string(&status).unwrap();
            CString::new(status_json).unwrap().into_raw()
        }
        Err(e) => {
            let error_json = format!(r#"{{"error": "{}"}}"#, e);
            CString::new(error_json).unwrap().into_raw()
        }
    }
}

/// Stop the worker system
#[no_mangle]
pub extern "C" fn stop_worker() -> *mut c_char {
    match bridge::stop_worker() {
        Ok(msg) => CString::new(msg).unwrap().into_raw(),
        Err(e) => {
            let error_json = format!(r#"{{"error": "{}"}}"#, e);
            CString::new(error_json).unwrap().into_raw()
        }
    }
}

/// Check if worker is running
#[no_mangle]
pub extern "C" fn is_worker_running() -> i32 {
    if bridge::is_worker_running() {
        1
    } else {
        0
    }
}

/// Poll for step events
///
/// # Returns
/// * JSON string with array of step events, or NULL if none (must be freed with free_rust_string)
#[no_mangle]
pub extern "C" fn poll_step_events() -> *mut c_char {
    match bridge::poll_step_events() {
        Ok(Some(events)) => {
            let events_json = serde_json::to_string(&events).unwrap();
            CString::new(events_json).unwrap().into_raw()
        }
        Ok(None) => std::ptr::null_mut(), // No events
        Err(e) => {
            let error_json = format!(r#"{{"error": "{}"}}"#, e);
            CString::new(error_json).unwrap().into_raw()
        }
    }
}

/// Complete a step event
///
/// # Arguments
/// * `event_id` - UUID string of the event
/// * `result_json` - JSON string with step execution result
///
/// # Returns
/// * 1 if successful, 0 if failed
#[no_mangle]
pub extern "C" fn complete_step_event(
    event_id: *const c_char,
    result_json: *const c_char,
) -> i32 {
    let event_id_str = unsafe {
        if event_id.is_null() {
            return 0;
        }
        CStr::from_ptr(event_id).to_str().unwrap_or("")
    };

    let result_str = unsafe {
        if result_json.is_null() {
            return 0;
        }
        CStr::from_ptr(result_json).to_str().unwrap_or("")
    };

    match bridge::complete_step_event(event_id_str, result_str) {
        Ok(true) => 1,
        Ok(false) | Err(_) => 0,
    }
}

/// Log an error message
#[no_mangle]
pub extern "C" fn log_error(message: *const c_char, fields_json: *const c_char) {
    ffi_logging::log_error(message, fields_json);
}

/// Log a warning message
#[no_mangle]
pub extern "C" fn log_warn(message: *const c_char, fields_json: *const c_char) {
    ffi_logging::log_warn(message, fields_json);
}

/// Log an info message
#[no_mangle]
pub extern "C" fn log_info(message: *const c_char, fields_json: *const c_char) {
    ffi_logging::log_info(message, fields_json);
}

/// Log a debug message
#[no_mangle]
pub extern "C" fn log_debug(message: *const c_char, fields_json: *const c_char) {
    ffi_logging::log_debug(message, fields_json);
}

/// Log a trace message
#[no_mangle]
pub extern "C" fn log_trace(message: *const c_char, fields_json: *const c_char) {
    ffi_logging::log_trace(message, fields_json);
}

/// Get FFI dispatch metrics
///
/// # Returns
/// * JSON string with metrics (must be freed with free_rust_string)
#[no_mangle]
pub extern "C" fn get_ffi_dispatch_metrics() -> *mut c_char {
    match bridge::get_ffi_dispatch_metrics() {
        Ok(metrics) => {
            let metrics_json = serde_json::to_string(&metrics).unwrap();
            CString::new(metrics_json).unwrap().into_raw()
        }
        Err(e) => {
            let error_json = format!(r#"{{"error": "{}"}}"#, e);
            CString::new(error_json).unwrap().into_raw()
        }
    }
}

/// Check for starvation warnings
#[no_mangle]
pub extern "C" fn check_starvation_warnings() {
    let _ = bridge::check_starvation_warnings();
}

/// Cleanup timed-out events
#[no_mangle]
pub extern "C" fn cleanup_timeouts() {
    let _ = bridge::cleanup_timeouts();
}

/// Free a Rust-allocated string
///
/// CRITICAL: All strings returned by FFI functions must be freed with this!
#[no_mangle]
pub extern "C" fn free_rust_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}
```

### src-rust/bridge.rs

```rust
//! TypeScript FFI Bridge with System Handle Management
//!
//! Follows the same pattern as Ruby and Python bridges.

use std::sync::{Arc, Mutex};
use tasker_shared::events::domain_events::DomainEvent;
use tasker_worker::worker::{FfiDispatchChannel, FfiDispatchMetrics, FfiStepEvent};
use tasker_worker::{WorkerSystemHandle, WorkerSystemStatus};
use tokio::sync::broadcast;

/// Global handle to the worker system for TypeScript FFI
pub static WORKER_SYSTEM: Mutex<Option<TypeScriptBridgeHandle>> = Mutex::new(None);

/// Bridge handle that maintains worker system state
pub struct TypeScriptBridgeHandle {
    /// Handle from tasker-worker bootstrap
    pub system_handle: WorkerSystemHandle,
    /// FFI dispatch channel for step execution events
    pub ffi_dispatch_channel: Arc<FfiDispatchChannel>,
    /// Domain event publisher
    pub domain_event_publisher: Arc<tasker_shared::events::domain_events::DomainEventPublisher>,
    /// In-process event receiver for fast domain events
    pub in_process_event_receiver: Option<Arc<Mutex<broadcast::Receiver<DomainEvent>>>>,
    /// Keep runtime alive
    #[allow(dead_code)]
    pub runtime: tokio::runtime::Runtime,
}

impl TypeScriptBridgeHandle {
    pub fn new(
        system_handle: WorkerSystemHandle,
        ffi_dispatch_channel: Arc<FfiDispatchChannel>,
        domain_event_publisher: Arc<tasker_shared::events::domain_events::DomainEventPublisher>,
        in_process_event_receiver: Option<broadcast::Receiver<DomainEvent>>,
        runtime: tokio::runtime::Runtime,
    ) -> Self {
        Self {
            system_handle,
            ffi_dispatch_channel,
            domain_event_publisher,
            in_process_event_receiver: in_process_event_receiver.map(|r| Arc::new(Mutex::new(r))),
            runtime,
        }
    }

    /// Poll for the next step execution event
    pub fn poll_step_event(&self) -> Option<FfiStepEvent> {
        self.ffi_dispatch_channel.poll()
    }

    /// Submit a completion result for an event
    pub fn complete_step_event(
        &self,
        event_id: uuid::Uuid,
        result: tasker_shared::messaging::StepExecutionResult,
    ) -> bool {
        self.ffi_dispatch_channel.complete(event_id, result)
    }

    /// Get metrics about FFI dispatch channel health
    pub fn get_ffi_dispatch_metrics(&self) -> FfiDispatchMetrics {
        self.ffi_dispatch_channel.metrics()
    }

    /// Check for starvation warnings
    pub fn check_starvation_warnings(&self) {
        self.ffi_dispatch_channel.check_starvation_warnings()
    }
}

// Bootstrap, status, stop, poll, complete functions here...
// (Similar to Python/Ruby patterns but using Result<T, String> for C FFI)
```

---

## Build Process (CORRECTED)

### Step 1: Build Rust FFI Bridge

```bash
cd workers/typescript
cargo build --release
# Produces: target/release/libtasker_worker_ts.{so,dylib,dll}
```

### Step 2: Build TypeScript Code

```bash
cd workers/typescript
bun install
bun run build
# Produces: dist/index.js
```

### Step 3: Run TypeScript Worker

```bash
# Bun
bun bin/tasker-worker.ts --namespace payments

# Node
npm run start -- --namespace payments

# Deno
deno run --allow-ffi --allow-net bin/tasker-worker.ts --namespace payments
```

---

## Key Corrections

1. **TypeScript DOES need Cargo.toml** - for the Rust FFI bridge
2. **TypeScript IS a workspace member** - just like Ruby/Python
3. **Builds BOTH Rust and TypeScript** - Rust cdylib + TypeScript npm package
4. **Uses extern "C" ABI** - universal C FFI instead of language-specific (Magnus/PyO3)
5. **Same patterns as Ruby/Python** - BridgeHandle, conversions, logging FFI

---

## Summary

The TypeScript worker follows the **exact same pattern** as Ruby and Python, but uses C FFI instead of language-specific FFI:

- **Ruby**: Magnus FFI (`ruby.define_module`, `wrap_pyfunction!` equivalent)
- **Python**: PyO3 FFI (`#[pyfunction]`, `wrap_pyfunction!`)
- **TypeScript**: C FFI (`#[no_mangle] extern "C"`, universal ABI)

All three expose the same `tasker-worker` functionality through a thin FFI bridge crate that provides language-appropriate bindings.

**Total Effort**: ~8 hours for Rust FFI bridge + ~20 hours for TypeScript implementation = ~28 hours

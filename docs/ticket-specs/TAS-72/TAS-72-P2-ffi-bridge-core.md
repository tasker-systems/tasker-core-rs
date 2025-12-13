# TAS-72-P2: FFI Bridge Core

## Overview

This phase ports the core FFI functionality from the Ruby worker to Python, establishing the worker system lifecycle management, type conversion layer, and logging integration.

## Prerequisites

- TAS-72-P1 complete (crate foundation established)
- Basic FFI module working

## Objectives

1. Implement worker bootstrap/shutdown lifecycle via FFI
2. Create type conversion layer (Rust ↔ Python)
3. Integrate structured logging through FFI
4. Establish error handling with Python exceptions

## Components to Port from Ruby

Reference: `workers/ruby/ext/tasker_core/src/`

### 1. Bridge Module (`bridge.rs`)

**Ruby Implementation Pattern:**
```rust
// Ruby: Global worker system singleton
static WORKER_SYSTEM: Mutex<Option<RubyBridgeHandle>> = Mutex::new(None);

pub struct RubyBridgeHandle {
    system_handle: WorkerSystemHandle,
    ffi_dispatch_channel: Arc<FfiDispatchChannel>,
    domain_event_publisher: Arc<DomainEventPublisher>,
    in_process_event_receiver: Option<broadcast::Receiver<InProcessDomainEvent>>,
    runtime: Runtime,
}
```

**Python Adaptation:**
```rust
// Python: Similar pattern with PyO3 specifics
static WORKER_SYSTEM: Mutex<Option<PythonBridgeHandle>> = Mutex::new(None);

pub struct PythonBridgeHandle {
    system_handle: WorkerSystemHandle,
    ffi_dispatch_channel: Arc<FfiDispatchChannel>,
    domain_event_publisher: Arc<DomainEventPublisher>,
    in_process_event_receiver: Option<broadcast::Receiver<InProcessDomainEvent>>,
    runtime: Runtime,
}
```

### 2. Bootstrap Module (`bootstrap.rs`)

FFI functions to implement:

| Function | Description | Returns |
|----------|-------------|---------|
| `bootstrap_worker(config)` | Initialize worker system | `BootstrapResult` |
| `stop_worker()` | Graceful shutdown | `bool` |
| `worker_status()` | Current worker state | `WorkerStatus` |
| `transition_to_graceful_shutdown()` | Begin shutdown sequence | `bool` |

### 3. Type Conversion (`conversions.rs`)

**Ruby uses `serde_magnus`; Python uses `pythonize`:**

```rust
// Ruby pattern
use serde_magnus::{serialize, deserialize};
let ruby_hash = serialize(&ruby, rust_struct)?;
let rust_struct: MyStruct = deserialize(&ruby, ruby_value)?;

// Python pattern
use pythonize::{pythonize, depythonize};
let py_dict = pythonize(py, &rust_struct)?;
let rust_struct: MyStruct = depythonize(&py_obj)?;
```

### 4. Logging FFI (`ffi_logging.rs`)

FFI functions for structured logging:

| Function | Description |
|----------|-------------|
| `log_trace(message, context)` | Trace level log |
| `log_debug(message, context)` | Debug level log |
| `log_info(message, context)` | Info level log |
| `log_warn(message, context)` | Warning level log |
| `log_error(message, context)` | Error level log |

## Pydantic Models

### BootstrapConfig

```python
from pydantic import BaseModel, Field
from typing import Optional

class BootstrapConfig(BaseModel):
    """Configuration for worker bootstrap."""
    worker_id: Optional[str] = None
    namespace: str = "default"
    config_path: Optional[str] = None
    log_level: str = Field(default="info", pattern="^(trace|debug|info|warn|error)$")

    class Config:
        extra = "forbid"
```

### BootstrapResult

```python
class BootstrapResult(BaseModel):
    """Result from worker bootstrap."""
    success: bool
    handle_id: str
    worker_id: str
    status: str
    error_message: Optional[str] = None
```

### WorkerStatus

```python
from enum import Enum

class WorkerState(str, Enum):
    """Worker lifecycle states."""
    STARTING = "starting"
    RUNNING = "running"
    SHUTTING_DOWN = "shutting_down"
    STOPPED = "stopped"
    ERROR = "error"

class WorkerStatus(BaseModel):
    """Current worker status."""
    state: WorkerState
    worker_id: str
    uptime_seconds: float
    steps_processed: int
    steps_failed: int
```

## Error Handling

### Custom Exception Hierarchy

```python
class TaskerError(Exception):
    """Base exception for all tasker-core errors."""
    pass

class WorkerNotInitializedError(TaskerError):
    """Raised when FFI called before bootstrap."""
    pass

class WorkerBootstrapError(TaskerError):
    """Raised when bootstrap fails."""
    pass

class FFIError(TaskerError):
    """Raised for FFI-level errors."""
    pass
```

### Rust Error Conversion

```rust
use pyo3::exceptions::PyRuntimeError;
use pyo3::PyErr;

/// Convert Rust errors to Python exceptions
fn convert_error(e: TaskerError) -> PyErr {
    match e {
        TaskerError::NotInitialized => PyErr::new::<WorkerNotInitializedError, _>(
            "Worker not initialized. Call bootstrap_worker() first."
        ),
        TaskerError::BootstrapFailed(msg) => PyErr::new::<WorkerBootstrapError, _>(msg),
        _ => PyErr::new::<PyRuntimeError, _>(e.to_string()),
    }
}
```

## Python API Surface

```python
# tasker_core/ffi.py

from tasker_core._tasker_core import (
    _bootstrap_worker,
    _stop_worker,
    _worker_status,
    _transition_to_graceful_shutdown,
    _log_trace,
    _log_debug,
    _log_info,
    _log_warn,
    _log_error,
)

def bootstrap_worker(config: BootstrapConfig) -> BootstrapResult:
    """Initialize the worker system.

    Args:
        config: Bootstrap configuration

    Returns:
        BootstrapResult with worker details

    Raises:
        WorkerBootstrapError: If bootstrap fails
    """
    result = _bootstrap_worker(config.model_dump())
    return BootstrapResult.model_validate(result)

def stop_worker() -> bool:
    """Stop the worker system gracefully.

    Returns:
        True if shutdown was successful

    Raises:
        WorkerNotInitializedError: If worker not running
    """
    return _stop_worker()
```

## Testing Strategy

### Unit Tests

```python
def test_bootstrap_with_defaults():
    """Test bootstrap with default configuration."""
    config = BootstrapConfig()
    result = bootstrap_worker(config)
    assert result.success
    assert result.worker_id
    stop_worker()

def test_bootstrap_with_custom_namespace():
    """Test bootstrap with custom namespace."""
    config = BootstrapConfig(namespace="test-namespace")
    result = bootstrap_worker(config)
    assert result.success
    stop_worker()

def test_double_bootstrap_fails():
    """Test that bootstrapping twice raises error."""
    config = BootstrapConfig()
    bootstrap_worker(config)
    with pytest.raises(WorkerBootstrapError):
        bootstrap_worker(config)
    stop_worker()

def test_stop_without_bootstrap():
    """Test that stopping without bootstrap raises error."""
    with pytest.raises(WorkerNotInitializedError):
        stop_worker()
```

### Integration Tests

Integration tests require database connection and will be covered in TAS-72-P6.

## Acceptance Criteria

- [ ] `bootstrap_worker(config)` initializes worker system
- [ ] `stop_worker()` cleanly shuts down worker
- [ ] `worker_status()` returns current state
- [ ] Logging functions work and integrate with Python logging
- [ ] Rust errors convert to appropriate Python exceptions
- [ ] Pydantic models validate FFI data correctly
- [ ] All unit tests pass

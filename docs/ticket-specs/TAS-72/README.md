# TAS-72: PyO3 Python Worker Foundations

## Overview

This specification outlines the phased development of a Python worker for tasker-core, using PyO3 for Rust FFI integration. The Python worker will provide a first-class Python experience for workflow step handling, mirroring the functionality and patterns established in the Ruby worker (`workers/ruby`).

## Problem Statement

Tasker-core currently provides a production-ready Ruby worker implementation using Magnus for FFI. Many enterprise environments prefer Python for data processing, ML/AI workloads, and scientific computing. To maximize adoption and provide flexibility in handler implementation, we need a fully-featured Python worker that:

1. **Integrates seamlessly** with the existing Rust orchestration infrastructure
2. **Follows Python best practices** for packaging, testing, and distribution
3. **Maintains parity** with the Ruby worker's functionality
4. **Provides excellent developer experience** with type hints, async support, and modern tooling

## Current State Analysis

### Existing Ruby Worker Architecture (TAS-77 Complete)

The Ruby worker (`workers/ruby`) demonstrates a production-ready FFI integration pattern:

```
workers/ruby/
├── ext/tasker_core/               # Rust FFI bridge (Magnus)
│   ├── src/
│   │   ├── lib.rs                 # Module initialization
│   │   ├── bridge.rs              # Core FFI bridge
│   │   ├── bootstrap.rs           # Worker initialization
│   │   ├── conversions.rs         # Type conversions (serde_magnus)
│   │   ├── event_publisher_ffi.rs # Domain event FFI
│   │   ├── observability_ffi.rs   # Health/metrics FFI (TAS-77)
│   │   └── ...
│   └── Cargo.toml
├── lib/tasker_core/               # Ruby business logic
│   ├── bootstrap.rb               # Worker lifecycle
│   ├── event_bridge.rb            # dry-events integration
│   ├── worker/                    # Pollers and dispatchers
│   ├── step_handler/              # Handler base classes
│   └── ...
├── spec/                          # RSpec tests
├── tasker-worker-rb.gemspec       # Gem distribution
└── Rakefile                       # Build tasks
```

**Key Patterns to Replicate:**
1. **FFI Bridge**: Singleton worker system with dispatch/completion channels
2. **Pull-based Polling**: Workers poll for events rather than receiving callbacks
3. **Event System**: In-process event bus (dry-events) for pub/sub
4. **Handler Registry**: Discovery and registration of step handlers
5. **Lifecycle Management**: Bootstrap, health checks, graceful shutdown
6. **Observability**: Structured logging, metrics, health endpoints

### Worker Event System Architecture

From `docs/worker-event-systems.md` and `tasker-worker/AGENTS.md`:

```
┌─────────────────────────────────────────────────────────────────┐
│                   FfiDispatchChannel (Rust)                      │
│  - dispatch_receiver: Receives steps for FFI handlers            │
│  - pending_events: HashMap tracking dispatched events            │
│  - completion_sender: Sends results back to completion pipeline  │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┴─────────────────────┐
        ▼                                           ▼
┌───────────────────┐                    ┌───────────────────┐
│  poll_step_events │                    │ complete_step_event│
│  (FFI call)       │                    │  (FFI call)        │
└───────────────────┘                    └───────────────────┘
        │                                           ▲
        ▼                                           │
┌───────────────────────────────────────────────────────────────┐
│                     Python Worker Layer                        │
│  - EventPoller: 10ms polling loop for step events              │
│  - EventBridge: Pub/sub for internal event distribution        │
│  - HandlerRegistry: Discovers and resolves handlers            │
│  - StepHandler: Base class for handler implementations         │
└───────────────────────────────────────────────────────────────┘
```

## Proposed Solution

### Python Worker Architecture

```
workers/python/
├── src/tasker_core/               # Rust FFI bridge (PyO3)
│   ├── src/
│   │   ├── lib.rs                 # Module initialization
│   │   ├── bridge.rs              # Core FFI bridge
│   │   ├── bootstrap.rs           # Worker initialization
│   │   ├── conversions.rs         # Type conversions (pythonize)
│   │   ├── event_publisher_ffi.rs # Domain event FFI
│   │   ├── observability_ffi.rs   # Health/metrics FFI
│   │   └── ...
│   └── Cargo.toml
├── python/tasker_core/            # Python package (src layout)
│   ├── __init__.py
│   ├── bootstrap.py               # Worker lifecycle
│   ├── event_bridge.py            # Event system integration
│   ├── worker/
│   │   ├── __init__.py
│   │   ├── event_poller.py
│   │   └── ...
│   ├── step_handler/
│   │   ├── __init__.py
│   │   ├── base.py
│   │   └── ...
│   ├── types/
│   │   └── ...
│   └── py.typed                   # PEP 561 marker
├── tests/                         # pytest tests
│   ├── conftest.py
│   ├── ffi/
│   ├── integration/
│   └── ...
├── Cargo.toml                     # Workspace member
├── pyproject.toml                 # Python project config
└── README.md
```

### Technology Choices

| Component | Choice | Rationale |
|-----------|--------|-----------|
| **FFI Layer** | PyO3 | Industry standard, mature, excellent docs |
| **Build Tool** | maturin | Optimized for PyO3, handles wheels/sdist |
| **Package Manager** | uv | Modern, fast, Rust-powered (aligns with project) |
| **Testing** | pytest | Standard, extensive ecosystem |
| **Data Models** | Pydantic v2 | Python equivalent to dry-struct, validation + serialization |
| **Event Bus** | pyee | Lightweight, sync/async support, Node.js-like API |
| **Type Hints** | Full typing + py.typed | First-class IDE support |
| **Async** | asyncio (optional) | For async handlers if needed |

### Pydantic for Data Modeling

Just as the Ruby worker uses `dry-struct` and `dry-types` for type-safe data modeling, the Python worker will use **Pydantic v2** throughout:

```python
# Ruby (dry-struct)
class StepHandlerCallResult < Dry::Struct
  attribute :success, Types::Bool
  attribute :result, Types::Hash
  attribute :error_message, Types::String.optional
  attribute :metadata, Types::Hash.default({}.freeze)
end

# Python (Pydantic)
from pydantic import BaseModel, Field

class StepHandlerCallResult(BaseModel):
    success: bool
    result: dict[str, Any]
    error_message: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)
```

**Benefits of Pydantic:**
1. **Validation at boundaries**: Automatic validation when deserializing FFI data
2. **Serialization**: Built-in JSON serialization for FFI calls
3. **IDE Support**: Excellent autocomplete and type checking
4. **Settings Management**: `pydantic-settings` for configuration
5. **FastAPI Integration**: Native support if we expose HTTP endpoints
6. **Performance**: Pydantic v2 uses Rust core for speed

### Event Bus Decision

**Recommendation: `pyee`** (PyPI: `pyee`)

Rationale:
1. **Lightweight**: No heavy dependencies (unlike Bubus which requires Pydantic)
2. **Node.js-like API**: Familiar pattern (`emit`, `on`, `once`)
3. **Sync/Async Flexibility**: Base `EventEmitter` is sync, `AsyncIOEventEmitter` available
4. **Mature**: Well-established, maintained library

The Ruby worker uses `dry-events` which has a similar API pattern. `pyee` provides equivalent functionality:

```python
# Ruby (dry-events)
event_bus.publish('step.execution.received', event_data)
event_bus.subscribe('step.execution.received') { |event| handle(event) }

# Python (pyee)
event_bus.emit('step.execution.received', event_data)
@event_bus.on('step.execution.received')
def handle(event_data):
    pass
```

## Implementation Phases

### Phase 1: Crate Foundation (TAS-72-P1)
**Scope**: Set up `workers/python` crate with PyO3 and minimal FFI surface

**Deliverables**:
- `workers/python/` directory structure
- Workspace member in root `Cargo.toml`
- Basic PyO3 module initialization
- `pyproject.toml` with maturin configuration
- Development environment setup with `uv`
- CI/CD pipeline for Python tests

**Acceptance Criteria**:
- `maturin develop` successfully builds and installs module
- `import tasker_core` works in Python
- Basic FFI function call works (e.g., `tasker_core.version()`)

### Phase 2: FFI Bridge Core (TAS-72-P2)
**Scope**: Port core FFI functionality from Ruby to Python

**Deliverables**:
- Bootstrap FFI (`bootstrap_worker`, `stop_worker`, `worker_status`)
- Type conversion layer (Rust types ↔ Python dicts/dataclasses)
- Logging FFI (`log_info`, `log_warn`, `log_error`, etc.)
- Basic error handling with Python exceptions

**Acceptance Criteria**:
- Worker can be bootstrapped and stopped via FFI
- Rust logs appear in Python logging system
- Rust errors convert to Python exceptions

### Phase 3: Event Dispatch System (TAS-72-P3)
**Scope**: Implement step event polling and completion

**Deliverables**:
- `poll_step_events` FFI function
- `complete_step_event` FFI function
- `FfiStepEvent` Python dataclass/TypedDict
- `StepExecutionResult` Python type
- Python `EventPoller` class (threaded polling loop)

**Acceptance Criteria**:
- Can poll for step events via FFI
- Can submit completion results via FFI
- Event poller runs without blocking Python main thread

### Phase 4: Event Bridge & Handler System (TAS-72-P4)
**Scope**: Implement in-process event system and handler infrastructure

**Deliverables**:
- `EventBridge` class using `pyee`
- `HandlerRegistry` for handler discovery and resolution
- `StepHandler` base class with `call()` protocol
- Step execution subscriber

**Acceptance Criteria**:
- Events flow from FFI poll → EventBridge → Handler → Completion
- Handlers can be registered and resolved by name
- Full step execution lifecycle works

### Phase 5: Domain Events & Observability (TAS-72-P5)
**Scope**: Implement domain event publishing and observability

**Deliverables**:
- `publish_domain_event` FFI function
- `poll_in_process_events` FFI function (fast path)
- Health check endpoints (`get_health_check`)
- Metrics endpoints (`get_metrics`, `get_ffi_dispatch_metrics`)
- Configuration access (`get_config`)

**Acceptance Criteria**:
- Domain events can be published from Python handlers
- Health checks return comprehensive status
- Metrics are accessible from Python

### Phase 6: Testing & Documentation (TAS-72-P6)
**Scope**: Comprehensive testing and documentation

**Deliverables**:
- Unit tests for all FFI functions
- Integration tests with database
- Example handlers (linear, diamond, DAG workflows)
- User documentation (README, API docs)
- Type stubs (`.pyi` files if needed)

**Acceptance Criteria**:
- >80% test coverage on Python code
- All integration tests pass
- Documentation complete and published

### Phase 6a: E2E & Integration Tests (TAS-72-P6a)
**Scope**: End-to-end testing infrastructure following Ruby/Rust patterns

**Deliverables**:
- Task templates in `tests/fixtures/task_templates/python/` (copied from Ruby with `_py` suffix)
- Example handlers in `workers/python/tests/handlers/examples/` mirroring Ruby patterns
- Rust E2E tests in `tests/e2e/python/` using IntegrationTestManager
- Docker Compose configuration for Python worker (port 8083)
- Test environment loader for auto-discovering example handlers
- IntegrationTestManager support for `WorkerType::Python`

**Acceptance Criteria**:
- All Ruby workflow patterns have Python equivalents (linear, diamond, tree, mixed DAG)
- Mathematical verification produces identical results to Ruby handlers
- Python E2E tests pass through full orchestration pipeline
- Docker Compose spins up Python worker alongside Rust/Ruby workers

### Phase 7: Distribution & CI/CD (TAS-72-P7)
**Scope**: Package distribution and deployment automation

**Deliverables**:
- Wheel builds for multiple platforms (manylinux, macOS, Windows)
- PyPI publication workflow
- GitHub Actions CI/CD
- Docker base image with Python worker

**Acceptance Criteria**:
- Wheels build for Linux x86_64, aarch64, macOS, Windows
- Package installable via `pip install tasker-core-py`
- CI/CD runs on all PRs

## Key Differences: PyO3 vs Magnus

| Aspect | Magnus (Ruby) | PyO3 (Python) | Notes |
|--------|---------------|---------------|-------|
| **GC Safety** | Manual stack placement | GIL token (`Python<'py>`) | PyO3 uses GIL token for safety proofs |
| **Type Conversion** | `serde_magnus` | `pythonize` or `IntoPy`/`FromPyObject` | Similar ergonomics |
| **Async** | Ruby GVL limitations | Full asyncio support | PyO3 can integrate with asyncio |
| **Module Init** | `#[magnus::init]` | `#[pymodule]` | Similar pattern |
| **Error Handling** | `magnus::Error` | `PyErr` | Both convert to language exceptions |
| **Lifetime Constraints** | Cannot heap-allocate Ruby objects | Cannot store `'py` lifetime across async | Similar constraints |

## Configuration

### pyproject.toml (Key Sections)

```toml
[project]
name = "tasker-core-py"
version = "0.1.0"
description = "Python worker for Tasker workflow orchestration"
requires-python = ">=3.10"
dependencies = [
    "pydantic>=2.9.0",
    "pydantic-settings>=2.6.0",
    "pyee>=12.0.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0.0",
    "pytest-asyncio>=0.23.0",
    "ruff>=0.5.0",
    "mypy>=1.10.0",
]

[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[tool.maturin]
python-source = "python"
module-name = "tasker_core._tasker_core"
features = ["pyo3/extension-module"]

[tool.pytest.ini_options]
testpaths = ["tests"]
python_files = ["test_*.py"]
asyncio_mode = "auto"

[tool.ruff]
target-version = "py310"
line-length = 100

[tool.mypy]
python_version = "3.10"
strict = true
```

### Cargo.toml (workers/python)

```toml
[package]
name = "tasker-worker-py"
version = "0.1.0"
edition = "2021"

[lib]
name = "_tasker_core"
crate-type = ["cdylib"]

[dependencies]
pyo3 = { version = "0.22", features = ["extension-module"] }
pythonize = "0.22"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
tracing = "0.1"

tasker-worker = { path = "../../tasker-worker" }
tasker-shared = { path = "../../tasker-shared" }

[dev-dependencies]
pyo3 = { version = "0.22", features = ["auto-initialize"] }
```

## Success Metrics

### Phase Completion Metrics

| Phase | Success Criteria |
|-------|------------------|
| P1 | Module imports, basic FFI works |
| P2 | Bootstrap/shutdown works, logging integrated |
| P3 | Step events can be polled and completed |
| P4 | Full handler execution lifecycle works |
| P5 | Domain events and observability functional |
| P6 | Tests pass, docs complete |
| P7 | Package published to PyPI |

### Quality Metrics

- **Test Coverage**: >80% on Python code
- **Type Coverage**: 100% type hints
- **Lint Compliance**: Zero ruff/mypy errors
- **Performance**: <1ms FFI overhead per call
- **Documentation**: All public APIs documented

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| PyO3 version conflicts | Low | High | Pin versions, test matrix |
| GIL contention in polling | Medium | Medium | Separate polling thread |
| Type conversion complexity | Medium | Low | Use pythonize, document types |
| Platform-specific build issues | Medium | Medium | CI matrix, manylinux containers |
| Event bus performance | Low | Low | Benchmark, fallback options |

## Related Documents

- `docs/worker-event-systems.md` - Worker architecture
- `docs/actors.md` - Actor pattern foundation
- `docs/states-and-lifecycles.md` - State machine reference
- `tasker-worker/AGENTS.md` - Handler dispatch details
- `workers/ruby/README.md` - Ruby worker reference

## Linear Tickets

| Ticket | Phase | Description |
|--------|-------|-------------|
| TAS-72 | Overview | PyO3 Python Worker Foundations |
| TAS-72-P1 | Phase 1 | Crate Foundation |
| TAS-72-P2 | Phase 2 | FFI Bridge Core |
| TAS-72-P3 | Phase 3 | Event Dispatch System |
| TAS-72-P4 | Phase 4 | Event Bridge & Handler System |
| TAS-72-P5 | Phase 5 | Domain Events & Observability |
| TAS-72-P6 | Phase 6 | Testing & Documentation |
| TAS-72-P6a | Phase 6a | E2E & Integration Tests |
| TAS-72-P7 | Phase 7 | Distribution & CI/CD |

## References

### External Documentation
- [PyO3 User Guide](https://pyo3.rs/)
- [Maturin User Guide](https://www.maturin.rs/)
- [uv Documentation](https://docs.astral.sh/uv/)
- [pyee Documentation](https://pyee.readthedocs.io/)
- [Maturin Project Layout](https://www.maturin.rs/project_layout.html)

# TAS-72-P6a: E2E and Integration Tests

## Overview

This phase establishes end-to-end and integration testing infrastructure for the Python worker, following the established patterns from Ruby and Rust workers. The goal is to verify that Python handlers can be invoked through the full tasker-core pipeline with identical behavior to Ruby handlers.

## Prerequisites

- TAS-72-P1: Crate Foundation (complete)
- TAS-72-P2: FFI Bridge Core
- TAS-72-P3: Event Dispatch
- TAS-72-P4: Handler System
- TAS-72-P5: Observability
- TAS-72-P6: Testing Framework (unit/integration tests within Python)

## Objectives

1. Copy Ruby task templates to Python with appropriate naming
2. Create Python example handlers mirroring Ruby patterns
3. Create Rust E2E tests in `tests/e2e/python/`
4. Configure Docker Compose for Python worker service
5. Update IntegrationTestManager to support Python worker
6. Establish test environment loading mechanism for Python

## Architecture Analysis

### Existing E2E Test Infrastructure

```
tests/
├── e2e/
│   ├── mod.rs                    # Entry point (add python module)
│   ├── ruby/                     # 8 test modules for Ruby handlers
│   │   ├── mod.rs
│   │   └── *_test.rs
│   ├── rust/                     # 14 test modules for Rust handlers
│   │   ├── mod.rs
│   │   └── *_test.rs
│   └── python/                   # NEW: Python E2E tests
│       ├── mod.rs
│       └── *_test.rs
├── common/
│   ├── integration_test_manager.rs    # Core test infrastructure
│   └── integration_test_utils.rs      # Helper utilities
└── fixtures/
    └── task_templates/
        ├── ruby/                 # 19 Ruby templates
        ├── rust/                 # 21 Rust templates
        └── python/               # NEW: Python templates
```

### Docker Service Configuration

Current port mapping:
- Orchestration: 8080
- Rust Worker: 8081
- Ruby Worker: 8082
- **Python Worker: 8083** (new)

### Handler Discovery Pattern (Ruby)

The Ruby worker loads handlers conditionally in test environment via:
- `lib/tasker_core/test_environment.rb` - ConditionalLoader
- `TASKER_ENV=test` triggers auto-discovery
- `TASKER_TEMPLATE_PATH` points to fixture templates

## Implementation Plan

### 1. Task Templates (`tests/fixtures/task_templates/python/`)

Copy Ruby templates with Python-specific modifications:

| Ruby Template | Python Template | Changes |
|---------------|-----------------|---------|
| `linear_workflow_handler.yaml` | `linear_workflow_py.yaml` | Handler callables, `_py` suffix |
| `diamond_workflow_handler.yaml` | `diamond_workflow_py.yaml` | Handler callables, `_py` suffix |
| `tree_workflow_handler.yaml` | `tree_workflow_py.yaml` | Handler callables, `_py` suffix |
| `mixed_dag_workflow_handler.yaml` | `mixed_dag_workflow_py.yaml` | Handler callables, `_py` suffix |
| `conditional_approval_handler.yaml` | `conditional_approval_py.yaml` | Handler callables, `_py` suffix |
| `error_testing_handler.yaml` | `error_testing_py.yaml` | Handler callables, `_py` suffix |
| `batch_processing_*.yaml` | `batch_processing_*_py.yaml` | Handler callables, `_py` suffix |

**Template Modifications:**

```yaml
# Ruby original
name: mathematical_sequence
namespace_name: linear_workflow
task_handler:
  callable: LinearWorkflow::LinearWorkflowHandler
steps:
  - name: linear_step_1
    handler:
      callable: LinearWorkflow::StepHandlers::LinearStep1Handler

# Python version
name: mathematical_sequence_py
namespace_name: linear_workflow_py
task_handler:
  callable: linear_workflow.handlers.LinearWorkflowHandler
steps:
  - name: linear_step_1
    handler:
      callable: linear_workflow.step_handlers.LinearStep1Handler
```

**Metadata Changes:**
```yaml
metadata:
  tags:
    - namespace:linear_workflow_py
    - implementation:python_ffi
    - language:python
    - type_safety:runtime
```

### 2. Python Example Handlers

#### Directory Structure

```
workers/python/
├── tests/
│   └── handlers/
│       └── examples/
│           ├── README.md
│           ├── __init__.py
│           ├── linear_workflow/
│           │   ├── __init__.py
│           │   ├── handlers/
│           │   │   ├── __init__.py
│           │   │   └── linear_workflow_handler.py
│           │   └── step_handlers/
│           │       ├── __init__.py
│           │       ├── linear_step_1_handler.py
│           │       ├── linear_step_2_handler.py
│           │       ├── linear_step_3_handler.py
│           │       └── linear_step_4_handler.py
│           ├── diamond_workflow/
│           │   ├── __init__.py
│           │   ├── handlers/
│           │   └── step_handlers/
│           ├── tree_workflow/
│           ├── mixed_dag_workflow/
│           ├── conditional_approval/
│           └── error_scenarios/
```

#### Handler Implementation Pattern

**Base Step Handler (from P4):**
```python
# workers/python/python/tasker_core/step_handler/base.py
from abc import ABC, abstractmethod
from typing import Any
from tasker_core.types import StepHandlerCallResult, Task, Sequence, Step

class StepHandlerBase(ABC):
    """Base class for all Python step handlers."""

    @abstractmethod
    def call(self, task: Task, sequence: Sequence, step: Step) -> StepHandlerCallResult:
        """Execute the step logic."""
        ...
```

**Example Linear Step Handler:**
```python
# workers/python/tests/handlers/examples/linear_workflow/step_handlers/linear_step_1_handler.py
from tasker_core.step_handler import StepHandlerBase
from tasker_core.types import StepHandlerCallResult, Task, Sequence, Step

class LinearStep1Handler(StepHandlerBase):
    """Square the initial even number (Step 1: n → n²)."""

    def call(self, task: Task, sequence: Sequence, step: Step) -> StepHandlerCallResult:
        # Get input from task context
        even_number = task.context.get("even_number")
        if even_number is None:
            return StepHandlerCallResult.failure(
                error="Missing required field: even_number"
            )

        # Perform mathematical operation
        result = even_number ** 2

        return StepHandlerCallResult.success(
            results={"result": result},
            metadata={
                "operation": "square",
                "input": even_number,
                "output": result,
            }
        )
```

**Multi-Parent Convergence Handler (Diamond End):**
```python
# workers/python/tests/handlers/examples/diamond_workflow/step_handlers/diamond_end_handler.py
from tasker_core.step_handler import StepHandlerBase
from tasker_core.types import StepHandlerCallResult, Task, Sequence, Step

class DiamondEndHandler(StepHandlerBase):
    """Converge parallel branches: multiply results then square."""

    def call(self, task: Task, sequence: Sequence, step: Step) -> StepHandlerCallResult:
        # Get results from both parallel branches
        branch_b = sequence.get_results("diamond_branch_b")
        branch_c = sequence.get_results("diamond_branch_c")

        if branch_b is None or branch_c is None:
            return StepHandlerCallResult.failure(
                error="Missing parent results"
            )

        # Multiply then square: (B × C)²
        multiplied = branch_b["result"] * branch_c["result"]
        result = multiplied ** 2

        return StepHandlerCallResult.success(
            results={"result": result},
            metadata={
                "operation": "multiply_and_square",
                "branch_b": branch_b["result"],
                "branch_c": branch_c["result"],
                "output": result,
            }
        )
```

### 3. Test Environment Loading

**Python Test Environment Loader:**
```python
# workers/python/python/tasker_core/test_environment.py
import os
import importlib
import pkgutil
from pathlib import Path
from typing import List, Type

from tasker_core.registry import HandlerRegistry

class TestEnvironmentLoader:
    """Load example handlers when TASKER_ENV=test."""

    @classmethod
    def should_load(cls) -> bool:
        """Check if we should load test handlers."""
        tasker_env = os.environ.get("TASKER_ENV", "").lower()
        return tasker_env == "test"

    @classmethod
    def load_example_handlers(cls) -> List[str]:
        """Auto-discover and load all example handlers."""
        if not cls.should_load():
            return []

        loaded = []
        examples_path = Path(__file__).parent.parent.parent / "tests" / "handlers" / "examples"

        if not examples_path.exists():
            return loaded

        # Walk through all Python files in examples directory
        for handler_file in examples_path.rglob("*_handler.py"):
            module_path = cls._file_to_module(handler_file, examples_path)
            try:
                module = importlib.import_module(module_path)
                loaded.append(module_path)
            except ImportError as e:
                print(f"Warning: Could not load {module_path}: {e}")

        return loaded

    @staticmethod
    def _file_to_module(file_path: Path, base_path: Path) -> str:
        """Convert file path to module import path."""
        relative = file_path.relative_to(base_path.parent.parent.parent)
        parts = list(relative.with_suffix("").parts)
        # tests/handlers/examples/... -> tests.handlers.examples...
        return ".".join(parts)
```

**Integration in Worker Startup:**
```python
# workers/python/python/tasker_core/worker/bootstrap.py
from tasker_core.test_environment import TestEnvironmentLoader

class WorkerBootstrap:
    def initialize(self):
        # ... other initialization ...

        # Load test handlers if in test environment
        loaded = TestEnvironmentLoader.load_example_handlers()
        if loaded:
            print(f"Loaded {len(loaded)} example handlers for testing")
```

### 4. Docker Compose Configuration

**Add to `docker/docker-compose.test.yml`:**

```yaml
  python-worker:
    build:
      context: ..
      dockerfile: docker/build/python-worker.test.Dockerfile
      cache_from:
        - type=local,src=/tmp/docker-cache/python-worker
      cache_to:
        - type=local,dest=/tmp/docker-cache/python-worker,mode=max
    environment:
      DATABASE_URL: postgresql://tasker:tasker@postgres:5432/tasker_rust_test
      TASKER_ENV: test
      RUST_LOG: info
      LOG_LEVEL: info
      RUST_BACKTRACE: 1
      WORKER_ID: test-python-worker-001
      WORKSPACE_PATH: /app
      TASKER_CONFIG_PATH: /app/config/tasker/worker-test.toml
      # Point to Python E2E test templates
      TASKER_TEMPLATE_PATH: /app/tests/fixtures/task_templates/python
      # Python-specific settings
      PYTHONPATH: /app/workers/python/python:/app/workers/python/tests
    ports:
      - "8083:8081"  # External 8083 → Internal 8081
    depends_on:
      postgres:
        condition: service_healthy
      orchestration:
        condition: service_started
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 10s
      timeout: 5s
      retries: 5
    networks:
      - tasker-network
    volumes:
      - ../tests/fixtures:/app/tests/fixtures:ro
      - ../workers/python/tests/handlers:/app/workers/python/tests/handlers:ro
```

**Dockerfile (`docker/build/python-worker.test.Dockerfile`):**

```dockerfile
# Build stage for Rust extension
FROM rust:1.75 as builder

WORKDIR /app

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./
COPY tasker-shared ./tasker-shared
COPY tasker-worker ./tasker-worker
COPY workers/python ./workers/python

# Build the Python extension
RUN cd workers/python && \
    cargo build --release

# Runtime stage
FROM python:3.10-slim

WORKDIR /app

# Install uv
RUN pip install uv

# Copy Python worker
COPY workers/python/pyproject.toml workers/python/uv.lock ./workers/python/
COPY workers/python/python ./workers/python/python
COPY workers/python/tests ./workers/python/tests

# Copy built extension from builder
COPY --from=builder /app/workers/python/target/release/*.so ./workers/python/python/tasker_core/

# Install dependencies
WORKDIR /app/workers/python
RUN uv venv && uv sync

# Run the worker
ENTRYPOINT ["uv", "run", "python", "-m", "tasker_core.worker"]
```

### 5. Rust E2E Tests

**Module Registration (`tests/e2e/mod.rs`):**

```rust
// Add Python module
pub mod python;
pub mod ruby;
pub mod rust;
```

**Python Test Module (`tests/e2e/python/mod.rs`):**

```rust
//! E2E tests for Python worker handlers
//!
//! These tests verify that Python handlers can process tasks through the
//! full tasker-core pipeline, using the same patterns as Ruby handlers.

pub mod conditional_approval_test;
pub mod diamond_workflow_test;
pub mod error_scenarios_test;
pub mod linear_workflow_test;
pub mod mixed_dag_workflow_test;
pub mod tree_workflow_test;
```

**Example Test (`tests/e2e/python/linear_workflow_test.rs`):**

```rust
//! Linear workflow E2E test for Python handlers
//!
//! Tests the sequential mathematical operations:
//! Step 1: n² (square)
//! Step 2: n² + 10 (add constant)
//! Step 3: (n² + 10) × 3 (multiply)
//! Step 4: ((n² + 10) × 3) ÷ 2 (divide)

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::{
    create_task_request, get_task_completion_timeout, wait_for_task_completion,
    IntegrationTestManager,
};

/// Set up manager with Python worker URL
async fn setup_python_worker() -> Result<IntegrationTestManager> {
    // Override worker URL to Python worker port
    std::env::set_var("TASKER_TEST_WORKER_URL", "http://localhost:8083");
    IntegrationTestManager::setup().await
}

#[tokio::test]
async fn test_python_linear_workflow_mathematical_sequence() -> Result<()> {
    let manager = setup_python_worker().await?;

    // Create task with Python namespace
    let task_request = create_task_request(
        "linear_workflow_py",
        "mathematical_sequence_py",
        json!({"even_number": 6}),
    );

    let task_response = manager.orchestration_client.create_task(task_request).await?;

    // Wait for completion
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final result
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;
    assert!(final_task.is_execution_complete());

    // Verify all steps completed
    let steps = manager.orchestration_client.list_task_steps(task_uuid).await?;
    assert_eq!(steps.len(), 4);

    for step in &steps {
        assert_eq!(
            step.current_state.to_ascii_uppercase(),
            "COMPLETE",
            "Step {} should be complete",
            step.step_name
        );
    }

    // Verify mathematical result
    // n=6: 6² = 36, 36+10 = 46, 46×3 = 138, 138÷2 = 69
    let final_step = steps.iter().find(|s| s.step_name == "linear_step_4").unwrap();
    let results: serde_json::Value = serde_json::from_value(final_step.results.clone())?;
    assert_eq!(results["result"], 69);

    Ok(())
}

#[tokio::test]
async fn test_python_linear_workflow_with_different_input() -> Result<()> {
    let manager = setup_python_worker().await?;

    // Test with different input value
    let task_request = create_task_request(
        "linear_workflow_py",
        "mathematical_sequence_py",
        json!({"even_number": 10}),
    );

    let task_response = manager.orchestration_client.create_task(task_request).await?;
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        get_task_completion_timeout(),
    )
    .await?;

    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let steps = manager.orchestration_client.list_task_steps(task_uuid).await?;

    // n=10: 10² = 100, 100+10 = 110, 110×3 = 330, 330÷2 = 165
    let final_step = steps.iter().find(|s| s.step_name == "linear_step_4").unwrap();
    let results: serde_json::Value = serde_json::from_value(final_step.results.clone())?;
    assert_eq!(results["result"], 165);

    Ok(())
}
```

**Diamond Workflow Test (`tests/e2e/python/diamond_workflow_test.rs`):**

```rust
//! Diamond workflow E2E test for Python handlers
//!
//! Pattern: A → (B, C) → D
//! Tests parallel branch execution and convergence

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::{
    create_task_request, get_task_completion_timeout, wait_for_task_completion,
    IntegrationTestManager,
};

async fn setup_python_worker() -> Result<IntegrationTestManager> {
    std::env::set_var("TASKER_TEST_WORKER_URL", "http://localhost:8083");
    IntegrationTestManager::setup().await
}

#[tokio::test]
async fn test_python_diamond_workflow_parallel_convergence() -> Result<()> {
    let manager = setup_python_worker().await?;

    let task_request = create_task_request(
        "diamond_workflow_py",
        "parallel_computation_py",
        json!({"even_number": 4}),
    );

    let task_response = manager.orchestration_client.create_task(task_request).await?;
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        get_task_completion_timeout(),
    )
    .await?;

    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let steps = manager.orchestration_client.list_task_steps(task_uuid).await?;

    // Verify all 4 steps completed
    assert_eq!(steps.len(), 4);

    // Verify step names
    let step_names: Vec<&str> = steps.iter().map(|s| s.step_name.as_str()).collect();
    assert!(step_names.contains(&"diamond_start"));
    assert!(step_names.contains(&"diamond_branch_b"));
    assert!(step_names.contains(&"diamond_branch_c"));
    assert!(step_names.contains(&"diamond_end"));

    // n=4: A=16, B=256, C=256, D=(256×256)²=4,294,967,296 = 4^16
    let final_step = steps.iter().find(|s| s.step_name == "diamond_end").unwrap();
    let results: serde_json::Value = serde_json::from_value(final_step.results.clone())?;
    assert_eq!(results["result"], 4u64.pow(16));

    Ok(())
}
```

### 6. Update IntegrationTestManager

**Add Python worker support:**

```rust
// tests/common/integration_test_manager.rs

impl IntegrationTestManager {
    /// Setup with specific worker (rust, ruby, python)
    pub async fn setup_with_worker(worker_type: WorkerType) -> Result<Self> {
        let worker_url = match worker_type {
            WorkerType::Rust => "http://localhost:8081",
            WorkerType::Ruby => "http://localhost:8082",
            WorkerType::Python => "http://localhost:8083",
        };

        std::env::set_var("TASKER_TEST_WORKER_URL", worker_url);
        Self::setup().await
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WorkerType {
    Rust,
    Ruby,
    Python,
}
```

### 7. Environment Variables

Add to `tests/e2e/python/README.md`:

```markdown
# Python E2E Tests

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TASKER_TEST_ORCHESTRATION_URL` | `http://localhost:8080` | Orchestration service URL |
| `TASKER_TEST_WORKER_URL` | `http://localhost:8083` | Python worker URL |
| `TASKER_TEST_SKIP_HEALTH_CHECK` | `false` | Skip health checks |
| `TASKER_TEST_HEALTH_TIMEOUT` | `60` | Health check timeout (seconds) |

## Running Tests

```bash
# Start services
docker compose -f docker/docker-compose.test.yml up -d

# Run Python E2E tests
TASKER_TEST_WORKER_URL=http://localhost:8083 cargo test --test e2e -- python::
```
```

## Directory Structure Summary

```
tests/
├── e2e/
│   ├── mod.rs                              # Add: pub mod python;
│   └── python/
│       ├── mod.rs
│       ├── linear_workflow_test.rs
│       ├── diamond_workflow_test.rs
│       ├── tree_workflow_test.rs
│       ├── mixed_dag_workflow_test.rs
│       ├── conditional_approval_test.rs
│       ├── error_scenarios_test.rs
│       └── README.md
└── fixtures/
    └── task_templates/
        └── python/                          # Copy from ruby/, modify handlers
            ├── linear_workflow_py.yaml
            ├── diamond_workflow_py.yaml
            ├── tree_workflow_py.yaml
            ├── mixed_dag_workflow_py.yaml
            ├── conditional_approval_py.yaml
            ├── error_testing_py.yaml
            └── ...

workers/python/
├── tests/
│   └── handlers/
│       └── examples/
│           ├── README.md
│           ├── __init__.py
│           ├── linear_workflow/
│           ├── diamond_workflow/
│           ├── tree_workflow/
│           ├── mixed_dag_workflow/
│           ├── conditional_approval/
│           └── error_scenarios/
└── python/
    └── tasker_core/
        └── test_environment.py              # Test handler loader

docker/
├── docker-compose.test.yml                  # Add python-worker service
└── build/
    └── python-worker.test.Dockerfile        # New
```

## Acceptance Criteria

- [ ] Task templates copied from Ruby with `_py` suffix and Python callables
- [ ] Example handlers implemented for all workflow patterns
- [ ] Test environment loader auto-discovers handlers when `TASKER_ENV=test`
- [ ] Docker Compose includes Python worker on port 8083
- [ ] Rust E2E tests pass for all workflow patterns
- [ ] Mathematical verification produces identical results to Ruby handlers
- [ ] IntegrationTestManager supports `WorkerType::Python`
- [ ] CI pipeline updated to run Python E2E tests

## Dependencies

This phase depends on:
- P2 (FFI Bridge) for step dispatch
- P3 (Event Dispatch) for event handling
- P4 (Handler System) for base handler classes
- P5 (Observability) for logging/metrics

## Notes

### Why Copy Ruby Templates?

Copying Ruby templates ensures:
1. **Identical test coverage** - Same workflow patterns tested
2. **Mathematical verification** - Results must match exactly
3. **Pattern consistency** - Same business logic, different implementation
4. **Easy comparison** - Side-by-side debugging possible

### Handler Callable Convention

Python uses module path notation instead of Ruby's class notation:
- Ruby: `LinearWorkflow::StepHandlers::LinearStep1Handler`
- Python: `linear_workflow.step_handlers.LinearStep1Handler`

This aligns with Python's module import system while maintaining the same logical structure.

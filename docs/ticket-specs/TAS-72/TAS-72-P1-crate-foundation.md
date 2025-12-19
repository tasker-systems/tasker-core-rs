# TAS-72-P1: Crate Foundation

## Overview

This phase establishes the foundational infrastructure for the Python worker, including directory structure, Cargo configuration, pyproject.toml, and basic FFI scaffolding.

## Objectives

1. Create `workers/python` directory structure following maturin best practices
2. Add workspace member to root `Cargo.toml`
3. Configure PyO3 with appropriate features
4. Set up Python packaging with `uv` and `maturin`
5. Implement minimal FFI module that can be imported
6. Establish CI/CD foundation for Python tests

## Directory Structure

```
workers/python/
├── src/                           # Rust FFI source
│   └── lib.rs                     # PyO3 module initialization
├── python/                        # Python source (maturin src layout)
│   └── tasker_core/
│       ├── __init__.py            # Package init, re-exports FFI
│       ├── _tasker_core.pyi       # Type stubs for FFI module
│       └── py.typed               # PEP 561 marker
├── tests/                         # pytest tests
│   ├── __init__.py
│   ├── conftest.py                # pytest fixtures
│   └── test_import.py             # Basic import tests
├── build.rs                       # Build script for rustc version
├── Cargo.toml                     # Rust crate configuration
├── pyproject.toml                 # Python project configuration
├── README.md                      # Crate documentation
└── .gitignore                     # Python/Rust ignores
```

## Implementation Details

### 1. Cargo.toml

```toml
[package]
name = "tasker-worker-py"
version = "0.1.0"
edition = "2021"
description = "Python FFI bindings for tasker-core worker"
license = "MIT"
rust-version = "1.70"

[lib]
name = "_tasker_core"
crate-type = ["cdylib"]

[dependencies]
# PyO3 with extension-module feature for building Python extension
pyo3 = { version = "0.22", features = ["extension-module", "abi3-py310"] }

# Serialization for Python type conversion
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
pythonize = "0.22"

# Workspace dependencies (added in later phases)
# tasker-worker = { path = "../../tasker-worker" }
# tasker-shared = { path = "../../tasker-shared" }

# Async runtime (needed for FFI blocking)
tokio = { version = "1.0", features = ["rt-multi-thread", "sync"] }

# Logging
tracing = "0.1"

[dev-dependencies]
pyo3 = { version = "0.22", features = ["auto-initialize"] }

[features]
default = []
# Enable when we integrate with tasker-worker
# worker-integration = ["tasker-worker", "tasker-shared"]
```

### 2. pyproject.toml

```toml
[project]
name = "tasker-core-py"
version = "0.1.0"
description = "Python worker for Tasker workflow orchestration"
readme = "README.md"
license = { text = "MIT" }
requires-python = ">=3.10"
classifiers = [
    "Development Status :: 3 - Alpha",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: MIT License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
    "Programming Language :: Rust",
    "Topic :: Software Development :: Libraries :: Python Modules",
]
dependencies = [
    "pydantic>=2.9.0",
    "pydantic-settings>=2.6.0",
    "pyee>=12.0.0",
]

[project.optional-dependencies]
dev = [
    "maturin>=1.7,<2.0",
    "pytest>=8.0.0",
    "pytest-asyncio>=0.23.0",
    "pytest-cov>=5.0.0",
    "ruff>=0.8.0",
    "mypy>=1.10.0",
]

# uv dependency groups for native uv sync support
[dependency-groups]
dev = [
    "maturin>=1.7,<2.0",
    "pytest>=8.0.0",
    "pytest-asyncio>=0.23.0",
    "pytest-cov>=5.0.0",
    "ruff>=0.8.0",
    "mypy>=1.10.0",
]

[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[tool.maturin]
# Python source location (mixed project layout)
python-source = "python"
# Module name: tasker_core._tasker_core (internal FFI module)
module-name = "tasker_core._tasker_core"
# Build with stable ABI for Python 3.10+
features = ["pyo3/extension-module", "pyo3/abi3-py310"]
# Strip symbols for smaller binaries
strip = true

[tool.pytest.ini_options]
testpaths = ["tests"]
python_files = ["test_*.py"]
python_classes = ["Test*"]
python_functions = ["test_*"]
asyncio_mode = "auto"
addopts = [
    "-v",
    "--tb=short",
    "--strict-markers",
]

[tool.ruff]
target-version = "py310"
line-length = 100
src = ["python", "tests"]

[tool.ruff.lint]
select = [
    "E",      # pycodestyle errors
    "W",      # pycodestyle warnings
    "F",      # pyflakes
    "I",      # isort
    "B",      # flake8-bugbear
    "C4",     # flake8-comprehensions
    "UP",     # pyupgrade
    "ARG",    # flake8-unused-arguments
    "SIM",    # flake8-simplify
]
ignore = [
    "E501",   # line too long (handled by formatter)
]

[tool.ruff.lint.isort]
known-first-party = ["tasker_core"]

[tool.mypy]
python_version = "3.10"
strict = true
warn_return_any = true
warn_unused_configs = true
exclude = [
    "tests/",
]

[[tool.mypy.overrides]]
module = "tasker_core._tasker_core"
ignore_missing_imports = true
```

### 3. Rust Source (src/lib.rs)

```rust
//! PyO3 bindings for tasker-core Python worker
//!
//! This module provides the FFI interface between Rust and Python,
//! exposing worker functionality through the `_tasker_core` module.

use pyo3::prelude::*;

/// Returns the version of the tasker-core-py package
#[pyfunction]
fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns the Rust library version for debugging
#[pyfunction]
fn get_rust_version() -> String {
    format!(
        "tasker-worker-py {} (rustc {})",
        env!("CARGO_PKG_VERSION"),
        env!("RUSTC_VERSION")  // Populated by build.rs
    )
}

/// Check if the FFI module is working correctly
#[pyfunction]
fn health_check() -> bool {
    true
}

/// The main Python module for tasker-core FFI
///
/// This is the low-level FFI module. The public API is exposed
/// through the `tasker_core` Python package.
#[pymodule]
fn _tasker_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Version information
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    m.add_function(wrap_pyfunction!(get_rust_version, m)?)?;
    m.add_function(wrap_pyfunction!(health_check, m)?)?;

    // Module metadata
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
```

### 4. Build Script (build.rs)

```rust
fn main() {
    // Capture rustc version at build time
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .expect("Failed to get rustc version");

    let version = String::from_utf8_lossy(&output.stdout);
    let version = version.trim();

    println!("cargo:rustc-env=RUSTC_VERSION={}", version);
    println!("cargo:rerun-if-changed=build.rs");
}
```

### 5. Python Package (python/tasker_core/__init__.py)

```python
"""
Tasker Core Python Worker

This package provides Python bindings for the tasker-core workflow
orchestration system, enabling Python step handlers to integrate
with Rust-based orchestration.

Example:
    >>> import tasker_core
    >>> tasker_core.version()
    '0.1.0'
"""

from __future__ import annotations

from tasker_core._tasker_core import (
    __version__,
    get_rust_version,
    get_version,
    health_check,
)

__all__ = [
    "__version__",
    "get_version",
    "get_rust_version",
    "health_check",
]


def version() -> str:
    """Return the package version."""
    return get_version()
```

### 6. Type Stubs (python/tasker_core/_tasker_core.pyi)

```python
"""Type stubs for the Rust FFI module.

This file provides type hints for the _tasker_core extension module
which is compiled from Rust using PyO3.
"""

# Version of the package from Cargo.toml
__version__: str

def get_version() -> str:
    """Return the package version string."""
    ...

def get_rust_version() -> str:
    """Return the Rust library version for debugging."""
    ...

def health_check() -> bool:
    """Check if the FFI module is working correctly."""
    ...
```

### 7. Python Type Marker (python/tasker_core/py.typed)

```
# PEP 561 marker file
# This package uses inline type hints
```

### 8. Test File (tests/test_import.py)

```python
"""Basic import and functionality tests for tasker_core."""

import pytest


def test_import_module():
    """Test that the module can be imported."""
    import tasker_core
    assert tasker_core is not None


def test_version():
    """Test that version is accessible."""
    import tasker_core
    version = tasker_core.version()
    assert isinstance(version, str)
    assert len(version) > 0


def test_rust_version():
    """Test that Rust version info is accessible."""
    import tasker_core
    rust_version = tasker_core.get_rust_version()
    assert isinstance(rust_version, str)
    assert "tasker-worker-py" in rust_version
    assert "rustc" in rust_version


def test_health_check():
    """Test that health check passes."""
    import tasker_core
    assert tasker_core.health_check() is True


def test_dunder_version():
    """Test that __version__ is exposed."""
    import tasker_core
    assert hasattr(tasker_core, "__version__")
    assert tasker_core.__version__ == tasker_core.version()
```

### 9. Test Configuration (tests/conftest.py)

```python
"""pytest configuration and fixtures for tasker_core tests."""

import pytest


@pytest.fixture(scope="session")
def tasker_core_module():
    """Provide the tasker_core module as a fixture."""
    import tasker_core
    return tasker_core
```

### 10. README.md

```markdown
# tasker-core-py

Python worker bindings for the Tasker workflow orchestration system.

## Installation

### From Source (Development)

```bash
# Install uv (if not already installed)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Create virtual environment and sync dependencies
uv venv
source .venv/bin/activate
uv sync --group dev

# Build and install the Rust extension
uv run maturin develop
```

### From PyPI (when published)

```bash
pip install tasker-core-py
```

## Quick Start

```python
import tasker_core

# Check version
print(tasker_core.version())

# Verify FFI is working
assert tasker_core.health_check()
```

## Development

### Prerequisites

- Python 3.10+
- Rust 1.70+
- uv (recommended) or pip

### Setup

```bash
# Create virtual environment
uv venv
source .venv/bin/activate

# Sync all dependencies (including dev)
uv sync --group dev

# Build and install in development mode
uv run maturin develop

# Run tests
uv run pytest

# Run linting
uv run ruff check .
uv run mypy python/
```

### Building Wheels

```bash
# Build wheel for current platform
uv run maturin build --release

# Build with all features
uv run maturin build --release --all-features
```

## License

MIT
```

### 11. .gitignore

```gitignore
# Python
__pycache__/
*.py[cod]
*$py.class
*.so
.Python
build/
develop-eggs/
dist/
downloads/
eggs/
.eggs/
lib/
lib64/
parts/
sdist/
var/
wheels/
*.egg-info/
.installed.cfg
*.egg

# Virtual environments
.venv/
venv/
ENV/

# Rust
target/

# Maturin
*.whl

# IDE
.idea/
.vscode/
*.swp
*.swo

# Testing
.pytest_cache/
.coverage
htmlcov/
.tox/
.nox/

# mypy
.mypy_cache/
```

## Workspace Integration

### Root Cargo.toml Update

Add to the workspace members in the root `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing members ...
    "workers/python",
]
```

## Development Commands

```bash
# Navigate to workers/python
cd workers/python

# Create virtual environment with uv
uv venv
source .venv/bin/activate

# Sync all dependencies (including dev)
uv sync --group dev

# Build and install in development mode
uv run maturin develop

# Run tests
uv run pytest -v

# Run linting
uv run ruff check .
uv run ruff format --check .

# Run type checking
uv run mypy python/

# Build release wheel
uv run maturin build --release
```

## CI/CD Configuration

### GitHub Actions Workflow (.github/workflows/python.yml)

```yaml
name: Python CI

on:
  push:
    paths:
      - 'workers/python/**'
  pull_request:
    paths:
      - 'workers/python/**'

defaults:
  run:
    working-directory: workers/python

jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        python-version: ['3.10', '3.11', '3.12', '3.13']

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: ${{ matrix.python-version }}

      - name: Set up Rust
        uses: dtolnay/rust-action@stable

      - name: Install uv
        uses: astral-sh/setup-uv@v4

      - name: Sync dependencies
        run: uv sync --group dev

      - name: Build extension
        run: uv run maturin develop

      - name: Run tests
        run: uv run pytest -v

      - name: Run linting
        run: uv run ruff check .

      - name: Run type checking
        run: uv run mypy python/

  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up Rust
        uses: dtolnay/rust-action@stable
        with:
          components: clippy, rustfmt

      - name: Check Rust formatting
        run: cargo fmt --check

      - name: Run Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
```

## Acceptance Criteria

- [ ] `workers/python` directory created with proper structure
- [ ] Workspace member added to root `Cargo.toml`
- [ ] `maturin develop` builds successfully
- [ ] `import tasker_core` works in Python
- [ ] `tasker_core.version()` returns correct version
- [ ] `tasker_core.health_check()` returns `True`
- [ ] `pytest` runs and passes all tests
- [ ] `ruff check .` passes
- [ ] `mypy python/` passes

## Notes

### Why `_tasker_core` as Internal Module?

Following maturin best practices, the compiled Rust extension is named `_tasker_core` (with underscore prefix) to indicate it's an internal implementation detail. The public API is exposed through the `tasker_core` Python package, which re-exports the necessary functions with proper docstrings and type hints.

This pattern:
1. Keeps the public API in pure Python for documentation
2. Allows adding Python-only utilities alongside FFI functions
3. Makes the package structure cleaner for users

### ABI3 Stable ABI

Using `pyo3/abi3-py310` means we build against Python's stable ABI with a minimum version of 3.10. This produces a single wheel that works across Python 3.10, 3.11, 3.12, and 3.13 without needing separate builds.

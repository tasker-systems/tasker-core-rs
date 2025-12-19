# tasker-core-py

Python worker bindings for the Tasker workflow orchestration system.

## Status

**Phase 1 (TAS-72-P1)**: Crate Foundation - Minimal FFI verification.

See `docs/ticket-specs/TAS-72/` for the full implementation plan.

## Installation

### From Source (Development)

```bash
# Install uv (if not already installed)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Create virtual environment and sync dependencies
uv venv
source .venv/bin/activate
uv sync --dev

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
uv sync --dev

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
maturin build --release

# Build with all features
maturin build --release --all-features
```

## Project Structure

```
workers/python/
├── src/                           # Rust FFI source
│   └── lib.rs                     # PyO3 module initialization
├── python/                        # Python source (maturin src layout)
│   └── tasker_core/
│       ├── __init__.py            # Package init, re-exports FFI
│       └── py.typed               # PEP 561 marker
├── tests/                         # pytest tests
│   ├── conftest.py
│   └── test_import.py
├── Cargo.toml                     # Rust crate configuration
├── pyproject.toml                 # Python project configuration
└── README.md
```

## Technology Stack

- **FFI Layer**: PyO3
- **Build Tool**: maturin
- **Package Manager**: uv
- **Testing**: pytest
- **Data Models**: Pydantic v2 (future phases)
- **Event Bus**: pyee (future phases)

## License

MIT

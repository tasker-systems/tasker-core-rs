# TAS-72-P7: Distribution & CI/CD

## Overview

This phase focuses on package distribution and deployment automation, including wheel builds for multiple platforms, PyPI publication, GitHub Actions CI/CD, and Docker base images.

## Prerequisites

- TAS-72-P1 through P6 complete
- All tests passing
- Documentation complete

## Objectives

1. Build wheels for multiple platforms (Linux, macOS, Windows)
2. Configure manylinux compliance for Linux wheels
3. Set up PyPI publication workflow
4. Create comprehensive GitHub Actions CI/CD
5. Build Docker base image with Python worker

## Platform Support Matrix

| Platform | Architecture | Python Versions | Notes |
|----------|--------------|-----------------|-------|
| Linux (manylinux2014) | x86_64 | 3.10, 3.11, 3.12, 3.13 | Primary target |
| Linux (manylinux2014) | aarch64 | 3.10, 3.11, 3.12, 3.13 | ARM servers |
| macOS | x86_64 | 3.10, 3.11, 3.12, 3.13 | Intel Macs |
| macOS | arm64 | 3.10, 3.11, 3.12, 3.13 | Apple Silicon |
| Windows | x86_64 | 3.10, 3.11, 3.12, 3.13 | Optional |

## GitHub Actions Workflows

### Main CI Workflow (.github/workflows/python-ci.yml)

```yaml
name: Python CI

on:
  push:
    branches: [main]
    paths:
      - 'workers/python/**'
      - '.github/workflows/python-ci.yml'
  pull_request:
    paths:
      - 'workers/python/**'
      - '.github/workflows/python-ci.yml'

defaults:
  run:
    working-directory: workers/python

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.12'

      - name: Set up Rust
        uses: dtolnay/rust-action@stable
        with:
          components: clippy, rustfmt

      - name: Install uv
        uses: astral-sh/setup-uv@v4

      - name: Create venv and install deps
        run: |
          uv venv
          source .venv/bin/activate
          uv pip install ruff mypy

      - name: Check Rust formatting
        run: cargo fmt --check

      - name: Run Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Check Python formatting
        run: |
          source .venv/bin/activate
          ruff format --check .

      - name: Run Ruff linting
        run: |
          source .venv/bin/activate
          ruff check .

      - name: Run mypy
        run: |
          source .venv/bin/activate
          mypy python/

  test:
    runs-on: ${{ matrix.os }}
    needs: lint
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
        python-version: ['3.10', '3.11', '3.12', '3.13']

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python ${{ matrix.python-version }}
        uses: actions/setup-python@v5
        with:
          python-version: ${{ matrix.python-version }}

      - name: Set up Rust
        uses: dtolnay/rust-action@stable

      - name: Install uv
        uses: astral-sh/setup-uv@v4

      - name: Create venv and install deps
        run: |
          uv venv
          source .venv/bin/activate
          uv pip install maturin pytest pytest-cov

      - name: Build extension
        run: |
          source .venv/bin/activate
          maturin develop

      - name: Run tests
        run: |
          source .venv/bin/activate
          pytest -v --cov=tasker_core --cov-report=xml

      - name: Upload coverage
        uses: codecov/codecov-action@v4
        if: matrix.os == 'ubuntu-latest' && matrix.python-version == '3.12'
        with:
          files: workers/python/coverage.xml
          flags: python

  integration-test:
    runs-on: ubuntu-latest
    needs: test
    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_USER: tasker
          POSTGRES_PASSWORD: tasker
          POSTGRES_DB: tasker_test
        ports:
          - 5432:5432
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.12'

      - name: Set up Rust
        uses: dtolnay/rust-action@stable

      - name: Install uv
        uses: astral-sh/setup-uv@v4

      - name: Create venv and install deps
        run: |
          uv venv
          source .venv/bin/activate
          uv pip install maturin pytest

      - name: Build extension
        run: |
          source .venv/bin/activate
          maturin develop

      - name: Run integration tests
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_test
        run: |
          source .venv/bin/activate
          pytest tests/integration/ -v -m integration
```

### Release Workflow (.github/workflows/python-release.yml)

```yaml
name: Python Release

on:
  push:
    tags:
      - 'python-v*'
  workflow_dispatch:
    inputs:
      version:
        description: 'Version to release (e.g., 0.1.0)'
        required: true

defaults:
  run:
    working-directory: workers/python

permissions:
  contents: read
  id-token: write  # For PyPI trusted publishing

jobs:
  build-wheels:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          # Linux x86_64
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            manylinux: '2014'
          # Linux aarch64
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            manylinux: '2014'
          # macOS x86_64
          - os: macos-13
            target: x86_64-apple-darwin
          # macOS arm64
          - os: macos-latest
            target: aarch64-apple-darwin
          # Windows x86_64
          - os: windows-latest
            target: x86_64-pc-windows-msvc

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.12'

      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          target: ${{ matrix.target }}
          manylinux: ${{ matrix.manylinux || 'auto' }}
          args: --release --out dist
          working-directory: workers/python

      - name: Upload wheels
        uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.target }}
          path: workers/python/dist/*.whl

  build-sdist:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Build sdist
        uses: PyO3/maturin-action@v1
        with:
          command: sdist
          args: --out dist
          working-directory: workers/python

      - name: Upload sdist
        uses: actions/upload-artifact@v4
        with:
          name: sdist
          path: workers/python/dist/*.tar.gz

  publish:
    runs-on: ubuntu-latest
    needs: [build-wheels, build-sdist]
    environment:
      name: pypi
      url: https://pypi.org/project/tasker-core-py/

    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true

      - name: Publish to PyPI
        uses: pypa/gh-action-pypi-publish@release/v1
        with:
          packages-dir: dist/
```

## Docker Configuration

### Dockerfile

```dockerfile
# Multi-stage build for Python worker
FROM python:3.12-slim as builder

# Install Rust
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install maturin
RUN pip install maturin

# Copy source
WORKDIR /build
COPY workers/python ./workers/python
COPY tasker-worker ./tasker-worker
COPY tasker-shared ./tasker-shared
COPY Cargo.toml Cargo.lock ./

# Build wheel
WORKDIR /build/workers/python
RUN maturin build --release --out dist

# Runtime stage
FROM python:3.12-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

# Copy wheel from builder
COPY --from=builder /build/workers/python/dist/*.whl /tmp/

# Install package
RUN pip install /tmp/*.whl && rm /tmp/*.whl

# Create non-root user
RUN useradd -m -u 1000 tasker
USER tasker

WORKDIR /app

# Default command
CMD ["python", "-c", "import tasker_core; print(f'tasker-core-py {tasker_core.version()}')"]
```

### docker-compose.yml (Development)

```yaml
version: '3.8'

services:
  python-worker:
    build:
      context: ../..
      dockerfile: workers/python/Dockerfile
    environment:
      - DATABASE_URL=postgresql://tasker:tasker@postgres:5432/tasker_dev
      - RUST_LOG=info
    depends_on:
      postgres:
        condition: service_healthy
    volumes:
      - ./handlers:/app/handlers
    command: ["python", "-m", "tasker_core", "run"]

  postgres:
    image: postgres:16
    environment:
      POSTGRES_USER: tasker
      POSTGRES_PASSWORD: tasker
      POSTGRES_DB: tasker_dev
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U tasker"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
```

## PyPI Configuration

### Trusted Publishing Setup

1. Create PyPI project `tasker-core-py`
2. Configure trusted publishing:
   - Owner: `tasker-systems`
   - Repository: `tasker-core`
   - Workflow name: `python-release.yml`
   - Environment: `pypi`

### Package Metadata (pyproject.toml)

```toml
[project]
name = "tasker-core-py"
dynamic = ["version"]
authors = [
    { name = "Tasker Systems", email = "dev@tasker-systems.dev" }
]
maintainers = [
    { name = "Pete Taylor", email = "pete@tasker-systems.dev" }
]
keywords = [
    "workflow",
    "orchestration",
    "task",
    "rust",
    "pyo3",
]

[project.urls]
Homepage = "https://github.com/tasker-systems/tasker-core"
Documentation = "https://docs.tasker-systems.dev/python"
Repository = "https://github.com/tasker-systems/tasker-core"
Changelog = "https://github.com/tasker-systems/tasker-core/blob/main/workers/python/CHANGELOG.md"
```

## Release Process

### Version Bumping

```bash
# Update version in:
# - workers/python/Cargo.toml
# - workers/python/python/tasker_core/_version.py

# Commit and tag
git add workers/python/Cargo.toml workers/python/python/tasker_core/_version.py
git commit -m "chore(python): release v0.1.0"
git tag python-v0.1.0
git push origin main --tags
```

### Changelog (CHANGELOG.md)

```markdown
# Changelog

All notable changes to tasker-core-py will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-XX-XX

### Added
- Initial release
- PyO3-based FFI bindings for tasker-core worker
- Pydantic models for type-safe data handling
- EventBridge for in-process pub/sub
- HandlerRegistry for handler discovery
- StepHandler base class
- Bootstrap/shutdown lifecycle management
- Domain event publishing
- Observability (health checks, metrics)
- pytest test suite
- Documentation
```

## Acceptance Criteria

- [ ] Wheels build for all target platforms
- [ ] manylinux2014 compliance verified
- [ ] PyPI trusted publishing configured
- [ ] GitHub Actions workflows pass
- [ ] Docker image builds successfully
- [ ] Package installable via `pip install tasker-core-py`
- [ ] Version bumping process documented
- [ ] CHANGELOG maintained

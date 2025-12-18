# GitHub Actions CI/CD Workflows

This directory contains the CI/CD pipeline for tasker-core, implementing an optimized DAG-based architecture for maximum parallelism and cache efficiency.

## Workflow Overview

### Main Pipeline

**File**: `ci.yml`

The main orchestrator implementing a multi-stage DAG with maximum parallelism.

**Pipeline DAG**:
```
                              build-postgres
                                    │
                 ┌──────────────────┼──────────────────┐
                 │                  │                  │
                 ▼                  ▼                  ▼
          code-quality      workspace-compile     (parallel)
          ┌─────────┐       ┌──────────────┐
          │ fmt     │       │ cargo build  │
          │ clippy  │       │ --workspace  │
          │ audit   │       │ (core crates)│
          │ doc     │       │ warms sccache│
          └─────────┘       └──────┬───────┘
                                   │
                      ┌────────────┴────────────┐
                      │                         │
                      ▼                         ▼
                unit-tests               build-workers
           ┌───────────────┐          ┌──────────────────┐
           │ nextest --lib │          │ workers/rust     │
           │ + doctests    │          │ workers/ruby FFI │
           │ (core pkgs)   │          │ workers/python   │
           └───────────────┘          └────────┬─────────┘
                                               │
                           ┌───────────────────┴───────────────────┐
                           │                                       │
                           ▼                                       ▼
                    worker-tests                          integration-tests
               ┌───────────────────┐                  ┌─────────────────────┐
               │ Ruby: rspec       │                  │ start-native-services│
               │ Python: pytest    │                  │ nextest matrix (2)  │
               │ Rust: worker tests│                  │   - partition 1/2   │
               └───────────────────┘                  │   - partition 2/2   │
                           │                          └──────────┬──────────┘
                           │                                     │
                           └──────────────┬──────────────────────┘
                                          │
                                          ▼
                                 performance-analysis
                                          │
                                          ▼
                                      ci-success
                                 (requires all above)
```

**Key Optimizations (TAS-88)**:
- **sccache warming**: `workspace-compile` builds shared dependencies first, warming cache for all subsequent jobs
- **Maximum parallelism**: `code-quality` runs alongside `workspace-compile`
- **Fast feedback**: `unit-tests` start immediately after core compilation (no waiting for FFI builds)
- **Worker isolation**: FFI builds happen in parallel with unit tests
- **Test separation**: Framework tests (rspec/pytest) run independently from integration tests
- **Nextest partitioning**: Integration tests split across 2 parallel runners
- **Reduced duplication**: Each job reuses sccache artifacts from prior jobs
- **Native-only execution**: Removed Docker mode for simplicity (~50% faster CI)

**Core Packages** (compiled in workspace-compile):
- tasker-orchestration
- tasker-shared
- tasker-worker
- pgmq-notify
- tasker-client

**Worker Packages** (compiled in build-workers):
- workers/rust
- workers/ruby (FFI extension via magnus)
- workers/python (FFI extension via maturin/pyo3)

---

## Test Workflows

### Workspace Compile (`test-integration.yml` - build-and-unit-tests job)

**Purpose**: Compile core packages and warm sccache for downstream jobs

**Compiles**:
- tasker-orchestration
- tasker-shared
- tasker-worker
- pgmq-notify
- tasker-client

**Key Features**:
- Uses sccache for distributed compilation caching
- Uploads compiled binaries as artifacts
- Creates nextest archive for partitioned test execution
- Does NOT build FFI extensions (deferred to build-workers)

**Duration**: ~5-8 minutes (faster on cache hit)

---

### Unit Tests (`test-integration.yml` - unit-tests job)

**Purpose**: Run library tests and doctests for core packages

**Runs**:
```bash
# Unit tests via nextest
cargo nextest run --profile ci --lib \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client

# Documentation tests
cargo test --doc \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client
```

**Duration**: ~2-3 minutes

---

### Build Workers (`test-integration.yml` - build-workers job)

**Purpose**: Compile FFI extensions for Ruby and Python workers

**Builds**:
- `workers/rust` - Rust worker binary
- `workers/ruby` - Ruby FFI extension (magnus)
- `workers/python` - Python FFI extension (maturin/pyo3)

**Key Features**:
- Runs in parallel with unit-tests
- Benefits from sccache warmed by workspace-compile
- Uploads artifacts for integration tests

**Duration**: ~6-8 minutes

---

### Integration Tests (`test-integration.yml` - integration-tests job)

**Purpose**: End-to-end testing with all services running

**Execution**:
- **Mode**: Native binary execution only (Docker mode removed for simplicity)
- **Partitioning**: Tests split across 2 parallel runners via nextest
- **Duration**: ~5-7 minutes per partition

**How it works**:
1. Downloads pre-compiled binaries and nextest archive from build jobs
2. Starts native services (orchestration, rust-worker, ruby-worker, python-worker)
3. Runs partitioned tests against native services

**Command**:
```bash
# Generate test configurations
cargo run --package tasker-client --bin tasker-cli -- config generate \
  --context orchestration --environment test \
  --source-dir config/tasker \
  --output config/tasker/orchestration-test.toml

# Start native services
.github/scripts/start-native-services.sh

# Run partitioned tests
cargo nextest run \
  --archive-file nextest-archive.tar.zst \
  --profile ci \
  --partition count:1/2 \  # or 2/2 for second partition
  --test '*' --no-fail-fast

# Stop services
.github/scripts/stop-native-services.sh
```

**Key Features**:
- **Nextest partitioning**: 2 parallel runners for faster execution
- **Native-only**: Simplified architecture, no Docker complexity
- Health checks for all services before testing
- Comprehensive service logs on failure
- JUnit XML output for CI reporting

---

### Doctests

**Purpose**: Validate documentation examples compile correctly

**Runs**:
```bash
cargo test --doc --all-features \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client \
  --package tasker-core
```

**Requirements**:
- SQLX_OFFLINE=true (no database needed)

**Duration**: ~1 minute

**Key Features**:
- Ensures documentation examples are valid across all packages
- Runs after comprehensive tests complete
- No external dependencies required

---

### Ruby Framework Tests (`test-ruby-framework.yml`)

**Purpose**: Ruby-specific framework testing with PostgreSQL service

**Runs**:
- FFI layer tests (`workers/ruby/spec/ffi/`)
- Type wrapper tests (`workers/ruby/spec/types/`)
- Worker core tests (`workers/ruby/spec/worker/`)

**Requirements**:
- Ruby 3.4
- Rust toolchain (for FFI extension compilation)
- PostgreSQL service (for sqlx query validation during compilation)
- Bundle install + rake compile

**Command**: `bundle exec rspec spec/ --exclude-pattern spec/integration/**/*_spec.rb`

**Test Count**: 77 tests

**Duration**: ~2-3 minutes

**Key Features**:
- PostgreSQL service container for FFI compilation (sqlx query validation)
- Fast feedback (lightweight service, no full Docker Compose)
- Validates Ruby FFI bindings and Rust integration
- Tests Ruby-specific framework logic
- Runs after comprehensive tests (Docker workers already shut down)
- Explicitly excludes integration tests (run in E2E suite)

---

## Support Workflows

### Build PostgreSQL Image (`build-postgres.yml`)

**Purpose**: Build and cache PostgreSQL with PGMQ extension

**Outputs**:
- `postgres-image`: PostgreSQL with PGMQ and pg_uuidv7 extensions

**Usage**:
- Shared across all CI jobs requiring database access
- Used by code-quality, comprehensive-tests, and ruby-framework-tests
- Built once per CI run, cached via GitHub Actions cache

---

### Code Quality (`code-quality.yml`)

**Purpose**: Linting and formatting checks

**Runs**:
- `cargo clippy`
- `cargo fmt --check`
- Ruby linting (if configured)

**Duration**: ~2-3 minutes

---

### CI Success (`ci-success.yml`)

**Purpose**: Final success gate for branch protection

**Status**: Passes only if all required jobs succeed

---

## Test Organization

### E2E Tests (Language-Agnostic)

**Location**: `tests/e2e/`

**Purpose**: Black-box API testing regardless of worker language

**Structure**:
```
tests/e2e/
├── rust/          # Rust worker scenarios
│   ├── linear_workflow.rs
│   ├── diamond_workflow.rs
│   └── ...
└── ruby/          # Ruby FFI worker scenarios
    ├── error_scenarios_test.rs
    └── ...
```

**Philosophy**: Tests call orchestration APIs and verify responses. The handler implementation language is irrelevant.

---

### Framework Tests (Language-Specific)

**Ruby Location**: `workers/ruby/spec/`

**Purpose**: Test Ruby-specific framework concerns

**Structure**:
```
workers/ruby/spec/
├── ffi/           # FFI bootstrap and calls
├── types/         # Type wrappers
├── worker/        # Worker core logic
├── fixtures/      # Test templates and handlers
└── handlers/      # Test handler implementations
```

**Philosophy**: Tests Ruby code that interacts with Rust FFI layer. No distributed system testing.

---

## Running Tests Locally

### Integration Tests (Native - Fast)

```bash
# Generate test configurations
mkdir -p config/v2
cargo run --quiet --package tasker-client --bin tasker-cli -- config generate \
  --context orchestration --environment test \
  --source-dir config/v2 \
  --output config/v2/orchestration-test.toml
cargo run --quiet --package tasker-client --bin tasker-cli -- config generate \
  --context worker --environment test \
  --source-dir config/v2 \
  --output config/v2/worker-test.toml

# Build all binaries
cargo build --all-features --all-targets

# Start native services
.github/scripts/start-native-services.sh

# Run all integration tests (all 482 tests)
cargo nextest run \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client \
  --package tasker-core \
  --test '*' \
  --no-fail-fast

# Cleanup
.github/scripts/stop-native-services.sh
```

### Integration Tests (Docker - Production-Like)

```bash
# Start Docker services
docker compose -f docker/docker-compose.test.yml up --build -d

# Wait for services
curl http://localhost:8080/health  # Orchestration
curl http://localhost:8081/health  # Rust worker
curl http://localhost:8082/health  # Ruby worker (may take 60s)

# Run all integration tests (all 482 tests)
cargo nextest run \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client \
  --package tasker-core \
  --test '*' \
  --no-fail-fast

# Cleanup
docker compose -f docker/docker-compose.test.yml down -v
```

### Doctests

```bash
# Run doctests (no Docker needed)
SQLX_OFFLINE=true cargo test --doc --all-features \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client \
  --package tasker-core
```

### Ruby Framework Tests

```bash
# Start PostgreSQL (needed for FFI compilation with sqlx)
docker compose -f docker/docker-compose.test.yml up -d postgres

# Wait for PostgreSQL to be ready
until pg_isready -h localhost -p 5432 -U tasker; do sleep 1; done

cd workers/ruby
bundle install

# Compile with database connection for sqlx query validation
DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test \
  bundle exec rake compile

# Run framework tests (excludes integration tests)
bundle exec rspec spec/ --exclude-pattern spec/integration/**/*_spec.rb

# Cleanup
docker compose -f docker/docker-compose.test.yml down
```

---

## Adding New Tests

### Adding Integration or E2E Test

1. Create test file in appropriate location:
   - Integration: `tests/integration/{workflow_type}/`
   - E2E (Rust): `tests/e2e/rust/`
   - E2E (Ruby): `tests/e2e/ruby/`
2. Use `IntegrationTestManager` or `LifecycleTestManager` for setup
3. Test runs automatically via comprehensive test suite
4. No CI changes needed

**Example (Integration Test)**:
```rust
// tests/integration/my_workflow/happy_path.rs
use crate::common::lifecycle_test_manager::LifecycleTestManager;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_my_workflow(pool: PgPool) -> Result<()> {
    let manager = LifecycleTestManager::new(pool).await?;
    // ... test logic ...
    Ok(())
}
```

**Example (E2E Test)**:
```rust
// tests/e2e/ruby/my_new_test.rs
use crate::common::integration_test_manager::IntegrationTestManager;

#[tokio::test]
async fn test_my_scenario() -> anyhow::Result<()> {
    let manager = IntegrationTestManager::setup().await?;
    // ... test logic ...
    Ok(())
}
```

### Adding Ruby Framework Test

1. Create test file in `workers/ruby/spec/{ffi,types,worker}/`
2. No Docker dependencies
3. Test runs automatically via `bundle exec rspec spec/`
4. No CI changes needed

---

## Debugging CI Failures

### Comprehensive Test Failures

1. **Check service logs**: Download `e2e-service-logs` artifact (if Docker-related)
2. **Check test output**: Download `e2e-test-results` artifact (JUnit XML)
3. **Reproduce locally**:
   ```bash
   docker compose -f docker/docker-compose.test.yml up --build -d
   cargo nextest run \
     --package tasker-shared \
     --package tasker-orchestration \
     --package tasker-worker \
     --package pgmq-notify \
     --package tasker-client \
     --package tasker-core \
     --no-fail-fast \
     -- --nocapture
   ```

### Doctest Failures

1. **Check compilation errors** in CI logs
2. **Reproduce locally**:
   ```bash
   SQLX_OFFLINE=true cargo test --doc --all-features \
     --package tasker-shared \
     --package tasker-orchestration \
     --package tasker-worker \
     --package pgmq-notify \
     --package tasker-client \
     --package tasker-core \
     -- --nocapture
   ```

### Ruby Framework Test Failures

1. **Check test output**: Download `ruby-framework-test-results` artifact
2. **Reproduce locally**:
   ```bash
   cd workers/ruby
   bundle exec rake compile
   bundle exec rspec spec/ --exclude-pattern spec/integration/**/*_spec.rb --format documentation
   ```

### Common Issues

**Ruby worker not starting**:
- FFI bootstrap can take 30-60 seconds
- Check `ruby-worker` logs for compilation errors
- Verify Rust toolchain installed in CI

**Handler discovery failing**:
- Check `TASKER_TEMPLATE_PATH` environment variable
- Verify templates mounted in Docker volumes
- Check handler registry logs

**Flaky tests**:
- Increase health check timeout
- Add retry logic for transient failures
- Check for timing dependencies

---

## CI Performance Targets

| Stage | Target Duration | Test Count | Notes |
|-------|----------------|------------|-------|
| Build PostgreSQL | < 2 min | N/A | Cached image |
| Code Quality | < 4 min | N/A | Parallel with compile |
| Workspace Compile | < 8 min | N/A | Warms sccache |
| Unit Tests + Doctests | < 3 min | ~200 | Parallel with build-workers |
| Build Workers | < 8 min | N/A | FFI extensions |
| Worker Tests | < 3 min | ~150 | Ruby + Python + Rust |
| Integration Tests (x2) | < 7 min each | ~800 | Partitioned |
| **Total CI** | **< 15 min** | **~1150 tests** | **With warm cache** |

**Key Optimizations (TAS-88)**:
- **sccache**: Distributed compilation caching across jobs
- **DAG parallelism**: Maximum concurrent job execution
- **Nextest partitioning**: Integration tests split across 2 runners
- **Deferred FFI builds**: Workers build in parallel with unit tests
- **Native-only**: Removed Docker mode (~50% faster)
- **Artifact reuse**: Pre-built binaries shared across jobs
- **Cache warming**: workspace-compile primes cache for all downstream jobs

**Cache Strategy**:
- sccache: Compilation artifacts shared via GitHub Actions cache
- Cargo registry: Dependencies cached per Cargo.lock hash
- Bundler: Ruby gems cached per Gemfile.lock
- uv: Python packages cached per pyproject.toml

---

## Migration from Old Structure

**Previous (TAS-56)**: Dual-mode execution with optional Docker

**Now (TAS-88)**: DAG-based pipeline with maximum parallelism

**Changes**:
- ✅ **DAG architecture**: Multi-stage pipeline with parallel job execution
- ✅ **sccache enabled**: Distributed compilation caching via mozilla-actions/sccache-action
- ✅ **Nextest partitioning**: Integration tests split across 2 parallel runners
- ✅ **Deferred FFI builds**: Workers build in parallel with unit tests (not blocking)
- ✅ **Docker mode removed**: Native-only execution for simplicity
- ✅ **Python worker support**: Full Python FFI worker in CI pipeline
- ✅ **Artifact reuse**: Ruby framework tests reuse pre-built FFI extension
- ✅ **Cache optimization**: code-quality uses integration cache fallback

**Architecture Changes**:
```
Before (TAS-56):                    After (TAS-88):

build-postgres                      build-postgres
    │                                     │
    ├─→ code-quality                ┌─────┼─────┐
    └─→ build + unit + FFI          │     │     │
              │                     ▼     ▼     ▼
              ▼                   code  compile workers
         integration              quality   │     │
              │                            ├─────┤
              ▼                            ▼     ▼
         framework                     unit   build-workers
                                      tests        │
                                             ┌─────┴─────┐
                                             ▼           ▼
                                         worker    integration
                                         tests     (partitioned)
```

**Performance Impact**:
- **Cold cache**: ~20-25 minutes (first run)
- **Warm cache**: ~12-15 minutes (subsequent runs)
- **Parallelism gain**: ~40% faster through DAG optimization
- **Nextest partitioning**: ~30% faster integration tests
- **Native-only**: ~50% faster than Docker mode

---

## Fallback Plan: Split Testing with GHCR Caching

**Status**: Documented but not implemented (use if current approach times out)

If the current approach still exceeds 30 minutes or hits disk space issues, we have a more robust architecture ready:

### Architecture
1. **Per-Crate Test Jobs** - Run unit + integration tests per crate in parallel
2. **Docker Image Jobs** - Build and push images to GHCR on success
3. **E2E Job** - Pull cached images from GHCR for fast E2E testing

### Workflow Structure
```
build-postgres → (parallel jobs below)

├─→ test-tasker-shared (unit + integration)
│   └─→ On success: no image needed
│
├─→ test-tasker-orchestration (unit + integration)
│   └─→ On success: build + push orchestration:$SHA to GHCR
│
├─→ test-tasker-worker (unit + integration)
│   └─→ On success: build + push worker:$SHA to GHCR
│
├─→ test-pgmq-notify (unit + integration)
│   └─→ On success: no image needed
│
└─→ test-ruby-worker (Ruby specs)
    └─→ On success: build + push ruby-worker:$SHA to GHCR

All test jobs succeed →
    e2e-tests (pulls cached images from GHCR, runs E2E only)
```

### Benefits
- **Parallel Execution**: All test jobs run simultaneously (~5-10 min each)
- **Image Caching**: Docker images built once, reused in E2E
- **Fast E2E**: No builds needed, just pull cached images (~5 min)
- **Disk Space**: Each job has independent disk space
- **Total Time**: ~15-20 minutes (vs current 30 min timeout)

### Implementation Files
If needed, create:
- `.github/workflows/test-by-crate.yml` - Per-crate test jobs
- `.github/workflows/build-and-cache-images.yml` - Docker image building
- Update `ci.yml` to orchestrate new structure

**Note**: Only implement if current approach fails. Current approach is simpler and should work with disk cleanup + parallel builds.

---

## Related Documentation

- [TAS-42 Specification](../../docs/ticket-specs/TAS-42/TAS-42-ci-and-e2e.md)
- [E2E Testing Guide](../../docs/testing-e2e.md) (TODO)
- [Ruby Framework Testing](../../workers/ruby/docs/framework-testing.md) (TODO)

---

**Last Updated**: 2025-12-18 (TAS-88 DAG-based pipeline with sccache and nextest partitioning)

# GitHub Actions CI/CD Workflows

This directory contains the CI/CD pipeline for tasker-core, implementing TAS-42's unified testing architecture.

## Workflow Overview

### Main Pipeline

**File**: `ci.yml`

The main orchestrator that runs all CI checks in parallel where possible.

**Job Flow**:
```
build-postgres
    ├─→ code-quality (parallel)
    └─→ integration-tests (parallel)

integration-tests
    └─→ ruby-framework-tests (needs build-postgres)

integration-tests + ruby-framework-tests
    └─→ performance-analysis

code-quality + integration-tests + ruby-framework-tests + performance-analysis
    └─→ ci-success
```

**Key Improvements (TAS-56)**:
- PostgreSQL image built once and reused across all jobs
- Code quality and integration tests run in parallel for faster CI
- **Native binary execution by default** (~7 min vs ~15 min Docker)
- Optional Docker execution via "run-docker" PR label for production-like testing
- Single test suite covers unit + integration + E2E (all 482 tests)
- Ruby framework tests use PostgreSQL service for FFI compilation
- Dependency cleanup removed 4 unused crates (5-10% compilation speedup)
- Dual-mode architecture: fast by default, thorough when needed

---

## Test Workflows

### Integration Tests (`test-integration.yml`)

**Purpose**: Complete test suite with dual-mode execution (native or Docker)

**Execution Modes**:

#### Mode 1: Native Binary Execution (Default)
- **Trigger**: No "run-docker" label on PR
- **Duration**: ~7 minutes
- **How it works**:
  1. Uses pre-compiled binaries from build job
  2. Starts services natively against GitHub Actions postgres
  3. Generates test config with config-builder
  4. Runs all 482 tests against native services

**Requirements**:
- PostgreSQL service container
- Pre-compiled binaries (target/debug/*)
- Ruby FFI extension compiled

**Command**:
```bash
# Generate test configurations
mkdir -p config/tasker
cargo run --quiet --package tasker-client --bin tasker-cli -- config generate \
  --context orchestration --environment test \
  --output config/tasker/orchestration-test.toml
cargo run --quiet --package tasker-client --bin tasker-cli -- config generate \
  --context worker --environment test \
  --output config/tasker/worker-test.toml

# Start native services
.github/scripts/start-native-services.sh

# Run all tests
cargo nextest run --profile ci \
  --package tasker-shared --package tasker-orchestration \
  --package tasker-worker --package pgmq-notify \
  --package tasker-client --package tasker-core \
  --test '*' --no-fail-fast

# Stop services
.github/scripts/stop-native-services.sh
```

#### Mode 2: Docker Execution (Conditional)
- **Trigger**: "run-docker" label on PR (`gh pr edit --add-label "run-docker"`)
- **Duration**: ~15 minutes
- **How it works**:
  1. Builds Docker images for all services
  2. Starts services via Docker Compose
  3. Runs same 482 tests against Docker services

**Use When**:
- Changing Docker configuration
- Modifying deployment scripts
- Adding/removing system dependencies
- Final validation before merge
- Debugging production-like issues

**Test Count**: 482 tests (same tests, different execution environment)

**Key Features**:
- **Dual-mode architecture**: Fast native by default, Docker when needed
- **Label-based switching**: Simple PR label controls execution mode
- **Shared build step**: Both modes use same compiled binaries
- **Same test coverage**: Tests are environment-agnostic
- Health checks for all services before testing
- Handler discovery validation
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
mkdir -p config/tasker
cargo run --quiet --package tasker-client --bin tasker-cli -- config generate \
  --context orchestration --environment test \
  --output config/tasker/orchestration-test.toml
cargo run --quiet --package tasker-client --bin tasker-cli -- config generate \
  --context worker --environment test \
  --output config/tasker/worker-test.toml

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

| Workflow | Target Duration | Actual | Test Count |
|----------|----------------|--------|------------|
| Build PostgreSQL | < 5 minutes | ~3-5 min | N/A |
| Code Quality | < 3 minutes | ~2-3 min | N/A |
| Integration Tests (Native) | < 10 minutes | ~7 min | 482 tests |
| Integration Tests (Docker) | < 20 minutes | ~15 min | 482 tests |
| Ruby Framework | < 3 minutes | ~2-3 min | 77 tests |
| **Total CI (Native)** | **< 15 minutes** | **~12-13 min** | **559 tests** |
| **Total CI (Docker)** | **< 25 minutes** | **~20-23 min** | **559 tests** |

**Key Optimizations (TAS-56)**:
- **Native binary execution by default** (~7 min vs ~15 min Docker)
- Optional Docker via "run-docker" label for production-like testing
- Dependency cleanup removed 4 unused crates (5-10% faster builds)
- PostgreSQL image built once and reused across all jobs
- Code quality and integration tests run in parallel
- Shared build step for both native and Docker modes
- config-builder generates test configuration dynamically
- Native services start in <30 seconds vs 3-5 minutes for Docker
- Ruby framework tests use lightweight PostgreSQL service

---

## Migration from Old Structure

**Previous (TAS-42)**: Unified `test-e2e.yml` with Docker-only execution

**Now (TAS-56)**: Dual-mode `test-integration.yml` with native execution by default

**Changes**:
- ✅ Native binary execution as default (~7 min vs ~15 min Docker)
- ✅ Conditional Docker execution via "run-docker" PR label
- ✅ Service startup scripts for native execution (.github/scripts/)
- ✅ Dynamic test configuration generation (config-builder)
- ✅ Removed 4 unused dependencies (factori, mockall, proptest, insta)
- ✅ Dependency analysis tools (analyze-unused-deps.sh, remove-unused-deps.sh)
- ✅ Shared build step for both native and Docker modes
- ✅ Updated sqlx query cache after dependency changes
- ✅ Renamed `test-e2e.yml` → `test-integration.yml` (more accurate)
- ✅ Updated all CI workflow references

**Performance Impact**:
- **Default (Native)**: ~12-13 minutes total CI time (53% faster)
- **With Docker**: ~20-23 minutes total CI time (when needed)
- 559 total tests (482 Rust + 77 Ruby)
- 5-10% faster builds from dependency cleanup
- 50% GitHub Actions cost reduction for most PRs
- Same test coverage, different execution environment

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

**Last Updated**: 2025-10-25 (TAS-56 native binary execution + dependency cleanup)

# GitHub Actions CI/CD Workflows

This directory contains the CI/CD pipeline for tasker-core, implementing TAS-42's unified testing architecture.

## Workflow Overview

### Main Pipeline

**File**: `ci.yml`

The main orchestrator that runs all CI checks in parallel where possible.

**Job Flow**:
```
docker-build
    ├─→ code-quality (parallel)
    └─→ comprehensive-tests (parallel)

comprehensive-tests
    ├─→ doctests
    └─→ ruby-framework-tests

comprehensive-tests + ruby-framework-tests
    └─→ performance-analysis

code-quality + comprehensive-tests + doctests + ruby-framework-tests + performance-analysis
    └─→ ci-success
```

**Key Improvements**:
- Code quality and comprehensive tests run in parallel for faster CI
- Single comprehensive test suite covers unit + integration + E2E (all 482 tests)
- Ruby framework tests run after comprehensive tests (workers already shut down)
- Doctests run separately with sqlx offline mode

---

## Test Workflows

### Comprehensive Tests (`test-e2e.yml`)

**Purpose**: Complete test suite covering unit + integration + E2E testing

**Runs**:
- Unit tests (all packages)
- Integration tests (`tests/integration/`)
- Rust worker E2E tests (`tests/e2e/rust/`)
- Ruby worker E2E tests (`tests/e2e/ruby/`)

**Requirements**:
- Docker Compose with all services (postgres, orchestration, rust-worker, ruby-worker)
- Extended timeout for Ruby worker FFI bootstrap (30-60s)

**Command**:
```bash
cargo nextest run \
  --profile ci \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client \
  --package tasker-core \
  --no-fail-fast
```

**Test Count**: 482 tests

**Duration**: ~10-15 minutes

**Key Features**:
- Single Docker startup for all tests
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

**Purpose**: Ruby-specific framework testing (no Docker needed)

**Runs**:
- FFI layer tests (`workers/ruby/spec/ffi/`)
- Type wrapper tests (`workers/ruby/spec/types/`)
- Worker core tests (`workers/ruby/spec/worker/`)

**Requirements**:
- Ruby 3.4
- Rust toolchain (for FFI extension compilation)
- Bundle install + rake compile

**Command**: `bundle exec rspec spec/ --exclude-pattern spec/integration/**/*_spec.rb`

**Test Count**: 77 tests

**Duration**: ~1-2 minutes

**Key Features**:
- Fast feedback (no Docker overhead)
- Validates Ruby FFI bindings
- Tests Ruby-specific framework logic
- Runs after comprehensive tests (workers already shut down)

---

## Support Workflows

### Build Docker Images (`build-docker-images.yml`)

**Purpose**: Build and cache Docker images for CI

**Outputs**:
- `postgres-image`: PostgreSQL with PGMQ extension
- Other service images as needed

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

### Comprehensive Tests

```bash
# Start Docker services
docker compose -f docker/docker-compose.test.yml up --build -d

# Wait for services
curl http://localhost:8080/health  # Orchestration
curl http://localhost:8081/health  # Rust worker
curl http://localhost:8082/health  # Ruby worker (may take 60s)

# Run comprehensive test suite (all 482 tests)
cargo nextest run \
  --package tasker-shared \
  --package tasker-orchestration \
  --package tasker-worker \
  --package pgmq-notify \
  --package tasker-client \
  --package tasker-core \
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
# Ensure Docker services are stopped first
docker compose -f docker/docker-compose.test.yml down

cd workers/ruby
bundle install
bundle exec rake compile
bundle exec rspec spec/ --exclude-pattern spec/integration/**/*_spec.rb
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
| Docker Build | < 5 minutes | ~3-5 min | N/A |
| Code Quality | < 3 minutes | ~2-3 min | N/A |
| Comprehensive Tests | < 15 minutes | ~10-15 min | 482 tests |
| Doctests | < 2 minutes | ~1 min | 3 examples |
| Ruby Framework | < 2 minutes | ~1-2 min | 77 tests |
| **Total CI** | **< 20 minutes** | **~15-18 min** | **562 tests** |

**Key Optimizations**:
- Code quality and comprehensive tests run in parallel (not sequential)
- Single Docker startup for all integration and E2E tests
- Doctests use sqlx offline mode (no database needed)
- Ruby framework tests leverage already-shut-down workers

---

## Migration from Old Structure

**Previous**: Separate `test-unit.yml`, `test-integration.yml`, and `test-ruby-integration.yml`

**Now**: Unified `test-e2e.yml` (comprehensive) + `doctests` + `test-ruby-framework.yml`

**Changes**:
- ✅ Consolidated all Rust tests into single comprehensive suite (482 tests)
- ✅ Added separate doctest job with sqlx offline mode
- ✅ Code quality and comprehensive tests run in parallel for faster CI
- ✅ Ruby framework tests run after comprehensive tests (workers already shut down)
- ✅ Removed duplicate test infrastructure
- ✅ Removed `workers/ruby/spec/integration/` (migrated to `tests/e2e/ruby/`)
- ✅ Removed redundant `test-unit.yml` and `test-ruby-unit.yml` workflows

**Performance Impact**:
- Total CI time reduced from ~20 minutes to ~15-18 minutes
- 562 total tests (482 Rust + 77 Ruby + 3 doctests)
- Single Docker startup for all integration and E2E tests

---

## Related Documentation

- [TAS-42 Specification](../../docs/ticket-specs/TAS-42/TAS-42-ci-and-e2e.md)
- [E2E Testing Guide](../../docs/testing-e2e.md) (TODO)
- [Ruby Framework Testing](../../workers/ruby/docs/framework-testing.md) (TODO)

---

**Last Updated**: 2025-10-06 (TAS-42 CI workflow restructure)

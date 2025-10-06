# GitHub Actions CI/CD Workflows

This directory contains the CI/CD pipeline for tasker-core, implementing TAS-42's unified testing architecture.

## Workflow Overview

### Main Pipeline

**File**: `ci.yml`

The main orchestrator that runs all CI checks in parallel where possible.

**Job Flow**:
```
docker-build
    ├─→ code-quality
    ├─→ unit-tests
    ├─→ ruby-unit-tests
    └─→ ruby-framework-tests

code-quality + unit-tests
    └─→ e2e-tests (all workers)

e2e-tests + ruby-framework-tests + ruby-unit-tests
    └─→ performance-analysis

performance-analysis
    └─→ ci-success
```

---

## Test Workflows

### E2E Tests (`test-e2e.yml`)

**Purpose**: Unified end-to-end testing for all workers

**Runs**:
- Rust integration tests (`tests/integration/`)
- Rust worker E2E tests (`tests/e2e/rust/`)
- Ruby worker E2E tests (`tests/e2e/ruby/`)

**Requirements**:
- Docker Compose with all services (postgres, orchestration, rust-worker, ruby-worker)
- Extended timeout for Ruby worker FFI bootstrap (30-60s)

**Command**: `cargo nextest run --test '*' --profile ci`

**Duration**: ~10-15 minutes

**Key Features**:
- Single Docker startup for all tests (~71 tests total)
- Health checks for all services before testing
- Handler discovery validation
- Comprehensive service logs on failure

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

**Duration**: ~1-2 minutes

**Key Features**:
- Fast feedback (no Docker overhead)
- Validates Ruby FFI bindings
- Tests Ruby-specific framework logic

---

### Unit Tests (`test-unit.yml`)

**Purpose**: Rust unit tests with PostgreSQL integration

**Runs**: `cargo nextest run --lib --bins`

**Duration**: ~3-5 minutes

---

### Ruby Unit Tests (`test-ruby-unit.yml`)

**Purpose**: Ruby unit tests (no Docker)

**Runs**: `bundle exec rspec spec/unit/` (if unit tests exist)

**Duration**: ~1 minute

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

### E2E Tests

```bash
# Start Docker services
docker compose -f docker/docker-compose.test.yml up --build -d

# Wait for services
curl http://localhost:8080/health  # Orchestration
curl http://localhost:8081/health  # Rust worker
curl http://localhost:8082/health  # Ruby worker (may take 60s)

# Run all E2E tests
cargo nextest run --test '*' --profile ci

# Cleanup
docker compose -f docker/docker-compose.test.yml down -v
```

### Ruby Framework Tests

```bash
cd workers/ruby
bundle install
bundle exec rake compile
bundle exec rspec spec/ --exclude-pattern spec/integration/**/*_spec.rb
```

### Unit Tests

```bash
# Rust unit tests
cargo nextest run --lib --bins

# Ruby unit tests (if they exist)
cd workers/ruby
bundle exec rspec spec/unit/
```

---

## Adding New Tests

### Adding E2E Test

1. Create test file in `tests/e2e/rust/` or `tests/e2e/ruby/`
2. Use `DockerIntegrationManager` for setup
3. Test runs automatically via `cargo nextest run --test '*'`
4. No CI changes needed

**Example**:
```rust
// tests/e2e/ruby/my_new_test.rs
use crate::common::integration_test_manager::IntegrationTestManager;

#[tokio::test]
#[ignore]  // Run only when Docker services available
async fn test_my_scenario() -> anyhow::Result<()> {
    let manager = IntegrationTestManager::setup().await?;
    // ... test logic ...
    Ok(())
}
```

### Adding Framework Test

1. Create test file in `workers/ruby/spec/{ffi,types,worker}/`
2. No Docker dependencies
3. Test runs automatically via `bundle exec rspec spec/`
4. No CI changes needed

---

## Debugging CI Failures

### E2E Test Failures

1. **Check service logs**: Download `e2e-service-logs` artifact
2. **Check test output**: Download `e2e-test-results` artifact (JUnit XML)
3. **Reproduce locally**:
   ```bash
   docker compose -f docker/docker-compose.test.yml up --build
   cargo nextest run --test '*' --nocapture
   ```

### Ruby Framework Test Failures

1. **Check test output**: Download `ruby-framework-test-results` artifact
2. **Reproduce locally**:
   ```bash
   cd workers/ruby
   bundle exec rake compile
   bundle exec rspec spec/ --exclude-pattern spec/integration/**/*_spec.rb
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

| Workflow | Target Duration | Actual |
|----------|----------------|--------|
| E2E Tests | < 15 minutes | ~10-15 min |
| Ruby Framework | < 2 minutes | ~1-2 min |
| Unit Tests | < 5 minutes | ~3-5 min |
| Code Quality | < 3 minutes | ~2-3 min |
| **Total CI** | **< 20 minutes** | **~15-20 min** |

---

## Migration from Old Structure

**Previous**: Separate `test-integration.yml` and `test-ruby-integration.yml`

**Now**: Unified `test-e2e.yml` + separate `test-ruby-framework.yml`

**Changes**:
- ✅ Consolidated E2E testing (single Docker startup)
- ✅ Removed duplicate test infrastructure
- ✅ Separated framework tests (fast, no Docker)
- ✅ Removed `workers/ruby/spec/integration/` (migrated to `tests/e2e/ruby/`)

---

## Related Documentation

- [TAS-42 Specification](../../docs/ticket-specs/TAS-42/TAS-42-ci-and-e2e.md)
- [E2E Testing Guide](../../docs/testing-e2e.md) (TODO)
- [Ruby Framework Testing](../../workers/ruby/docs/framework-testing.md) (TODO)

---

**Last Updated**: 2025-10-06 (TAS-42 Phase 6 completion)

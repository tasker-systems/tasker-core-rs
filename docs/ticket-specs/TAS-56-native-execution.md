# TAS-56: Native Binary Execution with Conditional Docker (Re-Draft)

## Executive Summary

**Current State**: CI uses Docker Compose for all integration tests, adding 3-5 minutes of overhead per run.

**Proposed State**: Native binary execution for 95% of PRs, with optional Docker execution via PR labels for production-like validation.

**Key Benefits**:
- 40-60% faster CI runs (5-8 minutes vs 10-15 minutes)
- Lower GitHub Actions costs
- Faster developer feedback
- Production-like testing when needed via labels

## Problem Statement

### Current Docker-Based Approach

**What We Do Now**:
```yaml
- name: Build and start Docker services
  run: docker compose -f docker/docker-compose.test.yml up --build -d

- name: Wait for services
  run: timeout 180 bash -c 'until curl -f http://localhost:8080/health...'

- name: Run integration tests
  run: cargo nextest run --test '*'
```

**Overhead**:
1. **Docker Image Builds**: 2-3 minutes
   - PostgreSQL image pull/build
   - Orchestration service build (despite cached Rust compilation)
   - Worker service build
   - Ruby worker FFI bootstrap
2. **Service Startup**: 30-60 seconds
   - Database initialization
   - PGMQ extension setup
   - Health check polling
3. **Disk Usage**: 2-4GB per run
   - Multiple Docker layers
   - Service logs and volumes

**Why We Use Docker**:
- Provides production-like environment
- Isolates services
- Matches deployment configuration
- **But**: Most PRs don't change deployment characteristics

### The Native Execution Opportunity

**Key Insight**: After `cargo build --all-features`, we have working binaries:
- `target/debug/tasker-server` (orchestration)
- `target/debug/rust-worker` (worker)
- `workers/ruby/bin/server.rb` (Ruby FFI worker)

**These binaries can run directly** against the GitHub Actions postgres service without Docker.

## Proposed Solution: Native-First with Conditional Docker

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ Pull Request Created/Updated                                     │
└─────────────────────────────────────────────────────────────────┘
                           ▼
                  ┌────────────────┐
                  │ Check PR Labels│
                  └────────────────┘
                           ▼
          ┌────────────────┴────────────────┐
          ▼                                  ▼
┌─────────────────────┐         ┌──────────────────────┐
│ No "run-docker"     │         │ Label: "run-docker"  │
│ Use Native Binaries │         │ Use Docker Compose   │
└─────────────────────┘         └──────────────────────┘
          │                                  │
          ▼                                  ▼
┌─────────────────────┐         ┌──────────────────────┐
│ 1. Build binaries   │         │ 1. Build binaries    │
│ 2. Start postgres   │         │ 2. Docker build      │
│ 3. Run migrations   │         │ 3. Docker up -d      │
│ 4. Start services   │         │ 4. Health checks     │
│ 5. Run tests        │         │ 5. Run tests         │
└─────────────────────┘         └──────────────────────┘
   5-8 minutes                      10-15 minutes
```

### Implementation: Native Binary Execution

**Step 1: Service Startup Scripts**

Create `.github/scripts/start-native-services.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Configuration
POSTGRES_URL="${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_rust_test}"
CONFIG_PATH="${TASKER_CONFIG_PATH:-$(pwd)/config/tasker/complete-test.toml}"
ORCHESTRATION_PORT="${ORCHESTRATION_PORT:-8080}"
WORKER_PORT="${WORKER_PORT:-8081}"
RUBY_WORKER_PORT="${RUBY_WORKER_PORT:-8082}"

echo "🚀 Starting native services..."

# 1. Run database migrations
echo "📊 Running database migrations..."
DATABASE_URL="$POSTGRES_URL" cargo sqlx migrate run

# 2. Start orchestration service in background
echo "🎯 Starting orchestration service on port $ORCHESTRATION_PORT..."
TASKER_CONFIG_PATH="$CONFIG_PATH" \
  DATABASE_URL="$POSTGRES_URL" \
  RUST_LOG=info \
  target/debug/tasker-server \
  --port "$ORCHESTRATION_PORT" \
  > orchestration.log 2>&1 &
ORCHESTRATION_PID=$!
echo "Orchestration PID: $ORCHESTRATION_PID"

# 3. Start Rust worker in background
echo "⚙️  Starting Rust worker on port $WORKER_PORT..."
TASKER_CONFIG_PATH="$CONFIG_PATH" \
  DATABASE_URL="$POSTGRES_URL" \
  RUST_LOG=info \
  target/debug/rust-worker \
  --port "$WORKER_PORT" \
  > worker.log 2>&1 &
WORKER_PID=$!
echo "Worker PID: $WORKER_PID"

# 4. Start Ruby FFI worker in background
echo "💎 Starting Ruby FFI worker on port $RUBY_WORKER_PORT..."
cd workers/ruby
TASKER_CONFIG_PATH="$CONFIG_PATH" \
  DATABASE_URL="$POSTGRES_URL" \
  TASKER_ENV=test \
  bundle exec bin/server.rb \
  --port "$RUBY_WORKER_PORT" \
  > ../../ruby-worker.log 2>&1 &
RUBY_WORKER_PID=$!
echo "Ruby worker PID: $RUBY_WORKER_PID"
cd ../..

# 5. Wait for health checks
echo "🏥 Waiting for services to be healthy..."
timeout 60 bash -c "
  until curl -sf http://localhost:$ORCHESTRATION_PORT/health > /dev/null; do
    echo '⏳ Waiting for orchestration...'
    sleep 2
  done
  echo '✅ Orchestration ready'

  until curl -sf http://localhost:$WORKER_PORT/health > /dev/null; do
    echo '⏳ Waiting for Rust worker...'
    sleep 2
  done
  echo '✅ Rust worker ready'

  until curl -sf http://localhost:$RUBY_WORKER_PORT/health > /dev/null; do
    echo '⏳ Waiting for Ruby worker...'
    sleep 2
  done
  echo '✅ Ruby worker ready'
"

# 6. Save PIDs for cleanup
echo "$ORCHESTRATION_PID" > .pids/orchestration.pid
echo "$WORKER_PID" > .pids/worker.pid
echo "$RUBY_WORKER_PID" > .pids/ruby-worker.pid

echo "✅ All services started and healthy!"
echo "   - Orchestration: http://localhost:$ORCHESTRATION_PORT (PID $ORCHESTRATION_PID)"
echo "   - Rust Worker:   http://localhost:$WORKER_PORT (PID $WORKER_PID)"
echo "   - Ruby Worker:   http://localhost:$RUBY_WORKER_PORT (PID $RUBY_WORKER_PID)"
```

Create `.github/scripts/stop-native-services.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "🛑 Stopping native services..."

if [ -f .pids/orchestration.pid ]; then
  kill $(cat .pids/orchestration.pid) 2>/dev/null || true
  echo "Stopped orchestration service"
fi

if [ -f .pids/worker.pid ]; then
  kill $(cat .pids/worker.pid) 2>/dev/null || true
  echo "Stopped Rust worker"
fi

if [ -f .pids/ruby-worker.pid ]; then
  kill $(cat .pids/ruby-worker.pid) 2>/dev/null || true
  echo "Stopped Ruby worker"
fi

rm -rf .pids
echo "✅ All services stopped"
```

**Step 2: Enhanced Workflow with Conditional Execution**

Create `.github/workflows/test-integration.yml`:

```yaml
name: Integration Tests

on:
  workflow_call:
    inputs:
      postgres-image:
        required: true
        type: string
      use-docker:
        description: 'Force Docker execution (overrides PR label check)'
        required: false
        type: boolean
        default: false

jobs:
  # Job 1: Build once, shared by both execution modes
  build-and-unit-tests:
    name: Build + Unit Tests
    runs-on: ubuntu-22.04
    timeout-minutes: 15

    services:
      postgres:
        image: ${{ inputs.postgres-image }}
        ports:
          - "5432:5432"
        env:
          POSTGRES_USER: tasker
          POSTGRES_PASSWORD: tasker
          POSTGRES_DB: tasker_rust_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Rust build artifacts
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-e2e-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-e2e-
            ${{ runner.os }}-cargo-

      - name: Install tools
        uses: ./.github/actions/install-tools
        with:
          tools: "nextest sqlx-cli"

      - name: Setup database
        run: sqlx migrate run
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_rust_test

      - name: Build all packages
        run: cargo build --all-features --all-targets
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_rust_test

      - name: Build Ruby FFI extension
        run: |
          cd workers/ruby
          bundle install
          bundle exec rake compile

      - name: Run unit tests
        run: |
          mkdir -p target/nextest/ci
          cargo nextest run \
            --profile ci \
            --lib \
            --package tasker-shared \
            --package tasker-orchestration \
            --package tasker-worker \
            --package pgmq-notify \
            --package tasker-client \
            --no-fail-fast
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_rust_test

      - name: Run doctests
        run: |
          cargo test --doc \
            --package tasker-shared \
            --package tasker-orchestration \
            --package tasker-worker \
            --package pgmq-notify \
            --package tasker-client
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_rust_test

  # Job 2a: Native binary execution (default)
  integration-tests-native:
    name: Integration Tests (Native Binaries)
    runs-on: ubuntu-22.04
    timeout-minutes: 20
    needs: build-and-unit-tests
    # Run if: no "run-docker" label AND not forced to use Docker
    if: |
      !contains(github.event.pull_request.labels.*.name, 'run-docker') &&
      !inputs.use-docker

    services:
      postgres:
        image: ${{ inputs.postgres-image }}
        ports:
          - "5432:5432"
        env:
          POSTGRES_USER: tasker
          POSTGRES_PASSWORD: tasker
          POSTGRES_DB: tasker_rust_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Restore Rust build cache
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-e2e-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-e2e-
            ${{ runner.os }}-cargo-

      - name: Setup Ruby for FFI worker
        uses: ruby/setup-ruby@v1
        with:
          ruby-version: '3.2'
          bundler-cache: true
          working-directory: workers/ruby

      - name: Install tools
        uses: ./.github/actions/install-tools
        with:
          tools: "nextest"

      - name: Generate test configuration
        run: |
          mkdir -p config/tasker
          cargo run --bin config-builder -- \
            --context common \
            --context orchestration \
            --context worker \
            --environment test \
            --output config/tasker/complete-test.toml

      - name: Start native services
        run: |
          mkdir -p .pids
          chmod +x .github/scripts/start-native-services.sh
          .github/scripts/start-native-services.sh
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_rust_test
          TASKER_CONFIG_PATH: ${{ github.workspace }}/config/tasker/complete-test.toml

      - name: Run integration tests
        run: |
          mkdir -p target/nextest/ci
          cargo nextest run \
            --profile ci \
            --package tasker-shared \
            --package tasker-orchestration \
            --package tasker-worker \
            --package tasker-client \
            --package tasker-core \
            --test '*' \
            --no-fail-fast
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_rust_test
          TASKER_TEST_ORCHESTRATION_URL: http://localhost:8080
          TASKER_TEST_WORKER_URL: http://localhost:8081
          TASKER_TEST_RUBY_WORKER_URL: http://localhost:8082
          RUST_LOG: info

      - name: Collect service logs on failure
        if: failure()
        run: |
          mkdir -p native-logs
          [ -f orchestration.log ] && cp orchestration.log native-logs/
          [ -f worker.log ] && cp worker.log native-logs/
          [ -f ruby-worker.log ] && cp ruby-worker.log native-logs/

      - name: Upload service logs
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: native-service-logs
          path: native-logs/

      - name: Stop native services
        if: always()
        run: |
          chmod +x .github/scripts/stop-native-services.sh
          .github/scripts/stop-native-services.sh

  # Job 2b: Docker execution (conditional)
  integration-tests-docker:
    name: Integration Tests (Docker - Production-Like)
    runs-on: ubuntu-22.04
    timeout-minutes: 30
    needs: build-and-unit-tests
    # Run if: "run-docker" label OR forced to use Docker
    if: |
      contains(github.event.pull_request.labels.*.name, 'run-docker') ||
      inputs.use-docker

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Restore Rust build cache
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-e2e-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-e2e-
            ${{ runner.os }}-cargo-

      - name: Install tools
        uses: ./.github/actions/install-tools
        with:
          tools: "nextest"

      - name: Free up disk space
        run: |
          docker system prune -af --volumes
          sudo rm -rf /usr/share/dotnet
          sudo rm -rf /opt/ghc
          sudo rm -rf /usr/local/share/boost

      - name: Build and start Docker services
        run: docker compose -f docker/docker-compose.test.yml up --build -d

      - name: Wait for services
        run: |
          timeout 180 bash -c '
            until curl -f http://localhost:8080/health 2>/dev/null; do
              echo "⏳ Waiting for orchestration..."
              sleep 2
            done

            until curl -f http://localhost:8081/health 2>/dev/null; do
              echo "⏳ Waiting for Rust worker..."
              sleep 2
            done

            until curl -f http://localhost:8082/health 2>/dev/null; do
              echo "⏳ Waiting for Ruby worker..."
              sleep 5
            done

            echo "✅ All services ready"
          '

      - name: Run integration tests
        run: |
          mkdir -p target/nextest/ci
          cargo nextest run \
            --profile ci \
            --package tasker-shared \
            --package tasker-orchestration \
            --package tasker-worker \
            --package tasker-client \
            --package tasker-core \
            --test '*' \
            --no-fail-fast
        env:
          DATABASE_URL: postgresql://tasker:tasker@localhost:5432/tasker_rust_test
          TASKER_TEST_ORCHESTRATION_URL: http://localhost:8080
          TASKER_TEST_WORKER_URL: http://localhost:8081
          TASKER_TEST_RUBY_WORKER_URL: http://localhost:8082
          RUST_LOG: info

      - name: Collect service logs on failure
        if: failure()
        run: |
          mkdir -p docker-logs
          docker compose -f docker/docker-compose.test.yml logs postgres > docker-logs/postgres.log
          docker compose -f docker/docker-compose.test.yml logs orchestration > docker-logs/orchestration.log
          docker compose -f docker/docker-compose.test.yml logs worker > docker-logs/rust-worker.log
          docker compose -f docker/docker-compose.test.yml logs ruby-worker > docker-logs/ruby-worker.log

      - name: Upload service logs
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: docker-service-logs
          path: docker-logs/

      - name: Shutdown services
        if: always()
        run: docker compose -f docker/docker-compose.test.yml down -v
```

**Step 3: Update Main CI Pipeline**

Update `.github/workflows/ci.yml`:

```yaml
jobs:
  build-postgres:
    uses: ./.github/workflows/build-postgres.yml

  code-quality:
    needs: build-postgres
    uses: ./.github/workflows/code-quality.yml
    with:
      postgres-image: ${{ needs.build-postgres.outputs.postgres-image }}

  # Use new integration test workflow
  integration-tests:
    needs: build-postgres
    uses: ./.github/workflows/test-integration.yml
    with:
      postgres-image: ${{ needs.build-postgres.outputs.postgres-image }}
      # Default: native execution unless PR has "run-docker" label

  ruby-framework-tests:
    needs: [build-postgres, integration-tests]
    uses: ./.github/workflows/test-ruby-framework.yml
    with:
      postgres-image: ${{ needs.build-postgres.outputs.postgres-image }}
```

### Usage: Adding "run-docker" Label to PRs

**Via GitHub UI**:
1. Open PR page
2. Click "Labels" in right sidebar
3. Add "run-docker" label
4. Workflow automatically uses Docker on next push

**Via gh CLI**:
```bash
# Add label to current PR
gh pr edit --add-label "run-docker"

# Remove label to return to native execution
gh pr edit --remove-label "run-docker"

# Create PR with label from start
gh pr create --label "run-docker" --title "..." --body "..."
```

**Recommended Usage**:
- **Default (no label)**: Fast native execution for most development
- **"run-docker" label**: For PRs that:
  - Change Docker configuration
  - Modify deployment scripts
  - Update environment variables
  - Add/remove dependencies requiring Docker layers
  - Need final validation before merge

## Benefits Analysis

### Time Savings Breakdown

**Current Docker Approach (15 minutes)**:
- Build: 8 minutes (cached Rust + Docker builds)
- Docker startup: 3 minutes
- Tests: 4 minutes

**Native Approach (7 minutes)**:
- Build: 8 minutes (cached Rust, same as before)
- Native startup: 30 seconds
- Tests: 4 minutes

**Savings**: ~8 minutes per PR run (53% reduction)

### Cost Impact

**Assumptions**:
- 20 PRs per week
- 3 CI runs per PR (initial + fixes)
- Current: 60 runs × 15 min = 900 min/week
- Native: 57 runs × 7 min + 3 runs × 15 min = 444 min/week

**Reduction**: 456 min/week = 50% cost savings

### Developer Experience

**Faster Feedback**:
- Unit tests: 3 minutes (unchanged)
- Integration tests: 7 minutes vs 15 minutes
- Total: 10 minutes vs 18 minutes

**Clearer Intent**:
- Native: "This is a normal code change"
- Docker label: "This changes deployment characteristics"

## Migration Strategy

### Phase 1: Parallel Execution (1 week)

**Goal**: Run both native and Docker in parallel to validate equivalence.

```yaml
# Temporary: run both modes
integration-tests-native:
  if: always()  # Always run

integration-tests-docker:
  if: always()  # Always run
```

**Validation**:
- Compare test results
- Verify same tests pass/fail
- Monitor for environment-specific issues

### Phase 2: Native Default (1 week)

**Goal**: Make native default, Docker optional.

```yaml
integration-tests-native:
  if: |
    !contains(github.event.pull_request.labels.*.name, 'run-docker')

integration-tests-docker:
  if: |
    contains(github.event.pull_request.labels.*.name, 'run-docker')
```

**Monitor**:
- CI run times
- Failure rates
- Developer adoption

### Phase 3: Optimization (ongoing)

**Potential Enhancements**:
- Pre-compile worker binaries with `--release` in build job
- Parallel service startup
- Smarter health check polling
- Caching Ruby bundle between jobs

## Configuration Requirements

### Test Configuration File

Create `config/tasker/complete-test.toml` via CLI:

```bash
cargo run --bin config-builder -- \
  --context common \
  --context orchestration \
  --context worker \
  --environment test \
  --output config/tasker/complete-test.toml
```

**Key Settings**:
```toml
[database]
url = "postgresql://tasker:tasker@localhost:5432/tasker_rust_test"

[orchestration.api]
host = "0.0.0.0"
port = 8080

[worker.api]
host = "0.0.0.0"
port = 8081

[mpsc_channels.orchestration.command_processor]
command_buffer_size = 100  # Small for test environment
```

### Environment Variables

**Build Job**:
```bash
DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test
```

**Native Integration Job**:
```bash
DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test
TASKER_CONFIG_PATH=/path/to/config/tasker/complete-test.toml
TASKER_TEST_ORCHESTRATION_URL=http://localhost:8080
TASKER_TEST_WORKER_URL=http://localhost:8081
TASKER_TEST_RUBY_WORKER_URL=http://localhost:8082
RUST_LOG=info
```

## Monitoring and Metrics

### Key Metrics to Track

**Performance**:
- Native run time vs Docker run time
- Cache hit rates (should remain unchanged)
- Test execution time (should be identical)

**Reliability**:
- Native test pass rate vs Docker pass rate
- Service startup failure rate
- Flaky test detection

**Usage**:
- % of PRs using native vs Docker
- "run-docker" label adoption
- Developer feedback

### Success Criteria

**Performance** (2 weeks):
- [ ] Native runs complete in <8 minutes (90th percentile)
- [ ] 50%+ reduction in average CI time
- [ ] No increase in test flakiness

**Reliability** (4 weeks):
- [ ] Native pass rate ≥ Docker pass rate
- [ ] Zero service startup failures
- [ ] <5% of PRs forced to switch modes

**Adoption** (4 weeks):
- [ ] >80% of PRs use native execution
- [ ] "run-docker" label used for deployment PRs
- [ ] Positive developer feedback on speed

## Rollback Plan

### If Native Execution Has Issues

**Immediate Rollback**:
```yaml
# In .github/workflows/test-integration.yml
integration-tests-native:
  if: false  # Disable native

integration-tests-docker:
  if: always()  # Force Docker for all
```

**Partial Rollback**:
```yaml
# Require opt-in for native instead of opt-out for Docker
integration-tests-native:
  if: contains(github.event.pull_request.labels.*.name, 'use-native')

integration-tests-docker:
  if: |
    !contains(github.event.pull_request.labels.*.name, 'use-native')
```

## Alternative Approaches Considered

### Option A: Testcontainers Only

**What**: Use Testcontainers Rust library for service orchestration.

**Rejected Because**:
- Still requires Docker runtime
- Complex configuration
- Previous attempt (TAS-XX) ran into significant complexity
- Doesn't reduce Docker overhead meaningfully

### Option B: Pre-built Docker Images

**What**: Build Docker images separately, pull in CI.

**Rejected Because**:
- Adds infrastructure complexity
- Still has Docker startup overhead
- Best for mature projects with stable images
- Doesn't address core issue (Docker overhead)

### Option C: Split Integration Tests

**What**: Separate "fast integration" (no Docker) from "full E2E" (Docker).

**Considered For**: Future optimization after native execution proven.

**Benefits**:
- Even faster feedback for fast tests
- Full E2E only when needed

**Defer To**: Post-native-execution optimization

## Documentation Updates Required

### README.md

Add section on CI execution modes:

```markdown
## CI Integration Tests

By default, integration tests run using native binaries for fast feedback.

### Native Execution (Default)
- Runs compiled binaries directly against GitHub Actions postgres service
- ~7 minute run time
- Sufficient for most code changes

### Docker Execution (Optional)
Add the `run-docker` label to your PR for production-like testing:

\`\`\`bash
gh pr edit --add-label "run-docker"
\`\`\`

Use Docker execution when:
- Changing Docker configuration
- Modifying deployment scripts
- Updating system dependencies
- Final validation before merge
```

### CLAUDE.md

Update CI commands section:

```markdown
### CI Testing Locally

# Run tests the way CI does (native)
.github/scripts/start-native-services.sh
cargo nextest run --test '*'
.github/scripts/stop-native-services.sh

# Run tests with Docker (production-like)
docker compose -f docker/docker-compose.test.yml up -d
cargo nextest run --test '*'
docker compose -f docker/docker-compose.test.yml down
```

## Dependency Cleanup for Build Performance

### Analysis Results

Analysis of 62 workspace dependencies identified **16 potentially unused crates (25%)**:

**Verified Unused** (not in `cargo tree`):
- `factori` - Test factory library (never used)
- `mockall` - Mocking library (never used)
- `proptest` - Property-based testing (never used)
- `insta` - Snapshot testing (never used)

**Likely Unused** (no `use` statements found):
- `cargo-llvm-cov` - Coverage tool (should be root dev-dep only)
- `rust_decimal` - Decimal library (we use `bigdecimal`)
- `tokio-test` - Test utilities (verify usage)
- `sysinfo` - System info (likely unused)

**Needs Investigation** (may be used indirectly):
- `axum-extra`, `crossbeam`, `dirs`, `is-terminal`
- `opentelemetry-semantic-conventions`, `toml`, `url`
- `tasker-core` (special case - integration tests)

### Impact Estimate

**Conservative** (remove 4 verified):
- Reduce compilation time: 5-10%
- Fewer transitive dependencies
- Smaller binaries

**Aggressive** (remove 10-16 after verification):
- Reduce compilation time: 15-25%
- Significantly fewer transitive dependencies
- Measurably smaller binaries

### Removal Strategy

**Phase 1 - Safe Removal** (Part of TAS-56):
```bash
./scripts/remove-unused-deps.sh --verified-only
```

Removes: `factori`, `mockall`, `proptest`, `insta`

**Phase 2 - Verified Removal** (After testing):
```bash
./scripts/remove-unused-deps.sh --with-verification
```

For each candidate:
1. Check `cargo tree -i <crate>`
2. Remove from Cargo.toml
3. Run `cargo test --all-features`
4. Commit if tests pass

**Phase 3 - Careful Review** (Manual):
- Investigate indirect usage
- Check for derive macro dependencies
- Verify trait implementations

### Tools

**Analysis Script**:
```bash
./scripts/analyze-unused-deps.sh
```

**Removal Script** (created as part of TAS-56):
```bash
./scripts/remove-unused-deps.sh [--verified-only|--with-verification]
```

### Documentation

Full analysis: `docs/analysis/unused-dependencies-analysis.md`

## Implementation Checklist

### Phase 0: Preparation (Current PR)
- [x] Phase 1: GitHub Actions caching
- [x] Phase 2: Job splitting
- [ ] Merge current CI improvements (PR #44)

### Phase 1: Native Execution Infrastructure (Week 1)
- [ ] Create service startup/shutdown scripts
- [ ] Create test configuration template
- [ ] Update workflow with both execution modes
- [ ] Run parallel validation (native + Docker both enabled)
- [ ] Monitor for equivalence

### Phase 2: Label-Based Switching (Week 2)
- [ ] Add "run-docker" label to repository
- [ ] Update workflow with conditional logic
- [ ] Document usage in README
- [ ] Test label toggling on test PR

### Phase 3: Default to Native (Week 3)
- [ ] Switch default to native execution
- [ ] Monitor metrics (time, reliability, adoption)
- [ ] Gather developer feedback
- [ ] Document any issues and fixes

### Phase 4: Optimization (Week 4+)
- [ ] **Remove unused dependencies** (4-16 crates identified via analysis)
- [ ] Pre-compile worker binaries if beneficial
- [ ] Optimize service startup scripts
- [ ] Add metrics dashboard
- [ ] Consider "fast integration" split

## Metadata

- **Ticket**: TAS-56
- **Type**: Infrastructure Improvement
- **Priority**: High (significant cost & time savings)
- **Estimated Effort**: 2-3 weeks (including validation)
- **Risk**: Low (easy rollback, Docker remains available)
- **Dependencies**:
  - PR #44 (current CI improvements)
  - config-builder CLI tool (TAS-50)

## Related Work

- **TAS-50**: Configuration consolidation (enables TASKER_CONFIG_PATH approach)
- **TAS-34**: Component-based configuration (simplifies test config)
- **Current PR #44**: Phases 1 & 2 (caching + job splitting)

## Questions & Discussion

### Q: What about services that truly need Docker?

**A**: Keep Docker optional via label. Most services (orchestration, workers) are just binaries + database. Docker adds value for:
- System-level dependency testing
- Multi-container networking
- Volume/filesystem isolation
- Production environment validation

Use native for speed, Docker for production-like validation.

### Q: What if native and Docker tests diverge?

**A**: Monitor in Phase 1 parallel execution. If divergence detected:
1. Identify environment-specific assumptions
2. Fix tests to be environment-agnostic
3. Document required Docker scenarios
4. Keep Docker for those specific scenarios

### Q: Can we A/B test this?

**A**: Yes! Phase 1 runs both modes in parallel. We can:
- Compare timing metrics
- Verify identical test results
- Measure flakiness
- Validate before switching defaults

### Q: What about local development?

**A**: Developers can:
- Use native scripts (`.github/scripts/start-native-services.sh`)
- Use Docker Compose (current workflow)
- Choose based on preference
- Both work with same test suite

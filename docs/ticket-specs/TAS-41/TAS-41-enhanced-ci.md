# TAS-41: Enhanced CI Pipeline with Nextest and Docker Compose Integration

## Overview

This specification outlines the modernization of our CI/CD pipeline to leverage cargo-nextest for parallel test execution, Docker Compose for integration testing, and sccache for build caching. The new architecture will use modular GitHub Actions workflows for better maintainability and faster feedback cycles.

## Goals

1. **Faster Test Execution**: Use cargo-nextest for parallel test execution with better output formatting
2. **Real Integration Testing**: Leverage Docker Compose for true end-to-end testing  
3. **Build Caching**: Implement sccache to reduce compilation times across CI runs
4. **Modular Architecture**: Break CI into reusable workflow components
5. **Progressive Testing**: Run integration tests only after basic checks pass
6. **Remove Obsolete Components**: Clean up Ruby bindings tests and outdated benchmark approaches

## Current Issues

- CI references non-existent `Dockerfile.postgres.extensions` (should be `docker/db/Dockerfile`)
- Integration tests workflow references non-existent `docker-compose.ci.yml` 
- Ruby integration tests are outdated (removed from workspace in Cargo.toml)
- Current benchmarks exist but don't measure meaningful end-to-end metrics
- Tests run sequentially, missing parallelization opportunities
- No proper integration test execution in CI with Docker Compose
- Missing nextest configuration for CI profile with proper test result archiving

## Proposed Architecture

### Workflow Structure

```
.github/workflows/
├── ci.yml                          # Main orchestrator workflow
├── code-quality.yml                # Linting, formatting, security  
├── test-unit.yml                   # Unit tests with nextest
├── test-integration.yml            # Docker Compose integration tests
├── build-docker-images.yml         # Docker image building/caching
└── ci-success.yml                  # Final status aggregation
```

### Execution Flow

```mermaid
graph TD
    A[Push/PR Trigger] --> B[Build Docker Images]
    B --> C[Code Quality Checks]
    C --> D[Unit Tests + Doctests]
    D --> E{All Pass?}
    E -->|Yes| F[Integration Tests]
    E -->|No| G[Fail Fast]
    F --> H[CI Success]
    G --> I[CI Failed]
```

## Implementation Plan

### Phase 1: Tool Installation and Configuration

#### 1.1 Update nextest.toml Configuration

Location: `.config/nextest.toml`

```toml
[profile.default]
retries = 0
leak-timeout = { period = "500ms", result = "fail" }
fail-fast = false

[profile.ci]
# CI-specific settings
fail-fast = false
slow-timeout = { period = "60s", terminate-after = 2 }
test-threads = "num-cpus"
status-level = "pass"
final-status-level = "slow"

# Archive settings for test result uploads
[profile.ci.junit]
path = "target/nextest/ci/junit.xml"
report-format = "junit"

[profile.local]
# Local development settings
fail-fast = true
status-level = "retry"
```

#### 1.2 Environment Variables Configuration

Common environment variables across all workflows:

```yaml
env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1
  RUSTC_WRAPPER: sccache
  SCCACHE_GHA_ENABLED: true
  DATABASE_URL: postgres://tasker:tasker@localhost:5432/tasker_rust_test
  TASKER_ENV: test
  LOG_LEVEL: warn
  RUST_LOG: warn
  JWT_PRIVATE_KEY: "-----BEGIN RSA PRIVATE KEY-----MIIEowIBAAKCAQEAxAh25l4WV5ibwXaIAETRchkQhVP/OymplWglG+GHM1j2fFdwsBR/jGGfdebsmDSC26/CB4CIQOCOf7YCjZsAbxLioK5R2lAE3AoE9VQRKouwzqpUcylXUswbXdPd1AJMrjaVDOzo27KI8v4/+SiyfgJsBJCLu4owiClKqD7iLHL2sxWKlTcqUy+puCH8u0wfdNM1O0YqQAbD6RhnDPcHY58/f/UD+AHIrCConFDuo1HFrEU0TieJ9BMpx4QM+r7/niLzvx8OfuV3g4OZfWA86fCQ38ACdj/BXJhnCIy3w6GqJiaU+BO6ZBWrYqy1FM4O7O+xvVizQDmwRVDQrxk8PQIDAQABAoIBADuK3qKGPX8JyXVvzUtXm85uMohsxP9xXiMVaQAY9nnwMZ3+6SlycHNxS4ACTibE/GJB9ktVDQ23p1C/NfNQ+0bBk7h+ejo3R+KV4H+rszMbDu8W6WO5UN7DRCSxr5UvxZ680XUFmIzyo4o7E69YXy7LCYgxZ1/lT2xsLlMAFq6tC9/nRx10ZfP5cYBN0gbBNav7fzcEwGvvcfwsxpoVeLR7fjicNjSQ1m6Lovn48rCiTC2xA3OkJBvKbp4rmO9zczxHBYWZBjDI8/Wus/MF3WmzFdNMo07a2xytszjLDvwr/P/ZVhq82Nhcr8xKRlCaGOIPGBcyBKNnGTmr1mL2KTkCgYEA96gRUPBLV9clR3acm0ViNDpGX7L6CIg6tnTI99fmdt+e6+zlfnO46uRVXREpHR+DCV1CC2vjai10zZzjSRCHBdP0GWcf97G9/wsW32hCuN4xE9PMBf1Ttm/oAU80gN7tDsutcsdYRCdIrSWuHlppByTlRWWwg9lCoHq4KYXtIGcCgYEAyqMqnQUzYqUzpZiypS0iKjrEP0pCUEOQ69eOsqLq7LnBYN7ugtuEN6GmL52fxam1G2y0Yzxy406Sf1hONnZXUhfUwVXHBS9eF5jtackPnvSeVyEQX9AdHlmOHNmypuRbzaPMkp2LjJlgUf+mNDtG1zfmsennkEZG9+maHkoqR7sCgYBOqjr926YC+9rijUGHbI2aC1ypLz+OkD8eD5B6cUDKR5PCWtg2x2lYazjWPAo0Lvs/cTj2ScnNwyyT1x626aIJ7t5dZ01XL0Uriwkz43k2IZWzN5ZZ3LLHg1pNeCw0NxtTlMy+ZaWa2GOUZCCfkZZE56pP1dIwv0UTloeC4QCGRwKBgQCGaEAFssNotQdS2bv1D8DPnfc5u7nMn2Rq6qm+F44XwwZfiL9PkOdcNx6SCs1FQNHeBPaJtDjISP+m9B28xjYZP7FhI9JEwCx7Hnarai+wUbUNOeMwikwmK2S2AjgbtvClr/YrcdB0S++1tAq8Lm1Ip82fSPTNn6/HFO2jFbKBrQKBgFDfovReI7rcF1iw90u6inVE+hCXlAfCbD96r3kM/xMJ0t4Udztxj5q0VMAGThAmTYrnD4r7Lst+5YRvTY8DXcKbSYKgJ7R8JyR8ujr+x6r1kxZs+fgcrI0EfygqICvOwkUlBuCwmDdDHcDxp4RgXkob2u3fj6kfpcd/nsV6EjVk-----END RSA PRIVATE KEY-----"
  JWT_PUBLIC_KEY: "-----BEGIN PUBLIC KEY-----MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAxAh25l4WV5ibwXaIAETRchkQhVP/OymplWglG+GHM1j2fFdwsBR/jGGfdebsmDSC26/CB4CIQOCOf7YCjZsAbxLioK5R2lAE3AoE9VQRKouwzqpUcylXUswbXdPd1AJMrjaVDOzo27KI8v4/+SiyfgJsBJCLu4owiClKqD7iLHL2sxWKlTcqUy+puCH8u0wfdNM1O0YqQAbD6RhnDPcHY58/f/UD+AHIrCConFDuo1HFrEU0TieJ9BMpx4QM+r7/niLzvx8OfuV3g4OZfWA86fCQ38ACdj/BXJhnCIy3w6GqJiaU+BO6ZBWrYqy1FM4O7O+xvVizQDmwRVDQrxk8PQIDAQAB-----END PUBLIC KEY-----"
  API_KEY: "ff3083e54e01ad76ffc3f791359c1ed92b347bcd613af94f88f0bf4991eaa168"
```

### Phase 2: Modular Workflow Components

#### 2.1 Main CI Orchestrator (.github/workflows/ci.yml)

```yaml
name: CI Pipeline

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  docker-build:
    uses: ./.github/workflows/build-docker-images.yml
    
  code-quality:
    needs: docker-build
    uses: ./.github/workflows/code-quality.yml
    with:
      postgres-image: ${{ needs.docker-build.outputs.postgres-image }}
    
  unit-tests:
    needs: docker-build
    uses: ./.github/workflows/test-unit.yml
    with:
      postgres-image: ${{ needs.docker-build.outputs.postgres-image }}
    
  integration-tests:
    needs: [code-quality, unit-tests]
    uses: ./.github/workflows/test-integration.yml
    
  ci-success:
    needs: [integration-tests]
    uses: ./.github/workflows/ci-success.yml
```

#### 2.2 Build Docker Images Workflow (.github/workflows/build-docker-images.yml)

```yaml
name: Build Docker Images

on:
  workflow_call:
    outputs:
      postgres-image:
        description: "PostgreSQL with PGMQ image tag"
        value: ${{ jobs.build-postgres.outputs.image }}

env:
  POSTGRES_IMAGE: ghcr.io/${{ github.repository_owner }}/tasker-pgmq:${{ github.sha }}

jobs:
  build-postgres:
    runs-on: ubuntu-22.04
    outputs:
      image: ${{ steps.image.outputs.image }}
    permissions:
      contents: read
      packages: write
      
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      
      - name: Log in to GitHub Container Registry
        id: ghcr-login
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
        continue-on-error: true
        
      - name: Build and push PostgreSQL image
        id: docker-build
        if: steps.ghcr-login.outcome == 'success'
        uses: docker/build-push-action@v5
        with:
          context: .
          file: ./docker/db/Dockerfile
          push: true
          tags: |
            ${{ env.POSTGRES_IMAGE }}
            ghcr.io/${{ github.repository_owner }}/tasker-pgmq:latest
          cache-from: type=gha
          cache-to: type=gha,mode=max
          
      - name: Output image strategy
        id: image
        run: |
          if [[ "${{ steps.docker-build.outcome }}" == "success" ]]; then
            echo "image=${{ env.POSTGRES_IMAGE }}" >> $GITHUB_OUTPUT
          else
            echo "image=ghcr.io/pgmq/pg17-pgmq:v1.5.1" >> $GITHUB_OUTPUT
            echo "::warning::Using fallback PGMQ image without pg_uuidv7 extension"
          fi
```

#### 2.3 Code Quality Workflow (.github/workflows/code-quality.yml)

```yaml
name: Code Quality

on:
  workflow_call:
    inputs:
      postgres-image:
        required: true
        type: string

jobs:
  quality:
    runs-on: ubuntu-22.04
    
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
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: mozilla-actions/sccache-action@v0.0.4
      
      - name: Install tools
        run: |
          cargo binstall sqlx-cli --secure
          cargo binstall cargo-audit --secure
          
      - name: Setup database
        run: sqlx migrate run
        
      - name: Check formatting
        run: cargo fmt --all -- --check
        
      - name: Run Clippy lints
        run: cargo clippy --all-targets --all-features -- -D warnings
        
      - name: Security audit
        run: cargo audit
        
      - name: Check documentation
        run: cargo doc --no-deps --document-private-items
```

#### 2.4 Unit Tests Workflow (.github/workflows/test-unit.yml)

```yaml
name: Unit Tests

on:
  workflow_call:
    inputs:
      postgres-image:
        required: true
        type: string

jobs:
  test:
    runs-on: ubuntu-22.04
    strategy:
      fail-fast: false
      matrix:
        rust-version: ["stable", "beta"]
        
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
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust-version }}
      - uses: mozilla-actions/sccache-action@v0.0.4
      
      - name: Install tools
        run: |
          cargo binstall cargo-nextest --secure
          cargo binstall sqlx-cli --secure
          
      - name: Setup database
        run: |
          until pg_isready -h localhost -p 5432 -U tasker; do
            echo "Waiting for PostgreSQL..."
            sleep 2
          done
          sqlx migrate run
          
      - name: Run nextest for core packages
        run: |
          cargo nextest run \
            --profile ci \
            --package tasker-shared \
            --package tasker-orchestration \
            --package tasker-worker \
            --package pgmq-notify \
            --package tasker-client \
            --no-fail-fast
            
      - name: Run doctests
        run: cargo test --doc --all-packages
        
      - name: Upload test results
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: test-results-${{ matrix.rust-version }}
          path: target/nextest/ci/junit.xml
```

#### 2.5 Integration Tests Workflow (.github/workflows/test-integration.yml)

```yaml
name: Integration Tests

on:
  workflow_call:

jobs:
  integration:
    runs-on: ubuntu-22.04
    
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: mozilla-actions/sccache-action@v0.0.4
      
      - name: Install tools
        run: cargo binstall cargo-nextest --secure
        
      - name: Start test services
        uses: hoverkraft-tech/compose-action@v2.0.1
        with:
          compose-file: "./docker/docker-compose.test.yml"
          up-flags: "--build -d"
          
      - name: Wait for services
        run: |
          timeout 60 bash -c '
            until curl -f http://localhost:8080/health 2>/dev/null; do
              echo "Waiting for orchestration service..."
              sleep 2
            done
            echo "Orchestration service is healthy"
            
            until curl -f http://localhost:8081/health 2>/dev/null; do
              echo "Waiting for worker service..."
              sleep 2
            done
            echo "Worker service is healthy"
          '
          
      - name: Run integration tests
        run: |
          cargo nextest run \
            --profile ci \
            --package tasker-core \
            --test '*' \
            --no-fail-fast
            
      - name: Upload test results
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: integration-test-results
          path: target/nextest/ci/junit.xml
          
      - name: Collect service logs
        if: failure()
        run: |
          docker compose -f docker/docker-compose.test.yml logs > service-logs.txt
          
      - name: Upload service logs
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: service-logs
          path: service-logs.txt
```

#### 2.6 CI Success Workflow (.github/workflows/ci-success.yml)

```yaml
name: CI Success

on:
  workflow_call:

jobs:
  success:
    runs-on: ubuntu-22.04
    steps:
      - name: CI Pipeline Success
        run: |
          echo "✅ All CI checks passed successfully!"
          echo "Ready to merge!"
```

### Phase 3: Docker Compose Configuration Updates

The existing `docker/docker-compose.test.yml` is already properly configured for our needs. The key services are:

- **postgres**: PostgreSQL with PGMQ extension from `docker/db/Dockerfile`
- **orchestration**: Orchestration service on port 8080
- **worker**: Rust worker service on port 8081

### Phase 4: Cleanup and Optimization

#### 4.1 Remove Obsolete Components

- [x] Ruby integration tests already removed from Cargo.toml workspace
- [ ] Remove Ruby test job from `.github/workflows/ci.yml`
- [ ] Remove references to `Dockerfile.postgres.extensions` in CI
- [ ] Remove references to non-existent `docker-compose.ci.yml`
- [ ] Remove current benchmark job from CI (defer redesign)

#### 4.2 Benchmark Strategy (Future)

Current benchmarks exist in:
- `tasker-orchestration/benches/orchestration_benchmarks.rs`
- `tasker-shared/benches/shared_benchmarks.rs`
- `tasker-worker/benches/worker_benchmarks.rs`

Future approach should focus on end-to-end performance:

```rust
// tests/performance/end_to_end_benchmark.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_linear_workflow(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("linear_workflow_e2e", |b| {
        b.to_async(&runtime).iter(|| async {
            // Measure time from task creation to completion
            // Using real workflows with realistic data
            let start = std::time::Instant::now();
            // ... create and execute task
            start.elapsed()
        });
    });
}

criterion_group!(benches, benchmark_linear_workflow);
criterion_main!(benches);
```

#### 4.3 Examples to Add

Based on the single existing example (`config_demo.rs`), add meaningful examples:

```rust
// examples/client_usage.rs - Demonstrate tasker-client usage
// examples/config_validator_usage.rs - Show configuration validation
```

### Phase 5: Documentation Updates

#### 5.1 Update CLAUDE.md

Add new CI commands section:

```markdown
### CI and Testing Commands

# Local testing with nextest
cargo nextest run --profile default

# Run CI profile tests locally
cargo nextest run --profile ci

# Run specific package tests
cargo nextest run --package tasker-shared --package tasker-orchestration

# Integration testing with Docker Compose
docker compose -f docker/docker-compose.test.yml up -d --build
cargo nextest run --package tasker-core --test '*'
docker compose -f docker/docker-compose.test.yml down

# View test results
cargo nextest show --profile ci
```

#### 5.2 Create CI Documentation

Create `.github/workflows/README.md`:

```markdown
# CI/CD Pipeline Documentation

## Architecture

Our CI pipeline uses modular workflows for better maintainability:

- **ci.yml**: Main orchestrator that calls other workflows
- **build-docker-images.yml**: Builds and caches Docker images
- **code-quality.yml**: Formatting, linting, security checks
- **test-unit.yml**: Unit tests with nextest
- **test-integration.yml**: Docker Compose integration tests
- **ci-success.yml**: Final status check

## Local Development

### Running Tests Locally

1. Start services:
   ```bash
   docker compose -f docker/docker-compose.test.yml up -d --build
   ```

2. Run unit tests:
   ```bash
   cargo nextest run --profile default
   ```

3. Run integration tests:
   ```bash
   cargo nextest run --package tasker-core --test '*'
   ```

4. Clean up:
   ```bash
   docker compose -f docker/docker-compose.test.yml down
   ```

## Troubleshooting

### Services fail to start
- Check Docker daemon is running
- Verify ports 5432, 8080, 8081 are available
- Check logs: `docker compose -f docker/docker-compose.test.yml logs`

### Tests timeout
- Increase timeout in IntegrationConfig
- Check service health endpoints
- Verify DATABASE_URL is correct
```

## Implementation Steps

### Step 1: Update nextest.toml
- Add CI profile with JUnit output
- Configure test parallelization
- Set appropriate timeouts

### Step 2: Create Modular Workflows
- Split current monolithic ci.yml into components
- Each workflow in separate file
- Use workflow_call for reusability

### Step 3: Tool Installation
- Add cargo binstall for fast tool installation
- Install nextest in test jobs
- Install sccache in all compilation jobs
- Keep sqlx-cli installation where needed

### Step 4: Update Docker References
- Change `Dockerfile.postgres-extensions` to `docker/db/Dockerfile`
- Remove references to `docker-compose.ci.yml`
- Use existing `docker/docker-compose.test.yml`

### Step 5: Integration Test Setup
- Use hoverkraft-tech/compose-action@v2.0.1
- Wait for service health endpoints
- Run tests with nextest CI profile
- Collect logs on failure

### Step 6: Cleanup
- Remove Ruby test job
- Remove benchmark job (defer redesign)
- Clean up obsolete Docker references

## Success Criteria

1. **Performance**: CI pipeline completes in < 10 minutes for typical PRs
2. **Reliability**: No flaky tests, proper service health checks
3. **Visibility**: Clear test output with nextest reporting and JUnit artifacts
4. **Caching**: 50%+ reduction in build times with sccache
5. **Integration**: Full end-to-end workflow validation with Docker Compose

## Migration Checklist

- [ ] Create nextest CI profile in `.config/nextest.toml`
- [ ] Split ci.yml into modular workflows
- [ ] Add cargo binstall for nextest and sccache installation
- [ ] Update Docker image references to correct paths
- [ ] Configure Docker Compose action for integration tests
- [ ] Remove Ruby integration test job
- [ ] Remove current benchmark job
- [ ] Update environment variables across all workflows
- [ ] Test full pipeline in feature branch
- [ ] Update documentation
- [ ] Deploy to main branch

## Risk Mitigation

1. **Rollback Plan**: Keep backup of current CI configuration during transition
2. **Gradual Migration**: Test new workflows in feature branch first
3. **Monitoring**: Use GitHub Actions insights to track performance improvements
4. **Documentation**: Ensure all changes are documented for team onboarding

## Timeline

- **Week 1**: 
  - Update nextest configuration
  - Create modular workflow files
  - Test basic workflow execution

- **Week 2**: 
  - Integrate Docker Compose for integration tests
  - Add sccache for build caching
  - Remove obsolete components

- **Week 3**: 
  - Testing in feature branch
  - Performance optimization
  - Documentation updates

- **Week 4**: 
  - Deploy to main branch
  - Monitor performance
  - Make adjustments based on metrics

## Notes

- Integration tests depend on `IntegrationTestManager` which expects services at localhost:8080 and localhost:8081
- The test helpers already handle Docker Compose service discovery via environment variables
- Nextest doesn't run doctests, requiring a separate `cargo test --doc` step
- sccache requires `RUSTC_WRAPPER=sccache` environment variable
- JUnit XML output enables better test reporting in GitHub UI
# Integration Testing Strategy for Tasker Core

## Executive Summary

This document outlines the implementation strategy for a robust, maintainable integration testing solution using testcontainers-rs for the tasker-core foundational codebase. As an early-stage open source project, we're establishing the correct patterns from day one rather than maintaining legacy approaches. The goal is to create reliable, fast, and reproducible integration tests that work consistently in both local development and CI environments.

## Current State Analysis

### Problems with Current Branch Implementation

1. **Docker Image Caching Issues**
   - `docker-compose.integration.yml` references images like `jcoletaylor/tasker-builder-base:latest`
   - These image tags cause Docker to pull from cached/remote versions instead of building locally
   - Integration tests don't reflect current code changes, creating false positives

2. **Naive Custom Implementation**
   - `docker_test_suite_manager.rs` (~600 lines) reinvents container management
   - Complex singleton pattern with global state management
   - Manual health checks and service orchestration
   - Should be replaced with battle-tested testcontainers-rs

3. **Resource Management Problems**
   - No automatic cleanup, orphaned containers accumulate
   - Port conflicts when tests fail to clean up
   - Brittle command-line Docker interactions

4. **Foundational Pattern Risk**
   - This approach exists only in this branch and hasn't been merged
   - Risk of establishing bad patterns in a new codebase
   - Better to implement correctly from the start

## Solution: Modern testcontainers-rs Foundation

### Why testcontainers-rs?

1. **Industry Standard**
   - Widely adopted in Rust ecosystem
   - Battle-tested with extensive community support
   - Regular updates and maintenance
   - Comprehensive documentation

2. **Built-in Features**
   - Automatic container lifecycle management
   - Proper wait strategies and health checks
   - Automatic cleanup on test completion/failure
   - Network isolation between test suites
   - Dynamic port allocation (no conflicts)

3. **Better Developer Experience**
   - Simple, declarative API
   - Works identically in local and CI environments
   - Built-in support for common services (PostgreSQL, Redis, etc.)
   - Easy custom container definitions

## Implementation Plan

### Phase 1: Infrastructure Setup

#### 1.1 Fix Docker Compose for Local Builds

Create `docker-compose.integration.local.yml` that always builds from local source:

```yaml
# Remove all image: tags that cause caching
services:
  postgres:
    build:
      context: .
      dockerfile: db/Dockerfile
      # NO image: tag here - always build locally
    # ... rest of config

  orchestration:
    build:
      context: ..
      dockerfile: docker/dev/orchestration/Dockerfile
      target: dev-runtime
      # NO image: tag here - always build locally
    # ... rest of config

  worker:
    build:
      context: ..
      dockerfile: docker/dev/workers/rust/Dockerfile
      target: dev-runtime
      # NO image: tag here - always build locally
    # ... rest of config
```

#### 1.2 Docker Build Strategy Analysis

**Current Issues with docker/build/Dockerfile:**

1. **Conflated Use Cases**: The base builder image tries to serve production, CI, and local testing simultaneously
2. **Remote Image Dependency**: Using `jcoletaylor/tasker-builder-base:latest` creates external dependencies
3. **Chef Complexity**: cargo-chef is excellent for production but adds overhead for local testing

**Recommended Three-Tier Strategy:**

```
docker/
├── build/
│   ├── Dockerfile.prod          # Production: chef + full optimization
│   ├── Dockerfile.ci            # CI: cached layers + sccache
│   └── Dockerfile.test          # Testing: fast builds, no external deps
├── db/
│   └── Dockerfile               # PostgreSQL with PGMQ (keep as-is, works well)
└── compose/
    ├── docker-compose.prod.yml  # Production deployment
    ├── docker-compose.ci.yml    # CI pipeline
    └── docker-compose.test.yml  # Local integration tests
```

#### 1.3 Create testcontainers Module Structure

```
src/test_helpers/
├── mod.rs
├── test_utils.rs                    # Existing utilities
├── docker_test_suite_manager.rs     # Current naive implementation (deprecated)
└── testcontainers/
    ├── mod.rs
    ├── postgres_container.rs        # PostgreSQL using docker/db/Dockerfile
    ├── orchestration_container.rs   # Orchestration service container
    ├── worker_container.rs          # Worker service container
    ├── test_suite.rs               # Orchestrated test suite manager
    └── utils.rs                    # Helper functions
```

### Phase 2: Docker Build Optimization

#### 2.1 Production Dockerfile (docker/build/Dockerfile.prod)

```dockerfile
# Production builds with chef for optimal layer caching
FROM rust:1.90-bullseye AS chef
WORKDIR /app
RUN cargo install cargo-chef

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --all-features
```

#### 2.2 CI Dockerfile (docker/build/Dockerfile.ci)

```dockerfile
# CI builds with sccache and GitHub cache
FROM rust:1.90-bullseye AS ci-builder
WORKDIR /app

# Install sccache for distributed caching
RUN cargo install sccache

ENV RUSTC_WRAPPER=sccache
ENV SCCACHE_GHA_ENABLED=true
ENV SCCACHE_CACHE_SIZE=2G

# Copy dependency files first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY */Cargo.toml ./

# Build dependencies
RUN cargo build --release --all-features || true

# Copy source and build
COPY . .
RUN cargo build --release --all-features
```

#### 2.3 Test Dockerfile (docker/build/Dockerfile.test)

```dockerfile
# Fast local test builds without external dependencies
FROM rust:1.90-bullseye AS test-builder
WORKDIR /app

# Install minimal dependencies
RUN apt-get update && apt-get install -y \
    libssl-dev libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy everything and build in debug mode for speed
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --all-features
```

### Phase 3: testcontainers Implementation

#### 3.1 PostgreSQL Container with PGMQ

```rust
// src/test_helpers/testcontainers/postgres_container.rs
use testcontainers::{Container, GenericImage, Docker};

pub struct PgmqPostgres {
    image: GenericImage,
}

impl PgmqPostgres {
    pub fn new() -> Self {
        // Use the existing docker/db/Dockerfile
        let image = GenericImage::new("tasker-postgres-test", "latest")
            .with_env_var("POSTGRES_DB", "tasker_integration")
            .with_env_var("POSTGRES_USER", "tasker")
            .with_env_var("POSTGRES_PASSWORD", "tasker")
            .with_exposed_port(5432)
            .with_wait_for(WaitFor::message_on_stderr("database system is ready to accept connections"));

        Self { image }
    }


}
```

#### 3.2 Orchestration Service Container

```rust
// src/test_helpers/testcontainers/orchestration_container.rs
use testcontainers::{Container, GenericImage, Docker};

pub struct OrchestrationService {
    image: GenericImage,
    database_url: String,
}

impl OrchestrationService {
    pub fn new(database_url: String, build_mode: BuildMode) -> Self {
        // Choose Dockerfile based on build mode
        let dockerfile = match build_mode {
            BuildMode::Fast => "docker/build/Dockerfile.test",
            BuildMode::CI => "docker/build/Dockerfile.ci",
            BuildMode::Production => "docker/build/Dockerfile.prod",
        };

        // Build image from local Dockerfile
        let image = GenericImage::new("tasker-orchestration-test", "latest")
            .with_env_var("DATABASE_URL", database_url.clone())
            .with_env_var("TASKER_ENV", "test")
            .with_env_var("LOG_LEVEL", "info")
            .with_exposed_port(8080)
            .with_wait_for(WaitFor::http("/health")
                .with_status_code(200)
                .with_timeout(Duration::from_secs(60)));

        Self { image, database_url }
    }

    pub async fn start(&self, docker: &Docker) -> Container<'_, GenericImage> {
        // Build the image locally first (respects build mode)
        std::process::Command::new("docker")
            .args(&["build", "-f", self.dockerfile, "-t", "tasker-orchestration-test", "."])
            .output()
            .expect("Failed to build orchestration image");

        docker.run(self.image)
    }

    pub fn api_endpoint(&self, container: &Container<GenericImage>) -> String {
        let port = container.get_host_port(8080);
        format!("http://localhost:{}", port)
    }
}
```

#### 3.3 Worker Service Container

```rust
// src/test_helpers/testcontainers/worker_container.rs
use testcontainers::{Container, GenericImage, Docker};

pub struct WorkerService {
    image: GenericImage,
    database_url: String,
}

impl WorkerService {
    pub fn new(database_url: String, build_mode: BuildMode) -> Self {
        // Same build mode strategy as orchestration
        let dockerfile = match build_mode {
            BuildMode::Fast => "docker/build/Dockerfile.test",
            BuildMode::CI => "docker/build/Dockerfile.ci",
            BuildMode::Production => "docker/build/Dockerfile.prod",
        };

        let image = GenericImage::new("tasker-worker-test", "latest")
            .with_env_var("DATABASE_URL", database_url.clone())
            .with_env_var("TASKER_ENV", "test")
            .with_env_var("LOG_LEVEL", "info")
            .with_exposed_port(8081)
            .with_wait_for(WaitFor::http("/health")
                .with_status_code(200)
                .with_timeout(Duration::from_secs(60)));

        Self { image, database_url }
    }

    pub async fn start(&self, docker: &Docker) -> Container<'_, GenericImage> {
        docker.run(self.image)
    }

    pub fn api_endpoint(&self, container: &Container<GenericImage>) -> String {
        let port = container.get_host_port(8081);
        format!("http://localhost:{}", port)
    }
}
```

#### 3.4 Build Mode Configuration

```rust
// src/test_helpers/testcontainers/utils.rs
pub enum BuildMode {
    Fast,       // Local development: debug builds, no optimization
    CI,         // CI pipeline: sccache + GitHub cache
    Production, // Production: chef + full optimization
}

impl BuildMode {
    pub fn from_env() -> Self {
        match std::env::var("TEST_BUILD_MODE").as_deref() {
            Ok("ci") => BuildMode::CI,
            Ok("prod") | Ok("production") => BuildMode::Production,
            _ => BuildMode::Fast, // Default for local development
        }
    }
}
```

#### 3.5 Integrated Test Suite Manager

```rust
// src/test_helpers/testcontainers/test_suite.rs
use testcontainers::Docker;

pub struct TestcontainersTestSuite {
    docker: Docker,
    postgres: Option<Container<'_, PgmqPostgres>>,
    orchestration: Option<Container<'_, GenericImage>>,
    worker: Option<Container<'_, GenericImage>>,
    database_url: String,
    orchestration_url: String,
    worker_url: String,
}

impl TestcontainersTestSuite {
    pub async fn start() -> Result<Self> {
        let docker = Docker::default();
        let build_mode = BuildMode::from_env();

        // Start PostgreSQL with PGMQ (using existing docker/db/Dockerfile)
        let postgres_container = PgmqPostgres::new();
        let postgres = postgres_container.start(&docker).await;
        let database_url = postgres_container.connection_string(&postgres);

        // Run migrations using existing MIGRATOR infrastructure
        let pool = PgPool::connect(&database_url).await?;
        tasker_core::test_helpers::MIGRATOR.run(&pool).await?;

        // Start Orchestration Service with appropriate build mode
        let orchestration_service = OrchestrationService::new(database_url.clone(), build_mode);
        let orchestration = orchestration_service.start(&docker).await;
        let orchestration_url = orchestration_service.api_endpoint(&orchestration);

        // Start Worker Service with appropriate build mode
        let worker_service = WorkerService::new(database_url.clone(), build_mode);
        let worker = worker_service.start(&docker).await;
        let worker_url = worker_service.api_endpoint(&worker);

        Ok(Self {
            docker,
            postgres: Some(postgres),
            orchestration: Some(orchestration),
            worker: Some(worker),
            database_url,
            orchestration_url,
            worker_url,
        })
    }

    pub fn orchestration_client(&self) -> OrchestrationApiClient {
        OrchestrationApiClient::new(OrchestrationApiConfig {
            base_url: self.orchestration_url.clone(),
            timeout_ms: 30000,
            max_retries: 3,
            auth: None,
        })
    }

    pub fn worker_client(&self) -> WorkerApiClient {
        WorkerApiClient::new(WorkerApiConfig {
            base_url: self.worker_url.clone(),
            timeout_ms: 30000,
            max_retries: 3,
            auth: None,
        })
    }
}

// Automatic cleanup on drop
impl Drop for TestcontainersTestSuite {
    fn drop(&mut self) {
        // testcontainers automatically cleans up containers
        // This is just for logging
        tracing::info!("Cleaning up test containers");
    }
}
```

### Phase 4: CI/CD Optimization Strategy

#### 4.1 Integration with Existing Test Infrastructure

The testcontainers implementation leverages existing test utilities:

```rust
// Use existing MIGRATOR and test setup
use tasker_core::test_helpers::{MIGRATOR, setup_test_environment, get_test_database_url};

impl TestcontainersTestSuite {
    pub async fn setup_database(&self, database_url: &str) -> Result<PgPool> {
        let pool = PgPool::connect(database_url).await?;

        // Use existing MIGRATOR infrastructure
        MIGRATOR.run(&pool).await?;

        Ok(pool)
    }
}

// Test pattern using existing utilities
#[tokio::test]
async fn test_with_existing_infrastructure() {
    setup_test_environment(); // Existing helper

    let suite = TestcontainersTestSuite::start().await?;
    let pool = suite.setup_database(&suite.database_url).await?;

    // Test with properly migrated database
}
```

#### 4.2 GitHub Actions Cache Configuration

```yaml
# .github/workflows/integration-tests.yml
name: Integration Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v2

      - name: Cache Docker layers
        uses: actions/cache@v3
        with:
          path: /tmp/.buildx-cache
          key: ${{ runner.os }}-buildx-${{ github.sha }}
          restore-keys: |
            ${{ runner.os }}-buildx-

      - name: Log in to GitHub Container Registry
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Build test images with cache
        run: |
          docker buildx build \
            --cache-from type=gha \
            --cache-to type=gha,mode=max \
            -f docker/build/Dockerfile.ci \
            -t tasker-test-base .

      - name: Run sccache-cache
        uses: mozilla-actions/sccache-action@v0.0.4

      - name: Run integration tests
        env:
          TEST_BUILD_MODE: ci
          RUSTC_WRAPPER: sccache
        run: cargo test --test integration --all-features
```

#### 4.2 Local Development Optimization

```bash
# Fast local builds (default)
cargo test --test integration

# Use CI-like build for consistency testing
TEST_BUILD_MODE=ci cargo test --test integration

# Production-like build for performance testing
TEST_BUILD_MODE=production cargo test --test integration
```

### Phase 5: Test Migration Strategy

#### 5.1 Test Implementation Strategy

**Complete Replacement Approach:**

1. **Delete Existing Tests**: Remove `tests/docker_integration/all_workflows_test.rs`
2. **Delete Naive Manager**: Remove `src/test_helpers/docker_test_suite_manager.rs`
3. **Build New Foundation**: Implement testcontainers-based tests from scratch

#### 5.2 New Test Patterns

**Modern testcontainers Pattern:**

```rust
#[tokio::test]
async fn test_linear_workflow_integration() {
    let suite = TestcontainersTestSuite::start().await?;
    let client = suite.orchestration_client();

    let context = create_mathematical_test_context(6);
    let request = create_test_task_request(
        "linear_workflow", "mathematical_sequence", "1.0.0",
        context, "Integration test for linear mathematical workflow"
    );

    let task = client.create_task(request).await?;
    let result = wait_for_completion(&client, &task.task_uuid, 30).await?;

    assert_eq!(result.status, "completed");
    assert_eq!(result.completion_percentage, 100.0);
    assert_eq!(result.total_steps, 4);
    assert_eq!(result.failed_steps, 0);
}
```

#### 5.3 Parallel Test Execution

With testcontainers, each test gets isolated containers:

```rust
// These can run in parallel safely
#[tokio::test]
async fn test_workflow_1() {
    let suite = TestcontainersTestSuite::start().await?;
    // Test uses isolated containers
}

#[tokio::test]
async fn test_workflow_2() {
    let suite = TestcontainersTestSuite::start().await?;
    // Different isolated containers, no interference
}
```



## Implementation Timeline

### Week 1: Foundation Replacement
- [ ] **Remove** `docker_test_suite_manager.rs` entirely
- [ ] **Remove** `docker-compose.integration.yml` with image tags
- [ ] Create three-tier Dockerfiles (prod, ci, test)
- [ ] Implement BuildMode enum and detection
- [ ] Set up GitHub Container Registry caching

### Week 2: testcontainers Implementation
- [ ] Create new testcontainers module structure
- [ ] Implement PostgreSQL container using existing `docker/db/Dockerfile`
- [ ] Build orchestration and worker service containers
- [ ] Create integrated test suite manager

### Week 3: Test Implementation
- [ ] **Replace** all existing integration tests with testcontainers approach
- [ ] Implement comprehensive test patterns
- [ ] Configure CI/CD with new approach
- [ ] Performance optimization and validation

### Week 4: Documentation and Finalization
- [ ] Complete documentation and examples
- [ ] Team training materials
- [ ] Benchmarking and optimization
- [ ] Merge foundational implementation

## Benefits of This Foundation-First Approach

1. **Establishes Correct Patterns**
   - Modern testcontainers-rs from day one
   - Industry-standard container management
   - No technical debt from naive implementations

2. **Build Optimization**
   - **Local**: Fast debug builds (30s vs 3min)
   - **CI**: Cached layers with sccache (50% faster)
   - **Production**: Optimized binaries with chef

3. **Cost Reduction**
   - GitHub Actions cache reduces Docker pulls by 60%
   - sccache eliminates redundant compilation
   - Shared base layers across CI runs

4. **Reliability**
   - Automatic cleanup prevents resource leaks
   - Isolated test environments prevent interference
   - Consistent PostgreSQL with PGMQ and UUID v7 extensions

5. **Developer Experience**
   - Simple, intuitive testcontainers API
   - Fast local iteration with debug builds
   - CI/production parity when needed for performance testing

6. **Open Source Best Practices**
   - Clear, maintainable code from the start
   - No external image dependencies
   - Documentation-driven development

## Success Metrics

- **Foundation Quality**: Clean testcontainers implementation from day one
- **Test Reliability**: >99.9% consistent pass/fail across runs
- **Test Speed**: <50% reduction in total test time through parallelization
- **Resource Usage**: Zero orphaned containers after test runs
- **CI Cost Reduction**: 60% reduction in Docker layer fetching
- **Developer Satisfaction**: Fast local iteration, easy debugging

## Appendix A: Common Patterns

### Pattern 1: Database-Only Tests

```rust
use tasker_core::test_helpers::{MIGRATOR, setup_test_environment};

#[tokio::test]
async fn test_database_operations() {
    setup_test_environment(); // Use existing helper

    let postgres = PgmqPostgres::new().start().await;
    let db_url = postgres.connection_string();

    let pool = PgPool::connect(&db_url).await?;
    MIGRATOR.run(&pool).await?; // Use existing MIGRATOR

    // Test database operations with PGMQ and UUID v7 extensions available
    let result = sqlx::query!("SELECT uuid_generate_v7() as test_uuid").fetch_one(&pool).await?;
    assert!(result.test_uuid.is_some());
}
```

### Pattern 2: Service Integration Tests

```rust
use tasker_core::test_helpers::setup_test_environment;

#[tokio::test]
async fn test_orchestration_workflow() {
    setup_test_environment(); // Use existing helper

    let suite = TestcontainersTestSuite::start().await?;
    let client = suite.orchestration_client();

    // Create task using existing test utilities
    let context = create_mathematical_test_context(6);
    let request = create_test_task_request(
        "linear_workflow", "mathematical_sequence", "1.0.0",
        context, "Service integration test"
    );

    let task = client.create_task(request).await?;
    let result = wait_for_task_completion(&client, &task.uuid).await?;

    assert_eq!(result.status, "completed");
}
```

### Pattern 3: Performance Tests

```rust
use tasker_core::test_helpers::{setup_test_environment, create_mathematical_test_context, create_test_task_request};

#[tokio::test]
async fn test_concurrent_workflows() {
    setup_test_environment(); // Use existing helper

    let suite = TestcontainersTestSuite::start().await?;
    let client = suite.orchestration_client();

    // Start multiple workflows concurrently using existing test utilities
    let tasks = futures::future::join_all(
        (0..10).map(|i| {
            let client = client.clone();
            async move {
                let context = create_mathematical_test_context((i + 1) * 2);
                let request = create_test_task_request(
                    "linear_workflow", "mathematical_sequence", "1.0.0",
                    context, &format!("Concurrent test {}", i)
                );
                client.create_task(request).await
            }
        })
    ).await;

    // Verify all completed
    for task in tasks {
        let result = wait_for_completion(&client, &task?.task_uuid).await?;
        assert_eq!(result.status, "completed");
    }
}
```

## Appendix B: Troubleshooting

### Issue: Containers fail to start
- Check Docker daemon is running
- Verify sufficient resources (CPU, memory, disk)
- Check image build logs for errors

### Issue: Tests timeout
- Increase wait strategies timeout
- Check service health endpoints
- Verify database migrations completed

### Issue: Port conflicts
- testcontainers uses random ports by default
- Check for hardcoded ports in test code
- Ensure no manual port bindings

### Issue: Slow test startup
- Pre-build images in CI
- Use Docker layer caching
- Optimize Dockerfiles for faster builds

## Conclusion

This foundation-first implementation of testcontainers-rs establishes the correct integration testing patterns for tasker-core from day one. By aggressively replacing the naive Docker approach before it becomes established, we ensure a clean, maintainable, and developer-friendly testing infrastructure that will serve the project well as it grows.

As an early-stage open source project, establishing these patterns correctly now prevents technical debt and provides a solid foundation for contributors and maintainers.

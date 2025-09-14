# TAS-41: Docker-Based Integration Testing System

## Overview

Implement a comprehensive Docker-based integration testing system that replaces the problematic in-process test coordination with proper containerized services. This will provide reliable, CI-compatible testing that mirrors production deployment architecture.

## Problem Statement

Current integration tests suffer from:
- **Resource Contention**: Multiple tests creating workers for same namespaces
- **Complex In-Process Coordination**: Orchestration + worker systems in same process
- **Hanging Event Listeners**: Workers waiting indefinitely for FFI events
- **HTTP API Coordination Issues**: Tests expect localhost:8080 but orchestration may not bind properly
- **Non-Deterministic Failures**: Timing issues and race conditions

## Solution Architecture

### High-Level Approach
1. **Containerized Services**: Separate containers for database, orchestration, and workers
2. **Unified API Client**: Single client library for both orchestration and worker APIs
3. **CLI Tool**: Rust Clap-based CLI for manual testing and debugging
4. **CI-Compatible**: Docker Compose setup that works in GitHub Actions
5. **Test Isolation**: Each test gets clean container environment

## Implementation Plan

### Phase 1: API Client Foundation (tasker-client)

#### 1.1 Create Unified API Client Library
**Location**: `tasker-client/src/api_clients/`

**Components**:
- **OrchestrationClient**: HTTP client for orchestration API
  - Task CRUD operations (`POST /v1/tasks`, `GET /v1/tasks/{id}`)
  - Task status polling and completion checking
  - Namespace management
- **WorkerClient**: HTTP client for worker API
  - Worker status and health endpoints
  - Step execution monitoring
  - Worker lifecycle management

**Shared Types**: Use types from `tasker-shared/src/types/web/` and `tasker-shared/src/types/api/`

**Features**:
```rust
// Core client traits
pub trait OrchestrationApiClient {
    async fn create_task(&self, request: TaskRequest) -> Result<TaskResponse>;
    async fn get_task(&self, id: Uuid) -> Result<TaskStatusResponse>;
    async fn list_tasks(&self, filters: TaskFilters) -> Result<Vec<TaskSummary>>;
}

pub trait WorkerApiClient {
    async fn get_worker_status(&self, worker_id: &str) -> Result<WorkerStatus>;
    async fn list_workers(&self, namespace: Option<&str>) -> Result<Vec<WorkerInfo>>;
    async fn worker_health(&self, worker_id: &str) -> Result<HealthStatus>;
}
```

#### 1.2 Create CLI Tool
**Location**: `tasker-client/src/bin/tasker-cli.rs`

**Commands**:
```bash
# Task operations
tasker-cli task create --namespace linear_workflow --name sequence --input '{"even_number": 6}'
tasker-cli task get --id uuid-here
tasker-cli task list --namespace linear_workflow --status pending

# Worker operations
tasker-cli worker list --namespace linear_workflow
tasker-cli worker status --id worker-id
tasker-cli worker health --all

# System operations
tasker-cli system health --orchestration --workers
tasker-cli system info
```

**Configuration**:
- Environment-based config (`TASKER_ORCHESTRATION_URL`, `TASKER_WORKER_URL`)
- Config file support (`~/.tasker/config.toml`)
- Command-line overrides

### Phase 2: Docker Infrastructure

#### 2.1 Enhanced Orchestration Dockerfile
**Location**: `tasker-orchestration/Dockerfile`

**Current State**: Already solid foundation with multi-stage build
**Enhancements Needed**:
- Health check endpoint validation
- Proper configuration mounting
- Test-specific environment variables

#### 2.2 Test-Specific Orchestration Dockerfile
**Location**: `tasker-orchestration/Dockerfile.test`

**Purpose**: Test-optimized image with:
- Debug logging enabled
- Test database configurations
- Faster startup times
- Development tools included

```dockerfile
FROM tasker-orchestration:latest AS test-base

# Add test-specific configurations
COPY config/test/ /app/config/test/
ENV TASKER_ENV=test
ENV RUST_LOG=debug
ENV TASKER_TEST_MODE=true

# Add development tools for debugging
USER root
RUN apt-get update && apt-get install -y curl jq netcat-openbsd
USER tasker

# Test-specific health check (more frequent)
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1
```

#### 2.3 Worker Dockerfile
**Location**: `tasker-worker/Dockerfile` (new)

**Multi-Stage Build**:
```dockerfile
# Build stage
FROM rust:1.80-slim AS builder
# ... similar to orchestration Dockerfile ...
RUN cargo build --release --bin tasker-worker --features web-api

# Runtime stage
FROM debian:bookworm-slim
# ... runtime setup ...
COPY --from=builder /app/target/release/tasker-worker /app/tasker-worker
ENV TASKER_WORKER_ID=docker-worker-001
ENV TASKER_NAMESPACES=linear_workflow,diamond_workflow,tree_workflow
EXPOSE 8081
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1
CMD ["./tasker-worker"]
```

#### 2.4 Rust Worker Integration Dockerfile
**Location**: `workers/rust/Dockerfile`

**Purpose**: Integration test worker with native Rust step handlers
**Base**: Uses `tasker-worker/Dockerfile` as foundation but includes Rust step handlers

```dockerfile
# Build stage - extends worker build with rust handlers
FROM rust:1.80-slim AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libpq-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo

# Copy all workspace crates
COPY tasker-shared/ tasker-shared/
COPY tasker-worker/ tasker-worker/
COPY workers/rust/ workers/rust/

# Build the rust worker with all step handlers
RUN cargo build --release --bin rust-worker -p tasker-worker-rust

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 libpq5 curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 tasker

# Create directories
RUN mkdir -p /app/config /app/log && \
    chown -R tasker:tasker /app

# Copy binary from builder
COPY --from=builder /app/target/release/rust-worker /app/rust-worker
COPY --from=builder /app/config /app/config

# Set ownership
RUN chown -R tasker:tasker /app
USER tasker
WORKDIR /app

# Multi-namespace worker configuration
ENV TASKER_WORKER_ID=rust-integration-worker
ENV TASKER_NAMESPACES=linear_workflow,diamond_workflow,tree_workflow,mixed_dag_workflow,order_fulfillment
ENV TASKER_ENV=integration_test

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

EXPOSE 8081
CMD ["./rust-worker"]
```

#### 2.5 Base Worker Dockerfile
**Location**: `tasker-worker/Dockerfile`

**Purpose**: Foundation worker image for standard deployments
**Note**: This becomes the base for language-specific worker implementations

### Phase 3: Integration Test Infrastructure

#### 3.1 Docker Compose for Integration Testing
**Location**: `docker-compose.integration.yml`

```yaml
version: '3.8'

services:
  postgres:
    build:
      context: .
      dockerfile: Dockerfile.postgres-extensions
    environment:
      POSTGRES_DB: tasker_integration_test
      POSTGRES_USER: tasker
      POSTGRES_PASSWORD: tasker
    ports:
      - "5433:5432"  # Different port to avoid conflicts
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U tasker -d tasker_integration_test"]
      interval: 5s
      timeout: 3s
      retries: 10
      start_period: 10s

  orchestration:
    build:
      context: tasker-orchestration
      dockerfile: Dockerfile.test
    environment:
      TASKER_ENV: integration_test
      DATABASE_URL: postgresql://tasker:tasker@postgres:5432/tasker_integration_test
      RUST_LOG: info
    ports:
      - "8080:8080"
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 10s
      timeout: 5s
      retries: 10
      start_period: 20s

  worker:
    build:
      context: .
      dockerfile: workers/rust/Dockerfile
    environment:
      TASKER_ENV: integration_test
      DATABASE_URL: postgresql://tasker:tasker@postgres:5432/tasker_integration_test
      TASKER_ORCHESTRATION_URL: http://orchestration:8080
      TASKER_WORKER_ID: rust-integration-worker
      # Multi-namespace worker handles all workflow types
      TASKER_NAMESPACES: linear_workflow,diamond_workflow,tree_workflow,mixed_dag_workflow,order_fulfillment
      RUST_LOG: info
    ports:
      - "8081:8081"
    depends_on:
      orchestration:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 10s
      timeout: 5s
      retries: 10
      start_period: 15s

networks:
  default:
    name: tasker-integration
```

#### 3.2 Integration Test Framework
**Location**: `tests/integration/docker_integration_tests.rs`

```rust
use tasker_client::api_clients::{OrchestrationClient, WorkerClient};
use tokio::time::{sleep, Duration};

pub struct DockerIntegrationTestSetup {
    orchestration_client: OrchestrationClient,
    worker_clients: HashMap<String, WorkerClient>,
    test_id: String,
}

impl DockerIntegrationTestSetup {
    pub async fn new() -> Result<Self> {
        // Wait for services to be healthy
        Self::wait_for_services().await?;

        let orchestration_client = OrchestrationClient::new("http://localhost:8080")?;
        let mut worker_clients = HashMap::new();
        // Single multi-namespace worker handles all workflow types
        worker_clients.insert(
            "rust-worker".to_string(),
            WorkerClient::new("http://localhost:8081")?
        );

        Ok(Self {
            orchestration_client,
            worker_clients,
            test_id: Uuid::new_v4().to_string(),
        })
    }

    async fn wait_for_services() -> Result<()> {
        // Health check all services before proceeding
        // This replaces the complex in-process coordination
    }

    pub async fn run_workflow_test(&self, namespace: &str, test_data: TaskRequest) -> Result<TaskExecutionSummary> {
        // Create task via orchestration API
        let task_response = self.orchestration_client.create_task(test_data).await?;

        // Poll for completion with timeout
        self.wait_for_task_completion(task_response.task_uuid, Duration::from_secs(30)).await
    }
}

#[tokio::test]
async fn test_linear_workflow_integration() {
    let setup = DockerIntegrationTestSetup::new().await.unwrap();

    let task_request = TaskRequest {
        namespace: "linear_workflow".to_string(),
        name: "mathematical_sequence".to_string(),
        context: json!({"even_number": 6}),
        // ... other fields
    };

    let result = setup.run_workflow_test("linear_workflow", task_request).await.unwrap();

    assert_eq!(result.status, "completed");
    assert_eq!(result.completion_percentage, 100.0);
}
```

### Phase 4: CI/CD Integration

#### 4.1 GitHub Actions Workflow
**Location**: `.github/workflows/integration-tests.yml`

```yaml
name: Integration Tests

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  integration-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 20

    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Start Integration Test Services
        run: |
          docker-compose -f docker-compose.integration.yml up -d

      - name: Wait for Services
        run: |
          ./scripts/wait-for-services.sh

      - name: Run Integration Tests
        run: |
          cargo test --test docker_integration_tests -- --test-threads=1

      - name: Collect Logs on Failure
        if: failure()
        run: |
          docker-compose -f docker-compose.integration.yml logs > integration-logs.txt

      - name: Upload Logs
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: integration-logs
          path: integration-logs.txt

      - name: Clean Up
        if: always()
        run: |
          docker-compose -f docker-compose.integration.yml down -v
```

#### 4.2 Service Wait Script
**Location**: `scripts/wait-for-services.sh`

```bash
#!/bin/bash
set -e

echo "Waiting for PostgreSQL..."
until docker-compose -f docker-compose.integration.yml exec -T postgres pg_isready -U tasker; do
    echo "PostgreSQL is unavailable - sleeping"
    sleep 2
done

echo "Waiting for Orchestration API..."
until curl -f http://localhost:8080/health; do
    echo "Orchestration API is unavailable - sleeping"
    sleep 2
done

echo "Waiting for Rust Worker..."
until curl -f http://localhost:8081/health; do
    echo "Rust Worker is unavailable - sleeping"
    sleep 2
done

echo "All services are ready!"
```

## Implementation Sequence

### Week 1: API Client Foundation
1. **Day 1-2**: Create `tasker-client` crate structure
2. **Day 3-4**: Implement OrchestrationClient with shared types
3. **Day 5**: Add WorkerClient following orchestration pattern

### Week 2: CLI Tool & Docker Infrastructure
1. **Day 1-2**: Build CLI tool with basic commands
2. **Day 3**: Create worker Dockerfile and Dockerfile.test
3. **Day 4**: Enhance orchestration Dockerfile.test
4. **Day 5**: Create docker-compose.integration.yml

### Week 3: Integration Test Framework
1. **Day 1-2**: Build DockerIntegrationTestSetup
2. **Day 3-4**: Implement workflow test patterns
3. **Day 5**: Add service health checking and wait logic

### Week 4: CI/CD & Refinement
1. **Day 1-2**: Create GitHub Actions workflow
2. **Day 3**: Add service wait scripts and logging
3. **Day 4-5**: End-to-end testing and bug fixes

## Benefits

### Immediate
- **Reliable Tests**: No more hanging or race conditions
- **True Integration**: Tests actual HTTP APIs and container deployment
- **Isolated Environment**: Each test run gets clean containers
- **Debugging Capability**: CLI tool for manual testing

### Long-term
- **CI-Compatible**: Ready for GitHub Actions and other CI systems
- **Production Parity**: Test environment mirrors deployment architecture
- **Scalable**: Easy to add new workflow types and test scenarios
- **Maintainable**: Clear separation between services and test logic

## Success Criteria

1. **All integration tests pass consistently** in Docker environment
2. **CLI tool successfully interacts** with orchestration and worker APIs
3. **GitHub Actions workflow** completes integration tests in under 15 minutes
4. **No hanging tests** or resource contention issues

## Risk Mitigation

### Docker Performance
- **Risk**: Slow container startup affecting test times
- **Mitigation**: Optimize Dockerfiles with proper caching, test-specific images

### Service Coordination
- **Risk**: Services not ready when tests start
- **Mitigation**: Robust health checking and wait scripts

### Resource Usage
- **Risk**: Too many containers overwhelming CI resources
- **Mitigation**: Minimal container images, selective service startup per test

### Network Issues
- **Risk**: Container networking problems affecting API calls
- **Mitigation**: Explicit network configuration, connection retry logic

## Conclusion

This Docker-based approach replaces the problematic in-process coordination with a robust, scalable integration testing system that mirrors production deployment. The unified API client provides both library and CLI interfaces, while the containerized infrastructure ensures reliable, reproducible test execution.

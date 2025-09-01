# Docker Build Strategy

This directory contains the reorganized Docker build strategy for Tasker Core, implementing a layered and sequential approach for consistency, performance, and maintainability.

## Directory Structure

```
docker/
├── build/
│   └── Dockerfile                      # Common Rust builder base with cargo-chef
├── deploy/                             # Production-optimized builds
│   ├── orchestration/
│   │   └── Dockerfile                  # Optimized orchestration service
│   └── workers/
│       ├── rust/
│       │   └── Dockerfile              # Optimized comprehensive Rust worker
│       ├── ruby/
│       │   └── Dockerfile              # Ruby worker with FFI integration
│       ├── python/
│       │   └── Dockerfile              # Python worker with PyO3 integration
│       └── wasm/
│           └── Dockerfile              # WASM worker with wasmtime runtime
├── dev/                                # Development-optimized builds
│   ├── orchestration/
│   │   └── Dockerfile                  # Dev orchestration with debugging
│   └── workers/
│       └── rust/
│           └── Dockerfile              # Dev Rust worker with debugging
├── scripts/                            # Convenience and configuration scripts
│   ├── run-integration.sh              # Start integration testing environment
│   ├── run-dev.sh                      # Start development environment
│   ├── run-dev-db.sh                   # Start database only for development
│   ├── run-prod.sh                     # Start production environment
│   ├── build-images.sh                 # Build all Docker images with tagging
│   ├── cleanup.sh                      # Cleanup Docker resources
│   ├── postgres-init.sql               # PostgreSQL initialization script
│   ├── postgres.conf                   # PostgreSQL integration config
│   ├── postgres-dev.conf               # PostgreSQL development config
│   └── postgres-prod.conf              # PostgreSQL production config
├── docker-compose.integration.yml      # Integration testing environment
├── docker-compose.dev.yml              # Development environment
├── docker-compose.deploy.yml           # Production deployment
├── docker-compose.db.yml               # Database-only for development
├── Dockerfile.postgres-extensions      # PostgreSQL with PGMQ and UUID v7
└── README.md                           # This documentation
```

## Key Features

### 🚀 Performance Optimizations

- **cargo-chef integration**: Docker layer caching optimization for Rust builds
- **sccache support**: Distributed compilation caching for CI environments
- **Multi-stage builds**: Minimal runtime images with optimized binaries
- **Rust 1.89**: Latest Rust version for improved performance and features

### 🏗️ Layered Architecture

- **Common Builder Base** (`docker/build/Dockerfile`): Shared Rust dependencies and build tools
- **Deploy Builds** (`docker/deploy/`): Production-optimized with release builds and minimal runtime
- **Dev Builds** (`docker/dev/`): Development-optimized with debug builds and enhanced tooling

### 🔒 Security & Best Practices

- **Non-root execution**: All containers run as unprivileged users
- **Minimal runtime images**: Only essential dependencies in final images
- **Resource limits**: Configurable CPU and memory constraints
- **Health checks**: Comprehensive health monitoring for all services

## Usage

### Environment Variables (Cleaned Up)

Only essential environment variables are used:

- `DATABASE_URL`: PostgreSQL connection string
- `TASKER_ENV`: Environment (development, integration, production)
- `WORKSPACE_PATH`: Application workspace path
- `TASKER_CONFIG_ROOT`: Configuration root directory

### Quick Start with Convenience Scripts

```bash
# Navigate to docker directory
cd docker

# Integration Testing
./scripts/run-integration.sh

# Development Environment  
./scripts/run-dev.sh

# Database Only (for local development)
./scripts/run-dev-db.sh start

# Production Deployment
POSTGRES_PASSWORD=secure_password ./scripts/run-prod.sh

# Build All Images
./scripts/build-images.sh all latest

# Cleanup Resources
./scripts/cleanup.sh basic
```

### Manual Docker Compose Usage

```bash
# Navigate to docker directory first
cd docker

# Development environment with debugging enabled
docker-compose -f docker-compose.dev.yml up --build

# Database only for development
docker-compose -f docker-compose.db.yml up -d

# Integration test environment
docker-compose -f docker-compose.integration.yml up --build

# Production environment
POSTGRES_PASSWORD=secure_password docker-compose -f docker-compose.deploy.yml up --build -d

# Scale workers as needed
docker-compose -f docker-compose.deploy.yml up --scale worker=3 -d
```

### Advanced Features

```bash
# Access Tokio console for debugging (development mode)
# Orchestration: http://localhost:6669
# Worker: http://localhost:6670

# Run integration tests
cd .. && cargo test --all-features -- docker_integration

# Build specific components
./scripts/build-images.sh rust v1.0.0        # Build only Rust worker
./scripts/build-images.sh orchestration      # Build orchestration service
```

## Build Optimization

### cargo-chef Benefits

- **Dependency Caching**: Dependencies are built in a separate layer that's cached when `Cargo.toml` doesn't change
- **Faster Rebuilds**: Source code changes don't invalidate dependency compilation
- **CI Efficiency**: Significant time savings in automated builds

### sccache Integration

For CI environments, add these environment variables to enable distributed compilation caching:

```dockerfile
ENV RUSTC_WRAPPER=sccache
ENV SCCACHE_GHA_ENABLED=true
ENV SCCACHE_CACHE_SIZE=2G
```

And use the Mozilla sccache action in GitHub Actions:

```yaml
- name: Run sccache-cache
  uses: mozilla-actions/sccache-action@v0.0.4
```

## Multi-Language Worker Support

The new structure supports multiple worker languages:

- **Rust Worker**: Comprehensive worker handling all namespaces
- **Ruby Worker**: FFI integration with Rust core via Ruby extension
- **Python Worker**: PyO3 integration with Rust core via Python bindings
- **WASM Worker**: Sandboxed execution via wasmtime runtime

Enable additional workers by uncommenting the relevant sections in the docker-compose files.

## Health Monitoring

All services include comprehensive health checks:

- **Database**: Connection validation and PGMQ extension verification
- **Orchestration**: API endpoint health and database connectivity
- **Workers**: Service health, database connectivity, and orchestration integration

## Resource Management

Production deployments include resource limits:

```yaml
deploy:
  resources:
    limits:
      memory: 2G
      cpus: '1.0'
    reservations:
      memory: 1G
      cpus: '0.5'
```

## Convenience Scripts

### Available Scripts

- **`run-integration.sh`**: Start complete integration testing environment
- **`run-dev.sh`**: Start development environment with debugging tools
- **`run-dev-db.sh`**: Start database-only for local development (PostgreSQL + PGMQ + UUID v7)
- **`run-prod.sh`**: Start production environment with security and performance optimizations
- **`build-images.sh`**: Build Docker images with proper tagging and registry push options
- **`cleanup.sh`**: Clean up Docker resources with different levels (containers, basic, images, cache, deep)

### Script Usage Examples

```bash
# Integration testing
./scripts/run-integration.sh
# Outputs service endpoints and health check commands

# Development with debugging
./scripts/run-dev.sh  
# Enables Tokio console, hot-reloading, enhanced logging

# Database only (lightweight for local development)
./scripts/run-dev-db.sh start
# PostgreSQL with PGMQ + UUID v7 extensions

# Production deployment
POSTGRES_PASSWORD=secure_password ./scripts/run-prod.sh
# Shows scaling examples and monitoring commands

# Build specific images
./scripts/build-images.sh rust           # Build only Rust worker
./scripts/build-images.sh all v1.2.0     # Build all with version tag
./scripts/build-images.sh all latest true # Build all and push to registry

# Cleanup options
./scripts/cleanup.sh containers          # Stop containers only
./scripts/cleanup.sh basic              # Standard cleanup
./scripts/cleanup.sh deep               # Full cleanup (removes all Docker resources)
```

### Database-Only Development

For lightweight local development when you only need PostgreSQL:

```bash
# Start database with all extensions
./scripts/run-dev-db.sh start

# Available database commands
./scripts/run-dev-db.sh psql             # Connect to database shell
./scripts/run-dev-db.sh test             # Test database and extensions
./scripts/run-dev-db.sh status           # Check database status
./scripts/run-dev-db.sh logs             # View database logs
./scripts/run-dev-db.sh reset            # Reset all data (destructive)
./scripts/run-dev-db.sh stop             # Stop all services

# Connection details
# DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test
# Includes: PGMQ extension, UUID v7 extension, all PGMQ queues pre-created
```

## Migration from Old Structure

All Docker-related files have been consolidated into the `docker/` directory:

**Moved Files:**
- `docker-compose.*.yml` → `docker/docker-compose.*.yml`
- `Dockerfile.postgres-extensions` → `docker/Dockerfile.postgres-extensions`
- `scripts/postgres*` → `docker/scripts/postgres*`

**Replaced Dockerfiles:**
- `workers/rust/Dockerfile` → `docker/deploy/workers/rust/Dockerfile`
- `tasker-orchestration/Dockerfile.test` → `docker/dev/orchestration/Dockerfile`
- `tasker-orchestration/Dockerfile` → `docker/deploy/orchestration/Dockerfile`
- `tasker-worker/Dockerfile` → Base functionality merged into comprehensive workers

**All docker-compose commands now require `cd docker` first** or use the convenience scripts.

## Troubleshooting

### Build Issues

1. **Builder base not found**: Ensure you build the base first:
   ```bash
   docker-compose --profile build up builder-base
   ```

2. **cargo-chef cache issues**: Clear Docker build cache:
   ```bash
   docker builder prune
   ```

3. **Memory issues during build**: Increase Docker memory allocation or use smaller concurrent build jobs.

### Runtime Issues

1. **Health check failures**: Check service logs:
   ```bash
   docker-compose logs -f orchestration
   docker-compose logs -f worker
   ```

2. **Database connection issues**: Verify PostgreSQL is healthy:
   ```bash
   docker-compose exec postgres pg_isready -U tasker
   ```

3. **Port conflicts**: Adjust port mappings in environment variables or docker-compose files.

## Contributing

When adding new services or modifying the Docker strategy:

1. Follow the layered approach (build → deploy/dev → compose)
2. Use the common builder base for Rust components
3. Implement comprehensive health checks
4. Include resource limits for production deployments
5. Update this README with any changes

This Docker strategy provides a robust, scalable, and maintainable foundation for the Tasker Core ecosystem.
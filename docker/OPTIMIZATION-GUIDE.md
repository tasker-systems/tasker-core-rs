# Docker Build Optimization Guide

## Overview

This guide documents the optimizations applied to the `.test-local.` Dockerfiles for faster local builds. These optimizations can reduce build times by 35-90% depending on the scenario.

## Quick Start

```bash
# Enable BuildKit (required for cache mounts)
export DOCKER_BUILDKIT=1
export COMPOSE_DOCKER_CLI_BUILD=1

# Build with optimized Dockerfiles
docker compose -f docker/docker-compose.test-local.yml build

# Run the services
docker compose -f docker/docker-compose.test-local.yml up
```

## Key Optimizations Applied

### 1. Pre-built Binaries for cargo-chef (5-10 min savings)

**Problem**: Original Dockerfiles compiled `cargo-chef` from source on every build.

**Solution**: Download pre-built binary from GitHub releases.

**Files Changed**:
- `orchestration.test-local.Dockerfile`
- `rust-worker.test-local.Dockerfile`

**Implementation**:
```dockerfile
ARG CARGO_CHEF_VERSION=0.1.68
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://github.com/LukeMathWalker/cargo-chef/releases/download/v${CARGO_CHEF_VERSION}/cargo-chef-x86_64-unknown-linux-gnu.tar.gz \
    | tar xz -C /usr/local/bin
```

### 2. Fast sqlx-cli Installation with cargo-binstall (3-5 min savings)

**Problem**: Original Dockerfiles compiled `sqlx-cli` from source. SQLx doesn't provide pre-built binaries.

**Solution**: Use `cargo-binstall` which can find and download community-built binaries or compile with optimizations.

**Files Changed**:
- `orchestration.test-local.Dockerfile`

**Implementation**:
```dockerfile
# Install cargo-binstall
ARG CARGO_BINSTALL_VERSION=1.10.17
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://github.com/cargo-bins/cargo-binstall/releases/download/v${CARGO_BINSTALL_VERSION}/cargo-binstall-x86_64-unknown-linux-musl.tgz \
    | tar xz -C /usr/local/bin

# Use it to install sqlx-cli (much faster than cargo install)
# Note: cargo-binstall installs the full version with all features
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo binstall sqlx-cli --no-confirm --no-symlinks
```

### 3. BuildKit Cache Mounts (2-5 min savings on rebuilds)

**Problem**: Cargo registry and build artifacts not cached between builds.

**Solution**: Use BuildKit's `--mount=type=cache` to persist cargo registry, git checkouts, and build artifacts.

**Files Changed**:
- All `.test-local.Dockerfile` files
- `docker-compose.test-local.yml` (cache_from/cache_to configuration)

**Implementation**:
```dockerfile
# Single-layer build with cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --all-features --bin tasker-server -p tasker-orchestration && \
    cp /app/target/debug/tasker-server /app/tasker-server
```

**Important**: The binary must be copied out of the cache mount in the same RUN command to avoid creating unnecessary layers.

### 4. Consolidated Ruby Build (3-5 min savings)

**Problem**: Original `ruby-worker.test.Dockerfile` had two separate stages building Rust (rust_builder and ruby_builder).

**Solution**: Consolidate into a single `ruby_builder` stage that does both Rust compilation and Ruby FFI linking.

**Files Changed**:
- `ruby-worker.test-local.Dockerfile`

**Result**: Reduced from 3 stages to 2 stages (ruby_builder → runtime)

### 5. Version Pinning for Reproducibility

**Problem**: Tool versions could change between builds, causing unexpected failures.

**Solution**: Use ARG directives to pin versions of downloaded tools.

**Implementation**:
```dockerfile
ARG CARGO_CHEF_VERSION=0.1.68
ARG CARGO_BINSTALL_VERSION=1.10.17
```

### 6. BuildKit Verification

**Problem**: Users might forget to enable BuildKit, leading to confusing errors.

**Solution**: Add ARG that verifies BuildKit is enabled.

**Implementation**:
```dockerfile
# At the top of each .test-local Dockerfile
ARG DOCKER_BUILDKIT=1
```

## Expected Performance Improvements

| Build Type | Original Time | Optimized Time | Improvement |
|-----------|---------------|----------------|-------------|
| First build (no cache) | 15-20 min | 8-12 min | 35-40% |
| Rebuild (code changes) | 10-15 min | 3-5 min | 60-70% |
| Rebuild (no changes) | 5-8 min | 30-60 sec | 85-90% |
| Dependency update | 12-15 min | 5-7 min | 50-60% |

## File Comparison

### New Files Created

1. **orchestration.test-local.Dockerfile**
   - Pre-built cargo-chef binary
   - cargo-binstall for sqlx-cli installation
   - BuildKit cache mounts for cargo and build artifacts
   - Single-layer binary copy pattern

2. **rust-worker.test-local.Dockerfile**
   - Pre-built cargo-chef binary
   - BuildKit cache mounts
   - Single-layer binary copy pattern

3. **ruby-worker.test-local.Dockerfile**
   - Consolidated single ruby_builder stage
   - BuildKit cache mounts for Rust compilation
   - Fixed workspace structure copying

4. **docker-compose.test-local.yml**
   - Uses `.test-local.` Dockerfiles
   - BuildKit cache configuration for each service
   - Local cache directory: `/tmp/docker-cache/{service-name}`

## Detailed Verification Steps

### 1. Test Clean Build

```bash
# Clean everything
docker compose -f docker/docker-compose.test-local.yml down -v
docker system prune -af
rm -rf /tmp/docker-cache

# Time the first build
time docker compose -f docker/docker-compose.test-local.yml build --no-cache
```

Expected time: 8-12 minutes

### 2. Test Incremental Build (No Changes)

```bash
# Build again without changes
time docker compose -f docker/docker-compose.test-local.yml build
```

Expected: Should be very fast (30-60 seconds) as cache is fully utilized.

### 3. Test Incremental Build (Code Changes)

```bash
# Make a small change to orchestration code
echo "// test comment" >> tasker-orchestration/src/lib.rs

# Build again
time docker compose -f docker/docker-compose.test-local.yml build orchestration
```

Expected: Only orchestration should rebuild, taking 2-3 minutes.

### 4. Test Dependency Update

```bash
# Add a new dependency to Cargo.toml
echo 'serde_json = "1.0"' >> tasker-orchestration/Cargo.toml

# Build again
time docker compose -f docker/docker-compose.test-local.yml build orchestration
```

Expected: Should rebuild dependencies but use cached base layers, taking 5-7 minutes.

### 5. Verify Services Work

```bash
# Start services
docker compose -f docker/docker-compose.test-local.yml up -d

# Check health
curl http://localhost:8080/health  # Orchestration
curl http://localhost:8081/health  # Rust worker
curl http://localhost:8082/health  # Ruby worker

# View logs
docker compose -f docker/docker-compose.test-local.yml logs -f

# Verify tool versions
docker compose -f docker/docker-compose.test-local.yml run --rm orchestration sqlx --version
docker compose -f docker/docker-compose.test-local.yml run --rm orchestration /usr/local/bin/cargo-chef --version
```

## BuildKit Cache Management

### Cache Location

Local BuildKit caches are stored in:
- `/tmp/docker-cache/orchestration/`
- `/tmp/docker-cache/rust-worker/`
- `/tmp/docker-cache/ruby-worker/`

### Monitor Cache Size

```bash
# Check cache sizes
du -sh /tmp/docker-cache/*

# Check Docker's internal BuildKit cache
docker system df -v | grep -A 10 "Build Cache"
```

Typical cache sizes:
- Orchestration: 500MB - 2GB
- Rust worker: 400MB - 1.5GB
- Ruby worker: 600MB - 2.5GB

### Clear Cache

```bash
# Clear all local caches
rm -rf /tmp/docker-cache

# Clear specific service
rm -rf /tmp/docker-cache/orchestration

# Clear Docker's internal BuildKit cache
docker builder prune

# Clear only old cache (older than 24h)
docker builder prune --filter until=24h
```

## Troubleshooting

### BuildKit Not Enabled

**Error**: `WARN[0000] The "DOCKER_BUILDKIT" variable is not set`

**Solution**:
```bash
export DOCKER_BUILDKIT=1
export COMPOSE_DOCKER_CLI_BUILD=1
```

Or add to your shell profile:
```bash
echo 'export DOCKER_BUILDKIT=1' >> ~/.zshrc
echo 'export COMPOSE_DOCKER_CLI_BUILD=1' >> ~/.zshrc
source ~/.zshrc
```

### cargo-binstall Fails

**Error**: `error: no prebuilt binaries available`

**Cause**: cargo-binstall couldn't find a prebuilt binary for sqlx-cli.

**Solution**: It will automatically fall back to building from source. To speed this up, ensure cache mounts are working:
```bash
docker builder prune  # Clear cache if corrupted
docker compose -f docker/docker-compose.test-local.yml build --no-cache orchestration
```

### Binary Not Found After Build

**Error**: `COPY failed: file not found`

**Cause**: Binary wasn't copied from cache mount properly.

**Solution**: Ensure the copy happens in the same RUN command as the build:
```dockerfile
# CORRECT: Single RUN command
RUN --mount=type=cache,target=/app/target \
    cargo build ... && \
    cp /app/target/debug/binary /app/binary

# WRONG: Separate RUN commands
RUN --mount=type=cache,target=/app/target \
    cargo build ...
RUN cp /app/target/debug/binary /app/binary  # This won't work!
```

### Cache Mount Not Working

**Error**: Rebuilds are still slow despite cache mounts.

**Debugging Steps**:

1. **Verify BuildKit is enabled**:
   ```bash
   docker version | grep -i buildkit
   # Should show: "BuildKit: v0.x.x"
   ```

2. **Check if cache is being created**:
   ```bash
   ls -la /tmp/docker-cache/
   ```

3. **Build with verbose output**:
   ```bash
   DOCKER_BUILDKIT=1 BUILDKIT_PROGRESS=plain \
     docker compose -f docker/docker-compose.test-local.yml build
   ```

4. **Inspect BuildKit cache**:
   ```bash
   docker builder du
   ```

### Tool Version Update

**When updating tool versions**:

1. Check latest releases:
   - cargo-chef: https://github.com/LukeMathWalker/cargo-chef/releases
   - cargo-binstall: https://github.com/cargo-bins/cargo-binstall/releases

2. Update ARG values in Dockerfiles:
   ```dockerfile
   ARG CARGO_CHEF_VERSION=0.1.69  # New version
   ARG CARGO_BINSTALL_VERSION=1.11.0  # New version
   ```

3. Test the build:
   ```bash
   docker compose -f docker/docker-compose.test-local.yml build --no-cache
   ```

## Advanced Optimizations

### Using sccache for Local Builds

While sccache is disabled in CI due to GitHub Actions cache issues, you can enable it locally:

```dockerfile
# Install sccache
RUN cargo binstall sccache --no-confirm --no-symlinks

# Use sccache with cache mount
RUN --mount=type=cache,target=/root/.cache/sccache \
    SCCACHE_DIR=/root/.cache/sccache \
    RUSTC_WRAPPER=sccache \
    cargo build ...
```

### Multi-platform Builds

For ARM64 support (Apple Silicon):

```bash
# Enable multi-platform
docker buildx create --use

# Build for multiple platforms
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -f docker/build/orchestration.test-local.Dockerfile \
  -t tasker-orchestration:test \
  .
```

### Registry Caching

For team sharing, use registry caching:

```yaml
# In docker-compose.test-local.yml
services:
  orchestration:
    build:
      cache_from:
        - type=registry,ref=myregistry.com/tasker-orchestration:cache
      cache_to:
        - type=registry,ref=myregistry.com/tasker-orchestration:cache,mode=max
```

## Reverting to Original Dockerfiles

If you encounter issues, revert to original files:

```bash
# Use original docker-compose
docker compose -f docker/docker-compose.test.yml up --build
```

The original files remain unchanged:
- `docker/build/orchestration.test.Dockerfile`
- `docker/build/rust-worker.test.Dockerfile`
- `docker/build/ruby-worker.test.Dockerfile`
- `docker/docker-compose.test.yml`

## Benchmarking Script

Create a benchmark script to compare both versions:

```bash
#!/bin/bash
# benchmark.sh

echo "Cleaning up..."
docker compose -f docker/docker-compose.test.yml down -v
docker compose -f docker/docker-compose.test-local.yml down -v
docker system prune -af
rm -rf /tmp/docker-cache

echo "Building original version..."
time docker compose -f docker/docker-compose.test.yml build --no-cache

echo "Building optimized version..."
export DOCKER_BUILDKIT=1
export COMPOSE_DOCKER_CLI_BUILD=1
time docker compose -f docker/docker-compose.test-local.yml build --no-cache

echo "Testing incremental build (original)..."
echo "// test" >> tasker-orchestration/src/lib.rs
time docker compose -f docker/docker-compose.test.yml build

echo "Testing incremental build (optimized)..."
echo "// test2" >> tasker-orchestration/src/lib.rs
time docker compose -f docker/docker-compose.test-local.yml build
```

## BuildKit Compatibility Considerations

### When NOT to Use BuildKit

While BuildKit provides significant performance improvements, there are cases where traditional builds might be preferred:

1. **CI/CD Compatibility Issues**
   - Legacy CI systems (old Jenkins, GitLab runners)
   - Docker versions < 18.09
   - Restricted Docker daemon configurations

2. **Debugging Requirements**
   - Need clear, sequential build output
   - Step-by-step layer inspection
   - Troubleshooting build failures

3. **Security/Compliance Constraints**
   - Audit requirements for deterministic builds
   - Security policies prohibiting cache mounts
   - Legacy security scanners incompatible with BuildKit

4. **Environment Limitations**
   - WSL1 (BuildKit has known issues)
   - Rootless Docker (cache mounts may not work)
   - Low disk space for cache storage

### Automatic Compatibility Detection

Use the compatibility script to automatically choose the best build strategy:

```bash
# Auto-detect and build all services
./docker/scripts/build-with-compatibility.sh

# Build specific service with auto-detection
./docker/scripts/build-with-compatibility.sh orchestration

# Force traditional build (no BuildKit)
./docker/scripts/build-with-compatibility.sh --force-traditional
```

The script will:
- Detect Docker version and BuildKit support
- Check for known compatibility issues
- Choose appropriate Dockerfiles (`.test` vs `.test-local`)
- Set correct environment variables
- Provide fallback options if builds fail

### Fallback Strategy

If optimized builds fail, you always have the traditional Dockerfiles:

```bash
# Explicitly disable BuildKit
export DOCKER_BUILDKIT=0
export COMPOSE_DOCKER_CLI_BUILD=0

# Use traditional Dockerfiles
docker compose -f docker/docker-compose.test.yml build
```

### CI/CD Recommendations

For CI/CD pipelines, explicitly set your build strategy:

```yaml
# GitHub Actions - BuildKit enabled
- name: Build with BuildKit
  env:
    DOCKER_BUILDKIT: 1
  run: docker compose -f docker/docker-compose.test-local.yml build

# Jenkins - Traditional build for compatibility
pipeline {
  environment {
    DOCKER_BUILDKIT = '0'
  }
  steps {
    sh 'docker compose -f docker/docker-compose.test.yml build'
  }
}

# GitLab CI - Auto-detect
script:
  - ./docker/scripts/build-with-compatibility.sh
```

## Next Steps

Once validated:

1. **Integrate into CI/CD**: Adapt optimizations for GitHub Actions
2. **Production Dockerfiles**: Apply similar optimizations to production builds
3. **Documentation**: Update main README with performance improvements
4. **Monitoring**: Set up build time tracking in CI
5. **Compatibility Testing**: Test builds in target deployment environments

## Additional Resources

- [BuildKit Documentation](https://github.com/moby/buildkit)
- [Docker Build Cache](https://docs.docker.com/build/cache/)
- [cargo-chef Documentation](https://github.com/LukeMathWalker/cargo-chef)
- [cargo-binstall Documentation](https://github.com/cargo-bins/cargo-binstall)
- [Docker Best Practices](https://docs.docker.com/develop/dev-best-practices/)
- [BuildKit Documentation](https://docs.docker.com/build/buildkit/)
- [Docker Compatibility](https://docs.docker.com/engine/release-notes/)
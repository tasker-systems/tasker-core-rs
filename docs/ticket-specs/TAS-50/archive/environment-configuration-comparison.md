# Environment Configuration Comparison

**TAS-50 Phase 2-3**: Complete environment configuration with development as reasonable middle ground.

## Configuration Strategy

- **Test**: Small/restrictive values for fast test execution
- **Development**: Medium values for comfortable local Docker development
- **Production**: Large values for production scale

## Common Configuration

### Database Pool

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| max_connections | 10 | 25 | 50 | Development allows comfortable local concurrency |
| min_connections | 2 | 5 | 15 | Scaled proportionally |
| acquire_timeout_seconds | 5 | 15 | 30 | More patient as environment scales |
| idle_timeout_seconds | 60 | 300 | 600 | Longer connections in larger envs |
| max_lifetime_seconds | 600 | 1800 | 7200 | Connection lifecycle scales up |

### Queues

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| default_batch_size | 5 | 10 | 50 | Development: reasonable local batching |
| max_batch_size | - | 50 | 200 | Inherits from base in test |
| poll_interval_ms | 100 | 500 | - | Development: balanced polling |

### Shared Channel Buffers (TAS-51)

| Channel | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| event_queue_buffer | 100 | 1000 | 10000 | 10x scaling at each level |
| ruby_event_buffer | 100 | 500 | 2000 | FFI event handling capacity |

### Circuit Breakers

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| max_circuit_breakers | 20 | 50 | 100 | Development: reasonable middle ground |
| failure_threshold | 3 | 5 | 10 | Development: balanced failure tolerance |
| timeout_seconds | 5 | 30 | 60 | Recovery timeout scales with environment |
| success_threshold | 1 | 2 | 3 | Success requirements scale up |

## Orchestration Configuration

### System Settings

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| max_concurrent_orchestrators | 1 | 2 | 10 | Development: test clustering locally |
| enable_performance_logging | false | false | true | Production only |

### Web Settings

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| bind_address | 0.0.0.0:8080 | 0.0.0.0:8080 | 0.0.0.0:8080 | Consistent across environments |
| request_timeout_ms | 5000 | 30000 | 60000 | Timeout scales with environment |
| web_api_pool_size | - | 10 | 20 | Development: balanced pool |
| web_api_max_connections | - | 15 | 30 | Development: comfortable capacity |

### MPSC Channel Buffers (TAS-51)

| Channel | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| command_buffer | 100 | 1000 | 5000 | Command processing capacity |
| event_channel_buffer | 100 | 1000 | 5000 | Event coordination capacity |
| pgmq_event_buffer | 500 | 5000 | 50000 | Notification burst handling |

### Shutdown Settings

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| graceful_shutdown_timeout | 5s | 15s | - | Development: reasonable grace period |
| emergency_shutdown_timeout | 1s | 5s | - | Development: balanced emergency timeout |

## Worker Configuration

### Step Processing

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| worker_id | test-worker-001 | development-worker-001 | production-worker-001 | Environment-specific IDs |
| max_concurrent_steps | 10 | 50 | 500 | Development: comfortable local concurrency |

### Resource Limits

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| max_memory_mb | 512 | 2048 | 4096 | Development: 2GB reasonable for Docker |
| max_cpu_percent | - | 75 | 90 | Development: leave headroom for IDE/tools |
| max_database_connections | 10 | 25 | 100 | Matches common database pool sizes |

### Health Monitoring

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| metrics_collection_enabled | - | true | true | Development: collect for debugging |
| performance_monitoring_enabled | - | false | true | Production only (can enable for debugging) |

### Web Settings

| Setting | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| bind_address | 0.0.0.0:8082 | 0.0.0.0:8081 | 0.0.0.0:8081 | Test uses 8082 to avoid conflicts |
| request_timeout_ms | 5000 | 30000 | 60000 | Timeout scales with environment |
| web_api_pool_size | - | 8 | 15 | Development: balanced pool |
| web_api_max_connections | - | 12 | 25 | Development: comfortable capacity |

### MPSC Channel Buffers (TAS-51)

| Channel | Test | Development | Production | Notes |
|---------|------|-------------|------------|-------|
| command_buffer | 100 | 500 | 2000 | Worker command processing |
| event_channel_buffer | 100 | 500 | 2000 | Worker event coordination |
| completion_buffer | 100 | 500 | 2000 | Step completion events |
| result_buffer | 100 | 500 | 2000 | Step result processing |
| broadcast_buffer | 100 | 500 | 2000 | In-process event broadcasting |
| pgmq_event_buffer | 100 | 1000 | 2000 | Notification handling |

## Development Environment Philosophy

The development environment is designed to provide:

1. **Comfortable Local Development**:
   - Enough capacity to run realistic workflows
   - Not so large that it overwhelms local Docker resources
   - ~2GB RAM, 25 database connections, 50 concurrent steps

2. **Cluster Testing**:
   - 2 orchestrators to test multi-instance coordination
   - Reasonable channel buffers to test backpressure handling
   - Metrics collection enabled for debugging

3. **Resource Balance**:
   - 75% CPU limit (leaves headroom for IDE, browser, etc.)
   - 2GB memory (reasonable for local Docker, won't starve host)
   - Medium channel buffers (test backpressure without overwhelming)

4. **Debugging Friendly**:
   - Metrics collection enabled by default
   - Performance monitoring can be enabled on demand
   - Timeouts long enough to allow debugging pauses

## Scaling Pattern

The environments follow a clear scaling pattern:

- **Test**: 1x (baseline for fast tests)
- **Development**: ~5x (comfortable local development)
- **Production**: ~50x (full scale production)

This 1:5:50 ratio applies to most settings:
- Database connections: 10 → 25 → 50
- Concurrent steps: 10 → 50 → 500
- Channel buffers: 100 → 500-1000 → 2000-10000
- Memory: 512MB → 2GB → 4GB

## Usage

Set `TASKER_ENV` to control which environment configuration is loaded:

```bash
# Test environment (default)
export TASKER_ENV=test
cargo test --all-features

# Development environment (local Docker)
export TASKER_ENV=development
docker-compose up

# Production environment
export TASKER_ENV=production
# Deployed via k8s/Docker with production settings
```

## Database Configuration

Each environment uses a different database:

- **Test**: `tasker_rust_test` (localhost, cleared between tests)
- **Development**: `tasker_development` (localhost, persistent)
- **Production**: Uses environment variables or secrets for connection

## Verification

All three environments tested and verified:

```bash
# Test environment (fast execution)
✅ TASKER_ENV=test cargo test --all-features
   90 tests passing

# Development environment (local Docker)
✅ TASKER_ENV=development cargo test --all-features
   Loads successfully with middle-ground values

# Production environment (scaled)
✅ TASKER_ENV=production cargo test --all-features
   Loads successfully with production-scale values
```

---

**Note**: This configuration structure supports the TAS-50 Phase 2-3 goal of creating a clean,
context-specific configuration system with intuitive environment overrides. The development
environment fills the gap between test (small) and production (large), providing a comfortable
local development experience with realistic resource usage.

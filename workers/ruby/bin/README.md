# Ruby Worker Binary Scripts

This directory contains executable scripts for the Ruby worker service.

## Scripts

### `server.rb`
Production-ready server script that bootstraps the Rust foundation via FFI and manages Ruby handler execution.

**Usage:**
```bash
bundle exec ruby bin/server.rb
```

**Features:**
- Bootstraps Rust worker foundation via FFI
- Manages Ruby handler registration and execution
- Production-grade signal handling (INT, TERM, USR1, USR2)
- Health check monitoring
- Graceful shutdown support

**Environment Variables:**
- `TASKER_ENV` - Environment (development, test, production)
- `DATABASE_URL` - PostgreSQL connection string
- `TASKER_TEMPLATE_PATH` - Path to task template YAML files
- `RUST_LOG` - Logging level (debug, info, warn, error)

### `health_check.rb`
Simple health check script for Docker HEALTHCHECK or Kubernetes liveness/readiness probes.

**Usage:**
```bash
ruby bin/health_check.rb
```

**Environment Variables:**
- `HEALTH_CHECK_ENDPOINT` - Health endpoint URL (default: http://localhost:8081/health)
- `HEALTH_CHECK_TIMEOUT` - Timeout in seconds (default: 5)

**Exit Codes:**
- `0` - Service is healthy
- `1` - Service is unhealthy or unreachable

### `console`
Interactive console for debugging and testing Ruby worker components.

**Usage:**
```bash
./bin/console
```

**Features:**
- Loads TaskerCore environment
- Provides access to all worker modules
- Uses Pry if available, otherwise IRB
- Pre-configured with helpful examples

## Docker Integration

The Docker entrypoint script (`docker/scripts/ruby-worker-entrypoint.sh`) uses `server.rb`:

```bash
# In the entrypoint script
exec bundle exec ruby bin/server.rb
```

The health check can be configured in the Dockerfile:

```dockerfile
HEALTHCHECK --interval=15s --timeout=10s --start-period=30s --retries=3 \
    CMD ruby /app/ruby_worker/bin/health_check.rb || exit 1
```

## Signal Handling

The server.rb script responds to the following signals:

- **INT (SIGINT)** - Graceful shutdown (Ctrl+C)
- **TERM (SIGTERM)** - Graceful shutdown (Docker/K8s termination)
- **USR1 (SIGUSR1)** - Report worker status
- **USR2 (SIGUSR2)** - Reload configuration (if supported)

## Development

When developing new features, use the console to test components interactively:

```bash
./bin/console

# In the console:
registry = TaskerCore::Registry::HandlerRegistry.instance
registry.registered_handlers

# Test FFI
TaskerCore::FFI.worker_status
```

# Library Deployment Patterns (TAS-77)

This document describes the library deployment patterns feature that enables applications to consume worker observability data (health, metrics, templates, configuration) either via the HTTP API or directly through FFI, without running a web server.

## Overview

Prior to TAS-77, applications needed to run the worker's HTTP server to access observability data. This created deployment overhead for applications that only needed programmatic access to health checks, metrics, or template information.

The library deployment patterns feature:
1. **Extracts observability logic into reusable services** - Business logic moved from HTTP handlers to service classes
2. **Exposes services via FFI** - Same functionality available without HTTP overhead
3. **Provides Ruby wrapper layer** - Type-safe Ruby interface with dry-struct types
4. **Makes HTTP server optional** - Services always available, web server is opt-in

## Architecture

### Service Layer

Four services encapsulate observability logic:

```
tasker-worker/src/worker/services/
├── health/          # HealthService - health checks
├── metrics/         # MetricsService - metrics collection
├── template_query/  # TemplateQueryService - template operations
└── config_query/    # ConfigQueryService - configuration queries
```

Each service:
- Contains all business logic previously in HTTP handlers
- Is independent of HTTP transport
- Can be accessed via web handlers OR FFI
- Returns typed response structures

### Service Access Patterns

```
                    ┌─────────────────────────────────────────┐
                    │            WorkerWebState               │
                    │  ┌────────────────────────────────────┐ │
                    │  │         Service Instances           │ │
                    │  │  ┌────────────┐ ┌────────────────┐ │ │
                    │  │  │HealthServ.│ │MetricsService  │ │ │
                    │  │  └────────────┘ └────────────────┘ │ │
                    │  │  ┌────────────┐ ┌────────────────┐ │ │
                    │  │  │TemplQuery │ │ConfigQuery     │ │ │
                    │  │  └────────────┘ └────────────────┘ │ │
                    │  └────────────────────────────────────┘ │
                    └──────────────┬───────────────┬──────────┘
                                   │               │
           ┌───────────────────────┴───┐     ┌─────┴──────────────────────┐
           │     HTTP Handlers         │     │     FFI Layer              │
           │  (web/handlers/*.rs)      │     │  (observability_ffi.rs)    │
           └───────────────────────────┘     └────────────────────────────┘
                       │                                 │
                       ▼                                 ▼
               ┌───────────────┐                ┌───────────────┐
               │  HTTP Clients │                │  Ruby/Python  │
               │  curl, etc.   │                │  Applications │
               └───────────────┘                └───────────────┘
```

## Usage

### Ruby FFI Access

The `TaskerCore::Observability` module provides type-safe access to all services:

```ruby
# Health checks
health = TaskerCore::Observability.health_basic
puts health.status        # => "healthy"
puts health.worker_id     # => "worker-abc123"

# Kubernetes-style probes
if TaskerCore::Observability.ready?
  puts "Worker ready to receive requests"
end

if TaskerCore::Observability.alive?
  puts "Worker is alive"
end

# Detailed health information
detailed = TaskerCore::Observability.health_detailed
detailed.checks.each do |name, check|
  puts "#{name}: #{check.status} (#{check.duration_ms}ms)"
end
```

### Metrics Access

```ruby
# Domain event statistics
events = TaskerCore::Observability.event_stats
puts "Events routed: #{events.router.total_routed}"
puts "FFI dispatches: #{events.in_process_bus.ffi_channel_dispatches}"

# Prometheus format (for custom scrapers)
prometheus_text = TaskerCore::Observability.prometheus_metrics
```

### Template Operations

```ruby
# List templates (JSON string)
templates_json = TaskerCore::Observability.templates_list

# Validate a template
validation = TaskerCore::Observability.template_validate(
  namespace: "payments",
  name: "process_payment",
  version: "v1"
)

if validation.valid
  puts "Template valid with #{validation.handler_count} handlers"
else
  validation.issues.each { |issue| puts "Issue: #{issue}" }
end

# Cache management
stats = TaskerCore::Observability.cache_stats
puts "Cache hits: #{stats.hits}, misses: #{stats.misses}"

TaskerCore::Observability.cache_clear  # Clear all cached templates
```

### Configuration Access

```ruby
# Get runtime configuration (secrets redacted)
config = TaskerCore::Observability.config
puts "Environment: #{config.environment}"
puts "Redacted fields: #{config.metadata.redacted_fields.join(', ')}"

# Quick environment check
env = TaskerCore::Observability.environment
puts "Running in: #{env}"  # => "production"
```

## Configuration

### HTTP Server Toggle

The HTTP server is now optional. Services are always created, but the HTTP server only starts if enabled:

```toml
# config/tasker/base/worker.toml
[worker.web]
enabled = true              # Set to false to disable HTTP server
bind_address = "0.0.0.0:8081"
request_timeout_ms = 30000
```

When `enabled = false`:
- WorkerWebState is still created (services available)
- HTTP server does NOT start
- All services accessible via FFI only
- Reduces resource usage (no HTTP listener, no connections)

### Deployment Modes

| Mode | HTTP Server | FFI Services | Use Case |
|------|------------|--------------|----------|
| Full | ✅ | ✅ | Standard deployment with monitoring |
| Library | ❌ | ✅ | Embedded in application, no external access |
| Headless | ❌ | ✅ | Container with external health checks disabled |

## Type Definitions

The Ruby wrapper uses dry-struct types for structured access:

### Health Types

```ruby
TaskerCore::Observability::Types::BasicHealth
  - status: String
  - worker_id: String
  - timestamp: String

TaskerCore::Observability::Types::DetailedHealth
  - status: String
  - timestamp: String
  - worker_id: String
  - checks: Hash[String, HealthCheck]
  - system_info: WorkerSystemInfo

TaskerCore::Observability::Types::HealthCheck
  - status: String
  - message: String?
  - duration_ms: Integer
  - last_checked: String
```

### Metrics Types

```ruby
TaskerCore::Observability::Types::DomainEventStats
  - router: EventRouterStats
  - in_process_bus: InProcessEventBusStats
  - captured_at: String
  - worker_id: String

TaskerCore::Observability::Types::EventRouterStats
  - total_routed: Integer
  - durable_routed: Integer
  - fast_routed: Integer
  - broadcast_routed: Integer
  - fast_delivery_errors: Integer
  - routing_errors: Integer
```

### Template Types

```ruby
TaskerCore::Observability::Types::CacheStats
  - total_entries: Integer
  - hits: Integer
  - misses: Integer
  - evictions: Integer
  - last_maintenance: String?

TaskerCore::Observability::Types::TemplateValidation
  - valid: Boolean
  - namespace: String
  - name: String
  - version: String
  - handler_count: Integer
  - issues: Array[String]
  - handler_metadata: Hash?
```

### Config Types

```ruby
TaskerCore::Observability::Types::RuntimeConfig
  - environment: String
  - common: Hash
  - worker: Hash
  - metadata: ConfigMetadata

TaskerCore::Observability::Types::ConfigMetadata
  - timestamp: String
  - source: String
  - redacted_fields: Array[String]
```

## Error Handling

FFI methods raise `RuntimeError` on failures:

```ruby
begin
  health = TaskerCore::Observability.health_basic
rescue RuntimeError => e
  if e.message.include?("Worker system not running")
    # Worker not bootstrapped yet
  elsif e.message.include?("Web state not available")
    # Services not initialized
  end
end
```

### Template Operation Errors

Template operations raise `RuntimeError` for missing templates or namespaces:

```ruby
begin
  result = TaskerCore::Observability.template_get(
    namespace: "unknown",
    name: "missing",
    version: "1.0.0"
  )
rescue RuntimeError => e
  puts "Template not found: #{e.message}"
end

# template_refresh handles errors gracefully, returning a result struct
result = TaskerCore::Observability.template_refresh(
  namespace: "unknown",
  name: "missing",
  version: "1.0.0"
)
puts result.success  # => false
puts result.message  # => error description
```

### Convenience Methods

The `ready?` and `alive?` methods handle errors gracefully:

```ruby
# These never raise - they return false on any error
TaskerCore::Observability.ready?  # => true/false
TaskerCore::Observability.alive?  # => true/false
```

**Note:** `alive?` checks for `status == "alive"` (from liveness probe), while `ready?` checks for `status == "healthy"` (from readiness probe).

## Best Practices

1. **Use type-safe methods when possible** - Methods returning dry-struct types provide better validation
2. **Handle errors gracefully** - FFI can fail if worker not bootstrapped
3. **Consider caching** - For high-frequency health checks, cache results briefly
4. **Use ready?/alive? helpers** - They handle exceptions and return boolean
5. **Prefer FFI for internal use** - Less overhead than HTTP for same-process access

## Migration Guide

### From HTTP to FFI

Before (HTTP):
```ruby
response = Faraday.get("http://localhost:8081/health")
health = JSON.parse(response.body)
```

After (FFI):
```ruby
health = TaskerCore::Observability.health_basic
```

### Disabling HTTP Server

1. Update configuration:
   ```toml
   [worker.web]
   enabled = false
   ```

2. Update health check scripts to use FFI:
   ```ruby
   # health_check.rb
   require 'tasker_core'

   exit(TaskerCore::Observability.ready? ? 0 : 1)
   ```

3. Update monitoring to scrape via FFI:
   ```ruby
   metrics = TaskerCore::Observability.prometheus_metrics
   # Send to Prometheus pushgateway or custom aggregator
   ```

## API Reference

### Health Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `health_basic` | `Types::BasicHealth` | Basic health status |
| `health_live` | `Types::BasicHealth` | Liveness probe (status: "alive") |
| `health_ready` | `Types::DetailedHealth` | Readiness probe with all checks |
| `health_detailed` | `Types::DetailedHealth` | Full health information |
| `ready?` | `Boolean` | True if status == "healthy" |
| `alive?` | `Boolean` | True if status == "alive" |

### Metrics Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `metrics_worker` | `String` (JSON) | Worker metrics as JSON |
| `event_stats` | `Types::DomainEventStats` | Domain event statistics |
| `prometheus_metrics` | `String` | Prometheus text format |

### Template Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `templates_list(include_cache_stats: false)` | `String` (JSON) | List all templates |
| `template_get(namespace:, name:, version:)` | `String` (JSON) | Get specific template (raises on error) |
| `template_validate(namespace:, name:, version:)` | `Types::TemplateValidation` | Validate template (raises on error) |
| `cache_stats` | `Types::CacheStats` | Cache statistics |
| `cache_clear` | `Types::CacheOperationResult` | Clear template cache |
| `template_refresh(namespace:, name:, version:)` | `Types::CacheOperationResult` | Refresh specific template |

### Config Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `config` | `Types::RuntimeConfig` | Full config (secrets redacted) |
| `environment` | `String` | Current environment name |

## Related Documentation

- [Configuration Management](./configuration-management.md) - Full configuration reference
- [Deployment Patterns](./deployment-patterns.md) - General deployment options
- [Observability](./observability/README.md) - Metrics and monitoring
- [FFI Telemetry Pattern](./ffi-telemetry-pattern.md) - FFI logging integration

# Configuration Management

**Last Updated**: 2025-10-17
**Audience**: Operators, Developers, Architects
**Status**: Active
**Related Docs**: [Environment Configuration Comparison](environment-configuration-comparison.md), [Deployment Patterns](deployment-patterns.md)

← Back to [Documentation Hub](README.md)

---

## Overview

Tasker Core implements a sophisticated **component-based configuration system** with environment-specific overrides, runtime observability, and comprehensive validation. This document explains how to manage, validate, inspect, and deploy Tasker configurations.

### Key Features

| Feature | Description | Benefit |
|---------|-------------|---------|
| **Component-Based Architecture** | 3 focused TOML files organized by common, orchestration, and worker | Easy to understand and maintain |
| **Environment Overrides** | Test, development, production-specific settings | Safe defaults with production scale-out |
| **Runtime Observability** | `/config` API endpoints with secret redaction | Live inspection of deployed configurations |
| **CLI Tools** | Generate and validate single deployable configs | Build-time verification, deployment artifacts |
| **Context-Specific Validation** | Orchestration and worker-specific validation rules | Catch errors before deployment |
| **Secret Redaction** | 12+ sensitive key patterns automatically hidden | Safe configuration inspection |

---

## Quick Start

### Inspect Running System Configuration

```bash
# Check orchestration configuration (includes common + orchestration-specific)
curl http://localhost:8080/config | jq

# Check worker configuration (includes common + worker-specific)
curl http://localhost:8081/config | jq

# Secrets are automatically redacted for safety
```

### Generate Deployable Configuration

```bash
# Generate production orchestration config
tasker-cli config generate \
    --context orchestration \
    --environment production

# Output: config/tasker/generated/orchestration-production.toml
```

### Validate Configuration

```bash
# Validate orchestration config for production
tasker-cli config validate \
    --context orchestration \
    --environment production

# Validates: type safety, ranges, required fields, business rules
```

---

## Part 1: Configuration Architecture

### 1.1 Component-Based Structure

Tasker uses a **component-based TOML architecture** where configuration is split into focused files with single responsibility:

```
config/tasker/
├── base/                           # Base configuration (defaults)
│   ├── common.toml                 # Shared: database, circuit breakers, telemetry
│   ├── orchestration.toml          # Orchestration-specific settings
│   └── worker.toml                 # Worker-specific settings
│
├── environments/                   # Environment-specific overrides
│   ├── test/
│   │   ├── common.toml             # Test overrides (small values, fast execution)
│   │   ├── orchestration.toml
│   │   └── worker.toml
│   │
│   ├── development/
│   │   ├── common.toml             # Development overrides (medium values, local Docker)
│   │   ├── orchestration.toml
│   │   └── worker.toml
│   │
│   └── production/
│       ├── common.toml             # Production overrides (large values, scale-out)
│       ├── orchestration.toml
│       └── worker.toml
│
└── generated/                      # Generated merged configs (deployment artifacts)
    ├── common-test.toml
    ├── orchestration-production.toml
    └── worker-production.toml
```

### 1.2 Configuration Contexts

Tasker has three configuration contexts:

| Context | Purpose | Components |
|---------|---------|------------|
| **Common** | Shared across orchestration and worker | Database, circuit breakers, telemetry, backoff, system |
| **Orchestration** | Orchestration-specific settings | Web API, MPSC channels, event systems, shutdown |
| **Worker** | Worker-specific settings | Handler discovery, resource limits, health monitoring |

### 1.3 Environment Detection

Configuration loading uses `TASKER_ENV` environment variable:

```bash
# Test environment (default) - small values for fast tests
export TASKER_ENV=test

# Development environment - medium values for local Docker
export TASKER_ENV=development

# Production environment - large values for scale-out
export TASKER_ENV=production
```

**Detection Order**:
1. `TASKER_ENV` environment variable
2. Default to "development" if not set

### 1.4 Merging Strategy

Configuration merging follows **environment overrides win** pattern:

```toml
# base/common.toml
[database.pool]
max_connections = 30
min_connections = 8

# environments/production/common.toml
[database.pool]
max_connections = 50

# Result: max_connections = 50, min_connections = 8 (inherited from base)
```

---

## Part 2: Runtime Observability (TAS-50 Phase 3)

### 2.1 Configuration API Endpoints

Tasker provides **unified configuration endpoints** that return complete configuration (common + context-specific) in a single response.

#### Orchestration API

**Endpoint**: `GET /config` (system endpoint at root level)

**Purpose**: Inspect complete orchestration configuration including common settings

**Example Request**:
```bash
curl http://localhost:8080/config | jq
```

**Response Structure**:
```json
{
  "environment": "production",
  "common": {
    "database": {
      "url": "***REDACTED***",
      "pool": {
        "max_connections": 50,
        "min_connections": 15
      }
    },
    "circuit_breakers": { "...": "..." },
    "telemetry": { "...": "..." },
    "system": { "...": "..." },
    "backoff": { "...": "..." },
    "task_templates": { "...": "..." }
  },
  "orchestration": {
    "web": {
      "bind_address": "0.0.0.0:8080",
      "request_timeout_ms": 60000
    },
    "mpsc_channels": {
      "command_buffer_size": 5000,
      "pgmq_notification_buffer_size": 50000
    },
    "event_systems": { "...": "..." }
  },
  "metadata": {
    "timestamp": "2025-10-17T15:30:45Z",
    "source": "runtime",
    "redacted_fields": [
      "database.url",
      "telemetry.api_key"
    ]
  }
}
```

#### Worker API

**Endpoint**: `GET /config` (system endpoint at root level)

**Purpose**: Inspect complete worker configuration including common settings

**Example Request**:
```bash
curl http://localhost:8081/config | jq
```

**Response Structure**:
```json
{
  "environment": "production",
  "common": {
    "database": { "...": "..." },
    "circuit_breakers": { "...": "..." },
    "telemetry": { "...": "..." }
  },
  "worker": {
    "template_path": "/app/templates",
    "max_concurrent_steps": 500,
    "resource_limits": {
      "max_memory_mb": 4096,
      "max_cpu_percent": 90
    },
    "web": {
      "bind_address": "0.0.0.0:8081",
      "request_timeout_ms": 60000
    }
  },
  "metadata": {
    "timestamp": "2025-10-17T15:30:45Z",
    "source": "runtime",
    "redacted_fields": [
      "database.url",
      "worker.auth_token"
    ]
  }
}
```

### 2.2 Design Philosophy

**Single Endpoint, Complete Configuration**: Each system has one `/config` endpoint that returns both common and context-specific configuration in a single response.

**Benefits**:
1. **Single curl command**: Get complete picture without correlation
2. **Easy comparison**: Compare orchestration vs worker configs for compatibility
3. **Tooling-friendly**: Automated tools can validate shared config matches
4. **Debugging-friendly**: No mental correlation between multiple endpoints
5. **System endpoint**: At root level like `/health`, `/metrics` (not under `/v1/`)

### 2.3 Comprehensive Secret Redaction

All sensitive configuration values are automatically redacted before returning to clients.

**Sensitive Key Patterns** (12+ patterns, case-insensitive):
- `password`, `secret`, `token`, `key`, `api_key`
- `private_key`, `jwt_private_key`, `jwt_public_key`
- `auth_token`, `credentials`, `database_url`, `url`

**Key Features**:
- **Recursive Processing**: Handles deeply nested objects and arrays
- **Field Path Tracking**: Reports which fields were redacted (e.g., `database.url`)
- **Smart Skipping**: Empty strings and booleans not redacted
- **Case-Insensitive**: Catches `API_KEY`, `Secret_Token`, `database_PASSWORD`
- **Structure Preservation**: Non-sensitive data remains intact

**Example**:
```json
{
  "database": {
    "url": "***REDACTED***",
    "adapter": "postgresql",
    "pool": {
      "max_connections": 30
    }
  },
  "metadata": {
    "redacted_fields": ["database.url"]
  }
}
```

### 2.4 OpenAPI/Swagger Integration

All configuration endpoints are documented with OpenAPI 3.0 and Swagger UI.

**Access Swagger UI**:
- Orchestration: http://localhost:8080/api-docs/ui
- Worker: http://localhost:8081/api-docs/ui

**OpenAPI Specification**:
- Orchestration: http://localhost:8080/api-docs/openapi.json
- Worker: http://localhost:8081/api-docs/openapi.json

---

## Part 3: CLI Tools (TAS-50 Phase 1)

### 3.1 Generate Command

**Purpose**: Generate a single merged configuration file from base + environment overrides for deployment.

**Command Signature**:
```bash
tasker-cli config generate \
    --context <common|orchestration|worker> \
    --environment <test|development|production>
```

**Examples**:
```bash
# Generate orchestration config for production
tasker-cli config generate --context orchestration --environment production

# Generate worker config for development
tasker-cli config generate --context worker --environment development

# Generate common config for test
tasker-cli config generate --context common --environment test
```

**Output Location**: Automatically generated at:
```
config/tasker/generated/{context}-{environment}.toml
```

**Key Features**:
1. **Automatic Paths**: No need for `--source-dir` or `--output` flags
2. **Metadata Headers**: Generated files include rich metadata:
   ```toml
   # Generated by Tasker Configuration System
   # Context: orchestration
   # Environment: production
   # Generated At: 2025-10-17T15:30:45Z
   # Base Config: config/tasker/base/orchestration.toml
   # Environment Override: config/tasker/environments/production/orchestration.toml
   #
   # This is a merged configuration file combining base settings with
   # environment-specific overrides. Environment values take precedence.
   ```
3. **Automatic Validation**: Validates during generation
4. **Smart Merging**: TOML-level merging preserves structure

### 3.2 Validate Command

**Purpose**: Validate configuration files with context-specific validation rules.

**Command Signature**:
```bash
tasker-cli config validate \
    --context <common|orchestration|worker> \
    --environment <test|development|production>
```

**Examples**:
```bash
# Validate orchestration config for production
tasker-cli config validate --context orchestration --environment production

# Validate worker config for test
tasker-cli config validate --context worker --environment test
```

**Validation Features**:
- Environment variable substitution (`${VAR:-default}`)
- Type checking (numeric ranges, boolean values)
- Required field validation
- Context-specific business rules
- Clear error messages

**Example Output**:
```
🔍 Validating configuration...
   Context: orchestration
   Environment: production
   ✓ Configuration loaded
   ✓ Validation passed

✅ Configuration is valid!

📊 Configuration Summary:
   Context: orchestration
   Environment: production
   Database: postgresql://tasker:***@localhost/tasker_production
   Web API: 0.0.0.0:8080
   MPSC Channels: 5 configured
```

### 3.3 Configuration Validator Binary

For quick validation without the full CLI:

```bash
# Validate all three environments
TASKER_ENV=test cargo run --bin config-validator
TASKER_ENV=development cargo run --bin config-validator
TASKER_ENV=production cargo run --bin config-validator
```

---

## Part 4: Environment-Specific Configurations

See **[Environment Configuration Comparison](environment-configuration-comparison.md)** for complete details on configuration values across environments.

### 4.1 Scaling Pattern

Tasker follows a **1:5:50 scaling pattern** across environments:

| Component | Test | Development | Production | Pattern |
|-----------|------|-------------|------------|---------|
| Database Connections | 10 | 25 | 50 | 1x → 2.5x → 5x |
| Concurrent Steps | 10 | 50 | 500 | 1x → 5x → 50x |
| MPSC Channel Buffers | 100-500 | 500-1000 | 2000-50000 | 1x → 5-10x → 20-100x |
| Memory Limits | 512MB | 2GB | 4GB | 1x → 4x → 8x |

### 4.2 Environment Philosophy

**Test Environment**:
- **Goal**: Fast execution, test isolation
- **Strategy**: Minimal resources, small buffers
- **Example**: 10 database connections, 100-500 MPSC buffers

**Development Environment**:
- **Goal**: Comfortable local Docker development
- **Strategy**: Medium values, realistic workflows
- **Example**: 25 database connections, 2GB RAM, 500-1000 MPSC buffers
- **Cluster Testing**: 2 orchestrators to test multi-instance coordination

**Production Environment**:
- **Goal**: High throughput, scale-out capacity
- **Strategy**: Large values, production resilience
- **Example**: 50 database connections, 4GB RAM, 2000-50000 MPSC buffers

---

## Part 5: Deployment Workflows

### 5.1 Docker Deployment

**Build-Time Configuration Generation**:
```dockerfile
FROM rust:1.75 as builder

# Build CLI tool
RUN cargo build --release --bin tasker-cli

# Generate production config
RUN ./target/release/tasker-cli config generate \
    --context orchestration \
    --environment production

FROM rust:1.75-slim

# Copy generated config
COPY --from=builder \
    config/tasker/generated/orchestration-production.toml \
    /app/config.toml

# Set environment
ENV TASKER_CONFIG_PATH=/app/config.toml
ENV TASKER_ENV=production

CMD ["./tasker-orchestration"]
```

### 5.2 Kubernetes Deployment

**ConfigMap Strategy**:
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: tasker-orchestration-config
data:
  orchestration.toml: |
    # Generated configuration
    [database.pool]
    max_connections = 50
    # ... rest of config
---
apiVersion: v1
kind: Deployment
metadata:
  name: tasker-orchestration
spec:
  template:
    spec:
      containers:
      - name: orchestration
        image: tasker/orchestration:latest
        env:
        - name: TASKER_ENV
          value: "production"
        - name: TASKER_CONFIG_PATH
          value: "/config/orchestration.toml"
        volumeMounts:
        - name: config
          mountPath: /config
      volumes:
      - name: config
        configMap:
          name: tasker-orchestration-config
```

### 5.3 Local Development

**Standard Workflow**:
```bash
# Set development environment
export TASKER_ENV=development

# Run with automatic config loading
cargo run --bin tasker-orchestration

# Or use Docker Compose
docker-compose up
```

**Configuration Loading**:
- Automatically loads from `config/tasker/base/` + `config/tasker/environments/development/`
- Merges at runtime
- No need to generate separately for local development

---

## Part 6: Configuration Validation

### 6.1 Context-Specific Validation

Each configuration context has specific validation rules:

**Common Configuration**:
- Database URL format and connectivity
- Pool size ranges (1-1000 connections)
- Circuit breaker thresholds (1-100 failures)
- Timeout durations (1-3600 seconds)

**Orchestration Configuration**:
- Web API bind address format
- Request timeout ranges (1000-300000 ms)
- MPSC channel buffer sizes (100-100000)
- Event system configuration consistency

**Worker Configuration**:
- Template path existence
- Resource limit ranges (memory, CPU %)
- Handler discovery path validation
- Concurrent step limits (1-10000)

### 6.2 Validation Workflow

**Pre-Deployment Validation**:
```bash
# Validate before generating deployment artifact
tasker-cli config validate --context orchestration --environment production

# Generate only if validation passes
tasker-cli config generate --context orchestration --environment production
```

**Runtime Validation**:
- Configuration validated on application startup
- Invalid config prevents startup (fail-fast)
- Clear error messages for troubleshooting

### 6.3 Common Validation Errors

**Example Error Messages**:
```
❌ Validation Error: database.pool.max_connections
   Value: 5000
   Issue: Exceeds maximum allowed value (1000)
   Fix: Reduce to 1000 or less

❌ Validation Error: web.bind_address
   Value: "invalid:port"
   Issue: Invalid IP:port format
   Fix: Use format like "0.0.0.0:8080" or "127.0.0.1:3000"
```

---

## Part 7: Operational Workflows

### 7.1 Compare Deployed Configurations

**Cross-System Comparison**:
```bash
# Get orchestration config
curl http://orchestration:8080/config > orch-config.json

# Get worker config
curl http://worker:8081/config > worker-config.json

# Compare common sections for compatibility
jq '.common' orch-config.json > orch-common.json
jq '.common' worker-config.json > worker-common.json

diff orch-common.json worker-common.json
```

**Why This Matters**:
- Ensures orchestration and worker share same database config
- Validates circuit breaker settings match
- Confirms telemetry endpoints aligned

### 7.2 Debug Configuration Issues

**Step 1: Inspect Runtime Config**
```bash
# Check what's actually deployed
curl http://localhost:8080/config | jq '.orchestration.web'
```

**Step 2: Compare to Expected**
```bash
# Check generated config file
cat config/tasker/generated/orchestration-production.toml

# Compare values
```

**Step 3: Trace Configuration Source**
```bash
# Check metadata for source files
curl http://localhost:8080/config | jq '.metadata'

# Metadata shows:
# - Environment (production)
# - Timestamp (when config was loaded)
# - Source (runtime)
# - Redacted fields (for transparency)
```

### 7.3 Configuration Drift Detection

**Manual Comparison**:
```bash
# Generate what should be deployed
tasker-cli config generate --context orchestration --environment production

# Compare to runtime
diff config/tasker/generated/orchestration-production.toml \
     <(curl -s http://localhost:8080/config | jq -r '.orchestration')
```

**Automated Monitoring** (future):
- Periodic config snapshots
- Alert on unexpected changes
- Configuration version tracking

---

## Part 8: Best Practices

### 8.1 Configuration Management

**DO**:
✅ Use environment variables for secrets (`${DATABASE_URL}`)
✅ Validate configs before deployment
✅ Generate single deployable artifacts for production
✅ Use `/config` endpoints for debugging
✅ Keep environment overrides minimal (only what changes)
✅ Document configuration changes in commit messages

**DON'T**:
❌ Commit production secrets to config files
❌ Mix test and production configurations
❌ Skip validation before deployment
❌ Use unbounded configuration values
❌ Override all settings in environment files

### 8.2 Security Best Practices

**Secrets Management**:
```toml
# ✅ GOOD: Use environment variable substitution
[database]
url = "${DATABASE_URL}"

# ❌ BAD: Hard-code credentials
[database]
url = "postgresql://user:password@localhost/db"
```

**Production Deployment**:
```bash
# ✅ GOOD: Use Kubernetes secrets
kubectl create secret generic tasker-db-url \
  --from-literal=url='postgresql://...'

# ❌ BAD: Commit secrets to config files
```

**Runtime Inspection**:
- `/config` endpoint automatically redacts secrets
- Safe to use in logging and monitoring
- Field path tracking shows what was redacted

### 8.3 Testing Strategy

**Test All Environments**:
```bash
# Ensure all environments validate
for env in test development production; do
  echo "Validating $env..."
  tasker-cli config validate --context orchestration --environment $env
done
```

**Integration Testing**:
```bash
# Test with generated configs
tasker-cli config generate --context orchestration --environment test
export TASKER_CONFIG_PATH=config/tasker/generated/orchestration-test.toml
cargo test --all-features
```

---

## Part 9: Troubleshooting

### 9.1 Common Issues

**Issue**: Configuration fails to load
```bash
# Check environment variable
echo $TASKER_ENV

# Check config files exist
ls -la config/tasker/base/
ls -la config/tasker/environments/$TASKER_ENV/

# Validate config
tasker-cli config validate --context orchestration --environment $TASKER_ENV
```

**Issue**: Unexpected configuration values at runtime
```bash
# Check runtime config
curl http://localhost:8080/config | jq

# Compare to expected
cat config/tasker/generated/orchestration-$TASKER_ENV.toml
```

**Issue**: Validation errors
```bash
# Run validation with detailed output
RUST_LOG=debug tasker-cli config validate \
  --context orchestration \
  --environment production
```

### 9.2 Debug Mode

**Enable Configuration Debug Logging**:
```bash
# Detailed config loading logs
RUST_LOG=tasker_shared::config=debug cargo run

# Shows:
# - Which files are loaded
# - Merge order
# - Environment variable substitution
# - Validation results
```

---

## Part 10: Future Enhancements

### 10.1 Planned Features (TAS-50 Phase 2)

**Explain Command** (Deferred):
```bash
# Get documentation for a parameter
tasker-cli config explain --parameter database.pool.max_connections

# Shows:
# - Purpose and system impact
# - Valid range and type
# - Environment-specific recommendations
# - Related parameters
# - Example usage
```

**Detect-Unused Command** (Deferred):
```bash
# Find unused configuration parameters
tasker-cli config detect-unused --context orchestration

# Auto-remove with backup
tasker-cli config detect-unused --context orchestration --fix
```

### 10.2 Operational Enhancements

**Configuration Versioning**:
- Track configuration changes over time
- Compare configs across versions
- Rollback capability

**Automated Drift Detection**:
- Periodic config snapshots
- Alert on unexpected changes
- Configuration compliance checking

**Configuration Templates**:
- Pre-built configurations for common scenarios
- Quick-start templates for new deployments
- Best practice configurations

---

## Related Documentation

- **[Environment Configuration Comparison](environment-configuration-comparison.md)** - Detailed comparison of configuration values across environments
- **[Deployment Patterns](deployment-patterns.md)** - Deployment modes and strategies
- **[TAS-50 CLI Specification](ticket-specs/TAS-50/CLI.md)** - Complete CLI implementation details
- **[Quick Start Guide](quick-start.md)** - Getting started with Tasker

---

## Summary

Tasker's configuration system provides:

1. **Component-Based Architecture**: Focused TOML files with single responsibility
2. **Environment Scaling**: 1:5:50 pattern from test → development → production
3. **Runtime Observability**: `/config` endpoints with comprehensive secret redaction
4. **CLI Tools**: Generate and validate single deployable configs
5. **Context-Specific Validation**: Catch errors before deployment
6. **Security First**: Automatic secret redaction, environment variable substitution

**Key Workflows**:
- **Development**: Use automatic loading with `TASKER_ENV=development`
- **Production**: Generate artifacts with CLI, deploy single files
- **Debugging**: Use `/config` endpoints to inspect runtime configuration
- **Validation**: Validate before generating deployment artifacts

← Back to [Documentation Hub](README.md)

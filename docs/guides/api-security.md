# API Security Guide (TAS-150)

API-level security for orchestration (8080) and worker (8081) endpoints using JWT bearer tokens and API key authentication with permission-based access control.

**Security is disabled by default** for backward compatibility. Enable it explicitly in configuration.

> See also: [Auth Documentation Hub](../auth/README.md) for architecture overview, [Permissions](../auth/permissions.md) for route mapping, [Configuration](../auth/configuration.md) for full reference, [Testing](../auth/testing.md) for E2E test patterns.

---

## Quick Start

### 1. Generate Keys

```bash
cargo run --bin tasker-cli -- auth generate-keys --output-dir ./keys
```

### 2. Generate a Token

```bash
cargo run --bin tasker-cli -- auth generate-token \
  --private-key ./keys/jwt-private-key.pem \
  --permissions "tasks:create,tasks:read,tasks:list,steps:read" \
  --subject my-service \
  --expiry-hours 24
```

### 3. Enable Auth in Configuration

In `config/tasker/base/orchestration.toml`:

```toml
[auth]
enabled = true
jwt_public_key_path = "./keys/jwt-public-key.pem"
jwt_issuer = "tasker-core"
jwt_audience = "tasker-api"
```

### 4. Use the Token

```bash
export TASKER_AUTH_TOKEN=<generated-token>
cargo run --bin tasker-cli -- task list
```

Or with curl:

```bash
curl -H "Authorization: Bearer $TASKER_AUTH_TOKEN" http://localhost:8080/v1/tasks
```

---

## Permission Vocabulary

| Permission | Resource | Description |
|------------|----------|-------------|
| `tasks:create` | tasks | Create new tasks |
| `tasks:read` | tasks | Read task details |
| `tasks:list` | tasks | List tasks |
| `tasks:cancel` | tasks | Cancel running tasks |
| `tasks:context_read` | tasks | Read task context data |
| `steps:read` | steps | Read workflow step details |
| `steps:resolve` | steps | Manually resolve steps |
| `dlq:read` | dlq | Read DLQ entries |
| `dlq:update` | dlq | Update DLQ investigations |
| `dlq:stats` | dlq | View DLQ statistics |
| `templates:read` | templates | Read task templates |
| `templates:validate` | templates | Validate templates |
| `system:config_read` | system | Read system configuration |
| `system:handlers_read` | system | Read handler registry |
| `system:analytics_read` | system | Read analytics data |
| `worker:config_read` | worker | Read worker configuration |
| `worker:templates_read` | worker | Read worker templates |

### Wildcards

- `tasks:*` - All task permissions
- `steps:*` - All step permissions
- `dlq:*` - All DLQ permissions
- `*` - All permissions (superuser)

### Show All Permissions

```bash
cargo run --bin tasker-cli -- auth show-permissions
```

---

## Configuration Reference

### Server-Side (orchestration.toml / worker.toml)

```toml
[auth]
enabled = true

# JWT Configuration
jwt_issuer = "tasker-core"
jwt_audience = "tasker-api"
jwt_token_expiry_hours = 24

# Key Configuration (one of these):
jwt_public_key_path = "./keys/jwt-public-key.pem"   # File path (preferred)
jwt_public_key = "-----BEGIN RSA PUBLIC KEY-----..." # Inline PEM
# Or set env: TASKER_JWT_PUBLIC_KEY_PATH

# JWKS (for dynamic key rotation)
jwt_verification_method = "jwks"  # "public_key" (default) or "jwks"
jwks_url = "https://auth.example.com/.well-known/jwks.json"
jwks_refresh_interval_seconds = 3600

# Permission validation
permissions_claim = "permissions"   # JWT claim containing permissions
strict_validation = true            # Reject tokens with unknown permissions
log_unknown_permissions = true

# API Key Authentication
api_key_header = "X-API-Key"
api_keys_enabled = true

[[auth.api_keys]]
key = "sk-prod-key-1"
permissions = ["tasks:read", "tasks:list", "steps:read"]
description = "Read-only monitoring service"

[[auth.api_keys]]
key = "sk-admin-key"
permissions = ["*"]
description = "Admin key"
```

### Client-Side (Environment Variables)

| Variable | Description |
|----------|-------------|
| `TASKER_AUTH_TOKEN` | Bearer token for both APIs |
| `TASKER_ORCHESTRATION_AUTH_TOKEN` | Override token for orchestration only |
| `TASKER_WORKER_AUTH_TOKEN` | Override token for worker only |
| `TASKER_API_KEY` | API key (fallback if no token) |
| `TASKER_API_KEY_HEADER` | Custom header name (default: `X-API-Key`) |

Priority: endpoint-specific token > global token > API key > config file.

---

## JWT Token Structure

```json
{
  "sub": "my-service",
  "iss": "tasker-core",
  "aud": "tasker-api",
  "iat": 1706000000,
  "exp": 1706086400,
  "permissions": [
    "tasks:create",
    "tasks:read",
    "tasks:list",
    "steps:read"
  ],
  "worker_namespaces": []
}
```

### Common Role Patterns

**Read-only operator:**
```
permissions: ["tasks:read", "tasks:list", "steps:read", "dlq:read", "dlq:stats"]
```

**Task submitter:**
```
permissions: ["tasks:create", "tasks:read", "tasks:list"]
```

**Ops admin:**
```
permissions: ["tasks:*", "steps:*", "dlq:*", "system:*"]
```

**Worker service:**
```
permissions: ["worker:config_read", "worker:templates_read"]
```

**Superuser:**
```
permissions: ["*"]
```

---

## Public Endpoints

These endpoints never require authentication:

- `GET /health` - Basic health check
- `GET /health/detailed` - Detailed health
- `GET /metrics` - Prometheus metrics

---

## API Key Authentication

API keys are validated against a configured registry. Each key has its own set of permissions.

```bash
# Using API key
curl -H "X-API-Key: sk-prod-key-1" http://localhost:8080/v1/tasks
```

API keys are simpler than JWTs but have limitations:
- No expiration (rotate by removing from config)
- No claims beyond permissions
- Best for service-to-service communication with static permissions

---

## Error Responses

### 401 Unauthorized (Missing/Invalid Credentials)
```json
{
  "error": "unauthorized",
  "message": "Missing authentication credentials"
}
```

### 403 Forbidden (Insufficient Permissions)
```json
{
  "error": "forbidden",
  "message": "Missing required permission: tasks:create"
}
```

---

## Migration Guide: Disabled to Enabled

1. **Generate keys** and distribute the public key to server config
2. **Generate tokens** for each service/user with appropriate permissions
3. **Set `enabled = true`** in auth config
4. **Deploy** - services without valid tokens will get 401 responses
5. **Monitor** the `tasker.auth.failures.total` metric for issues

All endpoints remain accessible without auth when `enabled = false`.

---

## Observability

### Structured Logs

- `info` on successful authentication (subject, method)
- `warn` on authentication failure (error details)
- `warn` on permission denial (subject, required permission)

### Prometheus Metrics

| Metric | Type | Labels |
|--------|------|--------|
| `tasker.auth.requests.total` | Counter | method, result |
| `tasker.auth.failures.total` | Counter | reason |
| `tasker.permission.denials.total` | Counter | permission |
| `tasker.auth.jwt.verification.duration` | Histogram | result |

---

## CLI Auth Commands

```bash
# Generate RSA key pair
tasker-cli auth generate-keys [--output-dir ./keys] [--key-size 2048]

# Generate JWT token
tasker-cli auth generate-token \
  --permissions tasks:create,tasks:read \
  --subject my-service \
  --private-key ./keys/jwt-private-key.pem \
  --expiry-hours 24

# List all permissions
tasker-cli auth show-permissions

# Validate a token
tasker-cli auth validate-token \
  --token <JWT> \
  --public-key ./keys/jwt-public-key.pem
```

---

## gRPC Authentication (TAS-177)

gRPC endpoints support the same authentication methods as REST, using gRPC metadata instead of HTTP headers.

### gRPC Ports

| Service | REST Port | gRPC Port |
|---------|-----------|-----------|
| Orchestration | 8080 | 9190 |
| Rust Worker | 8081 | 9191 |

### Bearer Token (gRPC)

```bash
# Using grpcurl with Bearer token
grpcurl -plaintext \
  -H "Authorization: Bearer $TASKER_AUTH_TOKEN" \
  localhost:9190 tasker.v1.TaskService/ListTasks
```

### API Key (gRPC)

```bash
# Using grpcurl with API key
grpcurl -plaintext \
  -H "X-API-Key: sk-prod-key-1" \
  localhost:9190 tasker.v1.TaskService/ListTasks
```

### gRPC Client Configuration

```rust
use tasker_client::grpc_clients::{OrchestrationGrpcClient, GrpcAuthConfig};

// With API key
let client = OrchestrationGrpcClient::connect_with_auth(
    "http://localhost:9190",
    GrpcAuthConfig::ApiKey("sk-prod-key-1".to_string()),
).await?;

// With Bearer token
let client = OrchestrationGrpcClient::connect_with_auth(
    "http://localhost:9190",
    GrpcAuthConfig::Bearer("eyJ...".to_string()),
).await?;
```

### gRPC Error Codes

| gRPC Status | HTTP Equivalent | Meaning |
|-------------|-----------------|---------|
| `UNAUTHENTICATED` | 401 | Missing or invalid credentials |
| `PERMISSION_DENIED` | 403 | Valid credentials but insufficient permissions |
| `NOT_FOUND` | 404 | Resource not found |
| `UNAVAILABLE` | 503 | Service unavailable |

### Public gRPC Endpoints

These endpoints never require authentication:

- `HealthService/CheckHealth` - Basic health check
- `HealthService/CheckLiveness` - Kubernetes liveness probe
- `HealthService/CheckReadiness` - Kubernetes readiness probe
- `HealthService/CheckDetailedHealth` - Detailed health metrics

---

## Security Considerations

- **Key storage**: Private keys should never be committed to git. Use file paths or environment variables.
- **Token expiry**: Set appropriate expiry times. Short-lived tokens (1-24h) are preferred.
- **Least privilege**: Grant only the permissions each service needs.
- **Key rotation**: Use JWKS for automatic key rotation in production.
- **API key rotation**: Remove old keys from config and redeploy.
- **Audit**: Monitor `tasker.auth.failures.total` and `tasker.permission.denials.total` for anomalies.

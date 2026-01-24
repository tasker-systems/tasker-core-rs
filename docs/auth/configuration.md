# Configuration Reference

Complete configuration for Tasker authentication: server-side TOML, environment variables, and client settings.

---

## Server-Side Configuration

Auth config lives under `[web.auth]` in both orchestration and worker TOML files.

### Location

```
config/tasker/base/orchestration.toml    → [web.auth]
config/tasker/base/worker.toml           → [web.auth]
config/tasker/environments/{env}/...     → environment overrides
```

Configuration follows the role-based structure (see [Configuration Management](../guides/configuration-management.md)).

### Full Reference

```toml
[web]
# Whether the /config endpoint is registered (default: false).
# When false, GET /config returns 404. When true, requires system:config_read permission.
config_endpoint_enabled = false

[web.auth]
# Master switch (default: false). When disabled, all routes are accessible without credentials.
enabled = false

# --- JWT Configuration ---

# Token issuer claim (validated against incoming tokens)
jwt_issuer = "tasker-core"

# Token audience claim (validated against incoming tokens)
jwt_audience = "tasker-api"

# Token expiry for generated tokens (via CLI)
jwt_token_expiry_hours = 24

# Verification method: "public_key" (static RSA key) or "jwks" (dynamic key rotation)
jwt_verification_method = "public_key"

# Static public key (one of these, path takes precedence):
jwt_public_key_path = "/etc/tasker/keys/jwt-public-key.pem"
jwt_public_key = ""  # Inline PEM string (use path instead for production)

# Private key (for token generation only, not needed for verification):
jwt_private_key = ""

# --- JWKS Configuration (when jwt_verification_method = "jwks") ---

# JWKS endpoint URL
jwks_url = "https://auth.example.com/.well-known/jwks.json"

# How often to refresh the key set (seconds)
jwks_refresh_interval_seconds = 3600

# --- Permission Validation ---

# JWT claim name containing the permissions array
permissions_claim = "permissions"

# Reject tokens with unrecognized permission strings
strict_validation = true

# Log unrecognized permissions even when strict_validation = false
log_unknown_permissions = true

# --- API Key Authentication ---

# Header name for API key authentication
api_key_header = "X-API-Key"

# Enable multi-key registry (default: false)
api_keys_enabled = false

# API key registry (multiple keys with individual permissions)
[[web.auth.api_keys]]
key = "sk-prod-monitoring-key"
permissions = ["tasks:read", "tasks:list", "dlq:read", "dlq:stats"]
description = "Production monitoring service"

[[web.auth.api_keys]]
key = "sk-prod-admin-key"
permissions = ["*"]
description = "Production admin"
```

---

## Environment Variables

### Server-Side

| Variable | Description | Overrides |
|----------|-------------|-----------|
| `TASKER_JWT_PUBLIC_KEY_PATH` | Path to RSA public key PEM file | `web.auth.jwt_public_key_path` |
| `TASKER_JWT_PUBLIC_KEY` | Inline PEM public key | `web.auth.jwt_public_key` |

These override TOML values via the config loader's environment interpolation.

### Client-Side

| Variable | Priority | Description |
|----------|----------|-------------|
| `TASKER_ORCHESTRATION_AUTH_TOKEN` | 1 (highest) | Bearer token for orchestration API only |
| `TASKER_WORKER_AUTH_TOKEN` | 1 (highest) | Bearer token for worker API only |
| `TASKER_AUTH_TOKEN` | 2 | Bearer token for both APIs |
| `TASKER_API_KEY` | 3 | API key (sent via configured header) |
| `TASKER_API_KEY_HEADER` | — | Custom header name (default: `X-API-Key`) |
| `TASKER_JWT_PRIVATE_KEY_PATH` | 4 (lowest) | Private key for on-demand token generation |

The `tasker-client` library checks these in priority order and uses the first available credential.

---

## Deployment Patterns

### Development (Auth Disabled)

```toml
[web.auth]
enabled = false
```

All endpoints accessible without credentials. Default behavior.

### Development (Auth Enabled, Static Key)

```toml
[web.auth]
enabled = true
jwt_verification_method = "public_key"
jwt_public_key_path = "./keys/jwt-public-key.pem"
jwt_issuer = "tasker-core"
jwt_audience = "tasker-api"
strict_validation = false

[[web.auth.api_keys]]
key = "dev-key"
permissions = ["*"]
description = "Dev superuser key"
```

### Production (JWKS + API Keys)

```toml
[web.auth]
enabled = true
jwt_verification_method = "jwks"
jwks_url = "https://auth.company.com/.well-known/jwks.json"
jwks_refresh_interval_seconds = 3600
jwt_issuer = "https://auth.company.com/"
jwt_audience = "tasker-api"
strict_validation = true
log_unknown_permissions = true
api_keys_enabled = true
api_key_header = "X-API-Key"

[[web.auth.api_keys]]
key = "sk-monitoring-prod"
permissions = ["tasks:read", "tasks:list", "steps:read", "dlq:read", "dlq:stats"]
description = "Monitoring service"

[[web.auth.api_keys]]
key = "sk-submitter-prod"
permissions = ["tasks:create", "tasks:read", "tasks:list"]
description = "Task submission service"
```

### Production (Config Endpoint Enabled)

```toml
[web]
config_endpoint_enabled = true

[web.auth]
enabled = true
# ... auth config ...
```

Exposes `GET /config` (requires `system:config_read` permission). Secrets are redacted in the response.

---

## Key Management

### Generating Keys

```bash
# Generate 2048-bit RSA key pair
cargo run --bin tasker-cli -- auth generate-keys --output-dir ./keys --key-size 2048

# Output:
#   keys/jwt-private-key.pem  (keep secret, used for token generation)
#   keys/jwt-public-key.pem   (distribute to servers for verification)
```

### Key Rotation (Static Key)

1. Generate a new key pair
2. Update `jwt_public_key_path` in server config
3. Restart servers
4. Re-generate tokens with the new private key
5. Old tokens become invalid immediately

### Key Rotation (JWKS)

Handled automatically by the identity provider. Tasker refreshes keys on:
- Timer interval (`jwks_refresh_interval_seconds`)
- Unknown `kid` in incoming token (triggers immediate refresh)

---

## Security Hardening Checklist

- [ ] Private keys never committed to version control
- [ ] `enabled = true` in production configs
- [ ] `strict_validation = true` to reject unknown permissions
- [ ] Token expiry set appropriately (1-24h recommended)
- [ ] API keys use descriptive names for audit trails
- [ ] `config_endpoint_enabled = false` unless needed (default)
- [ ] Monitor `tasker.auth.failures.total` metric for anomalies
- [ ] Use JWKS in production for automatic key rotation
- [ ] Least-privilege: each service gets only the permissions it needs

---

## Related

- [API Security Guide](../guides/api-security.md) — Quick start, CLI commands, error responses
- [Auth Integration Guide](../guides/auth-integration.md) — Auth0, Keycloak, Okta, JWKS setup
- [Permissions](permissions.md) — Full permission vocabulary and route mapping

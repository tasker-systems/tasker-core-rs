# API-Level Security: JWT/API Key Authentication and Permission Enforcement

## Overview

Implement API-level security for Tasker orchestration and worker services with JWT-based authentication and permission-based authorization. This security layer protects REST API endpoints while maintaining architectural simplicity by delegating identity management and role-based access control to external systems.

## Core Principle: Permission Enforcement, Not Policy Management

**Tasker's Responsibility:**

* Define the permission vocabulary (actions on resources)
* Verify JWT signatures and validate claims
* Enforce: "Does this token have permission X?"

**External System's Responsibility:**

* User/service account management
* Role definitions and role → permission mapping
* Token issuance with final permission list

## Permission Vocabulary

### Orchestration API (Port 8080)

**Always Public (No Auth Required):**

```
/health, /health/detailed, /health/ready, /health/live
/metrics
```

**Requires Permissions:**

```
tasks:create          POST /v1/tasks
tasks:read            GET /v1/tasks/{uuid}
tasks:list            GET /v1/tasks
tasks:cancel          DELETE /v1/tasks/{uuid}
tasks:context:read    GET /v1/tasks/{uuid}/context

steps:read            GET /v1/tasks/{uuid}/workflow_steps
steps:resolve         PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}

dlq:read              GET /v1/dlq/*, GET /v1/dlq/task/{uuid}
dlq:update            PATCH /v1/dlq/entry/{uuid}
dlq:stats             GET /v1/dlq/stats

templates:read        GET /v1/templates, GET /v1/templates/{ns}/{name}/{ver}
templates:validate    POST /v1/templates/{ns}/{name}/{ver}/validate

system:config:read    GET /config
```

### Worker API (Port 8081)

**Always Public (No Auth Required):**

```
/health, /health/detailed, /health/ready, /health/live
/metrics, /metrics/worker, /metrics/events
```

**Requires Permissions:**

```
worker:config:read    GET /config
worker:templates:read GET /templates, GET /templates/{ns}/{name}/{ver}
```

### Wildcard Support (Limited)

**Allowed:**

* `tasks:*` - All task operations
* `steps:*` - All step operations
* `dlq:*` - All DLQ operations
* `templates:*` - All template operations
* `worker:*` - All worker operations

**Not Allowed:**

* `*` - Too broad, must be explicit
* `*:read` - Cross-resource wildcards not supported

## Configuration Schema

### Orchestration Configuration

```toml
# config/tasker/base/orchestration.toml

[security]
enabled = false  # Disabled by default for backward compatibility

[security.jwt]
verification_method = "public_key"  # or "jwks"
public_key_path = "/etc/tasker/jwt-public-key.pem"
# OR public_key = "${JWT_PUBLIC_KEY}"
# OR jwks_url = "${JWKS_URL}"
# jwks_refresh_interval_seconds = 3600

issuer = "${JWT_ISSUER}"
audience = "tasker-orchestration"
permissions_claim = "permissions"

[security.validation]
strict_validation = true  # Reject tokens with unknown permissions
log_unknown_permissions = true

[security.api_keys]
enabled = false

[[security.api_keys.keys]]
key = "${TASKER_API_KEY_CI}"
permissions = ["tasks:create", "tasks:read", "templates:read"]
description = "CI/CD pipeline"
```

### Worker Configuration

```toml
# config/tasker/base/worker.toml

[security]
enabled = false

[security.jwt]
verification_method = "public_key"
public_key_path = "/etc/tasker/jwt-public-key.pem"
issuer = "${JWT_ISSUER}"
audience = "tasker-worker"
permissions_claim = "permissions"

[security.validation]
strict_validation = true
log_unknown_permissions = true

[security.api_keys]
enabled = false

[[security.api_keys.keys]]
key = "${TASKER_API_KEY_ADMIN}"
permissions = ["worker:*"]
description = "Admin access"
```

## Expected JWT Format

```json
{
  "iss": "https://auth.company.com",
  "sub": "user-uuid-or-service-account-id",
  "aud": "tasker-orchestration",
  "exp": 1737388800,
  "iat": 1737302400,
  "permissions": [
    "tasks:create",
    "tasks:read",
    "steps:read",
    "dlq:read",
    "system:config:read"
  ]
}
```

## Implementation Phases

### Phase 1: Core Infrastructure

- [ ] Add `jsonwebtoken` crate dependency
- [ ] JWT verification service (signature, issuer, audience, expiration)
- [ ] JWKS support (dynamic key rotation)
- [ ] Permission validation (known vocabulary, wildcards)
- [ ] Configuration schema implementation
- [ ] Unit tests for JWT verification and permission matching

### Phase 2: Orchestration API Protection

- [ ] Axum middleware for JWT extraction/verification
- [ ] Permission enforcement layer with wildcard support
- [ ] Route permission annotations (`.requires_permission()`)
- [ ] Public endpoint exemption (health/metrics)
- [ ] Security context in request extensions
- [ ] 401/403 responses with descriptive error messages
- [ ] Integration tests with valid/invalid/expired tokens

### Phase 3: Worker API Protection

- [ ] Apply same middleware to worker API
- [ ] Worker-specific permission definitions
- [ ] Public endpoint exemption (health/metrics)
- [ ] Integration tests

### Phase 4: API Key Support

- [ ] X-API-Key header extraction
- [ ] API key validation and permission lookup
- [ ] Permission validation for API keys (same rules)
- [ ] Configuration for multiple keys
- [ ] Integration tests

### Phase 5: Observability

- [ ] Audit logging (subject, action, resource, timestamp, result)
- [ ] Security metrics (auth failures by reason, permission denials by endpoint)
- [ ] Security context in all logs
- [ ] Unknown permission warnings in logs
- [ ] Metrics for JWT verification times

### Phase 6: Documentation

- [ ] Permission vocabulary reference table
- [ ] External OAuth/OIDC provider configuration guides (Auth0, Okta, Keycloak)
- [ ] JWT payload examples for different use cases
- [ ] API key configuration examples
- [ ] curl examples with auth headers
- [ ] Security best practices guide
- [ ] Migration guide (disabled → enabled security)

## Acceptance Criteria

### Core Functionality

- [ ] Security can be disabled (default) or enabled via configuration
- [ ] Health and metrics endpoints are always public (no auth)
- [ ] JWT tokens are verified using configured public key or JWKS endpoint
- [ ] Permissions are validated against known vocabulary
- [ ] Resource-level wildcards (e.g., `tasks:*`) work correctly
- [ ] Invalid tokens return 401 Unauthorized
- [ ] Missing permissions return 403 Forbidden
- [ ] API keys can be used as alternative to JWT

### Validation & Error Handling

- [ ] Strict validation mode rejects tokens with unknown permissions
- [ ] Unknown permissions are logged when `log_unknown_permissions = true`
- [ ] Clear error messages explain auth failures
- [ ] Expired tokens are rejected with appropriate error
- [ ] Malformed tokens are rejected with appropriate error

### Configuration

- [ ] Public key can be loaded from file or environment variable
- [ ] JWKS endpoint can be configured for dynamic key rotation
- [ ] Multiple API keys can be configured with different permission sets
- [ ] Security settings can be different for orchestration vs worker

### Testing

- [ ] Unit tests for permission matching (exact and wildcard)
- [ ] Unit tests for JWT verification (valid, expired, malformed)
- [ ] Integration tests for protected endpoints
- [ ] Integration tests for public endpoints
- [ ] Integration tests for API key authentication

### Documentation

- [ ] Permission vocabulary is documented
- [ ] Configuration examples for common scenarios
- [ ] Example JWT payloads for common roles
- [ ] External auth system integration examples
- [ ] Migration guide for enabling security

## Common Permission Sets (Documentation Examples)

**Developer:**

```json
["tasks:create", "tasks:read", "tasks:list", "steps:read", "templates:read", "system:config:read"]
```

**Operator:**

```json
["tasks:*", "steps:*", "dlq:*", "templates:*", "system:config:read"]
```

**CI/CD Pipeline:**

```json
["tasks:create", "tasks:read", "templates:read"]
```

**Read-Only:**

```json
["tasks:read", "tasks:list", "steps:read", "dlq:read", "templates:read", "system:config:read"]
```

## Out of Scope (Future Enhancements)

* Namespace-level permission filtering (e.g., `tasks:create:namespace:payment_processing`)
* Role management within Tasker
* User identity management
* Token issuance
* OAuth/OIDC flows

## Notes

* Database access remains protected by PostgreSQL credentials + network isolation
* Workers authenticate via database credentials, not HTTP auth
* Health and metrics endpoints must remain public for Kubernetes probes
* Permission vocabulary should be versioned for future evolution

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-150/api-level-security-jwtapi-key-authentication-and-permission](https://linear.app/tasker-systems/issue/TAS-150/api-level-security-jwtapi-key-authentication-and-permission)
- Identifier: TAS-150
- Status: In Progress
- Priority: Medium
- Assignee: Pete Taylor
- Labels: Feature
- Project: [Tasker Core Rust](https://linear.app/tasker-systems/project/tasker-core-rust-9b5a1c23b7b1). Alpha version of the Tasker Core in Rust
- Created: 2026-01-16T18:48:34.985Z
- Updated: 2026-01-23T21:00:19.034Z

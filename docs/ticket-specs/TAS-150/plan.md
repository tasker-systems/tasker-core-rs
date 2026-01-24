# TAS-150: API-Level Security Implementation Plan

## Summary

Implement permission-based API security for orchestration (8080) and worker (8081) APIs with JWT and API key authentication. Security is disabled by default for backward compatibility. Per-handler permission enforcement using a compile-time permission vocabulary.

---

## Current State

- `JwtAuthenticator` exists in `tasker-shared/src/types/auth.rs` (RSA signing/verification, `WorkerClaims`)
- Auth middleware in `tasker-orchestration/src/web/middleware/auth.rs` (`conditional_auth`, `require_auth`, `optional_auth`) - validates tokens but no permission checks
- `WebAuthConfig` adapter in `tasker-shared/src/config/web.rs` with route-pattern matching
- `AuthConfig` (V2) in `tasker-shared/src/config/tasker.rs` - basic JWT fields, single api_key, protected_routes
- Client (`tasker-client/src/config.rs`) has `auth_token: Option<String>` per endpoint
- Worker API has zero auth infrastructure
- No JWKS, no wildcard permission matching, no API key validation, no audit logging

---

## Phase 1: Permission Vocabulary and Security Types

**Goal**: Define the compile-time permission vocabulary and `SecurityContext`.

### New Files
- `tasker-shared/src/types/permissions.rs`
- `tasker-shared/src/types/security.rs`

### `permissions.rs` - Core Content
```rust
pub enum Permission {
    TasksCreate, TasksRead, TasksList, TasksCancel, TasksContextRead,
    StepsRead, StepsResolve,
    DlqRead, DlqUpdate, DlqStats,
    TemplatesRead, TemplatesValidate,
    SystemConfigRead, HandlersRead, AnalyticsRead,
    WorkerConfigRead, WorkerTemplatesRead,
}
```
- `Permission::as_str()` → `"tasks:create"` etc.
- `Permission::resource()` → `"tasks"` etc.
- `Permission::from_str_opt(s)` → `Option<Permission>`
- `permission_matches(claimed: &str, required: &Permission) -> bool` (exact + resource wildcard `tasks:*`)
- `has_permission(claimed: &[String], required: &Permission) -> bool`
- `validate_permissions(claimed: &[String]) -> Vec<String>` (returns unknown ones)

### `security.rs` - Core Content
```rust
pub struct SecurityContext {
    pub subject: String,
    pub auth_method: AuthMethod,
    pub permissions: Vec<String>,
    pub issuer: Option<String>,
    pub expires_at: Option<i64>,
}

pub enum AuthMethod { Jwt, ApiKey { description: String }, Disabled }
```
- `SecurityContext::has_permission(&self, required: &Permission) -> bool`
- `SecurityContext::disabled_context()` - for when auth is off
- Implement `FromRequestParts` for `SecurityContext` (axum extractor)

### Modify
- `tasker-shared/src/types/mod.rs` - add module declarations

### Unit Tests
- Permission exact matching, wildcard matching, unknown detection
- `SecurityContext::has_permission` with various scenarios

---

## Phase 2: Evolve AuthConfig and TokenClaims

**Goal**: Expand configuration to support JWKS, multiple API keys, and public key path. Rename `WorkerClaims` to `TokenClaims`.

### Modify `tasker-shared/src/config/tasker.rs`
Add fields to `AuthConfig`:
```rust
pub jwt_verification_method: String,    // "public_key" or "jwks" (default: "public_key")
pub jwt_public_key_path: String,        // Alternative to inline key
pub jwks_url: String,                   // For dynamic key rotation
pub jwks_refresh_interval_seconds: u32, // Default: 3600
pub permissions_claim: String,          // Default: "permissions"
pub strict_validation: bool,            // Default: true
pub log_unknown_permissions: bool,      // Default: true
pub api_keys_enabled: bool,             // Default: false
pub api_keys: Vec<ApiKeyConfig>,        // Multiple keys with per-key permissions
```

New struct:
```rust
pub struct ApiKeyConfig {
    pub key: String,
    pub permissions: Vec<String>,
    pub description: String,
}
```

### Modify `tasker-shared/src/types/auth.rs`
- Rename `WorkerClaims` → `TokenClaims`
- Make `worker_namespaces` optional (for backward compat with existing tokens)
- Add `pub type WorkerClaims = TokenClaims;` alias for gradual migration
- Update `JwtAuthenticator` to use `TokenClaims`
- Add `validate_token(&self, token: &str) -> Result<TokenClaims, AuthError>` (generic name)
- Keep `validate_worker_token` as deprecated alias

### Modify `tasker-shared/src/config/web.rs`
- Update `From<AuthConfig> for WebAuthConfig` to handle new fields
- Add new fields to `WebAuthConfig` for the expanded config

### Modify TOML configs
- `config/tasker/base/orchestration.toml` - add new auth fields
- `config/tasker/base/worker.toml` - add auth section
- `config/tasker/environments/test/orchestration.toml` - test auth config

---

## Phase 3: API Key Registry

**Goal**: Runtime API key validation with per-key permissions.

### New File
- `tasker-shared/src/types/api_key_auth.rs`

### Content
```rust
pub struct ApiKeyRegistry {
    keys: HashMap<String, ApiKeyEntry>,  // key_value → entry
}
impl ApiKeyRegistry {
    pub fn from_config(configs: &[ApiKeyConfig]) -> Self;
    pub fn validate_key(&self, key: &str) -> Result<SecurityContext, AuthError>;
}
```

### Unit Tests
- Valid key → SecurityContext with correct permissions
- Invalid key → error
- Multiple keys with different permissions

---

## Phase 4: JWKS Support

**Goal**: Async JWKS key fetching with caching and refresh.

### New File
- `tasker-shared/src/types/jwks.rs`

### Content
```rust
pub struct JwksKeyStore {
    keys: Arc<RwLock<JwksCacheEntry>>,
    jwks_url: String,
    refresh_interval: Duration,
    client: reqwest::Client,
}
impl JwksKeyStore {
    pub async fn new(url: String, refresh: Duration) -> Result<Self, AuthError>;
    pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, AuthError>;
}
```
- Parse RSA JWKs (n, e components) into `DecodingKey`
- Cache with time-based expiry
- Refresh on cache miss or stale

### Unit Tests
- Mock HTTP responses for JWKS endpoint
- Key rotation (new kid after refresh)
- Error handling (network failure, malformed response)

---

## Phase 5: SecurityService (Unified Auth)

**Goal**: Single service combining JWT + JWKS + API key authentication.

### New File
- `tasker-shared/src/types/security_service.rs`

### Content
```rust
pub struct SecurityService {
    jwt_authenticator: Option<JwtAuthenticator>,
    jwks_store: Option<Arc<JwksKeyStore>>,
    api_key_registry: Option<ApiKeyRegistry>,
    config: AuthConfig,
}
impl SecurityService {
    pub async fn from_config(config: &AuthConfig) -> Result<Self, AuthError>;
    pub async fn authenticate_bearer(&self, token: &str) -> Result<SecurityContext, AuthError>;
    pub fn authenticate_api_key(&self, key: &str) -> Result<SecurityContext, AuthError>;
    pub fn is_enabled(&self) -> bool;
    pub fn strict_validation(&self) -> bool;
    fn resolve_public_key(config: &AuthConfig) -> Result<String, AuthError>; // file > inline > env
    fn validate_token_permissions(&self, ctx: &SecurityContext) -> Result<(), AuthError>; // strict mode
}
```

### Modify `tasker-shared/src/types/mod.rs`
- Add `security_service`, `jwks`, `api_key_auth` modules
- Re-export `SecurityService`, `SecurityContext`, `Permission`

---

## Phase 6: Orchestration API Protection

**Goal**: Wire auth middleware + per-handler permission checks for all orchestration endpoints.

### Modify `tasker-orchestration/src/web/state.rs`
- Add `pub security_service: Option<Arc<SecurityService>>` to `AppState`
- Build `SecurityService` from config in `from_orchestration_core()`

### Rewrite `tasker-orchestration/src/web/middleware/auth.rs`
New middleware function:
```rust
pub async fn authenticate_request(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // If security disabled → inject SecurityContext::disabled_context()
    // Check Bearer header → authenticate_bearer()
    // Check API key header → authenticate_api_key()
    // Neither → 401
    // If strict_validation → validate unknown permissions
    // Store SecurityContext in extensions
}
```

### New `tasker-orchestration/src/web/middleware/permission.rs`
```rust
pub fn require_permission(ctx: &SecurityContext, perm: Permission) -> Result<(), ApiError> {
    if ctx.auth_method == AuthMethod::Disabled { return Ok(()); }
    if ctx.has_permission(&perm) { Ok(()) }
    else { Err(ApiError::authorization_error(format!("Missing: {}", perm))) }
}
```

### Modify `tasker-orchestration/src/web/extractors.rs`
- Add `impl FromRequestParts for SecurityContext` extractor
- Keep existing `AuthenticatedWorker` as a thin wrapper over `SecurityContext`

### Modify `tasker-orchestration/src/web/mod.rs`
- Replace `conditional_auth` layer with `authenticate_request`
- Remove old `conditional_auth` from middleware stack

### Modify All Protected Handlers
Each handler gains `security: SecurityContext` param and one-liner check:

| Handler File | Permission |
|---|---|
| `handlers/tasks.rs::create_task` | `Permission::TasksCreate` |
| `handlers/tasks.rs::list_tasks` | `Permission::TasksList` |
| `handlers/tasks.rs::get_task` | `Permission::TasksRead` |
| `handlers/tasks.rs::cancel_task` | `Permission::TasksCancel` |
| `handlers/steps.rs::list_steps` | `Permission::StepsRead` |
| `handlers/steps.rs::get_step` | `Permission::StepsRead` |
| `handlers/steps.rs::resolve_step` | `Permission::StepsResolve` |
| `handlers/dlq.rs::list_*` | `Permission::DlqRead` |
| `handlers/dlq.rs::update_*` | `Permission::DlqUpdate` |
| `handlers/dlq.rs::stats` | `Permission::DlqStats` |
| `handlers/templates.rs::list/get` | `Permission::TemplatesRead` |
| `handlers/templates.rs::validate` | `Permission::TemplatesValidate` |
| `handlers/config.rs` | `Permission::SystemConfigRead` |
| `handlers/handlers.rs` | `Permission::HandlersRead` |
| `handlers/analytics.rs` | `Permission::AnalyticsRead` |

Health and metrics handlers: **no permission check** (always public).

### Integration Tests
- 401 without credentials on protected endpoints
- 403 with wrong permissions
- 200 with correct permissions
- 200 with wildcard permissions (e.g., `tasks:*`)
- Health/metrics always 200 without auth
- API key authentication works
- Expired token → 401
- Auth disabled → all endpoints accessible

---

## Phase 7: Worker API Protection

**Goal**: Same auth pattern for worker API.

### New Files
- `tasker-worker/src/web/middleware/mod.rs`
- `tasker-worker/src/web/middleware/auth.rs`

### Modify `tasker-worker/src/web/state.rs`
- Add `SecurityService` to worker web state

### Modify `tasker-worker/src/web/mod.rs`
- Add auth middleware layer

### Modify Worker Handlers
| Handler | Permission |
|---|---|
| `config.rs` | `Permission::WorkerConfigRead` |
| `templates.rs::list/get` | `Permission::WorkerTemplatesRead` |

Health and metrics: always public.

---

## Phase 8: Client Enhancement

**Goal**: Client sends auth credentials to server. Primary method: Bearer token from env var.

### Modify `tasker-client/src/config.rs`
Add `AuthConfig` to `ApiEndpointConfig`:
```rust
pub struct ApiEndpointConfig {
    pub base_url: String,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub auth: Option<ClientAuthConfig>,
}

pub struct ClientAuthConfig {
    pub method: ClientAuthMethod,
}

pub enum ClientAuthMethod {
    BearerToken(String),
    ApiKey { key: String, header_name: String },
}
```

### Environment Variables (priority over config file)
- `TASKER_AUTH_TOKEN` → Bearer token (used for both orchestration and worker)
- `TASKER_ORCHESTRATION_AUTH_TOKEN` → Override for orchestration only
- `TASKER_WORKER_AUTH_TOKEN` → Override for worker only
- `TASKER_API_KEY` → API key (fallback if no token)
- `TASKER_API_KEY_HEADER` → Custom header name (default: X-API-Key)

### Modify `tasker-client/src/api_clients/orchestration_client.rs`
- Update `new()` to resolve auth from `ClientAuthConfig`
- Set default headers based on auth method
- Remove the broken TODO pattern

### Modify CLI command handlers
- Pass resolved auth config from `ClientConfig` to API client constructors

---

## Phase 9: Dev Tooling (CLI Subcommands)

**Goal**: `tasker-cli auth` subcommands for key generation and token creation.

### New File
- `tasker-client/src/bin/cli/commands/auth.rs`

### Commands
```
tasker-cli auth generate-keys [--output-dir ./keys] [--key-size 2048]
tasker-cli auth generate-token --permissions tasks:create,tasks:read [--subject my-service] [--private-key ./keys/private.pem] [--expiry-hours 24]
tasker-cli auth show-permissions   # List all known permissions
tasker-cli auth validate-token --token <JWT> --public-key ./keys/public.pem
```

### Implementation
- Key generation: Use `rsa` crate to generate RSA keypair, write PEM files
- Token generation: Use `JwtAuthenticator::generate_worker_token()` (will become `generate_token()`)
- Show permissions: Print the `Permission` enum values with descriptions
- Validate token: Decode and display claims

---

## Phase 10: Observability

**Goal**: Audit logging and security metrics.

### Modify auth middleware
- Add structured tracing events for auth success/failure:
  ```rust
  info!(subject = %ctx.subject, method = ?ctx.auth_method, permissions = ?ctx.permissions, "authenticated");
  warn!(reason = %reason, path = %path, "authentication_failed");
  warn!(required = %perm, subject = %ctx.subject, "permission_denied");
  ```

### Security Metrics (prometheus)
- `tasker_auth_requests_total{method="jwt|api_key", result="success|failure"}`
- `tasker_auth_failures_total{reason="expired|invalid|missing|forbidden"}`
- `tasker_permission_denials_total{permission="tasks:create", endpoint="/v1/tasks"}`
- `tasker_jwt_verification_duration_seconds` (histogram)

### Modify existing metrics infrastructure
- Add counters/histograms to the existing prometheus registry pattern

---

## Phase 11: Documentation

**Goal**: Permission vocabulary reference, configuration examples, integration guides.

### New/Updated Files
- `docs/guides/api-security.md` - Main security guide
  - Permission vocabulary table
  - Configuration examples (orchestration and worker)
  - JWT payload examples for common roles
  - API key configuration examples
  - curl examples with auth headers
  - Migration guide (disabled → enabled)
- `docs/guides/auth-integration.md` - External provider integration
  - Auth0 configuration example
  - Okta configuration example
  - Keycloak configuration example
  - JWKS endpoint setup
  - Vault integration for secrets
- Update `CLAUDE.md` with security section reference

---

## Implementation Order and Dependencies

```
Phase 1 (permissions, security types) ──┐
                                         ├── Phase 2 (config expansion, TokenClaims)
                                         │        │
                                    ┌────┤        ├────┐
                                    │    │        │    │
                              Phase 3    Phase 4  │    Phase 8
                              (API keys) (JWKS)   │    (Client)
                                    │    │        │
                                    └────┼────────┘
                                         │
                                    Phase 5 (SecurityService)
                                         │
                                    ┌────┴────┐
                                    │         │
                              Phase 6         Phase 7
                              (Orch API)      (Worker API)
                                    │         │
                                    └────┬────┘
                                         │
                                    Phase 9 (Dev tooling)
                                    Phase 10 (Observability)
                                    Phase 11 (Documentation)
```

---

## Critical Files Summary

| File | Action |
|------|--------|
| `tasker-shared/src/types/permissions.rs` | CREATE |
| `tasker-shared/src/types/security.rs` | CREATE |
| `tasker-shared/src/types/security_service.rs` | CREATE |
| `tasker-shared/src/types/api_key_auth.rs` | CREATE |
| `tasker-shared/src/types/jwks.rs` | CREATE |
| `tasker-shared/src/types/auth.rs` | MODIFY (TokenClaims, validate_token) |
| `tasker-shared/src/types/mod.rs` | MODIFY (add modules) |
| `tasker-shared/src/config/tasker.rs` | MODIFY (expand AuthConfig, add ApiKeyConfig) |
| `tasker-shared/src/config/web.rs` | MODIFY (expand WebAuthConfig) |
| `tasker-orchestration/src/web/middleware/auth.rs` | REWRITE |
| `tasker-orchestration/src/web/middleware/permission.rs` | CREATE |
| `tasker-orchestration/src/web/middleware/mod.rs` | MODIFY |
| `tasker-orchestration/src/web/extractors.rs` | MODIFY (SecurityContext) |
| `tasker-orchestration/src/web/state.rs` | MODIFY (add SecurityService) |
| `tasker-orchestration/src/web/mod.rs` | MODIFY (middleware stack) |
| `tasker-orchestration/src/web/handlers/*.rs` | MODIFY (add permission checks) |
| `tasker-worker/src/web/middleware/mod.rs` | CREATE |
| `tasker-worker/src/web/middleware/auth.rs` | CREATE |
| `tasker-worker/src/web/state.rs` | MODIFY |
| `tasker-worker/src/web/mod.rs` | MODIFY |
| `tasker-worker/src/web/handlers/*.rs` | MODIFY |
| `tasker-client/src/config.rs` | MODIFY (ClientAuthConfig) |
| `tasker-client/src/api_clients/orchestration_client.rs` | MODIFY |
| `tasker-client/src/bin/cli/commands/auth.rs` | CREATE |
| `config/tasker/base/orchestration.toml` | MODIFY |
| `config/tasker/base/worker.toml` | MODIFY |

---

## Verification Plan

### Build Check
```bash
cargo build --all-features
cargo clippy --all-targets --all-features
```

### Unit Tests
```bash
cargo test --features test-messaging --lib -p tasker-shared -- permissions
cargo test --features test-messaging --lib -p tasker-shared -- security
cargo test --features test-messaging --lib -p tasker-shared -- api_key
cargo test --features test-messaging --lib -p tasker-shared -- jwks
cargo test --features test-messaging --lib -p tasker-orchestration -- middleware::auth
```

### Integration Tests (require services)
```bash
cargo test --features test-services -p tasker-orchestration -- web::auth
cargo test --features test-services -p tasker-worker -- web::auth
```

### Manual Verification
```bash
# Generate dev keys
cargo run --bin tasker-cli -- auth generate-keys --output-dir ./dev-keys

# Generate a test token
cargo run --bin tasker-cli -- auth generate-token \
  --private-key ./dev-keys/jwt-private-key.pem \
  --permissions tasks:create,tasks:read,tasks:list \
  --subject test-user

# Start server with auth enabled (test env override)
TASKER_ENV=development cargo run --bin server

# Test without auth (should 401)
curl -s http://localhost:8080/v1/tasks | jq .

# Test with token (should 200)
curl -s -H "Authorization: Bearer <token>" http://localhost:8080/v1/tasks | jq .

# Test health endpoint (always 200, no auth)
curl -s http://localhost:8080/health | jq .

# Test with API key
curl -s -H "X-API-Key: configured-key" http://localhost:8080/v1/tasks | jq .
```

### Client Verification
```bash
# Set token via env var
export TASKER_AUTH_TOKEN=<generated-token>
cargo run --bin tasker-cli -- task list

# Verify auth failure
unset TASKER_AUTH_TOKEN
cargo run --bin tasker-cli -- task list  # Should show auth error
```

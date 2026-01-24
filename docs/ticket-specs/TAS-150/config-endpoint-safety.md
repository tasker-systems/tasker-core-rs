# TAS-150: Config Endpoint Safety (SafeConfig Approach)

## Summary

Replace the current redaction-based approach for the `/config` endpoint with a whitelist-only `SafeConfig` struct. Instead of trying to redact secrets from the full config (and risking missed fields), explicitly define what is safe to expose.

---

## Problem

The current implementation in `tasker-orchestration/src/web/handlers/config.rs` serializes the full config to JSON, then applies `redact_secrets()` to strip known sensitive field names. This has fundamental weaknesses:

1. **Incomplete coverage**: New config fields with secrets won't be redacted unless `redact_secrets()` is updated
2. **Pattern-based**: Relies on field name patterns ("key", "secret", "password", "token") — custom field names may slip through
3. **Brittle**: Auth config additions (API keys array, JWKS URL with embedded credentials) require updating the redaction list
4. **Defense gap**: The TAS-169 TODO already flags that `redact_secrets()` coverage needs review after auth config changes

### Current Approach (`redact_secrets()`)

```rust
pub fn redact_secrets(value: serde_json::Value) -> (serde_json::Value, Vec<String>) {
    // Walks JSON tree, replaces values where key matches patterns like
    // "key", "secret", "password", "token", "private_key", etc.
}
```

---

## Proposed Fix: SafeConfig Whitelist

### Design Principle

"Only expose what we explicitly choose to share." Rather than removing secrets from a full dump, construct a purpose-built response struct containing only operational metadata.

### New Struct: `SafeConfigResponse`

```rust
/// Operational configuration exposed via /config endpoint.
///
/// This struct explicitly whitelists what configuration values are safe
/// to expose. Adding new fields requires conscious decision about sensitivity.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SafeConfigResponse {
    pub metadata: ConfigMetadata,
    pub orchestration: SafeOrchestrationConfig,
    pub worker: Option<SafeWorkerConfig>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SafeOrchestrationConfig {
    /// Web server port
    pub port: u16,
    /// Whether auth is enabled
    pub auth_enabled: bool,
    /// Auth verification method (public_key, jwks, none)
    pub auth_method: String,
    /// JWT issuer (non-sensitive, used for token validation)
    pub jwt_issuer: String,
    /// JWT audience
    pub jwt_audience: String,
    /// API key header name (not the keys themselves)
    pub api_key_header: String,
    /// Number of configured API keys (not their values)
    pub api_key_count: usize,
    /// Circuit breaker settings
    pub circuit_breaker: Option<SafeCircuitBreakerConfig>,
    /// Database pool sizes
    pub database_pools: SafeDatabasePoolConfig,
    /// Deployment mode
    pub deployment_mode: String,
    /// Messaging configuration (non-sensitive)
    pub messaging: SafeMessagingConfig,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SafeMessagingConfig {
    /// Backend type: "pgmq" or "rabbitmq"
    pub backend: String,
    /// Queue names owned by this component
    pub queues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SafeWorkerConfig {
    pub port: u16,
    pub auth_enabled: bool,
    pub auth_method: String,
    pub api_key_header: String,
    pub api_key_count: usize,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SafeCircuitBreakerConfig {
    pub enabled: bool,
    pub failure_threshold: u32,
    pub recovery_timeout_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SafeDatabasePoolConfig {
    pub web_api_max_connections: u32,
    pub orchestration_max_connections: u32,
}
```

### Handler Rewrite

```rust
pub async fn get_config(
    State(state): State<AppState>,
    security: SecurityContext,
) -> Result<Json<SafeConfigResponse>, ApiError> {
    require_permission(&security, Permission::SystemConfigRead)?;

    let config = &state.config;
    let auth = config.auth.as_ref();

    let response = SafeConfigResponse {
        metadata: ConfigMetadata {
            environment: state.environment().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_source: "compiled".to_string(),
        },
        orchestration: SafeOrchestrationConfig {
            port: config.port,
            auth_enabled: auth.map(|a| a.enabled).unwrap_or(false),
            auth_method: auth.map(|a| a.jwt_verification_method.clone())
                .unwrap_or_else(|| "none".to_string()),
            jwt_issuer: auth.map(|a| a.jwt_issuer.clone())
                .unwrap_or_default(),
            jwt_audience: auth.map(|a| a.jwt_audience.clone())
                .unwrap_or_default(),
            api_key_header: auth.map(|a| a.api_key_header.clone())
                .unwrap_or_else(|| "X-API-Key".to_string()),
            api_key_count: auth.map(|a| a.api_keys.len()).unwrap_or(0),
            circuit_breaker: config.resilience.as_ref().map(|r| SafeCircuitBreakerConfig {
                enabled: r.circuit_breaker_enabled,
                failure_threshold: r.circuit_breaker_failure_threshold,
                recovery_timeout_seconds: r.circuit_breaker_recovery_timeout_seconds,
            }),
            database_pools: SafeDatabasePoolConfig {
                web_api_max_connections: config.database_pools.web_api_max_connections,
                orchestration_max_connections: config.database_pools.orchestration_max_connections,
            },
            deployment_mode: format!("{:?}", config.deployment_mode),
        },
        worker: None, // Populated if worker config available
    };

    Ok(Json(response))
}
```

### Worker Endpoint

Same pattern for `tasker-worker/src/web/handlers/config.rs` — construct `SafeWorkerConfig` from the worker's config fields.

---

## Migration Path

1. Create `SafeConfigResponse` and related structs in `tasker-shared/src/types/api/orchestration.rs`
2. Rewrite the orchestration `get_config` handler to construct the response directly
3. Rewrite the worker `get_config` handler similarly
4. Remove `redact_secrets()` function (no longer needed)
5. Update OpenAPI schema (response type changes)
6. Update E2E tests that validate config endpoint response shape

---

## What NOT to Expose

Never include in SafeConfig:
- JWT private/public keys
- API key values
- Database URLs / credentials
- JWKS URL (may contain embedded auth tokens)
- Any field whose value is a secret

Safe to expose:
- Ports, timeouts, pool sizes
- Feature flags (enabled/disabled)
- Algorithm names, issuer/audience identifiers
- Counts (number of API keys, not their values)
- Non-secret deployment metadata
- Messaging backend type (pgmq or rabbitmq) and queue names:
  - Orchestration: orchestration-owned queues only
  - Worker: namespace queues for the worker instance

---

## Files to Modify

| File | Action |
|------|--------|
| `tasker-shared/src/types/api/orchestration.rs` | Add SafeConfig structs, remove `redact_secrets` |
| `tasker-orchestration/src/web/handlers/config.rs` | Rewrite handler |
| `tasker-worker/src/web/handlers/config.rs` | Rewrite handler |
| `tasker-orchestration/tests/web/config_tests.rs` | Update response assertions |
| `tasker-worker/tests/web/config_tests.rs` | Update response assertions |

---

## Tests

1. **Positive test**: Authenticated request returns SafeConfig with expected fields
2. **Negative test**: Verify response does NOT contain any key/secret/token/password values
3. **Schema test**: Response matches OpenAPI schema
4. **Property test** (optional): Fuzz the config with random secrets in various fields, verify none leak through SafeConfig construction

---

## Verification

```bash
cargo build --all-features
cargo clippy --all-targets --all-features
cargo test --all-features --package tasker-orchestration -- config
cargo test --all-features --package tasker-worker -- config
```

## Priority

**High** - The current redaction approach is known to have coverage gaps (TAS-169 TODO). A whitelist approach eliminates the entire class of "missed redaction" bugs.

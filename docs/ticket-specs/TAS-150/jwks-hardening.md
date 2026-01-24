# TAS-150: JWKS Hardening

## Summary

Four hardening improvements for the JWKS (JSON Web Key Set) key store implementation in `tasker-shared/src/types/jwks.rs`. These address thundering herd on refresh, stale cache behavior, SSRF via JWKS URL, and algorithm pinning.

---

## Issue 9: Thundering Herd on JWKS Cache Miss

### Problem

When the JWKS cache is stale or a key ID is not found, every concurrent request triggers `refresh_keys()`. Under high concurrency, this causes a thundering herd of identical JWKS endpoint requests.

### Current Code (`jwks.rs:86-111`)

```rust
pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
    // Try cache first
    {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.as_ref() {
            if entry.fetched_at.elapsed() < self.refresh_interval {
                if let Some(key) = entry.keys.get(kid) {
                    return Ok(key.clone());
                }
            }
        }
    }

    // Cache miss or stale: refresh (N concurrent callers all hit this)
    self.refresh_keys().await?;
    // ...
}
```

### Fix

Use a `tokio::sync::Mutex` (or `Notify` + flag) to coalesce concurrent refresh requests. Only one refresh executes; others wait for its result.

```rust
pub struct JwksKeyStore {
    cache: Arc<RwLock<Option<JwksCacheEntry>>>,
    refresh_lock: Arc<tokio::sync::Mutex<()>>,  // Coalescing lock
    jwks_url: String,
    refresh_interval: Duration,
    client: reqwest::Client,
}

pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
    // Try cache first (fast path)
    {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.as_ref() {
            if entry.fetched_at.elapsed() < self.refresh_interval {
                if let Some(key) = entry.keys.get(kid) {
                    return Ok(key.clone());
                }
            }
        }
    }

    // Acquire refresh lock - only one caller refreshes
    let _guard = self.refresh_lock.lock().await;

    // Double-check after acquiring lock (another caller may have refreshed)
    {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.as_ref() {
            if entry.fetched_at.elapsed() < self.refresh_interval {
                if let Some(key) = entry.keys.get(kid) {
                    return Ok(key.clone());
                }
            }
        }
    }

    // Actually refresh
    self.refresh_keys().await?;
    // ...
}
```

### Impact

- Under 100 concurrent requests with a stale cache, 1 JWKS fetch instead of 100
- No additional latency for the winning caller; others wait on mutex (same net latency as before)

---

## Issue 10: Stale Cache Fallback on Refresh Failure

### Problem

When `refresh_keys()` fails (network error, endpoint down), the existing cache entry is still valid but expired. The current code returns an error immediately, rejecting all requests until the JWKS endpoint recovers.

### Current Behavior

```rust
async fn refresh_keys(&self) -> Result<(), AuthError> {
    let response = self.client.get(&self.jwks_url).send().await.map_err(|e| {
        error!(...);
        AuthError::ConfigurationError(...)  // Immediate failure
    })?;
    // ...
}
```

If refresh fails, `get_key()` propagates the error. The cache still holds the old (expired) keys, but they're never consulted on failure.

### Fix

Implement stale-while-revalidate: if refresh fails but a stale cache exists, use the stale cache with a warning. Add a `max_stale_duration` config to bound how long stale keys are acceptable.

```rust
pub struct JwksKeyStore {
    // ...
    max_stale_duration: Duration,  // e.g., 5 minutes past refresh_interval
}

pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
    // Try cache (fresh)
    // ...

    // Refresh
    match self.refresh_keys().await {
        Ok(()) => { /* use fresh cache */ }
        Err(refresh_err) => {
            // Check if stale cache is within acceptable bounds
            let cache = self.cache.read().await;
            if let Some(entry) = cache.as_ref() {
                let staleness = entry.fetched_at.elapsed();
                if staleness < self.refresh_interval + self.max_stale_duration {
                    warn!(
                        staleness_secs = staleness.as_secs(),
                        "JWKS refresh failed, using stale cache"
                    );
                    if let Some(key) = entry.keys.get(kid) {
                        return Ok(key.clone());
                    }
                }
            }
            return Err(refresh_err);
        }
    }
    // ...
}
```

### Configuration

Add to `AuthConfig`:

```toml
[orchestration.web.auth]
jwks_max_stale_seconds = 300  # Accept stale JWKS keys for up to 5 minutes after refresh failure
```

Default: 300 seconds (5 minutes). This provides a reasonable window for JWKS endpoint outages without accepting arbitrarily old keys.

---

## Issue 12: SSRF via JWKS URL

### Problem

The JWKS URL is fetched by the server at runtime. If an attacker can influence the `jwks_url` config value (e.g., via environment variable injection, config file manipulation, or a future admin API), they can make the server fetch arbitrary URLs — including internal services, cloud metadata endpoints (`169.254.169.254`), or file:// URIs.

### Current Code (`jwks.rs:62-68`)

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .build()?;
```

No URL validation or restriction is applied.

### Fix

1. **URL allowlist validation** at construction time:

```rust
impl JwksKeyStore {
    fn validate_jwks_url(url: &str) -> Result<(), AuthError> {
        let parsed = url::Url::parse(url).map_err(|e| {
            AuthError::ConfigurationError(format!("Invalid JWKS URL: {e}"))
        })?;

        // Must be HTTPS in production
        if parsed.scheme() != "https" {
            // Allow http only for testing (configurable)
            if parsed.scheme() != "http" {
                return Err(AuthError::ConfigurationError(
                    "JWKS URL must use HTTPS".to_string()
                ));
            }
        }

        // Block private/internal IPs
        if let Some(host) = parsed.host_str() {
            if Self::is_private_host(host) {
                return Err(AuthError::ConfigurationError(format!(
                    "JWKS URL points to private/internal address: {host}"
                )));
            }
        }

        Ok(())
    }

    fn is_private_host(host: &str) -> bool {
        // Block cloud metadata, localhost, link-local, private ranges
        let blocked = [
            "169.254.169.254",  // AWS/GCP metadata
            "metadata.google.internal",
            "localhost",
            "127.0.0.1",
            "::1",
            "0.0.0.0",
        ];
        if blocked.contains(&host) {
            return true;
        }
        // Check private IP ranges
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            return match ip {
                std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
                std::net::IpAddr::V6(v6) => v6.is_loopback(),
            };
        }
        false
    }
}
```

2. **Disable redirects** in the HTTP client:

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .redirect(reqwest::redirect::Policy::none())  // No redirects
    .build()?;
```

3. **Add `jwks_url_allow_http` config flag** (default: false) for test environments.

### Configuration

```toml
[orchestration.web.auth]
jwks_url = "https://auth.example.com/.well-known/jwks.json"
jwks_url_allow_http = false  # Only enable for local testing
```

---

## Issue 14: Algorithm Pinning in JWKS Validation

### Problem

The JWKS validation in `validate_with_jwks()` (`security_service.rs:228`) hardcodes `RS256`:

```rust
let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
```

However, the JWKS key entry has an `alg` field that is read but not enforced. If the JWKS endpoint serves a key with `alg: "none"` or a weaker algorithm, it won't be caught at key load time. Additionally, if the code is later changed to honor the JWK `alg` field, algorithm confusion attacks become possible.

### Fix

1. **Validate algorithm at key load time** in `refresh_keys()`:

```rust
for jwk in &jwks.keys {
    if jwk.kty != "RSA" {
        continue;
    }

    // Enforce algorithm allowlist
    if let Some(alg) = &jwk.alg {
        if !ALLOWED_ALGORITHMS.contains(&alg.as_str()) {
            warn!(kid = ?jwk.kid, alg = %alg, "Rejecting JWKS key with disallowed algorithm");
            continue;
        }
    }
    // ...
}

const ALLOWED_ALGORITHMS: &[&str] = &["RS256", "RS384", "RS512"];
```

2. **Make allowed algorithms configurable**:

```toml
[orchestration.web.auth]
jwt_allowed_algorithms = ["RS256"]  # Only accept RS256 tokens
```

3. **Match key algorithm during validation** when the JWK specifies one:

```rust
async fn validate_with_jwks(&self, jwks: &JwksKeyStore, token: &str) -> Result<SecurityContext, AuthError> {
    let header = jsonwebtoken::decode_header(token)?;

    // Use the algorithm from config, not from the token header
    let algorithm = self.config.jwt_allowed_algorithms
        .first()
        .and_then(|a| parse_algorithm(a))
        .unwrap_or(jsonwebtoken::Algorithm::RS256);

    let mut validation = jsonwebtoken::Validation::new(algorithm);
    validation.algorithms = self.config.jwt_allowed_algorithms
        .iter()
        .filter_map(|a| parse_algorithm(a))
        .collect();
    // ...
}
```

### Impact

- Prevents algorithm confusion attacks
- Explicit allowlist means new algorithm support requires conscious opt-in
- Defense-in-depth: even if JWKS endpoint is compromised, only allowed algorithms are accepted

---

## Configuration Additions

```toml
[orchestration.web.auth]
# JWKS hardening
jwks_max_stale_seconds = 300
jwks_url_allow_http = false
jwt_allowed_algorithms = ["RS256"]
```

Add corresponding fields to `AuthConfig` struct in `tasker-shared/src/config/tasker.rs`.

---

## Files to Modify

| File | Changes |
|------|---------|
| `tasker-shared/src/types/jwks.rs` | Thundering herd lock, stale cache fallback, SSRF validation |
| `tasker-shared/src/services/security_service.rs` | Algorithm pinning in `validate_with_jwks` |
| `tasker-shared/src/config/tasker.rs` | New config fields |
| `tasker-shared/Cargo.toml` | `url` crate dependency for URL parsing |

---

## Verification

```bash
cargo build --all-features
cargo clippy --all-targets --all-features
cargo test --all-features --package tasker-shared

# JWKS-specific tests:
# - Mock JWKS server returning valid/invalid/rotated keys
# - Concurrent cache miss test (verify single fetch)
# - Stale cache test (mock network failure after initial load)
# - SSRF test (verify blocked URLs rejected at construction)
# - Algorithm allowlist test (verify non-RS256 keys rejected)
```

## Priority

**Medium-High** - JWKS is only active when `jwt_verification_method = "jwks"`. The static key path (public_key) is not affected. However, when JWKS is enabled, these are real attack surfaces.

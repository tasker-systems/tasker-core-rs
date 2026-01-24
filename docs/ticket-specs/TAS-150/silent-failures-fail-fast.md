# TAS-150: Silent Failures & Fail-Fast Violations

## Summary

Three silent failure patterns in the auth implementation violate our fail-fast-and-loudly principles for unsafe defaults. Each allows an attacker to bypass or weaken security through malformed input or configuration edge cases.

---

## Issue 6: Timing Attack on API Key Comparison

### Problem

`ApiKeyRegistry::validate_key()` in `tasker-shared/src/types/api_key_auth.rs:40-52` uses `HashMap::get(key)` which performs standard string equality. This is vulnerable to timing attacks where an attacker can measure response time to incrementally guess valid API keys character-by-character.

### Current Code

```rust
pub fn validate_key(&self, key: &str) -> Result<SecurityContext, AuthError> {
    match self.keys.get(key) {  // Standard equality - timing-vulnerable
        Some(entry) => Ok(SecurityContext { ... }),
        None => Err(AuthError::InvalidToken("Invalid API key".to_string())),
    }
}
```

### Fix

Use constant-time comparison. Since we need HashMap for O(1) lookup by key, we can't avoid the HashMap lookup itself, but we should add a constant-time verification step after the initial hash bucket is found. A practical approach:

1. Add `subtle` crate dependency (`subtle = "2"`)
2. Hash all API keys with a fast keyed hash (HMAC-SHA256 or BLAKE3) at registry construction time
3. At validation time, hash the incoming key the same way and compare using `subtle::ConstantTimeEq`

```rust
use subtle::ConstantTimeEq;
use sha2::{Sha256, Digest};

pub struct ApiKeyRegistry {
    // Store SHA-256 hash → entry mapping
    keys: HashMap<[u8; 32], ApiKeyEntry>,
}

impl ApiKeyRegistry {
    pub fn from_config(configs: &[ApiKeyConfig]) -> Self {
        let keys = configs.iter().map(|c| {
            let hash = Sha256::digest(c.key.as_bytes());
            let entry = ApiKeyEntry { ... };
            (hash.into(), entry)
        }).collect();
        Self { keys }
    }

    pub fn validate_key(&self, key: &str) -> Result<SecurityContext, AuthError> {
        let input_hash: [u8; 32] = Sha256::digest(key.as_bytes()).into();
        // Iterate all keys with constant-time comparison
        for (stored_hash, entry) in &self.keys {
            if input_hash.ct_eq(stored_hash).into() {
                return Ok(SecurityContext { ... });
            }
        }
        Err(AuthError::InvalidToken("Invalid API key".to_string()))
    }
}
```

**Trade-off**: Linear scan over all keys (O(n)) vs HashMap O(1). With typical key counts (<100), this is negligible. For very large registries (>1000 keys), consider a keyed hash approach where the HashMap lookup uses a non-secret-derived index and the final comparison is constant-time.

### Dependencies

- Add `subtle = "2"` to `tasker-shared/Cargo.toml`
- Add `sha2 = "0.10"` to `tasker-shared/Cargo.toml` (or use existing if available)

### Tests

- Unit test: valid key returns correct context
- Unit test: invalid key returns error
- Integration test: response time for valid vs invalid keys is statistically indistinguishable (timing harness with multiple samples)

---

## Issue 7: Non-UTF-8 Header Silent Fallthrough

### Problem

In both auth middlewares (`tasker-orchestration/src/web/middleware/auth.rs:46-50` and `tasker-worker/src/web/middleware/auth.rs:42-53`), when `to_str()` fails on the Authorization or API key header (non-UTF-8 bytes), the result is `None` — which silently falls through to "missing credentials" (401).

While this isn't a direct bypass (still returns 401), it's a silent failure that could mask attacks using binary header injection. The failure reason "missing" is misleading - the credential IS present but malformed.

### Current Code

```rust
let bearer_token = request
    .headers()
    .get("authorization")
    .and_then(|h| h.to_str().ok())  // Non-UTF-8 → None (silent)
    .and_then(|s| s.strip_prefix("Bearer "))
    .map(|t| t.to_string());
```

### Fix

Distinguish between "no header present" and "header present but malformed":

```rust
let auth_header = request.headers().get("authorization");
let bearer_token = match auth_header {
    Some(h) => match h.to_str() {
        Ok(s) => s.strip_prefix("Bearer ").map(|t| t.to_string()),
        Err(_) => {
            warn!("Authorization header contains non-UTF-8 bytes");
            security_metrics::auth_failures_total()
                .add(1, &[KeyValue::new("reason", "malformed")]);
            return Err(ApiError::auth_error("Malformed Authorization header"));
        }
    },
    None => None,
};

// Same pattern for API key header
let api_key = match request.headers().get(api_key_header.as_str()) {
    Some(h) => match h.to_str() {
        Ok(s) => Some(s.to_string()),
        Err(_) => {
            warn!("API key header contains non-UTF-8 bytes");
            security_metrics::auth_failures_total()
                .add(1, &[KeyValue::new("reason", "malformed")]);
            return Err(ApiError::auth_error("Malformed API key header"));
        }
    },
    None => None,
};
```

### Impact

- Explicit rejection with correct failure reason
- Metrics track "malformed" as distinct from "missing" and "invalid"
- No silent fallthrough on binary injection attempts

### Files

- `tasker-orchestration/src/web/middleware/auth.rs`
- `tasker-worker/src/web/middleware/auth.rs`

---

## Issue 8: God-Mode Legacy Claims in Orchestration Middleware

### Problem

In `tasker-orchestration/src/web/middleware/auth.rs:124-134`, after successful authentication, legacy `TokenClaims` are injected with `worker_namespaces: vec!["*"]`. Any handler extracting these legacy claims gets unrestricted namespace access regardless of the actual token's permissions.

### Current Code

```rust
// Also insert legacy WorkerClaims for backward compat with existing extractors
let legacy_claims = tasker_shared::types::auth::TokenClaims {
    sub: ctx.subject.clone(),
    worker_namespaces: vec!["*".to_string()],  // GOD MODE
    iss: ctx.issuer.clone().unwrap_or_default(),
    aud: String::new(),
    exp: ctx.expires_at.unwrap_or(0),
    iat: chrono::Utc::now().timestamp(),
    permissions: ctx.permissions.clone(),
};
request.extensions_mut().insert(legacy_claims);
```

### Fix Options

**Option A: Derive namespaces from permissions** (Preferred)

Map permission strings to namespace restrictions. If the token has `tasks:*`, derive the relevant namespaces. If no namespace-related permissions exist, use empty vec (deny-all for namespace checks).

```rust
let worker_namespaces = if ctx.permissions.contains(&"*".to_string()) {
    vec!["*".to_string()]
} else {
    // Only grant wildcard if explicitly permitted
    vec![]
};
```

**Option B: Remove legacy claims entirely**

Audit all handlers that extract `TokenClaims` and migrate them to use `SecurityContext` directly. Then remove the legacy injection.

**Option C: Mark as deprecated with explicit opt-in**

Add a config flag `legacy_claims_enabled` (default: false) and only inject when explicitly opted in, with a deprecation warning in logs on first use.

### Recommended Approach

Option B is the cleanest. The legacy claims were added for backward compatibility during the TAS-150 migration. A search for `TokenClaims` extractors in handlers will reveal what needs updating. If no handlers use it, the block can simply be removed.

### Investigation Steps

1. `grep -r "TokenClaims" tasker-orchestration/src/web/handlers/` to find consumers
2. If none found, remove the legacy injection block
3. If found, migrate those handlers to `SecurityContext`
4. Remove the `TokenClaims` extension insertion

---

## Verification

After fixing all three issues:

```bash
cargo build --all-features
cargo clippy --all-targets --all-features
cargo test --all-features --package tasker-shared
cargo test --all-features --package tasker-orchestration
cargo test --all-features --package tasker-worker
```

## Priority

**High** - These are fail-fast violations in security-critical code paths. Items 6 and 8 represent real attack surface; item 7 is defense-in-depth.

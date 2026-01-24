# TAS-150: Rate Limiting & Gateway Signaling

## Summary

Tasker's API is fully stateless — it tracks nothing about client connections (IPs, headers, cookies) that would enable per-actor rate limiting. In expected deployments, API access is fronted by ALB/NLB/API gateways. Rather than implementing application-level rate limiting, we should signal auth failure context through response headers so upstream gateways can make informed rate-limiting decisions.

---

## Problem

Without any rate limiting or auth failure signaling:
- An attacker can brute-force API keys or JWT tokens at line speed
- The application has no way to slow down or block repeated failures
- Gateway services (ALB, NLB, API Gateway, Cloudflare) have no application-level signal about auth failure severity

## Design Principles

1. **Boundary of responsibility**: Tasker is the application; rate limiting belongs at the infrastructure layer (gateway/proxy)
2. **Stateless signaling**: Response headers provide actionable metadata without Tasker tracking client state
3. **Progressive response**: Headers signal escalating severity so gateways can respond proportionally
4. **No breaking changes**: Additional response headers are backward-compatible

---

## Proposed Solution: Auth Failure Response Headers

### Response Headers on 401/403

When authentication or authorization fails, include headers that gateways can use for rate-limiting decisions:

| Header | Value | Description |
|--------|-------|-------------|
| `X-Auth-Failure-Reason` | `missing`, `expired`, `invalid`, `malformed`, `forbidden` | Machine-readable failure category |
| `X-Auth-Failure-Severity` | `low`, `medium`, `high` | Suggested severity for gateway action |
| `Retry-After` | seconds (integer) | Standard header suggesting client backoff |

### Severity Mapping

| Failure Reason | Severity | Retry-After | Rationale |
|---------------|----------|-------------|-----------|
| `missing` | `low` | - | Likely a misconfigured client, not an attack |
| `expired` | `low` | - | Token needs refresh, not malicious |
| `invalid` | `high` | `60` | Possible brute-force attempt |
| `malformed` | `high` | `60` | Binary injection or fuzzing |
| `forbidden` | `medium` | `5` | Valid auth but wrong permissions |

### Implementation

Add header injection in both auth middlewares after determining the failure reason:

```rust
// In ApiError construction, attach metadata for the response layer
impl ApiError {
    pub fn auth_error_with_context(message: &str, reason: &str, severity: &str) -> Self {
        let mut error = Self::auth_error(message);
        error.headers.insert(
            "X-Auth-Failure-Reason".to_string(),
            reason.to_string(),
        );
        error.headers.insert(
            "X-Auth-Failure-Severity".to_string(),
            severity.to_string(),
        );
        if severity == "high" {
            error.headers.insert(
                "Retry-After".to_string(),
                "60".to_string(),
            );
        }
        error
    }
}
```

### ApiError Extension

The `ApiError` struct needs to support custom response headers:

```rust
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    pub error_type: String,
    /// Additional response headers (e.g., auth failure context)
    pub headers: HashMap<String, String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(ErrorBody { ... })).into_response();
        for (key, value) in &self.headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_str(key),
                HeaderValue::from_str(value),
            ) {
                response.headers_mut().insert(name, val);
            }
        }
        response
    }
}
```

### Middleware Changes

```rust
// Example from authenticate_request:
} else {
    warn!("Request missing authentication credentials");
    security_metrics::auth_failures_total().add(1, &[KeyValue::new("reason", "missing")]);
    return Err(ApiError::auth_error_with_context(
        "Missing authentication credentials",
        "missing",
        "low",
    ));
};
```

---

## Gateway Integration Patterns

### AWS ALB/NLB

ALB can't inspect custom response headers for rate limiting directly, but:
- The `Retry-After` header is respected by well-behaved clients
- AWS WAF rules can be configured to count 401/403 responses and rate-limit by source IP
- Custom headers can be logged to CloudWatch for alerting

### AWS API Gateway

API Gateway supports response-based throttling:
- Use a Lambda authorizer that reads `X-Auth-Failure-Severity` and returns throttling metadata
- Configure usage plans with per-client rate limits

### Cloudflare / Generic Reverse Proxy

Most reverse proxies can be configured to:
1. Count responses with `X-Auth-Failure-Severity: high`
2. Rate-limit the source IP after N high-severity failures
3. Block entirely after sustained attacks

### Nginx Example

```nginx
# Map auth failure severity to rate limit zone
map $upstream_http_x_auth_failure_severity $auth_limit_key {
    "high"   $binary_remote_addr;
    default  "";
}

limit_req_zone $auth_limit_key zone=auth_failures:10m rate=5r/m;

location /v1/ {
    proxy_pass http://tasker-api;
    limit_req zone=auth_failures burst=10 nodelay;
}
```

---

## Optional: Sliding Window Counter (Future Enhancement)

If application-level awareness is needed without gateway dependency:

```rust
/// In-memory sliding window counter for auth failures by source.
/// NOT for rate limiting decisions — only for metrics and alerting.
pub struct AuthFailureCounter {
    windows: DashMap<IpAddr, SlidingWindow>,
    window_size: Duration,
    alert_threshold: u32,
}
```

This would:
- Track failure counts per source IP (from `X-Forwarded-For` or socket addr)
- Emit metrics when thresholds are crossed
- NOT block requests (that's the gateway's job)
- Provide `X-Auth-Failure-Count` header for gateway awareness

**Decision**: Defer this to a future ticket. The header-based signaling approach is sufficient for initial deployment.

---

## Files to Modify

| File | Changes |
|------|---------|
| `tasker-shared/src/types/web.rs` | Add `headers` field to ApiError, update IntoResponse |
| `tasker-orchestration/src/web/middleware/auth.rs` | Use `auth_error_with_context` for failures |
| `tasker-worker/src/web/middleware/auth.rs` | Same |
| `tasker-orchestration/src/web/middleware/permission.rs` | Add severity header on 403 |

---

## Tests

1. **Unit test**: ApiError with headers produces correct response headers
2. **Integration test**: 401 response includes `X-Auth-Failure-Reason` header
3. **Integration test**: 403 response includes `X-Auth-Failure-Severity: medium`
4. **Integration test**: Invalid token → `Retry-After: 60` header present
5. **Integration test**: Missing credentials → no `Retry-After` header

---

## Verification

```bash
cargo build --all-features
cargo clippy --all-targets --all-features
cargo test --all-features -- auth_failure_headers

# Manual verification:
curl -v http://localhost:8080/v1/tasks  # Should show X-Auth-Failure-Reason: missing
curl -v -H "Authorization: Bearer invalid" http://localhost:8080/v1/tasks  # Should show Retry-After: 60
```

---

## Priority

**Medium** - This is defense-in-depth. Without gateway integration, the headers are informational only. However, they're cheap to implement and provide immediate value for any deployment with a reverse proxy.

## Non-Goals

- We are NOT implementing IP-based blocking at the application level
- We are NOT tracking client state across requests
- We are NOT implementing token bucket / leaky bucket algorithms
- We are NOT breaking API backward compatibility (headers are additive)

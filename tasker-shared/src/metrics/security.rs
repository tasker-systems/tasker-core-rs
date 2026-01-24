//! # Security Metrics (TAS-150)
//!
//! OpenTelemetry metrics for authentication and authorization:
//! - Authentication request counters (by method and result)
//! - Authentication failure counters (by reason)
//! - Permission denial counters (by permission)
//! - JWT verification duration histograms
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics::security::*;
//! use opentelemetry::KeyValue;
//!
//! // Record successful JWT authentication
//! auth_requests_total().add(1, &[
//!     KeyValue::new("method", "jwt"),
//!     KeyValue::new("result", "success"),
//! ]);
//!
//! // Record permission denial
//! permission_denials_total().add(1, &[
//!     KeyValue::new("permission", "tasks:create"),
//! ]);
//! ```

use opentelemetry::metrics::{Counter, Histogram, Meter};
use std::sync::OnceLock;

/// Lazy-initialized meter for security metrics
static SECURITY_METER: OnceLock<Meter> = OnceLock::new();

/// Get or initialize the security meter
fn meter() -> &'static Meter {
    SECURITY_METER.get_or_init(|| opentelemetry::global::meter_provider().meter("tasker-security"))
}

// Counters

/// Total authentication requests processed
///
/// Labels:
/// - method: jwt, api_key
/// - result: success, failure
pub fn auth_requests_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.auth.requests.total")
        .with_description("Total authentication requests processed")
        .build()
}

/// Total authentication failures
///
/// Labels:
/// - reason: expired, invalid, missing, forbidden
pub fn auth_failures_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.auth.failures.total")
        .with_description("Total authentication failures")
        .build()
}

/// Total permission denials
///
/// Labels:
/// - permission: the required permission (e.g., tasks:create)
pub fn permission_denials_total() -> Counter<u64> {
    meter()
        .u64_counter("tasker.permission.denials.total")
        .with_description("Total permission denial events")
        .build()
}

// Histograms

/// JWT token verification duration in milliseconds
///
/// Labels:
/// - result: success, failure
pub fn jwt_verification_duration() -> Histogram<f64> {
    meter()
        .f64_histogram("tasker.auth.jwt.verification.duration")
        .with_description("JWT token verification duration in milliseconds")
        .with_unit("ms")
        .build()
}

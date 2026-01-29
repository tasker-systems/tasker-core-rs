# TAS-177 gRPC Implementation Code Review

This document provides a comprehensive code review of the gRPC implementation for the Tasker Core system, identifying critical security issues, implementation inconsistencies, and architectural recommendations.

## Critical Issues (Must Fix Before PR)

### 1. Worker gRPC Services Missing Authentication 🔴

**Location:** `tasker-worker/src/grpc/services/templates.rs`, `tasker-worker/src/grpc/services/config.rs`

**Issue:** The worker's gRPC template and config services have NO authentication checks, while their REST equivalents require `worker:templates_read` and `worker:config_read` permissions. This creates an authentication bypass vulnerability.

**Fix Required:** Add `AuthInterceptor` and permission checks to worker gRPC services.

### 2. Port Number Documentation Inconsistency 🔴

**Issue:** All documentation shows ports 9090/9100, but actual configuration uses 9190/9191/9290:

| Document | Port Shown | Actual Default |
|----------|------------|----------------|
| CLAUDE.md | 9090, 9100 | 9190, 9191 |
| api-security.md | 9090, 9100 | 9190, 9191 |
| deployment-patterns.md | 9090, 9100 | 9190, 9191 |
| crate-architecture.md | 9090 | 9190 |

**Fix Required:** Update all documentation to reflect actual port values.

---

## High Priority Issues

### 3. Error Information Leakage

**Issue:** Internal error details are exposed to clients via `format!("Failed to...: {}", e)`. This leaks database errors, SQL fragments, and internal state.

**Recommendation:** Return generic error messages to clients, log full details server-side.

### 4. Auth Error Messages Too Detailed

**Issue:** Authentication failures expose validation specifics like "RSA key parsing error: {details}".

**Recommendation:** Return generic "Invalid credentials" messages.

### 5. Cross-Transport Behavioral Differences

**Issues:**
- `cancel_task`: gRPC returns `success: false`, REST returns `400 Bad Request`
- `DuplicateTask`: gRPC uses `INVALID_ARGUMENT`, REST returns `409 Conflict` (should use `ALREADY_EXISTS`)

**Recommendation:** Standardize error handling across transports to ensure consistent client behavior.

---

## Medium Priority Issues

### 6. Dead Code in Auth Interceptor

**Issue:** Redundant `is_enabled()` check creates unreachable branch (lines 99-101 in `auth.rs`).

**Recommendation:** Remove redundant code path to improve maintainability.

### 7. Permission Names Leaked in Error Messages

**Issue:** "Permission denied: requires TasksCreate" reveals permission model structure.

**Recommendation:** Use generic permission denied messages without exposing internal permission names.

### 8. Worker SharedApiServices Missing

**Issue:** Worker crate doesn't have a proper shared services layer like orchestration.

**Recommendation:** Implement consistent service architecture across all crates.

---

## Architectural Recommendations

### 1. Decouple gRPC from web-api Feature

**Current Issue:** gRPC requires `web-api`, preventing standalone gRPC deployments.

**Recommendation:** Make gRPC independent of REST API features to support pure gRPC deployments.

### 2. Create UnifiedWorkerClient

**Current Issue:** Missing transport abstraction for worker client.

**Recommendation:** Implement a unified client interface that can use either REST or gRPC transparently.

### 3. Add --transport CLI Flag

**Current Issue:** Testing different transports requires creating configuration profiles.

**Recommendation:** Add command-line flag for quick transport switching during development and testing.

### 4. Standardize Timestamp Representation

**Current Issue:** Mix of `google.protobuf.Timestamp` and ISO 8601 strings across the API.

**Recommendation:** Establish consistent timestamp format across all endpoints and transports.

---

## Positive Findings

The following aspects of the implementation demonstrate good engineering practices:

1. ✅ **Transport abstraction trait** is clean and usable
2. ✅ **"Fail loudly" principle** properly applied in conversions
3. ✅ **Profile system** provides good UX for different deployment scenarios
4. ✅ **Feature gating** is consistent and properly propagated across the workspace
5. ✅ **Health endpoints** correctly allow unauthenticated access
6. ✅ **Config service** properly sanitizes sensitive data before exposure
7. ✅ **Proto design** has excellent documentation and 1:1 enum mapping

---

## Security Assessment Summary

### Critical Security Issues
- **Authentication bypass** in worker gRPC services
- **Information leakage** through detailed error messages
- **Permission enumeration** through error responses

### Recommendations Priority
1. **Immediate:** Fix authentication bypass (Issue #1)
2. **Before release:** Implement generic error responses (Issues #3, #4, #7)
3. **Next iteration:** Address architectural improvements

### Overall Security Posture
The gRPC implementation maintains most security controls from the REST API, but the authentication bypass in worker services represents a significant vulnerability that must be addressed before production deployment.

---

## Implementation Quality

The gRPC implementation demonstrates solid architectural patterns and maintains consistency with existing codebase conventions. The transport abstraction design is particularly well-executed, providing a foundation for future multi-transport support.

Key strengths include proper feature gating, comprehensive proto documentation, and consistent error handling patterns. The main areas for improvement focus on security hardening and cross-transport consistency.

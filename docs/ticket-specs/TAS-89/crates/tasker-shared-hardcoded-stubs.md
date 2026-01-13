# TAS-89: Hardcoded Values & Stub Analysis - tasker-shared

**Analysis Date**: 2026-01-12
**Risk Level**: LOW

---

## Executive Summary

The tasker-shared crate is **remarkably clean** with respect to hardcoded stubs. No silent bugs were found that return fake data in production code paths.

**Key Finding**: 3 guard structs are fully implemented but never called anywhere - orphaned code from TAS-41.

---

## Findings

### Category 1: Orphaned Code (Unreachable)

| File:Line | Name | Risk |
|-----------|------|------|
| `state_machine/guards.rs:282` | `StepCanBeEnqueuedForOrchestrationGuard` | MEDIUM |
| `state_machine/guards.rs:317` | `StepCanBeCompletedFromOrchestrationGuard` | MEDIUM |
| `state_machine/guards.rs:352` | `StepCanBeFailedFromOrchestrationGuard` | MEDIUM |

**Analysis**:
- All three implement `StateGuard<WorkflowStep>` with real logic
- Marked with `#[allow(dead_code)]`
- **Zero call sites found** across entire workspace
- Likely TAS-41 orchestration artifacts that were superseded

**Impact**: No runtime impact - code is never executed. But indicates design drift.

**Recommendation**: Remove or add `#[deprecated]` with explanation.

---

### Category 2: Intentionally Unused (Documented)

| File:Line | Name | Status |
|-----------|------|--------|
| `metrics/mod.rs:102` | `init_opentelemetry_meter()` | ✅ Documented |

**Analysis**:
- Comment explains: "Currently unused - Prometheus text exporter used instead"
- Kept for potential future OTLP metrics export
- Correctly marked `#[allow(dead_code)]`

**Recommendation**: No action needed - well documented.

---

### Category 3: Test Factories (Expected)

| Location | Count | Status |
|----------|-------|--------|
| `models/factories/core.rs` | 6 instances | ✅ Expected |
| `models/factories/states.rs` | 2 instances | ✅ Expected |
| `macros.rs` | 2 instances | ✅ Expected |

**Analysis**: Factory methods used across multiple test modules. Correctly suppressed.

---

## Positive Findings

✅ **Zero hardcoded boolean stubs** (`return true` / `return false`)
✅ **Zero hardcoded numeric stubs** (`return 0`, `return 100.0`)
✅ **Zero empty collection returns** used as stubs
✅ **70+ metrics functions** - all properly initialized
✅ **Validation functions** - all fully implemented
✅ **Configuration system** - V2 canonical, no stubs

---

## Summary

| Category | Count | Risk |
|----------|-------|------|
| Hardcoded stubs | 0 | N/A |
| Silent bugs (used) | 0 | N/A |
| Orphaned code | 3 | MEDIUM |
| Intentional unused | 1 | LOW |
| Test factories | 10 | N/A |

**Overall Assessment**: Excellent codebase hygiene. The only action item is cleaning up the 3 orphaned guard structs.

---

## Action Items

### Priority 1 (Medium)
- [ ] Investigate `StepCanBeEnqueuedForOrchestrationGuard` and related guards
- [ ] Git blame to understand when/why added
- [ ] Remove if truly orphaned, or add deprecation notice

### No Action Needed
- Metrics backup implementation (documented)
- Test factories (expected pattern)

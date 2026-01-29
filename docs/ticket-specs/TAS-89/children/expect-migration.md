# TAS-89-7: Complete #[expect] Migration

**Parent**: TAS-89 (Codebase Evaluation)
**Type**: Tech Debt
**Priority**: Low
**Effort**: 2-3 hours

---

## Summary

Per TAS-58 linting standards, `#[expect]` with explicit reasons is preferred over `#[allow]`. There are 96 instances of `#[allow(dead_code)]` across the workspace that should be migrated.

---

## Background

### Why `#[expect]` over `#[allow]`?

1. **Requires explicit reasoning** - Documents why the suppression exists
2. **Fails if suppression becomes unnecessary** - CI catches when dead code is actually removed
3. **Better auditability** - Clear documentation for future maintainers

### TAS-58 Standard

From TAS-58 linting standards:
> Use `#[expect(lint_name, reason = "...")]` instead of `#[allow]` for justified suppressions.

---

## Scope

### Distribution by Crate

| Crate | Count | Primary Pattern |
|-------|-------|-----------------|
| `workers/rust` | 28 | API compatibility (handler configs) |
| `tasker-shared` | 24 | Factories, test helpers, state machine |
| `tasker-orchestration` | 16 | Core services, event systems |
| `tasker-worker` | 10 | Worker infrastructure |
| `tests/common` | 6 | Test manager helpers |
| `tasker-pgmq/tests` | 4 | Test helper functions |
| `workers/{python,typescript}` | 4 | FFI bridge runtimes |
| `workers/ruby` | 2 | FFI bridge extension |
| **Total** | **96** | |

---

## Migration Patterns

### Pattern 1: API Compatibility (28 instances)

**Location**: `workers/rust/src/step_handlers/*.rs`

```rust
// Before
#[allow(dead_code)] // api compatibility
config: StepHandlerConfig,

// After
#[expect(dead_code, reason = "API compatibility - config available for future handler enhancements")]
config: StepHandlerConfig,
```

---

### Pattern 2: Test Helpers (16 instances)

**Locations**: `tests/common/`, `tasker-pgmq/tests/`, `tasker-shared/tests/`

```rust
// Before
#[allow(dead_code)]
pub fn create_task_request_with_options(...) { }

// After
#[expect(dead_code, reason = "Test helper used by integration tests across multiple modules")]
pub fn create_task_request_with_options(...) { }
```

---

### Pattern 3: Factory Methods (6 instances)

**Location**: `tasker-shared/src/models/factories/`

```rust
// Before
#[allow(dead_code)]
impl TaskFactory {
    pub fn with_namespace(...) { }
}

// After
#[expect(dead_code, reason = "Factory builder method for test fixture creation")]
pub fn with_namespace(...) { }
```

---

### Pattern 4: FFI Runtime (4 instances)

**Locations**: `workers/python/src/bridge.rs`, `workers/typescript/src-rust/bridge.rs`

```rust
// Before
#[allow(dead_code)]
pub runtime: tokio::runtime::Runtime,

// After
#[expect(dead_code, reason = "Runtime kept alive for async FFI task lifetime management")]
pub runtime: tokio::runtime::Runtime,
```

---

### Pattern 5: Future Enhancement Fields (varies)

**Various locations**

```rust
// Before
#[allow(dead_code)]
orchestration_core: Arc<OrchestrationCore>,

// After
#[expect(dead_code, reason = "Reserved for future orchestration integration")]
orchestration_core: Arc<OrchestrationCore>,
```

---

## Execution Plan

### Phase 1: High-Visibility Files (covered in TAS-89-1)
- [x] Step handlers (28 instances)

### Phase 2: Test Infrastructure
- [ ] `tests/common/` (6 instances)
- [ ] `tasker-pgmq/tests/` (4 instances)
- [ ] `tasker-shared/tests/` (varies)

### Phase 3: Core Crates
- [ ] `tasker-shared/src/` (24 instances)
- [ ] `tasker-orchestration/src/` (16 instances)
- [ ] `tasker-worker/src/` (10 instances)

### Phase 4: FFI Bindings
- [ ] `workers/python/` (2 instances)
- [ ] `workers/typescript/` (2 instances)
- [ ] `workers/ruby/` (2 instances)

---

## Files to Modify

All files containing `#[allow(dead_code)]` - see dead-code-analysis.md for complete list.

---

## Acceptance Criteria

- [ ] All `#[allow(dead_code)]` converted to `#[expect]` with reasons
- [ ] All `#[allow(unused)]` converted to `#[expect]` with reasons
- [ ] Reasons are descriptive and explain why the suppression is needed
- [ ] CI passes (no new warnings)
- [ ] `cargo clippy --all-targets --all-features` clean

---

## Testing

```bash
# Verify no remaining #[allow(dead_code)]
grep -r "#\[allow(dead_code)\]" --include="*.rs" .

# Should return empty or only build artifacts

# Verify compilation
cargo build --all-features

# Verify clippy clean
cargo clippy --all-targets --all-features
```

---

## Risk Assessment

**Risk**: None
- Lint annotation only, no runtime change
- If an `#[expect]` becomes unnecessary, CI will flag it (this is good!)
- Purely mechanical transformation

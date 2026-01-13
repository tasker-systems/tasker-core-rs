# TAS-89: Evaluation Framework

**Purpose**: Define consistent criteria for evaluating code quality across all crates and workers.

---

## Evaluation Dimensions

### 1. Code Quality

**Naming**
- [ ] Functions/methods use clear, descriptive names
- [ ] Variables use meaningful names (no single letters except loop counters)
- [ ] Types/structs/classes follow language conventions (PascalCase, snake_case, etc.)
- [ ] Constants are SCREAMING_SNAKE_CASE (Rust) or appropriate for language

**Structure**
- [ ] Functions are appropriately sized (<50 lines guideline)
- [ ] Files have clear single responsibility
- [ ] Module organization is logical and navigable
- [ ] No deeply nested code (>3 levels of indentation)

**Complexity**
- [ ] Cyclomatic complexity is reasonable
- [ ] No god functions/methods that do too much
- [ ] Clear separation of concerns
- [ ] Appropriate use of abstractions (not too many, not too few)

**Error Handling**
- [ ] Errors are handled explicitly, not swallowed
- [ ] Error types are appropriate and informative
- [ ] Recovery paths are clear
- [ ] No panic! in library code (Rust), no bare raise (Ruby/Python)

---

### 2. Documentation Quality

**Inline Comments**
- [ ] Complex logic is explained
- [ ] No obvious/redundant comments ("increment counter")
- [ ] TODO/FIXME comments have ticket references
- [ ] Edge cases and assumptions are documented

**Module/File Level**
- [ ] Each module has a doc comment explaining purpose
- [ ] Public APIs have complete documentation
- [ ] Examples provided where helpful
- [ ] Cross-references to related modules

**Rust Specifics**
- [ ] `//!` module docs at top of lib.rs/mod.rs
- [ ] `///` doc comments on all public items
- [ ] `# Examples` section in doc comments
- [ ] `# Errors` and `# Panics` sections where applicable

**Ruby Specifics**
- [ ] YARD documentation on public methods
- [ ] Class-level documentation
- [ ] @param, @return, @raise tags where appropriate

**Python Specifics**
- [ ] Docstrings on all public functions/classes
- [ ] Type hints on function signatures
- [ ] Module-level docstrings

**TypeScript Specifics**
- [ ] JSDoc comments on exports
- [ ] Type definitions are self-documenting
- [ ] README in module directories where helpful

---

### 3. Test Coverage

**Unit Tests**
- [ ] Core logic has unit tests
- [ ] Edge cases are tested
- [ ] Error paths are tested
- [ ] Tests are isolated and don't depend on external state

**Integration Tests**
- [ ] Cross-component interactions are tested
- [ ] Database interactions are tested (where applicable)
- [ ] FFI boundaries are tested

**Property-Based Tests**
- [ ] Invariants are tested with proptest (Rust)
- [ ] Input fuzzing where appropriate

**Test Quality**
- [ ] Tests have descriptive names
- [ ] Arrange/Act/Assert pattern is clear
- [ ] No flaky tests
- [ ] Test data is meaningful, not random strings

---

### 4. Standard Compliance

**Tasker Core Tenets** (see principles/tasker-core-tenets.md)
- [ ] Defense in Depth - multiple protection layers
- [ ] Event-Driven with Polling Fallback - hybrid pattern
- [ ] Composition Over Inheritance - mixins/traits
- [ ] Cross-Language Consistency - aligned APIs
- [ ] Actor-Based Decomposition - clear boundaries
- [ ] State Machine Rigor - atomic transitions
- [ ] Audit Before Enforce - track, don't block
- [ ] Pre-Alpha Freedom - appropriate breaking changes
- [ ] PostgreSQL as Foundation - database coordination
- [ ] Bounded Resources - no unbounded channels

**API Convergence** (see workers/api-convergence-matrix.md)
- [ ] Handler signatures match across languages
- [ ] Result factories are consistent
- [ ] Registry APIs are aligned
- [ ] Specialized handlers follow pattern

**Linting Standards (TAS-58)**
- [ ] No `#[allow(...)]` without migration to `#[expect]`
- [ ] All public types implement Debug
- [ ] Clippy warnings are addressed

**Channel Guidelines (TAS-51)**
- [ ] All MPSC channels are bounded
- [ ] Channel sizes are configured via TOML
- [ ] No `unbounded_channel()` usage

---

### 5. Technical Debt

**TODO/FIXME Audit**
- [ ] TODOs have ticket references or rationale
- [ ] No stale TODOs (>6 months without progress)
- [ ] FIXMEs are prioritized for resolution
- [ ] XXX comments are rare and justified

**Lint Suppressions**
- [ ] `#[allow(dead_code)]` is justified and documented
- [ ] Suppressions are migrated to `#[expect(lint, reason = "...")]`
- [ ] No blanket suppressions at crate level

**Dead Code**
- [ ] Unused functions are removed
- [ ] Commented-out code is removed
- [ ] Feature-gated code is clearly marked

---

### 6. Cross-Language Consistency

**API Surface**
- [ ] Handler signatures match convergence matrix
- [ ] Result types are equivalent
- [ ] Error types are consistent
- [ ] Registry operations align

**Patterns**
- [ ] Composition pattern used consistently
- [ ] Lifecycle hooks available where documented
- [ ] Batch processing follows standard pattern
- [ ] Decision handlers use aligned API

**FFI Boundaries**
- [ ] Type conversions are explicit
- [ ] Errors are handled at boundaries
- [ ] Memory management is correct
- [ ] Telemetry crosses boundaries correctly

---

## Rating Scale

Each dimension is rated:

| Rating | Description |
|--------|-------------|
| **Excellent** | Exceeds standards, exemplary code |
| **Good** | Meets standards with minor issues |
| **Needs Review** | Multiple issues requiring attention |
| **Critical** | Significant problems requiring immediate action |

---

## Evaluation Template

```markdown
# {Component} Evaluation

## Summary
- **Overall Rating**: [Excellent/Good/Needs Review/Critical]
- **Priority Issues**: [count]
- **Quick Wins**: [count]

## Ratings by Dimension

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Code Quality | | |
| Documentation | | |
| Test Coverage | | |
| Standard Compliance | | |
| Technical Debt | | |
| Cross-Language Consistency | | |

## What's Done Well
-

## Areas Needing Review
-

## TODO/FIXME Audit
| Location | Type | Status | Action |
|----------|------|--------|--------|
| | | | |

## Inline Fixes Applied
-

## Recommendations
1.
```

---

## Process

1. **Read AGENTS.md** for the crate/worker (if exists)
2. **Sample key files** - entry points, public APIs, core logic
3. **Run checks** - `cargo make check-{language}`
4. **Search for patterns** - TODOs, #[allow], complexity
5. **Evaluate against framework** - score each dimension
6. **Document findings** - use template
7. **Apply trivial fixes** - #[expect] migration, dead code
8. **Create tickets** - for significant work

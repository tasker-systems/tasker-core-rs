# TAS-58: Rust Standards Compliance Implementation

**Status**: In Progress (Phase 2)
**Created**: 2025-10-28
**Last Updated**: 2025-10-28
**Branch**: `jcoletaylor/tas-58-rust-standards-update`

## Overview

Implement Microsoft Universal Guidelines and Rust API Guidelines across the tasker-core workspace through incremental lint enablement.

**Key References**:
- [Microsoft Rust Guidelines](https://microsoft.github.io/rust-guidelines/)
- [Rust API Guidelines Checklist](https://rust-lang.github.io/api-guidelines/checklist.html)
- [Rust Clippy Lints](https://rust-lang.github.io/rust-clippy/master/)

## Strategy

**Incremental Enablement**: Start with essential lints, fix violations, then enable next tier. This prevents overwhelming the codebase and allows focused, systematic improvements.

## Implementation Phases

### Phase 1: Essential Lints ✅ COMPLETE

**Enabled Lints**:
```toml
[workspace.lints.rust]
missing_debug_implementations = "warn"
redundant_imports = "warn"
unsafe_op_in_unsafe_fn = "warn"

[workspace.lints.clippy]
correctness = { level = "warn", priority = -1 }   # Bug prevention
suspicious = { level = "warn", priority = -1 }    # Likely bugs
cargo = { level = "warn", priority = -1 }         # Cargo metadata
dbg_macro = "warn"                                # No debug prints
undocumented_unsafe_blocks = "warn"               # Safety docs
```

**Accomplishments**:
- ✅ Workspace lint configuration established
- ✅ All 8 crates inherit workspace lints
- ✅ Cargo metadata complete (license, repository, readme, keywords, categories)
- ✅ Debug implementations added to pgmq-notify (5 types)
- ✅ Auto-fixable issues resolved
- ✅ Redundant feature name fixed (`sqlx-support` → `sqlx`)
- ✅ All tests passing

**Baseline**: 156 warnings (down from 2,460 initially - 94% reduction)

**Remaining Phase 1 Issues**:
- ⚠️ Redundant imports: 11 (auto-fixable)
- ⚠️ Missing Debug: 143 (systematic fix needed)
- ⚠️ Other: 2

### Phase 2: Complete Essential Lints 🔄 IN PROGRESS

**Current Task**: Fix remaining Phase 1 violations before enabling additional lints

**Subtasks**:
1. ✅ Create tracking document (this file)
2. ⏳ Auto-fix 11 redundant imports
3. ⏳ Fix 143 missing Debug implementations
   - Generate comprehensive list
   - Categorize: simple derive vs manual implementation
   - Apply fixes systematically by crate
4. ⏳ Validate all tests pass
5. ⏳ Update CLAUDE.md with Phase 2 completion status

**Target**: Zero warnings with Phase 1 lints enabled

### Phase 3: Additional Compiler Lints 📋 PLANNED

**Lints to Enable**:
```toml
ambiguous_negative_literals = "warn"
redundant_lifetimes = "warn"
trivial_numeric_casts = "warn"
unused_lifetimes = "warn"
```

**Expected Impact**: Low (these are mostly style/cleanup)

**Process**:
1. Uncomment lints in root Cargo.toml
2. Run clippy to assess violations
3. Auto-fix where possible
4. Manual fix remaining issues
5. Validate tests pass

### Phase 4: Safety & Correctness Restriction Lints 📋 PLANNED

**Lints to Enable**:
```toml
allow_attributes_without_reason = "warn"
as_underscore = "warn"
map_err_ignore = "warn"
unnecessary_safety_comment = "warn"
unnecessary_safety_doc = "warn"
```

**Expected Impact**: Moderate (~70 violations for `allow_attributes_without_reason`)

**Process**:
1. Identify all `#[allow]` attributes in production code
2. Convert to `#[expect(lint_name, reason = "...")]` with meaningful reasons
3. Review safety comments for accuracy

### Phase 5: Complexity & Performance 📋 PLANNED

**Lints to Enable**:
```toml
complexity = { level = "warn", priority = -1 }
perf = { level = "warn", priority = -1 }
```

**Expected Impact**: High (~400+ violations)

**Process**:
- Review complexity warnings for genuine improvements
- Address performance hints where beneficial
- Accept some complexity as necessary for domain logic

### Phase 6: Style Consistency 📋 PLANNED

**Lints to Enable**:
```toml
style = { level = "warn", priority = -1 }
```

**Expected Impact**: Moderate (~200+ violations)

**Process**:
- Apply consistent code style patterns
- Balance between consistency and readability

### Phase 7: Full Pedantic Mode 📋 FUTURE

**Lints to Enable**:
```toml
pedantic = { level = "warn", priority = -1 }
```

**Expected Impact**: High (1000+ violations)

**Process**:
- Reserved for future comprehensive review
- May require significant refactoring
- Consider long-term maintenance vs benefit trade-offs

### Phase 8: Remaining Restriction Lints 📋 FUTURE

**Lints to Enable** (16 total):
```toml
assertions_on_result_states = "warn"
clone_on_ref_ptr = "warn"
deref_by_slicing = "warn"
disallowed_script_idents = "warn"
empty_drop = "warn"
empty_enum_variants_with_brackets = "warn"
empty_structs_with_brackets = "warn"
fn_to_numeric_cast_any = "warn"
if_then_some_else_none = "warn"
redundant_type_annotations = "warn"
renamed_function_params = "warn"
semicolon_outside_block = "warn"
string_to_string = "warn"
unneeded_field_pattern = "warn"
unused_result_ok = "warn"
```

**Expected Impact**: Low to Moderate per lint

**Process**:
- Enable individually or in small groups
- Assess value vs effort for each
- Some may remain commented based on cost/benefit

## Progress Tracking

### Overall Statistics

| Metric | Initial | Phase 1 | Phase 2 Target | Phase 3+ Target |
|--------|---------|---------|----------------|-----------------|
| Total Warnings | 2,460 | 156 | 0 | 0 (per phase) |
| Reduction | - | 94% | 100% | N/A |
| Tests Passing | ✅ | ✅ | ✅ | ✅ |

### By Crate (Phase 1 Completion)

| Crate | Initial | Phase 1 | Phase 2 Target |
|-------|---------|---------|----------------|
| pgmq-notify | 185 | ~0 | 0 |
| tasker-shared | 572 | 49 | 0 |
| tasker-client | 82 | 2 | 0 |
| tasker-orchestration | 449 | 50 | 0 |
| tasker-worker | 380 | 17 | 0 |
| tasker-worker-rust | 344 | 36 | 0 |
| tasker-worker-rb | 50 | ~0 | 0 |

### Warning Types (Phase 1 Remaining)

| Type | Count | Strategy |
|------|-------|----------|
| Redundant imports | 11 | Auto-fix |
| Missing Debug | 143 | Systematic implementation |
| Other | 2 | Manual review |

## Implementation Guidelines

### Adding Debug Implementations

**Simple Derive** (when all fields implement Debug):
```rust
#[derive(Debug)]
pub struct MyType {
    field1: String,
    field2: usize,
}
```

**Manual Implementation** (for types with non-Debug fields like PgPool, AtomicU64):
```rust
impl std::fmt::Debug for MyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MyType")
            .field("field1", &self.field1)
            .field("pg_pool", &"PgPool") // Show type name for non-Debug fields
            .field("atomic_counter", &self.counter.load(Ordering::Relaxed))
            .finish()
    }
}
```

### Handling #[allow] → #[expect]

**Before**:
```rust
#[allow(clippy::too_many_arguments)]
pub fn complex_function(...) { }
```

**After**:
```rust
#[expect(clippy::too_many_arguments, reason = "Required for FFI compatibility with Ruby bindings")]
pub fn complex_function(...) { }
```

**Reason Guidelines**:
- Be specific about why the lint is suppressed
- Reference architectural decisions (e.g., "Actor pattern requires owned Arc")
- Note future improvements if applicable (e.g., "TODO: Refactor in TAS-XX")
- Keep concise but meaningful

### Running Incremental Checks

```bash
# Check current phase compliance
export DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test
cargo clippy --workspace --lib --all-features

# Auto-fix simple issues
cargo clippy --fix --workspace --lib --all-features --allow-dirty

# Run tests
cargo test --workspace --all-features

# Check specific crate
cargo clippy --package <crate-name> --lib --all-features
```

## Documentation Updates

### Files Modified

**Configuration**:
- `Cargo.toml` (root) - Workspace lint configuration
- `Cargo.toml` (all 8 crates) - Lint inheritance and metadata

**Code**:
- `pgmq-notify/src/*.rs` - Debug implementations, import cleanup
- Additional files per phase

**Documentation**:
- `CLAUDE.md` - Standards section added
- `docs/ticket-specs/TAS-58.md` - This tracking document

### CLAUDE.md Integration

The main CLAUDE.md includes a comprehensive "Rust Standards and Linting (TAS-58)" section documenting:
- Linting configuration
- Key standards
- Compliance status
- Development workflow
- References

## Success Criteria

### Phase 1
- ✅ Essential lints enabled
- ✅ Cargo metadata complete
- ✅ Tests passing
- ⏳ Zero warnings with Phase 1 lints (156 remaining)

### Phase 2
- ⏳ All Debug implementations added
- ⏳ Zero warnings with Phase 1 lints
- ⏳ Ready to enable Phase 3 lints

### Overall Success (All Phases)
- Zero clippy warnings with all enabled lints
- All tests passing
- Comprehensive documentation
- Clear patterns for future contributions

## Future Considerations

### Documentation Enhancement (Deferred)
- Error documentation (`missing_errors_doc` - 132 violations)
- Panic documentation (`missing_panics_doc` - 27 violations)
- Technical term backticks (`doc_markdown` - 93 violations)
- Rustdoc examples for public APIs

These are valuable but require significant time investment. Recommended approach:
- Add incrementally as code is naturally modified
- Create separate tickets for systematic documentation improvements
- Focus on most-used public APIs first

### Performance & Complexity (Evaluate Per Case)
- Cast warnings (376 total) - Often safe in context
- Complexity warnings - Domain logic complexity may be necessary
- Clone warnings - Performance vs ergonomics trade-off

## Notes

**Philosophy**: The goal is not to achieve zero warnings at all costs, but to establish a high-quality baseline with meaningful lint checks that catch real issues. Some warnings may be legitimately suppressed with `#[expect]` and clear reasoning.

**Maintenance**: As new code is added, it must comply with enabled lints. This ensures quality remains high without requiring retroactive cleanup.

**Flexibility**: Lint configuration can be adjusted based on team feedback and practical experience. Document any changes in this file.

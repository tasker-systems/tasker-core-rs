# Unused Dependencies Analysis

**Date**: 2025-10-25
**Analyzed**: 62 unique dependencies
**Potentially Unused**: 16 dependencies (25%)

## Summary

This analysis used a heuristic approach to identify dependencies that may be unused by searching for `use` statements in the codebase. The script found 16 potentially unused dependencies that warrant investigation.

## Methodology

```bash
./scripts/analyze-unused-deps.sh
```

The script:
1. Extracts all dependencies from `Cargo.toml` files
2. Collects all `use` statements from Rust source files
3. Matches dependency names against used modules
4. Reports dependencies with no matching `use` statements

**Limitations**: This approach may miss dependencies used via:
- Derive macros (`#[derive(...)]`)
- Re-exports from other crates
- Trait implementations without explicit `use`
- Build dependencies
- Transitive requirements

## Findings

### Definitely Unused (Verified with `cargo tree`)

These dependencies do NOT appear in the dependency tree at all:

| Dependency | Status | Recommendation |
|------------|--------|----------------|
| `factori` | Not in tree | **REMOVE** - Test factory library we don't use |
| `mockall` | Not in tree | **REMOVE** - Mocking library we don't use |
| `proptest` | Not in tree | **REMOVE** - Property-based testing library we don't use |
| `insta` | Not in tree | **REMOVE** - Snapshot testing library we don't use |

**Estimated Savings**: ~4-6 dependencies and their transitive dependencies.

### Likely Unused (Requires Manual Verification)

| Dependency | Module Name | Notes |
|------------|-------------|-------|
| `cargo-llvm-cov` | cargo_llvm_cov | Coverage tool - should only be in root workspace dev-deps |
| `rust_decimal` | rust_decimal | We use `bigdecimal` instead |
| `tokio-test` | tokio_test | Test utilities - need to verify if used in tests |
| `sysinfo` | sysinfo | System information - likely unused |

### Needs Investigation (May Be Used Indirectly)

| Dependency | Module Name | Possible Usage |
|------------|-------------|----------------|
| `axum-extra` | axum_extra | May be used for extra axum features |
| `crossbeam` | crossbeam | May be used by other crates, or for channels |
| `dirs` | dirs | May be used for config directory discovery |
| `is-terminal` | is_terminal | May be used for CLI color detection |
| `opentelemetry-semantic-conventions` | opentelemetry_semantic_conventions | Likely needed for telemetry attributes |
| `toml` | toml | Used by `config` crate, but may need direct dep |
| `url` | url | Used in types without explicit `use` |

### Special Cases

| Dependency | Module Name | Notes |
|------------|-------------|-------|
| `tasker-core` | tasker_core | Integration test package - used differently than typical deps |

## Recommended Actions

### Phase 1: Remove Obviously Unused (Low Risk)

Remove dependencies verified as not in tree:

```bash
# Search and remove from all Cargo.toml files
find . -name "Cargo.toml" -exec grep -l "factori\|mockall\|proptest\|insta" {} \;
```

Expected files to modify:
- Individual crate `Cargo.toml` files with these deps

**Expected Impact**:
- Reduce dependency count by 4
- Reduce compilation time for these crates and their transitive deps
- Smaller lock file

### Phase 2: Investigate and Remove Likely Unused (Medium Risk)

For each of:
- `cargo-llvm-cov`
- `rust_decimal`
- `tokio-test`
- `sysinfo`

1. Search for usage: `grep -r "rust_decimal" --include="*.rs" .`
2. Check dependency tree: `cargo tree -i rust_decimal`
3. Try removing and run tests: `cargo test --all-features`

### Phase 3: Careful Review of Indirect Usage (Higher Risk)

For dependencies like `toml`, `url`, `opentelemetry-semantic-conventions`:

1. May be used in struct definitions without `use`
2. May be required by other dependencies
3. May be needed for trait implementations

**Process**:
1. Create test branch
2. Remove one dependency
3. Run `cargo check --all-features`
4. Run full test suite
5. If passes, commit; if fails, investigate error

## Estimated Impact

**Conservative Estimate** (removing only verified unused):
- **Dependencies removed**: 4-6
- **Compilation time saved**: 5-10%
- **Binary size reduction**: 2-5%

**Aggressive Estimate** (removing all 16 flagged):
- **Dependencies removed**: 10-16 (if verification confirms)
- **Compilation time saved**: 15-25%
- **Binary size reduction**: 5-10%

## Next Steps

1. **Immediate**: Remove `factori`, `mockall`, `proptest`, `insta` (verified unused)
2. **Short-term**: Investigate `cargo-llvm-cov`, `rust_decimal`, `tokio-test`, `sysinfo`
3. **Medium-term**: Careful review of indirect dependencies
4. **CI Integration**: Add periodic unused dependency checks

## Related Work

- **TAS-56**: CI optimization work (native binary execution)
- **Current PR #44**: CI improvements (caching + job splitting)
- Tool: `./scripts/analyze-unused-deps.sh` for future analysis

## References

- Script: `./scripts/analyze-unused-deps.sh`
- Cargo tree: `cargo tree -i <package>` to see reverse dependencies
- Unused features: `cargo-udeps` (requires nightly) - more comprehensive but nightly-only

# CI Migration to cargo-make

This document outlines the plan to align GitHub Actions workflows with the cargo-make build system for consistency between local development and CI.

---

## Overview

The goal is to have CI workflows use the same `cargo make` commands that developers use locally, ensuring:
- Consistent behavior between local and CI environments
- Single source of truth for build/test commands
- Easier debugging when CI fails (developers can reproduce locally)

---

## Migration Status

### Phase 1: Framework Test Workflows ✅ COMPLETED

These workflows have been updated to use cargo-make:

#### `test-ruby-framework.yml` ✅

**Implemented**:
```yaml
- name: Run Ruby framework tests
  run: cargo make test-ci
```

**Changes Made**:
- [x] Added `test-ci` task to `workers/ruby/Makefile.toml` with JUnit XML output
- [x] Updated workflow to call `cargo make test-ci`
- [x] Output: `../../target/ruby-framework-results.xml`

---

#### `test-python-framework.yml` ✅

**Implemented**:
```yaml
- name: Run Python framework tests
  run: cargo make test-ci
```

**Changes Made**:
- [x] Added `test-ci` task to `workers/python/Makefile.toml` with `--junitxml` flag
- [x] Updated workflow to call `cargo make test-ci`
- [x] Output: `../../target/python-framework-results.xml`

---

#### `test-typescript-framework.yml` ✅

**Implemented**:
```yaml
- name: Run quality checks (lint + typecheck)
  run: |
    cargo make lint
    cargo make typecheck

- name: Run TypeScript unit tests
  run: cargo make test-ci
```

**Changes Made**:
- [x] Added `test-ci` task to `workers/typescript/Makefile.toml` for unit tests
- [x] Consolidated lint and typecheck into single step using `cargo make lint` + `cargo make typecheck`
- [x] Updated unit test step to use `cargo make test-ci`
- [x] FFI integration tests remain unchanged (runtime-specific handling with continue-on-error)
- [x] Output: `../../target/typescript-unit-results.txt`

---

### Phase 2: Build Workflows (Low Priority)

#### `build-workers.yml`

**Current State**: Already partially uses cargo-make for TypeScript. Individual language builds use native commands.

**Recommendation**: Keep as-is for now. The workflow handles:
- Artifact caching and upload (CI-specific)
- Build order dependencies
- Cross-language coordination

**Future Opportunity**: Could simplify to `cargo make build` if artifact handling is moved to a separate step.

---

### Phase 3: Quality Workflows (No Change Needed)

#### `code-quality.yml`

**Current State**: Uses direct cargo commands which map to cargo-make tasks:

| Current Command | Equivalent cargo-make Task |
|-----------------|---------------------------|
| `cargo fmt --all -- --check` | `cargo make rust-fmt-check` |
| `cargo clippy --all-targets --all-features` | `cargo make rust-clippy` |
| `cargo audit` | `cargo make rust-audit` |
| `cargo doc --no-deps --document-private-items` | `cargo make rust-docs` |

**Recommendation**: Keep as-is. The workflow:
- Has specific caching strategies for each tool
- Uploads artifacts (coverage, docs)
- Uses matrix strategy for parallelization

These CI-specific concerns are better handled in the workflow than abstracted into cargo-make.

---

### Phase 4: Integration Test Workflows (No Change)

#### `test-integration.yml`

**Recommendation**: Keep as-is. This workflow:
- Manages service lifecycle (start/stop native services)
- Handles complex test environment setup
- Requires CI-specific health checks and timeouts

**Future Opportunity**: Create `cargo make test-e2e-local` for developers to run similar tests locally.

---

## Workflows That Should NOT Use cargo-make

| Workflow | Reason |
|----------|--------|
| `ci.yml` | DAG orchestration layer - pure GitHub Actions coordination |
| `build-postgres.yml` | Docker image building - infrastructure concern |
| `ci-success.yml` | Branch protection gate - GitHub Actions requirement |

---

## New cargo-make Tasks Added ✅

### For CI Integration

Added to `workers/python/Makefile.toml`:
```toml
[tasks.test-ci]
description = "Run tests with CI output format (JUnit XML)"
dependencies = ["setup", "build-extension"]
script = '''
mkdir -p ../../target
uv run pytest tests/ -v --tb=short --junitxml=../../target/python-framework-results.xml
'''
```

Added to `workers/ruby/Makefile.toml`:
```toml
[tasks.test-ci]
description = "Run tests with CI output format (JUnit XML)"
dependencies = ["setup", "compile"]
script = '''
mkdir -p ../../target
bundle exec rspec spec/ \
  --format documentation \
  --format RspecJunitFormatter \
  --out ../../target/ruby-framework-results.xml
'''
```

Added to `workers/typescript/Makefile.toml`:
```toml
[tasks.test-ci]
description = "Run tests with CI output format (text logs for artifact upload)"
dependencies = ["install"]
script = '''
source scripts/load-env.sh
mkdir -p ../../target
bun test tests/unit/ 2>&1 | tee ../../target/typescript-unit-results.txt
'''
```

---

## Implementation Order

1. ✅ **Completed**: Added `test-ci` tasks to all worker Makefile.toml files
2. ✅ **Completed**: Updated TypeScript workflow (lint, typecheck, unit tests)
3. ✅ **Completed**: Updated Python workflow
4. ✅ **Completed**: Updated Ruby workflow
5. 🔄 **Pending**: CI run to validate changes in GitHub Actions

---

## Rollback Plan

If any workflow migration causes issues:
1. Revert the workflow file to previous version
2. Keep the cargo-make task (useful for local development)
3. Document the incompatibility for future reference

---

## Success Criteria

- [x] All framework test workflows use `cargo make` commands
- [x] Developers can run `cargo make test-ci` locally to reproduce CI behavior
- [ ] No increase in CI failure rate after migration (pending CI run)
- [ ] CI run times remain within 10% of previous times (pending CI run)

---

## Related Documentation

- [Build Tooling](./tooling.md) - Full cargo-make documentation
- [Development Patterns](./development-patterns.md) - General development workflows

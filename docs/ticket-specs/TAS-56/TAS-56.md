# CI Stabilization: Optimize Build Times and Resource Usage

## Problem Statement

**Current Issues:**

1. **CI Failures from Infrastructure Issues**
   * First failure: "Out of disk space" on GitHub Actions runner
   * Second failure: Linker bus errors (`ld terminated with signal 7 [Bus error]`) during test compilation
   * Multiple test targets failing: `sql_function_validation_tests`, `constants_test`, `integration_tests`
2. **Root Causes:**
   * Memory pressure during parallel compilation of large Rust workspace
   * Long compilation times for Rust codebase with heavy dependencies
   * Docker image builds adding significant overhead
   * No build artifact caching between jobs/runs
   * sccache previously disabled due to GitHub Actions issues
3. **Current Pain Points:**
   * CI runs take 15-20+ minutes
   * High GitHub Actions cost from repeated full rebuilds
   * Transient infrastructure failures require manual re-runs
   * No ability to split workloads without paying full compile cost multiple times

## Proposed Solutions

### Option 1: GitHub Actions Cache for Build Artifacts

**What:** Cache the entire `target/` directory and Cargo registry between workflow runs

**Implementation:**

```yaml
- name: Cache Rust Build Artifacts
  uses: actions/cache@v3
  with:
    path: |
      ~/.cargo/bin/
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      target/
    key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    restore-keys: |
      ${{ runner.os }}-cargo-
```

**Benefits:**

* Subsequent runs reuse compiled dependencies
* Only recompile changed code
* Dramatically faster CI runs (potentially 5-10x speedup on cache hits)
* Enables job splitting without duplicating build costs
* Works with existing GitHub Actions infrastructure

**Trade-offs:**

* Initial run still slow (cold cache)
* Cache invalidation on Cargo.lock changes (expected)
* Uses GitHub Actions cache quota (10GB limit, but manageable)

### Option 2: Split Jobs by Test Type

**What:** Separate unit tests (fast, no Docker) from integration tests (slow, requires Docker)

**Implementation:**

```yaml
jobs:
  build-and-unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/cache@v3  # Cache from Option 1
        # ... cache config ...
      - run: cargo build --all-features
      - run: cargo nextest run --all-features --lib  # Unit tests only
      - run: cargo clippy --all-features
      - run: cargo test --doc --all-features

  integration-tests:
    needs: build-and-unit-tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/cache@v3  # Restore cache from build job
        # ... cache config ...
      - run: docker-compose -f docker/docker-compose.test.yml up -d
      - run: cargo nextest run --test '*integration*'
      - run: cargo nextest run --test 'e2e*'
      - run: docker-compose -f docker/docker-compose.test.yml down
```

**Benefits:**

* Reduces memory pressure by splitting compilation load
* Fast feedback on unit tests (no Docker startup overhead)
* Integration tests run in parallel after build completes
* Easier to identify which test type is failing
* Can skip integration tests for documentation-only changes

**Trade-offs:**

* Slightly more complex workflow configuration
* Integration tests wait for unit tests (sequential dependency)
* Two runner instances required (vs one monolithic job)

## Combined Implementation Strategy

**Phase 1: Add Caching (Low Risk)**

1. Add `actions/cache` to existing workflow
2. Verify cache hit rates and speedup
3. Monitor for cache-related issues

**Phase 2: Split Jobs (If Phase 1 Insufficient)**

1. Create separate jobs for unit vs integration tests
2. Configure cache sharing between jobs
3. Set `needs:` dependency for integration job
4. Update CI badge/documentation

## Success Criteria

**Performance:**

- [ ] CI runs complete in <10 minutes on cache hits
- [ ] No more "out of disk space" failures
- [ ] No more linker bus errors from memory pressure

**Cost:**

- [ ] 50%+ reduction in GitHub Actions minutes consumed
- [ ] Cache hit rate >80% on typical PR workflow

**Reliability:**

- [ ] Zero transient infrastructure failures over 30-day period
- [ ] All tests pass consistently on re-runs

**Developer Experience:**

- [ ] Faster feedback loop for PRs
- [ ] Clear separation between unit and integration test failures
- [ ] CI failures clearly actionable (not infrastructure noise)

## Alternative Options Considered

**Option 3: Nextest Partitioning**

* Split tests across multiple runners with `--partition`
* Rejected: Adds complexity, still requires full build on each runner
* May revisit if Options 1+2 insufficient

**Option 4: Pre-built Docker Images**

* Build and publish base images separately
* Rejected: High setup overhead, more moving parts
* Better suited for mature/stable projects

**Option 5: Re-enable sccache**

* Distributed compilation cache
* Rejected: Previously disabled due to GitHub Actions cache service issues
* See `docs/sccache-configuration.md` for historical context

## Implementation Notes

**Cargo Configuration:**

* Keep `CARGO_INCREMENTAL=0` for build purity
* May enable selectively for test phase if needed
* Current `CARGO_BUILD_JOBS=4` is reasonable for GitHub runners

**Docker Optimization:**

* Investigate whether all integration tests truly need Docker
* Consider testcontainers alternatives for faster startup
* Future work: separate Docker-heavy tests from lighter integration tests

**Monitoring:**

* Track CI run times in GitHub Actions insights
* Monitor cache hit rates and storage usage
* Alert on >3 consecutive failures (infrastructure vs code issue)

## Related Work

* [TAS-50](https://linear.app/tasker-systems/issue/TAS-50/configuration-generator): Configuration cleanup (completed) - reduced configuration surface area
* **Docs**: `docs/sccache-configuration.md` - historical sccache issues
* **CI Config**: `.github/workflows/` - current CI configuration

## Next Steps

1. Create feature branch `jcoletaylor/tas-56-ci-optimization`
2. Implement Phase 1 (caching) with minimal changes
3. Validate on PR #44 re-runs
4. Measure and document speedup
5. Proceed to Phase 2 if needed

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-56/ci-stabilization-optimize-build-times-and-resource-usage](https://linear.app/tasker-systems/issue/TAS-56/ci-stabilization-optimize-build-times-and-resource-usage)
- Identifier: TAS-56
- Status: Todo
- Priority: Medium
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Rust](https://linear.app/tasker-systems/project/tasker-core-rust-9b5a1c23b7b1). Alpha version of the Tasker Core in Rust
- Created: 2025-10-25T01:16:08.840Z
- Updated: 2025-10-25T01:22:20.589Z

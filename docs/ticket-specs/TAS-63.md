# TAS-63: Code Coverage Analysis and Gap Closure

**Status**: Draft
**Priority**: High
**Created**: 2025-11-02
**Epic**: Code Quality and Testing Infrastructure

## Executive Summary

**✅ ACTUAL COVERAGE BASELINE ESTABLISHED: 43.35% line coverage**

Current code coverage analysis reveals **43.35% line coverage** across the workspace, with **917 tests run (all passing)**. This is a solid foundation showing we have comprehensive test infrastructure in place.

**Key Finding**: Initial coverage run was flawed (0.13% due to incorrect command), but the corrected run shows we have good test coverage as a baseline. Opportunity exists to improve coverage on critical paths and untested areas.

**Coverage Highlights**:
- Total: 43.35% line coverage (24,090 / 42,525 lines)
- Function coverage: 39.52% (2,108 / 5,334 functions)
- Region coverage: 44.24% (23,276 / 52,609 regions)
- Tests: 917 passed, 6 skipped

## Coverage Report Snapshot

**CORRECT Command** (from corrected run):
```bash
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
LOG_LEVEL=info \
TASKER_ENV=test \
cargo llvm-cov nextest --no-fail-fast --all-features --workspace
```

**Note**: Adding `run` after `nextest` acts as a test filter! Use just `nextest` without `run`.

**Results**:
```
Total Coverage: 43.35% line coverage (24,090 / 42,525 lines)
Function Coverage: 39.52% (2,108 / 5,334 functions)
Region Coverage: 44.24% (23,276 / 52,609 regions)
Tests Run: 917 passed, 6 skipped ✅
Execution Time: 28.453s
```

### Coverage by Area

**High Coverage Areas** (>90% line coverage):
- **Test Factories**: 96-100% coverage (foundation.rs, complex_workflows.rs, states.rs)
- **Worker Event Types**: 95-100% coverage (worker_queues/events unit tests)
- **Task Templates**: 97% coverage (task_template.rs)
- **Type Definitions**: 98% coverage (api/orchestration.rs)
- **Resilience Config**: 95-100% coverage (circuit_breaker, config)
- **Handler Registry**: 95% coverage (rust workers/registry.rs)
- **Health Checks**: 100% coverage (worker health.rs)

**Medium Coverage Areas** (40-70% line coverage):
- **State Machines**: 40-70% (actions, guards, persistence)
- **Database Models**: 12-97% (wide variance)
- **Scopes**: 65-95% (task, workflow_step, transitions)
- **Resilience**: 67-83% (circuit_breaker, manager, metrics)
- **Event Subscribers**: 52% (worker event_subscriber.rs)

**Low Coverage Areas** (0-20% line coverage):
- **CLI Binary**: 0% (tasker-cli.rs - 1,302 lines)
- **Server Binaries**: 0% (orchestration, worker servers)
- **Web Handlers**: 0% (all handlers untested via coverage)
- **Event Systems**: 0-30% (listeners, pollers)
- **Bootstrap**: 0-18% (orchestration, worker bootstrap)
- **FFI/Ruby Integration**: 0% (ruby ext bindings)

## Problem Statement

### Current Testing Landscape

**Strengths**:
- ✅ **917 tests, all passing** - Comprehensive test suite
- ✅ **43.35% line coverage baseline** - Solid foundation
- ✅ **High coverage on core utilities** - Factories, types, resilience at 90-100%
- ✅ **Integration tests** - E2E workflows, SQL functions tested
- ✅ **Good test infrastructure** - nextest, factories, test helpers

**Identified Gaps**:
1. **Web Handlers**: 0% coverage - Integration tested but not captured by coverage
2. **Actors**: 0% coverage - Critical orchestration logic needs unit tests
3. **CLI**: 0% coverage (1,302 lines) - Manual operations untested
4. **Bootstrap**: 0-20% coverage - Startup code paths untested
5. **Event Systems**: 0-30% coverage - Listeners/pollers need tests
6. **No CI Coverage**: Coverage not tracked in pull requests
7. **No Coverage Targets**: No established goals for improvement

### Root Causes Analysis

1. **Integration-Heavy Strategy**: Web handlers/actors tested via integration tests, not captured by unit test coverage
2. **No Coverage CI**: `cargo llvm-cov` not run in CI pipeline, regressions not caught
3. **Command Knowledge Gap**: Correct coverage command wasn't documented (`nextest` not `nextest run`)
4. **Missing Unit Tests**: Actors, CLI, bootstrap lack dedicated unit tests

## Objectives

### Primary Goals

1. ~~**Establish Coverage Baseline**~~ - ✅ **COMPLETE**: 43.35% line coverage with 917 tests
2. **Close Critical Coverage Gaps** - Add tests for actors, web handlers, CLI
3. **Set Coverage Targets** - Define improvement goals from 43% baseline
4. **Integrate Coverage into CI** - Track coverage in PRs, prevent regressions
5. **Improve Coverage Quality** - Focus on critical paths and edge cases

### Success Criteria

- [x] ✅ Full coverage report generated for all 917 tests - **COMPLETE**
- [x] ✅ Coverage baseline documented (43.35%) - **COMPLETE**
- [ ] Critical path coverage analysis complete
- [ ] Coverage targets defined (goal: 60% for critical paths, 50% overall)
- [ ] CI integration with coverage reporting
- [ ] Critical gaps closed: actors >50%, web handlers >40%, CLI >30%

## Analysis Plan

### Phase 1: Comprehensive Coverage Measurement (Week 1)

**Tasks**:

1. **Full Test Suite Coverage Run**
   ```bash
   # CORRECT - Run ALL tests in workspace with coverage
   DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
   TASKER_ENV=test \
   cargo llvm-cov nextest run --workspace --all-features --no-fail-fast \
   --lcov --output-path coverage/full-suite.lcov

   # Generate HTML report for browsing
   DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
   TASKER_ENV=test \
   cargo llvm-cov nextest run --workspace --all-features --no-fail-fast \
   --html --output-dir coverage/html

   # Open HTML report
   open coverage/html/index.html  # macOS
   # or: xdg-open coverage/html/index.html  # Linux
   ```

   **Key Change**: Use `--workspace` instead of listing individual packages. This ensures all tests run, including integration tests in the `tests/` directory.

2. **Package-Level Coverage Analysis**
   - Run coverage separately for each package
   - Identify per-package baselines
   - Document coverage distribution

3. **Test Category Breakdown**
   - Unit tests coverage
   - Integration tests coverage
   - E2E tests coverage
   - Identify which test types contribute most to coverage

4. **Coverage Report Artifacts**
   - Generate LCOV format for CI integration
   - Generate HTML for human review
   - Generate JSON for programmatic analysis

**Deliverables**:
- `coverage/full-suite.lcov` - Complete LCOV report
- `coverage/html/` - Browsable HTML coverage report
- `docs/coverage-baseline.md` - Initial coverage analysis
- `coverage/per-package.json` - Package-level breakdown

### Phase 2: Critical Path Identification (Week 1-2)

**Tasks**:

1. **Identify Critical Code Paths**
   - Task lifecycle (creation → completion)
   - Step execution and retry logic
   - State machine transitions
   - Error handling and recovery
   - DLQ entry and resolution
   - Manual intervention workflows

2. **Map Untested Critical Paths**
   ```
   Priority 1 (Production Critical):
   - State machine transitions (TaskStateMachine, StepStateMachine)
   - Actor message handling (all 5 actors)
   - SQL function execution (get_next_ready_tasks, etc.)
   - Error classification and retry logic
   - Circuit breaker behavior

   Priority 2 (High Value):
   - Web API handlers (task creation, step resolution)
   - Event systems (LISTEN/NOTIFY, PGMQ integration)
   - Configuration loading and validation
   - Health check endpoints

   Priority 3 (Lower Risk):
   - CLI commands (manual operations)
   - Metrics collection
   - Documentation generation
   ```

3. **Risk Assessment**
   - Map untested code to production incidents
   - Identify high-churn areas with low coverage
   - Document known bugs in untested areas

**Deliverables**:
- `docs/critical-paths-coverage.md` - Critical path analysis
- `docs/risk-assessment.md` - Coverage risk mapping
- Coverage heatmap visualization

### Phase 3: Coverage Target Setting (Week 2)

**Tasks**:

1. **Define Package-Level Targets**
   ```
   Proposed Targets (to be validated):

   Critical Packages (70% target):
   - tasker-shared/state_machine - State transitions must be tested
   - tasker-orchestration/actors - Message handling critical
   - tasker-orchestration/lifecycle - Core orchestration logic

   High-Value Packages (60% target):
   - tasker-shared/database/sql_functions - Database operations
   - tasker-orchestration/web/handlers - API contracts
   - tasker-worker/core - Worker execution

   Supporting Packages (40% target):
   - tasker-shared/config - Configuration loading
   - tasker-shared/models - Data models
   - tasker-client - Client library

   Utility Packages (30% target):
   - pgmq-notify - Third-party wrapper
   - tasker-shared/metrics - Observability
   ```

2. **Establish Coverage Gates**
   - Minimum coverage for new code
   - Coverage delta thresholds for PRs
   - Critical path coverage requirements

3. **Document Exceptions**
   - Identify code paths that cannot/should not be tested
   - Document why (e.g., third-party integration, manual-only operations)
   - Define alternative quality measures

**Deliverables**:
- `docs/coverage-targets.md` - Per-package coverage goals
- `.github/coverage-gates.yml` - CI enforcement rules
- `docs/coverage-exceptions.md` - Documented untestable code

### Phase 4: Gap Closure Strategy (Week 2-3)

**Tasks**:

1. **Prioritize Gap Closure**
   - Order gaps by risk × impact
   - Group related gaps for efficient closure
   - Estimate effort per gap

2. **Create Unit Test Plan**
   - Identify areas needing more unit tests
   - Design test fixtures and helpers
   - Plan mocking strategy for complex dependencies

3. **Integration Test Enhancement**
   - Identify integration test blind spots
   - Plan additional scenario coverage
   - Design test data strategies

4. **Coverage-Driven Development Plan**
   - Process for new features (test-first)
   - Coverage review in PR process
   - Refactoring strategy for legacy code

**Deliverables**:
- `docs/gap-closure-roadmap.md` - Prioritized plan
- `tests/fixtures/README.md` - Test fixture documentation
- `docs/testing-guidelines.md` - Team testing standards

### Phase 5: CI Integration (Week 3)

**Tasks**:

1. **Add Coverage to CI Pipeline**
   ```yaml
   # .github/workflows/coverage.yml
   - name: Generate Coverage
     run: |
       cargo llvm-cov nextest run --all-features --workspace \
         --lcov --output-path coverage.lcov

   - name: Upload to Codecov
     uses: codecov/codecov-action@v4
     with:
       files: ./coverage.lcov

   - name: Check Coverage Thresholds
     run: |
       cargo llvm-cov --fail-under-lines 40 \
         --fail-under-functions 35
   ```

2. **PR Coverage Comments**
   - Configure Codecov for PR comments
   - Show coverage delta in PR reviews
   - Block PRs that reduce coverage below thresholds

3. **Coverage Reporting**
   - Generate coverage badges
   - Create coverage dashboard
   - Set up coverage trend tracking

**Deliverables**:
- `.github/workflows/coverage.yml` - CI coverage workflow
- Coverage badge in README.md
- Codecov integration configured

## Implementation Recommendations

### Quick Wins (Immediate)

1. **Run Full Coverage Report** (Priority 1)
   ```bash
   # Execute comprehensive coverage analysis with ALL tests
   DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
   TASKER_ENV=test \
   cargo llvm-cov nextest run --workspace --all-features --no-fail-fast \
     --html --output-dir coverage/html

   # Open the report
   open coverage/html/index.html
   ```
   - This will run all ~900 tests (not just 2!)
   - Generate HTML report for team review
   - Identify actual coverage baseline

2. **Add Coverage to Local Development**
   ```bash
   # Add to Makefile or justfile
   coverage:
       @echo "Generating coverage report..."
       DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
       TASKER_ENV=test \
       cargo llvm-cov nextest run --workspace --all-features --html
       @echo "Report: file://$(pwd)/target/llvm-cov/html/index.html"
   ```

3. **Document Critical Paths**
   - List 10 most critical code paths
   - Map current test coverage
   - Estimate gap closure effort

### Medium-Term Actions (1-2 Months)

1. **Critical Path Coverage**
   - Achieve 70% coverage on state machines
   - Achieve 60% coverage on actors
   - Achieve 60% coverage on SQL functions

2. **Integration Test Expansion**
   - Add tests for DLQ workflows
   - Add tests for decision point logic
   - Add tests for manual intervention

3. **CI Integration**
   - Add coverage to PR checks
   - Set minimum coverage thresholds
   - Enable coverage trend tracking

### Long-Term Goals (3-6 Months)

1. **Overall Coverage Targets**
   - 60% line coverage for critical packages
   - 40% line coverage overall
   - 100% coverage for state machine transitions

2. **Test Quality Improvements**
   - Property-based testing for state machines
   - Chaos testing for resilience
   - Performance regression tests

3. **Coverage Culture**
   - Coverage-driven development practices
   - Regular coverage review meetings
   - Coverage quality metrics

## Technical Approach

### Coverage Tooling Stack

1. **`cargo-llvm-cov`** - Primary coverage tool
   - Generates accurate coverage via LLVM instrumentation
   - Supports multiple output formats (LCOV, HTML, JSON)
   - Integrates with nextest for parallel test execution

2. **`nextest`** - Test runner
   - Faster parallel test execution
   - Better test output and reporting
   - JUnit XML output for CI

3. **Codecov** - Coverage tracking service
   - PR coverage comments
   - Coverage trends over time
   - Team coverage dashboard

### Coverage Workflow

```bash
# Local development
make coverage               # Generate HTML report
make coverage-open         # Open HTML report in browser

# CI pipeline
cargo llvm-cov nextest run --all-features --workspace \
  --lcov --output-path coverage.lcov

# Per-package analysis
cargo llvm-cov nextest run --all-features \
  --package tasker-orchestration \
  --html --output-dir coverage/orchestration
```

### Coverage Analysis Tools

1. **HTML Reports** - Browse line-by-line coverage
2. **LCOV Reports** - Machine-readable format for CI
3. **JSON Reports** - Programmatic analysis and metrics
4. **Coverage Diff** - Compare coverage between branches

## Open Questions

1. **Coverage Tool Selection**
   - Is `cargo-llvm-cov` the right tool for our needs?
   - Should we also use `tarpaulin` for comparison?
   - Do we need coverage for FFI/Ruby integration?

2. **Coverage Targets**
   - What's a reasonable overall coverage target given our integration-heavy strategy?
   - Should we require 100% coverage for critical paths?
   - How do we handle third-party wrappers (pgmq-notify)?

3. **CI Integration**
   - Should coverage run on every PR or just on main?
   - What's an acceptable coverage delta threshold for PRs?
   - Should we block PRs that reduce coverage?

4. **Test Strategy**
   - Should we shift from integration-heavy to more unit tests?
   - How do we balance coverage with test execution time?
   - What's our approach to testing async/concurrent code?

## Resources Required

- **Time**: ~2-3 weeks for initial analysis and CI integration
- **Tools**: Codecov account (free for open source)
- **Compute**: CI runtime for coverage analysis (~5-10 min per run)
- **Team**: 1 engineer for analysis, team collaboration for gap closure

## Success Metrics

1. **Coverage Baseline Established** - Full coverage report generated
2. **Critical Paths Mapped** - Risk assessment complete
3. **Targets Defined** - Per-package goals documented
4. **CI Integrated** - Coverage runs on every PR
5. **Trend Tracking** - Coverage monitored over time
6. **Gap Closure Progress** - Monthly coverage improvements

## Next Steps

1. **Approve Ticket Scope** - Review and approve this plan
2. **Execute Phase 1** - Run comprehensive coverage analysis
3. **Review Findings** - Team review of baseline coverage
4. **Prioritize Gaps** - Identify critical gaps for immediate closure
5. **Implement CI** - Add coverage to GitHub Actions
6. **Create Gap Closure Tickets** - Break down improvements into actionable tasks

## References

- **Coverage Report**: `tmp/code-coverage-report.log`
- **llvm-cov Documentation**: https://github.com/taiki-e/cargo-llvm-cov
- **nextest Documentation**: https://nexte.st/
- **Codecov Integration**: https://docs.codecov.com/docs/rust
- **TAS-49 Work**: Example of comprehensive testing approach

## Appendix: Coverage Baseline Report

**✅ BASELINE ESTABLISHED** (2025-11-02)

**Command Used**:
```bash
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
LOG_LEVEL=info \
TASKER_ENV=test \
cargo llvm-cov nextest --no-fail-fast --all-features --workspace
```

**Critical Note**: Do NOT add `run` after `nextest` - it acts as a test filter!

**Baseline Results**:
```
Total Summary:
- Regions: 52,609 total, 29,333 missed (44.24% covered)
- Functions: 5,334 total, 3,226 missed (39.52% covered)
- Lines: 42,525 total, 24,090 covered (43.35% covered) ✅
- Branches: 0 total, 0 missed (no branch coverage)

Tests Run: 917 passed, 6 skipped
Execution Time: 28.453s
```

**Top Coverage Files** (>95%):
- `tasker-shared/models/factories/foundation.rs`: 100.00% (134/134 lines)
- `tasker-worker/health.rs`: 100.00% (64/64 lines)
- `tasker-shared/resilience/toml_config_test.rs`: 100.00% (86/86 lines)
- `tasker-shared/types/api/orchestration.rs`: 98.03% (149/152 lines)
- `tasker-shared/models/core/task_template.rs`: 97.02% (1,434/1,478 lines)
- `workers/rust/step_handlers/registry.rs`: 95.69% (200/209 lines)

**Lowest Coverage Critical Files** (priority for improvement):
- `tasker-client/bin/tasker-cli.rs`: 0.00% (0/1,302 lines) - 🎯 High Priority
- All web handlers: 0.00% - 🎯 High Priority
- `tasker-orchestration/actors/*`: 0.00% - 🎯 Critical Priority
- `tasker-worker/core.rs`: 0.00% (0/251 lines) - 🎯 High Priority
- `tasker-worker/bootstrap.rs`: 20.37% (44/216 lines)
- `tasker-orchestration/bootstrap.rs`: 0.00% (0/251 lines)

**Analysis**:
- **Strong foundation**: Test factories, type definitions, and shared utilities have excellent coverage
- **Critical gaps**: Actors, web handlers, CLI, and bootstrap code have 0% coverage
- **Opportunity**: Web handlers and actors are integration-tested but not captured by unit test coverage
- **Next Steps**: Add unit tests for actors, integration tests for web handlers, and CLI command tests

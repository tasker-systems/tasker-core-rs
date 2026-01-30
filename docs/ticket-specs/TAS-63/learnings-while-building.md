# TAS-63: Learnings While Building Code Coverage

## Key Decision: Integration Tests Are In Scope for Coverage

We established that **dependent-service-level integration tests are in scope** for expanding code coverage. Specifically:

- **PostgreSQL** is expected to be running for all coverage measurement. The database is a foundational dependency — there is no meaningful way to test most of tasker-shared's model layer, SQL functions, state machine persistence, or scope system without it.
- **The configured messaging backend** (PGMQ or RabbitMQ) is expected to be available when using `--features test-messaging` or `--all-features`.

This means coverage targets should be measured with `--all-features` and real infrastructure, not with mocked-out databases. The existing factory system (`TaskFactory`, `WorkflowStepFactory`, `ComplexWorkflowFactory`, etc.) and `#[sqlx::test]` annotation provide excellent test isolation — each test gets its own database transaction that rolls back automatically.

---

## Critical Finding: Orphaned Integration Test Modules

### The Problem

Rust's integration test system compiles each `.rs` file directly inside `tests/` as its own test binary. Subdirectories with `mod.rs` files are **not** automatically compiled — they must be explicitly referenced by a root `.rs` file.

In `tasker-shared`, we discovered that **5 entire module trees** in `tests/` were orphaned:

| Directory | Contents | Status |
|-----------|----------|--------|
| `tests/models/` | 13 test files across `core/`, `insights/`, `orchestration/` | Never compiled |
| `tests/state_machine/` | 8 test files (actions, events, guards, persistence, states, step/task state machines) | Never compiled |
| `tests/sql_functions/` | 5 test files (production validation, step readiness, system health, task execution, SQL executor) | Never compiled |
| `tests/database/` | 2 test files (pool tests, SQL function tests) | Never compiled |
| `tests/circuit_breaker/` | 1 test file | Never compiled; also referenced wrong crate |

Each directory had a `mod.rs` that correctly declared its submodules, but **no root `.rs` file in `tests/` referenced any of these directories**. The result: Rust silently ignored them. They appeared in the file tree, showed up in IDE navigation, but were never part of any build or CI run.

### How It Manifested

These tests had accumulated significant code rot:

- **Schema drift**: Fields like `skippable` were removed from `WorkflowStep`, `in_backoff_steps` removed from `SystemHealthCounts`, `processor_uuid` added to `NewTaskTransition` — none of these were caught because the tests never compiled.
- **API changes**: Methods renamed (`get_by_task` → `get_for_task`, `health_score` → `success_rate`), string fields replaced with enums (`execution_status: String` → `ExecutionStatus` enum), function signatures changed — all invisible.
- **State name changes**: Task states renamed (`in_progress` → `steps_in_process`), which would have been caught immediately by database CHECK constraints if the tests had ever run.
- **Privacy changes**: Structs made `pub(crate)` that tests still imported as `pub`.
- **Wrong crate references**: The `circuit_breaker/` tests imported `tasker_orchestration`, which is not a dependency of `tasker-shared`.

Despite all this, `cargo check --tests`, `cargo test`, and `cargo clippy` all passed cleanly. The orphaned files were invisible to every quality gate.

### The Fix

Rust does not support the `tests/foo.rs` + `tests/foo/` directory convention for implicit module resolution in integration tests (unlike in `src/`). The fix is to use explicit `#[path]` attributes in root test files:

```rust
// tests/models.rs — creates the "models" integration test binary
#[path = "models/core/mod.rs"]
pub mod core;
#[path = "models/insights/mod.rs"]
pub mod insights;
```

We created 4 root files (`models.rs`, `state_machine.rs`, `sql_functions.rs`, `database.rs`) and deleted the corresponding `mod.rs` files from the subdirectories (Rust disallows both `tests/foo.rs` and `tests/foo/mod.rs` simultaneously).

The circuit_breaker tests were removed entirely because they referenced `tasker_orchestration` — a different crate that `tasker-shared` does not depend on. Those tests, if still needed, belong in `tasker-orchestration`.

### Impact

| Metric | Before Fix | After Fix |
|--------|-----------|-----------|
| Tests discovered | 1,481 | 1,713 |
| Tests unlocked | — | +232 |
| Line coverage | 69.0% | 75.82% |
| Lines covered | 25,868 | 28,426 |

The targeted model files went from 6-12% coverage to 71-99% coverage. The tests expressed meaningful intent — by fixing their compilation and runtime errors, we gained genuine confidence in the system's behavior, not just a number.

---

## Guidance for Remaining Crates

### Audit Results (January 2026)

We audited all remaining Rust crates for the same orphan problem:

#### tasker-orchestration — HAS ISSUES

**Non-standard pattern**: Uses `tests/mod.rs` as a root integration test file. This works (Rust compiles it as a test binary named `mod`) but is unusual and causes a subtle problem:

- `tests/mod.rs` declares `pub mod state_manager;` and `pub mod task_initialization_cycle_detection_test;`
- But `tests/state_manager.rs` and `tests/task_initialization_cycle_detection_test.rs` also exist as standalone root files
- Result: **7 tests run twice** — once in the `mod` binary and once in the standalone binaries

The `services/`, `messaging/`, `web/`, and `web/auth/` subdirectories are all referenced through `mod.rs` and ARE being compiled (122 tests). But the duplication and non-standard pattern should be cleaned up.

**Recommended fix**: Rename `tests/mod.rs` to something like `tests/orchestration_tests.rs`, remove the duplicate `pub mod state_manager` and `pub mod task_initialization_cycle_detection_test` declarations (let those remain standalone), and use `#[path]` attributes for the subdirectories.

#### tasker-worker — HAS ORPHANED TESTS

`tests/integration/worker_system_test.rs` exists with 3 test functions but is **never compiled**. The `tests/integration/` directory has no `mod.rs` and no root file references it.

**Recommended fix**: Either create a root file with `#[path = "integration/worker_system_test.rs"]` or, if the tests are outdated, remove them. Check if they compile first — they may have the same code rot problem.

#### tasker-pgmq — CLEAN

Follows the standard pattern: root `.rs` files in `tests/` with a shared `common.rs` helper module. No subdirectories, no orphan risk.

#### tasker-client — CLEAN

Single test file, no subdirectories.

### Checklist for Future Coverage Work on Any Crate

1. **Audit `tests/` directory structure first**. List all subdirectories with `mod.rs` files. For each, verify a root `.rs` file references it. If not, those tests are orphaned.

2. **Check for `tests/mod.rs`**. This is a legal but non-standard pattern. It creates a test binary named `mod`. Verify it doesn't duplicate tests that also exist as standalone root `.rs` files.

3. **Count tests before and after**. Use `cargo nextest list --package <crate> --all-features 2>&1 | grep -c "::"` to count integration tests. Any new root file wiring should increase this count.

4. **Expect compilation failures in orphaned tests**. Files that were never compiled will have accumulated schema drift, API changes, renamed types, and removed fields. Budget time to triage: fix if the intent is sound, remove if the test references wrong crates or tests removed functionality.

5. **Expect runtime failures too**. Even after compilation fixes, tests may fail due to database CHECK constraints rejecting old state names, changed assertion conditions, or restructured return types. These are real signals — each fix improves confidence.

6. **Use `#[path]` attributes for subdirectory modules**. The pattern:
   ```rust
   // tests/my_tests.rs
   #[path = "my_tests/some_module.rs"]
   pub mod some_module;
   ```
   This is the only reliable way to reference subdirectory modules from integration test root files. Do NOT rely on implicit `tests/foo.rs` + `tests/foo/` resolution — it does not work for integration tests.

7. **Run coverage with `--all-features`**. The coverage task (`CRATE_NAME=<crate> cargo make coverage-crate`) already does this. Verify the test count in coverage output matches `cargo nextest list` — a mismatch means tests are being excluded.

### The Broader Lesson

Tests that exist on disk but aren't wired into the build system are worse than no tests at all. They create a false sense of coverage, accumulate code rot silently, and when finally discovered, require significant effort to rehabilitate. The Rust compiler's module system makes this particularly easy to get wrong with integration tests — subdirectories in `tests/` look like they should work but don't without explicit wiring.

**Prevention**: Any PR that adds a new `tests/` subdirectory should be verified with `cargo nextest list` to confirm the new tests actually appear in the test count.

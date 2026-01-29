# TAS-63: Coverage Tooling

**Status**: Complete (Tooling Phase)
**Branch**: `jcoletaylor/tas-63-code-coverage-analysis-and-gap-closure`

## Overview

Unified coverage tooling for the tasker-core workspace. Generates per-crate
coverage reports across Rust, Python, Ruby, and TypeScript with normalized
JSON output, threshold enforcement, and actionable file-level detail.

**Formal documentation:**
- [`docs/development/coverage-tooling.md`](../../development/coverage-tooling.md) - Commands, architecture, JSON schema, and script reference

**Related ticket documents:**
- `ticket.md` - Coverage gap analysis and prioritized closure plan
- `coverage-tooling.md` - Architecture deep-dive and design decisions

---

## Quick Start

```bash
# One-time: ensure coverage tools are installed
cargo make coverage-tools-setup

# Run coverage for a single crate
CRATE_NAME=tasker-shared cargo make coverage-crate

# Run coverage for all foundational crates
cargo make coverage-foundational

# Run coverage for all core crates
cargo make coverage-core

# Run everything (all languages)
cargo make coverage-all

# Generate aggregate report
cargo make coverage-report

# Check against thresholds
cargo make coverage-check
```

---

## Command Reference

### Rust Coverage

| Task | Alias | Description |
|------|-------|-------------|
| `coverage` | `cov` | Workspace-wide Rust coverage (JSON + HTML) |
| `coverage-crate` | - | Single crate (`CRATE_NAME` env var) |
| `coverage-foundational` | - | `tasker-shared` + `tasker-pgmq` |
| `coverage-core` | - | `tasker-orchestration` + `tasker-worker` + `tasker-client` |
| `coverage-rust-worker` | `covr` | Rust worker crate |

### Language Worker Coverage

| Task | Alias | Description |
|------|-------|-------------|
| `coverage-python` | `covp` | Python worker (`pytest-cov`) |
| `coverage-ruby` | `covrb` | Ruby worker (`SimpleCov`) |
| `coverage-typescript` | `covts` | TypeScript worker (`bun --coverage`) |

### Aggregate & Reporting

| Task | Alias | Description |
|------|-------|-------------|
| `coverage-all` | `cova` | Run all languages |
| `coverage-report` | - | Generate aggregate JSON report |
| `coverage-check` | `covc` | Check thresholds (exit code 1 on failure) |
| `coverage-clean` | - | Remove all coverage artifacts |
| `coverage-tools-setup` | - | Install `rustfilt` and sync Python project |

---

## Reading the Output

### Per-Crate Reports

After running a crate's coverage, find its normalized JSON at:

```
coverage-reports/rust/<crate>-coverage.json
```

The report is structured for actionability:

- **`summary`** - Total lines/functions covered and percentages
- **`files[]`** - Per-file breakdown, sorted worst-first
- **`uncovered_functions[]`** - Demangled Rust function names with no test coverage

**Example: finding your biggest gaps**

```bash
# Run coverage
CRATE_NAME=tasker-shared cargo make coverage-crate

# See the 10 worst-covered files
jq '.files[:10][] | "\(.line_coverage_percent)% \(.path)"' \
  coverage-reports/rust/tasker-shared-coverage.json

# See uncovered files (0% coverage)
jq '.files[] | select(.line_coverage_percent == 0) | "\(.lines_total) lines  \(.path)"' \
  coverage-reports/rust/tasker-shared-coverage.json

# See uncovered functions in a specific file
jq '.uncovered_functions[] | select(.file | contains("config/web.rs")) | .name' \
  coverage-reports/rust/tasker-shared-coverage.json
```

### Aggregate Report

After `cargo make coverage-report`:

```
coverage-reports/aggregate-coverage.json
```

Contains cross-crate summaries with threshold pass/fail status, plus:

- **`lowest_coverage_files[]`** - Worst 30 files across all crates
- **`uncovered_files[]`** - All files at 0%, sorted by size (biggest gaps first)

### HTML Reports

For visual browsing of line-by-line coverage:

```bash
cargo make coverage    # Generates HTML at coverage-reports/rust/html/
open coverage-reports/rust/html/index.html
```

---

## Thresholds

Thresholds are defined in `coverage-thresholds.json` at the project root:

```json
{
  "rust": {
    "tasker-shared": 70,
    "tasker-pgmq": 65,
    "tasker-orchestration": 55,
    "tasker-worker": 55,
    "tasker-client": 60
  },
  "python": { "tasker-core-py": 80 },
  "ruby": { "tasker-worker-rb": 70 },
  "typescript": { "tasker-worker-ts": 60 }
}
```

Thresholds represent target line coverage percentages. `cargo make coverage-check`
exits with code 1 if any crate is below its threshold.

---

## Key Technical Details

### Test Isolation

All Rust coverage tasks use `cargo llvm-cov nextest` (not `cargo llvm-cov test`).
This is required because auth integration tests set `TASKER_CONFIG_PATH` via
`std::env::set_var()`, which leaks to other tests in a single-process runner.
Nextest provides per-test process isolation, preventing this pollution.

### Symbol Demangling

The `rustfilt` tool (installed via `cargo install rustfilt`) demanles Rust symbol
names in the uncovered functions list. All mangled names are batch-piped through
a single `rustfilt` process, then deduplicated to collapse generic monomorphizations
into unique function entries.

### Python Tooling

The normalizer scripts live in a uv-backed Python project at
`cargo-make/scripts/coverage/`. All cargo-make tasks invoke them via
`uv run --project cargo-make/scripts/coverage` to ensure a consistent environment.
See `coverage-tooling.md` for details.

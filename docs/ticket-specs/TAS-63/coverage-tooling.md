# TAS-63: Coverage Tooling Architecture

**Status**: Complete
**Last Updated**: 2026-01-29

---

## Architecture Overview

```
                     cargo make coverage-*
                            |
                +-----------+-----------+
                |           |           |
         Rust Coverage  Python/Ruby/TS  Aggregate
         (llvm-cov +    (language-      (cross-crate
          nextest)       native tools)   reporting)
                |           |           |
                v           v           v
         *-raw.json    raw output   per-crate JSON
                |           |           |
                +-----+-----+          |
                      |                 |
               normalize-*.py           |
               (uv run)                 |
                      |                 |
                      v                 v
               *-coverage.json    aggregate-coverage.json
               (per-crate)        (all crates)
                      |                 |
                      +--------+--------+
                               |
                        check-thresholds.py
```

### Data Flow

1. **Coverage collection** - Language-specific tools produce raw output
2. **Normalization** - Python scripts convert to a standardized schema
3. **Aggregation** - Cross-crate report with threshold checking
4. **Thresholds** - Pass/fail enforcement from `coverage-thresholds.json`

---

## File Layout

```
tasker-core/
+-- Makefile.toml                          # Root coverage tasks
+-- coverage-thresholds.json               # Per-crate threshold config
+-- cargo-make/
|   +-- scripts/
|       +-- coverage/
|           +-- pyproject.toml             # uv Python project definition
|           +-- normalize-rust.py          # llvm-cov JSON -> standard schema
|           +-- normalize-python.py        # pytest-cov JSON -> standard schema
|           +-- normalize-ruby.py          # SimpleCov JSON -> standard schema
|           +-- normalize-typescript.py    # LCOV -> standard schema
|           +-- aggregate.py              # Combine all per-crate reports
|           +-- check-thresholds.py       # Enforce thresholds, exit code
+-- workers/
|   +-- rust/Makefile.toml                # Rust worker coverage task
|   +-- ruby/Makefile.toml                # Ruby worker coverage task
|   +-- ruby/.simplecov                   # SimpleCov configuration
|   +-- python/Makefile.toml              # Python worker coverage task
|   +-- typescript/Makefile.toml          # TypeScript worker coverage task
+-- coverage-reports/                      # Generated (gitignored)
    +-- rust/
    |   +-- <crate>-raw.json              # Raw llvm-cov output
    |   +-- <crate>-coverage.json         # Normalized report
    |   +-- html/                         # HTML browsable report
    +-- python/
    +-- ruby/
    +-- typescript/
    +-- aggregate-coverage.json
```

---

## Normalized Coverage JSON Schema

### Per-Crate Report

Every normalizer produces this schema. Rust and TypeScript reports include
function-level detail; Python and Ruby include line-level detail where available.

```json
{
  "meta": {
    "timestamp": "2026-01-29T18:36:55.034134+00:00",
    "crate": "tasker-shared",
    "language": "rust",
    "tool": "cargo-llvm-cov",
    "git_commit": "bab744a8d3f1",
    "git_branch": "jcoletaylor/tas-63-code-coverage-analysis-and-gap-closure"
  },
  "summary": {
    "lines_covered": 17527,
    "lines_total": 31333,
    "line_coverage_percent": 55.94,
    "functions_covered": 2196,
    "functions_total": 4404,
    "function_coverage_percent": 49.86
  },
  "files_tested": 113,
  "files_total": 143,
  "files": [
    {
      "path": "tasker-shared/src/config/web.rs",
      "lines_covered": 0,
      "lines_total": 44,
      "line_coverage_percent": 0.0,
      "functions_covered": 0,
      "functions_total": 8,
      "function_coverage_percent": 0.0
    }
  ],
  "uncovered_functions": [
    {
      "name": "<tasker_shared::cache::provider::CacheBackend>::delete",
      "file": "tasker-shared/src/cache/provider.rs"
    }
  ]
}
```

**Field notes:**

| Field | Description |
|-------|-------------|
| `files[]` | Sorted by `line_coverage_percent` ascending (worst first). Filtered to the target crate's `src/` directory only. |
| `uncovered_functions[]` | Functions with zero execution count. Demangled via `rustfilt`, deduplicated across generic monomorphizations. Scoped to the target crate. |
| `summary.lines_*` | From `cargo-llvm-cov` totals (includes all compiled code, not just this crate). |
| `files_tested` / `files_total` | Counts of crate-scoped files with/without coverage. |

**Language-specific fields:**

| Language | Extra `files[]` fields | `uncovered_functions[]` |
|----------|----------------------|------------------------|
| Rust | `functions_covered`, `functions_total`, `function_coverage_percent` | Yes (demangled) |
| Python | `missing_lines`, `excluded_lines` | No |
| Ruby | (none) | No |
| TypeScript | `functions_covered`, `functions_total`, `function_coverage_percent` | Yes |

### Aggregate Report

```json
{
  "meta": {
    "timestamp": "...",
    "git_commit": "...",
    "git_branch": "...",
    "report_count": 5
  },
  "summary": {
    "total_lines_covered": 17527,
    "total_lines": 31333,
    "overall_line_coverage_percent": 55.94,
    "crates_passing": 3,
    "crates_failing": 2
  },
  "crates": {
    "tasker-shared": {
      "language": "rust",
      "lines_covered": 17527,
      "lines_total": 31333,
      "line_coverage_percent": 55.94,
      "threshold": 70,
      "passes_threshold": false,
      "source_file": "coverage-reports/rust/tasker-shared-coverage.json"
    }
  },
  "lowest_coverage_files": [],
  "uncovered_files": []
}
```

| Field | Description |
|-------|-------------|
| `crates{}` | Per-crate summary with threshold pass/fail |
| `lowest_coverage_files[]` | Bottom 30 files across all crates by coverage percent |
| `uncovered_files[]` | All 0% coverage files, sorted by total lines descending |

---

## Python Project (`cargo-make/scripts/coverage/`)

The normalizer and aggregation scripts are managed as a uv-backed Python project.

### Project Structure

```
cargo-make/scripts/coverage/
+-- pyproject.toml         # Project definition (requires-python >= 3.11)
+-- .venv/                 # Virtual environment (gitignored)
+-- normalize-rust.py
+-- normalize-python.py
+-- normalize-ruby.py
+-- normalize-typescript.py
+-- aggregate.py
+-- check-thresholds.py
```

### Invocation

All cargo-make tasks use `uv run --project` to invoke scripts within the
project's virtual environment:

```bash
uv run --project cargo-make/scripts/coverage python3 \
  cargo-make/scripts/coverage/normalize-rust.py \
  coverage-reports/rust/tasker-shared-raw.json \
  coverage-reports/rust/tasker-shared-coverage.json \
  --crate tasker-shared
```

The `coverage-tools-setup` cargo-make task runs `uv sync --project` to ensure
dependencies are installed before any coverage task runs.

### Adding Python Dependencies

If a normalizer script needs a new Python package:

1. Add it to `cargo-make/scripts/coverage/pyproject.toml` under `dependencies`
2. Run `uv sync --project cargo-make/scripts/coverage`
3. Commit `pyproject.toml` and `uv.lock`

---

## Script Reference

### `normalize-rust.py`

Converts `cargo-llvm-cov` JSON export to the normalized schema.

```
Usage: normalize-rust.py <input_json> <output_json> --crate <crate_name>
```

**Key behaviors:**
- Filters out external dependency files (`index.crates.io-*`, `.cargo/registry/`, `target/`)
- Scopes files and functions to the target crate's `src/` directory
- Batch-demangles all Rust symbols through `rustfilt` in a single subprocess
- Deduplicates generic monomorphizations (many mangled names -> one demangled name)
- Special crate mapping: `tasker-worker-rust` -> `workers/rust/src/`
- Pass `--crate workspace` for unfiltered workspace-wide output

### `normalize-python.py`

Converts `pytest-cov` JSON output to the normalized schema.

```
Usage: normalize-python.py <input_json> <output_json>
```

Per-file detail includes `missing_lines` and `excluded_lines` counts.
Function-level tracking is not available from pytest-cov.

### `normalize-ruby.py`

Converts SimpleCov `.resultset.json` to the normalized schema.

```
Usage: normalize-ruby.py <input_json> <output_json>
```

Handles both old-format (array) and new-format (dict with `lines` key)
SimpleCov output. Strips workspace path prefixes from file paths.

### `normalize-typescript.py`

Parses LCOV format coverage data from Bun test runner.

```
Usage: normalize-typescript.py <coverage_dir> <output_json>
```

Extracts per-file and per-function data from `DA:` (line) and `FN:`/`FNDA:`
(function) LCOV records. Tracks uncovered functions.

### `aggregate.py`

Combines all per-crate normalized reports into a single aggregate.

```
Usage: aggregate.py --output <path> [--reports-dir <dir>]
```

Discovers reports in `coverage-reports/{rust,python,ruby,typescript}/`.
Applies thresholds from `coverage-thresholds.json`. Surfaces the worst-covered
files and all uncovered files in the aggregate output.

### `check-thresholds.py`

Enforces coverage thresholds. Exits 0 if all pass, 1 if any fail.

```
Usage: check-thresholds.py [--aggregate <path>] [--reports-dir <dir>]
```

Prefers the aggregate report if available; falls back to individual reports.

---

## External Dependencies

| Tool | Install | Purpose |
|------|---------|---------|
| `cargo-llvm-cov` | `cargo install cargo-llvm-cov` | Rust code coverage instrumentation |
| `cargo-nextest` | `cargo install cargo-nextest` | Test runner with per-test process isolation |
| `rustfilt` | `cargo install rustfilt` | Rust symbol demangling |
| `uv` | `brew install uv` (in Brewfile) | Python project management |
| `pytest-cov` | Via `uv` in workers/python | Python coverage |
| `simplecov` / `simplecov-json` | Via Bundler in workers/ruby | Ruby coverage |
| `bun` | `brew install bun` (in Brewfile) | TypeScript test runner with coverage |

The `coverage-tools-setup` task auto-installs `rustfilt` and syncs the uv project.

---

## Design Decisions

### Why nextest for coverage?

`cargo test` runs all tests in a single process. Auth integration tests use
`std::env::set_var("TASKER_CONFIG_PATH", ...)` to point at auth-enabled config,
which pollutes subsequent non-auth tests in the same process. `cargo nextest`
gives each test its own process, preventing env var leakage. See `ticket.md`
for the root cause analysis.

### Why normalize to JSON instead of using HTML directly?

HTML reports are great for humans but can't be diffed, aggregated, or enforced
in CI. The normalized JSON enables:
- Cross-language aggregation (Rust + Python + Ruby + TypeScript in one report)
- Threshold enforcement with exit codes
- File-level gap analysis via `jq`
- Trend tracking over time (compare JSON snapshots)

### Why filter to crate source in per-crate reports?

`cargo-llvm-cov --package X` runs X's tests but instruments all compiled code,
including workspace dependencies. Without filtering, the `tasker-shared` report
would show `tasker-pgmq` files (a dependency) with 0% coverage, creating noise.
Filtering to `<crate>/src/` focuses each report on code the crate owns.

### Why batch-demangle with rustfilt?

The raw llvm-cov JSON contains mangled Rust symbols like
`_RNvMs0_NtNtCs7cxDW5Sx6BT_13tasker_shared5cache8providerNtB5_13CacheProvider3new`.
These are unreadable. `rustfilt` demangles them to
`<tasker_shared::cache::provider::CacheProvider>::new`. Batch-piping all names
through a single subprocess call is efficient regardless of count.

Generic functions produce multiple mangled symbols (one per monomorphization)
that all demangle to the same name. Deduplication collapses these, reducing
the `tasker-shared` uncovered functions list from ~170k raw symbols to ~3.2k
unique demangled entries.

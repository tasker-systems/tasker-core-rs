#!/usr/bin/env python3
"""
Normalize cargo-llvm-cov JSON output to standard coverage schema.

Usage:
    python normalize-rust.py <input_json> <output_json> --crate <crate_name>
"""

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def get_git_info() -> tuple[str, str]:
    """Get current git commit hash and branch name."""
    try:
        commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], stderr=subprocess.DEVNULL, text=True
        ).strip()[:12]
    except (subprocess.CalledProcessError, FileNotFoundError):
        commit = "unknown"

    try:
        branch = subprocess.check_output(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            stderr=subprocess.DEVNULL,
            text=True,
        ).strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        branch = "unknown"

    return commit, branch


def strip_workspace_prefix(filepath: str) -> str:
    """Strip absolute workspace path prefix, returning a relative path."""
    # Common patterns: /Users/.../tasker-core/src/... or /home/.../tasker-core/src/...
    markers = ["/tasker-core/", "/tasker-systems/"]
    for marker in markers:
        idx = filepath.find(marker)
        if idx != -1:
            return filepath[idx + len(marker) :]
    # If no marker found, just return the basename portions after src/ or lib/
    for prefix in ["/src/", "/lib/", "/tests/"]:
        idx = filepath.find(prefix)
        if idx != -1:
            return filepath[idx + 1 :]
    return filepath


def extract_file_details(raw_files: list) -> list[dict]:
    """Extract per-file coverage details from llvm-cov files array.

    Only includes workspace source files (excludes external dependencies).
    Returns files sorted by line_coverage_percent ascending (worst coverage first).
    """
    file_details = []
    for f in raw_files:
        filename = f.get("filename", "")
        # Skip files from external dependencies
        if not is_workspace_file(filename):
            continue

        summary = f.get("summary", {})
        lines = summary.get("lines", {})
        functions = summary.get("functions", {})

        lines_covered = lines.get("covered", 0)
        lines_total = lines.get("count", 0)
        funcs_covered = functions.get("covered", 0)
        funcs_total = functions.get("count", 0)

        line_pct = (lines_covered / lines_total * 100) if lines_total > 0 else 0.0
        func_pct = (funcs_covered / funcs_total * 100) if funcs_total > 0 else 0.0

        file_details.append(
            {
                "path": strip_workspace_prefix(filename),
                "lines_covered": lines_covered,
                "lines_total": lines_total,
                "line_coverage_percent": round(line_pct, 2),
                "functions_covered": funcs_covered,
                "functions_total": funcs_total,
                "function_coverage_percent": round(func_pct, 2),
            }
        )

    # Sort by line coverage ascending — worst-covered files first
    file_details.sort(key=lambda f: (f["line_coverage_percent"], f["path"]))
    return file_details


def is_workspace_file(filepath: str) -> bool:
    """Check if a file path belongs to workspace source (not dependencies or build artifacts)."""
    exclude_patterns = [
        "/index.crates.io-",
        "/.cargo/registry/",
        "/.rustup/toolchains/",
        "/rustc/",
        "/target/",
    ]
    return not any(pat in filepath for pat in exclude_patterns)


def batch_demangle(names: list[str]) -> list[str]:
    """Batch-demangle Rust symbol names using rustfilt.

    Pipes all names through rustfilt in a single subprocess call.
    Falls back to raw names if rustfilt is not available.
    """
    if not names:
        return names

    try:
        result = subprocess.run(
            ["rustfilt"],
            input="\n".join(names),
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode == 0:
            demangled = result.stdout.strip().split("\n")
            if len(demangled) == len(names):
                return demangled
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    return names


def extract_uncovered_functions(raw_functions: list) -> list[dict]:
    """Extract functions with zero execution count from llvm-cov functions array.

    Only includes functions from workspace source files (excludes dependencies).
    Demangled via rustfilt for readability, then deduplicated by (name, file).
    Returns list of uncovered functions sorted by file path.
    """
    raw_uncovered = []
    for fn in raw_functions:
        count = fn.get("count", 0)
        if count == 0:
            filenames = fn.get("filenames", [])
            if not filenames:
                continue
            filepath = filenames[0]
            if not is_workspace_file(filepath):
                continue
            raw_uncovered.append(
                {
                    "mangled": fn.get("name", "unknown"),
                    "file": strip_workspace_prefix(filepath),
                }
            )

    # Batch-demangle all names in one subprocess call
    mangled_names = [fn["mangled"] for fn in raw_uncovered]
    demangled_names = batch_demangle(mangled_names)

    # Deduplicate: many mangled symbols demangle to the same function
    # (generic monomorphizations). Keep unique (demangled_name, file) pairs.
    seen = set()
    uncovered = []
    for fn, demangled in zip(raw_uncovered, demangled_names):
        key = (demangled, fn["file"])
        if key not in seen:
            seen.add(key)
            uncovered.append({"name": demangled, "file": fn["file"]})

    uncovered.sort(key=lambda f: (f["file"], f["name"]))
    return uncovered


def crate_source_prefix(crate_name: str) -> str:
    """Map crate name to its source directory prefix for filtering.

    E.g., 'tasker-shared' → 'tasker-shared/src/'
    """
    # Handle worker crates that live under workers/
    worker_crate_map = {
        "tasker-worker-rust": "workers/rust/src/",
    }
    if crate_name in worker_crate_map:
        return worker_crate_map[crate_name]
    return f"{crate_name}/src/"


def normalize_llvm_cov(input_path: Path, crate_name: str) -> dict:
    """Convert llvm-cov JSON to normalized schema."""
    with open(input_path) as f:
        raw_data = json.load(f)

    # Extract totals from llvm-cov format
    # llvm-cov JSON structure: { "data": [{ "totals": {...}, "files": [...], "functions": [...] }] }
    data_entry = raw_data.get("data", [{}])[0]
    totals = data_entry.get("totals", {})

    lines = totals.get("lines", {})
    functions = totals.get("functions", {})

    lines_covered = lines.get("covered", 0)
    lines_total = lines.get("count", 0)
    functions_covered = functions.get("covered", 0)
    functions_total = functions.get("count", 0)

    # Calculate percentages
    line_pct = (lines_covered / lines_total * 100) if lines_total > 0 else 0.0
    function_pct = (
        (functions_covered / functions_total * 100) if functions_total > 0 else 0.0
    )

    # Extract per-file details (filtered to workspace files only)
    raw_files = data_entry.get("files", [])
    file_details = extract_file_details(raw_files)

    # For per-crate reports, further filter to only this crate's source files
    src_prefix = crate_source_prefix(crate_name)
    crate_files = [f for f in file_details if f["path"].startswith(src_prefix)]

    # If we're running workspace-wide (crate_name == "workspace"), keep all files
    if crate_name == "workspace" or not crate_files:
        crate_files = file_details

    files_total = len(crate_files)
    files_tested = sum(1 for f in crate_files if f["lines_covered"] > 0)

    # Extract uncovered functions, filtered to this crate's source
    raw_functions = data_entry.get("functions", [])
    uncovered_functions = extract_uncovered_functions(raw_functions)
    if crate_name != "workspace":
        uncovered_functions = [
            fn for fn in uncovered_functions if fn["file"].startswith(src_prefix)
        ]

    git_commit, git_branch = get_git_info()

    return {
        "meta": {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "crate": crate_name,
            "language": "rust",
            "tool": "cargo-llvm-cov",
            "git_commit": git_commit,
            "git_branch": git_branch,
        },
        "summary": {
            "lines_covered": lines_covered,
            "lines_total": lines_total,
            "line_coverage_percent": round(line_pct, 2),
            "functions_covered": functions_covered,
            "functions_total": functions_total,
            "function_coverage_percent": round(function_pct, 2),
        },
        "files_tested": files_tested,
        "files_total": files_total,
        "files": crate_files,
        "uncovered_functions": uncovered_functions,
    }


def main():
    parser = argparse.ArgumentParser(description="Normalize Rust coverage JSON")
    parser.add_argument("input", type=Path, help="Input llvm-cov JSON file")
    parser.add_argument("output", type=Path, help="Output normalized JSON file")
    parser.add_argument("--crate", required=True, help="Crate name")
    args = parser.parse_args()

    if not args.input.exists():
        print(f"Error: Input file not found: {args.input}", file=sys.stderr)
        sys.exit(1)

    normalized = normalize_llvm_cov(args.input, args.crate)

    # Ensure output directory exists
    args.output.parent.mkdir(parents=True, exist_ok=True)

    with open(args.output, "w") as f:
        json.dump(normalized, f, indent=2)

    print(f"Normalized coverage written to: {args.output}")
    print(f"  Line coverage: {normalized['summary']['line_coverage_percent']}%")
    print(f"  Function coverage: {normalized['summary']['function_coverage_percent']}%")


if __name__ == "__main__":
    main()

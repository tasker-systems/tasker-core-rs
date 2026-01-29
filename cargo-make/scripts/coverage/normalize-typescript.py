#!/usr/bin/env python3
"""
Normalize Bun test coverage output to standard coverage schema.

Bun test --coverage outputs LCOV-format coverage data.
This script parses LCOV or coverage summary to extract metrics.

Usage:
    python normalize-typescript.py <coverage_dir> <output_json>
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
    """Strip absolute path prefix, returning a relative path from the worker root."""
    markers = ["/workers/typescript/", "/tasker-core/workers/typescript/"]
    for marker in markers:
        idx = filepath.find(marker)
        if idx != -1:
            return filepath[idx + len(marker) :]
    # Fallback: try to find src/ or lib/ or test/
    for prefix in ["/src/", "/lib/", "/test/"]:
        idx = filepath.find(prefix)
        if idx != -1:
            return filepath[idx + 1 :]
    return filepath


def parse_lcov(lcov_path: Path) -> dict:
    """Parse LCOV format coverage file.

    Returns dict with keys: lines_covered, lines_total, functions_covered,
    functions_total, files_tested, files_total, file_details, uncovered_functions.
    """
    lines_covered = 0
    lines_total = 0
    functions_covered = 0
    functions_total = 0
    files_tested = 0
    files_total = 0

    # Per-file tracking
    file_details = []
    uncovered_functions = []
    current_file = None
    current_file_lines_covered = 0
    current_file_lines_total = 0
    current_file_funcs_covered = 0
    current_file_funcs_total = 0
    current_file_has_coverage = False
    # Track function names defined in current file (FN records)
    current_file_fn_names = {}  # line_num -> name

    def _flush_current_file():
        nonlocal files_tested
        if current_file is None:
            return
        if current_file_has_coverage:
            files_tested += 1

        file_line_pct = (
            (current_file_lines_covered / current_file_lines_total * 100)
            if current_file_lines_total > 0
            else 0.0
        )
        file_func_pct = (
            (current_file_funcs_covered / current_file_funcs_total * 100)
            if current_file_funcs_total > 0
            else 0.0
        )

        file_details.append(
            {
                "path": strip_workspace_prefix(current_file),
                "lines_covered": current_file_lines_covered,
                "lines_total": current_file_lines_total,
                "line_coverage_percent": round(file_line_pct, 2),
                "functions_covered": current_file_funcs_covered,
                "functions_total": current_file_funcs_total,
                "function_coverage_percent": round(file_func_pct, 2),
            }
        )

    with open(lcov_path) as f:
        for line in f:
            line = line.strip()

            if line.startswith("SF:"):
                # Flush previous file before starting new one
                _flush_current_file()
                # Start of a new source file
                current_file = line[3:]
                files_total += 1
                current_file_lines_covered = 0
                current_file_lines_total = 0
                current_file_funcs_covered = 0
                current_file_funcs_total = 0
                current_file_has_coverage = False
                current_file_fn_names = {}

            elif line.startswith("DA:"):
                # Line coverage: DA:line_number,execution_count
                parts = line[3:].split(",")
                if len(parts) >= 2:
                    lines_total += 1
                    current_file_lines_total += 1
                    exec_count = int(parts[1])
                    if exec_count > 0:
                        lines_covered += 1
                        current_file_lines_covered += 1
                        current_file_has_coverage = True

            elif line.startswith("FN:"):
                # Function definition: FN:line_number,function_name
                parts = line[3:].split(",", 1)
                if len(parts) >= 2:
                    functions_total += 1
                    current_file_funcs_total += 1
                    current_file_fn_names[parts[0]] = parts[1]

            elif line.startswith("FNDA:"):
                # Function coverage: FNDA:execution_count,function_name
                parts = line[5:].split(",", 1)
                if len(parts) >= 2:
                    exec_count = int(parts[0])
                    fn_name = parts[1]
                    if exec_count > 0:
                        functions_covered += 1
                        current_file_funcs_covered += 1
                    else:
                        uncovered_functions.append(
                            {
                                "name": fn_name,
                                "file": strip_workspace_prefix(current_file or ""),
                            }
                        )

            elif line.startswith("FNF:"):
                # Functions found (summary): FNF:count
                # Bun emits FNF/FNH instead of individual FN/FNDA records
                fnf = int(line[4:])
                if current_file_funcs_total == 0:
                    # Only use FNF if we didn't get individual FN records
                    current_file_funcs_total = fnf
                    functions_total += fnf

            elif line.startswith("FNH:"):
                # Functions hit (summary): FNH:count
                fnh = int(line[4:])
                if current_file_funcs_covered == 0 and fnh > 0:
                    current_file_funcs_covered = fnh
                    functions_covered += fnh

            elif line == "end_of_record":
                _flush_current_file()
                current_file = None

    # Flush any remaining file
    _flush_current_file()

    # Sort files by coverage ascending, functions by file then name
    file_details.sort(key=lambda f: (f["line_coverage_percent"], f["path"]))
    uncovered_functions.sort(key=lambda f: (f["file"], f["name"]))

    return {
        "lines_covered": lines_covered,
        "lines_total": lines_total,
        "functions_covered": functions_covered,
        "functions_total": functions_total,
        "files_tested": files_tested,
        "files_total": files_total,
        "file_details": file_details,
        "uncovered_functions": uncovered_functions,
    }


def normalize_bun_coverage(coverage_dir: Path) -> dict:
    """Convert Bun coverage output to normalized schema."""
    # Bun can output LCOV format with --coverage-dir
    lcov_file = coverage_dir / "lcov.info"
    lcov_path = None

    if lcov_file.exists():
        lcov_path = lcov_file
    else:
        # Fall back to looking for any .info file
        info_files = list(coverage_dir.glob("*.info"))
        if info_files:
            lcov_path = info_files[0]

    if lcov_path:
        parsed = parse_lcov(lcov_path)
    else:
        # No LCOV data found, return empty metrics
        print(
            f"Warning: No LCOV coverage file found in {coverage_dir}",
            file=sys.stderr,
        )
        parsed = {
            "lines_covered": 0,
            "lines_total": 0,
            "functions_covered": 0,
            "functions_total": 0,
            "files_tested": 0,
            "files_total": 0,
            "file_details": [],
            "uncovered_functions": [],
        }

    # Calculate percentages
    lines_covered = parsed["lines_covered"]
    lines_total = parsed["lines_total"]
    functions_covered = parsed["functions_covered"]
    functions_total = parsed["functions_total"]

    line_pct = (lines_covered / lines_total * 100) if lines_total > 0 else 0.0
    function_pct = (
        (functions_covered / functions_total * 100) if functions_total > 0 else 0.0
    )

    git_commit, git_branch = get_git_info()

    return {
        "meta": {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "crate": "tasker-worker-ts",
            "language": "typescript",
            "tool": "bun-coverage",
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
        "files_tested": parsed["files_tested"],
        "files_total": parsed["files_total"],
        "files": parsed["file_details"],
        "uncovered_functions": parsed["uncovered_functions"],
    }


def main():
    parser = argparse.ArgumentParser(description="Normalize TypeScript coverage")
    parser.add_argument("coverage_dir", type=Path, help="Bun coverage output directory")
    parser.add_argument("output", type=Path, help="Output normalized JSON file")
    args = parser.parse_args()

    if not args.coverage_dir.exists():
        print(f"Error: Coverage directory not found: {args.coverage_dir}", file=sys.stderr)
        sys.exit(1)

    normalized = normalize_bun_coverage(args.coverage_dir)

    # Ensure output directory exists
    args.output.parent.mkdir(parents=True, exist_ok=True)

    with open(args.output, "w") as f:
        json.dump(normalized, f, indent=2)

    print(f"Normalized coverage written to: {args.output}")
    print(f"  Line coverage: {normalized['summary']['line_coverage_percent']}%")
    print(f"  Function coverage: {normalized['summary']['function_coverage_percent']}%")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
Normalize SimpleCov JSON output to standard coverage schema.

Usage:
    python normalize-ruby.py <input_json> <output_json>
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
    markers = ["/workers/ruby/", "/tasker-core/workers/ruby/"]
    for marker in markers:
        idx = filepath.find(marker)
        if idx != -1:
            return filepath[idx + len(marker) :]
    # Fallback: try to find lib/ or spec/ or app/
    for prefix in ["/lib/", "/app/", "/spec/"]:
        idx = filepath.find(prefix)
        if idx != -1:
            return filepath[idx + 1 :]
    return filepath


def normalize_simplecov(input_path: Path) -> dict:
    """Convert SimpleCov .resultset.json to normalized schema."""
    with open(input_path) as f:
        raw_data = json.load(f)

    # SimpleCov .resultset.json structure:
    # { "RSpec": { "coverage": { "file.rb": { "lines": [...] } }, "timestamp": ... } }
    # Each line value is: null (not executable), 0 (not covered), or >0 (covered)

    lines_covered = 0
    lines_total = 0
    files_tested = 0
    files_total = 0
    file_details = []

    # Find the test run data (usually "RSpec" key)
    for run_name, run_data in raw_data.items():
        if not isinstance(run_data, dict):
            continue
        coverage = run_data.get("coverage", {})

        for file_path, file_data in coverage.items():
            files_total += 1

            # Handle both old and new SimpleCov formats
            if isinstance(file_data, dict):
                # New format: { "lines": [...] }
                line_array = file_data.get("lines", [])
            elif isinstance(file_data, list):
                # Old format: [...]
                line_array = file_data
            else:
                continue

            file_lines_covered = 0
            file_lines_total = 0
            file_has_coverage = False

            for line in line_array:
                if line is None:
                    continue  # Not an executable line
                file_lines_total += 1
                if line > 0:
                    file_lines_covered += 1
                    file_has_coverage = True

            lines_covered += file_lines_covered
            lines_total += file_lines_total

            if file_has_coverage:
                files_tested += 1

            file_line_pct = (
                (file_lines_covered / file_lines_total * 100)
                if file_lines_total > 0
                else 0.0
            )

            file_details.append(
                {
                    "path": strip_workspace_prefix(file_path),
                    "lines_covered": file_lines_covered,
                    "lines_total": file_lines_total,
                    "line_coverage_percent": round(file_line_pct, 2),
                }
            )

    # Sort by line coverage ascending — worst-covered files first
    file_details.sort(key=lambda f: (f["line_coverage_percent"], f["path"]))

    # Calculate percentages
    line_pct = (lines_covered / lines_total * 100) if lines_total > 0 else 0.0

    git_commit, git_branch = get_git_info()

    return {
        "meta": {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "crate": "tasker-worker-rb",
            "language": "ruby",
            "tool": "simplecov",
            "git_commit": git_commit,
            "git_branch": git_branch,
        },
        "summary": {
            "lines_covered": lines_covered,
            "lines_total": lines_total,
            "line_coverage_percent": round(line_pct, 2),
            "functions_covered": 0,  # SimpleCov doesn't track functions
            "functions_total": 0,
            "function_coverage_percent": 0.0,
        },
        "files_tested": files_tested,
        "files_total": files_total,
        "files": file_details,
    }


def main():
    parser = argparse.ArgumentParser(description="Normalize Ruby SimpleCov JSON")
    parser.add_argument("input", type=Path, help="Input SimpleCov .resultset.json file")
    parser.add_argument("output", type=Path, help="Output normalized JSON file")
    args = parser.parse_args()

    if not args.input.exists():
        print(f"Error: Input file not found: {args.input}", file=sys.stderr)
        sys.exit(1)

    normalized = normalize_simplecov(args.input)

    # Ensure output directory exists
    args.output.parent.mkdir(parents=True, exist_ok=True)

    with open(args.output, "w") as f:
        json.dump(normalized, f, indent=2)

    print(f"Normalized coverage written to: {args.output}")
    print(f"  Line coverage: {normalized['summary']['line_coverage_percent']}%")


if __name__ == "__main__":
    main()

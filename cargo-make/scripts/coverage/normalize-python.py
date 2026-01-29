#!/usr/bin/env python3
"""
Normalize pytest-cov JSON output to standard coverage schema.

Usage:
    python normalize-python.py <input_json> <output_json>
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


def extract_file_details(raw_files: dict) -> list[dict]:
    """Extract per-file coverage details from pytest-cov files dict.

    Returns files sorted by line_coverage_percent ascending (worst coverage first).
    """
    file_details = []
    for filepath, file_data in raw_files.items():
        summary = file_data.get("summary", {})
        lines_covered = summary.get("covered_lines", 0)
        lines_total = summary.get("num_statements", 0)
        line_pct = summary.get("percent_covered", 0.0)
        missing = file_data.get("missing_lines", [])
        excluded = file_data.get("excluded_lines", [])

        entry = {
            "path": filepath,
            "lines_covered": lines_covered,
            "lines_total": lines_total,
            "line_coverage_percent": round(line_pct, 2),
            "missing_lines": len(missing),
            "excluded_lines": len(excluded),
        }
        file_details.append(entry)

    # Sort by line coverage ascending — worst-covered files first
    file_details.sort(key=lambda f: (f["line_coverage_percent"], f["path"]))
    return file_details


def normalize_pytest_cov(input_path: Path) -> dict:
    """Convert pytest-cov JSON to normalized schema."""
    with open(input_path) as f:
        raw_data = json.load(f)

    # pytest-cov JSON structure: { "totals": {...}, "files": {...} }
    totals = raw_data.get("totals", {})

    lines_covered = totals.get("covered_lines", 0)
    lines_total = totals.get("num_statements", 0)
    # pytest-cov doesn't track functions separately, so we leave them as 0
    functions_covered = 0
    functions_total = 0

    # Calculate percentages
    line_pct = totals.get("percent_covered", 0.0)

    # Count files and extract details
    raw_files = raw_data.get("files", {})
    files_total = len(raw_files)
    files_tested = sum(
        1
        for f_data in raw_files.values()
        if f_data.get("summary", {}).get("covered_lines", 0) > 0
    )
    file_details = extract_file_details(raw_files)

    git_commit, git_branch = get_git_info()

    return {
        "meta": {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "crate": "tasker-core-py",
            "language": "python",
            "tool": "pytest-cov",
            "git_commit": git_commit,
            "git_branch": git_branch,
        },
        "summary": {
            "lines_covered": lines_covered,
            "lines_total": lines_total,
            "line_coverage_percent": round(line_pct, 2),
            "functions_covered": functions_covered,
            "functions_total": functions_total,
            "function_coverage_percent": 0.0,
        },
        "files_tested": files_tested,
        "files_total": files_total,
        "files": file_details,
    }


def main():
    parser = argparse.ArgumentParser(description="Normalize Python coverage JSON")
    parser.add_argument("input", type=Path, help="Input pytest-cov JSON file")
    parser.add_argument("output", type=Path, help="Output normalized JSON file")
    args = parser.parse_args()

    if not args.input.exists():
        print(f"Error: Input file not found: {args.input}", file=sys.stderr)
        sys.exit(1)

    normalized = normalize_pytest_cov(args.input)

    # Ensure output directory exists
    args.output.parent.mkdir(parents=True, exist_ok=True)

    with open(args.output, "w") as f:
        json.dump(normalized, f, indent=2)

    print(f"Normalized coverage written to: {args.output}")
    print(f"  Line coverage: {normalized['summary']['line_coverage_percent']}%")


if __name__ == "__main__":
    main()

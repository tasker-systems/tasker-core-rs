#!/usr/bin/env python3
"""
Aggregate coverage reports from all languages into a single report.

Usage:
    python aggregate.py --output coverage-reports/aggregate-coverage.json
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


def load_thresholds() -> dict:
    """Load coverage thresholds from configuration."""
    thresholds_path = Path("coverage-thresholds.json")
    if thresholds_path.exists():
        with open(thresholds_path) as f:
            return json.load(f)
    return {}


def find_coverage_reports(base_dir: Path) -> list[Path]:
    """Find all normalized coverage JSON files."""
    reports = []

    for lang_dir in ["rust", "python", "ruby", "typescript"]:
        lang_path = base_dir / lang_dir
        if lang_path.exists():
            for json_file in lang_path.glob("*-coverage.json"):
                # Skip raw files (e.g., tasker-shared-raw.json, raw-coverage.json)
                if "-raw" not in json_file.stem and not json_file.stem.startswith("raw"):
                    reports.append(json_file)

    return reports


def aggregate_reports(
    reports: list[Path], thresholds: dict, worst_files_limit: int = 30
) -> dict:
    """Aggregate all coverage reports into a single summary."""
    git_commit, git_branch = get_git_info()

    aggregate = {
        "meta": {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "git_commit": git_commit,
            "git_branch": git_branch,
            "report_count": len(reports),
        },
        "summary": {
            "total_lines_covered": 0,
            "total_lines": 0,
            "overall_line_coverage_percent": 0.0,
            "crates_passing": 0,
            "crates_failing": 0,
        },
        "crates": {},
        "lowest_coverage_files": [],
        "uncovered_files": [],
    }

    total_lines_covered = 0
    total_lines = 0
    passing = 0
    failing = 0

    # Collect all file-level details across crates for cross-cutting analysis
    all_files = []
    all_uncovered_files = []

    for report_path in reports:
        try:
            with open(report_path) as f:
                report = json.load(f)
        except json.JSONDecodeError:
            print(f"Warning: Could not parse {report_path}", file=sys.stderr)
            continue

        crate_name = report.get("meta", {}).get("crate", report_path.stem)
        language = report.get("meta", {}).get("language", "unknown")
        summary = report.get("summary", {})

        lines_covered = summary.get("lines_covered", 0)
        lines_total = summary.get("lines_total", 0)
        line_pct = summary.get("line_coverage_percent", 0.0)

        total_lines_covered += lines_covered
        total_lines += lines_total

        # Check against threshold
        lang_thresholds = thresholds.get(language, {})
        threshold = lang_thresholds.get(crate_name, 0)
        passes = line_pct >= threshold

        if passes:
            passing += 1
        else:
            failing += 1

        aggregate["crates"][crate_name] = {
            "language": language,
            "lines_covered": lines_covered,
            "lines_total": lines_total,
            "line_coverage_percent": line_pct,
            "threshold": threshold,
            "passes_threshold": passes,
            "source_file": str(report_path),
        }

        # Collect file-level details from this report
        for file_entry in report.get("files", []):
            file_with_crate = {
                "crate": crate_name,
                "language": language,
                **file_entry,
            }
            if file_entry.get("lines_covered", 0) == 0 and file_entry.get("lines_total", 0) > 0:
                all_uncovered_files.append(file_with_crate)
            all_files.append(file_with_crate)

    # Calculate overall
    overall_pct = (total_lines_covered / total_lines * 100) if total_lines > 0 else 0.0

    aggregate["summary"]["total_lines_covered"] = total_lines_covered
    aggregate["summary"]["total_lines"] = total_lines
    aggregate["summary"]["overall_line_coverage_percent"] = round(overall_pct, 2)
    aggregate["summary"]["crates_passing"] = passing
    aggregate["summary"]["crates_failing"] = failing

    # Sort all files by coverage ascending, take worst N
    # Exclude files with 0 total lines (headers, etc.)
    covered_files = [f for f in all_files if f.get("lines_total", 0) > 0]
    covered_files.sort(
        key=lambda f: (f.get("line_coverage_percent", 0.0), f.get("path", ""))
    )
    aggregate["lowest_coverage_files"] = covered_files[:worst_files_limit]

    # Uncovered files sorted by total lines descending (biggest gaps first)
    all_uncovered_files.sort(key=lambda f: -f.get("lines_total", 0))
    aggregate["uncovered_files"] = all_uncovered_files

    return aggregate


def main():
    parser = argparse.ArgumentParser(description="Aggregate coverage reports")
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("coverage-reports/aggregate-coverage.json"),
        help="Output file path",
    )
    parser.add_argument(
        "--reports-dir",
        type=Path,
        default=Path("coverage-reports"),
        help="Directory containing coverage reports",
    )
    args = parser.parse_args()

    if not args.reports_dir.exists():
        print(f"Error: Reports directory not found: {args.reports_dir}", file=sys.stderr)
        sys.exit(1)

    thresholds = load_thresholds()
    reports = find_coverage_reports(args.reports_dir)

    if not reports:
        print("Warning: No coverage reports found", file=sys.stderr)

    aggregate = aggregate_reports(reports, thresholds)

    # Ensure output directory exists
    args.output.parent.mkdir(parents=True, exist_ok=True)

    with open(args.output, "w") as f:
        json.dump(aggregate, f, indent=2)

    # Print summary
    summary = aggregate["summary"]
    print("=" * 70)
    print("Coverage Aggregate Report")
    print("=" * 70)
    print(f"Overall line coverage: {summary['overall_line_coverage_percent']}%")
    print(f"Total lines: {summary['total_lines']:,}")
    print(f"Lines covered: {summary['total_lines_covered']:,}")
    print(f"Crates passing: {summary['crates_passing']}")
    print(f"Crates failing: {summary['crates_failing']}")
    print()

    # Print per-crate summary
    print("Per-crate coverage:")
    print("-" * 70)
    for crate_name, crate_data in sorted(aggregate["crates"].items()):
        status = "PASS" if crate_data["passes_threshold"] else "FAIL"
        print(
            f"  {crate_name}: {crate_data['line_coverage_percent']}% "
            f"(threshold: {crate_data['threshold']}%) [{status}]"
        )
    print()

    # Print uncovered files (biggest gaps)
    uncovered = aggregate.get("uncovered_files", [])
    if uncovered:
        print(f"Uncovered files ({len(uncovered)} files with 0% coverage):")
        print("-" * 70)
        for entry in uncovered[:15]:
            print(
                f"  [{entry['crate']}] {entry['path']}  "
                f"({entry.get('lines_total', 0)} lines)"
            )
        if len(uncovered) > 15:
            print(f"  ... and {len(uncovered) - 15} more (see JSON report)")
        print()

    # Print lowest coverage files
    lowest = aggregate.get("lowest_coverage_files", [])
    if lowest:
        # Show files that have some but low coverage (exclude 0%)
        low_partial = [f for f in lowest if f.get("line_coverage_percent", 0) > 0][:15]
        if low_partial:
            print("Lowest coverage files (with partial coverage):")
            print("-" * 70)
            for entry in low_partial:
                print(
                    f"  [{entry['crate']}] {entry['path']}  "
                    f"{entry.get('line_coverage_percent', 0)}% "
                    f"({entry.get('lines_covered', 0)}/{entry.get('lines_total', 0)} lines)"
                )
            print()

    print(f"Report written to: {args.output}")


if __name__ == "__main__":
    main()

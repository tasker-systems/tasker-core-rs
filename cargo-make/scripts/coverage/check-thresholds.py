#!/usr/bin/env python3
"""
Check coverage reports against configured thresholds.

Usage:
    python check-thresholds.py [--aggregate coverage-reports/aggregate-coverage.json]

Exit codes:
    0 - All crates pass thresholds
    1 - One or more crates fail thresholds
"""

import argparse
import json
import sys
from pathlib import Path


def load_thresholds() -> dict:
    """Load coverage thresholds from configuration."""
    thresholds_path = Path("coverage-thresholds.json")
    if not thresholds_path.exists():
        print(f"Warning: Thresholds file not found: {thresholds_path}", file=sys.stderr)
        return {}

    with open(thresholds_path) as f:
        return json.load(f)


def check_aggregate_report(report_path: Path, thresholds: dict) -> bool:
    """Check aggregate report against thresholds."""
    if not report_path.exists():
        print(f"Error: Aggregate report not found: {report_path}", file=sys.stderr)
        return False

    with open(report_path) as f:
        aggregate = json.load(f)

    all_pass = True
    failing_crates = []

    print("=" * 70)
    print("Coverage Threshold Check")
    print("=" * 70)
    print()

    for crate_name, crate_data in sorted(aggregate.get("crates", {}).items()):
        language = crate_data.get("language", "unknown")
        coverage = crate_data.get("line_coverage_percent", 0.0)

        # Get threshold for this crate
        lang_thresholds = thresholds.get(language, {})
        threshold = lang_thresholds.get(crate_name, 0)

        passes = coverage >= threshold

        if passes:
            status = "PASS"
            icon = "+"
        else:
            status = "FAIL"
            icon = "x"
            all_pass = False
            failing_crates.append((crate_name, coverage, threshold))

        print(f"[{icon}] {crate_name:<30} {coverage:>6.2f}% >= {threshold:>5.1f}%  [{status}]")

    print()
    print("-" * 70)

    if all_pass:
        print("All crates pass coverage thresholds")
        print()
        return True
    else:
        print("FAILURE: The following crates are below threshold:")
        print()
        for crate_name, coverage, threshold in failing_crates:
            gap = threshold - coverage
            print(f"  {crate_name}: {coverage:.2f}% (need {gap:.2f}% more to reach {threshold}%)")
        print()
        return False


def check_individual_reports(reports_dir: Path, thresholds: dict) -> bool:
    """Check individual coverage reports against thresholds."""
    all_pass = True

    print("=" * 70)
    print("Coverage Threshold Check (Individual Reports)")
    print("=" * 70)
    print()

    for lang_dir in ["rust", "python", "ruby", "typescript"]:
        lang_path = reports_dir / lang_dir
        if not lang_path.exists():
            continue

        lang_thresholds = thresholds.get(
            lang_dir if lang_dir != "typescript" else "typescript", {}
        )

        for json_file in sorted(lang_path.glob("*-coverage.json")):
            if "-raw" in json_file.stem:
                continue

            try:
                with open(json_file) as f:
                    report = json.load(f)
            except json.JSONDecodeError:
                print(f"[?] Could not parse: {json_file}")
                continue

            crate_name = report.get("meta", {}).get("crate", json_file.stem)
            coverage = report.get("summary", {}).get("line_coverage_percent", 0.0)

            threshold = lang_thresholds.get(crate_name, 0)
            passes = coverage >= threshold

            if passes:
                status = "PASS"
                icon = "+"
            else:
                status = "FAIL"
                icon = "x"
                all_pass = False

            print(
                f"[{icon}] {crate_name:<30} {coverage:>6.2f}% >= {threshold:>5.1f}%  [{status}]"
            )

    print()
    return all_pass


def main():
    parser = argparse.ArgumentParser(description="Check coverage thresholds")
    parser.add_argument(
        "--aggregate",
        type=Path,
        default=Path("coverage-reports/aggregate-coverage.json"),
        help="Path to aggregate coverage report",
    )
    parser.add_argument(
        "--reports-dir",
        type=Path,
        default=Path("coverage-reports"),
        help="Directory containing coverage reports (fallback if no aggregate)",
    )
    args = parser.parse_args()

    thresholds = load_thresholds()

    # Prefer aggregate report if it exists
    if args.aggregate.exists():
        success = check_aggregate_report(args.aggregate, thresholds)
    elif args.reports_dir.exists():
        success = check_individual_reports(args.reports_dir, thresholds)
    else:
        print("Error: No coverage reports found", file=sys.stderr)
        sys.exit(1)

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()

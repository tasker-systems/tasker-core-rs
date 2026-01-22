#!/usr/bin/env python3
"""
TAS-159: Criterion benchmark percentile extraction

Processes sample.json files from Criterion benchmark output to calculate
per-iteration latencies and compute p50, p95, p99 percentiles.

Usage:
    ./cargo-make/scripts/bench-percentiles.py [target/criterion]
"""

import json
import os
import sys
from pathlib import Path
from typing import Optional


def extract_per_iteration_times(sample_data: dict) -> list[float]:
    """
    Extract per-iteration times from Criterion sample data.

    Criterion has two sampling modes:
    - "Linear": cumulative iterations [5, 10, 15...], cumulative times
    - "Flat": per-sample iterations [4, 4, 4...], per-sample times

    Returns list of per-iteration times in milliseconds.
    """
    sampling_mode = sample_data.get("sampling_mode", "Linear")
    iters = sample_data.get("iters", [])
    times = sample_data.get("times", [])

    if not iters or not times or len(iters) != len(times):
        return []

    per_iteration_times = []

    if sampling_mode == "Flat":
        # Flat mode: each entry is a separate sample
        # time[i] = total time for iters[i] iterations
        for batch_iters, batch_time in zip(iters, times):
            if batch_iters > 0:
                # Convert nanoseconds to milliseconds
                avg_time_ms = (batch_time / batch_iters) / 1_000_000
                per_iteration_times.append(avg_time_ms)
    else:
        # Linear mode: cumulative iterations and times
        prev_iters = 0
        prev_time = 0.0

        for cum_iters, cum_time in zip(iters, times):
            delta_iters = cum_iters - prev_iters
            delta_time = cum_time - prev_time

            if delta_iters > 0:
                # Convert nanoseconds to milliseconds
                avg_time_ms = (delta_time / delta_iters) / 1_000_000
                per_iteration_times.append(avg_time_ms)

            prev_iters = cum_iters
            prev_time = cum_time

    return per_iteration_times


def calculate_percentiles(times: list[float]) -> dict[str, Optional[float]]:
    """Calculate p50, p95, p99 percentiles from a list of times."""
    if not times:
        return {"p50": None, "p95": None, "p99": None}

    sorted_times = sorted(times)
    n = len(sorted_times)

    def percentile(p: float) -> float:
        k = (n - 1) * p
        f = int(k)
        c = f + 1 if f + 1 < n else f
        return sorted_times[f] + (sorted_times[c] - sorted_times[f]) * (k - f)

    return {
        "p50": round(percentile(0.50), 3),
        "p95": round(percentile(0.95), 3) if n >= 20 else None,
        "p99": round(percentile(0.99), 3) if n >= 100 else None,
    }


def find_sample_files(criterion_dir: Path) -> list[tuple[str, Path]]:
    """Find all sample.json files and extract benchmark names."""
    results = []

    for sample_file in criterion_dir.rglob("new/sample.json"):
        # Extract benchmark path: e2e_tier1_core/workflow/linear_rust -> tier1_core/linear_rust
        parts = sample_file.parts
        try:
            criterion_idx = parts.index("criterion")
            bench_parts = parts[criterion_idx + 1:-2]  # Exclude 'new/sample.json'

            # Clean up the name (e.g., e2e_tier1_core -> tier1_core)
            tier_name = bench_parts[0].replace("e2e_", "")
            # Include all parts after tier name (e.g., ruby/linear, workflow/diamond_rust)
            scenario_parts = bench_parts[1:]
            scenario_name = "/".join(scenario_parts)

            bench_name = f"{tier_name}/{scenario_name}"
            results.append((bench_name, sample_file))
        except (ValueError, IndexError):
            continue

    return sorted(results, key=lambda x: x[0])


def process_benchmark(name: str, sample_file: Path) -> dict:
    """Process a single benchmark's sample.json file."""
    try:
        with open(sample_file) as f:
            sample_data = json.load(f)

        times = extract_per_iteration_times(sample_data)
        percentiles = calculate_percentiles(times)

        # Also read estimates.json for mean/median
        estimates_file = sample_file.parent / "estimates.json"
        mean_ms = None
        median_ms = None

        if estimates_file.exists():
            with open(estimates_file) as f:
                estimates = json.load(f)
            mean_ms = round(estimates.get("mean", {}).get("point_estimate", 0) / 1_000_000, 3)
            median_ms = round(estimates.get("median", {}).get("point_estimate", 0) / 1_000_000, 3)

        return {
            "name": name,
            "samples": len(times),
            "mean_ms": mean_ms,
            "median_ms": median_ms,
            "p50_ms": percentiles["p50"],
            "p95_ms": percentiles["p95"],
            "p99_ms": percentiles["p99"],
        }
    except Exception as e:
        return {"name": name, "error": str(e)}


def print_report(results: list[dict]):
    """Print a formatted report of benchmark results."""
    print("\n" + "=" * 100)
    print("BENCHMARK PERCENTILE REPORT")
    print("=" * 100)
    print()

    # Group by tier
    tiers = {}
    for r in results:
        tier = r["name"].split("/")[0]
        if tier not in tiers:
            tiers[tier] = []
        tiers[tier].append(r)

    for tier_name, tier_results in sorted(tiers.items()):
        print(f"## {tier_name.upper()}")
        print("-" * 100)
        print(f"{'Scenario':<40} {'Samples':>8} {'Mean':>10} {'Median':>10} {'P50':>10} {'P95':>10} {'P99':>10}")
        print("-" * 100)

        for r in tier_results:
            if "error" in r:
                print(f"{r['name']:<40} ERROR: {r['error']}")
                continue

            scenario = r["name"].split("/", 1)[1] if "/" in r["name"] else r["name"]
            samples = r.get("samples", 0)
            mean = f"{r['mean_ms']:.1f}ms" if r.get("mean_ms") else "-"
            median = f"{r['median_ms']:.1f}ms" if r.get("median_ms") else "-"
            p50 = f"{r['p50_ms']:.1f}ms" if r.get("p50_ms") else "-"
            p95 = f"{r['p95_ms']:.1f}ms" if r.get("p95_ms") else f"(n<20)"
            p99 = f"{r['p99_ms']:.1f}ms" if r.get("p99_ms") else f"(n<100)"

            print(f"{scenario:<40} {samples:>8} {mean:>10} {median:>10} {p50:>10} {p95:>10} {p99:>10}")

        print()

    print("=" * 100)
    print("Note: P95 requires ≥20 samples, P99 requires ≥100 samples for statistical validity")
    print("=" * 100)


def main():
    criterion_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("target/criterion")

    if not criterion_dir.exists():
        print(f"Error: Criterion directory not found: {criterion_dir}")
        print("Run benchmarks first: cargo bench")
        sys.exit(1)

    print(f"Scanning {criterion_dir} for benchmark results...")

    sample_files = find_sample_files(criterion_dir)
    if not sample_files:
        print("No benchmark results found.")
        sys.exit(1)

    print(f"Found {len(sample_files)} benchmark results")

    results = [process_benchmark(name, path) for name, path in sample_files]
    print_report(results)

    # Also output JSON for programmatic use
    json_output = criterion_dir / "percentile_report.json"
    with open(json_output, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nJSON report saved to: {json_output}")


if __name__ == "__main__":
    main()

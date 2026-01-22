#!/usr/bin/env python3
"""
Firefox Profiler JSON Analysis Tool

Analyzes samply-generated Firefox Profiler JSON files to extract:
- Top functions by CPU time (self time and total time)
- Hot path analysis
- Thread breakdown

Usage:
    python3 analyze_profile.py <profile.json> [--top N] [--filter PATTERN]

Examples:
    python3 analyze_profile.py profile.json
    python3 analyze_profile.py profile.json --top 30
    python3 analyze_profile.py profile.json --filter tasker
    python3 analyze_profile.py profile.json --filter "clone|alloc|serde"
"""

import json
import argparse
import re
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


@dataclass
class FunctionStats:
    name: str
    self_samples: int = 0
    total_samples: int = 0

    @property
    def self_pct(self) -> float:
        return 0.0

    @property
    def total_pct(self) -> float:
        return 0.0


class ProfileAnalyzer:
    """Analyzer for Firefox Profiler JSON format."""

    def __init__(self, profile_path: str):
        self.profile_path = Path(profile_path)
        self.data = None
        self.global_string_table = []

    def load(self):
        """Load the profile JSON."""
        with open(self.profile_path, 'r') as f:
            self.data = json.load(f)

        # Extract global string table
        # Firefox Profiler format v24+ uses shared.stringArray
        shared = self.data.get('shared', {})
        self.global_string_table = shared.get('stringArray', [])

        # Fallback to older format
        if not self.global_string_table:
            self.global_string_table = self.data.get('stringTable', [])

        return self

    def get_meta(self) -> dict:
        """Get profile metadata."""
        meta = self.data.get('meta', {})
        return {
            'product': meta.get('product', 'unknown'),
            'start_time': meta.get('startTime'),
            'interval_ms': meta.get('interval', 1),
            'symbolicated': meta.get('symbolicated', False),
        }

    def get_threads(self) -> list:
        """Get list of threads with sample counts."""
        threads = []
        for i, thread in enumerate(self.data.get('threads', [])):
            samples = thread.get('samples', {})
            sample_count = len(samples.get('stack', []))
            threads.append({
                'index': i,
                'name': thread.get('name', f'Thread {i}'),
                'samples': sample_count,
                'is_main': thread.get('isMainThread', False),
            })
        return sorted(threads, key=lambda x: x['samples'], reverse=True)

    def get_string(self, thread: dict, index: int) -> str:
        """Get string from thread's string table or global table."""
        # Try thread-local string table first
        local_st = thread.get('stringTable', [])
        if local_st and index < len(local_st):
            return local_st[index]

        # Fall back to global string table
        if index < len(self.global_string_table):
            return self.global_string_table[index]

        return f"<unknown:{index}>"

    def analyze_thread(self, thread_index: int, filter_pattern: Optional[str] = None) -> dict:
        """Analyze a specific thread's samples."""
        thread = self.data['threads'][thread_index]
        thread_name = thread.get('name', f'Thread {thread_index}')

        # Get tables
        samples = thread.get('samples', {})
        sample_stacks = samples.get('stack', [])
        sample_count = len(sample_stacks)

        if sample_count == 0:
            return {'name': thread_name, 'samples': 0, 'functions': []}

        stack_table = thread.get('stackTable', {})
        stack_frames = stack_table.get('frame', [])
        stack_prefixes = stack_table.get('prefix', [])

        frame_table = thread.get('frameTable', {})
        frame_funcs = frame_table.get('func', [])

        func_table = thread.get('funcTable', {})
        func_names = func_table.get('name', [])

        # Count function occurrences
        self_counts = Counter()  # Function at top of stack (self time)
        total_counts = Counter()  # Function anywhere in stack (total time)

        for stack_idx in sample_stacks:
            if stack_idx is None:
                continue

            # Walk the stack
            visited = set()
            current = stack_idx
            is_top = True

            while current is not None and current not in visited:
                visited.add(current)

                if current < len(stack_frames):
                    frame_idx = stack_frames[current]
                    if frame_idx < len(frame_funcs):
                        func_idx = frame_funcs[frame_idx]
                        if func_idx < len(func_names):
                            name_idx = func_names[func_idx]
                            func_name = self.get_string(thread, name_idx)

                            total_counts[func_name] += 1
                            if is_top:
                                self_counts[func_name] += 1
                                is_top = False

                if current < len(stack_prefixes):
                    current = stack_prefixes[current]
                else:
                    break

        # Build results
        all_funcs = set(self_counts.keys()) | set(total_counts.keys())
        functions = []

        filter_re = re.compile(filter_pattern, re.IGNORECASE) if filter_pattern else None

        for func_name in all_funcs:
            if filter_re and not filter_re.search(func_name):
                continue

            functions.append({
                'name': func_name,
                'self_samples': self_counts.get(func_name, 0),
                'self_pct': (self_counts.get(func_name, 0) / sample_count) * 100,
                'total_samples': total_counts.get(func_name, 0),
                'total_pct': (total_counts.get(func_name, 0) / sample_count) * 100,
            })

        # Sort by total samples
        functions.sort(key=lambda x: x['total_samples'], reverse=True)

        return {
            'name': thread_name,
            'samples': sample_count,
            'functions': functions,
        }

    def analyze_all_threads(self, min_samples: int = 100, filter_pattern: Optional[str] = None) -> list:
        """Analyze all threads with minimum sample threshold."""
        results = []
        for i, thread in enumerate(self.data.get('threads', [])):
            samples = thread.get('samples', {})
            if len(samples.get('stack', [])) >= min_samples:
                results.append(self.analyze_thread(i, filter_pattern))
        return sorted(results, key=lambda x: x['samples'], reverse=True)


def print_report(analyzer: ProfileAnalyzer, top_n: int = 20, filter_pattern: Optional[str] = None):
    """Print analysis report to stdout."""
    meta = analyzer.get_meta()

    print("=" * 70)
    print(f"PROFILE: {meta['product']}")
    print(f"Symbolicated: {meta['symbolicated']}")
    print("=" * 70)

    threads = analyzer.get_threads()
    print(f"\nThreads: {len(threads)}")
    print("\nThread Summary (by sample count):")
    for t in threads[:10]:
        marker = " [main]" if t['is_main'] else ""
        print(f"  {t['samples']:>7} samples: {t['name']}{marker}")

    # Analyze threads with significant samples
    print("\n" + "=" * 70)
    print("DETAILED ANALYSIS")
    print("=" * 70)

    results = analyzer.analyze_all_threads(min_samples=100, filter_pattern=filter_pattern)

    for thread_result in results:
        print(f"\n{'─' * 70}")
        print(f"Thread: {thread_result['name']} ({thread_result['samples']} samples)")
        print(f"{'─' * 70}")

        functions = thread_result['functions'][:top_n]

        if not functions:
            print("  (no matching functions)")
            continue

        print(f"\n{'Self%':>7} {'Total%':>7}  Function")
        print(f"{'─'*7} {'─'*7}  {'─'*50}")

        for func in functions:
            name = func['name']
            # Truncate long names
            if len(name) > 70:
                name = name[:67] + "..."
            print(f"{func['self_pct']:>6.1f}% {func['total_pct']:>6.1f}%  {name}")


def print_summary(analyzer: ProfileAnalyzer, top_n: int = 30, filter_pattern: Optional[str] = None):
    """Print aggregated summary across all worker threads."""
    meta = analyzer.get_meta()

    print("=" * 70)
    print(f"PROFILE SUMMARY: {meta['product']}")
    print("=" * 70)

    # Aggregate across all tokio-runtime-worker threads
    aggregated_self = Counter()
    aggregated_total = Counter()
    total_samples = 0
    worker_threads = 0

    for i, thread in enumerate(analyzer.data.get('threads', [])):
        thread_name = thread.get('name', '')
        samples = thread.get('samples', {})
        sample_count = len(samples.get('stack', []))

        # Focus on worker threads
        if 'tokio-runtime-worker' in thread_name and sample_count > 100:
            worker_threads += 1
            result = analyzer.analyze_thread(i, filter_pattern)
            total_samples += result['samples']

            for func in result['functions']:
                aggregated_self[func['name']] += func['self_samples']
                aggregated_total[func['name']] += func['total_samples']

    print(f"\nAggregated from {worker_threads} tokio-runtime-worker threads")
    print(f"Total samples: {total_samples}")

    if total_samples == 0:
        print("No samples found in worker threads.")
        return

    # Build sorted list
    all_funcs = set(aggregated_self.keys()) | set(aggregated_total.keys())
    functions = []

    for func_name in all_funcs:
        functions.append({
            'name': func_name,
            'self_samples': aggregated_self.get(func_name, 0),
            'self_pct': (aggregated_self.get(func_name, 0) / total_samples) * 100,
            'total_samples': aggregated_total.get(func_name, 0),
            'total_pct': (aggregated_total.get(func_name, 0) / total_samples) * 100,
        })

    # Sort by self time (where CPU is actually spent)
    functions.sort(key=lambda x: x['self_samples'], reverse=True)

    print(f"\nTop {top_n} Functions by Self Time (where CPU is spent):")
    print(f"{'─' * 70}")
    print(f"{'Self%':>7} {'Total%':>7} {'Self#':>7}  Function")
    print(f"{'─'*7} {'─'*7} {'─'*7}  {'─'*50}")

    for func in functions[:top_n]:
        name = func['name']
        if len(name) > 60:
            name = name[:57] + "..."
        print(f"{func['self_pct']:>6.2f}% {func['total_pct']:>6.2f}% {func['self_samples']:>7}  {name}")

    # Also show by total time
    functions.sort(key=lambda x: x['total_samples'], reverse=True)

    print(f"\nTop {top_n} Functions by Total Time (including callees):")
    print(f"{'─' * 70}")
    print(f"{'Self%':>7} {'Total%':>7} {'Total#':>7}  Function")
    print(f"{'─'*7} {'─'*7} {'─'*7}  {'─'*50}")

    for func in functions[:top_n]:
        name = func['name']
        if len(name) > 60:
            name = name[:57] + "..."
        print(f"{func['self_pct']:>6.2f}% {func['total_pct']:>6.2f}% {func['total_samples']:>7}  {name}")


def main():
    parser = argparse.ArgumentParser(
        description='Analyze Firefox Profiler JSON files from samply',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    parser.add_argument('profile', help='Path to profile JSON file')
    parser.add_argument('--top', type=int, default=20, help='Number of top functions to show (default: 20)')
    parser.add_argument('--filter', type=str, help='Regex pattern to filter function names')
    parser.add_argument('--summary', action='store_true', help='Show aggregated summary across worker threads')
    parser.add_argument('--detailed', action='store_true', help='Show detailed per-thread analysis')

    args = parser.parse_args()

    if not Path(args.profile).exists():
        print(f"Error: Profile not found: {args.profile}", file=sys.stderr)
        sys.exit(1)

    analyzer = ProfileAnalyzer(args.profile)
    analyzer.load()

    if args.summary or not args.detailed:
        print_summary(analyzer, args.top, args.filter)

    if args.detailed:
        print_report(analyzer, args.top, args.filter)


if __name__ == '__main__':
    main()

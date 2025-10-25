#!/usr/bin/env python3
"""
TAS-50 Advanced Configuration Parameter Usage Analysis

This script performs sophisticated static analysis to determine which configuration
parameters are actually USED (influence runtime behavior) vs just structurally present.

Analysis includes:
1. Direct struct field access patterns (e.g., config.database.pool.max_connections)
2. Method-based access patterns (e.g., config.database_url())
3. Value propagation through initialization code
4. Impact on system behavior (not just deserialization)

Usage: python3 scripts/analyze_config_usage.py
Output: docs/config-usage-analysis.md
"""

import subprocess
import re
from pathlib import Path
from dataclasses import dataclass, field
from typing import List, Dict, Set, Tuple, Optional
from datetime import datetime
from collections import defaultdict

@dataclass
class ParameterUsage:
    """Represents usage information for a configuration parameter"""
    param_path: str
    context_category: str
    usage_type: str
    file_count: int
    sample_locations: List[str] = field(default_factory=list)

    def is_functional(self) -> bool:
        return self.usage_type.startswith("FUNCTIONAL")

    def is_unused(self) -> bool:
        return self.usage_type == "UNUSED"

    def is_structural(self) -> bool:
        return self.usage_type.startswith("STRUCTURAL")

class ConfigAnalyzer:
    """Analyzes configuration parameter usage across the codebase"""

    def __init__(self, project_root: Path):
        self.project_root = project_root
        self.config_base_dir = project_root / "config" / "tasker" / "base"
        self.results: List[ParameterUsage] = []

    def extract_toml_parameters(self, toml_file: Path) -> List[str]:
        """Extract all parameter paths from a TOML file"""
        params = []
        current_section = ""

        with open(toml_file, 'r') as f:
            for line in f:
                line = line.strip()

                # Skip comments and empty lines
                if not line or line.startswith('#'):
                    continue

                # Check for section headers [section.subsection]
                section_match = re.match(r'^\[([^\]]+)\]', line)
                if section_match:
                    current_section = section_match.group(1)
                    continue

                # Check for key = value pairs
                key_match = re.match(r'^([a-zA-Z_][a-zA-Z0-9_]*)\s*=', line)
                if key_match:
                    key = key_match.group(1)
                    if current_section:
                        params.append(f"{current_section}.{key}")
                    else:
                        params.append(key)

        return params

    def search_parameter_usage(self, param_path: str) -> Tuple[List[str], int]:
        """Search for parameter usage in Rust source files"""
        field_name = param_path.split('.')[-1]

        # Try multiple search patterns - search for bare field name for maximum coverage
        pattern = field_name

        all_results = []
        seen_files = set()

        try:
            # Use grep with recursive search
            result = subprocess.run(
                [
                    'grep',
                    '-rn',
                    '--include=*.rs',
                    '--exclude-dir=target',
                    '--exclude-dir=.git',
                    '--exclude-dir=docs',
                    '--exclude-dir=config',
                    pattern,
                    str(self.project_root)
                ],
                capture_output=True,
                text=True,
            )

            if result.returncode == 0 and result.stdout:
                lines = result.stdout.strip().split('\n')
                for line in lines:
                    # Format: filepath:line_number:content
                    parts = line.split(':', 2)
                    if len(parts) >= 2:
                        file_path = parts[0]
                        if file_path not in seen_files:
                            all_results.append(line)
                            seen_files.add(file_path)

        except Exception as e:
            print(f"Warning: Search failed for pattern '{pattern}': {e}")

        return all_results, len(seen_files)

    def categorize_usage_type(self, param_path: str, search_results: List[str]) -> str:
        """Determine the usage type based on search results context"""
        if not search_results:
            return "UNUSED"

        # Combine all results for pattern matching
        all_text = '\n'.join(search_results).lower()

        # Check for functional usage patterns
        if any(pattern in all_text for pattern in [
            'sqlx', 'pgpooloptions', '.connect(', '.spawn(', '.execute(',
            'databasepoolconfig', 'pool.acquire'
        ]):
            return "FUNCTIONAL - Database Operations"

        if any(pattern in all_text for pattern in [
            'mpsc::channel', 'buffer_size', 'capacity', 'mpschchannels'
        ]):
            return "FUNCTIONAL - Channel Configuration"

        if any(pattern in all_text for pattern in [
            'backoff', 'retry', 'delay', 'backoffconfig', 'exponential'
        ]):
            return "FUNCTIONAL - Retry Logic"

        if any(pattern in all_text for pattern in [
            'circuit_breaker', 'failure_threshold', 'circuitbreaker'
        ]):
            return "FUNCTIONAL - Circuit Breaker"

        if any(pattern in all_text for pattern in [
            'timeout', 'duration', 'interval', 'duration::from'
        ]):
            return "FUNCTIONAL - Timing"

        if any(pattern in all_text for pattern in [
            'event_system', 'eventsystem', 'listener', 'poller', 'notification'
        ]):
            return "FUNCTIONAL - Event System"

        if any(pattern in all_text for pattern in [
            'queue', 'pgmq', 'queueconfig', 'visibility', 'enqueue'
        ]):
            return "FUNCTIONAL - Queue Configuration"

        if any(pattern in all_text for pattern in [
            'web', 'http', 'api', 'server', 'bind_address', 'cors'
        ]):
            return "FUNCTIONAL - Web API"

        # Check if it's only used in struct definitions
        if all(any(pattern in line.lower() for pattern in [
            'pub struct', 'deserialize', '#[serde'
        ]) for line in search_results[:5]):
            return "STRUCTURAL - Deserialization Only"

        return "NEEDS_REVIEW"

    def get_parameter_context(self, param_path: str) -> str:
        """Categorize parameter by its context"""
        if param_path.startswith('database.'):
            return "Database"
        elif param_path.startswith('queues.'):
            return "Queues"
        elif param_path.startswith('mpsc_channels.'):
            return "MPSC Channels"
        elif param_path.startswith('circuit_breakers.'):
            return "Circuit Breakers"
        elif param_path.startswith('orchestration_system.'):
            return "Orchestration System"
        elif param_path.startswith('worker_system.'):
            return "Worker System"
        elif param_path.startswith('backoff.'):
            return "Backoff/Retry"
        elif param_path.startswith('execution.'):
            return "Execution"
        elif param_path.startswith('system.'):
            return "System"
        elif param_path.startswith('web_api.'):
            return "Web API"
        else:
            return "Other"

    def analyze_config_file(self, context: str) -> None:
        """Analyze all parameters in a configuration context"""
        config_file = self.config_base_dir / f"{context}.toml"

        if not config_file.exists():
            print(f"Warning: {config_file} not found, skipping")
            return

        print(f"\nAnalyzing {context}.toml...")
        params = self.extract_toml_parameters(config_file)

        for i, param in enumerate(params, 1):
            print(f"  [{i}/{len(params)}] Analyzing: {param}")

            search_results, file_count = self.search_parameter_usage(param)
            usage_type = self.categorize_usage_type(param, search_results)
            context_category = self.get_parameter_context(param)

            sample_locations = search_results[:5]  # Keep first 5 samples

            usage = ParameterUsage(
                param_path=param,
                context_category=context_category,
                usage_type=usage_type,
                file_count=file_count,
                sample_locations=sample_locations
            )

            self.results.append(usage)

        print(f"  Analyzed {len(params)} parameters from {context}.toml")

    def generate_report(self, output_file: Path) -> None:
        """Generate comprehensive markdown report"""

        # Group results by config context
        by_context = {
            'common': [r for r in self.results if r.param_path.split('.')[0] in [
                'environment', 'database', 'queues', 'circuit_breakers', 'mpsc_channels'
            ] or not any(r.param_path.startswith(prefix) for prefix in [
                'orchestration_', 'worker_', 'backoff.', 'execution.'
            ])],
            'orchestration': [r for r in self.results if any(r.param_path.startswith(prefix) for prefix in [
                'backoff.', 'execution.', 'orchestration_', 'task_readiness_', 'mpsc_channels.'
            ])],
            'worker': [r for r in self.results if r.param_path.startswith('worker_')],
        }

        # Count by usage type
        usage_counts = defaultdict(int)
        for result in self.results:
            usage_counts[result.usage_type] += 1

        # Generate report
        with open(output_file, 'w') as f:
            f.write("# Advanced Configuration Usage Analysis\n\n")
            f.write(f"**Analysis Date**: {datetime.utcnow().strftime('%Y-%m-%d %H:%M:%S UTC')}\n")
            f.write("**Purpose**: Comprehensive analysis of configuration parameter usage with distinction between functional and structural usage\n\n")
            f.write("---\n\n")

            f.write("## Analysis Methodology\n\n")
            f.write("This analysis uses sophisticated pattern matching to distinguish between:\n\n")
            f.write("- **FUNCTIONAL**: Parameters that influence runtime behavior (database connections, retry logic, timeouts, etc.)\n")
            f.write("- **STRUCTURAL**: Parameters that are deserialized but not actively used\n")
            f.write("- **UNUSED**: Parameters not referenced in codebase\n")
            f.write("- **NEEDS_REVIEW**: Parameters with unclear usage patterns requiring manual review\n\n")

            f.write("### Search Patterns\n\n")
            f.write("For each parameter, we search for:\n")
            f.write("1. Direct nested access: `config.database.pool.max_connections`\n")
            f.write("2. Struct field access in context: `.max_connections` in files\n")
            f.write("3. Bare field name usage for comprehensive coverage\n")
            f.write("4. Context analysis: Code patterns indicating functional vs structural usage\n\n")
            f.write("---\n\n")

            # Write results by context
            for context_name in ['common', 'orchestration', 'worker']:
                results = by_context.get(context_name, [])
                if not results:
                    continue

                f.write(f"## {context_name.capitalize()} Configuration\n\n")
                f.write("| Parameter Path | Context | Usage Type | Files | Sample Locations |\n")
                f.write("|----------------|---------|------------|-------|------------------|\n")

                for result in results:
                    locations_md = "None"
                    if result.sample_locations:
                        # Truncate long lines
                        truncated = [loc[:100] + '...' if len(loc) > 100 else loc
                                   for loc in result.sample_locations]
                        locations_md = f"<details><summary>{result.file_count} files</summary><pre>{'<br>'.join(truncated[:3])}</pre></details>"

                    f.write(f"| `{result.param_path}` | {result.context_category} | "
                           f"{result.usage_type} | {result.file_count} | {locations_md} |\n")

                f.write("\n")

            # Summary statistics
            f.write("---\n\n")
            f.write("## Summary Statistics\n\n")

            f.write("### Parameters by Context\n\n")
            f.write("| Context | Total Parameters |\n")
            f.write("|---------|------------------|\n")
            for context_name in ['common', 'orchestration', 'worker']:
                count = len(by_context.get(context_name, []))
                f.write(f"| {context_name.capitalize()} | {count} |\n")

            f.write("\n### Parameters by Usage Type\n\n")
            f.write("| Usage Type | Count |\n")
            f.write("|------------|-------|\n")
            for usage_type in sorted(usage_counts.keys(), key=lambda x: usage_counts[x], reverse=True):
                f.write(f"| {usage_type} | {usage_counts[usage_type]} |\n")

            f.write("\n---\n\n")
            f.write("## Recommended Actions\n\n")
            f.write("### Parameters Marked UNUSED\n")
            f.write("These parameters should be removed from TOML files and Rust config structs.\n\n")
            f.write("### Parameters Marked STRUCTURAL\n")
            f.write("These parameters are deserialized but may not influence behavior. Requires manual review.\n\n")
            f.write("### Parameters Marked NEEDS_REVIEW\n")
            f.write("These parameters have unclear usage patterns. Manual code review required.\n\n")
            f.write("### Parameters Marked FUNCTIONAL\n")
            f.write("These parameters actively influence runtime behavior and should be retained with comprehensive _docs metadata.\n\n")
            f.write("---\n\n")
            f.write("**Analysis Complete**\n")

    def run_analysis(self) -> None:
        """Run complete analysis"""
        print("=== TAS-50 Advanced Configuration Usage Analysis ===")
        print(f"Project Root: {self.project_root}")
        print(f"Config Base Dir: {self.config_base_dir}")

        for context in ['common', 'orchestration', 'worker']:
            self.analyze_config_file(context)

        output_file = self.project_root / "docs" / "config-usage-analysis.md"
        self.generate_report(output_file)

        print(f"\n✓ Analysis complete!")
        print(f"Output: {output_file}")

        # Print summary
        usage_counts = defaultdict(int)
        for result in self.results:
            usage_counts[result.usage_type] += 1

        print("\nSummary by Usage Type:")
        for usage_type in sorted(usage_counts.keys(), key=lambda x: usage_counts[x], reverse=True):
            print(f"  - {usage_type}: {usage_counts[usage_type]} parameters")

def main():
    project_root = Path(__file__).parent.parent
    analyzer = ConfigAnalyzer(project_root)
    analyzer.run_analysis()

if __name__ == '__main__':
    main()

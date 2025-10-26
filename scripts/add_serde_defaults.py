#!/usr/bin/env python3
"""
TAS-50 Add #[serde(default)] to Structs with Unused Fields

Adds #[serde(default)] attribute to structs that have UNUSED fields,
allowing TOML deserialization to succeed even when fields are missing.
"""

import re
from pathlib import Path
from typing import Set, Dict, List

class SerdeDefaultAdder:
    def __init__(self, project_root: Path):
        self.project_root = project_root
        self.analysis_file = project_root / "docs" / "config-usage-analysis.md"
        # Map from struct name to set of unused field names
        self.struct_unused_fields: Dict[str, Set[str]] = {}

    def parse_analysis(self) -> None:
        """Extract UNUSED parameters and determine which structs they belong to"""
        print("Parsing config-usage-analysis.md...")

        with open(self.analysis_file, 'r') as f:
            content = f.read()

        # Find all UNUSED parameters
        pattern = r'\| `([^`]+)` \| [^|]+ \| UNUSED \|'
        matches = re.findall(pattern, content)

        # Determine struct names from parameter paths
        # e.g., "orchestration_system.web.resource_monitoring.pool_usage_warning_threshold"
        # -> struct likely WebResourceMonitoringConfig

        struct_field_map = {
            'queues_populated_at_runtime': 'OrchestrationEventSystemMetadata',
            'max_total_connections_hint': 'WebDatabasePoolsConfig',
            'report_pool_usage_to_health_monitor': 'WebResourceMonitoringConfig',
            'pool_usage_warning_threshold': 'WebResourceMonitoringConfig',
            'pool_usage_critical_threshold': 'WebResourceMonitoringConfig',
            'max_request_size_mb': 'WebConfig',
            'max_age_seconds': 'WebCorsConfig',
            'startup_timeout_seconds': 'EnhancedCoordinatorSettings',
            'shutdown_timeout_seconds': 'EnhancedCoordinatorSettings',
            'rollback_threshold_percent': 'EnhancedCoordinatorSettings',
            'global_channels': 'TaskReadinessNotificationConfig',
            'max_payload_size_bytes': 'TaskReadinessNotificationConfig',
            'parse_timeout_ms': 'TaskReadinessNotificationConfig',
            'max_connection_retries': 'ConnectionConfig',
            'connection_retry_delay_seconds': 'ConnectionConfig',
            'auto_reconnect': 'ConnectionConfig',
            'instance_id_prefix': 'TaskReadinessCoordinatorConfig',
            'operation_timeout_ms': 'TaskReadinessCoordinatorConfig',
            'stats_interval_seconds': 'TaskReadinessCoordinatorConfig',
            'broadcast_buffer_size': 'InProcessEventConfig',
            'retry_backoff_multiplier': 'StepProcessingConfig',
            'heartbeat_interval_seconds': 'StepProcessingConfig',
            'metrics_collection_enabled': 'HealthMonitoringConfig',
            'step_processing_rate_threshold': 'HealthMonitoringConfig',
            'memory_usage_threshold_mb': 'HealthMonitoringConfig',
        }

        for param_path in matches:
            field_name = param_path.split('.')[-1]
            if field_name in struct_field_map:
                struct_name = struct_field_map[field_name]
                if struct_name not in self.struct_unused_fields:
                    self.struct_unused_fields[struct_name] = set()
                self.struct_unused_fields[struct_name].add(field_name)

        print(f"Found {len(self.struct_unused_fields)} structs with unused fields:")
        for struct_name, fields in sorted(self.struct_unused_fields.items()):
            print(f"  - {struct_name}: {len(fields)} unused fields")

    def add_serde_default_to_struct(self, file_path: Path, struct_name: str) -> bool:
        """Add #[serde(default)] to a struct if not already present"""

        with open(file_path, 'r') as f:
            content = f.read()

        # Find the struct definition
        # Pattern: #[derive(...)] followed by pub struct StructName {
        struct_pattern = rf'(#\[derive\([^\]]+\)\])\s*\n(\s*)pub struct {re.escape(struct_name)}\s*[<{{]'

        match = re.search(struct_pattern, content)
        if not match:
            return False

        derive_attr = match.group(1)

        # Check if #[serde(default)] already exists
        if '#[serde(default)]' in content[max(0, match.start()-200):match.end()]:
            return False

        # Add #[serde(default)] after the #[derive(...)] line
        indentation = match.group(2)
        new_attr = f"{derive_attr}\n{indentation}#[serde(default)]"

        new_content = content[:match.start()] + content[match.start():match.end()].replace(
            derive_attr, new_attr, 1
        ) + content[match.end():]

        with open(file_path, 'w') as f:
            f.write(new_content)

        return True

    def process_files(self) -> None:
        """Add #[serde(default)] to all structs with unused fields"""
        config_dir = self.project_root / "tasker-shared" / "src" / "config"

        files_modified = 0
        structs_modified = 0

        for rust_file in config_dir.rglob("*.rs"):
            for struct_name in self.struct_unused_fields.keys():
                if self.add_serde_default_to_struct(rust_file, struct_name):
                    print(f"  ✓ Added #[serde(default)] to {struct_name} in {rust_file.name}")
                    files_modified += 1
                    structs_modified += 1

        print(f"\n✓ Modified {structs_modified} structs across config files")

    def run(self) -> None:
        """Run the complete process"""
        print("=== TAS-50 Serde Default Adder ===\n")

        self.parse_analysis()
        self.process_files()

        print("\n✓ Process complete!")
        print("\nNext steps:")
        print("  1. Review changes: git diff tasker-shared/src/config/")
        print("  2. Compile: cargo build --workspace --all-features")
        print("  3. Run tests: cargo test --workspace --all-features")

def main():
    project_root = Path(__file__).parent.parent
    adder = SerdeDefaultAdder(project_root)
    adder.run()

if __name__ == '__main__':
    main()

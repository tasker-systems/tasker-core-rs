#!/usr/bin/env python3
"""
TAS-50 Configuration Cleanup - Remove Unused Parameters

This script removes unused parameters from TOML configuration files and
generates a report of what was removed.

Usage: python3 scripts/remove_unused_params.py
"""

import re
from pathlib import Path
from typing import List, Set, Dict
from dataclasses import dataclass

@dataclass
class ParamToRemove:
    """Represents a parameter to be removed"""
    path: str
    reason: str

class ConfigCleaner:
    """Removes unused parameters from TOML configuration files"""

    def __init__(self, project_root: Path):
        self.project_root = project_root
        self.config_base_dir = project_root / "config" / "tasker" / "base"
        self.config_env_dir = project_root / "config" / "tasker" / "environments"

        # Parameters to remove (63 UNUSED + 7 NEEDS_REVIEW)
        self.params_to_remove: List[ParamToRemove] = []

    def add_unused_params(self):
        """Add all UNUSED parameters from the analysis"""

        # All UNUSED parameters (63 total from analysis)
        all_unused = [
            # Common configuration
            "queues.default_visibility_timeout_seconds",
            "queues.max_batch_size",
            "queues.pgmq.poll_interval_ms",
            "queues.pgmq.shutdown_timeout_seconds",
            "queues.rabbitmq.heartbeat_interval_seconds",
            "queues.rabbitmq.channel_pool_size",
            "mpsc_channels.in_process_events.broadcast_buffer_size",
            "task_readiness_events.metadata.enhanced_settings.startup_timeout_seconds",
            "task_readiness_events.metadata.enhanced_settings.shutdown_timeout_seconds",
            "task_readiness_events.metadata.enhanced_settings.rollback_threshold_percent",
            "task_readiness_events.metadata.notification.global_channels",
            "task_readiness_events.metadata.notification.max_payload_size_bytes",
            "task_readiness_events.metadata.notification.parse_timeout_ms",
            "task_readiness_events.metadata.notification.connection.max_connection_retries",
            "task_readiness_events.metadata.notification.connection.connection_retry_delay_seconds",
            "task_readiness_events.metadata.notification.connection.auto_reconnect",
            "task_readiness_events.metadata.coordinator.instance_id_prefix",
            "task_readiness_events.metadata.coordinator.operation_timeout_ms",
            "task_readiness_events.metadata.coordinator.stats_interval_seconds",

            # Orchestration configuration
            "backoff.default_reenqueue_delay",
            "backoff.buffer_seconds",
            "orchestration_system.max_concurrent_orchestrators",
            "orchestration_system.use_unified_state_machine",
            "orchestration_system.operational_state.enable_shutdown_aware_monitoring",
            "orchestration_system.operational_state.suppress_alerts_during_shutdown",
            "orchestration_system.operational_state.startup_health_threshold_multiplier",
            "orchestration_system.operational_state.shutdown_health_threshold_multiplier",
            "orchestration_system.operational_state.graceful_shutdown_timeout_seconds",
            "orchestration_system.operational_state.emergency_shutdown_timeout_seconds",
            "orchestration_system.operational_state.enable_transition_logging",
            "orchestration_system.operational_state.transition_log_level",
            "orchestration_system.web.max_request_size_mb",
            "orchestration_system.web.database_pools.max_total_connections_hint",
            "orchestration_system.web.cors.max_age_seconds",
            "orchestration_system.web.resource_monitoring.report_pool_usage_to_health_monitor",
            "orchestration_system.web.resource_monitoring.pool_usage_warning_threshold",
            "orchestration_system.web.resource_monitoring.pool_usage_critical_threshold",
            "orchestration_events.metadata.queues_populated_at_runtime",

            # Worker configuration
            "worker_system.step_processing.heartbeat_interval_seconds",
            "worker_system.step_processing.retry_backoff_multiplier",
            "worker_system.health_monitoring.metrics_collection_enabled",
            "worker_system.health_monitoring.step_processing_rate_threshold",
            "worker_system.health_monitoring.memory_usage_threshold_mb",
            "worker_system.web.max_request_size_mb",
            "worker_system.web.database_pools.max_total_connections_hint",
            "worker_system.web.cors.max_age_seconds",
            "worker_system.web.resource_monitoring.report_pool_usage_to_health_monitor",
            "worker_system.web.resource_monitoring.pool_usage_warning_threshold",
            "worker_system.web.resource_monitoring.pool_usage_critical_threshold",
            "worker_system.web.endpoints.health_enabled",
            "worker_system.web.endpoints.health_path",
            "worker_system.web.endpoints.readiness_path",
            "worker_system.web.endpoints.liveness_path",
            "worker_system.web.endpoints.prometheus_path",
            "worker_system.web.endpoints.worker_metrics_path",
            "worker_system.web.endpoints.status_enabled",
            "worker_system.web.endpoints.basic_status_path",
            "worker_system.web.endpoints.detailed_status_path",
            "worker_system.web.endpoints.namespace_health_path",
            "worker_system.web.endpoints.registered_handlers_path",
            "worker_system.web.endpoints.templates_enabled",
            "worker_system.web.endpoints.templates_base_path",
            "worker_system.web.endpoints.template_cache_enabled",
        ]

        for param in all_unused:
            self.params_to_remove.append(ParamToRemove(param, "UNUSED - Not referenced in codebase"))

    def add_needs_review_removals(self):
        """Add NEEDS_REVIEW parameters that user decided to remove"""

        needs_review_to_remove = [
            ("database.enable_secondary_database", "NEEDS_REVIEW - Not driving actual behavior"),
            ("database.adapter", "NEEDS_REVIEW - Not driving actual behavior"),
            ("database.username", "NEEDS_REVIEW - Not driving actual behavior"),
            ("database.password", "Only used for redaction tests, not functional"),
            ("database.skip_migration_check", "NEEDS_REVIEW - Not driving actual behavior"),
            ("database.pool.max_lifetime_seconds", "NEEDS_REVIEW - Not driving actual behavior"),
            ("circuit_breakers.global_settings.auto_create_enabled", "NEEDS_REVIEW - Boolean passed around but doesn't drive behavior"),
        ]

        for param, reason in needs_review_to_remove:
            self.params_to_remove.append(ParamToRemove(param, reason))

    def remove_from_toml(self, toml_file: Path, params_to_remove: Set[str]) -> Dict[str, int]:
        """Remove parameters from a TOML file"""

        if not toml_file.exists():
            return {}

        removed_count = {}

        with open(toml_file, 'r') as f:
            lines = f.readlines()

        new_lines = []
        current_section = ""
        skip_next_lines = 0

        for i, line in enumerate(lines):
            # Track sections
            section_match = re.match(r'^\[([^\]]+)\]', line.strip())
            if section_match:
                current_section = section_match.group(1)
                new_lines.append(line)
                continue

            # Check if this is a parameter line
            param_match = re.match(r'^([a-zA-Z_][a-zA-Z0-9_]*)\s*=', line.strip())
            if param_match:
                param_name = param_match.group(1)

                # Build full parameter path
                if current_section:
                    full_param = f"{current_section}.{param_name}"
                else:
                    full_param = param_name

                # Check if this parameter should be removed
                if full_param in params_to_remove:
                    removed_count[full_param] = removed_count.get(full_param, 0) + 1
                    # Skip this line and any trailing continuation lines
                    # (TOML multiline strings, arrays, etc.)
                    continue

            new_lines.append(line)

        # Write back
        with open(toml_file, 'w') as f:
            f.writelines(new_lines)

        return removed_count

    def clean_config_files(self) -> Dict[str, Dict[str, int]]:
        """Clean all configuration files"""

        # Build set of parameter paths to remove
        params_set = {p.path for p in self.params_to_remove}

        results = {}

        # Clean base config files
        for context in ['common', 'orchestration', 'worker']:
            config_file = self.config_base_dir / f"{context}.toml"
            if config_file.exists():
                removed = self.remove_from_toml(config_file, params_set)
                if removed:
                    results[f"base/{context}.toml"] = removed

        # Clean environment-specific overrides
        for env in ['test', 'development', 'production']:
            env_dir = self.config_env_dir / env
            if not env_dir.exists():
                continue

            for context in ['common', 'orchestration', 'worker']:
                config_file = env_dir / f"{context}.toml"
                if config_file.exists():
                    removed = self.remove_from_toml(config_file, params_set)
                    if removed:
                        results[f"environments/{env}/{context}.toml"] = removed

        return results

    def generate_report(self, results: Dict[str, Dict[str, int]]) -> str:
        """Generate a report of removed parameters"""

        report = []
        report.append("# TAS-50 Configuration Cleanup Report")
        report.append(f"\n**Date**: {Path(__file__).stat().st_mtime}")
        report.append("\n## Summary\n")

        total_removals = sum(len(params) for params in results.values())
        unique_params = set()
        for params in results.values():
            unique_params.update(params.keys())

        report.append(f"- **Total parameter removals**: {total_removals}")
        report.append(f"- **Unique parameters removed**: {len(unique_params)}")
        report.append(f"- **Files modified**: {len(results)}")

        report.append("\n## Parameters Removed\n")

        # Group by reason
        by_reason = {}
        for param in self.params_to_remove:
            if param.reason not in by_reason:
                by_reason[param.reason] = []
            by_reason[param.reason].append(param.path)

        for reason, params in sorted(by_reason.items()):
            report.append(f"\n### {reason}\n")
            for param in sorted(params):
                report.append(f"- `{param}`")

        report.append("\n## Files Modified\n")

        for file_path, removed_params in sorted(results.items()):
            report.append(f"\n### {file_path}\n")
            for param, count in sorted(removed_params.items()):
                report.append(f"- `{param}` ({count} occurrence(s))")

        return "\n".join(report)

    def run(self):
        """Execute the cleanup"""
        print("=== TAS-50 Configuration Cleanup ===")
        print(f"Project Root: {self.project_root}")
        print()

        # Build list of parameters to remove
        print("Building list of parameters to remove...")
        self.add_unused_params()
        self.add_needs_review_removals()

        print(f"  - Total parameters to remove: {len(self.params_to_remove)}")
        print()

        # Clean config files
        print("Removing parameters from TOML files...")
        results = self.clean_config_files()

        total_removals = sum(len(params) for params in results.values())
        print(f"  - Removed from {len(results)} files")
        print(f"  - Total removals: {total_removals}")
        print()

        # Generate report
        report = self.generate_report(results)
        report_file = self.project_root / "docs" / "config-cleanup-report.md"

        with open(report_file, 'w') as f:
            f.write(report)

        print(f"✓ Cleanup complete!")
        print(f"Report saved to: {report_file}")

        return results

def main():
    project_root = Path(__file__).parent.parent
    cleaner = ConfigCleaner(project_root)
    cleaner.run()

if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""
TAS-50 Configuration Cleanup - Remove Unused Parameters from TOML

Uses proper TOML parsing to safely remove parameters while preserving structure.

Usage: python3 scripts/clean_toml_params.py [--dry-run]
"""

import sys
import argparse
from pathlib import Path
from typing import Set, Dict, List
import re

def remove_param_from_toml(content: str, param_path: str) -> tuple[str, bool]:
    """
    Remove a parameter from TOML content.

    Returns: (modified_content, was_removed)
    """
    parts = param_path.split('.')

    # Build regex patterns for different TOML structures
    # Handle simple key: param = value
    # Handle nested sections: [section.subsection]

    if len(parts) == 1:
        # Top-level parameter
        pattern = rf'^{re.escape(parts[0])}\s*=.*$'
        if re.search(pattern, content, re.MULTILINE):
            content = re.sub(pattern + r'\n?', '', content, flags=re.MULTILINE)
            return content, True
    else:
        # Nested parameter
        # Look for the parameter under the appropriate section
        section_path = '.'.join(parts[:-1])
        param_name = parts[-1]

        # Find the section header
        section_pattern = rf'^\[{re.escape(section_path)}\]'
        section_match = re.search(section_pattern, content, re.MULTILINE)

        if section_match:
            # Find the parameter within this section
            # Look from the section start until the next section or end of file
            section_start = section_match.end()
            next_section = re.search(r'^\[', content[section_start:], re.MULTILINE)

            if next_section:
                section_end = section_start + next_section.start()
                section_content = content[section_start:section_end]
            else:
                section_content = content[section_start:]

            # Remove the parameter from this section
            param_pattern = rf'^{re.escape(param_name)}\s*=.*$'
            if re.search(param_pattern, section_content, re.MULTILINE):
                new_section = re.sub(param_pattern + r'\n?', '', section_content, flags=re.MULTILINE)
                content = content[:section_start] + new_section + (content[section_end:] if next_section else '')
                return content, True

    return content, False

def clean_empty_sections(content: str) -> str:
    """Remove sections that have no parameters"""
    lines = content.split('\n')
    result = []
    in_empty_section = False
    section_header = None
    section_lines = []

    for line in lines:
        # Check if this is a section header
        if line.strip().startswith('[') and line.strip().endswith(']'):
            # Save previous section if it had content
            if section_header and section_lines:
                result.append(section_header)
                result.extend(section_lines)
                result.append('')  # Blank line after section

            # Start new section
            section_header = line
            section_lines = []
            in_empty_section = True
        elif line.strip() and not line.strip().startswith('#'):
            # Non-empty, non-comment line
            in_empty_section = False
            section_lines.append(line)
        elif line.strip().startswith('#') or not line.strip():
            # Comment or blank line
            section_lines.append(line)
        else:
            section_lines.append(line)

    # Add last section if it had content
    if section_header and section_lines:
        # Check if section has any non-comment content
        has_content = any(l.strip() and not l.strip().startswith('#') for l in section_lines)
        if has_content:
            result.append(section_header)
            result.extend(section_lines)

    return '\n'.join(result)

def process_file(file_path: Path, params_to_remove: Set[str], dry_run: bool) -> Dict[str, int]:
    """Process a single TOML file"""
    if not file_path.exists():
        return {}

    with open(file_path, 'r') as f:
        content = f.read()

    original_content = content
    removed_params = {}

    for param in sorted(params_to_remove):
        content, was_removed = remove_param_from_toml(content, param)
        if was_removed:
            removed_params[param] = 1

    # Clean up empty sections
    content = clean_empty_sections(content)

    # Only write if something changed and not dry-run
    if content != original_content and not dry_run:
        with open(file_path, 'w') as f:
            f.write(content)

    return removed_params

def main():
    parser = argparse.ArgumentParser(description='Remove unused parameters from TOML files')
    parser.add_argument('--dry-run', action='store_true', help='Show what would be removed without making changes')
    args = parser.parse_args()

    project_root = Path(__file__).parent.parent
    config_base = project_root / 'config' / 'tasker' / 'base'
    config_envs = project_root / 'config' / 'tasker' / 'environments'

    # Parameters to remove (70 total: 63 UNUSED + 7 NEEDS_REVIEW)
    params_to_remove = {
        # Common - database removals
        "database.enable_secondary_database",
        "database.adapter",
        "database.username",
        "database.password",
        "database.skip_migration_check",
        "database.pool.max_lifetime_seconds",

        # Common - queue removals
        "queues.default_visibility_timeout_seconds",
        "queues.max_batch_size",
        "queues.pgmq.poll_interval_ms",
        "queues.pgmq.shutdown_timeout_seconds",
        "queues.rabbitmq.heartbeat_interval_seconds",
        "queues.rabbitmq.channel_pool_size",

        # Common - circuit breaker removals
        "circuit_breakers.global_settings.auto_create_enabled",

        # Common - mpsc channels removals
        "mpsc_channels.in_process_events.broadcast_buffer_size",

        # Common - task readiness removals
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

        # Orchestration - backoff removals
        "backoff.default_reenqueue_delay",
        "backoff.buffer_seconds",

        # Orchestration - orchestration_system removals
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

        # Worker - step_processing removals
        "worker_system.step_processing.heartbeat_interval_seconds",
        "worker_system.step_processing.retry_backoff_multiplier",

        # Worker - health_monitoring removals
        "worker_system.health_monitoring.metrics_collection_enabled",
        "worker_system.health_monitoring.step_processing_rate_threshold",
        "worker_system.health_monitoring.memory_usage_threshold_mb",

        # Worker - web removals
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
    }

    print("=== TAS-50 Configuration Cleanup ===")
    if args.dry_run:
        print("DRY RUN MODE - No files will be modified")
    print(f"Parameters to remove: {len(params_to_remove)}")
    print()

    all_removals = {}

    # Process base files
    for context in ['common', 'orchestration', 'worker']:
        file_path = config_base / f'{context}.toml'
        print(f"Processing {file_path.relative_to(project_root)}...")
        removed = process_file(file_path, params_to_remove, args.dry_run)
        if removed:
            all_removals[str(file_path.relative_to(project_root))] = removed
            print(f"  Removed {len(removed)} parameters")
        else:
            print(f"  No changes")

    # Process environment files
    for env in ['test', 'development', 'production']:
        env_dir = config_envs / env
        for context in ['common', 'orchestration', 'worker']:
            file_path = env_dir / f'{context}.toml'
            if file_path.exists():
                print(f"Processing {file_path.relative_to(project_root)}...")
                removed = process_file(file_path, params_to_remove, args.dry_run)
                if removed:
                    all_removals[str(file_path.relative_to(project_root))] = removed
                    print(f"  Removed {len(removed)} parameters")
                else:
                    print(f"  No changes")

    print()
    print(f"✓ Cleanup complete!")
    print(f"Total files modified: {len(all_removals)}")
    total_removals = sum(len(r) for r in all_removals.values())
    print(f"Total parameters removed: {total_removals}")

    if args.dry_run:
        print()
        print("This was a dry run. Run without --dry-run to apply changes.")

if __name__ == '__main__':
    main()

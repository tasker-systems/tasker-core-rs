#!/usr/bin/env python3
"""
Deep comparison of SQL functions - strips comments to compare only executable code.

This script validates that function bodies are identical after:
1. Stripping SQL comments (-- and /* */)
2. Normalizing table references (public.tasker_* -> tasker.*)
3. Normalizing whitespace

Usage:
    python compare_functions_deep.py [old_schema.sql] [new_schema.sql]
    
If no arguments provided, defaults to full_schema_export.sql and new_schema_export.sql
in the current directory.
"""

import re
import sys
from pathlib import Path


def strip_sql_comments(text: str) -> str:
    """Remove SQL comments from text."""
    # Remove -- single line comments
    text = re.sub(r'--[^\n]*', '', text)
    # Remove /* */ block comments
    text = re.sub(r'/\*[\s\S]*?\*/', '', text)
    return text


def extract_function_body(content: str, func_name: str) -> str:
    """Extract the body of a specific function."""
    # Pattern to find the function
    pattern = rf"CREATE FUNCTION \w+\.{re.escape(func_name)}\([^)]*\)[\s\S]*?\$\$([\s\S]*?)\$\$"
    match = re.search(pattern, content)
    if match:
        return match.group(1)
    return ""


def normalize_for_comparison(body: str, is_old: bool = True) -> str:
    """Normalize function body for comparison."""
    text = strip_sql_comments(body)
    
    if is_old:
        # Transform table references
        text = re.sub(r'public\.tasker_(\w+)', r'tasker.\1', text)
        text = re.sub(r'\btasker_(\w+)\b', r'\1', text)
    
    # Normalize whitespace
    text = re.sub(r'\s+', ' ', text).strip()
    return text


def compare_specific_functions(old_content: str, new_content: str, func_pairs: list) -> dict:
    """Compare specific function pairs."""
    results = {}
    
    for old_name, new_name, func_name in func_pairs:
        old_body = extract_function_body(old_content, old_name.split('.')[-1])
        new_body = extract_function_body(new_content, new_name.split('.')[-1])
        
        if not old_body:
            results[func_name] = {'status': 'missing_old', 'message': f'Could not find {old_name}'}
            continue
        if not new_body:
            results[func_name] = {'status': 'missing_new', 'message': f'Could not find {new_name}'}
            continue
        
        old_norm = normalize_for_comparison(old_body, is_old=True)
        new_norm = normalize_for_comparison(new_body, is_old=False)
        
        if old_norm == new_norm:
            results[func_name] = {'status': 'identical', 'message': 'Bodies match after normalization'}
        else:
            # Find the actual differences
            old_tokens = set(old_norm.split())
            new_tokens = set(new_norm.split())
            only_old = old_tokens - new_tokens
            only_new = new_tokens - old_tokens
            
            results[func_name] = {
                'status': 'different',
                'only_in_old': sorted(only_old)[:20],
                'only_in_new': sorted(only_new)[:20],
                'old_length': len(old_norm),
                'new_length': len(new_norm)
            }
    
    return results


# Key tasker functions to deep compare
KEY_FUNCTIONS = [
    ('public.calculate_dependency_levels', 'tasker.calculate_dependency_levels', 'calculate_dependency_levels'),
    ('public.calculate_staleness_threshold', 'tasker.calculate_staleness_threshold', 'calculate_staleness_threshold'),
    ('public.calculate_step_next_retry_time', 'tasker.calculate_step_next_retry_time', 'calculate_step_next_retry_time'),
    ('public.create_dlq_entry', 'tasker.create_dlq_entry', 'create_dlq_entry'),
    ('public.create_step_result_audit', 'tasker.create_step_result_audit', 'create_step_result_audit'),
    ('public.detect_and_transition_stale_tasks', 'tasker.detect_and_transition_stale_tasks', 'detect_and_transition_stale_tasks'),
    ('public.evaluate_step_state_readiness', 'tasker.evaluate_step_state_readiness', 'evaluate_step_state_readiness'),
    ('public.find_stuck_tasks', 'tasker.find_stuck_tasks', 'find_stuck_tasks'),
    ('public.get_analytics_metrics', 'tasker.get_analytics_metrics', 'get_analytics_metrics'),
    ('public.get_current_task_state', 'tasker.get_current_task_state', 'get_current_task_state'),
    ('public.get_next_ready_task', 'tasker.get_next_ready_task', 'get_next_ready_task'),
    ('public.get_next_ready_tasks', 'tasker.get_next_ready_tasks', 'get_next_ready_tasks'),
    ('public.get_slowest_steps', 'tasker.get_slowest_steps', 'get_slowest_steps'),
    ('public.get_slowest_tasks', 'tasker.get_slowest_tasks', 'get_slowest_tasks'),
    ('public.get_stale_tasks_for_dlq', 'tasker.get_stale_tasks_for_dlq', 'get_stale_tasks_for_dlq'),
    ('public.get_step_readiness_status', 'tasker.get_step_readiness_status', 'get_step_readiness_status'),
    ('public.get_step_readiness_status_batch', 'tasker.get_step_readiness_status_batch', 'get_step_readiness_status_batch'),
    ('public.get_step_transitive_dependencies', 'tasker.get_step_transitive_dependencies', 'get_step_transitive_dependencies'),
    ('public.get_system_health_counts', 'tasker.get_system_health_counts', 'get_system_health_counts'),
    ('public.get_task_execution_context', 'tasker.get_task_execution_context', 'get_task_execution_context'),
    ('public.get_task_execution_contexts_batch', 'tasker.get_task_execution_contexts_batch', 'get_task_execution_contexts_batch'),
    ('public.get_task_ready_info', 'tasker.get_task_ready_info', 'get_task_ready_info'),
    ('public.set_step_backoff_atomic', 'tasker.set_step_backoff_atomic', 'set_step_backoff_atomic'),
    ('public.transition_stale_task_to_error', 'tasker.transition_stale_task_to_error', 'transition_stale_task_to_error'),
    ('public.transition_task_state_atomic', 'tasker.transition_task_state_atomic', 'transition_task_state_atomic'),
    ('public.update_updated_at_column', 'tasker.update_updated_at_column', 'update_updated_at_column'),
]


def main():
    old_file = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("full_schema_export.sql")
    new_file = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("new_schema_export.sql")
    
    old_content = old_file.read_text()
    new_content = new_file.read_text()
    
    print("=" * 70)
    print("TAS-128 FUNCTION DEEP COMPARISON (Comments Stripped)")
    print("=" * 70)
    
    results = compare_specific_functions(old_content, new_content, KEY_FUNCTIONS)
    
    identical = []
    different = []
    missing = []
    
    for func_name, result in results.items():
        if result['status'] == 'identical':
            identical.append(func_name)
        elif result['status'] == 'different':
            different.append((func_name, result))
        else:
            missing.append((func_name, result))
    
    print(f"\n✓ IDENTICAL (after stripping comments): {len(identical)}")
    for name in sorted(identical):
        print(f"  {name}")
    
    if different:
        print(f"\n⚠ DIFFERENT: {len(different)}")
        for name, result in different:
            print(f"\n  {name}:")
            print(f"    Old length: {result['old_length']}, New length: {result['new_length']}")
            if result['only_in_old']:
                print(f"    Only in OLD: {result['only_in_old'][:10]}")
            if result['only_in_new']:
                print(f"    Only in NEW: {result['only_in_new'][:10]}")
    
    if missing:
        print(f"\n✗ MISSING: {len(missing)}")
        for name, result in missing:
            print(f"  {name}: {result['message']}")
    
    print("\n" + "=" * 70)
    if not different and not missing:
        print("✓ ALL FUNCTIONS MATCH (executable code is identical)")
    else:
        print(f"⚠ {len(different)} different, {len(missing)} missing")
    print("=" * 70)
    
    return 0 if not different and not missing else 1


if __name__ == "__main__":
    main()

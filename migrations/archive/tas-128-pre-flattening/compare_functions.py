#!/usr/bin/env python3
"""
Deep comparison of SQL functions between old and new schema exports.
Focuses on ensuring function bodies are correctly transformed.
"""

import re
import sys
from pathlib import Path
from collections import defaultdict
from difflib import unified_diff


def extract_functions_detailed(content: str) -> dict:
    """Extract complete function definitions including body."""
    functions = {}
    
    # Pattern to match CREATE FUNCTION ... $$ body $$ 
    # This is complex because function bodies can contain nested $$ or other delimiters
    pattern = r"CREATE FUNCTION (\w+)\.(\w+)\(([^)]*)\)\s+RETURNS\s+([^\n]+)\n\s+LANGUAGE\s+(\w+)\s*\n([\s\S]*?)(?=\n\n--|$)"
    
    for match in re.finditer(pattern, content):
        schema, name, params, returns, language, rest = match.groups()
        
        # Extract the function body between $$ delimiters
        body_match = re.search(r"\$\$\s*([\s\S]*?)\s*\$\$", rest)
        body = body_match.group(1).strip() if body_match else ""
        
        full_key = f"{schema}.{name}"
        functions[full_key] = {
            'schema': schema,
            'name': name,
            'params': params.strip(),
            'returns': returns.strip(),
            'language': language,
            'body': body,
            'full_match': match.group(0)
        }
    
    return functions


def normalize_function_body(body: str, is_old: bool = True) -> str:
    """Normalize function body for comparison."""
    text = body
    
    if is_old:
        # Transform table references: tasker_foo -> foo
        # But be careful not to transform things like 'tasker.' schema prefix
        
        # First, transform full references: public.tasker_foo -> tasker.foo
        text = re.sub(r'public\.tasker_(\w+)', r'tasker.\1', text)
        
        # Then transform unqualified table names in FROM/JOIN/INTO clauses
        # tasker_workflow_steps -> workflow_steps
        text = re.sub(r'\btasker_(\w+)\b', r'\1', text)
    
    # Normalize whitespace for comparison
    text = re.sub(r'\s+', ' ', text).strip()
    
    return text


def compare_function_bodies(old_func: dict, new_func: dict) -> list:
    """Compare two function bodies and return differences."""
    differences = []
    
    # Compare parameters
    old_params = old_func['params']
    new_params = new_func['params']
    if old_params != new_params:
        differences.append(f"  PARAMS differ:\n    OLD: {old_params}\n    NEW: {new_params}")
    
    # Compare return type
    old_returns = old_func['returns']
    new_returns = new_func['returns']
    if old_returns != new_returns:
        differences.append(f"  RETURNS differs:\n    OLD: {old_returns}\n    NEW: {new_returns}")
    
    # Compare normalized bodies
    old_body_norm = normalize_function_body(old_func['body'], is_old=True)
    new_body_norm = normalize_function_body(new_func['body'], is_old=False)
    
    if old_body_norm != new_body_norm:
        # Find specific differences
        old_lines = old_body_norm.split(';')
        new_lines = new_body_norm.split(';')
        
        differences.append(f"  BODY differs (showing key differences):")
        
        # Show a unified diff of the normalized bodies
        diff = list(unified_diff(
            old_body_norm.split(' '),
            new_body_norm.split(' '),
            lineterm='',
            n=2
        ))
        if diff:
            diff_str = ' '.join(diff[:50])  # First 50 tokens
            if len(diff) > 50:
                diff_str += f"... ({len(diff)} tokens total)"
            differences.append(f"    {diff_str}")
    
    return differences


def main():
    # Read files
    old_file = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("full_schema_export.sql")
    new_file = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("new_schema_export.sql")
    
    old_content = old_file.read_text()
    new_content = new_file.read_text()
    
    print("=" * 70)
    print("TAS-128 FUNCTION COMPARISON ANALYSIS")
    print("=" * 70)
    
    # Extract functions
    old_functions = extract_functions_detailed(old_content)
    new_functions = extract_functions_detailed(new_content)
    
    print(f"\nOld schema functions: {len(old_functions)}")
    print(f"New schema functions: {len(new_functions)}")
    
    # Build mapping from old names to new names
    # public.tasker_foo -> tasker.foo (remove tasker_ prefix)
    # public.foo -> tasker.foo (keep name, just move schema)
    
    def old_to_new_name(old_name: str) -> str:
        """Convert old function name to expected new name."""
        schema, fname = old_name.split('.', 1)
        if schema == 'public':
            # Check if it's a tasker function (should move to tasker schema)
            # But pgmq helper functions stay in public
            if fname.startswith(('pgmq_', 'extract_queue_namespace')):
                # These stay in public schema
                return f"public.{fname}"
            else:
                # These move to tasker schema
                return f"tasker.{fname}"
        return old_name
    
    # Track results
    matched = []
    missing_in_new = []
    extra_in_new = []
    with_differences = []
    
    # Check each old function
    for old_name, old_func in old_functions.items():
        expected_new_name = old_to_new_name(old_name)
        
        if expected_new_name in new_functions:
            new_func = new_functions[expected_new_name]
            differences = compare_function_bodies(old_func, new_func)
            if differences:
                with_differences.append((old_name, expected_new_name, differences))
            else:
                matched.append((old_name, expected_new_name))
        else:
            # Check if it exists with a different name transformation
            found = False
            for new_name in new_functions:
                if new_name.split('.')[-1] == old_name.split('.')[-1]:
                    # Same function name, different schema
                    new_func = new_functions[new_name]
                    differences = compare_function_bodies(old_func, new_func)
                    if differences:
                        with_differences.append((old_name, new_name, differences))
                    else:
                        matched.append((old_name, new_name))
                    found = True
                    break
            if not found:
                missing_in_new.append(old_name)
    
    # Find extra functions in new schema
    old_normalized = {old_to_new_name(n) for n in old_functions}
    for new_name in new_functions:
        if new_name not in old_normalized:
            # Check by function name alone
            new_fname = new_name.split('.')[-1]
            found = False
            for old_name in old_functions:
                old_fname = old_name.split('.')[-1]
                if old_fname == new_fname:
                    found = True
                    break
            if not found:
                extra_in_new.append(new_name)
    
    # Report results
    print("\n" + "=" * 70)
    print("RESULTS")
    print("=" * 70)
    
    print(f"\n✓ MATCHED FUNCTIONS (identical after normalization): {len(matched)}")
    if matched:
        for old, new in sorted(matched)[:20]:
            print(f"  {old} -> {new}")
        if len(matched) > 20:
            print(f"  ... and {len(matched) - 20} more")
    
    if with_differences:
        print(f"\n⚠ FUNCTIONS WITH DIFFERENCES: {len(with_differences)}")
        for old, new, diffs in with_differences:
            print(f"\n  {old} -> {new}:")
            for diff in diffs:
                print(diff)
    
    if missing_in_new:
        print(f"\n✗ MISSING IN NEW SCHEMA: {len(missing_in_new)}")
        for name in sorted(missing_in_new):
            print(f"  {name}")
    
    if extra_in_new:
        print(f"\n+ EXTRA IN NEW SCHEMA: {len(extra_in_new)}")
        for name in sorted(extra_in_new):
            # Check if this is expected (like uuid_generate_v7 wrapper)
            if name in ('public.uuid_generate_v7', 'tasker.uuid_generate_v7'):
                print(f"  {name} (EXPECTED - UUIDv7 compatibility wrapper)")
            elif name.startswith('tasker.pgmq_'):
                print(f"  {name} (EXPECTED - moved to tasker schema)")
            else:
                print(f"  {name}")
    
    # Summary
    print("\n" + "=" * 70)
    print("SUMMARY")
    print("=" * 70)
    total_issues = len(with_differences) + len(missing_in_new)
    if total_issues == 0:
        print("✓ All functions appear correctly translated!")
    else:
        print(f"⚠ {total_issues} potential issues found - review above")
    
    return 0 if total_issues == 0 else 1


if __name__ == "__main__":
    main()

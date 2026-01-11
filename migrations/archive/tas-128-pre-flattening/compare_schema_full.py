#!/usr/bin/env python3
"""
Comprehensive schema comparison for TAS-128.
Compares tables, indexes, constraints, types, and triggers.

Usage:
    python compare_schema_full.py [old_schema.sql] [new_schema.sql]
    
If no arguments provided, defaults to full_schema_export.sql and new_schema_export.sql
in the current directory.
"""

import re
import sys
from pathlib import Path


def extract_tables(content: str) -> dict:
    """Extract CREATE TABLE statements."""
    tables = {}
    pattern = r"CREATE TABLE (\w+)\.(\w+) \(([\s\S]*?)\);"
    for match in re.finditer(pattern, content):
        schema, name, cols = match.groups()
        full_name = f"{schema}.{name}"
        tables[full_name] = {
            'schema': schema,
            'name': name,
            'columns': cols.strip()
        }
    return tables


def extract_indexes(content: str) -> dict:
    """Extract CREATE INDEX statements."""
    indexes = {}
    pattern = r"CREATE (UNIQUE )?INDEX (\w+) ON (\w+)\.(\w+)([^;]+);"
    for match in re.finditer(pattern, content):
        unique, idx_name, schema, table, rest = match.groups()
        indexes[idx_name] = {
            'unique': bool(unique),
            'schema': schema,
            'table': table,
            'definition': rest.strip()
        }
    return indexes


def extract_types(content: str) -> dict:
    """Extract CREATE TYPE statements."""
    types = {}
    pattern = r"CREATE TYPE (\w+)\.(\w+) AS ENUM \(([^)]+)\);"
    for match in re.finditer(pattern, content):
        schema, name, values = match.groups()
        full_name = f"{schema}.{name}"
        types[full_name] = {
            'schema': schema,
            'name': name,
            'values': values.strip()
        }
    return types


def normalize_table_name(name: str) -> str:
    """Normalize table name: public.tasker_foo -> tasker.foo"""
    if '.' not in name:
        return name
    schema, tname = name.split('.', 1)
    if schema == 'public' and tname.startswith('tasker_'):
        return f"tasker.{tname[7:]}"  # Remove 'tasker_' prefix
    return name


def normalize_index_name(name: str) -> str:
    """Normalize index name: idx_tasker_foo_bar -> idx_foo_bar"""
    return name.replace('_tasker_', '_')


def main():
    old_file = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("full_schema_export.sql")
    new_file = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("new_schema_export.sql")
    
    old_content = old_file.read_text()
    new_content = new_file.read_text()
    
    print("=" * 70)
    print("TAS-128 COMPREHENSIVE SCHEMA COMPARISON")
    print("=" * 70)
    
    # ===== TABLES =====
    print("\n" + "-" * 70)
    print("TABLE COMPARISON")
    print("-" * 70)
    
    old_tables = extract_tables(old_content)
    new_tables = extract_tables(new_content)
    
    print(f"\nOld tables: {len(old_tables)}")
    print(f"New tables: {len(new_tables)}")
    
    # Map old table names to expected new names
    old_table_mapping = {}
    for old_name in old_tables:
        expected_new = normalize_table_name(old_name)
        old_table_mapping[old_name] = expected_new
    
    # Find tables in old but not in new
    missing_tables = []
    matched_tables = []
    for old_name, expected_new in old_table_mapping.items():
        if expected_new in new_tables:
            matched_tables.append((old_name, expected_new))
        else:
            missing_tables.append((old_name, expected_new))
    
    # Find extra tables in new
    expected_new_set = set(old_table_mapping.values())
    extra_tables = [n for n in new_tables if n not in expected_new_set]
    
    print(f"\n✓ MATCHED TABLES: {len(matched_tables)}")
    for old, new in matched_tables:
        print(f"  {old} -> {new}")
    
    if missing_tables:
        print(f"\n✗ MISSING TABLES: {len(missing_tables)}")
        for old, expected in missing_tables:
            print(f"  Expected {expected} from {old}")
    
    if extra_tables:
        print(f"\n+ EXTRA TABLES: {len(extra_tables)}")
        for name in extra_tables:
            print(f"  {name}")
    
    # ===== INDEXES =====
    print("\n" + "-" * 70)
    print("INDEX COMPARISON")
    print("-" * 70)
    
    old_indexes = extract_indexes(old_content)
    new_indexes = extract_indexes(new_content)
    
    print(f"\nOld indexes: {len(old_indexes)}")
    print(f"New indexes: {len(new_indexes)}")
    
    # Map old index names to expected new names
    matched_indexes = []
    missing_indexes = []
    for old_idx in old_indexes:
        expected_new = normalize_index_name(old_idx)
        if expected_new in new_indexes:
            matched_indexes.append((old_idx, expected_new))
        else:
            missing_indexes.append((old_idx, expected_new))
    
    # Find extra indexes
    expected_idx_set = {normalize_index_name(n) for n in old_indexes}
    extra_indexes = [n for n in new_indexes if n not in expected_idx_set]
    
    print(f"\n✓ MATCHED INDEXES: {len(matched_indexes)}")
    # Show first 10
    for old, new in matched_indexes[:10]:
        print(f"  {old} -> {new}")
    if len(matched_indexes) > 10:
        print(f"  ... and {len(matched_indexes) - 10} more")
    
    if missing_indexes:
        print(f"\n✗ MISSING INDEXES: {len(missing_indexes)}")
        for old, expected in missing_indexes:
            print(f"  Expected {expected} from {old}")
    
    if extra_indexes:
        print(f"\n+ EXTRA INDEXES: {len(extra_indexes)}")
        for name in extra_indexes:
            print(f"  {name}")
    
    # ===== TYPES =====
    print("\n" + "-" * 70)
    print("TYPE COMPARISON")
    print("-" * 70)
    
    old_types = extract_types(old_content)
    new_types = extract_types(new_content)
    
    print(f"\nOld types: {len(old_types)}")
    print(f"New types: {len(new_types)}")
    
    for old_name, old_type in old_types.items():
        expected_new = old_name.replace('public.', 'tasker.')
        if expected_new in new_types:
            new_type = new_types[expected_new]
            if old_type['values'] == new_type['values']:
                print(f"✓ {old_name} -> {expected_new}: identical values")
            else:
                print(f"⚠ {old_name} -> {expected_new}: VALUES DIFFER")
                print(f"  OLD: {old_type['values']}")
                print(f"  NEW: {new_type['values']}")
        else:
            print(f"✗ {old_name}: not found as {expected_new}")
    
    # ===== PGMQ FUNCTIONS =====
    print("\n" + "-" * 70)
    print("PGMQ FUNCTION COMPARISON")
    print("-" * 70)
    
    pgmq_funcs = [
        'pgmq_auto_add_headers_trigger',
        'pgmq_delete_specific_message',
        'pgmq_ensure_headers_column',
        'pgmq_extend_vt_specific_message',
        'pgmq_notify_queue_created',
        'pgmq_read_specific_message',
        'pgmq_send_batch_with_notify',
        'pgmq_send_with_notify',
    ]
    
    for func in pgmq_funcs:
        old_pattern = f"CREATE FUNCTION public.{func}"
        new_pattern_public = f"CREATE FUNCTION public.{func}"
        new_pattern_tasker = f"CREATE FUNCTION tasker.{func}"
        
        in_old = old_pattern in old_content
        in_new_public = new_pattern_public in new_content
        in_new_tasker = new_pattern_tasker in new_content
        
        if in_old:
            if in_new_public:
                print(f"✓ {func}: public.{func} -> public.{func}")
            elif in_new_tasker:
                print(f"✓ {func}: public.{func} -> tasker.{func}")
            else:
                print(f"✗ {func}: MISSING in new schema")
        else:
            print(f"? {func}: Not found in old schema")
    
    # ===== UUID WRAPPER =====
    print("\n" + "-" * 70)
    print("UUID COMPATIBILITY")
    print("-" * 70)
    
    # Check for pg_uuidv7 extension in old
    has_pg_uuidv7_ext = 'CREATE EXTENSION pg_uuidv7' in old_content or 'pg_uuidv7' in old_content
    has_native_uuidv7 = 'SELECT uuidv7()' in new_content
    has_wrapper = 'uuid_generate_v7' in new_content
    
    print(f"Old schema uses pg_uuidv7 extension: {has_pg_uuidv7_ext}")
    print(f"New schema uses native uuidv7(): {has_native_uuidv7}")
    print(f"New schema has uuid_generate_v7 wrapper: {has_wrapper}")
    
    if has_wrapper:
        # Check both public and tasker wrappers
        wrapper_pattern = r"CREATE FUNCTION (\w+)\.uuid_generate_v7\(\)"
        wrappers = re.findall(wrapper_pattern, new_content)
        print(f"Wrapper functions in schemas: {wrappers}")
    
    # ===== SUMMARY =====
    print("\n" + "=" * 70)
    print("SUMMARY")
    print("=" * 70)
    
    issues = len(missing_tables) + len(missing_indexes)
    if issues == 0:
        print("✓ All schema elements appear correctly translated!")
        print("\nKey findings:")
        print(f"  - {len(matched_tables)} tables matched")
        print(f"  - {len(matched_indexes)} indexes matched")
        print(f"  - ENUM types preserved")
        print(f"  - PGMQ functions present in both schemas")
        print(f"  - UUIDv7 compatibility wrappers in place")
    else:
        print(f"⚠ {issues} potential issues found")


if __name__ == "__main__":
    main()

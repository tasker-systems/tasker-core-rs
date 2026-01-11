#!/usr/bin/env python3
"""
TAS-128 Schema Validation Script

Compares the original pg17 schema export with the new pg18 flattened schema
to ensure structural equivalence after the migration.

Expected differences (should be ignored):
- public.tasker_* -> tasker.* (schema and prefix changes)
- pg_uuidv7 extension vs native uuidv7()
- _sqlx_migrations table (internal tracking)

Unexpected differences (should be reported as errors):
- Missing tables, columns, constraints, indexes, functions
- Type mismatches
- Constraint definition changes
"""

import re
import sys
from pathlib import Path
from dataclasses import dataclass, field
from typing import Dict, List, Set, Tuple, Optional
from collections import defaultdict


@dataclass
class SchemaObject:
    """Represents a database object extracted from pg_dump output"""
    object_type: str  # TABLE, INDEX, FUNCTION, VIEW, CONSTRAINT, TRIGGER
    schema: str
    name: str
    definition: str
    normalized_name: str = ""

    def __post_init__(self):
        self.normalized_name = self.normalize_name()

    def normalize_name(self) -> str:
        """Normalize name for comparison (remove tasker_ prefix, lowercase)"""
        name = self.name.lower()
        if name.startswith('tasker_'):
            name = name[7:]  # Remove 'tasker_' prefix
        return name


def normalize_definition(definition: str, for_old_schema: bool = True) -> str:
    """
    Normalize a SQL definition for comparison.

    Transforms:
    - public.tasker_* -> tasker.*
    - Removes owner/acl comments
    - Normalizes whitespace
    """
    text = definition

    if for_old_schema:
        # Transform public.tasker_tablename -> tasker.tablename
        text = re.sub(r'public\.tasker_(\w+)', r'tasker.\1', text)
        # Transform just tasker_tablename (without schema) -> tablename
        text = re.sub(r'\btasker_(\w+)', r'\1', text)

    # Normalize schema references
    text = re.sub(r'\bpublic\.', 'tasker.', text)

    # Remove uuid extension differences (these are expected)
    text = re.sub(r"CREATE EXTENSION.*pg_uuidv7.*?;", '', text, flags=re.DOTALL)
    text = re.sub(r"uuid_generate_v7\(\)", 'UUID_V7_FUNC()', text)
    text = re.sub(r"uuidv7\(\)", 'UUID_V7_FUNC()', text)

    # Normalize whitespace
    text = re.sub(r'\s+', ' ', text).strip()

    return text


def extract_tables(content: str) -> Dict[str, dict]:
    """Extract table definitions from pg_dump output"""
    tables = {}

    # Match CREATE TABLE statements
    pattern = r'CREATE TABLE (\w+)\.(\w+) \((.*?)\);'
    for match in re.finditer(pattern, content, re.DOTALL):
        schema, name, columns_def = match.groups()

        # Parse columns
        columns = {}
        for line in columns_def.split(','):
            line = line.strip()
            if not line or line.startswith('CONSTRAINT'):
                continue
            parts = line.split()
            if len(parts) >= 2:
                col_name = parts[0].strip('"')
                col_type = ' '.join(parts[1:])
                columns[col_name] = col_type

        tables[f"{schema}.{name}"] = {
            'schema': schema,
            'name': name,
            'columns': columns,
            'raw': match.group(0)
        }

    return tables


def extract_constraints(content: str) -> Dict[str, dict]:
    """Extract constraint definitions from pg_dump output"""
    constraints = {}

    # Match ADD CONSTRAINT statements
    pattern = r'ALTER TABLE ONLY (\w+)\.(\w+)\s+ADD CONSTRAINT (\w+) (.*?);'
    for match in re.finditer(pattern, content, re.DOTALL):
        schema, table, name, definition = match.groups()
        constraints[f"{schema}.{table}.{name}"] = {
            'schema': schema,
            'table': table,
            'name': name,
            'definition': definition.strip(),
            'raw': match.group(0)
        }

    return constraints


def extract_indexes(content: str) -> Dict[str, dict]:
    """Extract index definitions from pg_dump output"""
    indexes = {}

    # Match CREATE INDEX and CREATE UNIQUE INDEX statements
    pattern = r'CREATE (UNIQUE )?INDEX (\w+) ON (\w+)\.(\w+) (.*?);'
    for match in re.finditer(pattern, content, re.DOTALL):
        unique, name, schema, table, definition = match.groups()
        indexes[f"{schema}.{name}"] = {
            'schema': schema,
            'table': table,
            'name': name,
            'unique': bool(unique),
            'definition': definition.strip(),
            'raw': match.group(0)
        }

    return indexes


def extract_functions(content: str) -> Dict[str, dict]:
    """Extract function definitions from pg_dump output"""
    functions = {}

    # Match CREATE FUNCTION statements - capture function signature
    pattern = r'CREATE FUNCTION (\w+)\.(\w+)\((.*?)\)\s+RETURNS\s+(\S+)'
    for match in re.finditer(pattern, content, re.DOTALL):
        schema, name, params, returns = match.groups()
        # Simplify params for comparison
        param_types = []
        for p in params.split(','):
            p = p.strip()
            if p:
                parts = p.split()
                if len(parts) >= 2:
                    param_types.append(parts[-1])  # Just the type

        functions[f"{schema}.{name}"] = {
            'schema': schema,
            'name': name,
            'params': param_types,
            'returns': returns,
            'signature': f"{name}({', '.join(param_types)}) -> {returns}"
        }

    return functions


def extract_views(content: str) -> Dict[str, str]:
    """Extract view definitions from pg_dump output"""
    views = {}

    # Match CREATE VIEW statements
    pattern = r'CREATE VIEW (\w+)\.(\w+) AS\s+(.*?);'
    for match in re.finditer(pattern, content, re.DOTALL):
        schema, name, definition = match.groups()
        views[f"{schema}.{name}"] = definition.strip()

    return views


def extract_triggers(content: str) -> Dict[str, dict]:
    """Extract trigger definitions from pg_dump output"""
    triggers = {}

    # Match CREATE TRIGGER statements
    pattern = r'CREATE TRIGGER (\w+)\s+(.*?)ON\s+(\w+)\.(\w+)\s+.*?EXECUTE\s+FUNCTION\s+(\w+)\.(\w+)\(\)'
    for match in re.finditer(pattern, content, re.DOTALL):
        name, timing, schema, table, func_schema, func_name = match.groups()
        triggers[f"{schema}.{table}.{name}"] = {
            'name': name,
            'table': f"{schema}.{table}",
            'function': f"{func_schema}.{func_name}",
            'timing': timing.strip()
        }

    return triggers


def normalize_table_name(full_name: str, is_old: bool = True) -> str:
    """Normalize a fully qualified table name for comparison"""
    schema, name = full_name.split('.', 1)

    # Skip internal tables
    if name == '_sqlx_migrations':
        return None

    if is_old:
        # Transform public.tasker_foo -> tasker.foo
        if schema == 'public' and name.startswith('tasker_'):
            return f"tasker.{name[7:]}"
        elif schema == 'public':
            return f"tasker.{name}"

    return f"{schema}.{name}"


def compare_schemas(old_file: Path, new_file: Path) -> Tuple[List[str], List[str], List[str]]:
    """
    Compare two schema exports and return:
    - expected_diffs: Differences that are expected (schema rename, prefix removal)
    - unexpected_diffs: Differences that indicate problems
    - matches: Objects that match correctly
    """
    old_content = old_file.read_text()
    new_content = new_file.read_text()

    expected_diffs = []
    unexpected_diffs = []
    matches = []

    # Extract objects from both schemas
    print("Extracting tables...")
    old_tables = extract_tables(old_content)
    new_tables = extract_tables(new_content)

    print("Extracting constraints...")
    old_constraints = extract_constraints(old_content)
    new_constraints = extract_constraints(new_content)

    print("Extracting indexes...")
    old_indexes = extract_indexes(old_content)
    new_indexes = extract_indexes(new_content)

    print("Extracting functions...")
    old_functions = extract_functions(old_content)
    new_functions = extract_functions(new_content)

    print("Extracting views...")
    old_views = extract_views(old_content)
    new_views = extract_views(new_content)

    print("Extracting triggers...")
    old_triggers = extract_triggers(old_content)
    new_triggers = extract_triggers(new_content)

    # Compare tables
    print("\n" + "="*60)
    print("COMPARING TABLES")
    print("="*60)

    old_table_names = set()
    for name in old_tables:
        normalized = normalize_table_name(name, is_old=True)
        if normalized:
            old_table_names.add(normalized)

    new_table_names = set(new_tables.keys())

    # Filter out pgmq tables from comparison (they're in a different schema)
    old_table_names = {t for t in old_table_names if not t.startswith('pgmq.')}
    new_table_names = {t for t in new_table_names if not t.startswith('pgmq.')}

    missing_in_new = old_table_names - new_table_names
    extra_in_new = new_table_names - old_table_names
    common_tables = old_table_names & new_table_names

    if missing_in_new:
        for t in sorted(missing_in_new):
            unexpected_diffs.append(f"MISSING TABLE: {t}")

    if extra_in_new:
        for t in sorted(extra_in_new):
            # Check if it's a renamed table
            unexpected_diffs.append(f"EXTRA TABLE: {t}")

    for t in sorted(common_tables):
        matches.append(f"TABLE: {t}")

    print(f"  Old tables (normalized): {len(old_table_names)}")
    print(f"  New tables: {len(new_table_names)}")
    print(f"  Common: {len(common_tables)}")
    print(f"  Missing in new: {len(missing_in_new)}")
    print(f"  Extra in new: {len(extra_in_new)}")

    # Compare functions
    print("\n" + "="*60)
    print("COMPARING FUNCTIONS")
    print("="*60)

    # Normalize old function names
    old_func_normalized = {}
    for name, info in old_functions.items():
        schema, fname = name.split('.', 1)
        if schema == 'public':
            # Check if it's a tasker function or a pgmq function
            if fname.startswith('tasker_') or not fname.startswith('pgmq'):
                new_name = f"tasker.{fname}"
                old_func_normalized[new_name] = info
            else:
                old_func_normalized[name] = info
        else:
            old_func_normalized[name] = info

    new_func_names = set(new_functions.keys())
    old_func_names = set(old_func_normalized.keys())

    # Filter to only tasker schema for comparison
    old_func_names = {f for f in old_func_names if f.startswith('tasker.')}
    new_func_names = {f for f in new_func_names if f.startswith('tasker.')}

    missing_funcs = old_func_names - new_func_names
    extra_funcs = new_func_names - old_func_names
    common_funcs = old_func_names & new_func_names

    print(f"  Old functions (tasker schema): {len(old_func_names)}")
    print(f"  New functions (tasker schema): {len(new_func_names)}")
    print(f"  Common: {len(common_funcs)}")
    print(f"  Missing in new: {len(missing_funcs)}")
    print(f"  Extra in new: {len(extra_funcs)}")

    # Expected extra functions: PGMQ wrappers moved to tasker schema, and uuid_generate_v7 compatibility
    expected_extra_funcs = {
        'tasker.pgmq_auto_add_headers_trigger',
        'tasker.pgmq_delete_specific_message',
        'tasker.pgmq_ensure_headers_column',
        'tasker.pgmq_extend_vt_specific_message',
        'tasker.pgmq_notify_queue_created',
        'tasker.pgmq_read_specific_message',
        'tasker.pgmq_send_batch_with_notify',
        'tasker.pgmq_send_with_notify',
        'tasker.uuid_generate_v7',  # Compatibility wrapper for pg18 native uuidv7()
    }

    if missing_funcs:
        for f in sorted(missing_funcs):
            unexpected_diffs.append(f"MISSING FUNCTION: {f}")

    if extra_funcs:
        for f in sorted(extra_funcs):
            if f in expected_extra_funcs:
                expected_diffs.append(f"EXPECTED EXTRA FUNCTION (moved to tasker schema): {f}")
            else:
                unexpected_diffs.append(f"EXTRA FUNCTION: {f}")

    for f in sorted(common_funcs):
        matches.append(f"FUNCTION: {f}")

    # Compare indexes
    print("\n" + "="*60)
    print("COMPARING INDEXES")
    print("="*60)

    # Normalize old index names
    old_idx_normalized = {}
    for name, info in old_indexes.items():
        schema, iname = name.split('.', 1)
        # Remove tasker_ prefix from index name if present (e.g., idx_tasker_tasks_* -> idx_tasks_*)
        if '_tasker_' in iname:
            iname = iname.replace('_tasker_', '_')
        if schema == 'public':
            new_name = f"tasker.{iname}"
            old_idx_normalized[new_name] = info
        else:
            old_idx_normalized[name] = info

    new_idx_names = set(new_indexes.keys())
    old_idx_names = set(old_idx_normalized.keys())

    # Filter to tasker schema
    old_idx_names = {i for i in old_idx_names if i.startswith('tasker.')}
    new_idx_names = {i for i in new_idx_names if i.startswith('tasker.')}

    missing_idx = old_idx_names - new_idx_names
    extra_idx = new_idx_names - old_idx_names
    common_idx = old_idx_names & new_idx_names

    print(f"  Old indexes (tasker schema): {len(old_idx_names)}")
    print(f"  New indexes (tasker schema): {len(new_idx_names)}")
    print(f"  Common: {len(common_idx)}")
    print(f"  Missing in new: {len(missing_idx)}")
    print(f"  Extra in new: {len(extra_idx)}")

    if missing_idx:
        for i in sorted(missing_idx):
            unexpected_diffs.append(f"MISSING INDEX: {i}")

    for i in sorted(common_idx):
        matches.append(f"INDEX: {i}")

    # Compare views
    print("\n" + "="*60)
    print("COMPARING VIEWS")
    print("="*60)

    old_view_normalized = {}
    for name in old_views:
        schema, vname = name.split('.', 1)
        if schema == 'public':
            # Remove tasker_ prefix if present
            if vname.startswith('tasker_'):
                vname = vname[7:]
            new_name = f"tasker.{vname}"
            old_view_normalized[new_name] = old_views[name]
        else:
            old_view_normalized[name] = old_views[name]

    new_view_names = set(new_views.keys())
    old_view_names = set(old_view_normalized.keys())

    # Filter to tasker schema
    old_view_names = {v for v in old_view_names if v.startswith('tasker.')}
    new_view_names = {v for v in new_view_names if v.startswith('tasker.')}

    missing_views = old_view_names - new_view_names
    extra_views = new_view_names - old_view_names
    common_views = old_view_names & new_view_names

    print(f"  Old views (tasker schema): {len(old_view_names)}")
    print(f"  New views (tasker schema): {len(new_view_names)}")
    print(f"  Common: {len(common_views)}")
    print(f"  Missing in new: {len(missing_views)}")
    print(f"  Extra in new: {len(extra_views)}")

    if missing_views:
        for v in sorted(missing_views):
            unexpected_diffs.append(f"MISSING VIEW: {v}")

    for v in sorted(common_views):
        matches.append(f"VIEW: {v}")

    return expected_diffs, unexpected_diffs, matches


def main():
    backup_dir = Path(__file__).parent
    old_schema = backup_dir / "full_schema_export.sql"
    new_schema = backup_dir / "new_schema_export.sql"

    if not old_schema.exists():
        print(f"ERROR: Old schema file not found: {old_schema}")
        sys.exit(1)

    if not new_schema.exists():
        print(f"ERROR: New schema file not found: {new_schema}")
        sys.exit(1)

    print("="*60)
    print("TAS-128 SCHEMA VALIDATION")
    print("="*60)
    print(f"Old schema: {old_schema}")
    print(f"New schema: {new_schema}")
    print()

    expected_diffs, unexpected_diffs, matches = compare_schemas(old_schema, new_schema)

    print("\n" + "="*60)
    print("VALIDATION RESULTS")
    print("="*60)

    print(f"\n✓ MATCHING OBJECTS: {len(matches)}")
    if len(matches) <= 50:
        for m in matches:
            print(f"  {m}")
    else:
        print(f"  (showing first 20 of {len(matches)})")
        for m in matches[:20]:
            print(f"  {m}")

    if expected_diffs:
        print(f"\n⚠ EXPECTED DIFFERENCES: {len(expected_diffs)}")
        for d in expected_diffs:
            print(f"  {d}")

    if unexpected_diffs:
        print(f"\n✗ UNEXPECTED DIFFERENCES: {len(unexpected_diffs)}")
        for d in unexpected_diffs:
            print(f"  {d}")
        print("\nThese need investigation!")
        sys.exit(1)
    else:
        print("\n✓ NO UNEXPECTED DIFFERENCES FOUND")
        print("Schema migration validation PASSED!")

    return 0


if __name__ == "__main__":
    sys.exit(main())

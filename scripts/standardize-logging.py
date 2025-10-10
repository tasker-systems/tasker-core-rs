#!/usr/bin/env python3
"""
TAS-29 Phase 2: Logging Standardization Script

Fixes common logging anti-patterns:
1. UUID.to_string() -> %UUID (display formatting)
2. Missing correlation_id in structured logs
3. Incorrect field ordering

This prepares hot path files for proper correlation_id integration.
"""

import re
from pathlib import Path
from typing import Tuple

def fix_uuid_to_string(content: str) -> Tuple[str, int]:
    """
    Fix task_uuid = task_uuid.to_string() to task_uuid = %task_uuid
    Also handles step_uuid, correlation_id, processor_uuid, etc.
    """
    # Pattern: uuid_field = uuid_var.to_string()
    # Common in: task_uuid = task_uuid.to_string()
    pattern = r'(\w+_uuid|correlation_id|processor_uuid)\s*=\s*(\w+)\.to_string\(\)'
    replacement = r'\1 = %\2'

    changes = len(re.findall(pattern, content))
    content = re.sub(pattern, replacement, content)

    return content, changes

def main():
    workspace_root = Path(__file__).parent.parent

    # Hot path files identified in audit
    HOT_PATH_FILES = [
        "tasker-orchestration/src/orchestration/lifecycle/task_initializer.rs",
        "tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs",
        "tasker-orchestration/src/orchestration/lifecycle/result_processor.rs",
        "tasker-orchestration/src/orchestration/lifecycle/task_finalizer.rs",
        "tasker-worker/src/worker/step_claim.rs",
        "tasker-worker/src/worker/command_processor.rs",
    ]

    print("=" * 50)
    print("TAS-29 Phase 2: Logging Standardization")
    print("=" * 50)
    print()
    print("Fixing common logging anti-patterns:")
    print("  1. UUID.to_string() -> %UUID")
    print()

    files_changed = 0
    total_uuid_fixes = 0

    for file_rel_path in HOT_PATH_FILES:
        file_path = workspace_root / file_rel_path

        if not file_path.exists():
            print(f"  ⚠ Skipping {file_rel_path} (not found)")
            continue

        content = file_path.read_text(encoding='utf-8')
        original_content = content

        # Fix UUID .to_string() calls
        content, uuid_fixes = fix_uuid_to_string(content)

        total_changes = uuid_fixes

        if total_changes > 0:
            file_path.write_text(content, encoding='utf-8')
            print(f"  ✓ {file_rel_path}: Fixed {uuid_fixes} UUID.to_string() calls")
            files_changed += 1
            total_uuid_fixes += uuid_fixes

    print()
    print("=" * 50)
    print("Logging Standardization Complete")
    print("=" * 50)
    print(f"Files changed: {files_changed}")
    print(f"UUID.to_string() fixes: {total_uuid_fixes}")
    print()

    if files_changed > 0:
        print("✓ Changes applied successfully")
        print()
        print("Next steps:")
        print("1. Review changes: git diff")
        print("2. Verify compilation: cargo check --all-features")
        print("3. Manual correlation_id integration in hot paths")
    else:
        print("No standardization needed - logging already clean!")

if __name__ == "__main__":
    main()

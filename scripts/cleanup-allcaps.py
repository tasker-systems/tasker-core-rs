#!/usr/bin/env python3
"""
TAS-29 Phase 1.5: Automated All-Caps Prefix Removal Script
Removes all-caps component prefixes from tracing log messages.

These prefixes are redundant with module paths in structured logging.
"""

import re
import sys
from pathlib import Path

# All-caps prefix patterns found in codebase
# These are redundant with module paths in structured logging
ALL_CAPS_PREFIXES = [
    "BOOTSTRAP:",
    "STEP_ENQUEUER:",
    "ORCHESTRATION_LOOP:",
    "WORKER_EVENT_SYSTEM:",
    "CORE:",
    "ORCHESTRATION:",
    "WORKER:",
    "FINALIZER:",
    "RESULT_PROCESSOR:",
    "TASK_INITIALIZER:",
    "COMMAND_PROCESSOR:",
    "EVENT_SYSTEM:",
    "QUEUE_WORKER:",
    "ORCHESTRATOR:",
    "STEP_OPERATION:",
    "TASK_OPERATION:",
    "DATABASE:",
    "FFI:",
    "CONFIG:",
    "⚙CONFIG:",  # Also remove emoji variants that might remain
    "REGISTRY:",
    "TRACING:",
]

def remove_allcaps_prefixes_from_file(file_path: Path) -> int:
    """
    Remove all-caps prefixes from a file. Returns number of prefixes removed.
    """
    content = file_path.read_text(encoding='utf-8')
    original_content = content

    removed_count = 0

    # For each prefix, remove it along with any following space
    for prefix in ALL_CAPS_PREFIXES:
        # Pattern: prefix followed by optional space
        # Must be within a log message (in quotes)
        pattern = re.escape(prefix) + r' '

        # Count occurrences before removal
        before = content.count(prefix + ' ')

        # Remove the prefix and trailing space
        content = content.replace(prefix + ' ', '')

        removed_count += before

    # Only write if changes were made
    if content != original_content:
        file_path.write_text(content, encoding='utf-8')
        return removed_count

    return 0

def main():
    workspace_root = Path(__file__).parent.parent

    # Target directories
    CRATES = [
        "tasker-orchestration/src",
        "tasker-worker/src",
        "tasker-shared/src",
        "tasker-client/src",
        "pgmq-notify/src",
    ]

    print("=" * 50)
    print("TAS-29: All-Caps Prefix Removal Script")
    print("=" * 50)
    print()

    files_changed = 0
    prefixes_removed = 0

    for crate in CRATES:
        crate_path = workspace_root / crate
        if not crate_path.exists():
            continue

        print(f"Processing {crate}...")

        # Find all Rust files
        for rust_file in crate_path.rglob("*.rs"):
            removed = remove_allcaps_prefixes_from_file(rust_file)
            if removed > 0:
                rel_path = rust_file.relative_to(workspace_root)
                print(f"  ✓ {rel_path}: Removed {removed} all-caps prefixes")
                files_changed += 1
                prefixes_removed += removed

    print()
    print("=" * 50)
    print("All-Caps Prefix Removal Complete")
    print("=" * 50)
    print(f"Files changed: {files_changed}")
    print(f"Prefixes removed: {prefixes_removed}")
    print()

    if files_changed > 0:
        print("✓ Changes applied successfully")
        print()
        print("Next steps:")
        print("1. Review changes: git diff")
        print("2. Run tests: cargo test --all-features")
        print("3. Continue with TAS-XX reference cleanup")
    else:
        print("No all-caps prefixes found - codebase already clean!")

if __name__ == "__main__":
    main()

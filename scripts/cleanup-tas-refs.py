#!/usr/bin/env python3
"""
TAS-29 Phase 1.5: TAS-XX Ticket Reference Cleanup Script

Removes internal ticket references from:
- Runtime log messages (info!, debug!, warn!, error!)
- Standalone inline comments that are just ticket references

Preserves architectural documentation in module-level doc comments.
"""

import re
import sys
from pathlib import Path
from typing import Tuple, List

# Pattern to match TAS-XX references
TAS_PATTERN = r'\(TAS-\d+\)'

# Pattern to match standalone comment lines that are just TAS references
# Example: "// TAS-40: Command pattern migration"
STANDALONE_COMMENT_PATTERN = r'^\s*//\s*TAS-\d+:.*$'

def clean_tas_refs_from_logs(content: str) -> Tuple[str, int]:
    """
    Remove (TAS-XX) references from log messages.
    Returns cleaned content and count of removals.
    """
    # Pattern: (TAS-XX) followed by optional space at end of string/line
    # Common in log messages: "Step completed successfully (TAS-40)"

    original = content
    # Remove (TAS-XX) followed by optional comma/space before closing quote
    content = re.sub(r'\s*\(TAS-\d+\)\s*([,\)])', r'\1', content)
    # Remove (TAS-XX) followed by space or quote
    content = re.sub(r'\s*\(TAS-\d+\)(\s|")', r'\1', content)

    changes = len(re.findall(TAS_PATTERN, original))
    return content, changes

def clean_standalone_tas_comments(content: str) -> Tuple[str, int]:
    """
    Remove standalone comment lines that are just TAS-XX references.
    Returns cleaned content and count of removals.

    Examples removed:
        // TAS-40: Command pattern migration
        // TAS-41: Atomic state transitions

    Examples preserved:
        //! # TAS-40 Architecture  (doc comment with context)
        // Initialize processor (TAS-40)  (inline comment with code context)
    """
    lines = content.split('\n')
    cleaned_lines = []
    removed_count = 0

    i = 0
    while i < len(lines):
        line = lines[i]

        # Check if this is a standalone TAS reference comment
        if re.match(STANDALONE_COMMENT_PATTERN, line):
            # Skip this line (remove it)
            removed_count += 1
            i += 1
            continue

        # Keep all other lines
        cleaned_lines.append(line)
        i += 1

    return '\n'.join(cleaned_lines), removed_count

def clean_file(file_path: Path) -> dict:
    """
    Clean TAS-XX references from a file.
    Returns dict with counts of different types of removals.
    """
    content = file_path.read_text(encoding='utf-8')
    original_content = content

    # Clean log message references
    content, log_refs_removed = clean_tas_refs_from_logs(content)

    # Clean standalone comments
    content, comment_lines_removed = clean_standalone_tas_comments(content)

    # Calculate total changes
    total_changes = log_refs_removed + comment_lines_removed

    if total_changes > 0:
        file_path.write_text(content, encoding='utf-8')

    return {
        'log_refs': log_refs_removed,
        'comment_lines': comment_lines_removed,
        'total': total_changes
    }

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
    print("TAS-29: TAS-XX Reference Cleanup Script")
    print("=" * 50)
    print()
    print("Removing internal ticket references from:")
    print("  - Runtime log messages")
    print("  - Standalone comment lines")
    print()
    print("Preserving:")
    print("  - Architectural doc comments")
    print("  - Inline code comments with context")
    print()

    files_changed = 0
    total_log_refs = 0
    total_comment_lines = 0

    for crate in CRATES:
        crate_path = workspace_root / crate
        if not crate_path.exists():
            continue

        print(f"Processing {crate}...")

        # Find all Rust files
        for rust_file in crate_path.rglob("*.rs"):
            result = clean_file(rust_file)

            if result['total'] > 0:
                rel_path = rust_file.relative_to(workspace_root)
                changes = []
                if result['log_refs'] > 0:
                    changes.append(f"{result['log_refs']} log refs")
                if result['comment_lines'] > 0:
                    changes.append(f"{result['comment_lines']} comment lines")

                print(f"  ✓ {rel_path}: Removed {', '.join(changes)}")
                files_changed += 1
                total_log_refs += result['log_refs']
                total_comment_lines += result['comment_lines']

    print()
    print("=" * 50)
    print("TAS-XX Reference Cleanup Complete")
    print("=" * 50)
    print(f"Files changed: {files_changed}")
    print(f"Log message references removed: {total_log_refs}")
    print(f"Standalone comment lines removed: {total_comment_lines}")
    print(f"Total changes: {total_log_refs + total_comment_lines}")
    print()

    if files_changed > 0:
        print("✓ Changes applied successfully")
        print()
        print("Next steps:")
        print("1. Review changes: git diff")
        print("2. Check for remaining TAS-XX in doc comments manually")
        print("3. Run tests: cargo check --all-features")
        print("4. Proceed with correlation_id integration in hot paths")
    else:
        print("No TAS-XX references found in target locations!")

if __name__ == "__main__":
    main()

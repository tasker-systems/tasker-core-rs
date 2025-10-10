#!/usr/bin/env python3
"""
TAS-29 Phase 1.5: Automated Emoji Removal Script
Removes all emojis from tracing log messages
"""

import re
import sys
from pathlib import Path

# Emoji pattern (common ones used in codebase)
EMOJI_PATTERN = r'[🔧✅🚀❌⚠️📊🔍🎉🛡️⏱️📝🏗️🎯🔄💡📦🧪🌉🔌⏳🛑]'

# Target directories
CRATES = [
    "tasker-orchestration/src",
    "tasker-worker/src",
    "tasker-shared/src",
    "tasker-client/src",
    "pgmq-notify/src",
]

def remove_emojis_from_file(file_path: Path) -> int:
    """
    Remove emojis from a file. Returns number of emojis removed.
    """
    content = file_path.read_text(encoding='utf-8')
    original_content = content

    # Count emojis before
    before_count = len(re.findall(EMOJI_PATTERN, content))

    if before_count == 0:
        return 0

    # Remove emojis with trailing space first, then any remaining emojis
    content = re.sub(EMOJI_PATTERN + r' ', '', content)
    content = re.sub(EMOJI_PATTERN, '', content)

    # Count after
    after_count = len(re.findall(EMOJI_PATTERN, content))

    if after_count == 0:
        # Success - write cleaned content
        file_path.write_text(content, encoding='utf-8')
        return before_count
    else:
        # Failed - content still has emojis (shouldn't happen)
        return 0

def main():
    workspace_root = Path(__file__).parent.parent

    print("=" * 50)
    print("TAS-29: Emoji Removal Script")
    print("=" * 50)
    print()

    files_changed = 0
    emojis_removed = 0

    for crate in CRATES:
        crate_path = workspace_root / crate
        if not crate_path.exists():
            continue

        print(f"Processing {crate}...")

        # Find all Rust files
        for rust_file in crate_path.rglob("*.rs"):
            removed = remove_emojis_from_file(rust_file)
            if removed > 0:
                rel_path = rust_file.relative_to(workspace_root)
                print(f"  ✓ {rel_path}: Removed {removed} emojis")
                files_changed += 1
                emojis_removed += removed

    print()
    print("=" * 50)
    print("Emoji Removal Complete")
    print("=" * 50)
    print(f"Files changed: {files_changed}")
    print(f"Emojis removed: {emojis_removed}")
    print()

    if files_changed > 0:
        print("✓ Changes applied successfully")
        print()
        print("Next steps:")
        print("1. Review changes: git diff")
        print("2. Run tests: cargo test --all-features")
        print("3. Commit changes: git add . && git commit")
    else:
        print("No emojis found - codebase already clean!")

if __name__ == "__main__":
    main()

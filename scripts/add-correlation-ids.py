#!/usr/bin/env python3
"""
TAS-29 Phase 2: Add correlation_id to Hot Path Logging

Systematically adds correlation_id as the first field in all structured logging
where it's missing in hot path files.

Prerequisites:
- correlation_id must be available in function scope
- Logs must use structured format: macro!(field = value, "message")
"""

import re
from pathlib import Path

def add_correlation_id_to_logging(content: str, file_path: Path) -> tuple[str, int]:
    """
    Add correlation_id to logging statements that don't have it.
    Returns (updated_content, count_of_changes)
    """
    changes = 0
    lines = content.split('\n')
    result_lines = []

    i = 0
    while i < len(lines):
        line = lines[i]

        # Check if this line starts a logging macro call without correlation_id
        log_match = re.match(r'(\s+)(info|debug|warn|error)!\(', line)

        if log_match and 'correlation_id' not in line:
            # This is a logging call that might need correlation_id
            # Collect the full macro call (may span multiple lines)
            full_call = [line]
            indent = log_match.group(1)
            paren_count = line.count('(') - line.count(')')

            j = i + 1
            while paren_count > 0 and j < len(lines):
                full_call.append(lines[j])
                paren_count += lines[j].count('(') - lines[j].count(')')
                j += 1

            full_text = '\n'.join(full_call)

            # Check if it's a structured log (has field = value pattern)
            if '=' in full_text and 'task_uuid' in full_text:
                # Extract the opening line
                opening = line

                # Insert correlation_id as first field
                # Pattern: macro!(\n or macro!(field
                if re.search(r'!\(\s*$', opening):
                    # Multiline - add on next line
                    result_lines.append(opening)
                    result_lines.append(f'{indent}    correlation_id = %correlation_id,')
                    changes += 1
                    i += 1
                    while i < j:
                        result_lines.append(lines[i])
                        i += 1
                    continue
                else:
                    # Inline - insert after the opening paren
                    new_line = re.sub(
                        r'(info|debug|warn|error)!\(',
                        r'\1!(\n' + indent + '    correlation_id = %correlation_id,',
                        opening
                    )
                    result_lines.append(new_line)
                    changes += 1
                    i += 1
                    while i < j:
                        result_lines.append(lines[i])
                        i += 1
                    continue

        result_lines.append(line)
        i += 1

    return '\n'.join(result_lines), changes


def main():
    workspace_root = Path(__file__).parent.parent

    # Hot path files that need correlation_id
    HOT_PATH_FILES = [
        "tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs",
        "tasker-orchestration/src/orchestration/lifecycle/result_processor.rs",
        "tasker-orchestration/src/orchestration/lifecycle/task_finalizer.rs",
        "tasker-worker/src/worker/step_claim.rs",
        "tasker-worker/src/worker/command_processor.rs",
    ]

    print("=" * 50)
    print("TAS-29: Adding correlation_id to Hot Paths")
    print("=" * 50)
    print()

    total_changes = 0
    files_modified = 0

    for file_rel_path in HOT_PATH_FILES:
        file_path = workspace_root / file_rel_path

        if not file_path.exists():
            print(f"  ⚠ Skipping {file_rel_path} (not found)")
            continue

        content = file_path.read_text(encoding='utf-8')
        updated_content, changes = add_correlation_id_to_logging(content, file_path)

        if changes > 0:
            file_path.write_text(updated_content, encoding='utf-8')
            print(f"  ✓ {file_rel_path}: Added correlation_id to {changes} logging statements")
            total_changes += changes
            files_modified += 1
        else:
            print(f"  - {file_rel_path}: No changes needed")

    print()
    print("=" * 50)
    print("correlation_id Integration Complete")
    print("=" * 50)
    print(f"Files modified: {files_modified}")
    print(f"Logging statements updated: {total_changes}")
    print()

    if files_modified > 0:
        print("✓ Changes applied")
        print()
        print("Next steps:")
        print("1. Review changes: git diff")
        print("2. Verify compilation: cargo check --all-features")
        print("3. Run tests: cargo test --all-features")
    else:
        print("All hot paths already have correlation_id!")

if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
TAS-50 Automatic Unused Config Field Commenting

Reads config-usage-analysis.md and automatically comments out UNUSED fields
in Rust struct definitions with TAS-50 markers for easy reversal if needed.
"""

import re
from pathlib import Path
from typing import List, Set, Tuple

class UnusedFieldCommenter:
    def __init__(self, project_root: Path):
        self.project_root = project_root
        self.analysis_file = project_root / "docs" / "config-usage-analysis.md"
        self.unused_params: Set[str] = set()

    def parse_analysis(self) -> None:
        """Extract UNUSED parameters from analysis markdown"""
        print("Parsing config-usage-analysis.md...")

        with open(self.analysis_file, 'r') as f:
            content = f.read()

        # Find all UNUSED parameters in the markdown table
        # Format: | `param.path.name` | Context | UNUSED | 0 | None |
        pattern = r'\| `([^`]+)` \| [^|]+ \| UNUSED \|'
        matches = re.findall(pattern, content)

        self.unused_params = set(matches)
        print(f"Found {len(self.unused_params)} UNUSED parameters")

        # Print them for verification
        for param in sorted(self.unused_params):
            print(f"  - {param}")

    def get_field_name(self, param_path: str) -> str:
        """Extract field name from parameter path"""
        # e.g., "database.pool.max_connections" -> "max_connections"
        return param_path.split('.')[-1]

    def find_struct_files(self) -> List[Path]:
        """Find all Rust files that might contain config structs"""
        config_dir = self.project_root / "tasker-shared" / "src" / "config"

        rust_files = []
        for path in config_dir.rglob("*.rs"):
            rust_files.append(path)

        print(f"\nFound {len(rust_files)} Rust config files to process")
        return rust_files

    def comment_field_in_struct(self, file_path: Path, field_name: str) -> bool:
        """Comment out a field in a struct definition with TAS-50 marker"""

        with open(file_path, 'r') as f:
            lines = f.readlines()

        modified = False
        new_lines = []
        i = 0

        while i < len(lines):
            line = lines[i]

            # Look for field definitions: pub field_name: Type,
            # Match patterns like:
            #   pub max_connections: u32,
            #   pub username: String,
            field_pattern = rf'^\s*pub\s+{re.escape(field_name)}\s*:\s*.+[,;]?\s*$'

            if re.match(field_pattern, line):
                # Check if already commented
                if line.strip().startswith('//'):
                    new_lines.append(line)
                    i += 1
                    continue

                # Look backwards for doc comments
                doc_start_idx = i - 1
                while doc_start_idx >= 0 and lines[doc_start_idx].strip().startswith('///'):
                    doc_start_idx -= 1
                doc_start_idx += 1

                # Comment out doc comments
                for j in range(doc_start_idx, i):
                    if not lines[j].strip().startswith('//'):
                        new_lines[j - (i - doc_start_idx)] = f"// TAS-50: UNUSED - {lines[j]}"
                        modified = True

                # Add marker comment before field
                indent = len(line) - len(line.lstrip())
                new_lines.append(' ' * indent + f"// TAS-50: UNUSED - {field_name}\n")

                # Check if there's a serde attribute on the line before
                if i > 0 and '#[serde' in lines[i-1]:
                    # Comment out the serde attribute too
                    attr_line = new_lines.pop()  # Remove the marker we just added
                    new_lines[-1] = f"// TAS-50: UNUSED - {lines[i-1]}"
                    new_lines.append(attr_line)  # Re-add marker

                # Comment out the field itself
                new_lines.append(f"// TAS-50: UNUSED - {line}")
                modified = True

                print(f"  ✓ Commented out field '{field_name}' in {file_path.name}")
                i += 1
                continue

            new_lines.append(line)
            i += 1

        if modified:
            with open(file_path, 'w') as f:
                f.writelines(new_lines)

        return modified

    def process_all_files(self) -> None:
        """Process all config files and comment out unused fields"""
        rust_files = self.find_struct_files()

        total_commented = 0

        for file_path in rust_files:
            file_modified = False

            # Extract field names from unused params
            unused_fields = set()
            for param in self.unused_params:
                field_name = self.get_field_name(param)
                unused_fields.add(field_name)

            # Try to comment out each unused field
            for field_name in unused_fields:
                if self.comment_field_in_struct(file_path, field_name):
                    file_modified = True
                    total_commented += 1

        print(f"\n✓ Commented out {total_commented} unused fields across {len(rust_files)} files")

    def run(self) -> None:
        """Run the complete commenting process"""
        print("=== TAS-50 Unused Config Field Commenter ===\n")

        self.parse_analysis()
        self.process_all_files()

        print("\n✓ Process complete!")
        print("\nNext steps:")
        print("  1. Review the commented changes: git diff")
        print("  2. Run tests: cargo test --all-features --workspace")
        print("  3. Fix any compilation errors")
        print("  4. To revert: grep -r 'TAS-50: UNUSED' and uncomment")

def main():
    project_root = Path(__file__).parent.parent
    commenter = UnusedFieldCommenter(project_root)
    commenter.run()

if __name__ == '__main__':
    main()

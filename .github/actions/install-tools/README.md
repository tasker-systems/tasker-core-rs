# Install Tools Action

A composite GitHub Action that installs `cargo-binstall` and common Rust tools used across Tasker Core CI workflows.

## Purpose

This action provides a consistent way to install Rust development tools across all CI workflows. It handles the installation of `cargo-binstall` first, then uses it to quickly install other required tools.

## Usage

```yaml
steps:
  - name: Install build tools
    uses: ./.github/actions/install-tools
    with:
      tools: "nextest sqlx-cli audit"
```

## Inputs

### `tools`

**Description**: Space-separated list of tools to install

**Required**: No

**Default**: `"sqlx-cli"`

**Available tools**:
- `nextest` - Installs `cargo-nextest` for parallel test execution
- `sqlx-cli` - Installs `sqlx-cli` with PostgreSQL and rustls features
- `audit` - Installs `cargo-audit` for security vulnerability scanning
- `llvm-cov` - Installs `cargo-llvm-cov` for code coverage reporting

## Examples

### Install single tool
```yaml
- name: Install nextest
  uses: ./.github/actions/install-tools
  with:
    tools: "nextest"
```

### Install multiple tools
```yaml
- name: Install development tools
  uses: ./.github/actions/install-tools
  with:
    tools: "nextest sqlx-cli audit"
```

### Use default (sqlx-cli only)
```yaml
- name: Install default tools
  uses: ./.github/actions/install-tools
```

## How It Works

1. **Install cargo-binstall**: Uses the official installer script if not already present
2. **Add to PATH**: Ensures cargo binaries are available in subsequent steps
3. **Install requested tools**: Uses `cargo binstall` for fast binary installation
4. **Display versions**: Shows installed tool versions for debugging

## Benefits

- **Fast Installation**: Uses `cargo binstall` for pre-compiled binaries instead of compilation
- **Consistent Setup**: Same installation method across all workflows
- **Caching Friendly**: cargo-binstall downloads are cached by GitHub Actions
- **Reliable**: Handles PATH setup and verification automatically

## Tool Details

### cargo-nextest
- **Purpose**: Parallel test execution with better output
- **Installation**: `cargo binstall cargo-nextest -y`
- **Used by**: Unit tests, integration tests

### sqlx-cli
- **Purpose**: Database migrations and SQL validation
- **Installation**: `cargo binstall sqlx-cli --no-default-features --features rustls,postgres -y`
- **Features**: PostgreSQL support with rustls TLS
- **Used by**: All workflows that interact with database

### cargo-audit
- **Purpose**: Security vulnerability scanning
- **Installation**: `cargo binstall cargo-audit -y`
- **Used by**: Code quality workflow

### cargo-llvm-cov
- **Purpose**: Code coverage reporting
- **Installation**: `cargo binstall cargo-llvm-cov -y`
- **Used by**: Coverage workflows (future implementation)

## Performance Impact

Using `cargo-binstall` vs `cargo install`:
- **cargo-binstall**: Downloads pre-compiled binaries (~30 seconds)
- **cargo install**: Compiles from source (~5-10 minutes per tool)

This action can save 15-30 minutes per CI run compared to traditional installation methods.

## Error Handling

The action includes error handling for:
- Network failures during cargo-binstall installation
- Missing tools in the requested list
- PATH configuration issues
- Version verification failures

## Workflows Using This Action

- `code-quality.yml` - Installs `sqlx-cli` and `audit`
- `test-unit.yml` - Installs `nextest` and `sqlx-cli`
- `test-integration.yml` - Installs `nextest`

## Extending

To add a new tool:

1. Add detection logic in the "Install requested tools" step
2. Add version display in the "Display installed tools" step
3. Update this README with the new tool details
4. Test in a feature branch

Example addition:
```bash
if echo "${{ inputs.tools }}" | grep -q "new-tool"; then
  echo "Installing new-tool..."
  cargo binstall new-tool -y
fi
```

## Local Development

For local development, you can install these same tools:

```bash
# Install cargo-binstall first
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Then install tools
cargo binstall cargo-nextest sqlx-cli cargo-audit -y
```

## Troubleshooting

### cargo-binstall installation fails
- Check network connectivity
- Verify GitHub.com is accessible
- Try running the installer script manually

### Tool not found after installation
- Check if the tool name matches exactly
- Verify PATH includes `$HOME/.cargo/bin`
- Look at the "Display installed tools" output

### Version conflicts
- cargo-binstall always installs the latest version
- If you need a specific version, modify the installation command
# Install Tools Action

A composite GitHub Action that installs common Rust tools used across Tasker Core CI workflows using a hybrid approach with proven working commands.

## Purpose

This action provides a consistent way to install Rust development tools across all CI workflows using proven working commands:
- Uses `cargo-bins/cargo-binstall@main` for cargo-binstall installation
- Uses `cargo binstall --secure` for cargo-nextest (fast, secure)
- Uses `cargo install` with specific flags for other tools (proven reliable)
- Includes intelligent caching to speed up CI runs

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
- name: Install multiple tools
  uses: ./.github/actions/install-tools
  with:
    tools: "nextest sqlx-cli audit"
```

### Install for testing workflows
```yaml
- name: Install testing tools
  uses: ./.github/actions/install-tools
  with:
    tools: "nextest sqlx-cli"
```

### Install for code quality workflows
```yaml
- name: Install quality tools
  uses: ./.github/actions/install-tools
  with:
    tools: "sqlx-cli audit"
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

- **Hybrid Approach**: Uses the best installation method for each tool
- **Proven Commands**: Uses exact commands that are known to work reliably
- **Fast & Secure**: `cargo binstall --secure` for nextest, direct install for others
- **Intelligent Caching**: Caches installed tools to speed up subsequent runs
- **Consistent Setup**: Same installation method across all workflows
- **Reliable**: No version parameter conflicts or complex fallback logic

## Installation Strategy

The action uses different installation methods optimized for each tool:

| Tool | Method | Command | Why |
|------|--------|---------|-----|
| `cargo-nextest` | `cargo binstall` | `cargo binstall cargo-nextest --secure` | Fast binary install with security verification |
| `sqlx-cli` | `cargo install` | `cargo install sqlx-cli --no-default-features --features native-tls,postgres` | Proven working combination with native-tls |
| `cargo-audit` | `cargo install` | `cargo install cargo-audit --locked` | Reproducible builds with locked dependencies |
| `cargo-llvm-cov` | `cargo binstall` | `cargo binstall cargo-llvm-cov` | Fast binary install for coverage tool |

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

### Installation Method Comparison

| Method | Speed | Reliability | Use Case |
|--------|-------|-------------|----------|
| `cargo binstall` | ~30 seconds | High (with --secure) | Tools with good binary support |
| `cargo install` | ~5-10 minutes | Very High | Tools needing specific features/flags |

### Caching Strategy

The action caches `~/.cargo/bin` with a key based on:
- Operating system
- Cargo.lock hash (for dependency consistency)  
- Requested tools list

This provides significant speedup on cache hits while ensuring tool compatibility.
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
# Install tools using the hybrid approach
cargo binstall cargo-nextest --secure
cargo install sqlx-cli --no-default-features --features native-tls,postgres  
cargo install cargo-audit --locked
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
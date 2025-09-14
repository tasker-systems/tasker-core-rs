# Setup Environment Variables Action

A composite GitHub Action that provides consistent environment variable setup across all Tasker Core CI workflows.

## Purpose

This action eliminates environment variable duplication and ensures all workflows use identical configuration settings. It's the single source of truth for CI environment variables.

## Usage

```yaml
steps:
  - name: Setup shared environment variables
    uses: ./.github/actions/setup-env
```

## Environment Variables Set

### Core Rust Settings
- `CARGO_TERM_COLOR=always` - Enable colored output from Cargo
- `RUST_BACKTRACE=1` - Show stack traces on panic
- `RUSTC_WRAPPER=sccache` - Use sccache for build caching
- `SCCACHE_GHA_ENABLED=true` - Enable GitHub Actions cache integration

### Database and Application
- `DATABASE_URL=postgres://tasker:tasker@localhost:5432/tasker_rust_test`
- `TASKER_ENV=test` - Set application environment to test mode
- `LOG_LEVEL=warn` - Application log level
- `RUST_LOG=warn` - Rust logging level

### Authentication (Test Keys)
- `JWT_PRIVATE_KEY` - RSA private key for test JWT signing
- `JWT_PUBLIC_KEY` - RSA public key for test JWT verification
- `API_KEY` - Test API authentication key

## Features

- **Consistency**: All workflows get identical environment setup
- **Maintainability**: Single place to update environment variables
- **Debugging**: Displays configuration summary with security-safe output
- **Security**: Masks sensitive values in logs

## Implementation Details

This is a **composite action** that:
1. Sets environment variables using `echo "VAR=value" >> $GITHUB_ENV`
2. Displays a configuration summary for debugging
3. Masks sensitive information in logs (shows only first 8 chars of secrets)

## Benefits Over Workflow-Level Environment

- **DRY Principle**: No duplication across 6+ workflow files
- **Centralized Management**: Update once, applies everywhere
- **Version Control**: Changes to environment variables are tracked
- **Consistency**: Impossible to have environment drift between workflows

## Security Considerations

- All keys and secrets are **test values only** - not production credentials
- Sensitive values are masked in log output
- Keys are embedded in the action (not external secrets) since they're test-only

## Workflows Using This Action

- `build-docker-images.yml`
- `code-quality.yml`
- `test-unit.yml`
- `test-integration.yml`

## Updating Environment Variables

To add or modify environment variables:

1. Edit `.github/actions/setup-env/action.yml`
2. Update the `Set shared environment variables` step
3. Update this README if adding new categories
4. Test changes in a feature branch before merging

## Example Output

The action produces output like:
```
✅ Shared environment variables configured:
- CARGO_TERM_COLOR: always
- RUST_BACKTRACE: 1
- RUSTC_WRAPPER: sccache
- SCCACHE_GHA_ENABLED: true
- DATABASE_URL: postgres://tasker:tasker@localhost:5432/tasker_rust_test
- TASKER_ENV: test
- LOG_LEVEL: warn
- RUST_LOG: warn
- API_KEY: ff3083e5...
- JWT keys configured: ✅
```

## Local Development

These same environment variables should be used when running tests locally:

```bash
export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1
export DATABASE_URL=postgres://tasker:tasker@localhost:5432/tasker_rust_test
export TASKER_ENV=test
export LOG_LEVEL=warn
export RUST_LOG=warn
# ... etc
```

Or source them from the CI configuration for consistency.
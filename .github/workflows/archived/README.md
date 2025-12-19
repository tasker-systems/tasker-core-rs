# Archived GitHub Actions Workflows

This directory contains workflows that have been superseded or temporarily disabled.

## Superseded Workflows (December 2024)

These workflows were replaced by the DAG-based CI pipeline restructure.

### `test-e2e.yml`
**Replaced by**: `test-integration.yml` (integration-tests job with 2-way partitioning)

### `test-unit.yml`
**Replaced by**: `test-integration.yml` (unit-tests job runs in parallel with build-workers)

### `test-ruby-unit.yml`
**Replaced by**: `test-ruby-framework.yml` (comprehensive Ruby framework tests)

## Previously Archived

### `release.yml.disabled`
**Reason**: Prevents accidental publishing to crates.io during active development

**Original Purpose**:
- Automated release workflow with cross-platform binaries
- Publishes to crates.io and generates documentation
- Creates GitHub releases with multi-platform binary builds

**When to Re-enable**:
- When ready for first public release
- Rename back to `release.yml` and move to `.github/workflows/`
- Ensure version numbers and release strategy are finalized

### `ci-backup.yml`
**Reason**: Backup of previous CI structure before pipeline restructure

### Other archived files
- `claude-code-review.yml` / `claude-code-review.yml.disabled` - Claude code review integration
- `claude.yml` - Claude integration
- `integration-tests.yml` - Old integration test structure
- `ruby-gem.yml` - Ruby gem publishing

## Safety Notes

- Archived workflows will not run (GitHub ignores files outside `.github/workflows/`)
- Files with `.disabled` extension are explicitly marked as inactive
- This prevents accidental triggers from tags or manual workflow runs
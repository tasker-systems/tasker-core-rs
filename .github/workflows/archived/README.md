# Archived GitHub Actions Workflows

This directory contains workflows that are temporarily disabled during development.

## Currently Archived

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

**Re-activation Steps**:
```bash
# When ready for releases
mv .github/workflows/archived/release.yml.disabled .github/workflows/release.yml
```

## Safety Notes

- Archived workflows with `.disabled` extension will not run
- GitHub Actions ignores files outside `.github/workflows/` directory
- This prevents accidental triggers from tags or manual workflow runs
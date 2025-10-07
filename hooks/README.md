# Git Hooks

This directory contains version-controlled git hooks for the tasker-core project.

## Available Hooks

### pre-commit
Runs before every commit to ensure code quality:
- **Rust formatting** (`cargo fmt`) - Auto-formats code if needed
- **Clippy checks** (`cargo clippy`) - Lints code with all features enabled
- **Compilation check** (`cargo check`) - Verifies library and binary targets compile

**Key Features:**
- Uses `SQLX_OFFLINE=true` to work without database connection
- Only checks library and binary targets (excludes tests)
- Fast feedback loop for local development

### pre-push
Runs before pushing to remote to validate code quality:
- **Documentation builds** (`cargo doc`) - Ensures documentation compiles
- **Clippy checks** (`cargo clippy`) - Final lint check before push

**Key Features:**
- Uses `SQLX_OFFLINE=true` to work without database connection
- **No test execution** - Tests are run manually and CI will catch failures
- Faster push workflow

## Installation

### Option 1: Symlink (Recommended)

```bash
# From repository root
ln -sf ../../hooks/pre-commit .git/hooks/pre-commit
ln -sf ../../hooks/pre-push .git/hooks/pre-push
```

**Benefits:**
- Hooks automatically update when you pull changes
- Changes to hooks are version controlled

### Option 2: Copy

```bash
# From repository root
cp hooks/pre-commit .git/hooks/pre-commit
cp hooks/pre-push .git/hooks/pre-push
chmod +x .git/hooks/pre-commit .git/hooks/pre-push
```

**Note:** You'll need to manually copy updates when hooks change.

### Option 3: Automated Setup Script

```bash
#!/bin/bash
# install-hooks.sh

HOOKS_DIR="$(git rev-parse --show-toplevel)/hooks"
GIT_HOOKS_DIR="$(git rev-parse --show-toplevel)/.git/hooks"

for hook in pre-commit pre-push; do
    if [ -f "$HOOKS_DIR/$hook" ]; then
        ln -sf "../../hooks/$hook" "$GIT_HOOKS_DIR/$hook"
        echo "✅ Installed $hook hook"
    fi
done
```

## Rationale

### Why SQLX_OFFLINE=true?

The project uses `sqlx` with compile-time query verification. This requires a database connection during compilation to verify SQL queries. However:

1. **Local development**: Database may not always be running
2. **CI environments**: Database setup adds complexity
3. **Fast feedback**: Compilation shouldn't block on database

Using `SQLX_OFFLINE=true` allows the hooks to use cached query metadata from `.sqlx/` directories, enabling validation without database connectivity.

### Why No Tests in Hooks?

1. **Developer workflow**: Tests are run manually during development
2. **CI coverage**: GitHub Actions runs full test suite on every PR/push
3. **Speed**: Hooks complete in seconds, not minutes
4. **Flexibility**: Developers can skip hooks with `--no-verify` if needed

The pre-commit and pre-push hooks focus on:
- Code formatting consistency
- Lint cleanliness
- Compilation verification

Tests are better suited for:
- Manual execution: `cargo test --all-features`
- CI pipelines: Comprehensive test matrix
- Pre-release validation: Full integration test suite

### Why Exclude Test Targets from Compilation Check?

Test targets in `tests/common/lifecycle_test_manager.rs` contain SQL queries without cached metadata. These are integration tests that require a running database. Excluding them from the pre-commit check:

1. **Prevents false failures** when database is not running
2. **Faster feedback** - library and binary targets compile quickly
3. **CI coverage** - Integration tests run in CI with database

## Troubleshooting

### Hook fails with "SQLX_OFFLINE=true but no cached data"

This means the `.sqlx/` query cache is out of sync. Regenerate it:

```bash
# Start database
docker-compose -f docker/docker-compose.dev.yml up -d postgres

# Regenerate cache
DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_development" \
  cargo sqlx prepare --workspace -- --all-features

# Commit updated cache
git add .sqlx/
git commit -m "chore: update SQLX query cache"
```

### Hook fails with clippy warnings

Fix the warnings:

```bash
cargo clippy --fix --allow-dirty --all-features
```

Or review and fix manually based on the error messages.

### Hook fails with formatting issues

The hook auto-formats code. Review the changes and re-stage:

```bash
git add .
git commit
```

### Bypassing Hooks (Emergency Only)

If you absolutely need to bypass hooks:

```bash
git commit --no-verify
git push --no-verify
```

**Warning:** Only use this when:
- Hooks have a bug that needs fixing
- Emergency hotfix that will be cleaned up in follow-up commit
- CI will catch any real issues

## Updating Hooks

When updating hooks:

1. Edit the files in `hooks/` directory
2. Test the changes:
   ```bash
   ./hooks/pre-commit
   ./hooks/pre-push
   ```
3. Commit the changes:
   ```bash
   git add hooks/
   git commit -m "chore: update git hooks"
   ```
4. Reinstall (if using copy method):
   ```bash
   cp hooks/pre-commit .git/hooks/pre-commit
   cp hooks/pre-push .git/hooks/pre-push
   ```

If using symlinks, changes take effect immediately!

## Integration with CI

These hooks complement CI, not replace it:

| Check | Pre-Commit | Pre-Push | CI |
|-------|------------|----------|-----|
| Format | ✅ | - | ✅ |
| Clippy | ✅ | ✅ | ✅ |
| Compile (lib/bins) | ✅ | - | ✅ |
| Compile (all) | - | - | ✅ |
| Documentation | - | ✅ | ✅ |
| Tests | - | - | ✅ |
| Integration Tests | - | - | ✅ |
| Coverage | - | - | ✅ |

## Performance

Typical hook execution times on modern hardware:

- **pre-commit**: 3-10 seconds
- **pre-push**: 5-15 seconds

Fast enough for instant feedback, thorough enough to catch common issues.

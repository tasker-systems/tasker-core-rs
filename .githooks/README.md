# Git Hooks for Tasker Core Rust

This directory contains git hooks that ensure code quality and consistency for the Tasker Core Rust project.

## Quick Setup

```bash
# Make the installer executable and run it
chmod +x .githooks/install.sh
./.githooks/install.sh
```

## Available Hooks

### `pre-commit`

Runs before each commit to ensure code quality:

- **Code Formatting**: Runs `cargo fmt` and ensures code is properly formatted
- **Clippy Linting**: Runs `cargo clippy` with warnings treated as errors
- **Compilation Check**: Ensures code compiles successfully with `cargo check`

**What happens if checks fail:**
- For formatting issues: Auto-runs `cargo fmt` and asks you to re-stage
- For clippy issues: Shows errors and suggests running `cargo clippy --fix`
- For compilation errors: Shows errors and blocks commit

### `pre-push`

Runs before pushing to remote repository for comprehensive validation:

- **Full Test Suite**: Runs `cargo test --all-features`
- **Documentation Build**: Ensures `cargo doc` builds without warnings
- **Complete Clippy Check**: Runs clippy on all targets and features

## Skipping Hooks

Sometimes you may need to skip hooks (use sparingly):

```bash
# Skip pre-commit hook
git commit --no-verify

# Skip pre-push hook  
git push --no-verify
```

## Manual Hook Execution

You can run the hooks manually at any time:

```bash
# Run pre-commit checks
./.githooks/pre-commit

# Run pre-push checks
./.githooks/pre-push
```

## Benefits

✅ **Consistent Code Style**: Automatic formatting ensures uniform code style
✅ **Early Error Detection**: Catch issues before they reach CI/CD
✅ **Improved Code Quality**: Clippy catches common mistakes and suggests improvements
✅ **Documentation Compliance**: Ensures documentation always builds successfully
✅ **Test Coverage**: Prevents broken code from being pushed

## Integration with CI/CD

These hooks complement the GitHub Actions CI pipeline:
- **Local Development**: Hooks catch issues immediately during development
- **CI/CD Pipeline**: Final validation in controlled environment
- **Faster Feedback**: Fix issues locally rather than waiting for CI

## Troubleshooting

### Hook Permission Issues
```bash
chmod +x .githooks/*
./.githooks/install.sh
```

### Rust/Cargo Not Found
Ensure Rust is installed and `cargo` is in your PATH:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Database Connection Issues
For tests requiring database:
```bash
export DATABASE_URL="postgresql://localhost/tasker_rust_test"
```

## Uninstalling

To remove hooks:
```bash
rm .git/hooks/pre-commit
rm .git/hooks/pre-push
```
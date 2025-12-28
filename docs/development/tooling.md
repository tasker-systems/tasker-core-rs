# Build Tooling and Task Runner System

This document describes the cargo-make based build system used across the tasker-core workspace. The system provides unified commands across Rust core and polyglot workers (Python, Ruby, TypeScript, Rust).

---

## Quick Start

```bash
# Run all quality checks across the entire workspace
cargo make check

# Run all tests
cargo make test

# Auto-fix all fixable issues
cargo make fix

# Build everything
cargo make build

# Show all available tasks
cargo make --list-all-steps
```

---

## Architecture Overview

The cargo-make configuration follows a hierarchical inheritance pattern:

```
Makefile.toml (root)
    └── extends → cargo-make/main.toml
                      └── extends → cargo-make/base-tasks.toml

Crate-level Makefile.toml files:
    pgmq-notify/Makefile.toml      → extends → cargo-make/base-tasks.toml
    tasker-client/Makefile.toml    → extends → cargo-make/base-tasks.toml
    tasker-shared/Makefile.toml    → extends → cargo-make/base-tasks.toml
    tasker-orchestration/Makefile.toml → extends → cargo-make/base-tasks.toml
    tasker-worker/Makefile.toml    → extends → cargo-make/base-tasks.toml
    workers/rust/Makefile.toml     → extends → cargo-make/base-tasks.toml
```

Worker directories have their own complete Makefile.toml files (not extending base-tasks):
- `workers/python/Makefile.toml` - Uses uv, maturin, ruff, mypy, pytest
- `workers/ruby/Makefile.toml` - Uses bundler, rake, rubocop, rspec
- `workers/typescript/Makefile.toml` - Uses bun, biome, vitest

---

## Directory Structure

```
cargo-make/
├── main.toml              # Entry point, chains all modules
├── base-tasks.toml        # Base task templates for extension
├── workspace-config.toml  # Shared workspace configuration
├── cross-cutting.toml     # Cross-language quality tasks
├── test-tasks.toml        # Test configuration and profiles
├── ci-integration.toml    # CI workflow alignment
└── scripts/               # Shell scripts for complex operations
    ├── check-db.sh        # Database connectivity check
    ├── run-migrations.sh  # Run database migrations
    ├── reset-db.sh        # Reset database (drop/recreate)
    ├── sqlx-prepare.sh    # Prepare SQLX query cache
    ├── setup-workers.sh   # Setup all polyglot workers
    ├── clean-workers.sh   # Clean all worker artifacts
    ├── check-services.sh  # Verify required services running
    └── setup-env.sh       # CI environment setup
```

---

## Configuration Files

### `cargo-make/main.toml`

The main entry point that chains all module configurations. This file:
- Extends `base-tasks.toml` for core Rust tasks
- Defines cross-cutting composite tasks (code-quality, pre-commit, pre-push)
- Configures workspace-wide parallel operations
- Sets up CI integration tasks

### `cargo-make/base-tasks.toml`

Defines base task templates that crate-level Makefile.toml files extend:

| Base Task | Description | Used By |
|-----------|-------------|---------|
| `base-rust-format` | Check Rust formatting | All Rust crates |
| `base-rust-format-fix` | Fix Rust formatting | All Rust crates |
| `base-rust-lint` | Run Clippy lints | All Rust crates |
| `base-rust-lint-fix` | Fix Clippy issues | All Rust crates |
| `base-rust-test` | Run tests with nextest | All Rust crates |
| `base-rust-build` | Build crate | All Rust crates |
| `base-rust-build-release` | Build release | All Rust crates |

### `Makefile.toml` (Root)

The root configuration provides:
- Top-level composite tasks (`check`, `test`, `fix`, `build`)
- Language-specific delegation (`check-rust`, `check-python`, `check-ruby`, `check-typescript`)
- Database operations (`db-setup`, `db-check`, `db-migrate`, `db-reset`)
- SQLX cache management (`sqlx-prepare`, `sqlx-check`)
- Docker operations (`docker-up`, `docker-down`, `docker-logs`)
- FFI test delegation (`test-python-ffi`, `test-ruby-ffi`, `test-typescript-ffi`)
- CI tasks (`ci-check`, `ci-test`, `ci-prepare`)

---

## Shell Scripts

All complex operations are externalized to shell scripts in `cargo-make/scripts/` for easier debugging and maintenance.

### `check-db.sh`
Verifies PostgreSQL connectivity using `pg_isready`. Exits with helpful error message if database is unavailable.

### `run-migrations.sh`
Runs SQLx migrations against the configured DATABASE_URL. Validates connection before attempting migration.

### `reset-db.sh`
Drops and recreates the test database. **Use with caution** - this destroys all data.

### `sqlx-prepare.sh`
Prepares the SQLX offline query cache for all workspace crates. Required after modifying any `sqlx::query!` macros.

### `setup-workers.sh`
Sets up all polyglot workers in parallel:
- Python: Creates venv, syncs dependencies with uv
- Ruby: Runs bundle install
- TypeScript: Runs bun install

### `clean-workers.sh`
Cleans build artifacts from all workers:
- Python: Removes .venv, target, cache directories
- Ruby: Runs rake clean, removes compiled extensions
- TypeScript: Removes node_modules, dist, cache directories

### `check-services.sh`
Verifies required services are running for integration tests:
- PostgreSQL on configured port
- Worker services on their respective ports

### `setup-env.sh`
CI environment setup that mirrors `.github/actions/setup-env`:
- Installs required tools
- Configures environment variables
- Validates prerequisites

---

## Task Categories

### Top-Level Tasks

| Task | Description |
|------|-------------|
| `cargo make check` | Run all quality checks across workspace |
| `cargo make test` | Run all tests across workspace |
| `cargo make fix` | Auto-fix all fixable issues |
| `cargo make build` | Build everything |

### Language-Specific Tasks

| Task | Description |
|------|-------------|
| `cargo make check-rust` | Rust: fmt, clippy, docs, doctests |
| `cargo make check-python` | Python: ruff format, ruff lint, mypy, pytest |
| `cargo make check-ruby` | Ruby: rubocop, rust-check, compile, rspec |
| `cargo make check-typescript` | TypeScript: biome, tsc, vitest |

### Database Tasks

| Task | Description |
|------|-------------|
| `cargo make db-setup` | Setup database with migrations |
| `cargo make db-check` | Check database connectivity |
| `cargo make db-migrate` | Run database migrations |
| `cargo make db-reset` | Reset database (drop and recreate) |
| `cargo make sqlx-prepare` | Prepare SQLX query cache |
| `cargo make sqlx-check` | Verify SQLX cache is up to date |

### Docker Tasks

| Task | Description |
|------|-------------|
| `cargo make docker-up` | Start Docker services (postgres with PGMQ) |
| `cargo make docker-down` | Stop Docker services |
| `cargo make docker-logs` | Show Docker logs |

### CI Tasks

| Task | Description |
|------|-------------|
| `cargo make ci-check` | CI quality checks (fmt, clippy, docs, audit) |
| `cargo make ci-test` | CI test run with CI profile |
| `cargo make ci-prepare` | Prepare for offline builds |
| `cargo make ci-flow` | Complete CI flow |

### FFI Test Tasks

| Task | Description |
|------|-------------|
| `cargo make test-python-ffi` | Run Python FFI integration tests |
| `cargo make test-ruby-ffi` | Run Ruby FFI integration tests |
| `cargo make test-typescript-ffi` | Run TypeScript FFI tests (all runtimes) |
| `cargo make test-ffi-all` | Run all FFI integration tests |

### Shortcuts

| Shortcut | Expands To |
|----------|-----------|
| `cargo make c` | `cargo make check` |
| `cargo make t` | `cargo make test` |
| `cargo make f` | `cargo make fix` |
| `cargo make b` | `cargo make build` |

---

## Crate-Level Usage

Each Rust crate has its own `Makefile.toml` that extends the base tasks:

```bash
# Run checks for a specific crate
cd pgmq-notify && cargo make check

# Run tests for a specific crate
cd tasker-shared && cargo make test

# Run the orchestration service
cd tasker-orchestration && cargo make run
```

### Crate Makefile Pattern

All crate-level Makefile.toml files follow this pattern:

```toml
# Must be at root level, NOT inside [config]
extend = "../cargo-make/base-tasks.toml"

[config]
default_to_workspace = false

[env]
CRATE_NAME = "crate-name-here"

[tasks.default]
alias = "check"

[tasks.check]
description = "Run quality checks"
dependencies = ["format-check", "lint", "test"]

[tasks.format-check]
extend = "base-rust-format"

[tasks.lint]
extend = "base-rust-lint"

[tasks.test]
extend = "base-rust-test"
args = ["nextest", "run", "-p", "${CRATE_NAME}", "--all-features"]
```

---

## Worker-Specific Details

### Python Worker (`workers/python/`)

**Tools**: uv (package manager), maturin (Rust extension builder), ruff (linter/formatter), mypy (type checker), pytest

```bash
cd workers/python
cargo make check    # format-check, lint, typecheck, test
cargo make build    # Build Rust extension with maturin
cargo make fix      # Auto-fix with ruff
cargo make test-ffi # Run FFI tests
```

### Ruby Worker (`workers/ruby/`)

**Tools**: bundler (package manager), rake (build), rubocop (linter), rspec (tests), magnus (Rust FFI)

```bash
cd workers/ruby
cargo make check    # lint, rust-check, build, test
cargo make build    # Compile Rust extension
cargo make fix      # Auto-fix with rubocop
cargo make test-ffi # Run FFI tests (spec/ffi/)
```

### TypeScript Worker (`workers/typescript/`)

**Tools**: bun (runtime/package manager), biome (linter/formatter), vitest (tests)

```bash
cd workers/typescript
cargo make check    # lint, typecheck, test
cargo make build    # Build with bun
cargo make fix      # Auto-fix with biome
cargo make test-ffi-all # Run FFI tests (Bun, Node, Deno)
```

### Rust Worker (`workers/rust/`)

**Tools**: Standard Rust toolchain (cargo, clippy, rustfmt)

```bash
cd workers/rust
cargo make check    # format-check, lint, test
cargo make build    # Build worker binary
cargo make run      # Run the worker service
```

---

## Environment Variables

The build system uses these environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgresql://tasker:tasker@localhost:5432/tasker_rust_test` | Database connection string |
| `TASKER_ENV` | `test` | Environment (test, development, production) |
| `SCRIPTS_DIR` | `cargo-make/scripts` | Path to shell scripts |
| `PGPORT` | `5432` | PostgreSQL port for health checks |
| `PGUSER` | `tasker` | PostgreSQL user for health checks |

---

## Common Workflows

### Starting Development

```bash
# Start database
cargo make docker-up

# Setup all workers
cargo make setup

# Run all checks
cargo make check
```

### Before Committing

```bash
# Quick pre-commit checks
cargo make pre-commit

# Or full check
cargo make check
```

### Before Pushing

```bash
# Thorough pre-push validation
cargo make pre-push
```

### After Modifying SQL

```bash
# Update SQLX cache
cargo make sqlx-prepare

# Verify cache is valid
cargo make sqlx-check
```

### Running CI Locally

```bash
# Run the full CI flow
cargo make ci-flow

# Or individual CI tasks
cargo make ci-check
cargo make ci-test
```

---

## Troubleshooting

### "extend" Not Working

The `extend` directive must be at the **root level** of the TOML file, NOT inside `[config]`:

```toml
# CORRECT
extend = "../cargo-make/base-tasks.toml"

[config]
default_to_workspace = false

# WRONG - will show "Found unknown key: config.?.extend"
[config]
extend = { path = "../cargo-make/base-tasks.toml" }
```

### Script Path Issues

Use relative paths for `SCRIPTS_DIR` to avoid path duplication:

```toml
# CORRECT
SCRIPTS_DIR = "cargo-make/scripts"

# WRONG - causes path duplication
SCRIPTS_DIR = "${CARGO_MAKE_WORKING_DIRECTORY}/cargo-make/scripts"
```

### Task Not Found in Crate

Ensure the crate's Makefile.toml properly extends base-tasks.toml and defines the task:

```bash
# Check if task exists
cd <crate> && cargo make --list-all-steps | grep <task>
```

### Worker Setup Failures

```bash
# Clean and retry
cargo make clean-workers
cargo make setup-workers
```

---

## Adding New Tasks

### To Add a Workspace-Wide Task

1. Add the task definition to `cargo-make/main.toml` or the appropriate module file
2. If it uses shell commands, create a script in `cargo-make/scripts/`
3. Reference the script using `script = { file = "${SCRIPTS_DIR}/script-name.sh" }`

### To Add a Crate-Specific Task

1. Add the task to the crate's `Makefile.toml`
2. For Rust tasks, extend a base task if applicable
3. Use `CRATE_NAME` environment variable for package-specific commands

### To Add a New Base Task

1. Add the task template to `cargo-make/base-tasks.toml`
2. Name it with `base-` prefix (e.g., `base-rust-doc`)
3. Update crate Makefile.toml files to extend the new base task

---

## Related Documentation

- [Development Patterns](./development-patterns.md) - General development patterns
- [MPSC Channel Guidelines](./mpsc-channel-guidelines.md) - Channel configuration
- [TAS-111 Spec](../ticket-specs/TAS-111.md) - Original specification for this tooling

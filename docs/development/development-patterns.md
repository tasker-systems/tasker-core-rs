# Development Patterns

**Last Updated**: 2025-12-22
**Audience**: Engineers working on tasker-core
**Related**: [Crate Architecture](../crate-architecture.md), Worker READMEs

## Overview

tasker-core is a polyglot workspace combining Rust core crates with language-specific workers (Ruby, Python, TypeScript). This guide covers unified development patterns using `cargo-make` as the task runner.

## Quick Reference

```bash
# Install cargo-make (one-time)
cargo install cargo-make

# Common commands (from workspace root)
cargo make check      # Run all quality checks
cargo make test       # Run all tests
cargo make fix        # Auto-fix all issues
cargo make build      # Build everything

# Language-specific
cargo make check-rust       # Rust core only
cargo make check-python     # Python worker only
cargo make check-ruby       # Ruby worker only
cargo make check-typescript # TypeScript worker only

# Infrastructure
cargo make db-setup         # Setup database with migrations
cargo make sqlx-prepare     # Prepare SQLX offline cache
cargo make docker-up        # Start Docker services
```

## Workspace Structure

```
tasker-core/
├── Makefile.toml              # Root orchestrator
├── Cargo.toml                 # Workspace members
├── workers/
│   ├── python/
│   │   ├── Makefile.toml      # Python tasks (uv, maturin, ruff, pytest)
│   │   └── pyproject.toml
│   ├── ruby/
│   │   ├── Makefile.toml      # Ruby tasks (bundler, rubocop, rspec)
│   │   └── Gemfile
│   └── typescript/
│       ├── Makefile.toml      # TypeScript tasks (bun, biome, vitest)
│       └── package.json
└── [rust crates...]           # Core Rust crates
```

## Task Taxonomy

All workers follow a consistent task taxonomy:

| Task | Description | Rust | Python | Ruby | TypeScript |
|------|-------------|------|--------|------|------------|
| `check` | Run all quality checks | clippy, fmt, docs | ruff, mypy | rubocop | biome, tsc |
| `test` | Run test suite | nextest | pytest | rspec | vitest |
| `fix` | Auto-fix issues | fmt, clippy --fix | ruff --fix | rubocop -a | biome --write |
| `build` | Build artifacts | cargo build | maturin develop | rake compile | tsup + cargo |
| `setup` | Install dependencies | - | uv sync | bundle install | bun install |
| `clean` | Remove build artifacts | cargo clean | rm .venv, target | rake clean | rm dist, node_modules |

## Rust Core Tasks

### Quality Checks

```bash
cargo make check-rust   # All checks: fmt, clippy, docs, doctests

# Individual tasks
cargo make rust-fmt-check    # Check formatting
cargo make rust-fmt-fix      # Fix formatting
cargo make rust-clippy       # Run clippy lints
cargo make rust-clippy-fix   # Fix clippy issues
cargo make rust-docs         # Build documentation
cargo make rust-doctests     # Run documentation tests
cargo make rust-audit        # Security audit
```

### Testing

```bash
cargo make test-rust          # Run with nextest
cargo make test-rust-verbose  # With output (--no-capture)
```

### SQLX Cache

The SQLX offline cache enables compilation without database access (CI, Docker builds):

```bash
# After adding/modifying sqlx::query! macros:
cargo make sqlx-prepare   # Updates .sqlx/ directories

# Verify cache is valid:
cargo make sqlx-check     # SQLX_OFFLINE=true cargo check
```

**Important**: Commit `.sqlx/` directories after running `sqlx-prepare`.

## Python Worker Tasks

Located in `workers/python/`. Uses:
- **uv**: Fast Python package manager
- **maturin**: Rust-Python FFI builds (PyO3)
- **ruff**: Linting and formatting
- **mypy**: Type checking
- **pytest**: Testing

```bash
cd workers/python

cargo make check       # format-check, lint, typecheck, test
cargo make build       # Build Rust extension with maturin
cargo make fix         # format-fix, lint-fix
cargo make test        # pytest
cargo make test-verbose
cargo make test-coverage

# Rust extension tasks
cargo make rust-check
cargo make rust-clippy
cargo make rust-fmt
```

### First-Time Setup

```bash
cd workers/python
cargo make setup   # Creates .venv, syncs dependencies
```

## Ruby Worker Tasks

Located in `workers/ruby/`. Uses:
- **Bundler**: Ruby dependency management
- **Magnus**: Rust-Ruby FFI (via rake compile)
- **RuboCop**: Linting
- **RSpec**: Testing

```bash
cd workers/ruby

cargo make check       # lint, rust-check, build, test
cargo make build       # Bundle install + rake compile
cargo make fix         # lint-fix, rust-fmt-fix
cargo make test        # rspec
cargo make test-verbose
cargo make test-unit
cargo make test-integration

# Rust extension tasks (in ext/tasker_core)
cargo make rust-check
cargo make rust-clippy
cargo make rust-fmt
```

### First-Time Setup

```bash
cd workers/ruby
cargo make setup   # bundle install
```

## TypeScript Worker Tasks

Located in `workers/typescript/`. Uses:
- **Bun**: JavaScript runtime and package manager
- **Biome**: Fast linting and formatting
- **tsup**: TypeScript bundler
- **vitest**: Testing (via bun test)

```bash
cd workers/typescript

cargo make check       # lint, typecheck, test
cargo make build       # Build FFI + TypeScript
cargo make fix         # format-fix, lint-fix
cargo make test        # bun test
cargo make test-watch
cargo make test-coverage

# FFI tasks
cargo make build-ffi        # cargo build -p tasker-worker-ts --release
cargo make build-ffi-debug  # cargo build -p tasker-worker-ts (faster)
cargo make build-ts         # bun run build
```

### First-Time Setup

```bash
cd workers/typescript
cargo make install   # bun install --frozen-lockfile
```

## Database & Infrastructure

### Database Setup

```bash
cargo make db-setup    # Check connectivity + run migrations
cargo make db-check    # pg_isready check
cargo make db-migrate  # sqlx migrate run
cargo make db-reset    # Drop, recreate, migrate
```

### Docker Services

```bash
cargo make docker-up    # Start PostgreSQL with PGMQ
cargo make docker-down  # Stop services
cargo make docker-logs  # Follow logs
```

### Required Environment Variables

```bash
# Database (usually from .env)
export DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test
export TASKER_ENV=test
```

## CI Integration

The root `Makefile.toml` provides CI-specific tasks:

```bash
cargo make ci-check    # fmt-check, clippy, docs, audit
cargo make ci-test     # nextest with CI profile
cargo make ci-prepare  # Prepare for offline builds (sqlx-prepare)
```

### CI Workflow Example

```yaml
# .github/workflows/check.yml
jobs:
  check:
    steps:
      - uses: actions/checkout@v4
      - name: Install cargo-make
        run: cargo install cargo-make
      - name: Run checks
        run: cargo make ci-check
      - name: Run tests
        run: cargo make ci-test
```

## Development Workflow

### Full Workspace Check

```bash
# From workspace root
cargo make check   # Runs check-rust, check-python, check-ruby, check-typescript
```

### Watch Mode

```bash
cargo make watch        # Watch + run tests on change
cargo make watch-check  # Watch + run clippy on change

# TypeScript-specific
cd workers/typescript
cargo make dev   # Build debug FFI + watch TypeScript
```

### Quick Shortcuts

```bash
cargo make c   # Alias for check
cargo make t   # Alias for test
cargo make f   # Alias for fix
cargo make b   # Alias for build
```

## Adding New Crates or Workers

### Adding a Rust Crate

1. Add to workspace `Cargo.toml`:
   ```toml
   [workspace]
   members = [
       "your-new-crate",
       # ...
   ]
   ```

2. The root `Makefile.toml` automatically includes it via workspace commands.

### Adding a New Worker

1. Create `workers/<language>/Makefile.toml` following existing patterns
2. Add delegation tasks to root `Makefile.toml`:
   ```toml
   [tasks.check-<language>]
   description = "Run <language> worker quality checks"
   cwd = "workers/<language>"
   command = "cargo"
   args = ["make", "check"]
   ```
3. Add to composite tasks (`check`, `test`, `fix`, `build`)

## Makefile.toml Structure

### Worker Configuration

Each worker `Makefile.toml` should include:

```toml
[config]
default_to_workspace = false   # Prevent workspace resolution issues
skip_core_tasks = true         # Don't inherit cargo-make's built-in tasks

[tasks.default]
description = "Default: run all checks"
alias = "check"

[tasks.check]
description = "Run all quality checks"
dependencies = ["lint", "typecheck", "test"]
```

### Task Types

- **Command tasks**: Direct command execution
  ```toml
  [tasks.build-ffi]
  command = "cargo"
  args = ["build", "-p", "tasker-worker-ts", "--release"]
  ```

- **Script tasks**: Shell scripts
  ```toml
  [tasks.setup]
  script = '''
  if [ ! -d ".venv" ]; then
      uv venv
  fi
  uv sync --group dev
  '''
  ```

- **Composite tasks**: Dependencies on other tasks
  ```toml
  [tasks.build]
  dependencies = ["build-ffi", "build-ts"]
  ```

- **Alias tasks**: Shortcuts to other tasks
  ```toml
  [tasks.c]
  alias = "check"
  ```

## Troubleshooting

### cargo-make not found

```bash
cargo install cargo-make
```

### Worker tasks fail to find dependencies

Ensure you've run setup first:

```bash
cd workers/<language>
cargo make setup
```

### SQLX offline errors

```bash
# Update the cache
cargo make sqlx-prepare
git add .sqlx/
```

### TypeScript FFI library not found

```bash
cd workers/typescript
cargo make build-ffi   # Build the Rust FFI library first
```

### Ruby extension compilation fails

```bash
cd workers/ruby
cargo make compile-clean   # Clean and recompile
```

## Extension Patterns

### Custom Step Handler Resolvers (TAS-93)

The resolver chain pattern allows extending handler resolution with custom strategies. This is useful for:
- Service discovery integration
- Dynamic handler loading from plugins
- Environment-specific resolution
- Custom naming conventions

#### Creating a Custom Resolver

All resolvers implement the `StepHandlerResolver` trait/interface:

```rust
// Rust
#[async_trait]
pub trait StepHandlerResolver: Send + Sync + Debug {
    fn resolver_name(&self) -> &str;
    fn priority(&self) -> u32;
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool;
    async fn resolve(&self, definition: &HandlerDefinition, context: &ResolutionContext)
        -> Result<Arc<dyn ResolvedHandler>, ResolutionError>;
}
```

```ruby
# Ruby
class CustomResolver < TaskerCore::Registry::BaseResolver
  def resolver_name = "custom"
  def priority = 50
  def can_resolve?(definition) = definition.callable.start_with?("custom://")
  def resolve(definition, context) = # Return handler instance
end
```

```python
# Python
class CustomResolver(BaseResolver):
    def resolver_name(self) -> str: return "custom"
    def priority(self) -> int: return 50
    def can_resolve(self, definition: HandlerDefinition) -> bool: ...
    async def resolve(self, definition, context) -> ResolvedHandler: ...
```

```typescript
// TypeScript
class CustomResolver extends BaseResolver {
  resolverName(): string { return 'custom'; }
  priority(): number { return 50; }
  canResolve(definition: HandlerDefinition): boolean { ... }
  async resolve(definition, context): Promise<ResolvedHandler> { ... }
}
```

#### Registering Custom Resolvers

Add your resolver to the chain during worker initialization:

```rust
// Rust (in worker setup)
let mut chain = ResolverChain::new();
chain.register(Arc::new(ExplicitMappingResolver::new()));
chain.register(Arc::new(CustomResolver::new(config))); // Your resolver
chain.register(Arc::new(ClassConstantResolver::new()));
```

**Priority Guidelines**:
| Priority | Use Case |
|----------|----------|
| 1-9 | Override all other resolvers |
| 10 | ExplicitMappingResolver (default) |
| 20-90 | Custom domain resolvers |
| 100 | Class lookup resolver (fallback) - `ClassConstantResolver` (Ruby), `ClassLookupResolver` (Python/TS) |

See [Handler Resolution Guide](../guides/handler-resolution.md) for complete documentation.

## Related Documentation

- [Crate Architecture](../architecture/crate-architecture.md) - Workspace structure
- [MPSC Channel Guidelines](./mpsc-channel-guidelines.md) - Channel patterns
- [Configuration Management](../guides/configuration-management.md) - TOML configuration
- [Deployment Patterns](../architecture/deployment-patterns.md) - Production deployment
- [Handler Resolution Guide](../guides/handler-resolution.md) - Custom resolvers (TAS-93)

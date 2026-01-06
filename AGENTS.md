# CLAUDE.md - Tasker Core Project Context

**Project Status**: Production Ready (v0.1.0) | 1185+ tests | Rust orchestration with PostgreSQL/PGMQ

---

## Quick Reference

### Essential Commands

**Preferred: cargo-make (unified task runner)**
```bash
cargo make check       # All quality checks across workspace
cargo make test        # All tests across workspace
cargo make fix         # Auto-fix all fixable issues
cargo make build       # Build everything

# Shortcuts
cargo make c           # check
cargo make t           # test
cargo make f           # fix
cargo make b           # build

# Language-specific
cargo make check-rust
cargo make check-python
cargo make check-ruby
cargo make check-typescript
```

**Direct cargo commands (when needed)**
```bash
# Build and test (ALWAYS use --all-features)
cargo build --all-features
cargo test --all-features
cargo clippy --all-targets --all-features
cargo fmt

# Fast compilation check
cargo check --all-features

# Run specific package/test
cargo test --all-features --package <package-name>
cargo test --all-features <test-name>

# Documentation
cargo doc --all-features --open
```

### Database Operations
```bash
# Via cargo-make (preferred)
cargo make db-setup     # Setup database with migrations
cargo make db-check     # Check database connectivity
cargo make db-migrate   # Run migrations
cargo make db-reset     # Reset database (drop/recreate)
cargo make sqlx-prepare # Prepare SQLX query cache

# Direct commands (when needed)
export DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test
cargo sqlx migrate run

# Test connectivity
psql $DATABASE_URL -c "SELECT 1"
```

### SQLx Query Cache (Critical)
```bash
# After adding/modifying sqlx::query! macros, update cache:
DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_test \
cargo sqlx prepare --workspace -- --all-targets --all-features

git add .sqlx/
```
**Required when**: Adding `sqlx::query!` macros, modifying SQL, changing schema, or seeing "SQLX_OFFLINE but no cached data" errors.

### Ruby Integration
```bash
cd workers/ruby
bundle install && bundle exec rake compile

# Run integration tests
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test bundle exec rspec spec/integration/ --format documentation
```

### Testing with Nextest
```bash
cargo nextest run --profile default    # Local testing
cargo nextest run --profile ci         # CI profile (JUnit XML)
cargo nextest run --profile local      # Fail-fast mode
```

### Container Operations (Podman)
The project uses Podman as the container runtime. Docker CLI commands work via shell aliases (`docker` → `podman`, `docker-compose` → `podman compose`).

```bash
docker-compose up -d postgres                           # Start PostgreSQL with PGMQ
docker-compose --profile server up -d                   # Full server
docker compose -f docker/docker-compose.test.yml up -d  # Test services

# Direct podman commands (equivalent)
podman compose up -d postgres
podman compose -f docker/docker-compose.test.yml up -d
```

---

## Workspace Structure

```
[workspace.members]
- pgmq-notify          # PGMQ wrapper with notification support
- tasker-client        # Client library for external interactions
- tasker-orchestration # Core orchestration logic (see AGENTS.md)
- tasker-shared        # Shared types, traits, utilities
- tasker-worker        # Worker implementation (see AGENTS.md)
- workers/python       # Python FFI bindings (maturin/pyo3)
- workers/ruby         # Ruby FFI bindings (magnus)
- workers/rust         # Rust worker implementation
- workers/typescript   # TypeScript FFI bindings (Bun/Node/Deno)

[build tooling]
- cargo-make/          # Shared task runner configuration
  - scripts/           # Shell scripts for complex operations
- Makefile.toml        # Root task definitions
```

---

## Critical Rules

### Configuration Structure
Configuration is role-based, NOT component-based:
```
config/tasker/base/
├── common.toml          # Shared (circuit breakers, pgmq, pools)
├── orchestration.toml   # Orchestration settings
└── worker.toml          # Worker settings

config/tasker/environments/{test,development,production}/
├── common.toml          # Environment overrides
├── orchestration.toml
└── worker.toml
```
**Never create separate component files** (e.g., `auth.toml`, `circuit_breakers.toml`).

### Testing Requirements
- **Never use SQLX_OFFLINE=true** - always export DATABASE_URL from `.env`
- **Always use --all-features** for consistency
- E2E tests use `TASKER_FIXTURE_PATH` for fixture locations

### Linting Standards (TAS-58)
- Microsoft Universal Guidelines + Rust API Guidelines enforced
- Use `#[expect(lint_name, reason = "...")]` instead of `#[allow]`
- All public types must implement `Debug`

### MPSC Channels (TAS-51)
- All channels MUST be bounded (no `unbounded_channel()`)
- All channels configured via TOML, not hard-coded
- See: `docs/development/mpsc-channel-guidelines.md`

---

## Architecture Overview

### Core Pattern
PostgreSQL message queue (PGMQ) system with actor-based coordination. Rust handles orchestration via lightweight actors; workers process steps through queue polling.

### State Machines
- **Task States (12)**: Pending → Initializing → EnqueuingSteps → StepsInProcess → EvaluatingResults → Complete/Error
- **Step States (8)**: Pending → Enqueued → InProgress → Complete/Error
- Details: `docs/architecture/states-and-lifecycles.md`

### Actor Pattern
Four core actors handle orchestration:
1. **TaskRequestActor**: Task initialization
2. **ResultProcessorActor**: Step result processing
3. **StepEnqueuerActor**: Batch step enqueueing
4. **TaskFinalizerActor**: Task completion

Details: `docs/architecture/actors.md`, `tasker-orchestration/AGENTS.md`

### Worker Architecture
Dual-channel system: dispatch channel + completion channel
- `HandlerDispatchService`: Semaphore-bounded parallel execution
- `FfiDispatchChannel`: Pull-based polling for Ruby/Python FFI
- Details: `docs/architecture/worker-event-systems.md`, `tasker-worker/AGENTS.md`

### Deployment Modes
- **PollingOnly**: Traditional polling, higher latency
- **EventDrivenOnly**: PostgreSQL LISTEN/NOTIFY, lowest latency
- **Hybrid** (Recommended): Event-driven with polling fallback

---

## Documentation Index

**Navigation Guide**: For efficient documentation navigation, see `docs/CLAUDE-GUIDE.md` - trigger patterns, document mapping, and investigation patterns.

### Core Architecture
| Topic | Document |
|-------|----------|
| Actor Pattern | `docs/architecture/actors.md` |
| Worker Event Systems | `docs/architecture/worker-event-systems.md` |
| Events & Commands | `docs/architecture/events-and-commands.md` |
| Crate Structure | `docs/architecture/crate-architecture.md` |
| State Machines | `docs/architecture/states-and-lifecycles.md` |
| SQL Functions | `docs/reference/task-and-step-readiness-and-execution.md` |

### Workflow Patterns
| Topic | Document |
|-------|----------|
| Batch Processing | `docs/guides/batch-processing.md` |
| Conditional Workflows | `docs/guides/conditional-workflows.md` |
| Dead Letter Queue | `docs/guides/dlq-system.md` |
| Retry Semantics | `docs/guides/retry-semantics.md` |

### Infrastructure
| Topic | Document |
|-------|----------|
| Configuration | `docs/guides/configuration-management.md` |
| Circuit Breakers | `docs/architecture/circuit-breakers.md` |
| Backpressure | `docs/architecture/backpressure-architecture.md` |
| MPSC Channels | `docs/development/mpsc-channel-guidelines.md` |
| Observability | `docs/observability/README.md` |

### Development Tooling
| Topic | Document |
|-------|----------|
| Build System (cargo-make) | `docs/development/tooling.md` |
| Development Patterns | `docs/development/development-patterns.md` |
| FFI Callback Safety | `docs/development/ffi-callback-safety.md` |

### Operations
| Topic | Document |
|-------|----------|
| Deployment | `docs/architecture/deployment-patterns.md` |
| Channel Tuning | `docs/operations/mpsc-channel-tuning.md` |
| Backpressure Monitoring | `docs/operations/backpressure-monitoring.md` |

### Ticket Specs (Historical)
Detailed feature specifications: `docs/ticket-specs/TAS-{37,39,40,41,46,49,51,53,54,58,59,67,69,75}/`

---

## Troubleshooting

### Database Issues
- **Connection errors**: `pg_isready`, check DATABASE_URL format
- **PGMQ errors**: `psql $DATABASE_URL -c "SELECT * FROM pgmq.meta"`
- **Migration issues**: `cargo sqlx migrate run`

### Test Failures
- Run with `--nocapture` for full output
- Ensure `--all-features` flag used
- Check DATABASE_URL is exported (not SQLX_OFFLINE)

### Ruby Extension
- Clean rebuild: `cd workers/ruby && rake clean && rake compile`
- Check magnus version in Cargo.toml

### Configuration
- Validate: `TASKER_ENV=test cargo run --bin config-validator`
- Check environment overrides in `config/tasker/environments/`

### cargo-make Issues
- **Task not found**: Check crate's `Makefile.toml` extends base-tasks correctly
- **Script path errors**: Ensure `SCRIPTS_DIR` uses relative path (`cargo-make/scripts`)
- **Extend warning**: `extend = "path"` must be at file root, NOT inside `[config]`
- See: `docs/development/tooling.md` for full troubleshooting

---

## Performance Targets

- 10-100x faster dependency resolution vs PostgreSQL functions
- <1ms overhead per step coordination
- >10k events/sec cross-language processing
- Zero race conditions via atomic claiming

---

## Crate-Specific Context

For detailed module organization and development patterns:
- `tasker-orchestration/AGENTS.md` - Actor pattern, service decomposition
- `tasker-worker/AGENTS.md` - Handler dispatch, FFI integration

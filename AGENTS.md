# CLAUDE.md - Tasker Core Project Context

**Project Status**: Production Ready (v0.1.0) | 1185+ tests | Rust orchestration with PostgreSQL (PGMQ or RabbitMQ messaging)

---

## Quick Reference

### Essential Commands

**Preferred: cargo-make (unified task runner)**
```bash
cargo make check       # All quality checks across workspace
cargo make test        # All tests (requires services running)
cargo make fix         # Auto-fix all fixable issues
cargo make build       # Build everything

# Shortcuts
cargo make c           # check
cargo make t           # test (requires services)
cargo make f           # fix
cargo make b           # build

# Test levels (TAS-73 feature-gated hierarchy)
cargo make test-rust-unit     # tu - Unit tests (DB + messaging only)
cargo make test-rust-e2e      # te - E2E tests (requires services)
cargo make test-rust-cluster  # tc - Cluster tests (requires: cluster-start)
cargo make test-rust-all      # All tests including cluster

# Language-specific
cargo make check-rust
cargo make check-python
cargo make check-ruby
cargo make check-typescript
```

**Direct cargo commands (when needed)**
```bash
# Build (ALWAYS use --all-features for builds)
cargo build --all-features
cargo clippy --all-targets --all-features
cargo fmt

# Fast compilation check
cargo check --all-features

# Tests use feature flags for infrastructure levels:
#   --features test-messaging  → DB + messaging (unit/integration tests)
#   --features test-services   → + services running (E2E tests)
#   --features test-cluster    → + multi-instance cluster (local only)
#   --all-features             → everything including cluster tests
cargo test --features test-services              # Standard E2E tests
cargo test --features test-messaging --lib       # Library tests only

# Run specific package/test
cargo test --features test-services --package <package-name>
cargo test --features test-services <test-name>

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

### gRPC Testing (TAS-177)
```bash
# cargo-make tasks
cargo make test-grpc             # tg  - All gRPC tests
cargo make test-grpc-health      # gRPC health endpoint tests
cargo make test-grpc-parity      # tgp - REST/gRPC response parity tests
cargo make test-e2e-grpc         # tge - E2E tests with gRPC transport
cargo make test-both-transports  # E2E with both REST and gRPC

# grpcurl examples (requires services running)
grpcurl -plaintext localhost:9190 list                           # List services
grpcurl -plaintext localhost:9190 tasker.v1.HealthService/CheckLiveness
grpcurl -plaintext -H "X-API-Key: test-api-key-full-access" \
  -d '{"name":"test","namespace":"default","version":"1.0.0"}' \
  localhost:9190 tasker.v1.TaskService/CreateTask
```

**Port Allocation (gRPC)**:
| Service | REST Port | gRPC Port |
|---------|-----------|-----------|
| Orchestration | 8080 | 9190 |
| Rust Worker | 8081 | 9191 |
| Ruby Worker | 8082 | 9200 |
| Python Worker | 8083 | 9300 |
| TypeScript Worker | 8085 | 9400 |

### Container Operations (Podman)
The project uses Podman as the container runtime. Docker CLI commands work via shell aliases (`docker` → `podman`, `docker-compose` → `podman compose`).

```bash
docker-compose up -d postgres                           # Start PostgreSQL (includes PGMQ)
docker-compose --profile server up -d                   # Full server
docker compose -f docker/docker-compose.test.yml up -d  # Test services (includes RabbitMQ)

# Direct podman commands (equivalent)
podman compose up -d postgres
podman compose -f docker/docker-compose.test.yml up -d
```

---

## Workspace Structure

```
[workspace.members]
- tasker-pgmq          # PGMQ wrapper with notification support
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
- **Never remove assertions to fix compilation or test failures** - this hides problems rather than solving them. If a test assertion requires a dependency (e.g., `bigdecimal`), add the dependency. If an assertion is failing, fix the underlying issue. Removing test coverage to make things "pass" is unacceptable.

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
PostgreSQL-backed orchestration with provider-agnostic messaging (PGMQ default, RabbitMQ optional). Rust handles orchestration via lightweight actors; workers process steps via push notifications or polling.

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
| Identity Strategy | `docs/guides/identity-strategy.md` |

### Infrastructure
| Topic | Document |
|-------|----------|
| Caching | `docs/guides/caching.md` |
| Configuration | `docs/guides/configuration-management.md` |
| Circuit Breakers | `docs/architecture/circuit-breakers.md` |
| Backpressure | `docs/architecture/backpressure-architecture.md` |
| MPSC Channels | `docs/development/mpsc-channel-guidelines.md` |
| Observability | `docs/observability/README.md` |

### Authentication & Security
| Topic | Document |
|-------|----------|
| Auth Overview | `docs/auth/README.md` |
| Permissions & Routes | `docs/auth/permissions.md` |
| Configuration Reference | `docs/auth/configuration.md` |
| Auth Testing | `docs/auth/testing.md` |
| API Security Guide | `docs/guides/api-security.md` |
| External Providers (JWKS) | `docs/guides/auth-integration.md` |

### Development Tooling
| Topic | Document |
|-------|----------|
| Build System (cargo-make) | `docs/development/tooling.md` |
| Code Coverage | `docs/development/coverage-tooling.md` |
| Development Patterns | `docs/development/development-patterns.md` |
| FFI Callback Safety | `docs/development/ffi-callback-safety.md` |

### Operations
| Topic | Document |
|-------|----------|
| Deployment | `docs/architecture/deployment-patterns.md` |
| Channel Tuning | `docs/operations/mpsc-channel-tuning.md` |
| Backpressure Monitoring | `docs/operations/backpressure-monitoring.md` |

### Testing
| Topic | Document |
|-------|----------|
| Cluster Testing | `docs/testing/cluster-testing-guide.md` |
| Lifecycle Testing | `docs/testing/comprehensive-lifecycle-testing-guide.md` |
| Decision Point Tests | `docs/testing/decision-point-e2e-tests.md` |

### Ticket Specs (Historical)
Detailed feature specifications: `docs/ticket-specs/TAS-{37,39,40,41,46,49,51,53,54,58,59,67,69,73,75}/`

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
- For cluster tests: Ensure cluster is running (`cargo make cluster-start-all`)
- See: `docs/testing/cluster-testing-guide.md` for cluster test troubleshooting

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

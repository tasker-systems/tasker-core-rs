# Environment Configuration

This directory contains modular environment files that are assembled into `.env` files for different services and modes.

## Quick Start

```bash
# Generate .env for standard test mode
cargo make setup-env

# Generate .env for split database mode (TAS-78)
cargo make setup-env-split

# Generate .env files for all services
cargo make setup-env-all
```

## File Structure

| File | Purpose |
|------|---------|
| `base.env` | Common variables: logging, paths, TASKER_ENV |
| `test.env` | Test environment: DATABASE_URL (port 5432), JWT keys, web config |
| `test-split.env` | Split DB overrides: DATABASE_URL (port 5433), PGMQ_DATABASE_URL |
| `orchestration.env` | Orchestration service: port 8080, config path |
| `rust-worker.env` | Rust worker: port 8081, template path |
| `ruby-worker.env` | Ruby worker: port 8082, handler path |
| `python-worker.env` | Python worker: port 8083, handler path, PYTHONPATH |
| `typescript-worker.env` | TypeScript worker: port 8085, FFI paths |

## Assembly

The `setup-env.sh` script assembles files in this order:

1. `base.env` - Always included
2. `test.env` - For test modes
3. `test-split.env` - Only for split database mode
4. `<target>.env` - Target-specific (orchestration, rust-worker, etc.)

## Available Tasks

| Task | Output | Description |
|------|--------|-------------|
| `setup-env` | `.env` | Root env for standard tests |
| `setup-env-split` | `.env` | Root env for split DB tests |
| `setup-env-orchestration` | `tasker-orchestration/.env` | Orchestration service |
| `setup-env-rust-worker` | `workers/rust/.env` | Rust worker |
| `setup-env-ruby-worker` | `workers/ruby/.env` | Ruby worker |
| `setup-env-python-worker` | `workers/python/.env` | Python worker |
| `setup-env-typescript-worker` | `workers/typescript/.env` | TypeScript worker |
| `setup-env-all` | All above | Generate all env files |
| `setup-env-all-split` | All above | Generate all for split mode |

## Customization

To customize for your system, update `WORKSPACE_PATH` in `base.env`:

```bash
# In base.env
WORKSPACE_PATH=/your/path/to/tasker-core
```

The script expands `${WORKSPACE_PATH}` in all other paths automatically.

## Split Database Mode (TAS-78)

For testing PGMQ on a separate database:

```bash
# Start dual PostgreSQL
docker compose -f docker/docker-compose.dual-pg.test.yml up -d

# Generate split-mode env files
cargo make setup-env-all-split

# Run migrations
cargo make db-migrate-split

# Start services
cargo make services-start
```

Split mode sets:
- `DATABASE_URL` → port 5433, `tasker_split_test` database
- `PGMQ_DATABASE_URL` → port 5433, `pgmq_split_test` database
- `SQLX_OFFLINE=true` → Required for builds

## Legacy Files

The old `.env.test` files in worker directories are superseded by this system.
They can be removed once this approach is validated.

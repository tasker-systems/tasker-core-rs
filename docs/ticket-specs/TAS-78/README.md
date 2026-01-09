# TAS-78: PGMQ Separate Database Support

## Executive Summary

Enable PGMQ to be deployed and managed on a separate database from the main Tasker application. This foundational infrastructure change supports independent scaling, maintenance, and resource allocation for the message queue layer while maintaining backward compatibility for single-database deployments.

## Problem Statement

The current architecture assumes PGMQ is available at the same `DATABASE_URL` as the Tasker application. While this simplifies development and small deployments, it creates several operational challenges:

1. **Scaling Constraints**: PGMQ workloads (high-frequency queue operations) have different scaling characteristics than Tasker workloads (transactional task/step state management)

2. **Maintenance Windows**: PGMQ may need independent maintenance (partitioning via pg_partman, vacuuming high-churn queue tables) without affecting Tasker availability

3. **Resource Isolation**: Queue operations can compete with task orchestration for connection pool resources

4. **Operational Flexibility**: Organizations may want to use managed PostgreSQL for Tasker but self-hosted for PGMQ (or vice versa)

5. **Circuit Breaker Independence**: A PGMQ outage should degrade messaging gracefully without affecting Tasker database operations

## Relationship to Other Tickets

| Ticket | Relationship | Coordination Notes |
|--------|-------------|-------------------|
| **TAS-128** | Schema cleanup & pg18 migration | Migration flattening should account for the split. PGMQ-specific objects can move to a separate file |
| **TAS-133** | Messaging service strategy pattern | The strategy pattern abstraction should work with both single-DB and split-DB PGMQ configurations |
| **TAS-49** | DLQ system | DLQ SQL objects reference both Tasker tables and PGMQ functions - needs careful handling |

## Current State Analysis

### Database Configuration

**Config Location**: `config/tasker/base/common.toml`

```toml
[common.database]
url = "${DATABASE_URL:-postgresql://localhost/tasker}"
database = "tasker_development"

[common.database.pool]
max_connections = 25
min_connections = 5
```

**Environment Variables**:
- `DATABASE_URL` - Single URL for all database operations

### Migration File Analysis

| Migration | Primary Owner | PGMQ Dependency | Separation Strategy |
|-----------|---------------|-----------------|---------------------|
| `20250810140000_uuid_v7_initial_schema.sql` | **Mixed** | Creates `pgmq` extension, headers column triggers | Split into two files |
| `20250826180921_add_pgmq_notifications.sql` | **PGMQ** | 100% PGMQ wrapper functions | Move entirely to PGMQ migrations |
| `20250912000000_tas41_richer_task_states.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20250927000000_add_waiting_for_retry_state.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20251001000000_fix_permanently_blocked_detection.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20251006000000_fix_retry_eligibility_logic.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20251007000000_add_correlation_ids.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20251010000000_add_stale_task_discovery_fixes.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20251115000000_comprehensive_dlq_system.sql` | **Tasker** | References `pgmq.metrics()` | Add fallback when PGMQ unavailable |
| `20251118000000_add_unique_edge_constraint.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20251203000000_add_step_result_audit.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20251207000000_fix_execution_status_priority.sql` | **Tasker** | None | Keep in Tasker migrations |
| `20260102000000_add_checkpoint_column.sql` | **Tasker** | None | Keep in Tasker migrations |

### Pool Initialization Analysis

**pgmq-notify crate** (`pgmq-notify/src/client.rs`):
```rust
impl PgmqClient {
    pub async fn new(database_url: &str) -> Result<Self> {
        Self::new_with_config(database_url, PgmqNotifyConfig::default()).await
    }

    pub async fn new_with_pool(pool: sqlx::PgPool) -> Self {
        // BYOP - Bring Your Own Pool
    }
}
```

**Current Usage Pattern**: Creates pool from same `DATABASE_URL` as main application.

### SQL Function Dependencies

**Functions on PGMQ database** (required for messaging):
- `pgmq_send_with_notify()` - Atomic message send + notification
- `pgmq_send_batch_with_notify()` - Batch version
- `pgmq_read_specific_message()` - Event-driven message retrieval
- `pgmq_delete_specific_message()` - Message cleanup
- `pgmq_extend_vt_specific_message()` - VT extension
- `extract_queue_namespace()` - Namespace routing helper
- `pgmq_notify_queue_created()` - Queue creation trigger
- `pgmq_ensure_headers_column()` - Headers column helper

**Functions that reference BOTH databases** (problematic):
- None currently - clean separation is possible

## Solution Design

### Configuration Schema

#### New TOML Structure

```toml
# config/tasker/base/common.toml

[common.database]
url = "${DATABASE_URL:-postgresql://localhost/tasker}"
database = "tasker_development"
skip_migration_check = false

[common.database.pool]
max_connections = 25
min_connections = 5
acquire_timeout_seconds = 10
idle_timeout_seconds = 300
max_lifetime_seconds = 1800

# NEW: Separate PGMQ database configuration
[common.pgmq_database]
# When url is empty/not set, falls back to common.database.url
# This maintains backward compatibility
url = "${PGMQ_DATABASE_URL:-}"
enabled = true
skip_migration_check = false

[common.pgmq_database.pool]
max_connections = 15
min_connections = 3
acquire_timeout_seconds = 5
idle_timeout_seconds = 300
max_lifetime_seconds = 1800
```

#### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `DATABASE_URL` | Main Tasker database | Required |
| `PGMQ_DATABASE_URL` | PGMQ database (optional) | Falls back to `DATABASE_URL` |
| `PGMQ_ENABLED` | Enable PGMQ messaging | `true` |

### Migration Separation

#### New Directory Structure

```
migrations/
├── tasker/                              # Tasker application migrations
│   ├── 20250810140000_initial_schema.sql
│   ├── 20250912000000_tas41_richer_task_states.sql
│   ├── ... (all Tasker-specific migrations)
│   └── README.md
│
├── pgmq/                                # PGMQ-specific migrations
│   ├── 20250810140001_pgmq_extensions.sql
│   ├── 20250826180921_notification_functions.sql
│   └── README.md
│
└── backup/                              # Historical reference
    └── ... (existing backup files)
```

#### Tasker Initial Schema (Split)

```sql
-- migrations/tasker/20250810140000_initial_schema.sql
-- 
-- Tasker Core Schema
-- Does NOT include PGMQ extension or queue-related objects

CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;

-- All tasker_* tables
-- All tasker views
-- All tasker SQL functions (get_task_execution_context, etc.)
```

#### PGMQ Initial Schema (Split)

```sql
-- migrations/pgmq/20250810140001_pgmq_extensions.sql
--
-- PGMQ Schema and Extensions
-- Applied to PGMQ database (may be same as Tasker or separate)

CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;
CREATE EXTENSION IF NOT EXISTS pg_uuidv7 CASCADE;

-- PGMQ headers column support
-- Queue creation triggers
```

### Pool Architecture

#### Rust Types

```rust
// tasker-shared/src/database/pools.rs

use sqlx::PgPool;

/// Holds connection pools for both databases
/// When deployed with single database, both pools point to same DB
#[derive(Clone)]
pub struct DatabasePools {
    /// Main Tasker database pool (tasks, steps, transitions)
    pub tasker: PgPool,
    
    /// PGMQ database pool (queue operations)
    /// May be same as tasker pool in single-DB deployments
    pub pgmq: PgPool,
    
    /// Whether PGMQ is on a separate database
    pub pgmq_is_separate: bool,
}

impl DatabasePools {
    /// Create pools from configuration
    pub async fn from_config(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        let tasker = create_pool(&config.database).await?;
        
        let (pgmq, pgmq_is_separate) = if let Some(pgmq_config) = &config.pgmq_database {
            if pgmq_config.is_separate() {
                (create_pool(pgmq_config).await?, true)
            } else {
                (tasker.clone(), false)
            }
        } else {
            (tasker.clone(), false)
        };
        
        Ok(Self {
            tasker,
            pgmq,
            pgmq_is_separate,
        })
    }
    
    /// Get pool for a specific operation type
    pub fn for_operation(&self, op: DatabaseOperation) -> &PgPool {
        match op {
            DatabaseOperation::TaskQuery => &self.tasker,
            DatabaseOperation::StepQuery => &self.tasker,
            DatabaseOperation::TransitionInsert => &self.tasker,
            DatabaseOperation::QueueSend => &self.pgmq,
            DatabaseOperation::QueueRead => &self.pgmq,
            DatabaseOperation::QueueDelete => &self.pgmq,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DatabaseOperation {
    TaskQuery,
    StepQuery,
    TransitionInsert,
    QueueSend,
    QueueRead,
    QueueDelete,
}
```

### pgmq-notify Integration

Update the `PgmqClient` to accept separate pool:

```rust
// pgmq-notify/src/client.rs

impl PgmqClient {
    /// Create client with dedicated PGMQ pool
    /// Pool should be connected to database with pgmq extension
    pub async fn new_with_dedicated_pool(pgmq_pool: PgPool) -> Self {
        Self::new_with_pool_and_config(pgmq_pool, PgmqNotifyConfig::default()).await
    }
}
```

### SQLx Considerations

#### Compile-Time Query Verification

SQLx's compile-time verification requires database connection. With split databases:

1. **Development**: Use single-database mode (both pools point to same DB)
2. **CI**: Run `cargo sqlx prepare` against combined schema
3. **Production**: Runtime pools connect to appropriate databases

#### sqlx-data.json Management

The `.sqlx` directory contains cached query metadata. For split databases:

```bash
# Generate cache against development database (has all schemas)
cargo sqlx prepare --workspace

# Verify cache is valid
cargo sqlx prepare --check --workspace
```

### Circuit Breaker Integration

Update circuit breaker configuration to support independent PGMQ monitoring:

```toml
[common.circuit_breakers.component_configs.pgmq]
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2
# NEW: Separate health check endpoint
health_check_enabled = true
health_check_interval_seconds = 10
```

## Implementation Plan

### Phase 1: Configuration Foundation (2-3 hours)

1. Add `[common.pgmq_database]` section to TOML schema
2. Update `DatabaseConfig` struct in `tasker-shared/src/config/`
3. Add `PGMQ_DATABASE_URL` environment variable support
4. Implement fallback logic (empty URL → use main database)
5. Update configuration documentation

**Files Modified**:
- `tasker-shared/src/config/database.rs`
- `config/tasker/base/common.toml`
- `.env.template`

### Phase 2: Pool Architecture (3-4 hours)

1. Create `DatabasePools` struct with dual-pool support
2. Update `SystemContext` to hold `DatabasePools`
3. Implement `DatabaseOperation` enum for pool selection
4. Update `PgmqClient::new_with_pool()` calls throughout codebase

**Files Modified**:
- `tasker-shared/src/database/mod.rs` (new: `pools.rs`)
- `tasker-shared/src/system_context.rs`
- `tasker-orchestration/src/services/` (various)
- `tasker-worker/src/services/` (various)

### Phase 3: Migration Separation (2-3 hours)

1. Create `migrations/tasker/` and `migrations/pgmq/` directories
2. Split `20250810140000_uuid_v7_initial_schema.sql`
3. Move `20250826180921_add_pgmq_notifications.sql` to pgmq/
4. Update `Migrator` to support multiple migration paths
5. Add migration path selection based on configuration

**Files Modified**:
- `migrations/` (directory restructure)
- `tasker-shared/src/database/migrator.rs`

### Phase 4: Initialization Updates (2-3 hours)

1. Update orchestration bootstrap to create `DatabasePools`
2. Update worker bootstrap similarly
3. Pass correct pool to `PgmqClient` initialization
4. Update health check endpoints to report both pool statuses

**Files Modified**:
- `tasker-orchestration/src/bootstrap.rs`
- `tasker-worker/src/bootstrap.rs`
- `tasker-orchestration/src/web/handlers/health.rs`
- `tasker-worker/src/web/handlers/health.rs`

### Phase 5: Testing & Validation (3-4 hours)

1. Add integration tests for split-database configuration
2. Test fallback to single-database when `PGMQ_DATABASE_URL` not set
3. Test circuit breaker independence
4. Verify SQLx cache generation works
5. Update CI to test both configurations

**Files Modified**:
- `tests/integration/database_pools_test.rs` (new)
- `.github/workflows/test.yml`

### Phase 6: Documentation (1-2 hours)

1. Update deployment patterns documentation
2. Add PGMQ separation section to architecture docs
3. Update environment variable documentation
4. Add operational runbook for split-database deployments

**Files Modified**:
- `docs/architecture/deployment-patterns.md`
- `docs/operations/database-configuration.md` (new)
- `README.md`

## Estimated Total Effort

| Phase | Effort | Dependencies |
|-------|--------|--------------|
| Phase 1: Configuration | 2-3 hours | None |
| Phase 2: Pool Architecture | 3-4 hours | Phase 1 |
| Phase 3: Migration Separation | 2-3 hours | None (parallel) |
| Phase 4: Initialization | 2-3 hours | Phases 1, 2 |
| Phase 5: Testing | 3-4 hours | Phases 1-4 |
| Phase 6: Documentation | 1-2 hours | Phases 1-5 |
| **Total** | **13-19 hours** | |

## Backward Compatibility

This change is **fully backward compatible**:

1. **Single-database deployments**: When `PGMQ_DATABASE_URL` is not set or empty, both pools point to `DATABASE_URL`
2. **Existing configuration files**: No changes required; new sections are additive
3. **Existing code**: `DatabasePools.pgmq` returns appropriate pool regardless of configuration
4. **SQLx queries**: All queries continue to work; runtime pool selection handles routing

## Success Criteria

1. ✅ System functions identically when `PGMQ_DATABASE_URL` is not set (backward compatible)
2. ✅ System functions with separate PGMQ database when `PGMQ_DATABASE_URL` is set
3. ✅ Health endpoints report status of both database connections
4. ✅ Circuit breaker for PGMQ operates independently from Tasker database
5. ✅ Migrations can be applied independently to each database
6. ✅ SQLx compile-time verification continues to work
7. ✅ All existing tests pass
8. ✅ New integration tests cover split-database scenarios
9. ✅ Documentation updated for operational guidance

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Cross-database transactions impossible | Medium | Design already avoids cross-DB transactions; PGMQ operations are independent |
| Increased operational complexity | Low | Fallback to single-DB maintains simplicity for small deployments |
| SQLx cache invalidation | Medium | CI validates cache; development uses single-DB mode |
| Connection count doubling | Low | PGMQ pool can be smaller; total connections configurable |
| Health check complexity | Low | Independent health checks per pool with aggregated status |

## Future Considerations

1. **TAS-133 Integration**: The messaging service strategy pattern should receive appropriate pool based on provider (PGMQ gets PGMQ pool, RabbitMQ doesn't need it)

2. **Read Replicas**: This pattern could extend to support read replicas for heavy query workloads

3. **Connection Poolers**: External poolers (PgBouncer) may be inserted between Tasker and either database

4. **Multi-tenancy**: Separate PGMQ databases per tenant could be supported with this foundation

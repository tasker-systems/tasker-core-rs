# TAS-133f: Migration & Cleanup

**Parent**: TAS-133 (Messaging Service Strategy Pattern Abstraction)
**Status**: Ready for Implementation
**Branch**: `jcoletaylor/tas-133f-migration-cleanup`
**Merges To**: `jcoletaylor/tas-133-messaging-service-strategy-pattern-abstraction`
**Depends On**: TAS-133e

---

## Overview

Final cleanup phase: remove all legacy messaging abstractions, ensure no deprecated patterns remain, and verify the codebase is clean. This phase makes the migration irreversible.

## Validation Gate

- [ ] All old traits/structs deleted (see deletion list)
- [ ] No `UnifiedPgmqClient`, `UnifiedMessageClient`, old `MessageClient` trait references
- [ ] No `as_pgmq()` downcast pattern in codebase
- [ ] No `SimpleStepMessage` references (renamed to `StepMessage`)
- [ ] Full test suite passes: `cargo test --all-features`
- [ ] Clippy clean: `cargo clippy --all-targets --all-features`
- [ ] Documentation updated to reflect new architecture

## Scope

### Files to Delete

| File | Contains |
|------|----------|
| `tasker-shared/src/messaging/clients/traits.rs` | Old `MessageClient` trait, `PgmqClientTrait` |
| `tasker-shared/src/messaging/clients/unified_client.rs` | `UnifiedMessageClient` enum |
| `tasker-shared/src/messaging/clients/mod.rs` | Old clients module (if empty after deletions) |

### Code Patterns to Remove

#### 1. Downcast Pattern

```rust
// DELETE this pattern wherever found
let pgmq_client = self.message_client.as_pgmq()
    .ok_or_else(|| TaskerError::MessagingError("Expected PGMQ client".to_string()))?;
pgmq_client.read_messages(queue, timeout, batch).await?
```

#### 2. UnifiedPgmqClient References

```rust
// DELETE: UnifiedPgmqClient wrapper
pub struct UnifiedPgmqClient {
    client: PgmqClient,
}

// DELETE: Construction of UnifiedPgmqClient
let message_client = Arc::new(UnifiedPgmqClient::new_standard(pgmq_client));
```

#### 3. Old MessageClient Trait Usage

```rust
// DELETE trait definition
pub trait MessageClient: Send + Sync {
    async fn send_step_message(&self, ...) -> TaskerResult<()>;
    // ...
}

// DELETE impl blocks for old trait
impl MessageClient for PgmqClient { ... }
```

#### 4. SimpleStepMessage References

```rust
// VERIFY: No references to SimpleStepMessage remain
// Should have been renamed to StepMessage in TAS-133c
grep -r "SimpleStepMessage" --include="*.rs"  # Should return nothing
```

### SystemContext Cleanup

Remove deprecated field:

```rust
// BEFORE (TAS-133e compatibility layer)
pub struct SystemContext {
    pub messaging_provider: Arc<MessagingProvider>,
    pub message_client: Arc<MessageClient>,

    #[deprecated]
    pub old_message_client: Arc<UnifiedPgmqClient>,  // DELETE this
}

// AFTER
pub struct SystemContext {
    pub messaging_provider: Arc<MessagingProvider>,
    pub message_client: Arc<MessageClient>,
    // No deprecated fields
}
```

### Module Structure Cleanup

**Before cleanup:**
```
tasker-shared/src/messaging/
├── mod.rs
├── clients/           # DELETE entire directory
│   ├── mod.rs
│   ├── traits.rs      # Old MessageClient, PgmqClientTrait
│   ├── unified_client.rs
│   └── in_memory_client.rs  # KEEP if not migrated
├── service/           # NEW (from TAS-133a)
│   ├── mod.rs
│   ├── traits.rs
│   ├── types.rs
│   ├── errors.rs
│   ├── router.rs
│   ├── provider.rs
│   ├── pgmq.rs
│   ├── rabbitmq.rs
│   └── in_memory.rs
├── client.rs          # NEW MessageClient struct
└── message.rs         # StepMessage, etc.
```

**After cleanup:**
```
tasker-shared/src/messaging/
├── mod.rs
├── service/
│   ├── mod.rs
│   ├── traits.rs
│   ├── types.rs
│   ├── errors.rs
│   ├── router.rs
│   ├── provider.rs
│   ├── pgmq.rs
│   ├── rabbitmq.rs
│   └── in_memory.rs
├── client.rs
└── message.rs
```

### Documentation Updates

Update these docs to reflect new architecture:

| Document | Update |
|----------|--------|
| `docs/architecture/crate-architecture.md` | Messaging layer description |
| `tasker-shared/README.md` | Messaging module docs |
| `CLAUDE.md` | Any messaging-related guidance |
| `docs/ticket-specs/TAS-133/README.md` | Mark as complete |

## Verification Commands

### Check for Old Patterns

```bash
# No UnifiedPgmqClient references
grep -r "UnifiedPgmqClient" --include="*.rs" tasker-*/src

# No UnifiedMessageClient references
grep -r "UnifiedMessageClient" --include="*.rs" tasker-*/src

# No old MessageClient trait (careful: new struct has same name)
grep -r "impl MessageClient for" --include="*.rs" tasker-*/src

# No SimpleStepMessage
grep -r "SimpleStepMessage" --include="*.rs" tasker-*/src

# No as_pgmq downcast
grep -r "as_pgmq()" --include="*.rs" tasker-*/src

# No PgmqClientTrait
grep -r "PgmqClientTrait" --include="*.rs" tasker-*/src
```

### Full Validation

```bash
# Clean build
cargo clean
cargo build --all-features

# All tests
cargo test --all-features

# Clippy
cargo clippy --all-targets --all-features -- -D warnings

# Check for dead code
cargo clippy --all-targets --all-features -- -W dead_code
```

## Migration Checklist

- [ ] Delete `tasker-shared/src/messaging/clients/` directory
- [ ] Update `tasker-shared/src/messaging/mod.rs` to remove clients module
- [ ] Remove deprecated `old_message_client` from SystemContext
- [ ] Remove any `#[allow(deprecated)]` attributes added for migration
- [ ] Update re-exports in `tasker-shared/src/lib.rs`
- [ ] Run all verification commands
- [ ] Update architecture documentation
- [ ] Final review of all `*.rs` files in messaging module

## Testing Strategy

### Regression Testing

Full test suite must pass:

```bash
cargo test --all-features
cargo test --all-features --package tasker-orchestration
cargo test --all-features --package tasker-worker
```

### Integration Testing

Run E2E tests to ensure nothing broke:

```bash
# Start test services
docker compose -f docker/docker-compose.test.yml up -d

# Run integration tests
cargo test --all-features integration

# Run with RabbitMQ provider
TASKER_MESSAGING_PROVIDER=rabbitmq cargo test --all-features integration
```

### Clippy Compliance

```bash
# Must pass without warnings
cargo clippy --all-targets --all-features -- -D warnings

# Check for unused code
cargo clippy --all-targets --all-features -- -W unused
```

## Post-Completion

After this phase:

1. **Merge TAS-133 → main** - Feature branch is complete
2. **Update Linear** - Close TAS-133 and all child tickets
3. **Announce** - Team notification of new messaging architecture
4. **Monitor** - Watch for any issues in staging/production

## Dependencies

- TAS-133e (all new code must be integrated and working)
- All tests passing before deletion begins

## Estimated Complexity

**Low-Medium** - Mostly deletion and verification. Risk is breaking something by removing code still in use. Careful verification mitigates this.

---

## References

- [implementation-plan.md](../implementation-plan.md) - Phase 6 section
- All previous phase documents for context

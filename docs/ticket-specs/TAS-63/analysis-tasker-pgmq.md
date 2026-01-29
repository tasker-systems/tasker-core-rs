# Coverage Analysis: tasker-pgmq

**Current Coverage**: 48.54% line, 35.32% function
**Target**: 65%
**Gap**: 16.46 percentage points (line), 29.68 percentage points (function)

---

## Summary

The `tasker-pgmq` crate is a generic PostgreSQL LISTEN/NOTIFY integration layer for PGMQ queues, providing event-driven notification capabilities including a PGMQ client, notification listener, event emitters, and a CLI migration generator. Coverage sits at 48.54% line / 35.32% function -- well below the 65% target. The largest gaps are in `listener.rs` (10.32% line coverage, the most code-heavy module), `client.rs` (34.17%), and `events.rs` (38.68%). These modules contain the core runtime logic -- database-connected operations, async listener loops, and event type methods -- that are difficult to test without a live PostgreSQL + PGMQ instance. Reaching the 65% target will require approximately 249 additional lines covered, achievable primarily through unit tests for pure logic and integration tests for database-dependent code.

## File Coverage Overview

| File | Lines Covered | Lines Total | Line % | Functions Covered | Functions Total | Function % |
|------|--------------|-------------|--------|-------------------|-----------------|------------|
| `types.rs` | 0 | 12 | 0.00% | 0 | 4 | 0.00% |
| `listener.rs` | 26 | 252 | 10.32% | 5 | 64 | 7.81% |
| `error.rs` | 5 | 20 | 25.00% | 1 | 4 | 25.00% |
| `client.rs` | 82 | 240 | 34.17% | 14 | 68 | 20.59% |
| `events.rs` | 94 | 243 | 38.68% | 13 | 31 | 41.94% |
| `emitter.rs` | 54 | 131 | 41.22% | 12 | 26 | 46.15% |
| `channel_metrics.rs` | 146 | 209 | 69.86% | 20 | 25 | 80.00% |
| `config.rs` | 104 | 129 | 80.62% | 19 | 22 | 86.36% |
| `bin/cli.rs` | 223 | 276 | 80.80% | 5 | 8 | 62.50% |

## Uncovered Files (0% Coverage)

| File | Lines | Description | Why Uncovered |
|------|-------|-------------|---------------|
| `types.rs` | 12 | `MessagingError` enum `From` impls (4 conversion functions from `String`, `&str`, `anyhow::Error`, `Box<dyn Error>`) and `QueueMetrics`/`ClientStatus` structs | These `From` impls are never triggered in tests because existing tests use the `PgmqNotifyError` type rather than `MessagingError`. The struct types are exercised indirectly but their derive-generated code is not counted. |

## Lowest Coverage Files

| File | Line % | Lines Uncovered | Primary Gap Reason |
|------|--------|-----------------|-------------------|
| `listener.rs` | 10.32% | 226 | All async listener functionality requires a live PgListener connection. The module contains `PgmqNotifyListener` with connect/disconnect/listen lifecycle, two listening loop variants (`listen_with_handler`, `start_listening_with_handler`), and a simple queue-based listener (`start_listening`). Only 3 unit tests exist covering `ListenerStats::default()`, channel name building, and `MockEventHandler` setup -- none exercise the actual listener. |
| `error.rs` | 25.00% | 15 | Error factory methods (`config`, `invalid_channel`, `invalid_pattern`, `already_listening`) are never directly tested. Only `PgmqNotifyError::config` is exercised indirectly through config validation. |
| `client.rs` | 34.17% | 158 | All queue operations (send, read, pop, delete, archive, purge, drop, metrics, health_check, set_visibility_timeout, send_with_transaction) require a live PGMQ database. The `PgmqClientFactory` methods are also uncovered. Only `namespace_extraction` and basic client creation are tested. |
| `events.rs` | 38.68% | 149 | Builder methods and common accessor methods on `PgmqNotifyEvent` enum variants are partially tested. Missing coverage for: `MessageWithPayloadEvent` builders, `BatchReadyEvent` builders, `PgmqNotifyEvent::timestamp()`, `metadata()`, `msg_id()`, `has_payload()`, `payload()`, and several `with_timestamp`/`with_delay_seconds`/`with_visibility_timeout` builder methods. |
| `emitter.rs` | 41.22% | 77 | `DbEmitter` requires a live PgPool for all emission methods. `build_payload` logic is tested indirectly for size validation but not through the emitter. `NoopEmitter` is partially tested but `emit_batch_ready`, `emit_message_with_payload`, `emit_event`, and `config()` are not called. `EmitterFactory::database()` requires a pool. |

## Gap Analysis by Priority

### Critical Priority

**`listener.rs` -- 226 uncovered lines (largest module, 10.32% coverage)**

This is the event-driven core of the crate. It contains:
- `PgmqNotifyListener::new()` -- Constructor with bounded MPSC channel (TAS-51 compliance)
- `connect()` / `disconnect()` -- Database connection lifecycle
- `listen_channel()` / `unlisten_channel()` -- LISTEN/UNLISTEN management
- `listen_with_handler()` -- Synchronous listening loop with event parsing
- `start_listening_with_handler()` -- Background tokio task with handler
- `start_listening()` -- Background task with MPSC queue and channel monitoring
- `is_healthy()` / `listening_channels()` / `stats()` -- Accessor methods
- `listen_queue_created()`, `listen_message_ready_for_namespace()`, `listen_message_ready_global()`, `listen_default_namespaces()` -- Convenience channel subscription methods
- `next_event()` -- Blocking event receiver

**Risk**: This module implements the real-time notification path critical to event-driven deployment mode. Regressions here could silently break notification delivery without detection.

**Uncovered functions (59 of 64)**: Nearly all functions including constructor, connection management, all listen variants, event dispatch loops, stats tracking, and health checks.

### High Priority

**`client.rs` -- 158 uncovered lines (34.17% coverage)**

This is the unified PGMQ client providing all queue operations. Uncovered methods include:
- All message operations: `send_json_message`, `send_message_with_delay`, `read_messages`, `pop_message`, `read_specific_message`, `delete_message`, `archive_message`, `set_visibility_timeout`, `send_with_transaction`
- Queue management: `create_queue`, `purge_queue`, `drop_queue`, `queue_metrics`
- Client lifecycle: `new()`, `new_with_config()`, `new_with_pool()`, `new_with_pool_and_config()`
- Helper methods: `process_namespace_queue`, `complete_message`, `initialize_namespace_queues`
- Accessors: `pool()`, `pgmq()`, `config()`, `has_notify_capabilities()`, `health_check()`, `get_client_status()`, `extract_namespace()`, `create_listener()`
- Factory: All `PgmqClientFactory` methods

**Risk**: These are the primary API surface consumers use. Queue send/read operations use SQL wrapper functions (`pgmq_send_with_notify`) that combine message sending with notification emission atomically. Untested code paths could silently lose messages.

**`emitter.rs` -- 77 uncovered lines (41.22% coverage)**

The `DbEmitter` is the application-level notification emitter. Uncovered:
- `DbEmitter::new()`, `config()`, `build_payload()` (the payload size validation + metadata stripping logic)
- `notify_channel()` -- The actual PostgreSQL NOTIFY call
- All `PgmqNotifyEmitter` trait methods on `DbEmitter`: `emit_queue_created`, `emit_message_ready`, `emit_batch_ready`, `emit_message_with_payload`, `emit_event`, `is_healthy`
- `NoopEmitter::emit_batch_ready`, `emit_message_with_payload`, `emit_event`, `config()`
- `EmitterFactory::database()`

**Risk**: `build_payload()` contains payload size validation and metadata stripping logic that protects against exceeding pg_notify limits. The dual-channel emission pattern (namespace + global) is untested.

### Medium Priority

**`events.rs` -- 149 uncovered lines (38.68% coverage)**

Event types and builders. Uncovered:
- `PgmqNotifyEvent` common methods: `timestamp()`, `metadata()`, `msg_id()`, `has_payload()`, `payload()`
- `MessageReadyEvent` builders: `with_timestamp()`, `with_visibility_timeout()`, `add_metadata()`
- `MessageWithPayloadEvent`: All methods (`new()`, `with_timestamp()`, `with_delay_seconds()`)
- `BatchReadyEvent`: All methods (`new()`, `with_timestamp()`, `with_metadata()`, `add_metadata()`, `with_delay_seconds()`)
- `QueueCreatedEvent::with_timestamp()`

**Risk**: These are pure data types with builder patterns -- low risk of runtime failures but important for API correctness. The `OnceLock`-based empty metadata in `metadata()` for `MessageWithPayload` variant is an interesting edge case worth testing.

**`error.rs` -- 15 uncovered lines (25.00% coverage)**

Error factory methods:
- `PgmqNotifyError::config()` (partially covered via config validation)
- `PgmqNotifyError::invalid_channel()`
- `PgmqNotifyError::invalid_pattern()` (covered for one generic, uncovered for `&String`)
- `PgmqNotifyError::already_listening()`

**Risk**: Low -- these are simple constructors. However, ensuring error messages are correct prevents confusing diagnostics.

### Lower Priority

**`types.rs` -- 12 uncovered lines (0% coverage)**

Four `From` trait implementations for `MessagingError`:
- `From<String>` -> `MessagingError::Generic`
- `From<&str>` -> `MessagingError::Generic`
- `From<anyhow::Error>` -> `MessagingError::Generic`
- `From<Box<dyn Error + Send + Sync>>` -> `MessagingError::Generic`

**Risk**: Minimal -- these are simple type conversions. The `MessagingError` type appears to be a compatibility bridge to `tasker-shared`.

**`channel_metrics.rs` -- 63 uncovered lines (69.86% coverage)**

Already near target. Uncovered items:
- `ChannelHealthStatus::from_saturation()`, `is_healthy()`, `saturation_percent()`
- `ChannelMetrics::default()`
- `ChannelMonitor::clone()`, `Debug::fmt()`
- Several accessor methods: `buffer_size()`, `channel_name()`, `component_name()`
- `check_and_warn_saturation()`, `check_health()`

**Risk**: These are already well-tested at 69.86%. Most uncovered items are derive-adjacent or thin wrappers.

**`bin/cli.rs` -- 53 uncovered lines (80.80% coverage)**

CLI binary entry points:
- `main()` function
- `generate_migration()` (file I/O path)
- `generate_migration_sql()` (partially uncovered -- the rollback SQL branch)
- `validate_config()` function
- `CliConfig::default()`

**Risk**: Low -- this is a code generation tool, not runtime code. The SQL generation is already well-tested.

## Recommended Test Plan

### Phase 1: Pure Unit Tests (No Database Required) -- Target +100-120 lines

These tests can be added immediately without infrastructure changes.

**`events.rs` -- Add comprehensive builder and accessor tests:**
- Test all `PgmqNotifyEvent` common methods (`timestamp`, `metadata`, `msg_id`, `has_payload`, `payload`) across all four event variants
- Test `MessageWithPayloadEvent::new()`, `with_timestamp()`, `with_delay_seconds()`
- Test `BatchReadyEvent::new()`, `with_timestamp()`, `with_metadata()`, `add_metadata()`, `with_delay_seconds()`
- Test `MessageReadyEvent::with_timestamp()`, `with_visibility_timeout()`
- Test `QueueCreatedEvent::with_timestamp()`
- Test serialization/deserialization roundtrips for `MessageWithPayload` and `BatchReady` events
- Estimated impact: +80-100 lines covered

**`error.rs` -- Add error factory tests:**
- Test each factory method (`config`, `invalid_channel`, `invalid_pattern`, `already_listening`)
- Verify error display messages
- Estimated impact: +12-15 lines covered

**`types.rs` -- Add From conversion tests:**
- Test `MessagingError::from(String)`, `from(&str)`, `from(anyhow::Error)`, `from(Box<dyn Error>)`
- Estimated impact: +8-12 lines covered

**`emitter.rs` -- Expand NoopEmitter tests:**
- Test `NoopEmitter::config()` accessor
- Test `emit_batch_ready()`, `emit_message_with_payload()`, `emit_event()` with NoopEmitter
- Estimated impact: +10-15 lines covered

**`channel_metrics.rs` -- Fill remaining gaps:**
- Test `ChannelHealthStatus::saturation_percent()` for all variants
- Test `ChannelMetrics::default()` field values
- Test `ChannelMonitor` clone behavior (shared atomic counters)
- Estimated impact: +10-15 lines covered

### Phase 2: Integration Tests (Requires PostgreSQL + PGMQ) -- Target +100-130 lines

These tests require `--features test-messaging` or `--features test-services`.

**`client.rs` -- Queue operation integration tests:**
- Test `create_queue` / `drop_queue` lifecycle
- Test `send_json_message` / `read_messages` / `delete_message` cycle
- Test `send_message_with_delay` with short delay
- Test `pop_message` (read + delete)
- Test `archive_message`
- Test `purge_queue`
- Test `queue_metrics` after send/read operations
- Test `health_check` returns true with valid connection
- Test `get_client_status` response shape
- Test `has_notify_capabilities` with trigger config
- Test `PgmqClientFactory` creation methods
- Test `process_namespace_queue` / `complete_message` / `initialize_namespace_queues` helpers
- Test `send_with_transaction` with explicit transaction
- Estimated impact: +100-120 lines covered

**`emitter.rs` -- DbEmitter integration tests:**
- Test `DbEmitter::new()` with valid pool
- Test `build_payload()` with events that exceed and fit within size limits
- Test `build_payload()` with `include_metadata = false` (metadata stripping path)
- Test `notify_channel()` execution (verify no SQL errors)
- Test `is_healthy()` with valid pool
- Test `EmitterFactory::database()` creation
- Estimated impact: +30-40 lines covered

### Phase 3: Listener Integration Tests (Requires PostgreSQL) -- Target +50-80 lines

These are the most complex tests requiring coordinated async operations.

**`listener.rs` -- Listener lifecycle tests:**
- Test `PgmqNotifyListener::new()` creation with config validation
- Test `connect()` / `disconnect()` lifecycle
- Test `listen_channel()` / `unlisten_channel()` with state tracking
- Test `listen_channel()` when not connected returns `NotConnected` error
- Test `listen_queue_created()`, `listen_message_ready_for_namespace()`, `listen_message_ready_global()` channel name construction
- Test `listen_default_namespaces()` with configured namespaces
- Test `stats()` reflects connection state changes
- Test `is_healthy()` after connect/disconnect
- Test `listening_channels()` after adding/removing channels
- Test `next_event()` when not connected returns error
- Test `Debug` impl produces valid output

**`listener.rs` -- Event delivery tests (with NOTIFY):**
- Test `start_listening()` receives events sent via `pg_notify`
- Test `listen_with_handler()` calls handler on received events
- Test `start_listening_with_handler()` spawns background task
- Test parse error handling (send malformed JSON via NOTIFY)
- Test channel monitoring integration in `start_listening()`
- Estimated impact: +50-80 lines covered

## Estimated Impact

| Phase | Tests Added | Estimated Lines Covered | Projected Line % |
|-------|-----------|------------------------|-----------------|
| Current | -- | 734 / 1512 | 48.54% |
| Phase 1 (Unit) | ~20-25 tests | +100-120 lines | ~55-56% |
| Phase 2 (Integration) | ~15-20 tests | +100-130 lines | ~62-65% |
| Phase 3 (Listener) | ~10-15 tests | +50-80 lines | ~66-70% |
| **Total** | **~45-60 tests** | **+250-330 lines** | **~65-70%** |

Phase 1 alone is insufficient to reach 65%. Phases 1+2 combined should bring coverage to approximately 62-65%, with Phase 3 providing the margin to confidently exceed the target.

**Recommended minimum for 65% target**: Phase 1 (pure unit tests) + Phase 2 (client/emitter integration tests). This covers the highest-impact, most-feasible test categories and avoids the complexity of async listener coordination tests.

### Lines Needed Calculation

To reach 65% line coverage: `0.65 * 1512 = 983` lines needed.
Currently covered: 734 lines.
Additional lines needed: **249 lines** (minimum).

### Key Dependencies for Integration Tests

- PostgreSQL instance with PGMQ extension installed
- `DATABASE_URL` environment variable set
- Feature flag `test-messaging` or `test-services` enabled
- PGMQ wrapper function `pgmq_send_with_notify` and `pgmq_read_specific_message` installed (for client tests)

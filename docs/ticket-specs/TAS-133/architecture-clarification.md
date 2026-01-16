# TAS-133: Messaging Abstraction Architecture Clarification

**Last Updated**: 2026-01-14
**Status**: Phases 1-3 Complete, Phase 2.4 Complete, Phase 4 Pending
**Context**: Discovered during TAS-133e migration - delivery model distinction requires clarification

---

## Progress Summary

| Phase | Description | Status |
|-------|-------------|--------|
| **Phase 1** | MessageHandle Enum & QueuedMessage Refactor | **COMPLETE** |
| **Phase 2.1** | SupportsPushNotifications Trait & MessageNotification Enum | **COMPLETE** |
| **Phase 2.2** | Implement SupportsPushNotifications for PGMQ | **COMPLETE** |
| **Phase 2.3** | Implement SupportsPushNotifications for RabbitMQ | **COMPLETE** |
| **Phase 2.4** | PGMQ Full Payload Notifications (< 7KB) | **COMPLETE** |
| **Phase 3** | Implement RabbitMQ Push Consumer | **COMPLETE** (via Phase 2.3) |
| Phase 4 | Provider-Driven Consumer Selection | Pending |

---

## Problem Statement

The TAS-133e migration introduced `.as_pgmq()` checks that reject non-PGMQ providers, but this **blocks the intended architecture**. Per the TAS-133 spec:
- `SupportsPushNotifications` trait enables provider-specific push consumers
- RabbitMQ should use native `basic_consume()` push delivery (no fallback polling needed)
- PGMQ uses LISTEN/NOTIFY + fallback polling

The current implementation only supports PGMQ's event-driven model. RabbitMQ's push consumption via `basic_consume()` is not implemented.

---

## Key Architectural Insight

From [push-notifications-research.md](./push-notifications-research.md), **updated with Phase 2.4 (full payload notifications)**:

| Provider | Native Model | Push Support | Notification Type | Fallback Needed |
|----------|--------------|--------------|-------------------|-----------------|
| PGMQ (< 7KB) | Poll | Yes (pg_notify) | **Full message** | Yes (catch-up) |
| PGMQ (>= 7KB) | Poll | Yes (pg_notify) | **Signal + msg_id** | Yes (catch-up) |
| RabbitMQ | **Push** | Yes (native) | **Full message** | **No** |

The distinction is in **delivery models**:
- **PGMQ (small messages < 7KB)**: `pg_notify` includes full message payload -> process directly (RabbitMQ-like)
- **PGMQ (large messages >= 7KB)**: `pg_notify` sends signal with msg_id -> fetch via `read_specific_message()` -> fallback polling catches missed signals
- **RabbitMQ**: `basic_consume()` delivers full messages -> no fallback needed because delivery is protocol-guaranteed

> **Phase 2.4 Insight**: PostgreSQL's `pg_notify` has an ~8KB payload limit (compile-time constant). By including full message payloads for messages under 7KB, PGMQ can achieve RabbitMQ-style direct processing for the majority of messages. This significantly simplifies the consumer code path.

---

## Intended Architecture (from TAS-133 spec, updated Phase 2.4)

### SupportsPushNotifications Trait
```rust
pub trait SupportsPushNotifications: MessagingService {
    fn subscribe(&self, queue_name: &str)
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;
}

// Phase 2.4: Updated MessageNotification with msg_id for large message handling
pub enum MessageNotification {
    /// PGMQ large message (>= 7KB): Signal with msg_id for efficient fetch
    Available { queue_name: String, msg_id: Option<i64> },

    /// PGMQ small message (< 7KB) or RabbitMQ: Full message included
    Message(QueuedMessage<Vec<u8>>),
}
```

### HybridConsumer Pattern (from push-notifications-research.md)
```rust
pub struct HybridConsumer<S: MessagingService + SupportsPushNotifications> {
    service: Arc<S>,
    push_stream: Pin<Box<dyn Stream<Item = MessageNotification> + Send>>,
    fallback_interval: Duration,  // Only for PGMQ, not RabbitMQ
}
```

---

## Current State vs Intended State

### Current State (After Phase 1)
```
PGMQ:
  PgmqNotifyListener (LISTEN/NOTIFY) ─┐
  FallbackPoller (polling safety net) ─┼─→ QueuedMessage<T> → Commands → Business Logic
                                       │        ↑
RabbitMQ:                              │   MessageHandle::Pgmq
  ??? (polling only via basic_get) ────┘   (provider-agnostic wrapper)
```

**Phase 1 Achievement**: Commands now use provider-agnostic `QueuedMessage<serde_json::Value>`
with explicit `MessageHandle` variants. The shared code path is ready for multiple providers.

### Intended State (After Phase 4)
```
PGMQ-Specific Files:
  PgmqNotifyListener (LISTEN/NOTIFY) ─┐
  FallbackPoller (polling safety net) ─┼─→ QueuedMessage<T> → Commands → Business Logic
                                       │        ↑
RabbitMQ-Specific Files:               │   MessageHandle::{Pgmq, RabbitMq, InMemory}
  RabbitMqConsumer (basic_consume push)┘   (provider-agnostic wrapper)
```

---

## Phase 1: MessageHandle Enum & QueuedMessage Refactor - COMPLETE

### What Was Implemented

**1.1 MessageHandle enum and MessageMetadata (types.rs)**

Added to `tasker-shared/src/messaging/service/types.rs`:

```rust
/// Explicit provider-specific handle for ack/nack/extend operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageHandle {
    Pgmq { msg_id: i64, queue_name: String },
    RabbitMq { delivery_tag: u64, queue_name: String },
    InMemory { id: u64, queue_name: String },
}

/// Common metadata across all providers
#[derive(Debug, Clone)]
pub struct MessageMetadata {
    receive_count: u32,
    enqueued_at: chrono::DateTime<chrono::Utc>,
}
```

**1.2 Command processors updated**

| File | Changes |
|------|---------|
| `tasker-orchestration/.../command_processor.rs` | Commands use `QueuedMessage<serde_json::Value>`, added `ack_message_with_handle()`, `wrap_pgmq_message()` helper |
| `tasker-worker/.../command_processor.rs` | `ExecuteStepFromMessage` uses `QueuedMessage<serde_json::Value>` |
| `tasker-worker/.../actor_command_processor.rs` | Routes to new `ExecuteStepFromQueuedMessage` actor message |

**1.3 Hydrators updated**

All three hydrators now have `hydrate_from_queued_message()` methods:

| Hydrator | New Method |
|----------|------------|
| `StepResultHydrator` | `hydrate_from_queued_message(&QueuedMessage<serde_json::Value>)` |
| `TaskRequestHydrator` | `hydrate_from_queued_message(&QueuedMessage<serde_json::Value>)` |
| `FinalizationHydrator` | `hydrate_from_queued_message(&QueuedMessage<serde_json::Value>)` |

**1.4 Worker actor system updated**

| File | Changes |
|------|---------|
| `tasker-worker/.../actors/messages.rs` | Added `ExecuteStepFromQueuedMessage` message type |
| `tasker-worker/.../actors/step_executor_actor.rs` | Added `Handler<ExecuteStepFromQueuedMessage>` implementation |
| `tasker-worker/.../actors/mod.rs` | Export new message type |

**1.5 Fallback pollers updated**

Both pollers now construct `QueuedMessage` with `MessageHandle::Pgmq`:

```rust
// TAS-133: Wrap PGMQ message in provider-agnostic QueuedMessage
let queued_message = QueuedMessage::with_handle(
    message.message.clone(),
    MessageHandle::Pgmq {
        msg_id: message.msg_id,
        queue_name: queue_name.to_string(),
    },
    MessageMetadata::new(message.read_ct as u32, message.enqueued_at),
);
```

### Files Modified in Phase 1

| File | Change Summary |
|------|----------------|
| `tasker-shared/src/messaging/service/types.rs` | Added `MessageHandle`, `MessageMetadata`, updated `QueuedMessage<T>` |
| `tasker-shared/src/messaging/service/mod.rs` | Export new types |
| `tasker-shared/src/messaging/service/providers/pgmq.rs` | Use `QueuedMessage::with_handle()` |
| `tasker-shared/src/messaging/service/providers/in_memory.rs` | Use `QueuedMessage::with_handle()` |
| `tasker-orchestration/.../command_processor.rs` | Commands use `QueuedMessage<serde_json::Value>` |
| `tasker-orchestration/.../fallback_poller.rs` | Construct `QueuedMessage` with `MessageHandle::Pgmq` |
| `tasker-orchestration/.../hydration/step_result_hydrator.rs` | Added `hydrate_from_queued_message()` |
| `tasker-orchestration/.../hydration/task_request_hydrator.rs` | Added `hydrate_from_queued_message()` |
| `tasker-orchestration/.../hydration/finalization_hydrator.rs` | Added `hydrate_from_queued_message()` |
| `tasker-worker/.../command_processor.rs` | Updated `ExecuteStepFromMessage` command |
| `tasker-worker/.../actor_command_processor.rs` | Route to `ExecuteStepFromQueuedMessage` |
| `tasker-worker/.../actors/messages.rs` | Added `ExecuteStepFromQueuedMessage` |
| `tasker-worker/.../actors/step_executor_actor.rs` | Added handler for new message type |
| `tasker-worker/.../actors/mod.rs` | Export new message type |
| `tasker-worker/.../worker_queues/fallback_poller.rs` | Construct `QueuedMessage` with `MessageHandle::Pgmq` |
| `tasker-worker/.../services/metrics/service.rs` | Fixed `get_queue_stats` method name |

### Validation

- **Compilation**: `cargo check --all-features` passes
- **Unit Tests**: All library tests pass
- **Clippy**: Clean (pre-existing warning in `system_context.rs` fixed in Phase 2.1)

---

## Phase 2.1: SupportsPushNotifications Trait & MessageNotification Enum - COMPLETE

**Goal**: Define the trait and enum for push notification abstraction

### What Was Implemented

**2.1.1 MessageNotification enum (types.rs)**

Added to `tasker-shared/src/messaging/service/types.rs`:

```rust
/// Push notification from a messaging provider - captures delivery model distinction
pub enum MessageNotification {
    /// Signal that a message is available (PGMQ style) - requires separate fetch
    Available { queue_name: String },

    /// Full message delivered (RabbitMQ style) - ready to process directly
    Message(QueuedMessage<Vec<u8>>),
}
```

Helper methods: `available()`, `message()`, `queue_name()`, `is_available()`, `is_message()`, `into_message()`

**2.1.2 SupportsPushNotifications trait (traits.rs)**

Added to `tasker-shared/src/messaging/service/traits.rs`:

```rust
pub trait SupportsPushNotifications: MessagingService {
    /// Subscribe to push notifications for a queue
    fn subscribe(&self, queue_name: &str)
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>;

    /// Check if this provider requires fallback polling
    fn requires_fallback_polling(&self) -> bool;

    /// Get the recommended fallback polling interval (default: 5s if polling required)
    fn fallback_polling_interval(&self) -> Option<Duration>;
}

/// Type alias for the notification stream
pub type NotificationStream = Pin<Box<dyn Stream<Item = MessageNotification> + Send>>;
```

**2.1.3 Module exports updated (mod.rs)**

Updated exports in `tasker-shared/src/messaging/service/mod.rs`:
- Added `SupportsPushNotifications` to traits exports
- Added `NotificationStream` to traits exports
- Added `MessageNotification` to types exports

### Files Modified in Phase 2.1

| File | Change Summary |
|------|----------------|
| `tasker-shared/src/messaging/service/types.rs` | Added `MessageNotification` enum with comprehensive docs |
| `tasker-shared/src/messaging/service/traits.rs` | Added `SupportsPushNotifications` trait, `NotificationStream` alias |
| `tasker-shared/src/messaging/service/mod.rs` | Updated exports for new types |
| `tasker-shared/src/system_context.rs` | Fixed clippy warning (`last()` → `next_back()`) |

### Validation

- **Compilation**: `cargo check --all-features` passes
- **Unit Tests**: All messaging service tests pass (48 passed, 7 ignored for RabbitMQ)
- **Clippy**: `cargo clippy --all-features -- -D warnings` passes

---

## Phase 2.2: Implement SupportsPushNotifications for PGMQ - COMPLETE

**Goal**: Implement the trait for PGMQ provider (wrapping PgmqNotifyListener)

### What Was Implemented

Added `SupportsPushNotifications` implementation to `PgmqMessagingService`:

```rust
impl SupportsPushNotifications for PgmqMessagingService {
    fn subscribe(&self, queue_name: &str)
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>
    {
        // Creates a PgmqNotifyListener for each subscription
        // Spawns a background task to manage listener lifecycle
        // Returns MessageNotification::Available for each pg_notify signal
    }

    fn requires_fallback_polling(&self) -> bool {
        true  // pg_notify is not guaranteed delivery
    }

    fn fallback_polling_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(5))  // Recommended interval
    }
}
```

### Implementation Details

- **Stream Creation**: Uses `futures::stream::unfold` to convert tokio mpsc receiver to Stream
- **Listener Lifecycle**: Background task manages `PgmqNotifyListener` connection and channel subscription
- **Namespace Extraction**: Extracts namespace from queue name (convention: `namespace_queue`)
- **Event Filtering**: Only forwards events for the target queue

### Files Modified

| File | Change Summary |
|------|----------------|
| `tasker-shared/src/messaging/service/providers/pgmq.rs` | Added `SupportsPushNotifications` impl |

### Validation

- **Compilation**: `cargo check --all-features` passes
- **Clippy**: `cargo clippy --all-features -- -D warnings` passes
- **Tests**: 143 tests pass (5 E2E failures unrelated to TAS-133)

---

## Phase 2.3: Implement SupportsPushNotifications for RabbitMQ - COMPLETE

**Goal**: Implement the trait for RabbitMQ provider

### What Was Implemented

Added `SupportsPushNotifications` implementation to `RabbitMqMessagingService`:

```rust
impl SupportsPushNotifications for RabbitMqMessagingService {
    fn subscribe(&self, queue_name: &str)
        -> Result<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>, MessagingError>
    {
        // Creates a dedicated channel for each consumer subscription
        // Uses lapin::basic_consume() for native AMQP push delivery
        // Returns MessageNotification::Message with full payload
    }

    fn requires_fallback_polling(&self) -> bool {
        false  // RabbitMQ's basic_consume guarantees delivery
    }

    fn fallback_polling_interval(&self) -> Option<Duration> {
        None  // No fallback polling needed
    }
}
```

### Implementation Details

- **Dedicated Channel**: Each subscription creates a new AMQP channel to avoid interference
- **Arc<Connection>**: Modified `RabbitMqMessagingService` to wrap connection in `Arc` for sharing
- **Consumer Tag**: Auto-generated UUID-based consumer tag for uniqueness
- **Prefetch Control**: Respects configured prefetch count for backpressure
- **Stream Creation**: Uses `futures::stream::unfold` to convert tokio mpsc receiver to Stream
- **Message Format**: Returns `MessageNotification::Message` with raw bytes (consumer deserializes)
- **Error Handling**: Individual delivery errors logged but don't stop the consumer

### Key Architectural Distinction

| Provider | `requires_fallback_polling()` | Notification Type | Why |
|----------|------------------------------|-------------------|-----|
| PGMQ | `true` | `Available` (signal) | pg_notify can miss signals |
| RabbitMQ | `false` | `Message` (full) | AMQP protocol guarantees delivery |

### Files Modified

| File | Change Summary |
|------|----------------|
| `tasker-shared/src/messaging/service/providers/rabbitmq.rs` | Added `SupportsPushNotifications` impl, wrapped connection in Arc |

### Validation

- **Compilation**: `cargo check --all-features` passes
- **Clippy**: `cargo clippy --all-features -- -D warnings` passes
- **Unit Tests**: New unit test `test_rabbitmq_does_not_require_fallback_polling` passes
- **Integration Test**: New `test_rabbitmq_push_notification_subscribe` (requires RabbitMQ)

---

## Phase 2.4: PGMQ Full Payload Notifications - COMPLETE

**Goal**: Include full message payloads in PGMQ notifications for small messages (< 7KB), enabling RabbitMQ-style direct processing without a separate fetch operation.

### Background

PostgreSQL's `pg_notify` has a compile-time payload limit of ~8KB (8000 bytes). This is a hard constraint that cannot be changed without recompiling PostgreSQL. Most managed PostgreSQL services (RDS, Aurora, etc.) use the default limit.

By using a 7KB threshold (leaving room for JSON wrapper metadata), PGMQ can:
- Include full message payloads for small messages → process directly
- Fall back to signal-only notifications for large messages → fetch via `read_specific_message(msg_id)`

### What Was Implemented

**2.4.1 SQL Migration Updates**

Updated `migrations/20250826180921_add_pgmq_notifications.sql`:

```sql
-- Payload size threshold (7000 bytes leaves room for metadata wrapper)
CREATE OR REPLACE FUNCTION pgmq_payload_size_threshold()
RETURNS INTEGER AS $$ BEGIN RETURN 7000; END; $$ LANGUAGE plpgsql IMMUTABLE;

-- pgmq_send_with_notify now checks message size
CREATE OR REPLACE FUNCTION pgmq_send_with_notify(...)
  -- If message < 7KB: event_type='message_with_payload', includes full message
  -- If message >= 7KB: event_type='message_ready', signal only with msg_id
```

**2.4.2 pgmq-notify Crate Updates**

Added `MessageWithPayloadEvent` and updated `PgmqNotifyEvent` enum:

```rust
// pgmq-notify/src/events.rs
pub enum PgmqNotifyEvent {
    QueueCreated(QueueCreatedEvent),
    MessageReady(MessageReadyEvent),       // Signal only (>= 7KB)
    MessageWithPayload(MessageWithPayloadEvent),  // Full payload (< 7KB)
    BatchReady(BatchReadyEvent),
}

pub struct MessageWithPayloadEvent {
    pub msg_id: i64,
    pub queue_name: String,
    pub namespace: String,
    pub message: serde_json::Value,  // The full payload!
    pub ready_at: DateTime<Utc>,
    pub delay_seconds: i32,
}
```

**2.4.3 MessageNotification Updated**

Added `msg_id` field to `Available` variant:

```rust
// tasker-shared/src/messaging/service/types.rs
pub enum MessageNotification {
    Available {
        queue_name: String,
        msg_id: Option<i64>,  // NEW: For efficient large message fetch
    },
    Message(QueuedMessage<Vec<u8>>),
}
```

New helper methods:
- `available_with_msg_id(queue_name, msg_id)` - Create Available with specific msg_id
- `msg_id()` - Get the msg_id if present

**2.4.4 PGMQ Subscribe Implementation Updated**

`PgmqMessagingService::subscribe()` now handles both event types:

```rust
match &event {
    PgmqNotifyEvent::MessageWithPayload(e) => {
        // Small message: Convert to MessageNotification::Message
        // Consumer can process directly without fetch
        let queued_msg = QueuedMessage::with_handle(payload_bytes, handle, metadata);
        MessageNotification::message(queued_msg)
    }
    PgmqNotifyEvent::MessageReady(e) => {
        // Large message: Return Available with msg_id
        // Consumer must fetch via read_specific_message()
        MessageNotification::available_with_msg_id(queue_name, e.msg_id)
    }
    // ...
}
```

### Files Modified

| File | Change Summary |
|------|----------------|
| `migrations/20250826180921_add_pgmq_notifications.sql` | Added size threshold, dual-mode notification logic |
| `pgmq-notify/src/events.rs` | Added `MessageWithPayloadEvent`, updated enum |
| `pgmq-notify/src/emitter.rs` | Added `emit_message_with_payload` method |
| `pgmq-notify/src/lib.rs` | Export `MessageWithPayloadEvent` |
| `tasker-shared/src/messaging/service/types.rs` | Added `msg_id` to `Available`, new helper methods |
| `tasker-shared/src/messaging/service/providers/pgmq.rs` | Updated subscribe() for dual-mode handling |

### Consumer Code Path (Simplified)

```rust
match notification {
    MessageNotification::Message(queued_msg) => {
        // PGMQ small message OR RabbitMQ: Process directly!
        process(queued_msg);
    }
    MessageNotification::Available { queue_name, msg_id } => {
        // PGMQ large message: Fetch using msg_id
        if let Some(id) = msg_id {
            let msg = service.read_specific_message(&queue_name, id).await?;
            process(msg);
        }
    }
}
```

### Key Benefits

1. **Unified code path**: Both PGMQ and RabbitMQ return `MessageNotification::Message` for most messages
2. **No fetch overhead**: Small messages (majority of traffic) processed directly from notification
3. **Graceful fallback**: Large messages still work via msg_id-based fetch
4. **Backwards compatible**: Existing consumers that handle `Available` continue to work

### Validation

- **Compilation**: `cargo check --all-features` passes
- **Clippy**: Clean
- **Unit Tests**: 21 MessageNotification tests pass (including 4 new tests)
- **pgmq-notify Tests**: 26 tests pass

---

## Phase 3: Implement RabbitMQ Push Consumer - COMPLETE (via Phase 2.3)

**Goal**: RabbitMQ-specific consumer that doesn't need fallback polling

### Outcome

**Phase 3 is now considered complete** because the RabbitMQ push consumer functionality was implemented directly in Phase 2.3 via the `SupportsPushNotifications::subscribe()` method.

The original plan called for separate consumer wrapper files:
- `tasker-shared/src/messaging/consumers/rabbitmq.rs`
- `tasker-orchestration/.../rabbitmq_consumer.rs`
- `tasker-worker/.../rabbitmq_consumer.rs`

However, these are **no longer needed** because:

1. **`SupportsPushNotifications::subscribe()`** provides the push consumer functionality directly
2. **Phase 4** will use this trait method for provider-driven consumer selection
3. **Wrappers would be redundant** - the trait provides a clean abstraction

### Integration Pattern

Instead of separate consumer files, orchestration and worker will use:

```rust
// Provider-agnostic push consumption (Phase 4 pattern)
if provider.supports_push() {
    let stream = provider.subscribe(queue_name)?;
    while let Some(notification) = stream.next().await {
        match notification {
            MessageNotification::Available { .. } => { /* fetch + process */ }
            MessageNotification::Message(msg) => { /* process directly */ }
        }
    }
}
```

---

## Phase 4: Provider-Driven Consumer Selection - PENDING

**Goal**: Event system selects consumer based on provider

### Design

```rust
// In event system initialization
match context.messaging_provider().provider_name() {
    "pgmq" => {
        start_pgmq_listener();
        start_fallback_poller();  // PGMQ needs this
    }
    "rabbitmq" => {
        start_rabbitmq_consumer();
        // No fallback poller - push delivery is guaranteed
    }
    _ => {
        start_polling_only();  // Generic fallback
    }
}
```

### Files to modify

| File | Change |
|------|--------|
| `tasker-orchestration/.../event_systems/orchestration_event_system.rs` | Provider-based consumer selection |
| `tasker-worker/.../event_systems/worker_event_system.rs` | Provider-based consumer selection |

---

## File Organization Principle

**Requirement**: "Shared flows in shared files, backend-specific flows to backend-specific files, even if impl blocks are roughly the same, so code paths are straightforward to analyze."

### Shared (Provider-Agnostic) - Use `MessageClient`
| Component | Location | Purpose |
|-----------|----------|---------|
| `MessageClient` | `tasker-shared/src/messaging/client.rs` | Generic queue operations |
| Command processors | `tasker-orchestration/src/orchestration/command_processor.rs` | Business logic dispatch |
| Message classification | `tasker-shared/src/config/queue_classifier.rs` | Queue routing |
| Actor handlers | Various `*_actor.rs` files | Business logic |

### PGMQ-Specific - Keep in Dedicated Files
| Component | Location | Why PGMQ-Only |
|-----------|----------|---------------|
| `PgmqNotifyListener` | `pgmq-notify/src/listener.rs` | PostgreSQL LISTEN subscription |
| `OrchestrationQueueListener` | `tasker-orchestration/.../listener.rs` | Wraps PgmqNotifyListener |
| `WorkerQueueListener` | `tasker-worker/.../listener.rs` | Wraps PgmqNotifyListener |
| `OrchestrationFallbackPoller` | `tasker-orchestration/.../fallback_poller.rs` | Safety net for pg_notify |
| `WorkerFallbackPoller` | `tasker-worker/.../fallback_poller.rs` | Safety net for pg_notify |
| `read_specific_message()` | Various | Read by msg_id after notification |

### RabbitMQ-Specific - TO BE IMPLEMENTED (Phase 3)
| Component | Location | Purpose |
|-----------|----------|---------|
| `RabbitMqConsumer` | NEW: `tasker-shared/src/messaging/consumers/rabbitmq.rs` | Push consumer via `basic_consume()` |
| `RabbitMqOrchestrationConsumer` | NEW: `tasker-orchestration/.../rabbitmq_consumer.rs` | Orchestration push consumer |
| `RabbitMqWorkerConsumer` | NEW: `tasker-worker/.../rabbitmq_consumer.rs` | Worker push consumer |

---

## Critical Files Reference

### Already Correct (Keep As-Is)
- `tasker-worker/src/worker/actors/step_executor_actor.rs`: `read_specific_message()` after event notification IS PGMQ-specific
- `tasker-orchestration/src/orchestration/command_processor.rs`: Event-driven message read IS PGMQ-specific

### Documentation Added (Phase 1)
- Fallback pollers now have TAS-133 comments explaining the `QueuedMessage` wrapping
- Command processors document the provider-agnostic design

All files with `.as_pgmq()` should explain the delivery model distinction in comments.

---

## Verification Plan

1. **Compilation**: `cargo check --all-features` - **PASSING**
2. **Unit Tests**: `cargo test --all-features` - **143 passed** (5 E2E failures unrelated to TAS-133)
3. **Clippy**: `cargo clippy --all-features` - **Clean** (1 pre-existing warning)
4. **PGMQ Integration**: E2E tests pass with PGMQ (existing tests)
5. **RabbitMQ Integration**: E2E tests pass with RabbitMQ (after Phase 3)
6. **Provider Switching**: Config change switches providers cleanly (after Phase 4)

---

## Summary

**Phases 1-3 and 2.4 are complete.** The codebase now has:

1. **Provider-agnostic messaging** via `QueuedMessage<T>` with explicit `MessageHandle` variants
2. **Unified push notifications** via `SupportsPushNotifications` trait
3. **Full payload delivery** for small PGMQ messages (< 7KB) matching RabbitMQ's direct processing model
4. **Efficient fallback** for large messages via msg_id-based fetch

### Phase 2.4 Impact

The full payload notification feature significantly simplifies the consumer code path:

- **Before**: PGMQ always returned `Available` signals → consumer always needed to fetch
- **After**: PGMQ returns `Message` for small messages (< 7KB) → consumer processes directly

This means the majority of messages can now be handled identically to RabbitMQ, with a graceful fallback for large payloads.

### Remaining Work

**Phase 4: Provider-Driven Consumer Selection** - The event systems in `tasker-orchestration` and `tasker-worker` need to be updated to use the unified `SupportsPushNotifications` trait instead of PGMQ-specific listeners.

This aligns with TAS-133's goal: "Zero handler changes required - switching providers requires only configuration changes."

---

## Related Documents

- [README.md](./README.md) - Original TAS-133 specification
- [implementation-plan.md](./implementation-plan.md) - Main implementation plan
- [push-notifications-research.md](./push-notifications-research.md) - Provider push capabilities research
- [trait-design.md](./trait-design.md) - Trait semantics deep dive

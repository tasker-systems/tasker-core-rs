# TAS-133: Channel and Message Type Intentionality

## Problem Statement

During the RabbitMQ backend implementation, we discovered runtime errors that could have been caught at compile time:

```
CRITICAL: Signal-only notification received but provider 'rabbitmq' does not support fetch-by-message-ID
```

The root cause: our command pattern uses generic channels and notification types that don't encode provider capabilities or message source semantics. This creates several problems:

1. **No compile-time safety** - Mismatched message flows are only caught at runtime
2. **Implicit assumptions** - Code assumes PGMQ-style fetch-by-ID without type enforcement
3. **Difficult static analysis** - `tx.send(Command::X)` → `match command { X => ... }` is hard to trace
4. **Provider capability blindness** - Nothing in the type system says "RabbitMQ can't do this"

## Goals

1. **Compile-time enforcement** of message flow compatibility
2. **Self-documenting types** that encode source, direction, and capabilities
3. **Zero runtime cost** via NewType patterns
4. **Easier static analysis** of data flow through the system

---

## Part 1: NewType Wrappers for Channels

### Current State

```rust
// Generic channels - no semantic meaning
let (tx, rx) = mpsc::channel::<WorkerNotification>(100);
let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCommand>(200);

// Any sender can be passed anywhere that expects the base type
fn process_events(sender: mpsc::Sender<WorkerNotification>) { ... }
```

### Proposed Pattern

```rust
// NewType wrappers with semantic meaning
pub struct WorkerEventSender(mpsc::Sender<WorkerNotification>);
pub struct WorkerEventReceiver(mpsc::Receiver<WorkerNotification>);

pub struct WorkerCommandSender(mpsc::Sender<WorkerCommand>);
pub struct WorkerCommandReceiver(mpsc::Receiver<WorkerCommand>);

// Even more specific - encode the source
pub struct PgmqNotificationSender(mpsc::Sender<SignalOnlyNotification>);
pub struct RabbitMqNotificationSender(mpsc::Sender<FullMessageNotification>);

impl WorkerEventSender {
    pub async fn send(&self, notification: WorkerNotification) -> Result<(), SendError<WorkerNotification>> {
        self.0.send(notification).await
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl WorkerEventReceiver {
    pub async fn recv(&mut self) -> Option<WorkerNotification> {
        self.0.recv().await
    }
}
```

### Benefits

- **Type mismatch = compile error**: Can't pass `WorkerEventSender` where `OrchestrationEventSender` is expected
- **Self-documenting function signatures**: `fn handle_worker_events(rx: WorkerEventReceiver)`
- **Zero runtime cost**: NewType compiles away entirely
- **IDE support**: Autocomplete shows only valid operations

### Channel Factory Pattern

```rust
pub struct ChannelFactory;

impl ChannelFactory {
    pub fn worker_event_channel(buffer_size: usize) -> (WorkerEventSender, WorkerEventReceiver) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (WorkerEventSender(tx), WorkerEventReceiver(rx))
    }

    pub fn worker_command_channel(buffer_size: usize) -> (WorkerCommandSender, WorkerCommandReceiver) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (WorkerCommandSender(tx), WorkerCommandReceiver(rx))
    }

    pub fn orchestration_event_channel(buffer_size: usize) -> (OrchestrationEventSender, OrchestrationEventReceiver) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (OrchestrationEventSender(tx), OrchestrationEventReceiver(rx))
    }
}
```

---

## Part 2: Trait Boundaries for Message Types

### Current State

```rust
// Commands don't encode what data they need
pub enum WorkerCommand {
    ExecuteStepFromEvent { message_event: MessageEvent, resp: ... },
    ExecuteStepFromMessage { message: QueuedMessage<Value>, resp: ... },
}

// Runtime check for capability
if !provider.supports_fetch_by_message_id() {
    error!("CRITICAL: Provider doesn't support fetch-by-message-ID");
    return Err(...);
}
```

### Proposed Pattern: Capability Traits

```rust
/// Marker trait for providers that support fetch-by-message-ID
pub trait SupportsFetchById: MessagingService {}

/// Marker trait for providers that deliver full messages via push
pub trait SupportsFullMessagePush: MessagingService {}

// PGMQ implements fetch-by-ID
impl SupportsFetchById for PgmqMessagingService {}

// RabbitMQ implements full message push
impl SupportsFullMessagePush for RabbitMqMessagingService {}

// Both implement the base push notifications trait
impl SupportsPushNotifications for PgmqMessagingService {}
impl SupportsPushNotifications for RabbitMqMessagingService {}
```

### Proposed Pattern: Message Source Types

```rust
/// A notification that contains only a signal (queue name + message ID)
/// Requires fetch-by-ID to get the actual message
#[derive(Debug, Clone)]
pub struct SignalOnlyNotification {
    pub queue_name: String,
    pub message_id: i64,
    pub namespace: String,
}

/// A notification that contains the full message payload
/// No fetch required - message is ready to process
#[derive(Debug, Clone)]
pub struct FullMessageNotification<T> {
    pub message: QueuedMessage<T>,
    pub namespace: String,
}

/// Provider-specific notification enums
pub enum PgmqNotification {
    Signal(SignalOnlyNotification),
    // PGMQ can also inline small messages
    SmallMessage(FullMessageNotification<Vec<u8>>),
}

pub enum RabbitMqNotification {
    // RabbitMQ always delivers full messages
    Message(FullMessageNotification<Vec<u8>>),
}
```

### Handler Traits Bound to Message Types

```rust
/// Handler for signal-only notifications (requires fetch)
pub trait HandlesSignalNotification {
    type Provider: SupportsFetchById;

    async fn handle_signal(
        &self,
        notification: SignalOnlyNotification,
        provider: &Self::Provider,
    ) -> Result<()>;
}

/// Handler for full message notifications (no fetch needed)
pub trait HandlesFullMessageNotification {
    async fn handle_full_message(
        &self,
        notification: FullMessageNotification<Vec<u8>>,
    ) -> Result<()>;
}

// StepExecutorActor implements both
impl HandlesSignalNotification for StepExecutorActor {
    type Provider = PgmqMessagingService; // Compile-time binding!

    async fn handle_signal(&self, notification: SignalOnlyNotification, provider: &Self::Provider) -> Result<()> {
        let message = provider.read_specific_message(&notification.queue_name, notification.message_id).await?;
        self.process_step(message).await
    }
}

impl HandlesFullMessageNotification for StepExecutorActor {
    async fn handle_full_message(&self, notification: FullMessageNotification<Vec<u8>>) -> Result<()> {
        let message = parse_message(notification.message)?;
        self.process_step(message).await
    }
}
```

---

## Part 3: Directional Naming (From/To Source Encoding)

### Current State

```rust
pub enum WorkerCommand {
    ExecuteStep { ... },
    ExecuteStepFromMessage { ... },
    ExecuteStepFromEvent { ... },
}

pub enum WorkerNotification {
    Event(WorkerQueueEvent),
    StepMessageWithPayload(QueuedMessage<Vec<u8>>),
}
```

### Proposed Naming Convention

Encode **source**, **direction**, and **payload type** in names:

```rust
/// Commands encode: [Action][From][Source][PayloadType]
pub enum WorkerCommand {
    // Direct execution (testing, internal)
    ExecuteStep { step: StepMessage, resp: ... },

    // From PGMQ signal - requires fetch
    ExecuteStepFromPgmqSignal { signal: SignalOnlyNotification, resp: ... },

    // From RabbitMQ push - full payload available
    ExecuteStepFromRabbitMqPush { message: FullMessageNotification<Vec<u8>>, resp: ... },

    // From fallback polling - full payload available
    ExecuteStepFromPolledMessage { message: QueuedMessage<Value>, resp: ... },
}

/// Notifications encode: [Source][PayloadType][Notification]
pub enum WorkerNotification {
    // PGMQ signal-only (pg_notify)
    PgmqSignal(SignalOnlyNotification),

    // RabbitMQ full message (basic_consume)
    RabbitMqPush(FullMessageNotification<Vec<u8>>),

    // System events
    ConnectionError(String),
    HealthUpdate(WorkerHealthUpdate),
}
```

### Benefits of Directional Naming

1. **Self-documenting**: `ExecuteStepFromPgmqSignal` tells you exactly what data is available
2. **Grep-friendly**: `grep "FromRabbitMq"` finds all RabbitMQ-sourced flows
3. **Review-friendly**: Code reviewers can spot mismatches visually
4. **IDE-friendly**: Autocomplete groups related commands together

---

## Part 4: Provider Abstraction (Keep Current Approach)

### Why the Current Enum Works Well

The existing `MessagingProvider` enum is actually a good design:

```rust
pub enum MessagingProvider {
    Pgmq(PgmqMessagingService),
    RabbitMq(RabbitMqMessagingService),
    InMemory(InMemoryMessagingService),
}
```

**Benefits of the enum approach:**
- **Stack-allocated**: No heap allocation overhead
- **Exhaustive matching**: Compiler ensures all variants are handled
- **Optimized dispatch**: Compiler can inline and optimize match arms
- **Simple mental model**: Easy to understand and debug

### What We Considered (and Why We're Not Doing It)

We initially considered a type-state pattern with `PhantomData` capability markers:

```rust
// NOT RECOMMENDED - adds complexity without proportional benefit
pub struct Provider<Caps> {
    inner: Box<dyn MessagingService>,  // Heap allocation!
    _caps: PhantomData<Caps>,
}
```

**Problems with this approach:**
- Introduces heap allocation where we have stack-allocated enums
- Adds vtable indirection overhead
- More complex type signatures throughout the codebase
- The real issues (message source encoding) are better solved by Phases 1-3

### Recommendation

**Keep the current enum-based `MessagingProvider`**. The compile-time safety we need comes from:
- Phase 1: NewType channel wrappers
- Phase 2: Message source types (`SignalOnlyNotification` vs `FullMessageNotification`)
- Phase 3: Directional command naming

These three phases address the actual problems we encountered without over-engineering the provider abstraction.

---

## Current State Analysis

### What's Already Good (Keep As-Is)

Based on codebase analysis, several patterns are already well-designed:

**Orchestration Commands** (`tasker-orchestration/src/orchestration/command_processor.rs:40-126`)
- ✅ `ProcessStepResultFromMessage` / `ProcessStepResultFromMessageEvent` - clear source encoding
- ✅ `InitializeTaskFromMessage` / `InitializeTaskFromMessageEvent` - clear source encoding
- ✅ `FinalizeTaskFromMessage` / `FinalizeTaskFromMessageEvent` - clear source encoding

**Worker Commands** (`tasker-worker/src/worker/command_processor.rs:30-94`)
- ✅ `ExecuteStepFromMessage` (line 83) - provider-agnostic full message
- ✅ `ExecuteStepFromEvent` (line 90) - event-driven signal-only

**Message Handle Enum** (`tasker-shared/src/messaging/service/types.rs:185-277`)
- ✅ `MessageHandle::Pgmq`, `MessageHandle::RabbitMq`, `MessageHandle::InMemory` - provider-specific dispatch

### What Needs Improvement

**Generic Channel Types** - No semantic meaning in channel types:
- `tasker-worker/src/worker/event_systems/worker_event_system.rs:357` - generic `mpsc::channel::<WorkerNotification>`
- `tasker-worker/src/worker/actor_command_processor.rs:133` - generic `mpsc::channel(command_buffer_size)`
- `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs:455,520` - generic channels

**MessageNotification Naming** (`tasker-shared/src/messaging/service/types.rs:871-961`)
- `Available { queue_name, msg_id }` → should be `Signal(SignalOnlyNotification)`
- `Message(QueuedMessage<Vec<u8>>)` → should be `Full(FullMessageNotification)`

**Worker Notification Naming** (`tasker-worker/src/worker/worker_queues/events.rs:43-55`)
- `Event(WorkerQueueEvent)` → could be `PgmqSignal(...)` for clarity
- `StepMessageWithPayload(...)` → could be `FullMessage(...)` for consistency

**Orchestration Notification Naming** (`tasker-orchestration/src/orchestration/orchestration_queues/listener.rs:113-125`)
- `Event(OrchestrationQueueEvent)` → could be `PgmqSignal(...)`
- `StepResultWithPayload(...)` → could be `FullMessage(...)`

---

## Implementation Plan

### Phase 1: NewType Channel Wrappers (Low Risk)

Create semantic NewType wrappers for MPSC channels to prevent accidental misuse.

#### 1.1 Create Channel Module

**New file**: `tasker-shared/src/channels/mod.rs`

```rust
//! Semantic NewType wrappers for MPSC channels (TAS-133)

use tokio::sync::mpsc;

// Worker channels
pub struct WorkerNotificationSender(pub mpsc::Sender<WorkerNotification>);
pub struct WorkerNotificationReceiver(pub mpsc::Receiver<WorkerNotification>);
pub struct WorkerCommandSender(pub mpsc::Sender<WorkerCommand>);
pub struct WorkerCommandReceiver(pub mpsc::Receiver<WorkerCommand>);

// Orchestration channels
pub struct OrchestrationNotificationSender(pub mpsc::Sender<OrchestrationNotification>);
pub struct OrchestrationNotificationReceiver(pub mpsc::Receiver<OrchestrationNotification>);
pub struct OrchestrationCommandSender(pub mpsc::Sender<OrchestrationCommand>);
pub struct OrchestrationCommandReceiver(pub mpsc::Receiver<OrchestrationCommand>);

// Factory for consistent creation
pub struct ChannelFactory;
impl ChannelFactory {
    pub fn worker_notification_channel(size: usize) -> (WorkerNotificationSender, WorkerNotificationReceiver);
    pub fn worker_command_channel(size: usize) -> (WorkerCommandSender, WorkerCommandReceiver);
    pub fn orchestration_notification_channel(size: usize) -> (...);
    pub fn orchestration_command_channel(size: usize) -> (...);
}
```

#### 1.2 Update Worker Event System

**File**: `tasker-worker/src/worker/event_systems/worker_event_system.rs`

| Line | Current | Change To |
|------|---------|-----------|
| 357 | `mpsc::channel::<WorkerNotification>(buffer_size)` | `ChannelFactory::worker_notification_channel(buffer_size)` |
| Field | `notification_sender: mpsc::Sender<WorkerNotification>` | `notification_sender: WorkerNotificationSender` |

#### 1.3 Update Worker Actor Command Processor

**File**: `tasker-worker/src/worker/actor_command_processor.rs`

| Line | Current | Change To |
|------|---------|-----------|
| 133 | `mpsc::channel(command_buffer_size)` | `ChannelFactory::worker_command_channel(command_buffer_size)` |
| Field | `command_sender: mpsc::Sender<WorkerCommand>` | `command_sender: WorkerCommandSender` |

#### 1.4 Update Orchestration Event System

**File**: `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs`

| Line | Current | Change To |
|------|---------|-----------|
| 455 | `mpsc::channel(buffer_size as usize)` | `ChannelFactory::orchestration_notification_channel(buffer_size)` |
| 520 | `mpsc::channel(buffer_size as usize)` | `ChannelFactory::orchestration_notification_channel(buffer_size)` |

#### 1.5 Update Orchestration Command Processor

**File**: `tasker-orchestration/src/orchestration/command_processor.rs`

| Line | Current | Change To |
|------|---------|-----------|
| 291 | `mpsc::channel(buffer_size)` | `ChannelFactory::orchestration_command_channel(buffer_size)` |

---

### Phase 2: Message Source Types (Medium Risk)

Create explicit types that encode whether a notification requires fetching or has full payload.

#### 2.1 Add Source Types to Shared

**File**: `tasker-shared/src/messaging/service/types.rs`

Add after line 961 (after MessageNotification):

```rust
/// A notification that contains only a signal (requires fetch-by-ID)
/// Used by PGMQ for large messages via pg_notify
#[derive(Debug, Clone)]
pub struct SignalOnlyNotification {
    pub queue_name: String,
    pub msg_id: i64,
    pub namespace: String,
}

/// A notification that contains the full message payload
/// Used by RabbitMQ (always) and PGMQ (for small messages < 7KB)
#[derive(Debug, Clone)]
pub struct FullMessageNotification {
    pub message: QueuedMessage<Vec<u8>>,
    pub namespace: String,
}

impl SignalOnlyNotification {
    pub fn from_available(queue_name: String, msg_id: i64, namespace: String) -> Self {
        Self { queue_name, msg_id, namespace }
    }
}

impl FullMessageNotification {
    pub fn from_queued_message(message: QueuedMessage<Vec<u8>>, namespace: String) -> Self {
        Self { message, namespace }
    }
}
```

#### 2.2 Update MessageNotification Enum

**File**: `tasker-shared/src/messaging/service/types.rs:871-900`

| Current Variant | New Variant | Reason |
|-----------------|-------------|--------|
| `Available { queue_name, msg_id }` | `Signal(SignalOnlyNotification)` | Explicit type for signal-only |
| `Message(QueuedMessage<Vec<u8>>)` | `Full(FullMessageNotification)` | Explicit type for full payload |

#### 2.3 Update Worker Notification Enum

**File**: `tasker-worker/src/worker/worker_queues/events.rs:43-55`

| Current Variant | New Variant | Reason |
|-----------------|-------------|--------|
| `Event(WorkerQueueEvent)` | `Signal(SignalOnlyNotification)` | Clearer source encoding |
| `StepMessageWithPayload(QueuedMessage<Vec<u8>>)` | `FullMessage(FullMessageNotification)` | Consistent naming |

**Also update**: `WorkerQueueEvent` inner enum (lines 27-36) to use `SignalOnlyNotification`

#### 2.4 Update Worker Listener

**File**: `tasker-worker/src/worker/worker_queues/listener.rs`

Update `process_subscription_stream()` to emit new notification types:
- `MessageNotification::Available` → `WorkerNotification::Signal(...)`
- `MessageNotification::Message` → `WorkerNotification::FullMessage(...)`

#### 2.5 Update Orchestration Notification Enum

**File**: `tasker-orchestration/src/orchestration/orchestration_queues/listener.rs:113-125`

| Current Variant | New Variant | Reason |
|-----------------|-------------|--------|
| `Event(OrchestrationQueueEvent)` | `Signal(SignalOnlyNotification)` | Clearer source encoding |
| `StepResultWithPayload(QueuedMessage<Vec<u8>>)` | `FullMessage(FullMessageNotification)` | Consistent naming |

#### 2.6 Update Event System Handlers

**Files to update**:
- `tasker-worker/src/worker/event_systems/worker_event_system.rs:387-470` - match on new variants
- `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs:560-650` - match on new variants

---

### Phase 3: Directional Command Naming (Medium Risk)

Enhance command names to be more explicit about message source. Note: Most orchestration commands already have good naming.

#### 3.1 Worker Command Updates

**File**: `tasker-worker/src/worker/command_processor.rs:30-94`

| Current | Proposed | Rationale |
|---------|----------|-----------|
| `ExecuteStep` (line 32) | `ExecuteStepDirect` | Clarify it's for direct/test invocation |
| `ExecuteStepFromEvent` (line 90) | `ExecuteStepFromSignal` | Aligns with `SignalOnlyNotification` |
| `ExecuteStepFromMessage` (line 83) | Keep as-is | Already clear |

#### 3.2 Update Command Handlers

**File**: `tasker-worker/src/worker/actor_command_processor.rs`

Update match arms (around line 200+) to use new variant names.

#### 3.3 Update Command Senders

**Files**:
- `tasker-worker/src/worker/event_systems/worker_event_system.rs` - update command creation
- `tasker-worker/src/worker/worker_queues/fallback_poller.rs` - update command creation

---

### Phase 4: Not Needed

After analysis, we determined that Phases 1-3 address the actual problems we encountered.
The current enum-based `MessagingProvider` works well and doesn't need to change.

See "Part 4: Provider Abstraction (Keep Current Approach)" for rationale.

---

## Channel Inventory (Reference)

### Worker Channels

| Channel | Location | Type | Buffer Config |
|---------|----------|------|---------------|
| Command | `actor_command_processor.rs:133` | `WorkerCommand` | `command_buffer_size` param |
| Notification | `worker_event_system.rs:357` | `WorkerNotification` | `WorkerEventSystemConfig` |
| Dispatch | `actors/registry.rs:221` | `DispatchHandlerMessage` | `dispatch_buffer_size` (1000) |
| Completion | `actors/registry.rs:224` | completion messages | `completion_buffer_size` (1000) |
| Domain Event | `domain_event_system.rs:306` | `DomainEventCommand` | `channel_buffer_size` (1000) |

### Orchestration Channels

| Channel | Location | Type | Buffer Config |
|---------|----------|------|---------------|
| Command | `command_processor.rs:291` | `OrchestrationCommand` | `command_buffer_size` (5000) |
| Notification (Hybrid) | `orchestration_event_system.rs:455` | `OrchestrationNotification` | `event_channel_buffer_size` (10000) |
| Notification (EventDriven) | `orchestration_event_system.rs:520` | `OrchestrationNotification` | `event_channel_buffer_size` (10000) |

---

## Command Flow Summary

### Worker Flow (Signal-Only Path - PGMQ)
```
WorkerQueueListener
  → receives MessageNotification::Available (signal-only)
  → sends WorkerNotification::Signal(SignalOnlyNotification)
WorkerEventSystem
  → creates WorkerCommand::ExecuteStepFromSignal
  → sends to command channel
ActorCommandProcessor
  → routes to StepExecutorActor
  → actor fetches message via read_specific_message(msg_id)
```

### Worker Flow (Full Message Path - RabbitMQ)
```
WorkerQueueListener
  → receives MessageNotification::Message (full payload)
  → sends WorkerNotification::FullMessage(FullMessageNotification)
WorkerEventSystem
  → creates WorkerCommand::ExecuteStepFromMessage
  → sends to command channel
ActorCommandProcessor
  → routes to StepExecutorActor
  → actor processes message directly (no fetch)
```

### Orchestration Flow (Similar Pattern)
```
OrchestrationQueueListener
  → Signal path: OrchestrationNotification::Signal → ProcessStepResultFromSignal
  → Full path: OrchestrationNotification::FullMessage → ProcessStepResultFromMessage
```

---

## Success Criteria

1. **Compile-time errors** for provider/message type mismatches
2. **No runtime "CRITICAL" errors** for capability mismatches
3. **Improved IDE support** with type-aware autocomplete
4. **Self-documenting code** that shows data flow at a glance
5. **Full test suite passes** with both PGMQ and RabbitMQ backends

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Large refactor surface | Medium | Phase implementation, one component at a time |
| Breaking existing tests | Medium | Run full test suite after each phase |
| Over-engineering | Low | Scoped to Phases 1-3; rejected Phase 4 complexity |
| Performance regression | None | NewTypes are zero-cost abstractions |

---

## Known Issues

### Domain Event Tests Fail with RabbitMQ Backend

**Status**: Known issue, to be addressed after baseline established

**Affected Tests** (6 total):
- `tasker-shared::domain_events_test::test_publish_event`
- `tasker-shared::domain_events_test::test_publish_event_with_correlation`
- `tasker-shared::domain_events_test::test_multiple_namespaces`
- `tasker-worker-rust::payment_event_publisher_integration_test::test_payment_event_publisher_success_flow`
- `tasker-worker-rust::payment_event_publisher_integration_test::test_payment_event_publisher_failure_flow`
- `tasker-worker-rust::payment_event_publisher_integration_test::test_end_to_end_payment_flow`

**Root Cause**: Tests verify message publishing by directly querying PGMQ tables:
```rust
// Direct PGMQ table query - fails when RabbitMQ is broker
let message_count: i64 =
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM pgmq.q_{}", queue_name))
        .fetch_one(pgmq_pool)
        .await?;
```

**Fix**: Replace direct PGMQ table queries with provider-agnostic `receive_messages()` API for verification, consistent with TAS-133 abstraction goals.

**Files to Update**:
- `tasker-shared/tests/domain_events_test.rs` (lines 192-195, 254-259, 312-316)
- `workers/rust/tests/payment_event_publisher_integration_test.rs` (lines 322-325, 400-403, 495-498)

---

## Related Documents

- `docs/ticket-specs/TAS-133/README.md` - Main TAS-133 spec
- `docs/ticket-specs/TAS-133/push-notifications-research.md` - Provider capability research
- `docs/development/mpsc-channel-guidelines.md` - Channel configuration guidelines

# TAS-133: Listener Migration to Provider-Agnostic Architecture

## Executive Summary

The current `OrchestrationQueueListener` and `WorkerQueueListener` are hardcoded to use PGMQ's `PgmqNotifyListener`. This document outlines the migration to use the `SupportsPushNotifications::subscribe()` abstraction, enabling both PGMQ and RabbitMQ backends while preserving all existing functionality.

**Key Changes:**
1. Create provider-agnostic `MessageEvent` type in `tasker-shared/src/messaging/service/types.rs`
2. Update domain event enums to use `MessageEvent` instead of `pgmq_notify::MessageReadyEvent`
3. Refactor existing listeners to use `messaging_provider().subscribe()` internally (evolve in place)

**Design Decision:** Rather than creating new `*EventProcessor` types and deprecating the existing listeners, we evolve the listeners in place. This approach:
- Keeps the same API surface (event systems unchanged)
- Preserves existing stats, health checks, and lifecycle methods
- Reduces code churn and cognitive load
- Can be renamed to `*EventConsumer` later as optional polish

## Current State Analysis

### Architecture Gap

```
CURRENT (PGMQ-hardcoded):
┌─────────────────────────────────────────────────────────────────────┐
│ OrchestrationEventSystem / WorkerEventSystem                        │
│   └─> *QueueListener                                                │
│         └─> PgmqNotifyListener::new(context.pgmq_pool())  ← PGMQ!  │
│               └─> PgmqNotifyEvent { MessageReady, MessageWithPayload, ... }
└─────────────────────────────────────────────────────────────────────┘

UNUSED ABSTRACTION:
┌─────────────────────────────────────────────────────────────────────┐
│ MessagingProvider (exists but bypassed for subscriptions)           │
│   ├─> PgmqMessagingService::subscribe()    ✓ implemented           │
│   └─> RabbitMqMessagingService::subscribe() ✓ implemented          │
│         └─> MessageNotification { Available, Message }              │
└─────────────────────────────────────────────────────────────────────┘
```

### EventDrivenSystem Trait Analysis

The `EventDrivenSystem` trait in `tasker-shared/src/event_system/event_driven.rs` is **already provider-agnostic**:

```rust
#[async_trait]
pub trait EventDrivenSystem: Send + Sync {
    type SystemId: Send + Sync + Clone + fmt::Display + fmt::Debug;
    type Event: Send + Sync + Clone + fmt::Debug;  // ← Associated type
    type Config: Send + Sync + Clone;
    type Statistics: EventSystemStatistics + Send + Sync + Clone;

    async fn start(&mut self) -> Result<(), DeploymentModeError>;
    async fn stop(&mut self) -> Result<(), DeploymentModeError>;
    async fn process_event(&self, event: Self::Event) -> Result<(), DeploymentModeError>;
    async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError>;
    // ...
}
```

**The problem is in the concrete `type Event` implementations:**

```rust
// tasker-orchestration/.../events.rs - PGMQ-SPECIFIC!
pub enum OrchestrationQueueEvent {
    StepResult(pgmq_notify::MessageReadyEvent),      // ← PGMQ type
    TaskRequest(pgmq_notify::MessageReadyEvent),     // ← PGMQ type
    TaskFinalization(pgmq_notify::MessageReadyEvent), // ← PGMQ type
    Unknown { queue_name: String, payload: String },
}

// tasker-worker/.../events.rs - PGMQ-SPECIFIC!
pub enum WorkerQueueEvent {
    StepMessage(pgmq_notify::MessageReadyEvent),       // ← PGMQ type
    HealthCheck(pgmq_notify::MessageReadyEvent),       // ← PGMQ type
    ConfigurationUpdate(pgmq_notify::MessageReadyEvent), // ← PGMQ type
    Unknown { queue_name: String, payload: String },
}
```

**Decision: Evolve the event types** to use a provider-agnostic inner type rather than creating parallel event hierarchies.

### Files Affected

| File | Current Role | Migration Impact |
|------|--------------|------------------|
| `tasker-shared/src/messaging/service/types.rs` | Provider-agnostic message types | **Add `MessageEvent` type** |
| `tasker-orchestration/.../orchestration_queues/events.rs` | Domain event definitions | **Update to use `MessageEvent`** |
| `tasker-worker/.../worker_queues/events.rs` | Domain event definitions | **Update to use `MessageEvent`** |
| `tasker-orchestration/.../orchestration_queues/listener.rs` | PGMQ-specific listener | **Refactor internals to use `messaging_provider().subscribe()`** |
| `tasker-worker/.../worker_queues/listener.rs` | PGMQ-specific listener | **Refactor internals to use `messaging_provider().subscribe()`** |
| `tasker-orchestration/.../event_systems/orchestration_event_system.rs` | Creates OrchestrationQueueListener | **No changes** (listener API unchanged) |
| `tasker-worker/.../event_systems/worker_event_system.rs` | Creates WorkerQueueListener | **No changes** (listener API unchanged) |

## Target Architecture

### Processing Pipeline

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ *QueueListener (Refactored - Same API, Different Internals)                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  STAGE 1: Subscription (Provider-Specific, Hidden Inside Listener)          │
│  ─────────────────────────────────────────────────────────────────          │
│  messaging_provider().subscribe(queue_name)                                 │
│    ├─> PGMQ: Stream<MessageNotification::Available { queue_name, msg_id }> │
│    └─> RabbitMQ: Stream<MessageNotification::Message(QueuedMessage)>       │
│                                      │                                      │
│                                      ▼                                      │
│  STAGE 2: Notification → MessageEvent (Inside Listener)                     │
│  ─────────────────────────────────────────────────────────────────          │
│  process_notification(notification)                                         │
│    ├─> Available: extract → MessageEvent { queue_name, namespace, msg_id } │
│    └─> Message: extract from QueuedMessage → MessageEvent                  │
│                                      │                                      │
│                                      ▼                                      │
│  STAGE 3: Classification (Existing Logic, Updated Inner Type)               │
│  ─────────────────────────────────────────────────────────────────          │
│  classify(MessageEvent) → OrchestrationQueueEvent / WorkerQueueEvent       │
│    ├─> Orchestration: StepResult, TaskRequest, TaskFinalization, Unknown   │
│    └─> Worker: StepMessage, HealthCheck, ConfigurationUpdate, Unknown      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ STAGE 4: Dispatch (Unchanged - Same as Today)                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ mpsc::Sender<*Notification>.send(Event(*QueueEvent))                       │
│   └─> Command processors, actors, handlers (unchanged)                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### What Changes Inside Listeners

```rust
// BEFORE (current listener internals)
impl OrchestrationQueueListener {
    pub async fn new(...) -> TaskerResult<Self> {
        // Creates PGMQ-specific listener
        let pgmq_listener = PgmqNotifyListener::new(
            context.pgmq_pool(),
            queues,
            event_handler,
        ).await?;
        // ...
    }

    // Receives PgmqNotifyEvent, converts to OrchestrationQueueEvent
}

// AFTER (refactored listener internals)
impl OrchestrationQueueListener {
    pub async fn new(...) -> TaskerResult<Self> {
        // Uses provider abstraction
        let streams: Vec<_> = config.monitored_queues.iter()
            .map(|q| context.messaging_provider().subscribe(q))
            .collect::<Result<_, _>>()?;
        // ...
    }

    // Receives MessageNotification, converts to MessageEvent,
    // then classifies to OrchestrationQueueEvent
}
```

### Type Organization in `tasker-shared/src/messaging/service/types.rs`

```
Existing Types:
├── MessageId         - provider-agnostic message identifier
├── ReceiptHandle     - string-based handle for ack/nack
├── MessageHandle     - enum with provider-specific variants (Pgmq, RabbitMq, InMemory)
├── MessageMetadata   - common metadata (receive_count, enqueued_at)
├── QueuedMessage<T>  - full message with payload + handle + metadata
├── MessageNotification - push delivery model (Available vs Message)
├── QueueStats        - queue statistics
└── QueueHealthReport - health check results

NEW Type:
└── MessageEvent      - provider-agnostic event for routing/classification
```

**Design Rationale:**
- `MessageNotification` answers: "Do I have the full message or just a signal?" (delivery model)
- `MessageEvent` answers: "What queue/namespace is this for?" (routing/classification)

These are different stages in the processing pipeline and serve distinct purposes.

## Implementation Plan

### Phase 0: Add `MessageEvent` Type

**Goal**: Create provider-agnostic message event type in shared types.

**File**: `tasker-shared/src/messaging/service/types.rs`

```rust
// =============================================================================
// MessageEvent - Provider-agnostic event for routing and classification
// =============================================================================

/// Provider-agnostic message event for event routing and classification
///
/// This is the provider-agnostic equivalent of `pgmq_notify::MessageReadyEvent`.
/// Used as the inner type for classified domain events (OrchestrationQueueEvent,
/// WorkerQueueEvent) to enable provider-agnostic event processing.
///
/// ## Processing Pipeline
///
/// ```text
/// subscribe() → MessageNotification (delivery model)
///     → process → MessageEvent (routing info)
///         → classify → OrchestrationQueueEvent/WorkerQueueEvent (domain events)
/// ```
///
/// ## Conversion
///
/// Created from `MessageNotification` during event processing:
/// - `Available { queue_name, msg_id }` → extract routing info + namespace from queue name
/// - `Message(QueuedMessage)` → extract from handle
///
/// Also convertible from PGMQ types for backward compatibility during migration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MessageEvent {
    /// Queue name where the message resides
    pub queue_name: String,

    /// Namespace for routing (e.g., "orchestration", "linear_workflow")
    ///
    /// Extracted from queue name patterns or explicitly provided.
    pub namespace: String,

    /// Provider-agnostic message identifier
    pub message_id: MessageId,
}

impl MessageEvent {
    /// Create a new message event
    pub fn new(
        queue_name: impl Into<String>,
        namespace: impl Into<String>,
        message_id: impl Into<MessageId>,
    ) -> Self {
        Self {
            queue_name: queue_name.into(),
            namespace: namespace.into(),
            message_id: message_id.into(),
        }
    }

    /// Create from a MessageNotification::Available variant
    ///
    /// Requires namespace to be provided since Available doesn't contain it.
    pub fn from_available(queue_name: &str, msg_id: Option<i64>, namespace: &str) -> Self {
        Self {
            queue_name: queue_name.to_string(),
            namespace: namespace.to_string(),
            message_id: msg_id.map(MessageId::from).unwrap_or_else(|| MessageId::new("unknown")),
        }
    }

    /// Create from a QueuedMessage (for MessageNotification::Message variant)
    pub fn from_queued_message<T>(msg: &QueuedMessage<T>, namespace: &str) -> Self {
        Self {
            queue_name: msg.queue_name().to_string(),
            namespace: namespace.to_string(),
            message_id: msg.handle.as_receipt_handle().as_str().into(),
        }
    }
}

impl std::fmt::Display for MessageEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.namespace, self.queue_name, self.message_id)
    }
}

// Backward compatibility: Convert from pgmq_notify::MessageReadyEvent
impl From<pgmq_notify::MessageReadyEvent> for MessageEvent {
    fn from(event: pgmq_notify::MessageReadyEvent) -> Self {
        Self {
            queue_name: event.queue_name,
            namespace: event.namespace,
            message_id: MessageId::from(event.msg_id),
        }
    }
}
```

**Checklist:**
- [ ] Add `MessageEvent` struct to `types.rs`
- [ ] Add conversion methods (`from_available`, `from_queued_message`)
- [ ] Add `From<pgmq_notify::MessageReadyEvent>` for backward compatibility
- [ ] Export from `tasker-shared/src/messaging/service/mod.rs`
- [ ] Add unit tests

#### Phase 0 Validation Gates

**Type Properties (unit tests):**
```rust
#[test]
fn message_event_fields_are_accessible() {
    let event = MessageEvent::new("test_queue", "orchestration", MessageId::from(123i64));
    assert_eq!(event.queue_name, "test_queue");
    assert_eq!(event.namespace, "orchestration");
    // message_id is comparable
}

#[test]
fn message_event_display_format() {
    let event = MessageEvent::new("q", "ns", MessageId::from(1i64));
    assert_eq!(format!("{}", event), "ns:q:1");
}
```

**Conversion Correctness (unit tests):**
```rust
#[test]
fn from_available_handles_some_msg_id() {
    let event = MessageEvent::from_available("queue", Some(42), "ns");
    // message_id should represent 42
}

#[test]
fn from_available_handles_none_msg_id() {
    let event = MessageEvent::from_available("queue", None, "ns");
    // message_id should be "unknown" sentinel
}

#[test]
fn from_pgmq_event_preserves_all_fields() {
    let pgmq_event = pgmq_notify::MessageReadyEvent {
        msg_id: 123,
        queue_name: "test_queue".to_string(),
        namespace: "orchestration".to_string(),
    };
    let event: MessageEvent = pgmq_event.into();
    assert_eq!(event.queue_name, "test_queue");
    assert_eq!(event.namespace, "orchestration");
    // message_id represents 123
}
```

**Trait Bounds (compile-time):**
- `MessageEvent: Debug + Clone + PartialEq + Eq + Send + Sync`
- `MessageEvent: Serialize + Deserialize` (for logging/debugging)

**Documentation (rustdoc):**
- Module-level doc explains the processing pipeline position
- Each method has examples showing input → output
- `from_available` and `from_queued_message` document the namespace requirement

### Phase 1: Update Domain Event Types

**Goal**: Change domain event enums to use `MessageEvent` instead of `pgmq_notify::MessageReadyEvent`.

#### 1.1 Update OrchestrationQueueEvent

**File**: `tasker-orchestration/src/orchestration/orchestration_queues/events.rs`

```rust
// BEFORE
pub enum OrchestrationQueueEvent {
    StepResult(pgmq_notify::MessageReadyEvent),
    TaskRequest(pgmq_notify::MessageReadyEvent),
    TaskFinalization(pgmq_notify::MessageReadyEvent),
    Unknown { queue_name: String, payload: String },
}

// AFTER
use tasker_shared::messaging::service::MessageEvent;

pub enum OrchestrationQueueEvent {
    StepResult(MessageEvent),
    TaskRequest(MessageEvent),
    TaskFinalization(MessageEvent),
    Unknown { queue_name: String, payload: String },
}
```

#### 1.2 Update WorkerQueueEvent

**File**: `tasker-worker/src/worker/worker_queues/events.rs`

```rust
// BEFORE
pub enum WorkerQueueEvent {
    StepMessage(pgmq_notify::MessageReadyEvent),
    HealthCheck(pgmq_notify::MessageReadyEvent),
    ConfigurationUpdate(pgmq_notify::MessageReadyEvent),
    Unknown { queue_name: String, payload: String },
}

// AFTER
use tasker_shared::messaging::service::MessageEvent;

pub enum WorkerQueueEvent {
    StepMessage(MessageEvent),
    HealthCheck(MessageEvent),
    ConfigurationUpdate(MessageEvent),
    Unknown { queue_name: String, payload: String },
}
```

**Checklist:**
- [ ] Update `OrchestrationQueueEvent` to use `MessageEvent`
- [ ] Update `WorkerQueueEvent` to use `MessageEvent`
- [ ] Update all code that accesses `.msg_id`, `.queue_name`, `.namespace` fields
- [ ] Update `ConfigDrivenMessageEvent` classifier if needed
- [ ] Ensure all tests compile and pass

#### Phase 1 Validation Gates

**Field Access Migration (compile-time):**
```rust
// All existing field accesses must work with MessageEvent
// BEFORE: event.msg_id (i64)
// AFTER:  event.message_id (MessageId)

// These patterns must compile after migration:
fn access_orchestration_event(event: &OrchestrationQueueEvent) {
    match event {
        OrchestrationQueueEvent::StepResult(msg) => {
            let _queue = &msg.queue_name;      // String
            let _ns = &msg.namespace;          // String
            let _id = &msg.message_id;         // MessageId
        }
        // ... other variants
    }
}
```

**Classification Equivalence (unit tests):**
```rust
#[test]
fn classifier_produces_same_variants_with_message_event() {
    // Given the same queue_name, classification should produce same variant
    let pgmq_event = pgmq_notify::MessageReadyEvent {
        queue_name: "orchestration_step_results".into(),
        namespace: "orchestration".into(),
        msg_id: 1,
    };
    let message_event: MessageEvent = pgmq_event.clone().into();

    // Both should classify to StepResult
    let old_result = classifier.classify_pgmq(&pgmq_event);  // Before migration
    let new_result = classifier.classify(&message_event);    // After migration

    // Variant matches (inner type differs but that's expected)
    assert!(matches!(old_result, OrchestrationQueueEvent::StepResult(_)));
    assert!(matches!(new_result, OrchestrationQueueEvent::StepResult(_)));
}
```

**Downstream Compatibility (integration):**
- `OrchestrationCommand::ProcessStepResultFromMessageEvent` accepts `MessageEvent`
- `OrchestrationCommand::InitializeTaskFromMessageEvent` accepts `MessageEvent`
- `WorkerCommand::ExecuteStepFromEvent` accepts `MessageEvent`
- Command processors can extract `message_id` for database lookups

**No pgmq_notify Import (grep verification):**
```bash
# After Phase 1, these files should NOT import pgmq_notify::MessageReadyEvent:
grep -r "pgmq_notify::MessageReadyEvent" tasker-orchestration/src/orchestration/orchestration_queues/events.rs
grep -r "pgmq_notify::MessageReadyEvent" tasker-worker/src/worker/worker_queues/events.rs
# Should return empty (no matches)
```

### Phase 2: Refactor Listeners to Use Provider Abstraction

**Goal**: Update existing listener internals to use `messaging_provider().subscribe()` instead of `PgmqNotifyListener`.

**Key Insight**: The listeners already have the right API surface (new, start, stop, is_healthy, stats). We just need to change their internal subscription source.

#### 2.1 OrchestrationQueueListener Refactoring

**File**: `tasker-orchestration/src/orchestration/orchestration_queues/listener.rs`

**Changes Required:**

1. **Remove `PgmqNotifyListener` dependency**
2. **Store subscription streams instead of pgmq listener**
3. **Process `MessageNotification` → `MessageEvent` → classify**

```rust
pub struct OrchestrationQueueListener {
    listener_id: Uuid,
    config: OrchestrationListenerConfig,
    context: Arc<SystemContext>,
    notification_sender: mpsc::Sender<OrchestrationNotification>,
    channel_monitor: ChannelMonitor,
    stats: Arc<OrchestrationListenerStats>,  // Same stats structure
    queue_classifier: QueueClassifier,        // Same classifier
    is_running: AtomicBool,

    // CHANGED: Instead of PgmqNotifyListener
    // subscription_streams: Vec<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>>,
    // Or store JoinHandles for spawned subscription tasks
    subscription_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl OrchestrationQueueListener {
    pub async fn new(
        config: OrchestrationListenerConfig,
        context: Arc<SystemContext>,
        notification_sender: mpsc::Sender<OrchestrationNotification>,
        channel_monitor: ChannelMonitor,
    ) -> TaskerResult<Self> {
        // Same setup as before, but don't create PgmqNotifyListener yet
        let queue_classifier = QueueClassifier::from_config(&context.tasker_config)?;

        Ok(Self {
            listener_id: Uuid::new_v4(),
            config,
            context,
            notification_sender,
            channel_monitor,
            stats: Arc::new(OrchestrationListenerStats::default()),
            queue_classifier,
            is_running: AtomicBool::new(false),
            subscription_handles: Vec::new(),
        })
    }

    pub async fn start(&mut self) -> TaskerResult<()> {
        // Subscribe to each monitored queue via provider abstraction
        let provider = self.context.messaging_provider();

        for queue_name in &self.config.monitored_queues {
            let stream = provider.subscribe(queue_name)?;
            let sender = self.notification_sender.clone();
            let classifier = self.queue_classifier.clone();
            let stats = self.stats.clone();
            let namespace = self.config.namespace.clone();
            let monitor = self.channel_monitor.clone();

            let handle = tokio::spawn(async move {
                Self::process_subscription_stream(
                    stream, sender, classifier, stats, namespace, monitor
                ).await;
            });

            self.subscription_handles.push(handle);
        }

        self.is_running.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn process_subscription_stream(
        mut stream: Pin<Box<dyn Stream<Item = MessageNotification> + Send>>,
        sender: mpsc::Sender<OrchestrationNotification>,
        classifier: QueueClassifier,
        stats: Arc<OrchestrationListenerStats>,
        namespace: String,
        monitor: ChannelMonitor,
    ) {
        use futures::StreamExt;

        while let Some(notification) = stream.next().await {
            // Convert MessageNotification → MessageEvent
            let message_event = match notification {
                MessageNotification::Available { queue_name, msg_id } => {
                    MessageEvent::from_available(&queue_name, msg_id, &namespace)
                }
                MessageNotification::Message(queued_msg) => {
                    MessageEvent::from_queued_message(&queued_msg, &namespace)
                }
            };

            stats.events_received.fetch_add(1, Ordering::Relaxed);

            // Classify and dispatch (same logic as before, now with MessageEvent)
            let domain_event = classifier.classify(&message_event);

            monitor.record_send_attempt();
            if sender.send(OrchestrationNotification::Event(domain_event)).await.is_ok() {
                monitor.record_send_success();
            }
        }

        // Stream ended - could be connection loss
        let _ = sender.send(OrchestrationNotification::ConnectionError(
            "Subscription stream ended".to_string()
        )).await;
    }

    // stop(), is_healthy(), stats() methods stay the same
}
```

#### 2.2 WorkerQueueListener Refactoring

**File**: `tasker-worker/src/worker/worker_queues/listener.rs`

Same pattern as orchestration, but with worker-specific classification (pattern matching on queue names).

**Checklist:**
- [ ] Remove `PgmqNotifyListener` usage from `OrchestrationQueueListener`
- [ ] Remove `PgmqNotifyListener` usage from `WorkerQueueListener`
- [ ] Add `process_subscription_stream()` method to handle `MessageNotification`
- [ ] Update classification to work with `MessageEvent` (should be straightforward since fields match)
- [ ] Preserve all existing stats, health checks, lifecycle methods
- [ ] Unit tests pass with new internals

#### Phase 2 Validation Gates

**API Surface Unchanged (compile-time):**
```rust
// Event systems must compile WITHOUT modification
// This is the key success criterion - same constructor signature

// OrchestrationEventSystem (unchanged code):
let listener = OrchestrationQueueListener::new(
    listener_config,           // OrchestrationListenerConfig
    self.context.clone(),      // Arc<SystemContext>
    event_sender,              // mpsc::Sender<OrchestrationNotification>
    channel_monitor.clone(),   // ChannelMonitor
).await?;

listener.start().await?;
listener.stop().await?;
let healthy = listener.is_healthy();
let stats = listener.stats();
```

**Provider Abstraction Used (unit tests):**
```rust
#[tokio::test]
async fn listener_uses_messaging_provider_subscribe() {
    // Mock SystemContext with mock MessagingProvider
    let mock_provider = MockMessagingProvider::new();
    mock_provider.expect_subscribe()
        .times(3)  // For 3 orchestration queues
        .returning(|_| Ok(mock_stream()));

    let context = create_context_with_provider(mock_provider);
    let listener = OrchestrationQueueListener::new(config, context, sender, monitor).await?;

    listener.start().await?;

    // Verify subscribe() was called for each queue
    mock_provider.checkpoint();
}
```

**MessageNotification Handling (unit tests):**
```rust
#[tokio::test]
async fn listener_handles_available_notification() {
    // Given: Stream yields MessageNotification::Available
    let notification = MessageNotification::Available {
        queue_name: "orchestration_step_results".to_string(),
        msg_id: Some(42),
    };

    // When: Listener processes notification
    // Then: Produces OrchestrationQueueEvent::StepResult(MessageEvent)
    let received = receiver.recv().await.unwrap();
    assert!(matches!(
        received,
        OrchestrationNotification::Event(OrchestrationQueueEvent::StepResult(msg))
        if msg.queue_name == "orchestration_step_results"
    ));
}

#[tokio::test]
async fn listener_handles_message_notification() {
    // Given: Stream yields MessageNotification::Message (RabbitMQ style)
    let queued_msg = QueuedMessage::new(...);
    let notification = MessageNotification::Message(queued_msg);

    // When: Listener processes notification
    // Then: Produces same domain event variant
}
```

**Stats Preservation (unit tests):**
```rust
#[tokio::test]
async fn listener_increments_events_received_stat() {
    let listener = create_listener().await;
    listener.start().await?;

    // Send notification through mock stream
    send_mock_notification().await;

    // Verify stat incremented
    let stats = listener.stats();
    assert_eq!(stats.events_received.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn listener_tracks_connection_errors() {
    // When stream terminates unexpectedly
    // Then ConnectionError notification is sent
    // And stats reflect the error
}
```

**Lifecycle Correctness (unit tests):**
```rust
#[tokio::test]
async fn listener_stop_cancels_subscription_tasks() {
    let listener = create_listener().await;
    listener.start().await?;
    assert!(listener.is_running());

    listener.stop().await?;
    assert!(!listener.is_running());

    // Subscription handles should be aborted/completed
}

#[tokio::test]
async fn listener_health_reflects_stream_state() {
    let listener = create_listener().await;

    // Before start: not healthy (or healthy but not running)
    listener.start().await?;
    assert!(listener.is_healthy());

    // After stream error: health degrades
}
```

**No PgmqNotifyListener Import (grep verification):**
```bash
# After Phase 2, listener files should NOT import PgmqNotifyListener:
grep -r "PgmqNotifyListener" tasker-orchestration/src/orchestration/orchestration_queues/listener.rs
grep -r "PgmqNotifyListener" tasker-worker/src/worker/worker_queues/listener.rs
# Should return empty (no matches)

# Should NOT import pgmq_notify crate at all in listeners:
grep -r "use pgmq_notify" tasker-orchestration/src/orchestration/orchestration_queues/listener.rs
grep -r "use pgmq_notify" tasker-worker/src/worker/worker_queues/listener.rs
# Should return empty (no matches)
```

**Integration Validation (Deferred to TAS-133f):**

The following E2E tests are out of scope for TAS-133e but should be implemented during migration cleanup:

```rust
// TAS-133f: Full E2E validation
#[tokio::test]
async fn orchestration_flow_works_with_refactored_listener() {
    // Full flow: send message to queue → listener receives → command dispatched
}

#[tokio::test]
async fn worker_flow_works_with_refactored_listener() {
    // Full flow: enqueue step → listener receives → worker executes
}

#[tokio::test]
async fn hybrid_mode_catches_missed_notifications() {
    // Fallback polling resilience test
}
```

## Future Enhancement: Multi-Queue Subscription

**Goal**: Add intent-expressive API for subscribing to multiple queues.

Currently, listeners will use map/collect ergonomics directly:

```rust
let streams: Vec<_> = queues.iter()
    .map(|q| provider.subscribe(q))
    .collect::<Result<_, _>>()?;
```

A future enhancement would add `subscribe_many` to `SupportsPushNotifications` for clearer intent:

```rust
/// Subscribe to multiple queues, returning a stream per queue
fn subscribe_many(
    &self,
    queue_names: &[String],
) -> Result<Vec<Pin<Box<dyn Stream<Item = MessageNotification> + Send>>>, MessagingError> {
    queue_names.iter()
        .map(|q| self.subscribe(q))
        .collect()
}
```

This is not blocking for the migration - the map/collect pattern works fine. Adding `subscribe_many` is optional polish that improves API ergonomics.

## Functional Parity Requirements

### Must Preserve: Common Infrastructure

| Capability | Current Implementation | Migration Approach |
|------------|------------------------|-------------------|
| **Listener ID** | `Uuid` for tracing | **No change** (keep in listener) |
| **Configuration** | `*ListenerConfig` structs | **No change** (same config structs) |
| **Event Channel** | `mpsc::Sender<*Notification>` | **No change** |
| **Channel Monitor** | TAS-51 `ChannelMonitor` | **No change** |
| **Statistics** | `*ListenerStats` with atomics | **No change** (same stats structs) |
| **Lifecycle** | `start()`, `stop()` methods | **No change** (same methods) |
| **Health Check** | `is_healthy()`, `is_connected()` | Track stream state instead of pgmq connection |

### Must Preserve: Orchestration-Specific

| Capability | Current | Migration |
|------------|---------|-----------|
| **Classification** | `QueueClassifier` from config | No change - reuse directly |
| **Event Types** | StepResults, TaskRequests, TaskFinalizations, Unknown | **Inner type changes to `MessageEvent`** |
| **Stats Fields** | events_received, step_results_processed, task_requests_processed, queue_management_processed, unknown_events, connection_errors, last_event_at, started_at | No change |
| **Output Type** | `OrchestrationNotification` enum | No change |
| **Error Notifications** | ConnectionError, Reconnected variants | Adapt to stream errors |

### Must Preserve: Worker-Specific

| Capability | Current | Migration |
|------------|---------|-----------|
| **Classification** | Pattern matching on queue name | No change - reuse directly |
| **Event Types** | StepMessage, HealthCheck, ConfigurationUpdate, Unknown | **Inner type changes to `MessageEvent`** |
| **Stats Fields** | events_received, step_messages_processed, health_checks_processed, config_updates_processed, unknown_events, connection_errors, last_event_at, started_at | No change |
| **Output Type** | `WorkerNotification` enum | No change |
| **Namespace Filtering** | `supported_namespaces` config | No change |

## Provider Behavior Differences

### Message Delivery Model

| Provider | `subscribe()` Returns | Action Required |
|----------|----------------------|-----------------|
| **PGMQ** | `MessageNotification::Available { queue_name, msg_id }` | Fetch message via `read_specific_message()` or `receive_messages()` |
| **RabbitMQ** | `MessageNotification::Message(QueuedMessage<Vec<u8>>)` | Message already available, deserialize and process |

### Fallback Polling

| Provider | Fallback Needed | Reason |
|----------|-----------------|--------|
| **PGMQ** | Yes | `pg_notify` is fire-and-forget; signals can be lost |
| **RabbitMQ** | No | Broker tracks delivery; redelivers unacked messages |

This is already handled by the `DeploymentMode::effective_for_provider()` implementation.

### Error Handling

| Provider | Current Error Handling | Target Error Handling |
|----------|------------------------|----------------------|
| **PGMQ** | `PgmqEventHandler::handle_connection_error()` callback | Stream yields error or terminates; detect and send ConnectionError |
| **RabbitMQ** | N/A (not implemented) | Stream yields error or terminates; detect and send ConnectionError |

## Testing Strategy

### Unit Tests

1. **MessageEvent Creation**: Test all constructors and conversions
2. **Listener Initialization**: Verify listeners initialize with correct config (unchanged API)
3. **Available Handling**: Mock `Available` notification → verify `MessageEvent` extraction
4. **Message Handling**: Mock `Message` notification → verify `MessageEvent` extraction
5. **Stats Tracking**: Verify all counters increment correctly (same stats structure)
6. **Classification**: Verify queue → event type mapping preserved with new inner type

### Integration Tests

1. **PGMQ End-to-End**: Send message → receive `Available` → fetch → process
2. **RabbitMQ End-to-End**: Send message → receive `Message` → process
3. **Provider Switching**: Same test suite runs against both providers
4. **Fallback Poller Interaction**: PGMQ with Hybrid mode processes missed messages
5. **Event System Unchanged**: Verify event systems work without modification

### Regression Tests

1. **Orchestration Flows**: Task request → step enqueue → result processing
2. **Worker Flows**: Step message → handler dispatch → completion
3. **Error Handling**: Connection loss → reconnection → recovery
4. **Existing Listener Tests**: All current listener tests pass with new internals

## Migration Checklist

Each phase has validation gates defined in the Implementation Plan section above. A phase is complete when all checklist items AND validation gates pass.

### Phase 0: MessageEvent Type

**Implementation:**
- [ ] Add `MessageEvent` struct to `tasker-shared/src/messaging/service/types.rs`
- [ ] Add conversion methods (`from_available`, `from_queued_message`)
- [ ] Add `From<pgmq_notify::MessageReadyEvent>` for backward compatibility
- [ ] Export from `tasker-shared/src/messaging/service/mod.rs`

**Validation Gates (see Phase 0 Validation Gates):**
- [ ] Type properties tests pass (field access, Display format)
- [ ] Conversion correctness tests pass (Some/None msg_id, pgmq event preservation)
- [ ] Trait bounds compile (`Debug + Clone + PartialEq + Eq + Send + Sync + Serialize + Deserialize`)
- [ ] Rustdoc builds with examples

**Phase 0 Exit Criterion:** `cargo test -p tasker-shared message_event` passes

### Phase 1: Domain Event Types

**Implementation:**
- [ ] Update `OrchestrationQueueEvent` to use `MessageEvent`
- [ ] Update `WorkerQueueEvent` to use `MessageEvent`
- [ ] Update `ConfigDrivenMessageEvent` classifier to work with `MessageEvent`
- [ ] Update all code accessing inner event fields (`.msg_id` → `.message_id`, etc.)

**Validation Gates (see Phase 1 Validation Gates):**
- [ ] Field access migration compiles (all match arms access `message_id`, `queue_name`, `namespace`)
- [ ] Classification equivalence tests pass (same queue_name → same variant)
- [ ] Downstream compatibility verified (commands accept `MessageEvent`)
- [ ] Grep verification: no `pgmq_notify::MessageReadyEvent` in events.rs files

**Phase 1 Exit Criterion:** `cargo build --all-features` succeeds with zero changes to event_systems/*.rs

### Phase 2: Refactor Listeners

**Implementation:**
- [ ] Remove `PgmqNotifyListener` usage from `OrchestrationQueueListener`
- [ ] Remove `PgmqNotifyListener` usage from `WorkerQueueListener`
- [ ] Add `process_subscription_stream()` to handle `MessageNotification`
- [ ] Update `start()` to use `messaging_provider().subscribe()`
- [ ] Update `stop()` to cancel subscription handles
- [ ] Update `is_healthy()` to track stream state

**Validation Gates (see Phase 2 Validation Gates):**
- [ ] API surface unchanged (event systems compile without modification)
- [ ] Provider abstraction used (mock tests verify `subscribe()` called)
- [ ] MessageNotification handling works (Available and Message variants)
- [ ] Stats preservation verified (events_received increments)
- [ ] Lifecycle correctness verified (start/stop/is_running)
- [ ] Grep verification: no `PgmqNotifyListener` or `use pgmq_notify` in listener.rs files

*Note: E2E/integration tests deferred to TAS-133f*

**Phase 2 Exit Criterion:** Package-specific tests pass AND grep verifications return empty
```bash
cargo test -p tasker-orchestration --all-features -- listener
cargo test -p tasker-worker --all-features -- listener
# Both pass
```

### Post-Migration Validation (TAS-133e Scope)

**Package-Level Success:**
- [ ] `cargo test -p tasker-shared --all-features` passes
- [ ] `cargo test -p tasker-orchestration --all-features` passes
- [ ] `cargo test -p tasker-worker --all-features` passes
- [ ] `cargo build --all-features` succeeds

**Code Quality:**
- [ ] `cargo clippy --all-features` passes
- [ ] Grep verifications return empty (no pgmq_notify in domain types/listeners)

**Final Exit Criterion (TAS-133e):**
```bash
cargo build --all-features && \
cargo test -p tasker-shared --all-features && \
cargo test -p tasker-orchestration --all-features && \
cargo test -p tasker-worker --all-features && \
cargo clippy --all-features
```

### Deferred to TAS-133f (Migration Cleanup)

The following validation is explicitly **out of scope** for TAS-133e and will be addressed in subsequent work:

- [ ] Full E2E tests pass (`cargo test --all-features`)
- [ ] Fallback polling resilience (disable pg_notify → poller catches messages)
- [ ] RabbitMQ integration tests
- [ ] Legacy messaging cleanup (`tasker-shared/src/messaging/clients/` deletion)
- [ ] Provider switching via config only
- [ ] Performance benchmarks

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Classification regression | Low | High | Extensive test coverage, reuse existing logic |
| Stats drift | Low | Medium | Same stat fields, same increment points |
| Connection handling changes | Medium | Medium | Test reconnection scenarios explicitly |
| Performance regression | Low | Medium | Benchmark before/after |
| Channel backpressure issues | Low | Medium | TAS-51 monitoring unchanged |
| Field access breakage | Medium | Low | Compiler catches all usages, `MessageEvent` has same fields |

## Success Criteria (TAS-133e Scope)

Each criterion has a verification method to objectively assess completion.

| # | Criterion | Verification Method |
|---|-----------|---------------------|
| 1 | **Event Systems Unchanged** | `git diff` on `*_event_system.rs` shows zero changes |
| 2 | **Stats Structure Unchanged** | Same stat fields exist; compile-time verification |
| 3 | **Package Tests Pass** | `cargo test -p <package> --all-features` for each package |
| 4 | **Clean Types** | `grep pgmq_notify events.rs` returns empty in both crates |
| 5 | **Clean Listeners** | `grep PgmqNotifyListener listener.rs` returns empty |
| 6 | **Builds Clean** | `cargo build --all-features && cargo clippy --all-features` |

### How to Verify Each Criterion

**1. Event Systems Unchanged:**
```bash
git diff HEAD~N tasker-orchestration/src/orchestration/event_systems/
git diff HEAD~N tasker-worker/src/worker/event_systems/
# Both should show no changes (or only import path changes if any)
```

**2. Stats Structure Unchanged:**
```rust
// Same struct fields exist - compile-time verification
let stats: OrchestrationListenerStats = listener.stats();
let _ = stats.events_received;
let _ = stats.step_results_processed;
// etc.
```

**3. Package Tests Pass:**
```bash
cargo test -p tasker-shared --all-features
cargo test -p tasker-orchestration --all-features
cargo test -p tasker-worker --all-features
# All pass
```

**4. Clean Types:**
```bash
grep -r "pgmq_notify::MessageReadyEvent" tasker-orchestration/src/orchestration/orchestration_queues/events.rs
grep -r "pgmq_notify::MessageReadyEvent" tasker-worker/src/worker/worker_queues/events.rs
# Both return empty
```

**5. Clean Listeners:**
```bash
grep -r "PgmqNotifyListener" tasker-orchestration/src/orchestration/orchestration_queues/listener.rs
grep -r "PgmqNotifyListener" tasker-worker/src/worker/worker_queues/listener.rs
# Both return empty
```

**6. Builds Clean:**
```bash
cargo build --all-features
cargo clippy --all-features
# Both succeed with no errors
```

### Deferred Success Criteria (TAS-133f)

The following criteria are explicitly deferred to the migration cleanup phase:

| Criterion | Why Deferred |
|-----------|--------------|
| E2E tests pass | Requires full wiring, legacy cleanup |
| Provider agnostic switching | Requires RabbitMQ integration work |
| Fallback polling resilience | Requires E2E test infrastructure |
| Performance benchmarks | Meaningful after full integration |

## Appendix: Type Mappings

### Inner Event Type

| Current (PGMQ-specific) | Target (Provider-agnostic) |
|-------------------------|---------------------------|
| `pgmq_notify::MessageReadyEvent` | `tasker_shared::messaging::service::MessageEvent` |

**Field Mapping:**

| `MessageReadyEvent` | `MessageEvent` |
|---------------------|----------------|
| `msg_id: i64` | `message_id: MessageId` |
| `queue_name: String` | `queue_name: String` |
| `namespace: String` | `namespace: String` |

### Input Types (Subscription)

| Current (PGMQ-specific) | Target (Provider-agnostic) |
|-------------------------|---------------------------|
| `PgmqNotifyEvent::MessageReady` | `MessageNotification::Available` |
| `PgmqNotifyEvent::MessageWithPayload` | `MessageNotification::Message` or `Available` |
| `PgmqNotifyEvent::BatchReady` | N/A (informational only) |
| `PgmqNotifyEvent::QueueCreated` | N/A (informational only) |

### Output Types (Unchanged)

| Orchestration | Worker |
|---------------|--------|
| `OrchestrationNotification::Event(OrchestrationQueueEvent)` | `WorkerNotification::Event(WorkerQueueEvent)` |
| `OrchestrationNotification::ConnectionError(String)` | `WorkerNotification::ConnectionError(String)` |
| `OrchestrationNotification::Reconnected` | `WorkerNotification::Reconnected` |

### Classification (Unchanged, Inner Type Changes)

| Orchestration Queue | Event Type |
|---------------------|------------|
| `orchestration_step_results` | `StepResult(MessageEvent)` |
| `orchestration_task_requests` | `TaskRequest(MessageEvent)` |
| `orchestration_task_finalizations` | `TaskFinalization(MessageEvent)` |
| Other | `Unknown { ... }` |

| Worker Queue Pattern | Event Type |
|---------------------|------------|
| `*_queue` | `StepMessage(MessageEvent)` |
| `*_health` | `HealthCheck(MessageEvent)` |
| `*_config` | `ConfigurationUpdate(MessageEvent)` |
| Other | `Unknown { ... }` |

# Message Lifecycle Patterns

**Last Updated**: 2026-01-07

This document analyzes Tasker's current PGMQ message lifecycle patterns and maps them to RabbitMQ equivalents for the TAS-133 messaging abstraction.

---

## Executive Summary

Tasker uses a consistent **"Read → Evaluate → Delete on Success"** pattern across all message processing. Messages are read with a visibility timeout, processed, and only deleted upon successful completion. Failed processing leaves the message in the queue to be redelivered after the visibility timeout expires.

This pattern maps cleanly to RabbitMQ's acknowledgment model, but with important semantic differences that the abstraction must handle.

---

## Current PGMQ Patterns

### Pattern 1: Event-Driven Processing (Primary Path)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    EVENT-DRIVEN MESSAGE LIFECYCLE                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   1. pg_notify fires (message ready)                                         │
│      └── Contains: msg_id, queue_name, namespace                            │
│                                                                              │
│   2. read_specific_message(msg_id, vtt=30s)                                  │
│      └── Message becomes invisible to other consumers                        │
│      └── Returns full message payload                                        │
│                                                                              │
│   3. Hydrate & Process                                                       │
│      └── Deserialize payload                                                 │
│      └── Load additional context from database                               │
│      └── Execute business logic (state transitions, handler dispatch, etc.)  │
│                                                                              │
│   4. Outcome-Based Cleanup                                                   │
│      ├── SUCCESS: delete_message(msg_id)                                     │
│      │   └── Message permanently removed                                     │
│      │                                                                       │
│      └── FAILURE: (no action)                                                │
│          └── Message reappears after visibility timeout                      │
│          └── Will be reprocessed by event or fallback poller                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Pattern 2: Fallback Polling (Safety Net)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    FALLBACK POLLING MESSAGE LIFECYCLE                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   1. Polling interval fires (e.g., every 30 seconds)                         │
│                                                                              │
│   2. read_messages(queue, vtt=30s, batch_size=50)                            │
│      └── Returns batch of visible messages                                   │
│      └── All returned messages become invisible                              │
│                                                                              │
│   3. For each message: Hydrate & Process                                     │
│      └── Same processing logic as event-driven path                          │
│                                                                              │
│   4. Outcome-Based Cleanup (per message)                                     │
│      ├── SUCCESS: delete_message(msg_id)                                     │
│      └── FAILURE: (no action) → retry after VTT expiry                       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Code Reference: Orchestration Side

### Step Result Processing

**Location**: `tasker-orchestration/src/orchestration/command_processor.rs`

```rust
async fn handle_step_result_from_message(
    &self,
    queue_name: &str,
    message: PgmqMessage,
) -> TaskerResult<StepProcessResult> {
    // 1. Hydrate: Transform lightweight message to full StepExecutionResult
    let step_execution_result = self
        .step_result_hydrator
        .hydrate_from_message(&message)
        .await?;

    // 2. Process: Delegate to result processor
    let result = self
        .handle_process_step_result(step_execution_result.clone())
        .await;

    // 3. Delete on Success
    match &result {
        Ok(_) => {
            // SUCCESS PATH: Delete the message
            match self.pgmq_client.delete_message(queue_name, message.msg_id).await {
                Ok(_) => info!("Successfully deleted processed message"),
                Err(e) => warn!("Failed to delete message (will be reprocessed): {}", e),
            }
        }
        Err(_) => {
            // FAILURE PATH: Message remains, will be redelivered after VTT
            error!("Result processing failed (message will remain for retry)");
        }
    }

    result
}
```

### Task Initialization

```rust
async fn handle_task_initialize_from_message(
    &self,
    queue_name: &str,
    message: PgmqMessage,
) -> TaskerResult<TaskInitializeResult> {
    // 1. Hydrate
    let task_request = self.task_request_hydrator.hydrate_from_message(&message).await?;

    // 2. Process
    let result = self.handle_initialize_task(task_request).await;

    // 3. Delete on Success
    match &result {
        Ok(_) => {
            self.pgmq_client.delete_message(queue_name, message.msg_id).await?;
        }
        Err(_) => {
            // Message remains for retry
        }
    }

    result
}
```

### Task Finalization

```rust
async fn handle_task_finalize_from_message(
    &self,
    queue_name: &str,
    message: PgmqMessage,
) -> TaskerResult<TaskFinalizationResult> {
    // 1. Hydrate
    let task_uuid = self.finalization_hydrator.hydrate_from_message(&message).await?;

    // 2. Process
    let result = self.handle_finalize_task(task_uuid).await;

    // 3. Delete ONLY on Success (not NotClaimed or Failed)
    if matches!(result, Ok(TaskFinalizationResult::Success { .. })) {
        self.pgmq_client.delete_message(queue_name, message.msg_id).await?;
    }

    result
}
```

---

## Code Reference: Worker Side

### Step Execution

**Location**: `tasker-worker/src/worker/actors/step_executor_actor.rs`

The worker has an additional **claim** step before dispatch:

```rust
#[async_trait]
impl Handler<ExecuteStepFromEventMessage> for StepExecutorActor {
    async fn handle(&self, msg: ExecuteStepFromEventMessage) -> TaskerResult<()> {
        // 1. Read specific message (with VTT)
        let message = self.context.message_client
            .read_specific_message::<SimpleStepMessage>(
                &msg.message_event.queue_name,
                msg.message_event.msg_id,
                msg.message_event.visibility_timeout_seconds.unwrap_or(30),
            )
            .await?;

        match message {
            Some(m) => {
                let step_uuid = m.message.step_uuid;
                let task_uuid = m.message.task_uuid;

                // 2. Claim and dispatch (includes capacity check, step claiming, handler dispatch)
                let claimed = self
                    .claim_and_dispatch(step_uuid, task_uuid, correlation_id, trace_context)
                    .await?;

                // 3. Delete on successful claim+dispatch
                if claimed {
                    self.context.message_client
                        .delete_message(&msg.message_event.queue_name, m.msg_id)
                        .await?;
                }
                // If not claimed: message remains, VTT will expire, retry occurs
            }
            None => {
                // Message already processed by another consumer
            }
        }

        Ok(())
    }
}
```

### Claim and Dispatch Pattern

```rust
async fn claim_and_dispatch(
    &self,
    step_uuid: Uuid,
    task_uuid: Uuid,
    correlation_id: Uuid,
    trace_context: Option<TraceContext>,
) -> TaskerResult<bool> {
    // TAS-75: Load shedding - check capacity before claiming
    let (has_capacity, utilization) = self.check_capacity();
    if !has_capacity {
        // Return false WITHOUT claiming - VTT will expire and retry
        return Ok(false);
    }

    // Hydrate step context from database
    let task_sequence_step = step_claimer
        .get_task_sequence_step_by_uuid(step_uuid)
        .await?;

    // Claim the step (state machine transition)
    let claimed = step_claimer
        .try_claim_step(&task_sequence_step, correlation_id)
        .await?;

    if !claimed {
        // Step already claimed by another worker
        return Ok(false);
    }

    // Dispatch to handler (fire-and-forget)
    self.dispatch_sender.try_send(msg)?;

    Ok(true)
}
```

---

## Critical Insight: Two Layers of "Claiming"

Tasker has **two distinct claiming mechanisms**:

### 1. Message-Level Claiming (PGMQ Visibility Timeout)

```
Consumer A reads message → Message invisible for 30s
                           ↓
                     If Consumer A crashes
                           ↓
                     After 30s: Message visible again
                           ↓
                     Consumer B can read it
```

### 2. Domain-Level Claiming (Step State Machine)

```
Worker A claims step → Step state: "pending" → "in_progress" (claimed_by: Worker A)
                       ↓
                 If Worker B tries to claim
                       ↓
                 State machine rejects (already in_progress)
                       ↓
                 Worker B skips, message deleted anyway
```

**This is defense in depth**: Even if PGMQ delivers the same message to two consumers (e.g., during network partition recovery), only one can claim the step at the domain level.

---

## RabbitMQ Translation

### Semantic Mapping

| PGMQ Operation | RabbitMQ Equivalent | Notes |
|----------------|---------------------|-------|
| `read_specific_message(msg_id, vtt)` | `basic_get` + ack deadline | RabbitMQ doesn't have per-message VTT; use consumer timeout |
| `read_messages(batch_size, vtt)` | `basic_consume` with prefetch | Messages stay unacked until explicit ack |
| `delete_message(msg_id)` | `basic_ack(delivery_tag)` | Permanent removal |
| VTT expiry (implicit requeue) | `basic_nack(requeue=true)` or consumer timeout | RabbitMQ requires explicit nack OR consumer death |
| No action on failure | Consumer timeout OR explicit `basic_nack` | Different failure semantics |

### Key Semantic Differences

#### PGMQ: Implicit Requeue on VTT Expiry

```rust
// PGMQ: If we don't delete within VTT, message automatically requeues
let msg = pgmq.read_message("queue", vtt=30).await?;
// ... processing crashes ...
// After 30s: message automatically visible again
```

#### RabbitMQ: Explicit Acknowledgment Required

```rust
// RabbitMQ: Must explicitly ack OR nack
let delivery = consumer.next().await?;
// ... processing ...
if success {
    channel.basic_ack(delivery.delivery_tag, false).await?;
} else {
    // MUST explicitly nack to requeue
    channel.basic_nack(delivery.delivery_tag, false, true).await?;  // requeue=true
}
// If consumer crashes without ack/nack: depends on consumer timeout config
```

### Abstraction Strategy

The `MessagingService` trait should handle this difference internally:

```rust
pub trait MessagingService: Send + Sync + 'static {
    /// Acknowledge successful processing (PGMQ: delete, RabbitMQ: ack)
    async fn ack_message(
        &self, 
        queue_name: &str, 
        receipt_handle: &ReceiptHandle
    ) -> Result<(), MessagingError>;
    
    /// Negative acknowledge - return to queue for retry
    /// PGMQ: No-op (VTT handles this)
    /// RabbitMQ: basic_nack with requeue=true
    async fn nack_message(
        &self, 
        queue_name: &str, 
        receipt_handle: &ReceiptHandle,
        requeue: bool
    ) -> Result<(), MessagingError>;
}
```

**Usage pattern remains the same**:

```rust
// Abstract pattern (works for both PGMQ and RabbitMQ)
let messages = messaging.receive_messages("queue", 10, Duration::from_secs(30)).await?;

for msg in messages {
    match process_message(&msg).await {
        Ok(_) => {
            messaging.ack_message("queue", &msg.receipt_handle).await?;
        }
        Err(e) if e.is_retryable() => {
            messaging.nack_message("queue", &msg.receipt_handle, true).await?;
        }
        Err(e) => {
            // Dead letter or discard
            messaging.nack_message("queue", &msg.receipt_handle, false).await?;
        }
    }
}
```

---

## Receipt Handle Abstraction

PGMQ and RabbitMQ track messages differently:

```rust
/// Provider-agnostic message receipt
/// 
/// PGMQ: Contains msg_id (i64)
/// RabbitMQ: Contains delivery_tag (u64) and channel reference
#[derive(Debug, Clone)]
pub struct ReceiptHandle {
    /// Opaque bytes containing provider-specific tracking data
    inner: Vec<u8>,
}

impl ReceiptHandle {
    /// Create from PGMQ message ID
    pub fn from_pgmq(msg_id: i64) -> Self {
        Self { inner: msg_id.to_le_bytes().to_vec() }
    }
    
    /// Create from RabbitMQ delivery tag
    pub fn from_rabbitmq(delivery_tag: u64, channel_id: u16) -> Self {
        let mut inner = Vec::with_capacity(10);
        inner.extend_from_slice(&delivery_tag.to_le_bytes());
        inner.extend_from_slice(&channel_id.to_le_bytes());
        Self { inner }
    }
    
    /// Extract PGMQ message ID
    pub fn as_pgmq_msg_id(&self) -> Option<i64> {
        if self.inner.len() == 8 {
            Some(i64::from_le_bytes(self.inner[..8].try_into().ok()?))
        } else {
            None
        }
    }
    
    /// Extract RabbitMQ delivery info
    pub fn as_rabbitmq_delivery(&self) -> Option<(u64, u16)> {
        if self.inner.len() == 10 {
            let tag = u64::from_le_bytes(self.inner[..8].try_into().ok()?);
            let channel = u16::from_le_bytes(self.inner[8..10].try_into().ok()?);
            Some((tag, channel))
        } else {
            None
        }
    }
}
```

---

## Visibility Timeout vs Consumer Timeout

### PGMQ Visibility Timeout

- Set per-read operation
- Message invisible for exactly `vtt` seconds
- After expiry: message visible to any consumer
- No explicit requeue needed

### RabbitMQ Consumer Timeout

RabbitMQ has multiple timeout mechanisms:

1. **Consumer Timeout** (`consumer_timeout`): If consumer doesn't ack within timeout, connection closed
2. **Delivery Acknowledgement Timeout**: Similar, per-delivery
3. **Heartbeat Timeout**: Connection-level health check

**Recommended RabbitMQ configuration for Tasker semantics**:

```toml
[messaging.rabbitmq]
# Consumer timeout should match Tasker's typical VTT
consumer_timeout_ms = 30000

# Prefetch = 1 ensures we don't grab more than we can process
prefetch_count = 1

# Or higher prefetch with careful ack management
# prefetch_count = 10
```

---

## Error Handling Patterns

### Current PGMQ Pattern

```rust
match process_message(&msg).await {
    Ok(_) => {
        // Happy path: delete
        pgmq.delete_message(queue, msg.msg_id).await?;
    }
    Err(e) => {
        // Sad path: log and let VTT handle requeue
        error!("Processing failed: {}", e);
        // Message will reappear after VTT expiry
    }
}
```

### Required RabbitMQ Pattern

```rust
match process_message(&msg).await {
    Ok(_) => {
        channel.basic_ack(delivery_tag, false).await?;
    }
    Err(e) if e.is_retryable() => {
        // Explicit requeue
        channel.basic_nack(delivery_tag, false, true).await?;
    }
    Err(e) => {
        // Send to dead letter (don't requeue)
        channel.basic_nack(delivery_tag, false, false).await?;
    }
}
```

### Abstracted Pattern (TAS-133 Goal)

```rust
// Handler code doesn't change between providers
match process_message(&msg).await {
    Ok(_) => {
        messaging.ack_message(queue, &msg.receipt_handle).await?;
    }
    Err(e) => {
        // Abstraction handles provider-specific nack/requeue semantics
        messaging.nack_message(queue, &msg.receipt_handle, e.is_retryable()).await?;
    }
}
```

---

## Implementation Implications for TAS-133

### 1. Current Code Changes Required

**Minimal**: The current pattern of "process then delete on success" maps directly to "process then ack on success". The abstraction layer handles the semantic translation.

### 2. Explicit Nack Consideration

Current Tasker code **doesn't explicitly nack** - it relies on VTT expiry. For RabbitMQ:

**Option A**: Abstract away nack entirely
- PGMQ impl: no-op
- RabbitMQ impl: auto-nack on Drop if not acked

**Option B**: Add explicit nack to pattern
- More explicit error handling
- Better for dead letter routing

**Recommendation**: Option A for backward compatibility, Option B as opt-in enhancement.

### 3. Receipt Handle Lifecycle

Receipt handles must be valid until ack/nack. This is naturally handled by:
- PGMQ: msg_id is just an i64, always valid
- RabbitMQ: delivery_tag valid until channel closes

### 4. Batch Acknowledgment

PGMQ deletes are individual. RabbitMQ can batch ack (`basic_ack` with `multiple=true`).

Consider adding to trait:
```rust
async fn ack_messages(&self, queue: &str, handles: &[ReceiptHandle]) -> Result<(), MessagingError>;
```

---

## Summary: What Must Translate

| PGMQ Behavior | RabbitMQ Equivalent | Abstraction Method |
|---------------|---------------------|-------------------|
| `read_message` with VTT | `basic_consume` with prefetch | `receive_messages` |
| `read_specific_message` | Not directly available | `receive_by_id` (PGMQ-only?) or use push model |
| `delete_message` | `basic_ack` | `ack_message` |
| VTT expiry requeue | `basic_nack` or consumer timeout | `nack_message` |
| No explicit failure handling | Explicit nack required | Abstraction handles on Drop or explicit call |

The abstraction is viable because the fundamental pattern - "read, process, acknowledge on success" - is the same. The differences are in failure handling and can be abstracted cleanly.

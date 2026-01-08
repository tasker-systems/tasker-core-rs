# TAS-132: PGMQ Message Groups Analysis

## Executive Summary

**RECOMMENDATION**: Adopt a **hybrid approach** that uses queue-per-namespace for domain isolation while leveraging message groups *within* namespaces for sub-task ordering guarantees (e.g., DAG step ordering, entity-specific processing order).

The two concepts serve different architectural purposes and complement rather than replace each other.

---

## Background

### What Are PGMQ Message Groups?

PGMQ recently (December 2025) stabilized a "message groups" feature that provides FIFO ordering guarantees within logical groups of messages in a single queue. This is analogous to AWS SQS FIFO queues with `MessageGroupId`.

**Key characteristics:**
- Messages are tagged with a group identifier via the `x-pgmq-group` header
- Two read patterns available:
  - `read_grouped` (SQS-style): Fills batch from earliest group first, maximizes throughput for related messages
  - `read_grouped_rr` (round-robin): Fair distribution across groups, prevents starvation
- **Critical guarantee**: Once one or more messages from a group are read, other transactions cannot read messages from the same group until those messages are deleted, archived, or their visibility timeout expires
- Uses advisory locks (`pg_try_advisory_xact_lock`) for concurrency control

### Current Tasker Architecture: Queue-per-Namespace

Tasker currently uses a **queue-per-namespace** pattern:

```
worker_payment_reconciliation_queue
worker_etl_workflows_queue  
worker_user_notifications_queue
```

Queue names follow the pattern `worker_{namespace}_queue`, with namespace extraction via regex `(?P<namespace>\w+)_queue`.

**Rationale behind this design:**
- Namespaces are declarative of intent (domain boundaries)
- Independent scaling/monitoring per domain
- Natural isolation for different workload characteristics
- Separate queue metrics, archiving, and partitioning strategies

---

## Analysis: Queue-per-Namespace vs Message Groups

### What Each Pattern Provides

| Aspect | Queue-per-Namespace | Message Groups |
|--------|---------------------|----------------|
| **Primary Purpose** | Domain isolation, workload separation | Ordering guarantees within a workload |
| **Isolation Unit** | Business domain / workflow type | Entity or logical sequence |
| **Scaling Model** | Add consumers per domain | Add consumers, groups auto-distribute |
| **Metrics Granularity** | Per-domain queue metrics | Single queue, must filter by group |
| **Partition Management** | Separate pg_partman config per queue | Single partitioning strategy |
| **Failure Isolation** | Queue issues don't affect other domains | Group stalls don't block other groups |
| **Operational Complexity** | More queues to manage | More complex read patterns |

### Key Insight: Orthogonal Concerns

**Namespaces and message groups solve different problems:**

1. **Namespaces** answer: "What *domain* does this work belong to?"
   - Payment reconciliation vs ETL vs notifications
   - Different handler types, different SLAs
   - Different operational teams may own different namespaces

2. **Message Groups** answer: "What *ordering constraint* applies within a domain?"
   - Steps within a single task must execute in DAG order
   - All events for customer X must process in sequence
   - Batch records for entity Y must maintain order

---

## Tasker Use Cases for Message Groups

### Use Case 1: Step Ordering Within Tasks (High Value)

**Problem**: Currently, DAG step ordering is enforced at the orchestration layer, not the queue layer. If multiple steps for the same task become ready simultaneously, there's no queue-level guarantee they'll be processed in order.

**Solution with Message Groups**:
```rust
// When enqueueing a step, set group = task_uuid
let headers = json!({ "x-pgmq-group": task.task_uuid.to_string() });
pgmq.send_with_headers(queue_name, message, headers).await?;
```

This ensures:
- All steps for `task_abc123` are processed in FIFO order
- Parallel tasks (`task_def456`) can execute concurrently
- No orchestration-layer coordination needed for ordering

### Use Case 2: Entity-Specific Processing Order (Medium Value)

**Problem**: Batchable handlers process entities (customers, orders, etc.). Sometimes entity-level ordering matters:
- Customer A's records must process in order
- But Customer B's records can process in parallel with A's

**Solution**:
```rust
// Group by entity identifier
let headers = json!({ "x-pgmq-group": format!("customer:{}", customer_id) });
```

### Use Case 3: Priority Within Namespace (Lower Value)

**Problem**: High-priority tasks compete with low-priority tasks in the same namespace queue.

**Solution**: Could use groups like `priority:urgent`, `priority:normal`, combined with `read_grouped_rr` for fair distribution.

**Caveat**: This may be better solved with separate priority queues rather than groups.

---

## Recommended Architecture: Hybrid Model

### Layer 1: Queue-per-Namespace (Domain Isolation)

Retain the current pattern. Each namespace gets its own queue:

```
worker_payment_reconciliation_queue
worker_etl_workflows_queue
```

**Benefits preserved:**
- Domain-level isolation and monitoring
- Independent scaling characteristics  
- Namespace-specific queue configuration (partitioning, retention)
- Clear operational boundaries

### Layer 2: Message Groups Within Queues (Ordering Guarantees)

Use message groups for sub-namespace ordering requirements:

```
Queue: worker_etl_workflows_queue
├── Group: "task:abc123" → Steps for task abc123 (FIFO)
├── Group: "task:def456" → Steps for task def456 (FIFO)
└── Group: "_default_fifo_group" → Ungrouped messages
```

### Implementation Approach

```rust
// In messaging abstraction (TAS-133)
pub struct EnqueueOptions {
    pub queue_name: String,
    pub message: StepMessage,
    pub group_key: Option<String>,  // NEW: Optional ordering group
    pub delay_seconds: Option<u64>,
}

// The PGMQ implementation sets the header
impl MessageClient for PgmqMessageClient {
    async fn send_step_message(&self, options: EnqueueOptions) -> TaskerResult<i64> {
        let headers = match options.group_key {
            Some(key) => Some(json!({ "x-pgmq-group": key })),
            None => None,
        };
        
        self.pgmq.send_with_headers(
            &options.queue_name,
            &options.message,
            headers
        ).await
    }
}
```

### Read Pattern Selection

For Tasker's use case, **`read_grouped_rr` (round-robin)** is likely preferred:

- Prevents a large task with many steps from starving other tasks
- Fair distribution aligns with Tasker's DAG execution model
- Tasks are independent; don't want one task to monopolize workers

However, the choice should be configurable per namespace:

```toml
[namespace.etl_workflows]
message_group_read_pattern = "round_robin"  # fair distribution

[namespace.batch_processing]  
message_group_read_pattern = "batch"  # maximize throughput per entity
```

---

## Interaction with Related Tickets

### TAS-133: Messaging Service Strategy Pattern

Message groups should be abstracted at the `MessageClient` trait level:

```rust
#[async_trait]
pub trait MessageClient: Send + Sync {
    // Add group support
    async fn send_step_message_grouped(
        &self, 
        namespace: &str, 
        message: StepMessage,
        group_key: &str,
    ) -> TaskerResult<()>;
    
    // Grouped read with pattern selection
    async fn receive_step_messages_grouped(
        &self,
        namespace: &str,
        limit: i32,
        visibility_timeout: i32,
        pattern: GroupReadPattern,
    ) -> TaskerResult<Vec<StepMessage>>;
}

pub enum GroupReadPattern {
    Standard,    // pgmq.read() - no grouping
    RoundRobin,  // pgmq.read_grouped_rr()
    Batch,       // pgmq.read_grouped()
}
```

**Important**: RabbitMQ doesn't have exact equivalent semantics. The abstraction should:
- Make grouping optional (default to standard read)
- Allow PGMQ-specific optimizations when available
- Provide RabbitMQ approximation via routing keys (different semantics, document limitations)

### TAS-78: PGMQ Separate Database

No direct interaction, but note that message groups are an extension feature that would run on the PGMQ database, not the Tasker application database.

---

## Implementation Considerations

### PGMQ Version Requirements

- Feature merged: December 29, 2025 (very recent)
- Available in PGMQ 1.8.0+
- Functions: `pgmq.read_grouped()`, `pgmq.read_grouped_rr()`
- Header: `x-pgmq-group`

**Action**: Verify Tasker's PGMQ dependency version supports groups before implementation.

### Advisory Lock Implications

Message groups use `pg_try_advisory_xact_lock` for concurrency control. This is transaction-scoped and shouldn't conflict with Tasker's other lock usage, but verify there's no collision in lock key space.

### Index Considerations

Groups add query complexity. For high-throughput queues, consider:
- The index on `(x-pgmq-group header, msg_id)` is created automatically
- Monitor query plans for grouped reads
- May need additional indexes for large queues with many groups

### Migration Path

1. **Phase 1**: Add optional `group_key` to `MessageClient` trait (backward compatible)
2. **Phase 2**: Implement PGMQ-specific grouped send/receive
3. **Phase 3**: Use groups for task step ordering (high value, low risk)
4. **Phase 4**: Consider entity-level grouping for specific use cases

---

## Decision Matrix

| Question | Answer |
|----------|--------|
| Should we replace namespace queues with message groups? | **No** - they serve different purposes |
| Should we use message groups at all? | **Yes** - for ordering within namespaces |
| What's the primary use case? | Task step ordering (group = task_uuid) |
| Which read pattern? | Round-robin (`read_grouped_rr`) for fairness |
| When to implement? | After TAS-133 (messaging abstraction provides clean integration point) |

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| PGMQ version too new, might have bugs | Feature has been in development since June 2025, merged after extensive review. Monitor PGMQ issues. |
| Advisory locks could conflict | Use distinct lock key namespace, test thoroughly |
| Grouped reads slower than standard | Benchmark; only use groups where ordering matters |
| Abstraction leaky for RabbitMQ | Document limitations clearly; groups are PGMQ optimization |
| Operational complexity | Groups are optional; can disable per-namespace |

---

## Success Criteria

1. ✅ Task steps execute in correct DAG order without orchestration-layer coordination
2. ✅ Large tasks don't starve other tasks (round-robin distribution)
3. ✅ No regression in throughput for non-grouped messages
4. ✅ `MessageClient` trait cleanly abstracts grouped vs non-grouped operations
5. ✅ Metrics available for group-level monitoring (stretch goal)

---

## References

- [PGMQ PR #442: Message Groups Implementation](https://github.com/pgmq/pgmq/pull/442)
- [PGMQ Issue #294: Original FIFO + Message Keys Discussion](https://github.com/tembo-io/pgmq/issues/294)
- [AWS SQS FIFO Documentation](https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/FIFO-queues.html) (conceptual reference)
- Tasker TAS-133: Messaging Service Strategy Pattern
- Tasker TAS-78: PGMQ Separate Database Support

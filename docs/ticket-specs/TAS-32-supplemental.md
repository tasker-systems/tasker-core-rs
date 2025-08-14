# TAS-32 Supplemental: Visual Architecture Comparison

This document provides visual diagrams comparing the current queue-based workflow processing with the proposed TAS-32 improvements for queue state management.

## Current Architecture (Problem State)

### Current Workflow State Diagram

```mermaid
stateDiagram-v2
    [*] --> pending: Task/Step Created
    
    pending --> in_progress: Orchestration enqueues step
    
    state in_progress {
        [*] --> message_enqueued: Step message sent to queue
        message_enqueued --> message_visible: Visibility timeout expires
        message_visible --> worker_claims: Worker reads message
        worker_claims --> worker_deletes: Worker deletes message immediately
        worker_deletes --> actually_processing: Worker starts processing
        actually_processing --> [*]: Processing complete
    }
    
    in_progress --> complete: Success result received
    in_progress --> error: Error result received
    in_progress --> cancelled: Manual cancellation
    
    complete --> [*]
    error --> pending: Retry (backoff)
    error --> [*]: Permanent failure
    cancelled --> [*]

    note right of in_progress: PROBLEM - Step marked in_progress when actually just enqueued. Workers must aggressively delete messages before processing to prevent double-processing. Queue message presence becomes source of truth for state.
```

### Current Message Flow (Complex Messages)

```mermaid
sequenceDiagram
    participant O as Orchestration (Rust)
    participant Q as fulfillment_queue (pgmq)
    participant W as Worker (Ruby)
    participant DB as Database (PostgreSQL)
    participant H as Step Handler (Ruby)
    participant RQ as orchestration_step_results (pgmq)

    Note over O,RQ: Current Complex Message Architecture (PROBLEMATIC)
    
    O->>DB: 1. Mark step as "in_progress"
    Note right of O: WRONG STATE - Step not actually processing
    
    O->>O: 2. Build complex execution context
    Note right of O: 600+ lines of serialization
    
    O->>Q: 3. Enqueue complex message
    Note right of Q: Large message payload ~80% larger than needed
    
    W->>Q: 4. Read message (visibility timeout)
    W->>Q: 5. DELETE message immediately
    Note right of W: AGGRESSIVE DELETE - Before processing starts!
    
    W->>W: 6. Complex hash-to-object conversion
    Note right of W: 300+ lines of type conversion, Fragile dry-struct parsing
    
    W->>H: 7. Call handler with converted objects
    H->>H: 8. Execute business logic
    H->>W: 9. Return result
    
    W->>RQ: 10. Publish result with serialized data
    Note right of RQ: Results duplicated in queue
    
    O->>RQ: 11. Read result message
    O->>DB: 12. Persist results & update state
    O->>DB: 13. Mark step as "complete"
```

### Current Architecture Issues

#### State Management Problems
1. **Misleading State**: Steps marked `in_progress` when only `enqueued`
2. **Queue as State Source**: Message presence indicates processing state
3. **Race Conditions**: Multiple workers can claim if deletion fails
4. **Aggressive Deletion**: Messages deleted before processing starts

#### Message Complexity Issues
1. **Large Payloads**: 600+ lines of complex serialization
2. **Type Conversion**: Hash-to-object transformation fragility
3. **Duplicated Data**: Results serialized in both database and queue
4. **Complex Dependencies**: Nested execution context serialization

---

## Proposed Architecture (TAS-32 Solution)

### Proposed Workflow State Diagram

```mermaid
stateDiagram-v2
    [*] --> pending: Task/Step Created
    
    pending --> enqueued: Orchestration enqueues step
    
    state enqueued {
        [*] --> message_sent: Simple message sent to queue
        message_sent --> message_visible: Visibility timeout expires
        message_visible --> [*]: Ready for worker claiming
    }
    
    enqueued --> in_progress: Worker claims step
    
    state in_progress {
        [*] --> worker_claimed: Worker transitions state in DB
        worker_claimed --> worker_deletes: Worker deletes queue message
        worker_deletes --> actually_processing: Worker processes step
        actually_processing --> results_persisted: Worker saves results to DB
        results_persisted --> [*]: Processing complete
    }
    
    in_progress --> complete: Worker marks complete
    in_progress --> retryable_error: Worker marks retryable error
    in_progress --> permanent_error: Worker marks permanent error
    in_progress --> cancelled: Manual cancellation
    
    complete --> [*]
    retryable_error --> pending: Orchestration retries (with backoff)
    permanent_error --> [*]: Task fails
    cancelled --> [*]

    note right of enqueued: CORRECT STATE - Step is enqueued not processing. Database is source of truth for processing state. Workers can safely claim steps without race conditions.
    
    note right of in_progress: ACTUAL PROCESSING - Worker has claimed step and is actively processing. Results persisted directly to database by worker.
```

### Proposed Message Flow (Simple Messages)

```mermaid
sequenceDiagram
    participant O as Orchestration (Rust)
    participant Q as fulfillment_queue (pgmq)
    participant W as Worker (Ruby)
    participant DB as Database (PostgreSQL)
    participant H as Step Handler (Ruby)
    participant RQ as orchestration_step_results (pgmq)

    Note over O,RQ: Proposed Simple Message Architecture (TAS-32)
    
    O->>DB: 1. Mark step as "enqueued"
    Note right of O: CORRECT STATE - Step is enqueued not processing
    
    O->>Q: 2. Enqueue simple message
    Note right of Q: Tiny message payload ~80% size reduction
    
    W->>Q: 3. Read message (visibility timeout)
    
    W->>DB: 4. Claim step - enqueued to in_progress
    Note right of W: DATABASE CLAIM - Atomic state transition
    
    W->>Q: 5. Delete message after successful claim
    Note right of W: SAFE DELETE - After claiming ownership
    
    W->>DB: 6. Fetch task step dependencies by UUID
    Note right of W: Real ActiveRecord models, No type conversion needed
    
    W->>H: 7. Call handler with AR models
    H->>H: 8. Execute business logic
    H->>W: 9. Return result
    
    W->>DB: 10. Persist results directly
    W->>DB: 11. Mark step as complete
    Note right of W: WORKER PERSISTENCE - No queue serialization
    
    W->>RQ: 12. Send completion signal
    Note right of RQ: Tiny signal message, No result data
    
    O->>RQ: 13. Read completion signal
    O->>DB: 14. Query updated step state
    Note right of O: Database is source of truth
```

---

## State Comparison Table

| Aspect | Current (Problematic) | Proposed (TAS-32) |
|--------|----------------------|-------------------|
| **Orchestration Transition** | `pending → in_progress` | `pending → enqueued` |
| **Worker Transition** | None (already "in_progress") | `enqueued → in_progress` |
| **State Source of Truth** | Queue message presence | Database state |
| **Message Size** | Large (complex context) | Small (3 UUIDs) |
| **Data Fetching** | Serialized in message | ActiveRecord queries |
| **Result Persistence** | Orchestration handles | Worker handles directly |
| **Idempotency** | Race conditions possible | Database-enforced |
| **Message Deletion** | Before processing | After claiming |

---

## Key Architectural Benefits

### 1. Proper State Semantics
- **Current**: "in_progress" means "enqueued" (confusing)
- **Proposed**: "enqueued" means "enqueued", "in_progress" means "processing"

### 2. Idempotency Guarantee
- **Current**: Queue message presence determines eligibility
- **Proposed**: Database state determines eligibility (atomic)

### 3. Simplified Messages
- **Current**: Complex nested JSON with execution context
- **Proposed**: Simple 3-UUID structure (80% size reduction)

### 4. ActiveRecord Integration  
- **Current**: Hash-to-object conversion fragility
- **Proposed**: Real ActiveRecord models with full ORM functionality

### 5. Clear Responsibility
- **Current**: Orchestration handles result persistence
- **Proposed**: Workers handle their own result persistence

---

## Implementation Impact Analysis

### Code Reduction Estimates

| Component | Current Lines | Proposed Lines | Reduction |
|-----------|---------------|----------------|-----------|
| **Ruby Message Processing** | ~825 lines | ~300 lines | ~525 lines |
| **Ruby Step Message Types** | ~525 lines | ~150 lines | ~375 lines |
| **Ruby Handler Registry** | ~271 lines | ~100 lines | ~171 lines |
| **Rust Message Creation** | ~300 lines | ~100 lines | ~200 lines |
| **Total** | ~1,921 lines | ~650 lines | **~1,271 lines** |

### Performance Benefits

1. **Message Size**: 80% reduction (3 UUIDs vs complex JSON)
2. **Serialization**: Eliminated complex object serialization
3. **Database Queries**: Efficient UUID lookups with proper indexes
4. **Memory Usage**: No complex object graphs in messages
5. **Network Overhead**: Smaller message payloads

### Risk Mitigation

1. **Database Load**: Offset by eliminating complex serialization
2. **Query Efficiency**: UUID indexes provide fast lookups
3. **ActiveRecord Queries**: Can use includes/joins for efficient loading
4. **Stale Messages**: UUID-based lookup prevents processing wrong records

---

## Testing Strategy

### Integration Test Updates Required

1. **State Transition Tests**: Verify `pending → enqueued → in_progress` flow
2. **Message Format Tests**: Update for simple UUID-based messages
3. **Handler Tests**: Verify handlers work with ActiveRecord models
4. **Idempotency Tests**: Verify multiple workers cannot claim same step
5. **Error Handling Tests**: Verify graceful handling of missing records

### Success Criteria

1. ✅ All existing workflow integration tests pass
2. ✅ Message payload size reduced by >80%
3. ✅ No race conditions in step claiming
4. ✅ Handlers receive proper ActiveRecord models
5. ✅ End-to-end workflow completion works reliably

---

This architectural change addresses the core issues identified in TAS-32 while maintaining compatibility with existing handler interfaces and dramatically simplifying the message processing pipeline.
# TAS-67 Foundational Context

**Last Updated**: 2025-12-06
**Purpose**: Establish the core Tasker design principles that inform the behavior mapping and edge case analysis for TAS-67.

---

## 1. Idempotency and Atomicity (Defense-in-Depth Architecture)

### Four-Layer Protection Model

1. **Database Atomicity**:
   - Unique constraints prevent duplicate task creation (`identity_hash`)
   - Row-level locking (`FOR UPDATE`, `FOR UPDATE SKIP LOCKED`)
   - Compare-and-swap semantics in UPDATE WHERE clauses
   - All critical operations wrapped in database transactions

2. **State Machine Guards**:
   - Current state validation before all transitions
   - TaskStateMachine and StepStateMachine enforce valid transitions
   - Terminal state protection prevents invalid re-activation
   - Processor UUID tracked for audit (not enforced after TAS-54)

3. **Transaction Boundaries**:
   - Task initialization is atomic: namespace + task + steps + edges
   - Cycle detection validates DAG before committing
   - Rollback on any failure prevents partial state

4. **Application Logic**:
   - Find-or-create pattern handles concurrent entity creation
   - State-based filtering naturally deduplicates work
   - **State-Before-Queue Pattern (TAS-29)**: Commit state change FIRST, THEN send PGMQ notification

### Key Protection: TAS-54 Removed Processor Ownership Enforcement

- Processor UUID stored for audit trail only
- Not enforced in state transitions
- Enables automatic recovery after orchestrator crashes
- Idempotency guaranteed by state guards + transaction atomicity + atomic claiming

### Operational Impact

- Operations are safely retryable without coordination
- Multiple orchestrators can process work concurrently
- Crashes don't require manual intervention
- All-or-nothing semantics for complex operations

---

## 2. States and Lifecycles (12-Task + 9-Step State Machines)

### Task State Machine (12 States)

- **Initial**: Pending, Initializing
- **Active**: EnqueuingSteps, StepsInProcess, EvaluatingResults
- **Waiting**: WaitingForDependencies, WaitingForRetry, BlockedByFailures
- **Terminal**: Complete, Error, Cancelled, ResolvedManually

### Workflow Step State Machine (9 States)

- **Pipeline**: Pending, Enqueued, InProgress, EnqueuedForOrchestration, EnqueuedAsErrorForOrchestration
- **Waiting**: WaitingForRetry (TAS-42: separated from permanent Error state)
- **Terminal**: Complete, Error, Cancelled, ResolvedManually

### Key Evolution - TAS-42

- Before: `Error` state meant both retryable and permanent failures
- After: `Error` = permanent only, `WaitingForRetry` = retryable failure awaiting backoff
- Requires proper `get_step_readiness_status()` logic to recognize WaitingForRetry

### Persistence Pattern

- State transitions stored in `tasker_task_transitions` and `tasker_workflow_step_transitions`
- Current state resolved via `most_recent = true` flag (O(1) lookup)
- Processor UUID tracked in transitions (audit trail, not enforcement after TAS-54)

---

## 3. Retry Semantics (max_attempts + retryable Fields)

### Semantic Clarity

- **max_attempts**: Total number of execution attempts INCLUDING first attempt
  - `max_attempts=1`: Exactly 1 execution, no retries
  - `max_attempts=3`: First attempt + up to 2 retries

- **retryable**: Whether step can be retried AFTER first execution fails
  - First execution (attempts=0): ALWAYS eligible regardless of retryable setting
  - Retry attempts (attempts>0): Require retryable=true

### SQL Retry Eligibility Logic

```
attempts = 0 ? TRUE (first execution always allowed)
  : retryable=true AND attempts < max_attempts ? TRUE (retry allowed)
  : FALSE (no more attempts)
```

### Edge Cases

- `max_attempts=0`: Step can never execute (configuration error)
- `retryable=false` + `max_attempts > 1`: First execution allowed, no retries
- Should set `max_attempts=1` when `retryable=false` for clarity

---

## 4. Domain Events (Fire-and-Forget, Three Delivery Modes)

### Key Design Principle

Domain event publishing NEVER fails the step execution.

### Three Delivery Modes

1. **Durable** (PGMQ):
   - Persisted to namespace-specific PGMQ queue
   - External consumer boundary (not consumed internally by Tasker)
   - For external system integration (Kafka, SNS, SQS, custom proxies)
   - Survives service restarts

2. **Fast** (In-Process):
   - Broadcast via `tokio::broadcast::Sender`
   - ONLY delivery mode with internal subscriber support
   - Dual-path: Rust subscribers + Ruby FFI channel
   - Sub-millisecond latency, lost on restart

3. **Broadcast** (Both Paths):
   - Sends to both Fast and Durable simultaneously
   - Internal subscribers receive same event as external consumers

### Publication Conditions

- `success`: Only on step success
- `failure`: Any step failure (backward compatible)
- `retryable_failure`: Only retryable failures
- `permanent_failure`: Only permanent failures
- `always`: Regardless of outcome

### Event Flow

1. Step completes, handler returns result
2. Orchestration notified successfully via PGMQ
3. **ONLY THEN**: Domain events dispatched (fire-and-forget via `try_send`)
4. If domain event dispatch fails → event dropped, logged as warning
5. Step considered complete from system perspective

---

## 5. Events and Commands (Hybrid Architecture with Actor Pattern)

### Three Deployment Modes

1. **EventDrivenOnly**: Pure PostgreSQL LISTEN/NOTIFY (lowest latency)
2. **PollingOnly**: Traditional polling (reliable fallback)
3. **Hybrid** (Recommended): Event-driven primary, polling fallback

### Command Pattern Architecture (TAS-40)

- Event systems generate commands (100 lines vs 1000+ old coordinators)
- Commands sent via `tokio::mpsc` channels
- Command processor receives and routes to actors
- Pure routing, no complex business logic

### State-Before-Queue Pattern (Critical)

1. Commit state change to database FIRST
2. Send PGMQ message THEN
3. Workers query database and see correct state
4. Prevents workers from claiming steps in wrong state

### Actor Integration (TAS-46)

Four Production-Ready Actors:
1. **TaskRequestActor**: Task initialization
2. **ResultProcessorActor**: Step result processing
3. **StepEnqueuerActor**: Step enqueueing
4. **TaskFinalizerActor**: Task finalization with atomic claiming

---

## Cross-Cutting Design Principles for TAS-67

### Idempotency Guarantees
- Operations safe to retry indefinitely
- No duplicate work from concurrent processing
- Automatic recovery after crashes

### Atomicity Requirements
- All-or-nothing database transactions
- No partial state persistence
- CAS validation in UPDATE statements

### Event Ordering
- State-Before-Queue pattern ensures consistency
- Task → Step discovery → enqueueing → execution → result processing
- Depends on state guards, not message ordering

### Crash Recovery
- No processor ownership enforcement (TAS-54)
- State guards sufficient for idempotency
- Any orchestrator can resume interrupted work

---

## Relevance to TAS-67

These principles inform the dual-channel dispatch refactor:

1. **Dispatch Channel**: Fire-and-forget is safe because state machine guards ensure idempotency
2. **Completion Channel**: Results routed to orchestration; domain events only fire AFTER successful send
3. **Handler Timeout**: Timeouts generate failure results that the retry system handles
4. **Lock Poisoning**: Recovery is safe because operations are idempotent
5. **Concurrent Execution**: Out-of-order results are safe because state guards validate transitions

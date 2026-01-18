# TAS-73: Resiliency and Redundancy - Research & Analysis Plan

## Vision Statement

Validate that Tasker's architecture delivers on its core promise: **truly stateless, horizontally scalable orchestration and worker deployments** where N orchestration servers and M workers (across languages) can operate against shared PostgreSQL + message queues without coordination, conflict, or data corruption.

## Background

The Tasker system is designed to be fully event-driven, state machine backed, with a defense-in-depth approach. This means we should be able to spin up as many orchestration systems and workers as we want, without conflict, to spread workload horizontally:

- Orchestration systems don't need to know about worker systems
- No mesh registration required
- Workers don't need to know about the orchestration system or one another
- Processor UUIDs are tracked for observability only

In theory, we should be able to launch any number of orchestration systems pointed to the same database and message queues, any number of Ruby/Python/Rust/TypeScript workers handling the same namespaces and tasks. Our message claim semantics, task and step claim semantics, state machines, and PostgreSQL-backed atomicity should allow workers to handle different ready steps while convergence and dependency graph mechanics keep everything flowing smoothly.

### Motivating Example: TAS-151

During TAS-151 development, we discovered a missing `UNIQUE` constraint on `(task_uuid, named_step_uuid)` in the `workflow_steps` table. This allowed race conditions where multiple orchestrators processing a decision point outcome could create duplicate workflow steps. The fix required:

1. Adding the database constraint (defense at data layer)
2. Adding `find_or_create_with_transaction` with "first past the finish" semantics (defense at application layer)

This is exactly the type of issue TAS-73 should systematically identify and address.

---

## Part 1: Inventory of Concurrency-Sensitive Code Paths

Before testing, we need to systematically identify every code path where concurrent execution could cause problems.

### 1.1 Task Lifecycle Entry Points

| Code Path | Current Protection | Risk Level | Notes |
|-----------|-------------------|------------|-------|
| Task creation (POST /v1/tasks) | `identity_hash` unique constraint | Low | Already idempotent |
| Task initialization | Transaction-scoped | Medium | Multiple orchestrators could initialize same task? |
| Task state transitions | State machine + most_recent pattern | Medium | Verify atomic transition semantics |
| Task finalization | ? | **High** | What prevents double-finalization? |
| Task DLQ entry | `idx_dlq_unique_pending_task` | Low | Already constrained |

### 1.2 Step Lifecycle Entry Points

| Code Path | Current Protection | Risk Level | Notes |
|-----------|-------------------|------------|-------|
| Initial step creation | Transaction-scoped | Low | Part of task init transaction |
| Decision point step creation | `uq_workflow_step_task_named_step` + find_or_create | Low | Fixed in TAS-151 |
| Batch worker creation | Idempotency check + unique constraint | Low | Has edge count check |
| Step claiming (PGMQ read) | Visibility timeout | Low | PGMQ handles this |
| Step result processing | ? | **High** | What if two processors get same result? |
| Step state transitions | State machine + most_recent | Medium | Verify atomic semantics |
| Step retry scheduling | ? | Medium | Could same step be retried twice? |

### 1.3 Edge/Relationship Creation

| Code Path | Current Protection | Risk Level | Notes |
|-----------|-------------------|------------|-------|
| Workflow step edge creation | `unique_edge_per_step_pair` | Low | Already constrained |
| Named step creation | `find_or_create_by_name` | Low | Already idempotent |
| Named task creation | Unique on (namespace, name, version) | Low | Already constrained |

### 1.4 Message Queue Operations

| Code Path | Current Protection | Risk Level | Notes |
|-----------|-------------------|------------|-------|
| Step enqueueing | ? | Medium | Same step enqueued twice? |
| Result message processing | ? | **High** | Duplicate result messages? |
| Notification handling | Event-driven | Low | Notifications are hints, not commands |

---

## Part 2: Research Questions

### 2.1 Architectural Questions

1. **Task Finalization Race**: If two orchestrators simultaneously detect that all steps are complete, what prevents both from finalizing the task?
   - Is there an atomic "claim finalization" pattern?
   - Do we need a finalization state transition guard?

2. **Result Processing Idempotency**: When a step result arrives:
   - Can the same result be processed by multiple orchestrators?
   - Is result processing idempotent (processing twice = same outcome)?
   - What happens if result processing fails mid-way and retries?

3. **Step Enqueueing Idempotency**: When steps become ready:
   - Can the same step be enqueued to PGMQ multiple times?
   - Is there a "step already enqueued" check?
   - What's the step state transition that marks "enqueued"?

4. **State Machine Atomicity**: Our state machines use `most_recent = true` pattern:
   - Is the transition atomic (old most_recent=false, new most_recent=true)?
   - What happens if two transitions race?

5. **Message Acknowledgment**: When a worker completes a step:
   - Is the pgmq.delete atomic with result recording?
   - What if ack fails after result recorded?

### 2.2 Operational Questions

1. **Graceful Shutdown**: When an orchestrator/worker shuts down:
   - Are in-flight operations completed or released?
   - Do visibility timeouts handle abandoned work?

2. **Crash Recovery**: When a service crashes mid-operation:
   - What operations are left in inconsistent state?
   - What recovery mechanisms exist?

3. **Network Partitions**: If a service loses database connectivity:
   - What happens to in-flight work?
   - Are there timeout-based recovery paths?

---

## Part 3: Testing Infrastructure Design

### 3.1 Multi-Service Deployment Scripts

```
cargo-make/scripts/
├── multi-deploy/
│   ├── start-orchestration-cluster.sh  # Start N orchestration servers
│   ├── start-ruby-worker-cluster.sh    # Start M Ruby workers
│   ├── start-python-worker-cluster.sh  # Start M Python workers
│   ├── start-rust-worker-cluster.sh    # Start M Rust workers
│   └── stop-all-clusters.sh
```

**Configuration considerations:**
- Each service needs unique port (orchestration: 3000, 3001, 3002...)
- Each service needs unique processor_uuid
- Shared DATABASE_URL and PGMQ_DATABASE_URL
- Environment variable for instance index (TASKER_INSTANCE_ID=0,1,2...)

### 3.2 Test Harness Modifications

**API Endpoint Abstraction:**
```rust
// Round-robin or random selection of orchestration endpoints
struct OrchestrationCluster {
    endpoints: Vec<String>,  // ["http://localhost:3000", "http://localhost:3001", ...]
}

impl OrchestrationCluster {
    fn submit_task(&self, request: TaskRequest) -> Result<Task> {
        let endpoint = self.select_endpoint(); // round-robin, random, etc.
        // POST to endpoint
    }
}
```

**Test Categorization:**
```rust
#[cfg(test)]
mod tests {
    // Tests that work with multi-instance deployment
    #[test]
    #[cfg_attr(feature = "multi-instance", ignore)] // or include
    fn test_workflow_completion() { ... }

    // Tests that require single-instance (metrics assertions, etc.)
    #[test]
    #[cfg_attr(feature = "multi-instance", ignore)]
    fn test_worker_metrics() { ... }
}
```

### 3.3 Stress Test Scenarios

| Scenario | Description | What We're Testing |
|----------|-------------|-------------------|
| **Thundering Herd** | Submit 100 identical tasks simultaneously | Task deduplication via identity_hash |
| **Step Contention** | Many workers competing for same queue | PGMQ claim semantics |
| **Decision Point Storm** | Tasks with decision points processed by different orchestrators | Step creation idempotency |
| **Batch Processing Scale** | Large batch jobs across multiple workers | Worker creation idempotency |
| **Rapid Completion** | Fast-completing tasks with quick step transitions | State machine race conditions |
| **Convergence Correctness** | Fan-out/fan-in patterns with multiple workers | Dependency tracking accuracy |
| **Chaos Injection** | Random service restarts during processing | Recovery and consistency |

### 3.4 Observability Requirements

To debug issues in multi-instance tests, we need:

1. **Correlation Tracking**: Which processor handled which operation
2. **Timeline Reconstruction**: Ordered log of all state transitions
3. **Conflict Detection**: Log entries when idempotency kicks in (e.g., "step already existed")
4. **Metrics Aggregation**: Combined metrics across all instances

---

## Part 4: Implementation Plan Structure

Once research is complete, the implementation plan should address:

### Phase 1: Audit & Hardening
- Systematically review each code path from Part 1
- Add missing idempotency patterns where identified
- Add missing constraints where identified
- Document the "contract" for each operation

### Phase 2: Infrastructure
- Create multi-service deployment scripts
- Modify test harness for cluster awareness
- Set up aggregated observability

### Phase 3: Test Development
- Create stress test suite
- Create chaos test suite
- Establish baseline metrics (single instance)
- Run multi-instance comparison

### Phase 4: Documentation
- Document scaling guidelines
- Document recovery procedures
- Document operational runbook

---

## Part 5: Test Isolation Strategy

### Tests to EXCLUDE from multi-instance testing:

1. **Worker-specific metrics tests**: Can't predict which worker processes what
2. **Processor UUID assertions**: Non-deterministic in multi-instance
3. **Exact timing assertions**: More variability with multiple processors
4. **Single-step-handler tests**: May need specific worker

### Tests to PRIORITIZE for multi-instance testing:

1. **End-to-end workflow completion**: Task submitted → all steps complete
2. **Decision point workflows**: Dynamic step creation
3. **Batch processing workflows**: Worker creation and convergence
4. **Error handling and retry**: Failure recovery across instances
5. **Long-running workflows**: State consistency over time

---

## Part 6: Success Criteria

TAS-73 should establish that:

1. **No data corruption**: No duplicate records, no orphaned records, no inconsistent state
2. **Exactly-once semantics**: Each step processed exactly once
3. **Correct convergence**: Fan-out/fan-in produces correct results
4. **Graceful degradation**: Service failures don't corrupt state
5. **Linear scalability**: 2x instances ≈ 2x throughput (for parallelizable workloads)

---

## Suggested Sub-Tickets

TAS-73 could be broken into sub-tickets:

- **TAS-73a**: Code path audit and documentation
- **TAS-73b**: Multi-service deployment infrastructure
- **TAS-73c**: Stress test suite development
- **TAS-73d**: Chaos test suite development
- **TAS-73e**: Hardening fixes (as issues are discovered)

---

## References

- TAS-151: Fixed race condition in decision point step creation (missing unique constraint)
- `docs/architecture/states-and-lifecycles.md`: Task and step state machines
- `docs/architecture/actors.md`: Orchestration actor pattern
- `docs/architecture/worker-event-systems.md`: Worker architecture

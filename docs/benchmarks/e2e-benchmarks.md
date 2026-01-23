# E2E Benchmark Scenarios: Workflow Shapes and Per-Step Lifecycle

**Last Updated**: 2026-01-23
**Audience**: Architects, Developers, Performance Engineers
**Related Docs**: [Benchmarks README](./README.md) | [States & Lifecycles](../architecture/states-and-lifecycles.md) | [Actor Pattern](../architecture/actors.md)

<- Back to [Benchmarks](./README.md)

---

## What Each Benchmark Measures

Each E2E benchmark executes a **complete workflow** through the full distributed system:
HTTP API call, task initialization, step discovery, message queue dispatch, worker execution,
result submission, dependency graph re-evaluation, and task finalization.

A 4-step linear workflow at P50=257ms means the system completes **76+ database operations**,
**8 message queue round-trips**, **16+ state machine transitions**, and **4 dependency graph
evaluations** — all across a 10-instance distributed cluster — in approximately one quarter
of a second.

---

## Per-Step Lifecycle: What Happens for Every Step

Before examining the benchmark scenarios, it's important to understand the work the system
performs for each individual step. Every step in every benchmark goes through this complete
lifecycle.

**Messaging Backend**: Tasker uses a `MessagingService` trait with provider variants for
PGMQ (PostgreSQL-native, single-dependency) and RabbitMQ (high-throughput). The benchmark
results documented here were captured using the RabbitMQ backend. The per-step lifecycle
is identical regardless of backend — only the transport layer differs.

### State Machine Transitions (per step)

```
Step:  Pending → Enqueued → InProgress → EnqueuedForOrchestration → Complete
Task:  StepsInProcess → EvaluatingResults → (EnqueuingSteps if more ready) → Complete
```

### Database Operations (per step): ~19 operations

| Phase | Operations | Description |
|-------|-----------|-------------|
| **Discovery** | 2 queries | `get_next_ready_tasks` + `get_step_readiness_status_batch` (8-CTE query) |
| **Enqueueing** | 4 writes | Fetch correlation_id, transition Pending→Enqueued (SELECT sort_key + UPDATE most_recent + INSERT transition) |
| **Message send** | 1 op | Send step dispatch to worker queue (via MessagingService) |
| **Worker claim** | 1 op | Claim message with visibility timeout (via MessagingService) |
| **Worker transition** | 3 writes | Transition Enqueued→InProgress |
| **Result submission** | 4 writes | Transition InProgress→EnqueuedForOrchestration + audit trigger INSERT + send completion to orchestration queue |
| **Result processing** | 4 writes | Fetch step state, transition →Complete, delete consumed message |
| **Task coordination** | 1+ queries | Re-evaluate `get_step_readiness_status_batch` for remaining steps |
| **Total** | **~19 ops** | |

### Message Queue Round-Trips (per step): 2

1. **Orchestration → Worker**: Step dispatch message (task_uuid, step_uuid, handler, context)
2. **Worker → Orchestration**: Completion notification (task_uuid, step_uuid, results)

### Dependency Graph Evaluation (per step completion)

After each step completes, the orchestration:
1. Queries all steps in the task for current state
2. Evaluates dependency edges (parent steps must be Complete)
3. Calculates retry eligibility (attempts < max_attempts, backoff expired)
4. Identifies newly-ready steps for enqueueing
5. Updates task state (more steps ready → EnqueuingSteps, all complete → Complete)

### Idempotency Guarantees

- **Message visibility timeout**: MessagingService prevents duplicate processing (30s window)
- **State machine guards**: Transitions validate from-state before applying
- **Atomic claiming**: Workers claim via the messaging backend's atomic read operation
- **Audit trail**: Every transition creates an immutable `workflow_step_transitions` record

---

## Tier 1: Core Performance (Rust Native)

### Linear Rust (4 steps, sequential)

**Fixture**: `tests/fixtures/task_templates/rust/mathematical_sequence.yaml`
**Namespace**: `rust_e2e_linear` | **Handler**: `mathematical_sequence`

```
linear_step_1 → linear_step_2 → linear_step_3 → linear_step_4
```

| Step | Handler | Operation | Depends On | Math |
|------|---------|-----------|------------|------|
| linear_step_1 | LinearStep1 | square | none | 6^2 = 36 |
| linear_step_2 | LinearStep2 | square | step_1 | 36^2 = 1,296 |
| linear_step_3 | LinearStep3 | square | step_2 | 1,296^2 = 1,679,616 |
| linear_step_4 | LinearStep4 | square | step_3 | 1,679,616^2 |

**Distributed system work for this workflow:**

| Metric | Count |
|--------|-------|
| State machine transitions (step) | 16 (4 per step) |
| State machine transitions (task) | 6 (Pending→Init→Enqueue→InProcess→Eval→Complete) |
| Database operations | 76 (19 per step) |
| MQ messages | 8 (2 per step) |
| Dependency evaluations | 4 (after each step completes) |
| HTTP calls (benchmark→API) | 1 create + ~5 polls |
| Sequential stages | 4 |

**Why this matters**: This is the purest sequential latency test. Each step must fully
complete (all 19 DB operations + 2 message round-trips) before the next step can begin.
The P50 of ~257ms means each step's complete lifecycle averages ~64ms including all
distributed coordination.

---

### Diamond Rust (4 steps, 2-way parallel)

**Fixture**: `tests/fixtures/task_templates/rust/diamond_pattern.yaml`
**Namespace**: `rust_e2e_diamond` | **Handler**: `diamond_pattern`

```
         diamond_start
           /       \
          /         \
  diamond_branch_b  diamond_branch_c    ← parallel execution
          \         /
           \       /
         diamond_end                    ← 2-way convergence
```

| Step | Handler | Operation | Depends On | Parallelism |
|------|---------|-----------|------------|-------------|
| diamond_start | Start | square | none | - |
| diamond_branch_b | BranchB | square | start | parallel with C |
| diamond_branch_c | BranchC | square | start | parallel with B |
| diamond_end | End | multiply_and_square | branch_b AND branch_c | convergence |

**Distributed system work:**

| Metric | Count |
|--------|-------|
| State machine transitions (step) | 16 |
| Database operations | 76 |
| MQ messages | 8 |
| Dependency evaluations | 4 |
| Sequential stages | 3 (start → parallel → end) |
| Convergence points | 1 (diamond_end waits for both branches) |
| Dependency edge checks | 4 (start→B, start→C, B→end, C→end) |

**Why this matters**: Tests the system's ability to dispatch and execute steps concurrently.
The convergence point (diamond_end) requires the orchestration to correctly evaluate that
BOTH branch_b AND branch_c are Complete before enqueueing diamond_end. Under light load,
this completes in 3 sequential stages vs 4 for linear (~30% faster).

---

## Tier 2: Complexity Scaling

### Complex DAG (7 steps, mixed parallelism)

**Fixture**: `tests/fixtures/task_templates/rust/complex_dag.yaml`
**Namespace**: `rust_e2e_mixed_dag` | **Handler**: `complex_dag`

```
              dag_init
             /        \
   dag_process_left   dag_process_right     ← 2-way parallel
        /    |              |    \
       /     |              |     \
dag_validate dag_transform dag_analyze      ← mixed dependencies
       \          |          /
        \         |         /
         dag_finalize                       ← 3-way convergence
```

| Step | Depends On | Type |
|------|-----------|------|
| dag_init | none | init |
| dag_process_left | init | parallel branch |
| dag_process_right | init | parallel branch |
| dag_validate | left AND right | 2-way convergence |
| dag_transform | left only | linear continuation |
| dag_analyze | right only | linear continuation |
| dag_finalize | validate AND transform AND analyze | 3-way convergence |

**Distributed system work:**

| Metric | Count |
|--------|-------|
| State machine transitions (step) | 28 (7 steps x 4) |
| Database operations | 133 (7 x 19) |
| MQ messages | 14 (7 x 2) |
| Dependency evaluations | 7 |
| Sequential stages | 4 (init → left/right → validate/transform/analyze → finalize) |
| Convergence points | 2 (dag_validate: 2-way, dag_finalize: 3-way) |
| Dependency edge checks | 8 |

**Why this matters**: Tests multiple convergence points with different fan-in widths.
The orchestration must correctly handle that dag_validate needs 2 parents while
dag_finalize needs 3. Also tests mixed patterns: some steps continue from a single
parent (transform from left only) while others require multiple.

---

### Hierarchical Tree (8 steps, 4-way convergence)

**Fixture**: `tests/fixtures/task_templates/rust/hierarchical_tree.yaml`
**Namespace**: `rust_e2e_tree` | **Handler**: `hierarchical_tree`

```
                    tree_root
                   /         \
        tree_branch_left    tree_branch_right    ← 2-way parallel
          /       \           /        \
  tree_leaf_d  tree_leaf_e  tree_leaf_f  tree_leaf_g  ← 4-way parallel
         \          |            |          /
          \         |            |         /
           tree_final_convergence               ← 4-way convergence
```

| Level | Steps | Parallelism | Operation |
|-------|-------|-------------|-----------|
| 0 | root | sequential | square |
| 1 | branch_left, branch_right | 2-way parallel | square |
| 2 | leaf_d, leaf_e, leaf_f, leaf_g | 4-way parallel | square |
| 3 | final_convergence | 4-way convergence | multiply_all_and_square |

**Distributed system work:**

| Metric | Count |
|--------|-------|
| State machine transitions (step) | 32 (8 x 4) |
| Database operations | 152 (8 x 19) |
| MQ messages | 16 (8 x 2) |
| Dependency evaluations | 8 |
| Sequential stages | 4 (root → branches → leaves → convergence) |
| Maximum fan-out | 2-way (each branch → 2 leaves) |
| Maximum fan-in | 4-way (convergence waits for all 4 leaves) |
| Dependency edge checks | 9 |

**Why this matters**: Tests the widest convergence pattern — 4 parallel leaves must all
complete before the final step can execute. This exercises the dependency evaluation with
a large number of parent checks per step. Also tests hierarchical fan-out (root→2 branches→4 leaves).

---

### Conditional Routing (5 steps, 3 executed)

**Fixture**: `tests/fixtures/task_templates/rust/conditional_approval_rust.yaml`
**Namespace**: `conditional_approval_rust` | **Handler**: `approval_routing`
**Context**: `{"amount": 500, "requester": "benchmark"}`

```
validate_request
       ↓
routing_decision          ← DECISION POINT (routes based on amount)
   /      |      \
  /       |       \
auto_approve  manager_approval  finance_review
(< $1000)     ($1000-$5000)     (> $5000)
  \       |       /
   \      |      /
  finalize_approval               ← deferred convergence
```

With benchmark context `amount=500`, only the auto_approve path executes:

```
validate_request → routing_decision → auto_approve → finalize_approval
```

| Step | Executed | Condition |
|------|----------|-----------|
| validate_request | Yes | always |
| routing_decision | Yes | always (decision point) |
| auto_approve | Yes | amount < 1000 |
| manager_approval | Skipped | amount 1000-5000 |
| finance_review | Skipped | amount > 5000 |
| finalize_approval | Yes | deferred convergence (waits for executed paths only) |

**Distributed system work (executed steps only):**

| Metric | Count |
|--------|-------|
| State machine transitions (step) | 16 (4 executed x 4) |
| Database operations | 76 (4 executed x 19) |
| MQ messages | 8 (4 executed x 2) |
| Dependency evaluations | 4 |
| Sequential stages | 4 (validate → decision → approve → finalize) |
| Skipped steps | 2 (manager_approval, finance_review) |

**Why this matters**: Tests deferred convergence — the finalize_approval step depends on
ALL conditional branches, but only blocks on branches that actually executed. The
orchestration must correctly determine that manager_approval and finance_review were
skipped (not just incomplete) and allow finalize_approval to proceed. Also tests the
decision point routing pattern.

---

## Tier 3: Cluster Performance

### Single Task Linear (4 steps, round-robin across 2 orchestrators)

Same workflow as Tier 1 linear_rust, but benchmarked with round-robin across 2
orchestration instances to measure cluster coordination overhead.

**Distributed system work**: Same as linear_rust (76 DB ops, 8 MQ messages) plus
cluster coordination overhead (shared database, message queue visibility).

**Why this matters**: Validates that running in cluster mode adds negligible overhead
compared to single-instance. The P50 difference (261ms vs 257ms = ~4ms) represents
the entire cluster coordination tax.

### Concurrent Tasks 2x (2 tasks simultaneously across 2 orchestrators)

Two linear workflows submitted simultaneously, one to each orchestration instance.

**Distributed system work:**

| Metric | Count |
|--------|-------|
| State machine transitions | 44 (22 per task) |
| Database operations | 152 (76 per task) |
| MQ messages | 16 (8 per task) |
| Concurrent step executions | up to 2 |
| Database connection contention | 2 orchestrators + 2 workers competing |

**Why this matters**: Tests work distribution across cluster instances under concurrent
load. The P50 of ~332-384ms for TWO tasks (vs ~261ms for one) shows that the second task
adds only 30-50% latency, not 100% — demonstrating effective parallelism in the cluster.

---

## Tier 4: FFI Language Comparison

Same linear and diamond patterns as Tier 1, but using FFI workers (Ruby via Magnus,
Python via PyO3, TypeScript via Bun FFI) instead of native Rust handlers.

**Additional per-step work for FFI:**

| Phase | Additional Operations |
|-------|----------------------|
| Handler dispatch | FFI bridge call (Rust → language runtime) |
| Context serialization | JSON serialize context for foreign runtime |
| Result deserialization | JSON deserialize results back to Rust |
| Circuit breaker check | `should_allow()` (sync, atomic check) |
| Completion callback | FFI completion channel (bounded MPSC) |

**FFI overhead: ~23% (~60ms for 4 steps)**

The overhead is framework-dominated (Rust dispatch + serialization + completion channel),
not language-dominated — all three languages perform within 3ms of each other.

---

## Tier 5: Batch Processing

### CSV Products 1000 Rows (7 steps, 5-way parallel)

**Fixture**: `tests/fixtures/task_templates/rust/batch_processing_products_csv.yaml`
**Namespace**: `csv_processing_rust` | **Handler**: `csv_product_inventory_analyzer`

```
analyze_csv                    ← reads CSV, returns BatchProcessingOutcome
    ↓
[orchestration creates 5 dynamic workers from batch template]
    ↓
process_csv_batch_001 ──┐
process_csv_batch_002 ──┤
process_csv_batch_003 ──├──→ aggregate_csv_results    ← deferred convergence
process_csv_batch_004 ──┤
process_csv_batch_005 ──┘
```

| Step | Type | Rows | Operation |
|------|------|------|-----------|
| analyze_csv | batchable | all 1000 | Count rows, compute batch ranges |
| process_csv_batch_001 | batch_worker | 1-200 | Compute inventory metrics |
| process_csv_batch_002 | batch_worker | 201-400 | Compute inventory metrics |
| process_csv_batch_003 | batch_worker | 401-600 | Compute inventory metrics |
| process_csv_batch_004 | batch_worker | 601-800 | Compute inventory metrics |
| process_csv_batch_005 | batch_worker | 801-1000 | Compute inventory metrics |
| aggregate_csv_results | deferred_convergence | all | Merge batch results |

**Distributed system work:**

| Metric | Count |
|--------|-------|
| State machine transitions (step) | 28 (7 x 4) |
| Database operations | 133 (7 x 19) |
| MQ messages | 14 (7 x 2) |
| Dynamic step creation | 5 (batch workers created at runtime) |
| Dependency edges (dynamic) | 6 (batch workers → analyze, aggregate → batch_template) |
| File I/O operations | 6 (1 analysis read + 5 batch reads of CSV) |
| CSV rows processed | 1000 |
| Sequential stages | 3 (analyze → 5 parallel workers → aggregate) |

**Why this matters**: Tests the most complex orchestration pattern — dynamic step
generation. The analyze_csv step returns a `BatchProcessingOutcome` that tells the
orchestration to create N worker steps at runtime. The orchestration must:
1. Create new step records in the database
2. Create dependency edges dynamically
3. Enqueue all batch workers for parallel execution
4. Use deferred convergence for the aggregate step (waits for batch template, not specific steps)

At P50=358-368ms for 1000 rows, throughput is ~2,700 rows/second with all the
distributed system overhead included.

---

## Summary: Operations Per Benchmark

| Benchmark | Steps | DB Ops | MQ Msgs | Transitions | Convergence | P50 |
|-----------|-------|--------|-----------|-------------|-------------|-----|
| Linear Rust | 4 | 76 | 8 | 22 | none | 257ms |
| Diamond Rust | 4 | 76 | 8 | 22 | 2-way | 200-259ms |
| Complex DAG | 7 | 133 | 14 | 34 | 2+3-way | 382ms |
| Hierarchical Tree | 8 | 152 | 16 | 38 | 4-way | 389-426ms |
| Conditional | 4* | 76 | 8 | 22 | deferred | 251-262ms |
| Cluster single | 4 | 76 | 8 | 22 | none | 261ms |
| Cluster 2x | 8 | 152 | 16 | 44 | none | 332-384ms |
| FFI linear | 4 | 76 | 8 | 22 | none | 312-316ms |
| FFI diamond | 4 | 76 | 8 | 22 | 2-way | 260-275ms |
| Batch 1000 rows | 7 | 133 | 14 | 34 | deferred | 358-368ms |

*Conditional executes 4 of 5 defined steps (2 skipped by routing decision)

---

## Performance per Sequential Stage

For workflows with known sequential depth, we can calculate per-stage overhead:

| Benchmark | Sequential Stages | P50 | Per-Stage Avg |
|-----------|-------------------|-----|---------------|
| Linear (4 seq) | 4 | 257ms | 64ms |
| Diamond (3 seq) | 3 | 200ms* | 67ms |
| Complex DAG (4 seq) | 4 | 382ms | 96ms** |
| Tree (4 seq) | 4 | 389ms | 97ms** |
| Conditional (4 seq) | 4 | 257ms | 64ms |
| Batch (3 seq) | 3 | 363ms | 121ms*** |

*Diamond under light load (parallelism helping)
**Higher per-stage due to multiple steps per stage (more DB ops per evaluation cycle)
***Higher per-stage due to batch worker creation overhead + file I/O

The ~64ms per sequential stage for simple patterns represents the total distributed
round-trip: orchestration discovery → MQ dispatch → worker claim → handler execute
(~1ms for math operations) → MQ completion → orchestration result processing →
dependency re-evaluation. The handler execution itself is negligible; the 64ms is
almost entirely orchestration infrastructure.

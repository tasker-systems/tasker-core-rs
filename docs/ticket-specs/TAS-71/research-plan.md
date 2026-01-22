# TAS-71: Profiling, Benchmarks, and Optimizations - Research Plan

**Date:** 2026-01-20
**Status:** Research Planning Phase
**Branch:** `jcoletaylor/tas-71-profiling-benchmarks-and-optimizations`
**Depends On:** TAS-73 (cluster infrastructure complete)

---

## Executive Summary

With TAS-73 establishing system correctness (stateless horizontal scaling, N orchestration + M workers, no locks/contention), TAS-71 focuses on **performance characterization and optimization**. This is a research-first ticket: we build tooling foundations, gather data, and let analysis drive subsequent optimization work.

---

## Current State Assessment

### Existing Benchmark Infrastructure

| Category | Location | Status | Notes |
|----------|----------|--------|-------|
| SQL Functions | `tasker-shared/benches/sql_functions.rs` | ✅ Active | EXPLAIN ANALYZE, diverse sampling |
| API Task Init | `tasker-client/benches/task_initialization.rs` | ✅ Active | Linear/diamond task creation |
| E2E Latency | `tests/benches/e2e_latency.rs` | ✅ Active | Full workflow completion |
| Event Propagation | `tasker-shared/benches/event_propagation.rs` | ✅ Active | LISTEN/NOTIFY round-trip |
| Handler Overhead | `tasker-worker/benches/handler_overhead.rs` | ⚠️ Exists | Needs verification |
| Worker Execution | `tasker-worker/benches/worker_execution.rs` | ⚠️ Exists | Needs verification |
| Step Enqueueing | `tasker-orchestration/benches/step_enqueueing.rs` | ⚠️ Exists | Needs verification |
| Orchestration | `tasker-orchestration/benches/orchestration_benchmarks.rs` | ⚠️ Exists | Needs verification |

### Cluster Infrastructure (from TAS-73)

| Capability | Status | Command |
|------------|--------|---------|
| Single-instance services | ✅ | `cargo make services-start` |
| Multi-instance cluster | ✅ | `cargo make cluster-start-all` |
| Configurable N orchestration | ✅ | `TASKER_ORCHESTRATION_INSTANCES=N` |
| Configurable M workers per language | ✅ | `TASKER_WORKER_{RUST,RUBY,PYTHON,TS}_INSTANCES=M` |
| PGMQ messaging | ✅ | Default |
| RabbitMQ messaging | ✅ | Via config toggle |

### Profiling Tooling (NOT YET CONFIGURED)

| Tool | Purpose | Status |
|------|---------|--------|
| cargo-flamegraph | CPU flame graphs | ❌ Not installed/configured |
| samply | macOS sampling profiler | ❌ Not evaluated |
| tokio-console | Async runtime introspection | ❌ Not configured |
| tracing-flame | Tracing-based flamegraphs | ❌ Not configured |

---

## Research Dimensions

### Dimension 1: Workflow Complexity

Our e2e test suite exercises various DAG patterns. We need benchmarks for each:

| Pattern | Description | Files | Complexity |
|---------|-------------|-------|------------|
| Linear | A → B → C → D | 4 sequential steps | Low |
| Diamond | A → (B,C) → D | Fan-out/fan-in | Medium |
| Multi-path DAG | Complex dependency graph | 8+ steps, multiple paths | High |
| Decision-conditional | Dynamic step creation | Runtime-determined paths | Variable |
| Batchable | Parallel execution | N parallel steps | Scale-dependent |
| Tree | Hierarchical fan-out | Deep nesting | High |

### Dimension 2: Service Configuration

| Configuration | Orchestration | Workers | Total Services |
|---------------|---------------|---------|----------------|
| Single | 1 | 1 (Rust) | 2 |
| Dual | 2 | 2 (Rust) | 4 |
| Multi-worker | 1 | 4 (Rust) | 5 |
| Language-mixed | 2 | 2 each (R,Rb,Py,TS) | 10 |
| Scale test | 3 | 3 each (all) | 15 |

### Dimension 3: Messaging Backend

| Backend | Characteristics | When to Test |
|---------|-----------------|--------------|
| PGMQ | Lower latency, transaction-scoped | Default, most scenarios |
| RabbitMQ | Higher throughput ceiling, separate service | Scalability edge cases |

### Dimension 4: Build Configuration

| Mode | Flags | Purpose |
|------|-------|---------|
| Debug | default | Development, instrumentation |
| Release | `--release` | Production-like performance |
| Release + debug | `--release` with `debug=1` | Profiling with symbols |

---

## Research Questions

### Performance Characterization

1. **Happy-path latency**: What is the p50/p95/p99 latency for completing workflows of each complexity type?
2. **Throughput ceiling**: How many tasks/second can we sustain before degradation?
3. **Per-step overhead**: What is the fixed overhead per step (DB, queue, coordination)?
4. **Language overhead**: What is the FFI overhead for Ruby/Python/TypeScript vs native Rust handlers?

### Scaling Characteristics

5. **Worker scaling**: Does adding workers improve throughput linearly? Where does it plateau?
6. **Orchestration scaling**: At what worker count does orchestration become the bottleneck?
7. **Database scaling**: When does PostgreSQL become the bottleneck?
8. **Queue saturation**: At what message volume do PGMQ/RabbitMQ show degradation?

### Bottleneck Identification

9. **Hot paths**: Where does the system spend most time in the happy path?
10. **Contention points**: Are there locks, mutexes, or resources causing contention?
11. **Allocation patterns**: Are there excessive allocations in hot paths?
12. **Async overhead**: Is tokio task scheduling or channel communication a bottleneck?

### Tuning Questions

13. **Config tunables**: Which config parameters most impact performance?
14. **Pool sizing**: What are optimal connection pool sizes for different loads?
15. **Channel buffers**: What are optimal MPSC channel buffer sizes?
16. **Batch sizes**: What are optimal batch sizes for step enqueueing?

---

## Parallelizable Research Areas

The following research tasks can be executed in parallel by sub-agents:

### Research Area A: Profiling Tooling Evaluation

**Goal:** Evaluate and recommend profiling tools for our stack

**Sub-tasks:**
1. Research `cargo-flamegraph` - installation, usage with services, release profile requirements
2. Research `samply` - macOS-specific, comparison to flamegraph, async support
3. Research `tokio-console` - setup requirements, runtime instrumentation, what it reveals
4. Research `tracing-flame` - integration with our existing tracing, output formats
5. Evaluate memory profilers (heaptrack, dhat, memory-profiler)
6. Document tool selection recommendations

### Research Area B: Existing Benchmark Audit

**Goal:** Assess current benchmarks, identify gaps, verify they work

**Sub-tasks:**
1. Audit `tasker-shared/benches/*.rs` - verify running, review methodology
2. Audit `tasker-client/benches/*.rs` - verify running, review methodology
3. Audit `tasker-orchestration/benches/*.rs` - verify running, review methodology
4. Audit `tasker-worker/benches/*.rs` - verify running, review methodology
5. Audit `tests/benches/e2e_latency.rs` - verify running, review methodology
6. Document gaps and improvement recommendations

### Research Area C: E2E Workflow Benchmark Design

**Goal:** Design benchmarks that exercise full task lifecycles across complexity levels

**Sub-tasks:**
1. Review existing e2e test fixtures for workflow patterns available
2. Design linear workflow benchmark (baseline)
3. Design diamond workflow benchmark (fan-out/fan-in)
4. Design decision-conditional workflow benchmark
5. Design batch-parallel workflow benchmark
6. Design multi-path DAG workflow benchmark
7. Document benchmark harness requirements (task creation, completion polling, metrics)

### Research Area D: Cluster Benchmark Infrastructure

**Goal:** Design infrastructure for benchmarking across service configurations

**Sub-tasks:**
1. Review TAS-73 cluster scripts for benchmark compatibility
2. Design automated cluster configuration switching
3. Design metrics collection across cluster instances
4. Design aggregated result reporting
5. Identify resource requirements (memory, CPU) for each configuration
6. Document cluster benchmark workflow

### Research Area E: Load Testing Framework

**Goal:** Design framework for sustained load testing and saturation analysis

**Sub-tasks:**
1. Research existing Rust load testing tools (drill, goose, criterion throughput mode)
2. Design concurrent task submission harness
3. Design backpressure measurement approach
4. Design saturation detection (when does throughput stop increasing?)
5. Design resource monitoring during load tests (CPU, memory, connections)
6. Document load testing workflow

### Research Area F: Tokio Runtime Analysis

**Goal:** Understand async runtime behavior and potential optimizations

**Sub-tasks:**
1. Research tokio::spawn naming for traceability (mentioned in ticket)
2. Research tokio-console integration requirements
3. Analyze current spawn patterns in codebase
4. Identify opportunities for spawn naming
5. Research tokio runtime tuning (worker threads, blocking pool)
6. Document tokio optimization recommendations

---

## Implementation Plan Structure

### Phase 1: Tooling Foundation (This Ticket)

**Deliverables:**
- [ ] Profiling tool selection and setup guide
- [ ] Existing benchmark audit and gaps document
- [ ] Benchmark harness improvements (if quick wins identified)
- [ ] tokio-console integration (if straightforward)
- [ ] Release profile configuration for profiling

### Phase 2: Benchmark Development (Likely Follow-up Ticket)

**Deliverables:**
- [ ] E2E workflow benchmarks for each complexity level
- [ ] Cluster benchmark infrastructure
- [ ] Automated benchmark run scripts
- [ ] Baseline measurements document

### Phase 3: Analysis & Profiling (Likely Follow-up Ticket)

**Deliverables:**
- [ ] Flame graphs for key workflows
- [ ] Bottleneck identification report
- [ ] Scaling characteristic graphs
- [ ] Tuning recommendations document

### Phase 4: Optimizations (Subsequent Tickets)

**Deliverables:**
- [ ] Targeted optimizations based on findings
- [ ] Before/after benchmark comparisons
- [ ] Documentation updates

---

## Success Criteria for TAS-71

TAS-71 (this ticket) will be considered complete when:

1. **Tooling selected**: Profiling tools evaluated, selection documented
2. **Existing benchmarks audited**: Current state documented, gaps identified
3. **Benchmark design documented**: Designs ready for implementation
4. **Foundation in place**: Basic profiling workflow demonstrable
5. **Follow-up tickets created**: Subsequent work broken into actionable tickets

---

## File Structure

```
docs/ticket-specs/TAS-71/
├── research-plan.md              # This file
├── profiling-tool-evaluation.md  # Research Area A output
├── benchmark-audit.md            # Research Area B output
├── e2e-benchmark-design.md       # Research Area C output
├── cluster-benchmark-design.md   # Research Area D output
├── load-testing-design.md        # Research Area E output
└── tokio-analysis.md             # Research Area F output
```

---

## References

- TAS-73 Research Findings: `docs/ticket-specs/TAS-73/research-findings.md`
- Existing Benchmarks README: `docs/benchmarks/README.md`
- Benchmarking Guide: `docs/observability/benchmarking-guide.md`
- Cluster Testing Guide: `docs/testing/cluster-testing-guide.md`
- Test Feature Flags: `docs/ticket-specs/TAS-73/test-feature-flags-design.md`

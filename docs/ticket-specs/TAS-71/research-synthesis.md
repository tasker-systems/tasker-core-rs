# TAS-71: Research Synthesis and Recommendations

**Date:** 2026-01-20
**Status:** Research Complete
**Branch:** `jcoletaylor/tas-71-profiling-benchmarks-and-optimizations`

---

## Executive Summary

This document synthesizes findings from six research areas completed for TAS-71. The research establishes the foundation for profiling, benchmarking, and optimization work. Key findings:

1. **Profiling tooling** is readily available and well-suited for our stack
2. **Existing benchmarks** are solid but have significant coverage gaps
3. **Tokio runtime** is well-configured with comprehensive tracing, but spawns need naming
4. **E2E benchmark designs** are ready for implementation
5. **Load testing framework** design is complete

---

## Research Documents Completed

| Area | Document | Key Finding |
|------|----------|-------------|
| A | `profiling-tool-evaluation.md` | samply + tokio-console recommended for macOS |
| B | `benchmark-audit.md` | 4/9 benchmarks implemented, 2 empty stubs to remove |
| C | `e2e-benchmark-design.md` | 6 workflow patterns available, 5 benchmark tiers defined |
| E | `load-testing-design.md` | Custom Rust generator with completion tracking recommended |
| F | `tokio-analysis.md` | 42 unnamed spawns, 6.5 hours for tokio-console integration |
| - | `research-plan.md` | Original plan (updated with findings) |

---

## Key Findings by Research Area

### Area A: Profiling Tooling

**Recommended Stack:**

| Need | macOS Development | Linux CI |
|------|-------------------|----------|
| CPU Profiling | samply | cargo-flamegraph |
| Async Debugging | tokio-console | tokio-console |
| Memory Profiling | dhat-rs | heaptrack |
| Instrumentation | tracing-flame | tracing-flame |

**Critical Action:** Add profiling profile to `Cargo.toml`:
```toml
[profile.profiling]
inherits = "release"
debug = true
strip = false
panic = "unwind"
```

**Issue Identified:** Current release profile has `strip = true` which removes debug symbols.

### Area B: Benchmark Audit

**Benchmark Status:**

| Status | Count | Benchmarks |
|--------|-------|------------|
| Fully Implemented | 4 | sql_functions, event_propagation, task_initialization, e2e_latency |
| Documented Placeholder | 3 | step_enqueueing, handler_overhead, worker_execution |
| Empty Stub | 2 | orchestration_benchmarks, worker_benchmarks |

**Coverage Gaps:**
- No memory benchmarks
- No throughput benchmarks
- No connection pool benchmarks
- No large dataset (10K+) scaling tests

**Cleanup Needed:**
- Remove `orchestration_benchmarks.rs` (empty)
- Remove `worker_benchmarks.rs` (empty)

### Area C: E2E Workflow Benchmark Design

**Available Patterns:**

| Pattern | Steps | Complexity | Implementation |
|---------|-------|------------|----------------|
| Linear | 4 | Low | Ready |
| Diamond | 4 | Medium | Ready |
| Complex DAG | 7 | High | Design complete |
| Hierarchical Tree | 8 | High | Design complete |
| Decision Conditional | 3-6 | Variable | Design complete |
| Batch Processing | 2+N | Scale-dependent | Design complete |

**Benchmark Tiers:**
- Tier 1: Core performance (every PR)
- Tier 2: Complexity scaling (weekly)
- Tier 3: Horizontal scaling (local only)
- Tier 4: Language comparison (on-demand)
- Tier 5: Batch scaling (on-demand)

### Area E: Load Testing Framework

**Recommended Approach:** Hybrid
- Custom Rust load generator for throughput-to-completion
- External tools (k6) for API stress testing
- Completion tracker for both

**Key Scenarios:**
1. Ramp-up test (find saturation point)
2. Sustained load test (stability/leaks)
3. Burst load test (spike handling)
4. Scaling comparison test (horizontal scaling)

### Area F: Tokio Runtime Analysis

**Spawn Analysis:**
- 42 total spawns, 0 named
- 15 long-running spawns (priority for naming)
- Zero spawn_blocking calls (good!)
- All MPSC channels bounded (TAS-51 compliant)

**Tracing Status:**
- Comprehensive OpenTelemetry integration
- 100+ `#[instrument]` macros
- Correlation ID propagation
- OTLP export configured

**tokio-console Integration:**
- Estimated effort: 6.5 hours
- Requires: console-subscriber, tokio_unstable cfg
- High value for async bottleneck identification

---

## Recommended Implementation Roadmap

### Phase 1: Quick Wins (1-2 days)

| Task | Effort | Impact |
|------|--------|--------|
| Add profiling profile to Cargo.toml | 30 min | Enables flamegraph/samply |
| Install samply on dev machine | 15 min | CPU profiling ready |
| Remove 2 empty benchmark stubs | 30 min | Cleaner codebase |
| Document basic profiling workflow | 1 hr | Developer enablement |

### Phase 2: Tooling Foundation (COMPLETED in TAS-158)

| Task | Effort | Status |
|------|--------|--------|
| Add tokio-console feature flag | 0.5 hr | DONE |
| Name long-running spawns | 1.5 hr | DONE (13 spawns) |
| Add console-subscriber to logging.rs | 0.5 hr | DONE |
| Create spawn_named! macro | 0.5 hr | DONE |
| Create cargo-make profiling tasks | 1 hr | Pending |

**Usage:**
```bash
# Build with tokio-console support
RUSTFLAGS="--cfg tokio_unstable" cargo build --features tokio-console

# Run your binary
./target/debug/your-binary

# In another terminal
tokio-console
```

### Phase 3: Benchmark Expansion (1 week)

| Task | Effort | Impact |
|------|--------|--------|
| Add Complex DAG benchmark | 4 hr | Complexity coverage |
| Add Tree benchmark | 4 hr | Parallelism measurement |
| Add Decision benchmark | 4 hr | Dynamic step coverage |
| Add Python/TypeScript language benchmarks | 4 hr | FFI comparison |

### Phase 4: Load Testing (1 week)

| Task | Effort | Impact |
|------|--------|--------|
| Implement LoadGenerator struct | 8 hr | Core infrastructure |
| Add completion tracker | 4 hr | Throughput measurement |
| Create ramp-up test scenario | 4 hr | Saturation detection |
| Add system metrics collection | 4 hr | Bottleneck identification |

### Phase 5: Analysis & Optimization (Subsequent Tickets)

Based on profiling and benchmark results, create targeted optimization tickets:
- Database query optimization
- Channel tuning
- Connection pool sizing
- Async pattern improvements

---

## Follow-up Tickets Status

| Ticket | Description | Priority | Status |
|--------|-------------|----------|--------|
| TAS-71 | Add profiling profile and install tooling | P0 | DONE |
| TAS-158 | tokio-console integration + spawn naming | P1 | DONE |
| TAS-159 | Expand E2E benchmarks (DAG, Tree, Decision) | P1 | Pending |
| TAS-160 | Implement load testing framework | P2 | Pending |
| TAS-161 | Profile system and identify optimization targets | P1 | Pending |

---

## Artifacts Produced

```
docs/ticket-specs/TAS-71/
├── research-plan.md                 # Original research plan
├── profiling-tool-evaluation.md     # Profiling tools research (21.8KB)
├── benchmark-audit.md               # Existing benchmark audit (19.9KB)
├── tokio-analysis.md                # Tokio runtime analysis (14.5KB)
├── e2e-benchmark-design.md          # E2E benchmark designs (12.3KB)
├── load-testing-design.md           # Load testing framework (13.9KB)
└── research-synthesis.md            # This synthesis document
```

---

## Success Criteria Met

| Criterion | Status |
|-----------|--------|
| Tooling selected | ✅ samply, tokio-console, dhat-rs, tracing-flame |
| Existing benchmarks audited | ✅ 9 files reviewed, gaps documented |
| Benchmark designs documented | ✅ E2E and load testing designs complete |
| Foundation in place | ✅ Ready for implementation |
| Follow-up tickets identified | ✅ 7 follow-up tickets suggested |

---

## Key Metrics to Establish (Post-Implementation)

### Performance Baselines (TBD)

| Metric | Pattern | Single Instance | Cluster (2x4) |
|--------|---------|-----------------|---------------|
| P95 Latency | Linear | TBD | TBD |
| P95 Latency | Diamond | TBD | TBD |
| P95 Latency | Complex DAG | TBD | TBD |
| Throughput | Linear | TBD tasks/sec | TBD tasks/sec |
| Saturation Point | - | TBD tasks/sec | TBD tasks/sec |

### Resource Baselines (TBD)

| Metric | Idle | Under Load | Saturated |
|--------|------|------------|-----------|
| CPU Usage | TBD | TBD | TBD |
| Memory Usage | TBD | TBD | TBD |
| DB Connections | TBD | TBD | TBD |

---

## Conclusion

TAS-71 research phase is complete. The team now has:

1. **Clear tooling recommendations** for profiling the Rust/tokio/PostgreSQL stack
2. **Detailed audit** of existing benchmarks with actionable cleanup items
3. **Comprehensive designs** for E2E and load testing benchmarks
4. **Deep understanding** of tokio runtime patterns and optimization opportunities
5. **Prioritized roadmap** for implementation work

The next step is to begin Phase 1 (Quick Wins) to enable profiling, followed by Phase 2 (Tooling Foundation) to integrate tokio-console. With these tools in place, the team can gather performance data to inform targeted optimizations.

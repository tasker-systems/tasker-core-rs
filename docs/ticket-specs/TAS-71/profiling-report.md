# TAS-161: Profiling Report

**Date:** 2026-01-22
**Profile Duration:** ~2 minutes under benchmark load
**Workload:** `cargo make bench-e2e` (Tier 1 + Tier 2)

---

## Executive Summary

Both orchestration server and Rust worker are **I/O bound, not CPU bound**. Over 93% of CPU time is spent waiting for I/O (network, disk, IPC). The actual application code accounts for less than 10% of CPU time.

**Key Finding:** No significant CPU bottlenecks were identified. The system's latency is dominated by:
1. Database round-trips (SQLx/PostgreSQL)
2. Message queue operations (RabbitMQ)
3. Async runtime scheduling overhead

---

## Orchestration Server Profile

**Total Samples:** 112,754 (across 5 tokio-runtime-worker threads)

### CPU Time Distribution

| Category | Self Time | Notes |
|----------|-----------|-------|
| Idle (pthread wait) | 60.0% | `__psynch_cvwait` - waiting for work |
| Idle (I/O wait) | 32.9% | `kevent` - waiting for network/events |
| Stdout logging | 2.45% | `write` syscall |
| Network recv | 1.17% | `__recvfrom` |
| Network send | 0.78% | `__sendto` |
| **Application code** | **~3%** | Actual tasker logic |

### Tasker Code Breakdown (Total Time)

| Component | % of Total | Samples |
|-----------|------------|---------|
| `command_processor_actor` | 4.31% | 4,856 |
| `result_processor` | 4.03-4.11% | 4,542 |
| `step_enqueuer` | 2.49% | 2,808 |
| `task_finalizer` | 2.27% | 2,563 |
| `task_initializer` | 1.22% | 1,378 |
| `step_state_machine` | 1.22% | 1,378 |
| `task_state_machine` | 0.38% | 427 |

### Tracing/Logging Overhead

| Component | % of Total | Samples |
|-----------|------------|---------|
| `tracing_subscriber` total | 2.70% | 3,046 |
| `Instrumented` futures | 1.05% | 1,182 |
| `fmt::write` | 0.46% | 516 |
| Stdout `write` syscall | 2.45% | 2,760 |

**Total logging overhead: ~5.15%** (tracing + stdout)

### Serialization (serde_json)

| Operation | % of Total |
|-----------|------------|
| JSON deserialization | 0.40% |
| JSON serialization | 0.03% |
| Drop JSON values | 0.11% |

**Total serialization: ~0.54%** - minimal impact

### Allocation/Cloning

| Operation | % of Total |
|-----------|------------|
| `malloc` variants | 0.12% |
| `Clone` impls | 0.02% |
| `drop_in_place` | 0.15% |

**Total allocation overhead: ~0.29%** - minimal impact

---

## Rust Worker Profile

**Total Samples:** 44,489 (across 3 tokio-runtime-worker threads)

### CPU Time Distribution

| Category | Self Time | Notes |
|----------|-----------|-------|
| Idle (pthread wait) | 60.6% | `__psynch_cvwait` |
| Idle (I/O wait) | 34.1% | `kevent` |
| Network recv | 1.09% | `__recvfrom` |
| Network send | 0.86% | `__sendto` |
| Stdout logging | 0.24% | `write` syscall |
| **Application code** | **~3%** | Actual tasker logic |

### Tasker Code Breakdown (Total Time)

| Component | % of Total | Samples |
|-----------|------------|---------|
| `WorkerCore::start` | 3.77% | 1,677 |
| `actor_command_processor` | 2.55% | 1,134 |
| `step_executor_actor` | 2.49-2.52% | 1,108-1,123 |
| `step_state_machine` | 1.55% | 688 |
| `step_claim::get_task_seq` | 1.37% | 610 |
| `completion_processor` | 1.20% | 533 |
| `task_handler_registry` | 0.63% | 280 |

### Database Operations (SQLx)

| Operation | % of Total | Samples |
|-----------|------------|---------|
| Connection waiting | 1.32% | 587 |
| Query execution | 1.28% | 569 |
| Pool acquire | 0.85% | 376 |
| Stream recv | 1.18% | 524 |

**Total database overhead: ~4.6%** - significant portion of worker time

---

## Optimization Candidates

Based on profiling data, prioritized by impact:

### Tier 1: Low-Hanging Fruit

| Target | Current | Potential Savings | Effort |
|--------|---------|-------------------|--------|
| **Stdout logging** | 2.45% (orch) | ~2% | Low |
| **Tracing spans** | 2.7% (orch) | ~1% | Medium |

**Recommendations:**
1. Consider buffered/async logging for production
2. Evaluate `tracing::enabled!` guards for debug-level spans
3. Review if all spans are necessary in hot paths

### Tier 2: Medium Effort

| Target | Current | Potential | Effort |
|--------|---------|-----------|--------|
| **DB connection pool** | 4.6% (worker) | ~1-2% | Medium |
| **JSON serialization** | 0.54% | Negligible | N/A |

**Recommendations:**
1. Profile database query patterns (which queries are slow?)
2. Consider connection pool tuning
3. JSON overhead is minimal - MessagePack unlikely to help significantly

### Tier 3: Architectural (Not Recommended)

| Target | Reason |
|--------|--------|
| MessagePack | JSON overhead only 0.54% - not worth complexity |
| Clone optimization | Only 0.02% - no hot spots found |
| Memory layout | No allocation hot spots identified |

---

## Conclusions

1. **The system is I/O bound**, not CPU bound. 93%+ of time is spent waiting.

2. **Logging overhead is the largest optimization target** at ~5% combined (tracing + stdout).

3. **Database operations dominate worker time** at ~4.6% - consider connection pool tuning.

4. **Serialization (serde_json) is minimal** at 0.54% - MessagePack migration not justified.

5. **No clone/allocation hot spots** - struct layout optimization not needed.

6. **Actor overhead is reasonable** - no single actor dominates CPU time.

---

## Follow-up Tickets

Based on this profiling analysis, the following optimization tickets were created or updated:

| Ticket | Title | Priority | Impact |
|--------|-------|----------|--------|
| [TAS-162](https://linear.app/tasker-systems/issue/TAS-162) | Hot Path Logging Optimization | High | ~2-3% CPU |
| [TAS-134](https://linear.app/tasker-systems/issue/TAS-134) | Atomic Counters for Simple Metrics | Medium | Lock contention (counters) |
| [TAS-163](https://linear.app/tasker-systems/issue/TAS-163) | Concurrent Data Structures for HashMaps | Medium | Lock contention (maps) |
| [TAS-164](https://linear.app/tasker-systems/issue/TAS-164) | Connection Pool Observability | Medium | Observability |

**Note:** TAS-134 and TAS-163 are complementary:
- TAS-134 handles simple counter metrics → atomic operations
- TAS-163 handles HashMap-based state → DashMap/tokio::sync

See `docs/ticket-specs/TAS-71/optimization-tickets.md` for detailed specifications.

---

## Profiling Commands

```bash
# Build with profiling symbols
cargo make bp

# Profile orchestration server
cargo make rso  # Then run benchmarks, Ctrl-C to collect

# Profile Rust worker
cargo make rswr  # Then run benchmarks, Ctrl-C to collect

# Analyze profiles
cargo make pa    # Full analysis
cargo make pat   # Tasker-specific code only
```

---

## Raw Data

Profile JSON files saved to: `docs/ticket-specs/TAS-71/profile-data/`
- `tasker-server 2026-01-22 08.02 profile.json` (3.1MB)
- `rust-worker 2026-01-22 08.03 profile.json` (1.5MB)

Analysis tool: `cargo-make/scripts/profiling/analyze_profile.py`

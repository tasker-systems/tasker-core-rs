# Historical Ticket Specifications Reference

This document provides a reference to historical ticket specifications that shaped Tasker Core's architecture. The detailed specs have been archived, but key insights have been extracted to appropriate documentation locations (ADRs, principles, architecture docs).

For active ticket specs, see the directories in this folder (TAS-92 and later).

## Ticket Reference

| Ticket | Title | Summary | Linear |
|--------|-------|---------|--------|
| TAS-29 | Observability & Benchmarking | OpenTelemetry integration, correlation IDs, SQL benchmarking | [TAS-29](https://linear.app/tasker-systems/issue/TAS-29) |
| TAS-32 | Enqueued State Architecture | Step-level idempotency via "Enqueued" state | [TAS-32](https://linear.app/tasker-systems/issue/TAS-32) |
| TAS-34 | OrchestrationExecutor Trait | Executor pool system replacing naive polling loops | [TAS-34](https://linear.app/tasker-systems/issue/TAS-34) |
| TAS-37 | Task Finalizer Race Fix | Atomic claim SQL functions for finalization | [TAS-37](https://linear.app/tasker-systems/issue/TAS-37) |
| TAS-40 | Worker Foundations | Rust WorkerSystem with FFI integration | [TAS-40](https://linear.app/tasker-systems/issue/TAS-40) |
| TAS-41 | Pure Rust Worker | Standalone Rust worker with handler registry | [TAS-41](https://linear.app/tasker-systems/issue/TAS-41) |
| TAS-42 | Ruby Binding Simplification | Simplified Ruby to pure business logic | [TAS-42](https://linear.app/tasker-systems/issue/TAS-42) |
| TAS-43 | Event-Driven Task Claiming | PostgreSQL LISTEN/NOTIFY for task discovery | [TAS-43](https://linear.app/tasker-systems/issue/TAS-43) |
| TAS-47 | Blog Post Migration | Migrated Rails examples to tasker-core with Ruby FFI | [TAS-47](https://linear.app/tasker-systems/issue/TAS-47) |
| TAS-49 | DLQ & Lifecycle Management | Dead letter queue with staleness detection | [TAS-49](https://linear.app/tasker-systems/issue/TAS-49) |
| TAS-50 | Configuration System | TOML-based hierarchical configuration | [TAS-50](https://linear.app/tasker-systems/issue/TAS-50) |
| TAS-53 | DecisionPoint Steps | Dynamic workflow branching with convergence | [TAS-53](https://linear.app/tasker-systems/issue/TAS-53) |
| TAS-54 | Ownership Removal | Audit-only processor UUID (see ADR) | [TAS-54](https://linear.app/tasker-systems/issue/TAS-54) |
| TAS-56 | CI Stabilization | Pipeline reliability improvements | [TAS-56](https://linear.app/tasker-systems/issue/TAS-56) |
| TAS-57 | Backoff Consolidation | Unified retry/backoff strategy (see ADR) | [TAS-57](https://linear.app/tasker-systems/issue/TAS-57) |
| TAS-58 | Rust Standards Compliance | Microsoft/Rust API Guidelines implementation | [TAS-58](https://linear.app/tasker-systems/issue/TAS-58) |
| TAS-59 | Batch Processing | Cursor-based batch processing with checkpoints | [TAS-59](https://linear.app/tasker-systems/issue/TAS-59) |
| TAS-60 | Configuration Bug Fix | Duplicate of TAS-50 | [TAS-60](https://linear.app/tasker-systems/issue/TAS-60) |
| TAS-61 | Configuration v2 | Environment-based config architecture | [TAS-61](https://linear.app/tasker-systems/issue/TAS-61) |
| TAS-64 | Retry E2E Testing | Retryability and resumability test suite | [TAS-64](https://linear.app/tasker-systems/issue/TAS-64) |
| TAS-65 | Domain Events | Fire-and-forget domain event publication | [TAS-65](https://linear.app/tasker-systems/issue/TAS-65) |
| TAS-67 | Dual Event System | Non-blocking dual-channel worker pattern (see ADR) | [TAS-67](https://linear.app/tasker-systems/issue/TAS-67) |
| TAS-69 | Worker Decomposition | Actor-based worker refactor (see ADR) | [TAS-69](https://linear.app/tasker-systems/issue/TAS-69) |
| TAS-72 | Python Worker | PyO3-based Python worker foundations | [TAS-72](https://linear.app/tasker-systems/issue/TAS-72) |
| TAS-75 | Backpressure | Unified backpressure handling strategy | [TAS-75](https://linear.app/tasker-systems/issue/TAS-75) |

## Related Documentation

- [Architecture Decision Records](../decisions/) - Key architectural decisions
- [Principles](../principles/) - Core design principles
- [CHRONOLOGY](../CHRONOLOGY.md) - Development timeline

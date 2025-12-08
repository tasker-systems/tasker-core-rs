# TAS-67 Edge Cases and Risks Index

**Status**: Superseded by comprehensive analysis (2025-12-06)

This document has been expanded into a comprehensive analysis grounded in Tasker's design principles.

## Comprehensive Documentation

The complete edge cases and risks analysis is now in:

**[04-edge-cases-and-risks.md](./04-edge-cases-and-risks.md)**

This comprehensive document covers 13 edge cases with:
- Comparison of main vs feature branch behavior
- Mapping to Tasker design principles (idempotency, atomicity, state machines, retry semantics)
- Risk assessment (likelihood, impact, risk level)
- Mitigation status and recommendations

## Edge Cases Covered

| # | Edge Case | Risk Level |
|---|-----------|------------|
| 1 | Dispatch channel backpressure | LOW |
| 2 | Handler timeout | LOW |
| 3 | Handler panic | LOW |
| 4 | Lock poisoning recovery | LOW |
| 5 | Concurrent step execution | LOW |
| 6 | Semaphore acquisition failure | MEDIUM |
| 7 | FFI polling starvation | MEDIUM |
| 8 | FFI completion correlation | LOW |
| 9 | Domain event ordering | LOW |
| 10 | Graceful shutdown | LOW |
| 11 | Completion channel backpressure | MEDIUM |
| 12 | Handler registry race condition | LOW |
| 13 | FFI thread runtime context | MEDIUM |

## Quick Summary

The TAS-67 dual-channel refactor **significantly improves** edge case handling:

- **Improved**: Domain event ordering, timeout detection, panic handling, lock poisoning, bounded concurrency
- **Trade-offs**: Backpressure cascading, FFI thread runtime context
- **All risks grounded in**: Tasker's state machine guards, retry semantics, crash recovery (TAS-54)

---

*See [04-edge-cases-and-risks.md](./04-edge-cases-and-risks.md) for the complete analysis.*

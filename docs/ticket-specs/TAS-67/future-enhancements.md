# TAS-67: Future Enhancements

**Status**: Deferred (Post-TAS-67 Completion)
**Priority**: Low - Optimize based on production metrics

---

## 1. Parallel Completion Processor

**Current State**: `CompletionProcessorService` processes completions sequentially.

**Location**: `tasker-worker/src/worker/handlers/completion_processor.rs:95-109`

```rust
// Current: Sequential processing
while let Some(result) = self.completion_receiver.recv().await {
    self.process_completion(result).await;  // Blocks on each
}
```

**Proposed Enhancement**: Apply the same semaphore-bounded parallelism pattern used in `HandlerDispatchService`.

### Implementation Outline

1. Add `concurrency_semaphore: Arc<Semaphore>` to `CompletionProcessorService`
2. Wrap `FFICompletionService` in `Arc` for sharing across spawned tasks
3. Spawn tasks instead of awaiting directly
4. Add `max_concurrent_completions` to TOML configuration

### Effort Estimate

| Item | Lines | Complexity |
|------|-------|------------|
| Config changes | ~10 | Low |
| Arc wrapper | ~5 | Low |
| Semaphore + spawn pattern | ~30 | Medium |
| TOML config | ~20 | Low |
| Tests | ~50 | Medium |
| **Total** | ~115 | **Low-Medium** |

### When to Implement

This becomes valuable when:
- Completion rate exceeds ~10-20/sec (current sequential max throughput)
- Completion channel backpressure warnings appear in logs
- Benchmark testing reveals completion processing as bottleneck

**Decision**: Handler execution is typically much slower than completion processing, so completions rarely queue up. Monitor in production before implementing.

### Reference Implementation

See `HandlerDispatchService` in `tasker-worker/src/worker/handlers/dispatch_service.rs` for the parallel execution pattern with:
- Semaphore-bounded concurrency (lines 148-149, 253-313)
- Fire-and-forget task spawning (lines 251-361)
- Graceful semaphore closure handling

---

## 2. Channel Saturation Monitoring Alerts

**Current State**: TAS-51 `ChannelMonitor` baseline exists with logging.

**Enhancement**: Add production alerting rules for Prometheus/Grafana.

**When to Implement**: When deploying to production with monitoring infrastructure.

---

## 3. Comprehensive Handler Dispatch Test Suite

**Current State**: E2E tests cover happy paths and error scenarios.

**Enhancement**: Add targeted tests for:
- Concurrent handler execution limits
- Semaphore exhaustion under load
- Callback timeout behavior
- Channel backpressure scenarios

**When to Implement**: Before major production scaling.

---

## Related Documentation

- [Risk Mitigation Plan](./risk-mitigation-plan.md) - Completed mitigations
- [Edge Cases and Risks](./04-edge-cases-and-risks.md) - Addressed risks
- [RCA: Parallel Execution Timing Bugs](../../architecture-decisions/rca-parallel-execution-timing-bugs.md) - Lessons learned

# ADR: TAS-51 Bounded MPSC Channel Migration

**Status**: Implemented
**Date**: 2025-10-14
**Decision Makers**: Engineering Team
**Related Tickets**: TAS-51, TAS-40, TAS-46, TAS-43

## Context and Problem Statement

Prior to TAS-51, the tasker-core system had inconsistent and risky MPSC channel usage:

1. **Unbounded Channels (3 critical sites)**: Risk of unbounded memory growth under load
   - PGMQ notification listener: Could exhaust memory during notification bursts
   - Event publisher: Vulnerable to event storms
   - Ruby FFI handler: No backpressure across FFI boundary

2. **Configuration Disconnect (6 sites)**: TOML configuration existed but wasn't used
   - Hard-coded values (100, 1000) with no rationale
   - Test/dev/prod environments used identical capacities
   - No ability to tune without code changes

3. **No Backpressure Strategy**: Missing overflow handling policies
   - No monitoring of channel saturation
   - No documented behavior when channels fill
   - No metrics for operational visibility

### Production Impact

- **Memory Risk**: OOM possible under high database notification load (10k+ tasks enqueued)
- **Operational Pain**: Cannot tune channel sizes without code deployment
- **Environment Mismatch**: Test environment uses production-scale buffers, masking issues
- **Technical Debt**: Wasted configuration infrastructure

## Decision

**Migrate to 100% bounded, configuration-driven MPSC channels with explicit backpressure handling.**

### Key Principles

1. **All Channels Bounded**: Zero `unbounded_channel()` calls in production code
2. **Configuration-Driven**: All capacities from TOML with environment overrides
3. **Separation of Concerns**: Infrastructure (sizing) separate from business logic (retry behavior)
4. **Explicit Backpressure**: Document and implement overflow policies
5. **Full Observability**: Metrics for channel saturation and overflows

## Solution Architecture

### Configuration Structure

Created unified MPSC channel configuration in `config/tasker/base/mpsc_channels.toml`:

```toml
[mpsc_channels]

# Orchestration subsystem
[mpsc_channels.orchestration.command_processor]
command_buffer_size = 1000

[mpsc_channels.orchestration.event_listeners]
pgmq_event_buffer_size = 10000  # Large for notification bursts

# Task readiness subsystem
[mpsc_channels.task_readiness.event_channel]
buffer_size = 1000
send_timeout_ms = 1000

# Worker subsystem
[mpsc_channels.worker.command_processor]
command_buffer_size = 1000

[mpsc_channels.worker.in_process_events]
broadcast_buffer_size = 1000  # Rust → Ruby FFI

# Shared/cross-cutting
[mpsc_channels.shared.event_publisher]
event_queue_buffer_size = 5000

[mpsc_channels.shared.ffi]
ruby_event_buffer_size = 1000

# Overflow policy
[mpsc_channels.overflow_policy]
log_warning_threshold = 0.8  # Warn at 80% full
drop_policy = "block"
```

### Environment-Specific Overrides

**Production** (`config/tasker/environments/production/mpsc_channels.toml`):
- Orchestration command: 5000 (5x base)
- PGMQ listeners: 50000 (5x base) - handles bulk task creation bursts
- Event publisher: 10000 (2x base)

**Development** (`config/tasker/environments/development/mpsc_channels.toml`):
- Task readiness: 500 (0.5x base)
- Worker FFI: 500 (0.5x base)

**Test** (`config/tasker/environments/test/mpsc_channels.toml`):
- Orchestration command: 100 (0.1x base) - exposes backpressure issues
- Task readiness: 100 (0.1x base)

### Critical Implementation Detail

**Environment override files MUST use full `[mpsc_channels.*]` prefix:**

```toml
# ✅ CORRECT
[mpsc_channels.task_readiness.event_channel]
buffer_size = 100

# ❌ WRONG - creates top-level key that overrides correct config
[task_readiness.event_channel]
buffer_size = 100
```

This was discovered during implementation when environment files created conflicting top-level configuration keys.

### Configuration Migration

Migrated MPSC sizing fields from `event_systems.toml` to `mpsc_channels.toml`:

**Moved to mpsc_channels.toml:**
- `event_systems.task_readiness.metadata.event_channel.buffer_size`
- `event_systems.task_readiness.metadata.event_channel.send_timeout_ms`
- `event_systems.worker.metadata.in_process_events.broadcast_buffer_size`

**Kept in event_systems.toml (event processing logic):**
- `event_systems.task_readiness.metadata.event_channel.max_retries`
- `event_systems.task_readiness.metadata.event_channel.backoff`

**Rationale**: Separation of concerns - infrastructure sizing vs business logic behavior.

### Rust Type System

Created comprehensive type system in `tasker-shared/src/config/mpsc_channels.rs`:

```rust
pub struct MpscChannelsConfig {
    pub orchestration: OrchestrationChannelsConfig,
    pub task_readiness: TaskReadinessChannelsConfig,
    pub worker: WorkerChannelsConfig,
    pub shared: SharedChannelsConfig,
    pub overflow_policy: OverflowPolicyConfig,
}
```

All channel creation sites updated to use configuration:

```rust
// Before
let (tx, rx) = mpsc::unbounded_channel();

// After
let buffer_size = config.mpsc_channels
    .orchestration.event_listeners.pgmq_event_buffer_size;
let (tx, rx) = mpsc::channel(buffer_size);
```

### Observability

**ChannelMonitor Integration:**
- Tracks channel usage in real-time
- Logs warnings at 80% saturation
- Exposes metrics via OpenTelemetry

**Metrics Available:**
- `mpsc_channel_usage_percent` - Current channel fill percentage
- `mpsc_channel_capacity` - Configured capacity
- Component and channel name labels for filtering

## Consequences

### Positive

1. **Memory Safety**: Bounded channels prevent OOM from unbounded growth
2. **Operational Flexibility**: Tune channel sizes via configuration without code changes
3. **Environment Appropriateness**: Test uses small buffers (exposes issues), production uses large buffers (handles load)
4. **Observability**: Channel saturation visible in metrics and logs
5. **Documentation**: Clear guidelines for future channel additions

### Negative

1. **Backpressure Complexity**: Must handle full channel conditions
2. **Configuration Overhead**: More configuration files to maintain
3. **Tuning Required**: May need adjustment based on production load patterns

### Neutral

1. **No Performance Impact**: Bounded channels with appropriate sizes perform identically to unbounded
2. **Backward Compatible**: Existing deployments automatically use new defaults

## Implementation Notes

### Backpressure Strategies by Component

**PGMQ Notification Listener:**
- Strategy: Block sender (apply backpressure)
- Rationale: Cannot drop database notifications
- Buffer: Large (10000 base, 50000 production) to handle bursts

**Event Publisher:**
- Strategy: Drop events with metrics when full
- Rationale: Internal events are non-critical
- Buffer: Medium (5000 base, 10000 production)

**Ruby FFI Handler:**
- Strategy: Return error to Rust (signal backpressure)
- Rationale: Ruby must handle gracefully
- Buffer: Standard (1000) with monitoring

### Sizing Guidelines

**Command Channels (orchestration, worker):**
- Base: 1000
- Test: 100 (expose issues)
- Production: 2000-5000 (concurrent load)

**Event Channels:**
- Base: 1000
- Production: Higher if event-driven architecture

**Notification Channels:**
- Base: 10000 (burst handling)
- Production: 50000 (bulk operations)

## Validation

### Testing Performed

1. **Unit Tests**: Configuration loading and validation ✅
2. **Integration Tests**: All tests pass with bounded channels ✅
3. **Local Verification**: Service starts successfully in test environment ✅
4. **Configuration Verification**: All environments load correctly ✅

### Success Criteria Met

- ✅ Zero unbounded channels in production code
- ✅ 100% configurable channel capacities
- ✅ Environment-specific overrides working
- ✅ Backpressure handling implemented
- ✅ Observability through ChannelMonitor
- ✅ All tests passing

## Future Considerations

1. **Dynamic Sizing**: Consider runtime buffer adjustment based on load (not current scope)
2. **Priority Queues**: Allow critical events to bypass overflow drops (evaluate based on metrics)
3. **Notification Coalescing**: Reduce PGMQ notification volume during bursts (future optimization)
4. **Advanced Metrics**: Percentile latencies for channel send operations

## References

- **Ticket**: [TAS-51](https://linear.app/tasker-systems/issue/TAS-51)
- **Implementation PR**: `jcoletaylor/tas-51-bounded-mpsc-channels`
- **Configuration Files**: `config/tasker/base/mpsc_channels.toml`
- **Rust Module**: `tasker-shared/src/config/mpsc_channels.rs`
- **Related ADRs**: TAS-40 (Command Pattern), TAS-46 (Actor Pattern)

## Lessons Learned

1. **Configuration Structure Matters**: Environment override files must use proper prefixes
2. **Separation of Concerns**: Keep infrastructure config (sizing) separate from business logic (retry behavior)
3. **Test Appropriately**: Small buffers in test environment expose backpressure issues early
4. **Migration Strategy**: Moving config fields requires coordinated struct updates across all files
5. **Type Safety**: Rust's type system caught many configuration mismatches during development

---

**Decision**: Approved and Implemented
**Review Date**: 2025-10-14
**Next Review**: 2026-Q1 (evaluate sizing based on production metrics)

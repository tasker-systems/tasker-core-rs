# TAS-43: Event-Driven Task Readiness System

## Executive Summary

**Goal**: Replace polling-based task readiness detection with event-driven architecture using PostgreSQL LISTEN/NOTIFY, achieving <10ms orchestration response times while maintaining reliability through fallback polling.

**Current State**: We have built the event detection infrastructure (triggers, listeners, event types) but have not yet integrated it with the command processing system that actually orchestrates tasks.

## Core Architecture

### The Problem We're Solving

Currently, orchestration polls the database every N seconds to find ready tasks. This creates unnecessary latency and database load. When a step completes, we already update the database - we should use that moment to trigger immediate task readiness processing.

### The Solution: Event-Driven with Command Integration

```
Step Completes → Database Update → Trigger Fires → NOTIFY
    ↓
PostgreSQL LISTEN → TaskReadinessListener → TaskReadinessEvent
    ↓
UnifiedEventCoordinator → OrchestrationCommand::ProcessTaskReadiness
    ↓
CommandProcessor → TaskClaimStepEnqueuer → Immediate Step Enqueueing
```

**Key Principle**: Events are converted to commands. The existing command processor already knows how to claim tasks and enqueue steps - we just need to trigger it via events instead of polling.

## Implementation Status

### ✅ Phase 1: Database Infrastructure (COMPLETE)

**Status**: Fully implemented and tested

**Location**: `migrations/20250828000001_add_task_readiness_triggers.sql`

**What We Built**:
- `notify_task_ready_on_step_transition()` - Fires when step states change
- `notify_task_state_change()` - Fires when task states change  
- `notify_namespace_created()` - Fires when new namespaces are created
- Proper indexes for trigger performance
- Integration with existing `get_task_execution_context()` function

**Key Design Decisions**:
- Reuses proven readiness logic from `get_task_execution_context()`
- Minimal payload size for pg_notify 8KB limit
- Namespace-specific and global notification channels

### ✅ Phase 2: Event Types and Classification (COMPLETE)

**Status**: Fully implemented with proper type safety

**Location**: `tasker-orchestration/src/orchestration/task_readiness/events.rs`

**What We Built**:
- `TaskReadinessEvent` enum with proper variants
- `TaskReadyEvent`, `TaskStateChangeEvent`, `NamespaceCreatedEvent` types
- `ReadinessEventClassifier` for config-driven classification
- Serde serialization for pg_notify payloads

**Key Design Decisions**:
- Config-driven classification (not hardcoded strings)
- Follows same pattern as `ConfigDrivenMessageEvent`
- Exhaustive pattern matching for type safety

### ✅ Phase 3: PostgreSQL LISTEN/NOTIFY Client (COMPLETE)

**Status**: Fully implemented with auto-reconnection

**Location**: `tasker-orchestration/src/orchestration/task_readiness/listener.rs`

**What We Built**:
- `TaskReadinessListener` with connection management
- Auto-reconnection on connection failures
- Namespace-specific channel subscription
- Event classification and dispatching

**Key Design Decisions**:
- Uses sqlx PgListener for robust connection handling
- Sends classified events via mpsc channel
- Supports dynamic namespace addition/removal

### ⚠️ Phase 4: Command Integration (PARTIALLY COMPLETE)

**Status**: Infrastructure exists but not properly connected

**What We Have**:
- `TaskReadinessEventSystem` in `event_systems/` (built but doesn't use commands)
- `UnifiedEventCoordinator` framework (built but not used in bootstrap)
- Command processor infrastructure (exists and works for other events)

**What's Missing**:
1. ❌ `OrchestrationCommand::ProcessTaskReadiness` variant doesn't exist
2. ❌ Command handler for task readiness in `command_processor.rs`
3. ❌ TaskReadinessEventSystem doesn't send commands (tries to process directly)
4. ❌ Bootstrap doesn't use UnifiedEventCoordinator

**Required Implementation**:

```rust
// 1. Add to command_processor.rs - matches actual pg_notify payload structure
pub enum OrchestrationCommand {
    // ... existing variants ...
    ProcessTaskReadiness { 
        task_uuid: Uuid,
        namespace: String,
        priority: i32,
        ready_steps: i32,
        triggered_by: String,  // "step_transition", "task_start", "fallback_polling"
        step_uuid: Option<Uuid>,  // Present for step_transition triggers
        step_state: Option<String>,  // Present for step_transition triggers
        task_state: Option<String>,  // Present for task_start triggers
    },
}

// 2. Add handler in command_processor.rs
OrchestrationCommand::ProcessTaskReadiness { task_uuid, namespace, priority, ready_steps, .. } => {
    // Create synthetic ClaimedTask for task_claim_step_enqueuer
    let claimed_task = self.create_claimed_task(task_uuid, namespace, priority, ready_steps);
    self.task_claim_step_enqueuer.process_claimed_task(&claimed_task).await?;
}

// 3. Fix TaskReadinessEventSystem to use commands
impl EventDrivenSystem for TaskReadinessEventSystem {
    async fn process_event(&mut self, event: SystemEvent) -> TaskerResult<()> {
        if let SystemEvent::TaskReady(task_ready) = event {
            // Don't process directly - send command!
            let command = OrchestrationCommand::ProcessTaskReadiness {
                task_uuid: task_ready.task_uuid,
                namespace: task_ready.namespace,
                priority: task_ready.priority,
                ready_steps: task_ready.ready_steps,
                triggered_by: task_ready.triggered_by.to_string(),
                step_uuid: task_ready.step_uuid,
                step_state: task_ready.step_state,
                task_state: task_ready.task_state,
            };
            self.command_sender.send(command).await?;
        }
        Ok(())
    }
}
```

### ⚠️ Phase 5: Fallback Polling (PARTIALLY COMPLETE)

**Status**: Built but not integrated with command pattern

**Location**: `tasker-orchestration/src/orchestration/task_readiness/fallback_poller.rs`

**What We Have**:
- `ReadinessFallbackPoller` implementation
- Queries `tasker_ready_tasks` view for stale tasks
- Creates synthetic `TaskReadyEvent` events

**What's Missing**:
- ❌ Poller should send commands, not events (same command pattern as event-driven)
- ❌ Not wired into UnifiedEventCoordinator
- ❌ Configuration should come from deployment mode, not separate settings

**Key Insight**: The fallback poller should generate the same `ProcessTaskReadiness` commands as the event system, just with `triggered_by: "fallback_polling"`. This ensures identical processing whether tasks are discovered via events or polling.

### ❌ Phase 6: Bootstrap Integration (NOT COMPLETE)

**Status**: Still using old coordinator pattern

**Current Bootstrap Flow** (incorrect):
```rust
// In OrchestrationCore::start()
let coordinator = EventDrivenOrchestrationCoordinator::new(...);
coordinator.start().await?;
```

**Required Bootstrap Flow**:
```rust
// Should be:
let unified = UnifiedEventCoordinator::new(
    orchestration_core.clone(),
    config.unified_coordinator_config(), // From TOML
).await?;
unified.start().await?;
```

### ❌ Phase 7: Configuration Integration (NOT COMPLETE)

**Status**: Should use deployment mode configuration

**What's Missing**:
1. ❌ Event systems should check deployment mode (EventDrivenOnly, Hybrid, PollingOnly)
2. ❌ Remove individual enabled/disabled flags in favor of deployment mode
3. ❌ Configuration should come from existing TOML sections

**Configuration Strategy**:
- **EventDrivenOnly**: Task readiness via PostgreSQL LISTEN/NOTIFY only
- **Hybrid** (default): Both event-driven and fallback polling enabled
- **PollingOnly**: Fallback polling only (for testing or degraded operation)

```toml
# config/tasker/base/orchestration.toml (existing file)
[orchestration.deployment]
mode = "Hybrid"  # EventDrivenOnly | Hybrid | PollingOnly

[orchestration.task_readiness]
fallback_polling_interval_seconds = 30
age_threshold_seconds = 5
batch_size = 50
```

**No Individual Flags**: Stop having `enabled = true/false` in each system. The deployment mode determines what runs.

## Architectural Issues to Fix

### 1. Duplicate Coordinators
- **Issue**: Both `coordinator.rs` and `enhanced_coordinator.rs` exist with overlapping functionality
- **Fix**: Keep only one coordinator that integrates with command processor

### 2. Missing Command Integration
- **Issue**: Task readiness tries to orchestrate directly instead of using commands
- **Fix**: Convert all events to commands, let command processor handle orchestration

### 3. Unused UnifiedEventCoordinator
- **Issue**: We built it but bootstrap doesn't use it
- **Fix**: Replace EventDrivenOrchestrationCoordinator with UnifiedEventCoordinator in bootstrap

### 4. Configuration Not Connected
- **Issue**: Using hardcoded defaults instead of TOML configuration
- **Fix**: Load all settings from TaskerConfig

### 5. Wrong Abstraction Level
- **Issue**: Event systems trying to do orchestration instead of just event → command conversion
- **Fix**: Event systems should only classify events and send commands

## Success Criteria

1. **<10ms Response Time**: Measured from step completion to new step enqueueing
2. **Zero Polling in Normal Operation**: Events drive all task readiness
3. **Fallback Reliability**: Polling catches any missed events within 30 seconds
4. **Command Integration**: All orchestration goes through command processor
5. **Configuration Driven**: All settings from TOML files
6. **No Code Duplication**: Single coordinator, single event system

## Implementation Priority

1. **CRITICAL**: Add `ProcessTaskReadiness` command and handler
2. **CRITICAL**: Wire TaskReadinessEventSystem to send commands
3. **HIGH**: Replace bootstrap with UnifiedEventCoordinator
4. **HIGH**: Connect configuration to TOML files
5. **MEDIUM**: Remove duplicate coordinators
6. **LOW**: Performance optimization and metrics

## Testing Strategy

### Unit Tests
- ✅ Database trigger firing (complete)
- ✅ Event classification (complete)
- ✅ PostgreSQL listener connection (complete)
- ❌ Command generation from events (needed)
- ❌ Command processing for task readiness (needed)

### Integration Tests
- ❌ End-to-end: step completion → task ready → step enqueued
- ❌ Fallback polling catches missed events
- ❌ Configuration loading from TOML
- ❌ Multi-namespace handling

### Performance Tests
- ❌ Measure latency: trigger → command → enqueue
- ❌ Verify <10ms response time
- ❌ Load test with high event volume

## Deployment Strategy

**No Migration Needed**: This is net new functionality that enhances existing polling without breaking it.

1. **Hybrid Mode Default**: System runs both event-driven and polling by default
2. **Deployment Mode Control**: `orchestration.deployment.mode` in TOML controls behavior
3. **No Breaking Changes**: Existing polling continues to work unchanged
4. **Immediate Rollback**: Change deployment mode to `PollingOnly` if issues arise

## Metrics and Monitoring

**Required Metrics**:
- `task_readiness_events_received`: Counter by event type
- `task_readiness_command_sent`: Counter for commands generated
- `task_readiness_latency`: Histogram from event to command
- `fallback_polls_executed`: Counter for fallback polling
- `fallback_tasks_found`: Counter for tasks found by polling

## Conclusion

The core insight is that **events should trigger commands, not orchestration**. We built excellent event infrastructure but tried to recreate orchestration logic instead of reusing the proven command processor pattern. The fix is straightforward: convert events to commands and let the existing machinery handle the rest.

The system is ~60% complete with all the hard parts (database triggers, PostgreSQL listeners, event classification) done. What remains is the simpler task of wiring events to commands and fixing the bootstrap.

## Implementation Action Plan

### Step 1: Add ProcessTaskReadiness Command
1. Add `ProcessTaskReadiness` variant to `OrchestrationCommand` enum
2. Add handler in `CommandProcessor::process_command()` 
3. Handler creates `ClaimedTask` and delegates to `task_claim_step_enqueuer`

### Step 2: Fix TaskReadinessEventSystem
1. Add command_sender to TaskReadinessEventSystem
2. Convert `TaskReadyEvent` to `ProcessTaskReadiness` command
3. Remove any direct orchestration logic

### Step 3: Fix Fallback Poller
1. Change from sending events to sending commands
2. Use same `ProcessTaskReadiness` command with `triggered_by: "fallback_polling"`

### Step 4: Wire UnifiedEventCoordinator in Bootstrap
1. Replace `EventDrivenOrchestrationCoordinator` with `UnifiedEventCoordinator` in `OrchestrationCore::start()`
2. Configure both `OrchestrationEventSystem` and `TaskReadinessEventSystem`
3. Use deployment mode to control which systems are active

### Step 5: Remove Duplicates
1. Delete `task_readiness/coordinator.rs`
2. Rename `enhanced_coordinator.rs` to `coordinator.rs`
3. Remove performance comparison code from `deployment.rs`

### Step 6: Connect Configuration
1. Read deployment mode from `orchestration.deployment.mode`
2. Remove individual `enabled` flags
3. Load fallback polling settings from TOML
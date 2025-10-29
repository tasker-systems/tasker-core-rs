# TAS-58: Visibility Analysis - Public API vs Internal Implementation

## Problem Statement

77 types are missing Debug implementations. Many of these might be **internal implementation details** that are currently exposed as `pub` when they should be `pub(crate)` or private.

This is a "visibility smell" - if we need Debug on everything public, we might be exposing too much API surface.

## Current Public API Structure

### tasker-orchestration
```rust
pub mod actors;          // Exposes ALL actor types
pub mod orchestration;   // Exposes ALL orchestration internals
```

### tasker-shared
```rust
pub mod state_machine;   // Exposes ALL guards and actions
pub mod metrics;         // Exposes ALL metrics internals
pub mod monitoring;      // Exposes ALL monitoring types
```

## Analysis of 77 Remaining Types

### Category 1: Definitely Internal (Should be pub(crate))

#### Actors (6 types)
**Current:** All pub in actors/ module
**Proposal:** All should be pub(crate) - used only by ActorRegistry

1. `DecisionPointActor` (actors/decision_point_actor.rs:82)
2. `ResultProcessorActor` (actors/result_processor_actor.rs:54)
3. `StepEnqueuerActor` (actors/step_enqueuer_actor.rs:53)
4. `TaskFinalizerActor` (actors/task_finalizer_actor.rs:57)
5. `TaskRequestActor` (actors/task_request_actor.rs:55)

**Reasoning:**
- Actors are internal to ActorRegistry
- External code should use ActorRegistry, not individual actors
- ActorRegistry provides the public interface via message passing
- Individual actors are implementation details

**Impact:** ActorRegistry remains public, but individual actors become internal

#### Event Systems (4 types)
**Current:** All pub in orchestration/event_systems/
**Proposal:** All should be pub(crate)

1. `OrchestrationEventSystem` (orchestration/event_systems/orchestration_event_system.rs:42)
2. `TaskReadinessEventSystem` (orchestration/event_systems/task_readiness_event_system.rs:17)
3. `UnifiedEventCoordinator` (orchestration/event_systems/unified_event_coordinator.rs:32)
4. `OrchestrationFallbackPoller` (orchestration/orchestration_queues/fallback_poller.rs:72)

**Reasoning:**
- Event coordination is internal to OrchestrationCore
- External code bootstraps via OrchestrationCore, not directly with event systems
- These are wiring/plumbing details

#### Hydrators (1 type shown, 3 total)
**Current:** All pub in orchestration/hydration/
**Proposal:** All should be pub(crate)

1. `StepResultHydrator` (orchestration/hydration/step_result_hydrator.rs:47)
2. `TaskRequestHydrator` (already has Debug via derive)
3. `FinalizationHydrator` (already has Debug via derive)

**Reasoning:**
- Message transformation is internal to command processing
- External code sends messages, doesn't need to know about hydration

#### Command Handlers (1 type)
**Current:** pub
**Proposal:** pub(crate)

1. `OrchestrationProcessorCommandHandler` (orchestration/command_processor.rs:292)

**Reasoning:**
- Internal to OrchestrationProcessor
- Command routing is an implementation detail

#### State Machine Internals (5 types in tasker-shared)
**Current:** pub in state_machine module
**Proposal:** pub(crate) or private

1. `PublishTransitionEventAction` (state_machine/actions.rs:236)
2. `TriggerStepDiscoveryAction` (state_machine/actions.rs:672)
3. `StepCanBeEnqueuedForOrchestrationGuard` (state_machine/guards.rs:282)
4. `StepCanBeCompletedFromOrchestrationGuard` (state_machine/guards.rs:316)
5. `StepCanBeFailedFromOrchestrationGuard` (state_machine/guards.rs:350)

**Reasoning:**
- Actions and guards are used internally by TaskStateMachine and StepStateMachine
- Public API is the state machines themselves, not their internal actions/guards
- External code calls state machine methods, doesn't compose actions/guards

#### Metrics Internals (2 types in tasker-shared)
**Current:** pub in metrics module
**Proposal:** pub(crate)

1. `ChannelMetricsRecorder` (metrics/channels.rs:232)
2. `ChannelMetricsRegistry` (metrics/channels.rs:380)

**Reasoning:**
- Internal recording infrastructure
- Public API is the metrics collection interface, not the recorder/registry
- ChannelMonitor might stay public for external metrics collection

### Category 2: Possibly Public (Need Usage Analysis)

#### Bootstrap/Core Types
**Investigation needed:**

1. `ActorRegistry` (actors/registry.rs:53)
   - **Current:** pub
   - **Question:** Should external code access registry directly, or only via OrchestrationCore?
   - **Likely:** pub(crate) - internal to OrchestrationCore

2. `OrchestrationSystemHandle` (orchestration/bootstrap.rs:32)
   - **Current:** pub
   - **Likely:** Must stay pub - returned from bootstrap, used to manage lifecycle
   - **Needs:** Manual Debug implementation

3. `OrchestrationProcessor` (orchestration/command_processor.rs:204)
   - **Current:** pub
   - **Question:** Direct access needed, or only via OrchestrationCore?
   - **Likely:** pub(crate) - internal to OrchestrationCore

4. `OrchestrationCore` (orchestration/core.rs:39)
   - **Current:** pub
   - **Must stay:** pub - main bootstrap entry point
   - **Needs:** Manual Debug implementation

5. `StateManager` (orchestration/state_manager.rs:92)
   - **Current:** pub
   - **Likely:** pub(crate) - internal state management

### Category 3: Definitely Public (Part of API)

These must remain public and need Debug implementations:

1. `JwtAuthenticator` (types/auth.rs:69)
   - Public authentication API
   - Needs manual Debug (hide sensitive keys)

2. `ChannelMonitor` (monitoring/channel_metrics.rs:118)
   - Public monitoring/metrics API
   - Needs manual Debug (AtomicU64 fields)

## Proposed Changes

### Phase 1: Restrict Actor Visibility (6 types → pub(crate))
**Files to modify:**
- `tasker-orchestration/src/actors/decision_point_actor.rs`
- `tasker-orchestration/src/actors/result_processor_actor.rs`
- `tasker-orchestration/src/actors/step_enqueuer_actor.rs`
- `tasker-orchestration/src/actors/task_finalizer_actor.rs`
- `tasker-orchestration/src/actors/task_request_actor.rs`

Change: `pub struct XxxActor` → `pub(crate) struct XxxActor`

**Expected:** 6 warnings removed (no longer need Debug for non-public types)

### Phase 2: Restrict Event System Visibility (4 types → pub(crate))
**Files to modify:**
- `tasker-orchestration/src/orchestration/event_systems/*.rs`
- `tasker-orchestration/src/orchestration/orchestration_queues/fallback_poller.rs`

**Expected:** 4 warnings removed

### Phase 3: Restrict Hydrators (1 type → pub(crate))
**Files to modify:**
- `tasker-orchestration/src/orchestration/hydration/step_result_hydrator.rs`

**Expected:** 1 warning removed

### Phase 4: Restrict State Machine Internals (5 types → pub(crate))
**Files to modify:**
- `tasker-shared/src/state_machine/actions.rs`
- `tasker-shared/src/state_machine/guards.rs`

**Expected:** 5 warnings removed

### Phase 5: Restrict Metrics Internals (2 types → pub(crate))
**Files to modify:**
- `tasker-shared/src/metrics/channels.rs`

**Expected:** 2 warnings removed

### Phase 6: Investigate Core Types
**Decision needed:**
- ActorRegistry - likely pub(crate)
- OrchestrationProcessor - likely pub(crate)
- StateManager - likely pub(crate)

**Expected:** 3-4 warnings removed if made pub(crate)

### Phase 7: Implement Debug for True Public API
**Remaining types after visibility restrictions (estimated ~5-10):**
- OrchestrationSystemHandle
- OrchestrationCore
- JwtAuthenticator
- ChannelMonitor
- Any other verified public API types

## Benefits

### Reduced Public API Surface
- Clearer separation between public API and internal implementation
- Less to document in public API docs
- More freedom to refactor internals without breaking changes

### Fewer Debug Implementations Needed
**Estimated reduction:** 77 → 5-10 types needing Debug
- 18 types can be pub(crate) (no Debug needed)
- 5-10 types remain as true public API (need manual Debug)

### Better Encapsulation
- Actors hidden behind ActorRegistry interface
- Event systems hidden behind OrchestrationCore
- State machine internals hidden behind state machine public methods

### Maintenance Benefits
- Internal types can change without semver implications
- Implementation details not locked into public API
- Clearer intended usage patterns

## Risks and Considerations

### Breaking Changes
- If external code currently uses these types, this is a breaking change
- Need to verify no external dependencies before changing visibility
- Could be a good semver-major change opportunity

### Integration Tests
- Some integration tests might use internal types
- May need to update tests to use public API only
- Tests within the crate can still use pub(crate) types

### Documentation
- May need to update docs to reflect clearer public API
- Remove internal types from public documentation

## Implementation Strategy

1. **Start conservative:** Make actors pub(crate) first (low risk)
2. **Verify:** Ensure tests pass with restricted visibility
3. **Iterate:** Progressively restrict other categories
4. **Document:** Update crate-level docs with clear public API
5. **Final:** Implement Debug only for remaining public types

## Success Metrics

- **Warnings reduced:** 77 → 5-10 (85-90% reduction via visibility)
- **Public API clarity:** Clear separation of public vs internal
- **Compilation:** All tests pass with new visibility
- **Documentation:** Public API docs only show public types

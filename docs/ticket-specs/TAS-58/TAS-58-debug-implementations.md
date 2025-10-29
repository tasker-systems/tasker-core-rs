# TAS-58: Debug Implementation Analysis

**Generated**: 2025-10-28
**Status**: Phase 2 - Debug Implementation Planning

## Summary

**Total Missing Debug Implementations**: 143 types across 4 crates
**Redundant Imports**: 11 (must be manually removed)

## Redundant Imports to Remove (11 total)

### tasker-shared (10)
1. `tasker-shared/src/config/unified_loader.rs:342` - `use std::env;` (already imported at top)
2. `tasker-shared/src/database/migrator.rs:1` - `use sqlx;` (extern prelude)
3. `tasker-shared/src/errors.rs:5` - `use anyhow;` (extern prelude)
4. `tasker-shared/src/messaging/message.rs:7` - `use serde_json;` (extern prelude)
5. `tasker-shared/src/models/factories/base.rs:181` - `use fastrand;` (extern prelude, in test module)
6. `tasker-shared/src/registry/task_handler_registry.rs:218` - `named_task::NamedTask` (duplicate import)
7. `tasker-shared/src/registry/task_handler_registry.rs:218` - `task_namespace::TaskNamespace` (duplicate import)
8. `tasker-shared/src/registry/task_handler_registry.rs:468` - `named_task::NamedTask` (duplicate import)
9. `tasker-shared/src/registry/task_handler_registry.rs:468` - `task_namespace::TaskNamespace` (duplicate import)
10. `tasker-shared/src/system_context.rs:519` - `use uuid::Uuid;` (already imported at top, in test module)

### tasker-orchestration (1)
11. `tasker-orchestration/src/orchestration/viable_step_discovery.rs:446` - `SqlFunctionExecutor` (duplicate import in test)

## Debug Implementations Needed by Crate

### tasker-shared (40 types)

#### Simple Derive (12 types)
Types with all fields implementing Debug - can use `#[derive(Debug)]`:

1. **QueueClassifier** (`config/queue_classification.rs:25`)
   - Fields: `OrchestrationOwnedQueues`, `String`, `String` (all have Debug)

2. **ChannelMetricsRecorder** (`metrics/channels.rs:232`)
   - Fields: `ChannelMonitor`
   - Note: `ChannelMonitor` needs Debug first (see below)

3. **TransitionDescriptionFormatter** (`models/core/workflow_step_transition.rs:995`)
   - Empty struct

4. **PrometheusMetricsExporter** (`resilience/metrics.rs:236`)
   - Empty struct

5. **SpecialQuery** (`scopes/common.rs:88`)
   - Enum with `Uuid` fields (has Debug)

6. **UpdateTaskCompletionAction** (`state_machine/actions.rs:307`)
   - Empty struct

7. **UpdateStepResultsAction** (`state_machine/actions.rs:346`)
   - Empty struct

8. **ErrorStateCleanupAction** (`state_machine/actions.rs:713`)
   - Empty struct

9. **TransitionGuard** (`state_machine/guards.rs:21`)
   - Empty struct

10. **AllStepsCompleteGuard** (`state_machine/guards.rs:117`)
    - Empty struct

11. **StepDependenciesMetGuard** (`state_machine/guards.rs:140`)
    - Empty struct

12. **TaskNotInProgressGuard** (`state_machine/guards.rs:163`)
    - Empty struct

13. **StepNotInProgressGuard** (`state_machine/guards.rs:185`)
    - Empty struct

14. **TaskCanBeResetGuard** (`state_machine/guards.rs:207`)
    - Empty struct

15. **StepCanBeRetriedGuard** (`state_machine/guards.rs:241`)
    - Empty struct

16. **StepCanBeEnqueuedForOrchestrationGuard** (`state_machine/guards.rs:275`)
    - Empty struct

17. **StepCanBeCompletedFromOrchestrationGuard** (`state_machine/guards.rs:309`)
    - Empty struct

18. **StepCanBeFailedFromOrchestrationGuard** (`state_machine/guards.rs:343`)
    - Empty struct

19. **TaskTransitionPersistence** (`state_machine/persistence.rs:57`)
    - Empty struct

20. **StepTransitionPersistence** (`state_machine/persistence.rs:163`)
    - Empty struct

#### Manual Implementation (28 types)
Types with non-Debug fields requiring manual implementation:

21. **SqlFunctionExecutor** (`database/sql_functions.rs:93`)
    - Field: `PgPool` (no Debug)
    - Pattern: Show "PgPool" as string

22. **FunctionRegistry** (`database/sql_functions.rs:685`)
    - Field: `SqlFunctionExecutor`
    - Dependency: Needs #21 first

23. **ChannelMetricsRegistry** (`metrics/channels.rs:380`)
    - Field: `tokio::sync::RwLock<Vec<ChannelMetricsRecorder>>`
    - Pattern: Show lock state

24. **ChannelMonitor** (`monitoring/channel_metrics.rs:118`)
    - Fields: Multiple `Arc<AtomicU64>`
    - Pattern: Load atomic values for Debug output
    - Note: We already have this in pgmq-notify - copy pattern

25. **TaskHandlerRegistry** (`registry/task_handler_registry.rs:108`)
    - Field: `PgPool`
    - Pattern: Show "PgPool" as string

26. **WorkflowStepEdgeScope** (`scopes/edges.rs:12`)
    - Field: `QueryBuilder<'static, Postgres>` (no Debug)
    - Pattern: Show "QueryBuilder" as string

27. **NamedTaskScope** (`scopes/named_task.rs:11`)
    - Field: `QueryBuilder<'static, Postgres>`
    - Pattern: Show "QueryBuilder" as string

28. **TaskNamespaceScope** (`scopes/named_task.rs:135`)
    - Field: `QueryBuilder<'static, Postgres>`
    - Pattern: Show "QueryBuilder" as string

29. **TaskScope** (`scopes/task.rs:12`)
    - Field: `QueryBuilder<'static, Postgres>`
    - Pattern: Show "QueryBuilder" as string

30. **TaskTransitionScope** (`scopes/transitions.rs:13`)
    - Field: `QueryBuilder<'static, Postgres>`
    - Pattern: Show "QueryBuilder" as string

31. **WorkflowStepTransitionScope** (`scopes/transitions.rs:140`)
    - Field: `QueryBuilder<'static, Postgres>`
    - Pattern: Show "QueryBuilder" as string

32. **WorkflowStepScope** (`scopes/workflow_step.rs:13`)
    - Field: `QueryBuilder<'static, Postgres>`
    - Pattern: Show "QueryBuilder" as string

33. **TransitionActions** (`state_machine/actions.rs:32`)
    - Field: `PgPool`
    - Pattern: Show "PgPool" as string

34. **PublishTransitionEventAction** (`state_machine/actions.rs:234`)
    - Field: `Arc<EventPublisher>`
    - Pattern: Show as "EventPublisher"

35. **TriggerStepDiscoveryAction** (`state_machine/actions.rs:668`)
    - Field: `Arc<EventPublisher>`
    - Pattern: Show as "EventPublisher"

36. **StepStateMachine** (`state_machine/step_state_machine.rs:20`)
    - Fields: `PgPool`, `Arc<EventPublisher>`
    - Pattern: Combine patterns

37. **TaskStateMachine** (`state_machine/task_state_machine.rs:20`)
    - Fields: `PgPool`, `Arc<EventPublisher>`
    - Pattern: Combine patterns

38. **JwtAuthenticator** (`types/auth.rs:69`)
    - Fields: `EncodingKey`, `DecodingKey` (no Debug for security)
    - Pattern: Hide sensitive keys, show config

39. **StepTransitiveDependenciesQuery** (`models/orchestration/step_transitive_dependencies.rs:121`)
    - Field: `PgPool`
    - Pattern: Show "PgPool" as string

40. **ChannelMetricsRecorder** (`metrics/channels.rs:232`) - Already listed in simple, but depends on #24

### tasker-orchestration (50 types)

#### Simple Derive (11 types)

41. **OrchestrationBootstrap** (`orchestration/bootstrap.rs:167`)
    - Empty struct

42. **FinalizationHydrator** (`orchestration/hydration/finalization_hydrator.rs:39`)
    - Empty struct

43. **TaskRequestHydrator** (`orchestration/hydration/task_request_hydrator.rs:41`)
    - Empty struct

44. **SystemEventsManager** (`orchestration/system_events.rs:148`)
    - Field: `Arc<SystemEventsConfig>` (has Debug)

45-50. **Various empty/simple handler structs** (to be detailed)

#### Manual Implementation (39 types)

Types requiring manual Debug due to complex fields:

51. **DecisionPointActor** (`actors/decision_point_actor.rs:82`)
    - Fields: `Arc<SystemContext>`, `Arc<DecisionPointService>`
    - Pattern: Show type names

52. **ActorRegistry** (`actors/registry.rs:53`)
    - Field: `Arc<SystemContext>` + many actor fields
    - Pattern: Show actor availability

53. **ResultProcessorActor** (`actors/result_processor_actor.rs:54`)
    - Fields: `Arc<SystemContext>`, `Arc<OrchestrationResultProcessor>`
    - Pattern: Show type names

54. **StepEnqueuerActor** (`actors/step_enqueuer_actor.rs:53`)
    - Fields: `Arc<SystemContext>`, `Arc<StepEnqueuerService>`
    - Pattern: Show type names

55. **TaskFinalizerActor** (`actors/task_finalizer_actor.rs:57`)
    - Fields: `Arc<SystemContext>`, `TaskFinalizer`
    - Pattern: Show type names

56. **TaskRequestActor** (`actors/task_request_actor.rs:55`)
    - Fields: `Arc<SystemContext>`, `Arc<TaskRequestProcessor>`
    - Pattern: Show type names

57. **OrchestrationSystemHandle** (`orchestration/bootstrap.rs:32`)
    - Multiple `Arc<T>` fields
    - Pattern: Show system state

58. **OrchestrationProcessor** (`orchestration/command_processor.rs:204`)
    - Complex with channels and monitors
    - Pattern: Show statistics

59. **OrchestrationProcessorCommandHandler** (`orchestration/command_processor.rs:292`)
    - Fields: `Arc<SystemContext>`, `Arc<ActorRegistry>`
    - Pattern: Show type names

60. **OrchestrationCore** (`orchestration/core.rs:39`)
    - Core system with multiple Arc fields
    - Pattern: Show system state

61. **StateManager** (`orchestration/state_manager.rs:92`)
    - Fields: `Arc<SystemContext>`, `HashMap<Uuid, T>` in Mutex
    - Pattern: Show count of state machines

62-90. **Remaining orchestration types** (similar patterns)

### tasker-client (2 types)

91. **TemplateValidator** (`config/templates/validation.rs:47`)
    - Unknown fields - needs investigation

92. **[One more type]** - needs identification

### tasker-worker (17 types)

93-109. **Worker-related types** - needs detailed analysis

### tasker-worker-rust (36 types)

110-143. **Rust worker handler types** - most likely simple derives (empty handler structs)

## Implementation Strategy

### Phase 1: Simple Derives (Quick Wins)
- Estimated time: 30 minutes
- Apply `#[derive(Debug)]` to ~50 simple types
- Includes all empty structs and enums

### Phase 2: Standard Manual Patterns
- Estimated time: 1-2 hours
- Types with `PgPool`: Show "PgPool" as string (~15 types)
- Types with `QueryBuilder`: Show "QueryBuilder" as string (~7 types)
- Types with `Arc<T>`: Show type name (~20 types)

### Phase 3: Complex Manual Implementations
- Estimated time: 1-2 hours
- Types with atomics: Load and show values (~5 types)
- Types with locks: Show lock state (~3 types)
- Types with sensitive data: Hide appropriately (~2 types)

### Phase 4: Worker Crates
- Estimated time: 1 hour
- Most worker handlers are likely empty structs
- Can be batch-applied with simple derives

## Standard Patterns

### Pattern 1: PgPool Fields
```rust
impl std::fmt::Debug for MyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MyType")
            .field("pool", &"PgPool")
            .field("other_field", &self.other_field)
            .finish()
    }
}
```

### Pattern 2: QueryBuilder Fields
```rust
impl std::fmt::Debug for MyScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MyScope")
            .field("query", &"QueryBuilder")
            .field("has_conditions", &self.has_conditions)
            .finish()
    }
}
```

### Pattern 3: Arc/Complex Types
```rust
impl std::fmt::Debug for MyActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MyActor")
            .field("context", &"SystemContext")
            .field("service", &"ServiceType")
            .finish()
    }
}
```

### Pattern 4: Atomic Fields (from pgmq-notify/ChannelMonitor)
```rust
impl std::fmt::Debug for ChannelMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelMonitor")
            .field("channel_name", &self.channel_name)
            .field("messages_sent", &self.messages_sent.load(Ordering::Relaxed))
            .field("messages_received", &self.messages_received.load(Ordering::Relaxed))
            .finish()
    }
}
```

## Next Steps

1. Remove 11 redundant imports
2. Apply simple derives (Phase 1)
3. Apply standard manual patterns (Phase 2-3)
4. Worker crates (Phase 4)
5. Validate zero warnings
6. Run full test suite

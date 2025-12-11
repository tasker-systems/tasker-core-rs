# AGENTS.md - Tasker Orchestration Crate

This file provides detailed context for AI agents working on the `tasker-orchestration` crate.

## Module Organization

```
tasker-orchestration/src/orchestration/
├── actors/                          # Actor pattern implementation
│   ├── traits.rs                    # OrchestrationActor, Handler<M>, Message traits
│   ├── registry.rs                  # ActorRegistry with lifecycle management
│   ├── task_request_actor.rs        # Task initialization actor
│   ├── result_processor_actor.rs    # Result processing actor
│   ├── step_enqueuer_actor.rs       # Step enqueueing actor
│   └── task_finalizer_actor.rs      # Task finalization actor
│
├── hydration/                       # Message hydration layer
│   ├── step_result_hydrator.rs      # PGMQ message → StepExecutionResult
│   ├── task_request_hydrator.rs     # PGMQ message → TaskRequestMessage
│   └── finalization_hydrator.rs     # PGMQ message → task_uuid
│
├── lifecycle/                       # Decomposed services
│   ├── task_initialization/         # Task init service components
│   ├── result_processing/           # Result processing components
│   ├── step_enqueuer_services/      # Step enqueuer components
│   └── task_finalization/           # Task finalization components
│       ├── service.rs               # Main TaskFinalizer (~200 lines)
│       ├── completion_handler.rs
│       ├── event_publisher.rs
│       ├── execution_context_provider.rs
│       └── state_handlers.rs
│
├── event_systems/                   # Event-driven coordination
├── command_processor.rs             # Pure routing to actors
└── core.rs                          # Bootstrap with ActorRegistry
```

## Key Architectural Principles

1. **Actors**: Message-based coordination with type-safe `Handler<M>` trait
2. **Hydration**: PGMQ message transformation layer (raw messages → domain types)
3. **Services**: Focused components with single responsibility (<300 lines each)
4. **Command Processor**: Pure routing without business logic
5. **No Wrapper Layers**: Direct actor calls from command processor

## Actor Traits

```rust
pub trait OrchestrationActor: Send + Sync {
    fn started(&self) -> impl Future<Output = TaskerResult<()>> + Send;
    fn stopped(&self) -> impl Future<Output = TaskerResult<()>> + Send;
}

pub trait Handler<M: Message>: OrchestrationActor {
    type Response: Send;
    fn handle(&self, msg: M) -> impl Future<Output = Self::Response> + Send;
}
```

## Command → Actor Mapping

| Command | Actor | Message |
|---------|-------|---------|
| `InitializeTask` | TaskRequestActor | `ProcessTaskRequestMessage` |
| `ProcessStepResult` | ResultProcessorActor | `ProcessStepResultMessage` |
| `FinalizeTask` | TaskFinalizerActor | `FinalizeTaskMessage` |
| `ProcessTaskReadiness` | StepEnqueuerActor | Coordination message |

## Core Components

### OrchestrationCore (`core.rs`)
- Unified bootstrap system
- Builds ActorRegistry with all 4 actors
- Manages database pools, circuit breakers, executor pools
- Single entry point preventing configuration mismatches

### ActorRegistry (`actors/registry.rs`)
- Central registry managing 4 actors
- Lifecycle management: `started()` and `stopped()` hooks
- Shared system context with database pools and circuit breakers

### Command Processor (`command_processor.rs`)
- Pure routing to actors (~100 lines)
- No business logic - delegates everything to actors
- Atomic operations through actor delegation

## Task Execution Flow

1. **Task Initialization**: Client sends TaskRequestMessage via pgmq_send_with_notify
   - Command processor routes to **TaskRequestActor**
   - Actor delegates to TaskInitializer service
2. **Step Discovery**: Orchestration discovers ready steps using SQL functions
   - **StepEnqueuerActor** coordinates batch processing
3. **Step Enqueueing**: Ready steps enqueued to namespace queues
   - Actor delegates to StepEnqueuerService
4. **Result Processing**: Orchestration processes results, discovers next steps
   - **ResultProcessorActor** handles step result processing
5. **Task Completion**: All steps complete, task finalized atomically
   - **TaskFinalizerActor** handles finalization

## Related Documentation

- Actor pattern details: `docs/actors.md`
- Events and commands: `docs/events-and-commands.md`
- State machines: `docs/states-and-lifecycles.md`
- SQL functions: `docs/task-and-step-readiness-and-execution.md`
- TAS-46 spec: `docs/ticket-specs/TAS-46/`

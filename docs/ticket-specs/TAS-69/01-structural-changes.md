# TAS-69 Structural Changes Analysis

## Overview

This document analyzes the structural changes in the TAS-69 Worker Command-Actor-Service refactor, comparing the main branch structure to the feature branch.

## Directory Structure Comparison

### Main Branch (Before)
```
tasker-worker/src/worker/
├── command_processor.rs      # 1575 lines - monolithic implementation
├── core.rs                   # WorkerCore initialization
├── orchestration_result_sender.rs
├── step_claim.rs
├── task_template_manager.rs
├── event_publisher.rs
├── event_router.rs
└── event_systems/
    ├── mod.rs
    ├── domain_event_system.rs
    └── worker_event_system.rs
```

### Feature Branch (After)
```
tasker-worker/src/worker/
├── actor_command_processor.rs  # NEW: Pure routing (~300 LOC)
├── actors/                     # NEW: 5 actors + registry
│   ├── mod.rs
│   ├── traits.rs
│   ├── messages.rs
│   ├── registry.rs
│   ├── step_executor_actor.rs
│   ├── ffi_completion_actor.rs
│   ├── template_cache_actor.rs
│   ├── domain_event_actor.rs
│   └── worker_status_actor.rs
├── hydration/                  # NEW: Message transformation
│   ├── mod.rs
│   └── step_message_hydrator.rs
├── services/                   # NEW: Decomposed services
│   ├── mod.rs
│   ├── step_execution/
│   │   ├── mod.rs
│   │   └── service.rs          # StepExecutorService
│   ├── ffi_completion/
│   │   ├── mod.rs
│   │   └── service.rs          # FFICompletionService
│   └── worker_status/
│       ├── mod.rs
│       └── service.rs          # WorkerStatusService
├── command_processor.rs        # REDUCED: Types only (123 LOC)
├── core.rs                     # MODIFIED: Uses ActorRegistry
├── orchestration_result_sender.rs  # Preserved
├── step_claim.rs               # Preserved
├── task_template_manager.rs    # Preserved
├── event_publisher.rs          # Preserved
├── event_router.rs             # Preserved
└── event_systems/              # Preserved
```

## File Additions

### New Directories

| Directory | Purpose | Files |
|-----------|---------|-------|
| `actors/` | Actor pattern implementation | 9 files |
| `hydration/` | Message transformation layer | 2 files |
| `services/` | Decomposed business logic | 6 files |

### New Files (17 total)

#### Actor Layer (`actors/`)
| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | ~30 | Module exports |
| `traits.rs` | ~60 | WorkerActor, Handler<M>, Message traits |
| `messages.rs` | ~150 | Typed message definitions |
| `registry.rs` | ~320 | WorkerActorRegistry lifecycle management |
| `step_executor_actor.rs` | ~260 | Step execution coordination |
| `ffi_completion_actor.rs` | ~150 | FFI completion handling |
| `template_cache_actor.rs` | ~100 | Template cache management |
| `domain_event_actor.rs` | ~80 | Domain event dispatching |
| `worker_status_actor.rs` | ~180 | Status and health reporting |

#### Hydration Layer (`hydration/`)
| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | ~10 | Module exports |
| `step_message_hydrator.rs` | ~100 | PGMQ message → actor message transformation |

#### Service Layer (`services/`)
| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | ~20 | Module exports |
| `step_execution/mod.rs` | ~10 | Step execution exports |
| `step_execution/service.rs` | ~400 | StepExecutorService |
| `ffi_completion/mod.rs` | ~10 | FFI completion exports |
| `ffi_completion/service.rs` | ~200 | FFICompletionService |
| `worker_status/mod.rs` | ~10 | Worker status exports |
| `worker_status/service.rs` | ~200 | WorkerStatusService |

#### Command Processor
| File | Lines | Purpose |
|------|-------|---------|
| `actor_command_processor.rs` | ~350 | Pure routing command processor |

## File Modifications

### Major Reductions

| File | Before | After | Change |
|------|--------|-------|--------|
| `command_processor.rs` | 1575 | 123 | -92% |

### Modified Files

| File | Change Type | Description |
|------|-------------|-------------|
| `core.rs` | Modified | Uses ActorCommandProcessor, TaskTemplateManager sharing |
| `lib.rs` | Modified | Updated module documentation |
| `mod.rs` | Modified | Added new module exports |

## Files Preserved (Unchanged Logic)

These files retain their original functionality:

| File | Purpose | Status |
|------|---------|--------|
| `orchestration_result_sender.rs` | Send results to orchestration | Preserved |
| `step_claim.rs` | Step claiming logic | Preserved |
| `task_template_manager.rs` | Template management | Preserved |
| `event_publisher.rs` | Event publishing | Preserved |
| `event_router.rs` | Event routing | Preserved |
| `event_systems/` | Event system implementations | Preserved |

## Code Distribution Analysis

### Before (Main Branch)
- **command_processor.rs**: 1575 lines (monolithic)
- Single file contained all command handling logic

### After (Feature Branch)
- **actor_command_processor.rs**: ~350 lines (pure routing)
- **actors/**: ~1100 lines (5 actors + registry + traits)
- **services/**: ~800 lines (3 services)
- **hydration/**: ~110 lines (message transformation)

### LOC Summary

| Component | Main | Feature | Delta |
|-----------|------|---------|-------|
| Command Processing | 1575 | 350 | -1225 |
| Actor Layer | 0 | 1100 | +1100 |
| Service Layer | 0 | 800 | +800 |
| Hydration Layer | 0 | 110 | +110 |
| **Total** | 1575 | 2360 | +785 |

**Note**: Total LOC increases due to:
1. Explicit trait definitions and documentation
2. Test coverage in each module
3. Comprehensive message type definitions
4. Registry lifecycle management

However, the **effective complexity per file** decreases significantly:
- Largest file: 1575 → ~400 lines
- Average file size: ~150 lines
- Single responsibility per file

## Module Dependency Graph

```
┌─────────────────────────────────────────────────────────────────┐
│                        WorkerCore                                │
│                            │                                     │
│                            ▼                                     │
│                 ActorCommandProcessor                            │
│                            │                                     │
│          ┌─────────────────┼─────────────────┐                  │
│          ▼                 ▼                 ▼                  │
│   WorkerActorRegistry    Hydration    (Direct Routing)          │
│          │                                                       │
│    ┌─────┼─────┬─────────┬──────────┬────────────┐              │
│    ▼     ▼     ▼         ▼          ▼            ▼              │
│  Step  FFI   Template  Domain   Worker        Messages          │
│  Exec  Comp  Cache     Event    Status                          │
│  Actor Actor Actor     Actor    Actor                           │
│    │     │     │         │        │                             │
│    ▼     ▼     ▼         ▼        ▼                             │
│  Step  FFI   Template  Domain  Worker                           │
│  Exec  Comp  Manager   Event   Status                           │
│  Svc   Svc   (exist)   System  Svc                              │
│                        (exist)                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Key Structural Patterns

### 1. Actor-Service Separation
Each actor wraps a corresponding service:
- `StepExecutorActor` → `StepExecutorService`
- `FFICompletionActor` → `FFICompletionService`
- `WorkerStatusActor` → `WorkerStatusService`

### 2. Trait-Based Polymorphism
```rust
// All actors implement WorkerActor
pub trait WorkerActor: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn context(&self) -> &Arc<SystemContext>;
    fn started(&mut self) -> TaskerResult<()>;
    fn stopped(&mut self) -> TaskerResult<()>;
}

// Message handlers use Handler<M> trait
#[async_trait]
pub trait Handler<M: Message>: WorkerActor {
    async fn handle(&self, msg: M) -> TaskerResult<M::Response>;
}
```

### 3. Registry Pattern
`WorkerActorRegistry` manages all actors:
- Centralized lifecycle management
- Consistent initialization order
- Unified shutdown coordination

### 4. Hydration Layer
Transforms PGMQ messages to typed actor messages:
- `PgmqMessage<SimpleStepMessage>` → `ExecuteStepMessage`
- `MessageReadyEvent` → `ExecuteStepFromEventMessage`

## Implications

### Benefits
1. **Maintainability**: Each file has a single responsibility
2. **Testability**: Services can be tested in isolation
3. **Extensibility**: New actors/services follow established patterns
4. **Readability**: Smaller files are easier to understand

### Considerations
1. **Navigation**: More files to navigate
2. **Indirection**: Extra layer between command and execution
3. **Coordination**: Registry must manage actor dependencies

## Summary

The TAS-69 refactor transforms a monolithic 1575-line command processor into a well-structured actor-based system with:
- 5 specialized actors
- 3 decomposed services
- 1 hydration layer
- Pure routing command processor

This mirrors the orchestration architecture (TAS-46) for consistency across the codebase.

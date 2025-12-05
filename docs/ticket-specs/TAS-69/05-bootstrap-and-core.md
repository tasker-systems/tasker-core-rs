# TAS-69 Bootstrap and Core Changes Analysis

## Overview

This document analyzes the changes to `WorkerCore` and the bootstrap process to integrate the actor-based architecture from TAS-69.

## WorkerCore Architecture

### Before (Main Branch)

```rust
pub struct WorkerCore {
    context: Arc<SystemContext>,
    task_template_manager: Arc<TaskTemplateManager>,
    command_sender: mpsc::Sender<WorkerCommand>,
    processor_handle: Option<JoinHandle<()>>,
    event_system_handle: Option<WorkerEventSystemHandle>,
    domain_event_handle: Option<DomainEventSystemHandle>,
    worker_id: String,
    // ... additional fields
}

impl WorkerCore {
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        // 1. Create TaskTemplateManager
        // 2. Create WorkerProcessor (monolithic)
        // 3. Create command channel
        // 4. Spawn processor task
        // 5. Initialize event systems
    }
}
```

### After (Feature Branch)

```rust
pub struct WorkerCore {
    context: Arc<SystemContext>,
    task_template_manager: Arc<TaskTemplateManager>,
    command_sender: mpsc::Sender<WorkerCommand>,
    processor_handle: Option<JoinHandle<()>>,
    event_system_handle: Option<WorkerEventSystemHandle>,
    domain_event_handle: Option<DomainEventSystemHandle>,
    worker_id: String,
    // Same fields - API preserved
}

impl WorkerCore {
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        // 1. Create TaskTemplateManager
        // 2. Discover namespaces
        // 3. Create ActorCommandProcessor (with shared TaskTemplateManager)
        // 4. Create command channel
        // 5. Spawn processor task
        // 6. Initialize event systems
        // 7. Wire event publisher to actors
    }
}
```

## Key Changes

### 1. Command Processor Initialization

**Before:**
```rust
let worker_id = format!("worker_{}", uuid::Uuid::now_v7());
let (processor, command_sender) = WorkerProcessor::new(
    context.clone(),
    worker_id.clone(),
    task_template_manager.clone(),
    command_buffer_size,
    command_channel_monitor,
);
```

**After:**
```rust
let worker_id = format!("worker_{}", uuid::Uuid::now_v7());
let (mut processor, command_sender) = ActorCommandProcessor::with_task_template_manager(
    context.clone(),
    worker_id.clone(),
    task_template_manager.clone(),
    command_buffer_size,
    command_channel_monitor,
).await?;
```

Key differences:
- `ActorCommandProcessor::with_task_template_manager()` replaces `WorkerProcessor::new()`
- Method is async (returns `TaskerResult`)
- Shares pre-initialized `TaskTemplateManager` for namespace discovery

### 2. TaskTemplateManager Sharing

**Before:**
The `WorkerProcessor` created its own `TaskTemplateManager` or received one without namespace coordination.

**After:**
```rust
// WorkerCore creates and initializes the manager
let task_template_manager = Arc::new(TaskTemplateManager::new(
    context.task_handler_registry.clone(),
));

// Discover namespaces BEFORE passing to processor
task_template_manager.discover_namespaces().await?;

// Share with ActorCommandProcessor
let (mut processor, command_sender) = ActorCommandProcessor::with_task_template_manager(
    context.clone(),
    worker_id.clone(),
    task_template_manager.clone(),  // Shared reference
    command_buffer_size,
    command_channel_monitor,
).await?;
```

This ensures:
1. Namespaces are discovered once at startup
2. All actors share the same namespace configuration
3. Namespace validation works correctly during step execution

### 3. Event Publisher Wiring

**Before:**
Event publisher was created and passed to `WorkerProcessor` during construction.

**After:**
```rust
// Event publisher created after processor
let event_publisher = WorkerEventPublisher::new(command_sender.clone());

// Wired to actors after registry creation
processor.set_event_publisher(event_publisher.clone()).await;
processor.set_domain_event_handle(domain_event_handle.clone()).await;
```

The two-phase initialization is necessary because:
1. `WorkerEventPublisher` needs the command sender
2. Command sender is created by `ActorCommandProcessor::with_task_template_manager()`
3. Actors need the event publisher for FFI handler invocation

### 4. Domain Event Handle Integration

**Before:**
Domain event handle passed to `WorkerProcessor` during construction.

**After:**
```rust
// Create domain event system
let domain_event_handle = DomainEventSystem::new(context.clone()).await?;

// Wire to actors
processor.set_domain_event_handle(domain_event_handle.clone()).await;
```

The actor receives the handle and passes it to the underlying service.

## Constructor Chain

### ActorCommandProcessor Constructors

```rust
impl ActorCommandProcessor {
    /// Standard constructor (creates new TaskTemplateManager)
    pub async fn new(
        context: Arc<SystemContext>,
        worker_id: String,
        command_buffer_size: usize,
        command_channel_monitor: ChannelMonitor,
    ) -> TaskerResult<(Self, mpsc::Sender<WorkerCommand>)>;

    /// Constructor with pre-initialized TaskTemplateManager
    pub async fn with_task_template_manager(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
        command_buffer_size: usize,
        command_channel_monitor: ChannelMonitor,
    ) -> TaskerResult<(Self, mpsc::Sender<WorkerCommand>)>;
}
```

### WorkerActorRegistry Constructors

```rust
impl WorkerActorRegistry {
    /// Standard constructor (creates new TaskTemplateManager)
    pub async fn build(context: Arc<SystemContext>) -> TaskerResult<Self>;

    /// Constructor with custom worker ID
    pub async fn build_with_worker_id(
        context: Arc<SystemContext>,
        worker_id: String,
    ) -> TaskerResult<Self>;

    /// Constructor with pre-initialized TaskTemplateManager
    pub async fn build_with_task_template_manager(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> TaskerResult<Self>;
}
```

### Call Chain

```
WorkerCore::new()
    │
    ├── TaskTemplateManager::new()
    │       └── discover_namespaces()
    │
    ├── ActorCommandProcessor::with_task_template_manager()
    │       │
    │       └── WorkerActorRegistry::build_with_task_template_manager()
    │               │
    │               ├── StepExecutorActor::new(task_template_manager)
    │               ├── FFICompletionActor::new()
    │               ├── TemplateCacheActor::new(task_template_manager)
    │               ├── DomainEventActor::new()
    │               └── WorkerStatusActor::new(task_template_manager)
    │
    ├── WorkerEventPublisher::new(command_sender)
    │
    ├── processor.set_event_publisher()
    │
    └── processor.set_domain_event_handle()
```

## API Preservation

### Public API (Unchanged)

| Method | Signature | Status |
|--------|-----------|--------|
| `new()` | `async fn new(context: Arc<SystemContext>) -> TaskerResult<Self>` | Preserved |
| `send_command()` | `fn send_command(&self, cmd: WorkerCommand) -> Result<(), SendError>` | Preserved |
| `start_event_system()` | `async fn start_event_system(&mut self) -> TaskerResult<()>` | Preserved |
| `stop()` | `async fn stop(&mut self) -> TaskerResult<()>` | Preserved |
| `worker_id()` | `fn worker_id(&self) -> &str` | Preserved |
| `is_running()` | `fn is_running(&self) -> bool` | Preserved |

All public APIs remain unchanged, ensuring backward compatibility.

### Internal Changes (Transparent)

- `WorkerProcessor` → `ActorCommandProcessor`
- Direct handling → Actor delegation
- Single file logic → Distributed across actors/services

## Initialization Sequence

### Complete Bootstrap Flow

```
1. SystemContext created (database pool, message client, etc.)
     │
     ▼
2. WorkerBootstrap::bootstrap()
     │
     ├── Load configuration (TOML)
     ├── Create SystemContext
     │
     ▼
3. WorkerCore::new(context)
     │
     ├── Create TaskTemplateManager
     │     └── discover_namespaces()  ← Critical for validation
     │
     ├── Create ActorCommandProcessor
     │     └── WorkerActorRegistry::build_with_task_template_manager()
     │           │
     │           ├── Create 5 actors
     │           │     ├── StepExecutorActor
     │           │     ├── FFICompletionActor
     │           │     ├── TemplateCacheActor
     │           │     ├── DomainEventActor
     │           │     └── WorkerStatusActor
     │           │
     │           └── Call started() on each actor
     │
     ├── Spawn processor task
     │
     ├── Create DomainEventSystem
     │
     ├── Create WorkerEventPublisher
     │
     ├── Wire event publisher to processor/actors
     │
     └── Wire domain event handle to processor/actors
     │
     ▼
4. WorkerCore ready for commands
     │
     ▼
5. Start WorkerEventSystem (optional, mode-dependent)
     │
     ├── Start queue listeners (EventDriven/Hybrid mode)
     └── Start fallback pollers (Hybrid mode)
```

## Error Handling

### Initialization Errors

```rust
impl WorkerCore {
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        // Actor registry build can fail
        let (mut processor, command_sender) = ActorCommandProcessor::with_task_template_manager(
            context.clone(),
            worker_id.clone(),
            task_template_manager.clone(),
            command_buffer_size,
            command_channel_monitor,
        ).await?;  // Propagates initialization errors

        // ... rest of initialization
    }
}
```

Potential errors:
- Actor `started()` hook failure
- Database connectivity issues
- Configuration validation failures

### Shutdown Handling

```rust
impl WorkerCore {
    pub async fn stop(&mut self) -> TaskerResult<()> {
        // 1. Send Shutdown command
        self.send_command(WorkerCommand::Shutdown { resp })?;

        // 2. Wait for processor task to complete
        if let Some(handle) = self.processor_handle.take() {
            handle.await?;
        }

        // 3. Stop event systems
        if let Some(event_handle) = self.event_system_handle.take() {
            event_handle.stop().await?;
        }

        Ok(())
    }
}
```

## Configuration Integration

### MPSC Channel Configuration

```rust
// From config/tasker/base/mpsc_channels.toml
let command_buffer_size = context.config.mpsc_channels
    .worker
    .command_processor
    .command_buffer_size;

// Channel monitor for observability
let command_channel_monitor = ChannelMonitor::new(
    "worker_command_processor",
    command_buffer_size,
);
```

### Environment-Specific Overrides

```toml
# config/tasker/environments/test/mpsc_channels.toml
[mpsc_channels.worker.command_processor]
command_buffer_size = 100  # Small for testing

# config/tasker/environments/production/mpsc_channels.toml
[mpsc_channels.worker.command_processor]
command_buffer_size = 5000  # Large for production
```

## Testing Implications

### WorkerCore Tests

Existing `WorkerCore` tests continue to pass because:
1. Public API unchanged
2. Internal refactoring transparent to callers
3. Behavior preserved through actor delegation

### Integration Tests

E2E tests validate the full chain:
1. `WorkerCore::new()` initializes correctly
2. Commands route through actors
3. Step execution completes successfully
4. Results sent to orchestration

## Summary

The bootstrap and core changes:

| Aspect | Change | Impact |
|--------|--------|--------|
| Command Processor | `WorkerProcessor` → `ActorCommandProcessor` | Internal |
| Task Template Manager | Shared via constructor | Enables namespace validation |
| Event Publisher | Two-phase wiring | Required for FFI handlers |
| Public API | Unchanged | Backward compatible |
| Configuration | Preserved | No config changes needed |

The changes are internal implementation details, maintaining full backward compatibility with existing code that uses `WorkerCore`.

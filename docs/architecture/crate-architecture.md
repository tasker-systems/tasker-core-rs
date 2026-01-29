# Crate Architecture

**Last Updated**: 2026-01-15
**Audience**: Developers, Architects
**Status**: Active (TAS-46 Actor Architecture, TAS-133 Messaging Abstraction Complete)
**Related Docs**: [Documentation Hub](README.md) | [Actor-Based Architecture](actors.md) | [Events and Commands](events-and-commands.md) | [Quick Start](quick-start.md)

← Back to [Documentation Hub](README.md)

---

## Overview

Tasker Core is organized as a Cargo workspace with 7 member crates, each with a specific responsibility in the workflow orchestration system. This document explains the role of each crate, their inter-dependencies, and how they work together to provide a complete orchestration solution.

### Design Philosophy

The crate structure follows these principles:

1. **Separation of Concerns**: Each crate has a well-defined responsibility
2. **Minimal Dependencies**: Crates depend on the minimum necessary dependencies
3. **Shared Foundation**: Common types and utilities in `tasker-shared`
4. **Language Flexibility**: Support for multiple worker implementations (Rust, Ruby, Python planned)
5. **Production Ready**: Workers and the orchestration system can be deployed and scaled independently

---

## Workspace Structure

```
tasker-core/
├── pgmq-notify/              # PGMQ wrapper with notification support
├── tasker-shared/            # Shared types, SQL functions, utilities
├── tasker-orchestration/     # Task coordination and lifecycle management
├── tasker-worker/            # Step execution and handler integration
├── tasker-client/            # API client library and CLI tools
└── workers/
    ├── ruby/ext/tasker_core/ # Ruby FFI bindings
    └── rust/                 # Rust native worker
```

### Crate Dependency Graph

```
┌─────────────────────────────────────────────────────────┐
│                   External Dependencies                 │
│  (sqlx, tokio, serde, pgmq, magnus, axum, etc.)       │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                    pgmq-notify                          │
│  PGMQ wrapper with PostgreSQL LISTEN/NOTIFY            │
└─────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                    tasker-shared                        │
│  Core types, SQL functions, state machines             │
└─────────────────────────────────────────────────────────┘
                            │
               ┌────────────┴────────────┐
               │                         │
               ▼                         ▼
┌──────────────────────────┐  ┌──────────────────────────┐
│  tasker-orchestration    │  │    tasker-worker         │
│  Task coordination       │  │    Step execution        │
│  Lifecycle management    │  │    Handler integration   │
│  REST API                │  │    FFI support           │
└──────────────────────────┘  └──────────────────────────┘
               │                         │
               ▼                         │
┌──────────────────────────┐            │
│    tasker-client         │            │
│    API client library    │            │
│    CLI tools             │            │
└──────────────────────────┘            │
                                        │
               ┌────────────────────────┘
               │
      ┌────────┴────────┐
      ▼                 ▼
┌────────────┐  ┌────────────┐
│ workers/   │  │ workers/   │
│   ruby/    │  │   rust/    │
│   ext/     │  │            │
└────────────┘  └────────────┘
```

---

## Core Crates

### pgmq-notify

**Purpose**: Wrapper around PostgreSQL Message Queue (PGMQ) with native PostgreSQL LISTEN/NOTIFY support

**Location**: `pgmq-notify/`

**Key Responsibilities**:
- Wrap `pgmq` crate with notification capabilities
- Provide atomic `pgmq_send_with_notify()` operations
- Handle notification channel management
- Support namespace-aware queue naming

**Public API**:
```rust
pub struct PgmqClient {
    // Send message with atomic notification
    pub async fn send_with_notify<T>(&self, queue: &str, msg: T) -> Result<i64>;

    // Read message with visibility timeout
    pub async fn read<T>(&self, queue: &str, vt: i32) -> Result<Option<Message<T>>>;

    // Delete processed message
    pub async fn delete(&self, queue: &str, msg_id: i64) -> Result<bool>;
}
```

**When to Use**:
- When you need reliable message queuing with PostgreSQL
- When you need atomic send + notify operations
- When building event-driven systems on PostgreSQL

**Dependencies**:
- `pgmq` - Core PostgreSQL message queue functionality
- `sqlx` - Database connectivity
- `tokio` - Async runtime

---

### tasker-shared

**Purpose**: Foundation crate containing all shared types, utilities, and SQL function interfaces

**Location**: `tasker-shared/`

**Key Responsibilities**:
- Core domain models (`Task`, `WorkflowStep`, `TaskTransition`, etc.)
- State machine implementations (Task + Step)
- SQL function executor and registry
- Database utilities and migrations
- Event system traits and types
- **Messaging abstraction layer (TAS-133)**: Provider-agnostic messaging with PGMQ, RabbitMQ, and InMemory backends
- Factory system for testing
- Metrics and observability primitives

**Public API**:
```rust
// Core Models
pub mod models {
    pub struct Task { /* ... */ }
    pub struct WorkflowStep { /* ... */ }
    pub struct TaskTransition { /* ... */ }
    pub struct WorkflowStepTransition { /* ... */ }
}

// State Machines
pub mod state_machine {
    pub struct TaskStateMachine { /* ... */ }
    pub struct StepStateMachine { /* ... */ }
    pub enum TaskState { /* 12 states */ }
    pub enum WorkflowStepState { /* 9 states */ }
}

// SQL Functions
pub mod database {
    pub struct SqlFunctionExecutor { /* ... */ }
    pub async fn get_step_readiness_status(...) -> Result<Vec<StepReadinessStatus>>;
    pub async fn get_next_ready_tasks(...) -> Result<Vec<ReadyTaskInfo>>;
}

// Event System
pub mod event_system {
    pub trait EventDrivenSystem { /* ... */ }
    pub enum DeploymentMode { Hybrid, EventDrivenOnly, PollingOnly }
}

// Messaging (TAS-133)
pub mod messaging {
    // Provider abstraction
    pub enum MessagingProvider { Pgmq, RabbitMq, InMemory }
    pub trait MessagingService { /* send_message, receive_messages, ack_message, ... */ }
    pub trait SupportsPushNotifications { /* subscribe, subscribe_many, requires_fallback_polling */ }
    pub enum MessageNotification { Available { ... }, Message(...) }

    // Domain client
    pub struct MessageClient { /* High-level queue operations */ }

    // Message types
    pub struct SimpleStepMessage { /* ... */ }
    pub struct TaskRequestMessage { /* ... */ }
    pub struct StepExecutionResult { /* ... */ }
}
```

**When to Use**:
- **Always** - This is the foundation for all other crates
- When you need core domain models
- When you need state machine logic
- When you need SQL function access
- When you need testing factories

**Dependencies**:
- `pgmq-notify` - Message queue operations
- `sqlx` - Database operations
- `serde` - Serialization
- Many workspace-shared dependencies

**Why It's Separate**:
- Eliminates circular dependencies between orchestration and worker
- Provides single source of truth for domain models
- Enables independent testing of core logic
- Allows multiple implementations (orchestration vs worker) to share code

---

### tasker-orchestration

**Purpose**: Task coordination, lifecycle management, and orchestration REST API

**Location**: `tasker-orchestration/`

**Key Responsibilities**:
- Actor-based lifecycle coordination (TAS-46)
- Task initialization and finalization
- Step discovery and enqueueing
- Result processing from workers
- Dynamic executor pool management
- Event-driven coordination
- REST API endpoints
- Health monitoring
- Metrics collection

**Public API**:
```rust
// Core orchestration
pub struct OrchestrationCore {
    pub async fn new() -> Result<Self>;
    pub async fn from_config(config: ConfigManager) -> Result<Self>;
}

// Actor-based coordination (TAS-46)
pub mod actors {
    pub struct ActorRegistry { /* ... */ }
    pub struct TaskRequestActor { /* ... */ }
    pub struct ResultProcessorActor { /* ... */ }
    pub struct StepEnqueuerActor { /* ... */ }
    pub struct TaskFinalizerActor { /* ... */ }

    pub trait OrchestrationActor { /* ... */ }
    pub trait Handler<M: Message> { /* ... */ }
    pub trait Message { /* ... */ }
}

// Lifecycle services (wrapped by actors)
pub mod lifecycle {
    pub struct TaskInitializer { /* ... */ }
    pub struct StepEnqueuerService { /* ... */ }
    pub struct OrchestrationResultProcessor { /* ... */ }
    pub struct TaskFinalizer { /* ... */ }
}

// Message hydration (Phase 4)
pub mod hydration {
    pub struct StepResultHydrator { /* ... */ }
    pub struct TaskRequestHydrator { /* ... */ }
    pub struct FinalizationHydrator { /* ... */ }
}

// REST API (Axum)
pub mod web {
    // POST /v1/tasks
    pub async fn create_task(request: TaskRequest) -> Result<TaskResponse>;

    // GET /v1/tasks/{uuid}
    pub async fn get_task(uuid: Uuid) -> Result<TaskResponse>;

    // GET /health
    pub async fn health_check() -> Result<HealthResponse>;
}

// gRPC API (Tonic) - TAS-177
// Feature-gated behind `grpc-api`
pub mod grpc {
    pub struct GrpcServer { /* ... */ }
    pub struct GrpcState { /* wraps Arc<SharedApiServices> */ }

    pub mod services {
        pub struct TaskServiceImpl { /* 6 RPCs */ }
        pub struct StepServiceImpl { /* 4 RPCs */ }
        pub struct TemplateServiceImpl { /* 2 RPCs */ }
        pub struct HealthServiceImpl { /* 4 RPCs */ }
        pub struct AnalyticsServiceImpl { /* 2 RPCs */ }
        pub struct DlqServiceImpl { /* 6 RPCs */ }
        pub struct ConfigServiceImpl { /* 1 RPC */ }
    }

    pub mod interceptors {
        pub struct AuthInterceptor { /* Bearer token, API key */ }
    }
}

// Event systems
pub mod event_systems {
    pub struct OrchestrationEventSystem { /* ... */ }
    pub struct TaskReadinessEventSystem { /* ... */ }
}
```

**Actor Architecture** (TAS-46):

The orchestration crate implements a lightweight actor pattern for lifecycle component coordination:

- **ActorRegistry**: Manages all 4 orchestration actors with lifecycle hooks
- **Message-Based Communication**: Type-safe message handling via `Handler<M>` trait
- **Service Decomposition**: Large services decomposed into focused components (<300 lines per file)
- **Direct Integration**: Command processor calls actors directly without wrapper layers

See [Actor-Based Architecture](actors.md) for comprehensive documentation.

**When to Use**:
- When you need to run the orchestration server
- When you need task coordination logic
- When building custom orchestration components
- When integrating with the REST API

**Dependencies**:
- `tasker-shared` - Core types and SQL functions
- `pgmq-notify` - Message queuing
- `axum` - REST API framework
- `tower-http` - HTTP middleware

**Deployment**: Typically deployed as a server process (`tasker-server` binary)

**Dual-Server Architecture (TAS-177)**:

Orchestration supports both REST and gRPC APIs running simultaneously via `SharedApiServices`:

```rust
pub struct SharedApiServices {
    pub security_service: Option<Arc<SecurityService>>,
    pub task_service: TaskService,
    pub step_service: StepService,
    pub health_service: HealthService,
    // ... other services
}

// Both APIs share the same service instances
AppState { services: Arc<SharedApiServices>, ... }   // REST
GrpcState { services: Arc<SharedApiServices>, ... }  // gRPC
```

**Port Allocation**:
- REST: 8080 (configurable)
- gRPC: 9090 (configurable)

---

### tasker-worker

**Purpose**: Step execution, handler integration, and worker coordination

**Location**: `tasker-worker/`

**Key Responsibilities**:
- Claim steps from namespace queues
- Execute step handlers (Rust or FFI)
- Submit results to orchestration
- Template management and caching
- Event-driven step claiming
- Worker health monitoring
- FFI integration layer

**Public API**:
```rust
// Worker core
pub struct WorkerCore {
    pub async fn new(config: WorkerConfig) -> Result<Self>;
    pub async fn start(&mut self) -> Result<()>;
}

// Handler execution
pub mod handlers {
    pub trait StepHandler {
        async fn execute(&self, context: StepContext) -> Result<StepResult>;
    }
}

// Template management
pub mod task_template_manager {
    pub struct TaskTemplateManager {
        pub async fn load_templates(&mut self) -> Result<()>;
        pub fn get_template(&self, name: &str) -> Option<&TaskTemplate>;
    }
}

// Event systems
pub mod event_systems {
    pub struct WorkerEventSystem { /* ... */ }
}
```

**When to Use**:
- When you need to run a worker process
- When implementing custom step handlers
- When integrating with Ruby/Python handlers via FFI
- When building worker-specific tools

**Dependencies**:
- `tasker-shared` - Core types and messaging
- `pgmq-notify` - Message queuing
- `magnus` (optional) - Ruby FFI bindings

**Deployment**: Deployed as worker processes, typically one per namespace or scaled horizontally

---

### tasker-client

**Purpose**: API client library and command-line tools

**Location**: `tasker-client/`

**Key Responsibilities**:
- HTTP client for orchestration REST API
- CLI tools for task management
- Integration testing utilities
- Client-side request building

**Public API**:
```rust
// REST client
pub struct RestOrchestrationClient {
    pub async fn new(base_url: &str) -> Result<Self>;
    // Task, step, template, health operations
}

// gRPC client (TAS-177, feature-gated)
#[cfg(feature = "grpc")]
pub struct GrpcOrchestrationClient {
    pub async fn connect(endpoint: &str) -> Result<Self>;
    pub async fn connect_with_auth(endpoint: &str, auth: GrpcAuthConfig) -> Result<Self>;
    // Same operations as REST client
}

// Transport-agnostic client
pub enum UnifiedOrchestrationClient {
    Rest(Box<RestOrchestrationClient>),
    Grpc(Box<GrpcOrchestrationClient>),
}

// Client trait for transport abstraction
pub trait OrchestrationClient: Send + Sync {
    async fn create_task(&self, request: TaskRequest) -> Result<TaskResponse>;
    async fn get_task(&self, uuid: Uuid) -> Result<TaskResponse>;
    async fn list_tasks(&self, filters: TaskFilters) -> Result<Vec<TaskResponse>>;
    async fn health_check(&self) -> Result<HealthResponse>;
    // ... more operations
}
```

**CLI Tools**:
```bash
# Task management
tasker-cli task create --template linear_workflow
tasker-cli task get <uuid>
tasker-cli task list --namespace payments

# Health checks
tasker-cli health
```

**When to Use**:
- When you need to interact with orchestration API from Rust
- When building integration tests
- When creating CLI tools for task management
- When implementing client applications

**Dependencies**:
- `reqwest` - HTTP client
- `clap` - CLI argument parsing
- `serde_json` - JSON serialization

---

## Worker Implementations

### workers/ruby/ext/tasker_core

**Purpose**: Ruby FFI bindings enabling Ruby workers to execute Rust-orchestrated workflows

**Location**: `workers/ruby/ext/tasker_core/`

**Key Responsibilities**:
- Expose Rust worker functionality to Ruby via Magnus (FFI)
- Handle Ruby handler execution
- Manage Ruby <-> Rust type conversions
- Provide Ruby API for template registration
- FFI performance optimization

**Ruby API**:
```ruby
# Worker bootstrap
result = TaskerCore::Worker::Bootstrap.start!

# Template registration (automatic)
# Ruby templates in workers/ruby/app/tasker/tasks/templates/

# Handler execution (automatic via FFI)
class MyHandler < TaskerCore::StepHandler
  def execute(context)
    # Step implementation
    { success: true, result: "done" }
  end
end
```

**When to Use**:
- When you have existing Ruby handlers
- When you need Ruby-specific libraries or gems
- When migrating from Ruby-based orchestration
- When team expertise is primarily Ruby

**Dependencies**:
- `magnus` - Ruby FFI bindings
- `tasker-worker` - Core worker logic
- Ruby runtime

**Performance Considerations**:
- FFI overhead: ~5-10ms per step (measured)
- Ruby GC can impact latency
- Thread-safe FFI calls via Ruby global lock
- Best for I/O-bound operations, not CPU-intensive

---

### workers/rust

**Purpose**: Native Rust worker implementation for maximum performance

**Location**: `workers/rust/`

**Key Responsibilities**:
- Native Rust step handler execution
- Template definitions in Rust
- Direct integration with tasker-worker
- Maximum performance for CPU-intensive operations

**Handler API**:
```rust
// Define handler in Rust
pub struct MyHandler;

#[async_trait]
impl StepHandler for MyHandler {
    async fn execute(&self, context: StepContext) -> Result<StepResult> {
        // Step implementation
        Ok(StepResult::success(json!({"result": "done"})))
    }
}

// Register in template
pub fn register_template() -> TaskTemplate {
    TaskTemplate {
        name: "my_workflow",
        steps: vec![
            StepTemplate {
                name: "my_step",
                handler: Box::new(MyHandler),
                // ...
            }
        ]
    }
}
```

**When to Use**:
- When you need maximum performance
- For CPU-intensive operations
- When building new workflows in Rust
- When minimizing latency is critical

**Dependencies**:
- `tasker-worker` - Core worker logic
- `tokio` - Async runtime

**Performance**: Native Rust handlers have zero FFI overhead

---

## Crate Relationships

### How Crates Work Together

#### Task Creation Flow
```
Client Application
  ↓ [HTTP POST]
tasker-client
  ↓ [REST API]
tasker-orchestration::web
  ↓ [Task lifecycle]
tasker-orchestration::lifecycle::TaskInitializer
  ↓ [Uses]
tasker-shared::models::Task
tasker-shared::database::sql_functions
  ↓ [PostgreSQL]
Database + PGMQ
```

#### Step Execution Flow
```
tasker-orchestration::lifecycle::StepEnqueuer
  ↓ [pgmq_send_with_notify]
PGMQ namespace queue
  ↓ [pg_notify event]
tasker-worker::event_systems::WorkerEventSystem
  ↓ [Claims step]
tasker-worker::handlers::execute_handler
  ↓ [FFI or native]
workers/ruby or workers/rust
  ↓ [Returns result]
tasker-worker::orchestration_result_sender
  ↓ [pgmq_send_with_notify]
PGMQ orchestration_step_results queue
  ↓ [pg_notify event]
tasker-orchestration::lifecycle::ResultProcessor
  ↓ [Updates state]
tasker-shared::models::WorkflowStepTransition
```

### Dependency Rationale

**Why tasker-shared exists**:
- Prevents circular dependencies (orchestration ↔ worker)
- Single source of truth for domain models
- Enables independent testing
- Allows SQL function reuse

**Why workers are separate from tasker-worker**:
- Language-specific implementations
- Independent deployment
- FFI boundary separation
- Multiple worker types supported

**Why pgmq-notify is separate**:
- Reusable in other projects
- Focused responsibility
- Easy to test independently
- Can be published as separate crate

---

## Building and Testing

### Build All Crates
```bash
# Build everything with all features
cargo build --all-features

# Build specific crate
cargo build --package tasker-orchestration --all-features

# Build workspace root (minimal, mostly for integration)
cargo build
```

### Test All Crates
```bash
# Test everything
cargo test --all-features

# Test specific crate
cargo test --package tasker-shared --all-features

# Test with database
DATABASE_URL="postgresql://..." cargo test --all-features
```

### Feature Flags

```toml
# Root workspace features
[features]
benchmarks = [
    "tasker-shared/benchmarks",
    # ...
]
test-utils = [
    "tasker-orchestration/test-utils",
    "tasker-shared/test-utils",
    "tasker-worker/test-utils",
]
```

---

## Migration Notes

### Root Crate Being Phased Out

The root `tasker-core` crate (defined in the workspace root `Cargo.toml`) is being phased out:

- **Current**: Contains minimal code, mostly workspace configuration
- **Future**: Will be removed entirely, replaced by individual crates
- **Impact**: No functional impact, internal restructuring only
- **Timeline**: Complete when all functionality moved to member crates

**Why**: Cleaner workspace structure, better separation of concerns, easier to understand

### Adding New Crates

When adding a new crate to the workspace:

1. Add to `[workspace.members]` in root `Cargo.toml`
2. Create crate: `cargo new --lib tasker-new-crate`
3. Add workspace dependencies to crate's `Cargo.toml`
4. Update this documentation
5. Add to dependency graph above
6. Document public API

---

## Best Practices

### When to Create a New Crate

Create a new crate when:
- ✅ You have a distinct, reusable component
- ✅ You need independent versioning
- ✅ You want to reduce compile times
- ✅ You need isolation for testing
- ✅ You have language-specific implementations

Don't create a new crate when:
- ❌ It's tightly coupled to existing crates
- ❌ It's only used in one place
- ❌ It would create circular dependencies
- ❌ It's a small utility module

### Dependency Management

- **Use workspace dependencies**: Define versions in root `Cargo.toml`
- **Minimize dependencies**: Only depend on what you need
- **Version consistently**: Use `workspace = true` in member crates
- **Document dependencies**: Explain why each dependency is needed

### API Design

- **Stable public API**: Changes should be backward compatible
- **Clear documentation**: Every public item needs docs
- **Examples in docs**: Show how to use the API
- **Error handling**: Use `Result` with meaningful error types

---

## Related Documentation

- **[Actor-Based Architecture](actors.md)** - Actor pattern implementation in tasker-orchestration
- **[Messaging Abstraction](messaging-abstraction.md)** - Provider-agnostic messaging (TAS-133)
- **[Quick Start](quick-start.md)** - Get running with the crates
- **[Events and Commands](events-and-commands.md)** - How crates coordinate
- **[States and Lifecycles](states-and-lifecycles.md)** - State machines in tasker-shared
- **[Task Readiness & Execution](task-and-step-readiness-and-execution.md)** - SQL functions in tasker-shared
- **[Archive: Ruby Integration Lessons](archive/ruby-integration-lessons.md)** - FFI patterns

---

← Back to [Documentation Hub](README.md)

# Worker Foundations

## Current State

The rust Tasker Core is responsible for orchestration of tasks and their lifecycles, including the task initialization from a TaskRequest, identification of ready tasks and steps, claiming tasks to enqueue steps, and managing task state transitions. It handles finalization and determination of backoff policies, and ensures the scalability and readiness of orchestration executors.

The current Ruby codebase represents a prototype of a Tasker Worker. It manages the execution of steps, including listening on a queue for ready steps, claiming the step for execution, identifying the step's handler, initializing and running the configured handler, and sending consistent and well structured responses back to the Tasker Core, in the case of errors or successful completion, with metadata about the step's execution relevant to the determination of backoff policies.

The crux of this divide is the separation of concerns between the Tasker Core and the Tasker Worker. The Tasker Core is responsible for orchestration and coordination of tasks, while the Tasker Worker is responsible for executing individual steps. This separation allows for greater scalability and flexibility, as the Tasker Core can be scaled independently of the Tasker Worker, and the Tasker Worker can be customized to handle specific types of steps.

The Tasker Core and Tasker Workers share intentional dependencies on the queuing system and the postgresql database, at least insofar as they are required to coordinate tasks and steps. This is an intentional design choice, as this ensures atomicity of task and step execution, which is crucial for maintaining the integrity of the system and ensuring that tasks are processed correctly and consistently.

## Problem Statement

The problem is that the current implementation of the Tasker Worker in Ruby requires significant boostrapping and setup, and requires the ruby codebase to attempt to scalably manage worker threads, database connections, queue connections, and other implementation details that any other binding language will need to support as well. This is not a scalable solution, and it presents dangerous opportunities for drift between intentions of the system. The high degree of cohesion between the Tasker Core and Tasker Workers is intentional and is a valuable aspect of the system's design. It ensures that the Tasker Core and Tasker Workers work together seamlessly, and that the system is able to scale and adapt to changing requirements.

However, we do not want the binding language to need to know or care about any of that. We want the binding language to be a simple mechanism for implementing business logic of step handlers, without having to worry about the underlying infrastructure. This will allow us to focus on the core business logic of the system, while leaving the infrastructure concerns to the Tasker Core and Tasker Workers.

One of the reasons that the ruby codebase was the prototype worker was of course because of the foundational tasker-engine rails engine on which the original concepts for the Tasker Core were based. This meant that I had easy and ready access to ActiveRecord and other niceties that meant I could quickly prototype. But the ease of this also meant that I made a design decision that I don't want to live with over the long term.

At the moment the ruby Worker prototype utilizes concurrent ruby to manage concurrency, and uses ActiveRecord thread-safe connection pools to allow each handler to process a step and manage persisting results and enqueuing these back for investigation by the Tasker Core.

While it is correct that a Worker is responsible for the lifecycle of a step when it is being handled, it is also too much of an implementation detail for the binding language to be expected to manage worker resilience and scalability under load in a way that is consistent across all languages. We lose a core value of common functionality when we delegate Tasker Core / Tasker Worker shared responsibilities to the binding language.

## Future State

I am proposing a new design. Borrowing ideas from our current orchestration system, we will create a rust foundation for the Tasker Worker, a WorkerSystem that utilizes similar concepts to our OrchestrationSystem, our OrchestrationLoopCoordinator, our OrchestratorExecutor system, the health metrics, system limits, graceful shutdown mechanics, etc.

The future worker system will borrow extensively from the current orchestration system and the ruby worker prototype. The ruby system has a number of boostrapping requirements that load task templates, ensure database connections, ensure queue access, etc. This should all be handled by the WorkerSystem, which will manage the lifecycle of the Worker and ensure that it is resilient and scalable under load. The WorkerLoopCoordinator will borrow from the OrchestrationLoopCoordinator and the OrchestratorExecutor system.

The effective workflow will be as follows:

1. The WorkerSystem will bootstrap the worker according to the configuration toml system. We will likely need to split our configuration system into orchestrator|worker directories, but using a common mechanism - the TaskerConfig will need to have a TaskerWorkerConfig that is used to configure the WorkerSystem.
2. On bootstrapping, the worker system will ensure that the configured number of executor threads per executor type and queue namespace are available. The main executor type will be one listening for enqueued steps.
3. When an enqueued step is received, the worker system will use the same database models and queue clients as the tasker core, ensuring consistency and reliability across all components. It will use the logic that the ruby system does currently to read a message from the queue, determine if the step is one that it can handle based on it's own cache of task templates it supports. If it can handle the step, it will claim the step via a state transition of the step from enqueued to in_progress, and it will delete the message from the queue.
4. This is where we now interact with the FFI layer of the binding language. In Ruby, we will be using dry-events as our pub-sub mechanism, and we will have an exposed simple interface in ruby for the rust worker system to call with a magnus-wrapped release-immediately struct.
5. This struct will be have been hydrated by querying the db based on the queue message, getting the relevant information to populate the (task, sequence, step) tuple that all callable step handlers in the Tasker Ecosystem expect. The Sequence is well described in both the rust and ruby codebases so far, and is a mechanism for representing all ancestor dependencies including transitive dependencies of a step. We have this logic already.
6. The rust worker system will call the FFI layer with this struct, publishing an event and payload to the dry-events system.
7. The dry-events system will have configured a listener for the event and payload, which will then call the appropriate step handler, with the correct wrapper logic to ensure that the step is executed in the correct context, and that failures and errors (RetryableError or PermanentError) and metadata are properly captured and populated.
8. The ruby system will then publish an event and payload to the dry-events system, which will have a subscriber that calls the rust worker system to take over and handle the mechanics of evaluating success and failure, serializing the results correctly, managing the state transition of the step to completed or failure, updating the database with the results, and sending the correct message to the queue to notify the orchestration system that the step has completed, so that it can manage the finalization decision making workflow for retryability, completion, or error.

## Benefits

The binding language will no longer have to manage library dependencies for database connections, queue connections, or https request connections with the orchestration system. It can of course use make use of api calls or event systems or any other necessity for step handling, but that should be fully relegated to the implementation of business logic.

The rust worker foundation will manage the mechanics of the Tasker system without requiring any additional dependencies or configurations.

This will mean a massive reduction of the ruby codebase, and significantly increase the reusability of the rust system's shared components, ensuring consistency and reliability no matter the binding language.

## Implications

We will not implement an embedded orchestration system at all. The orchestration core and the worker systems will run as separate, parallel services from day one. The worker will be configured with the endpoint of the orchestration system - this can be behind a load balancer or a reverse proxy for multiple instances of the orchestration system. This clean separation enables immediate scalability and deployment flexibility.

This implies creating a new Docker image based on the release version of the orchestration system.

This also implies the work already outlined for an Axum web service.

The reason this is necessary is the simple need to be able to POST a task request to a /v1/tasks endpoint to create a new task, and get back the task uuid and associated metadata. While the worker has no responsibilities to invoke the enqueuing of steps, this happens automatically via the task claim executor mechanism, it is valuable for the worker to have a record of the task_uuid for correlation logging and tracing, along with integration testing, and even health querying about the task status if needed by the framework that would wrap the worker.

## Implementation Plan

### Phase 1: Core Worker System Infrastructure
**Goal**: Create the Rust foundation for worker execution, mirroring orchestration patterns

#### 1.1 Worker Configuration System
- **Create new configuration components**:
  - `config/tasker/base/worker.toml` - Worker-specific configuration
  - `config/tasker/base/worker_pools.toml` - Worker executor pool configuration
  - Split configuration directories: `config/tasker/orchestrator/` and `config/tasker/worker/`
  - Add `TaskerWorkerConfig` to ConfigManager
  
#### 1.2 Worker Core Module Structure
```
src/worker/
├── mod.rs                     # Module exports
├── worker_core.rs             # WorkerCore bootstrap (similar to OrchestrationCore)
├── worker_system.rs           # WorkerSystem main coordinator
├── worker_loop.rs             # Main worker processing loop
├── step_processor.rs          # Step claim, process, and result handling
├── ffi_bridge.rs              # FFI interface to binding languages
└── coordinator/
    ├── mod.rs
    ├── pool.rs                # WorkerPoolManager
    ├── scaling.rs             # Auto-scaling logic
    ├── health.rs              # Health monitoring
    ├── operational_state.rs   # Shutdown coordination
    └── resource_limits.rs     # Resource validation
```

#### 1.3 WorkerCore Bootstrap System
- Create `WorkerCore` similar to `OrchestrationCore`:
  - Unified bootstrap for worker components
  - Database pool management
  - PGMQ client initialization
  - Configuration loading and validation
  - Operational state management

### Phase 2: Worker Loop Coordinator
**Goal**: Implement the WorkerLoopCoordinator borrowing from OrchestrationLoopCoordinator

#### 2.1 WorkerLoopCoordinator
- **Core responsibilities**:
  - Manage worker executor pools per queue namespace
  - Auto-scaling based on queue depth and processing metrics
  - Health monitoring and reporting
  - Graceful shutdown coordination
  
#### 2.2 Worker Executor Types
- **StepListener**: Listen for enqueued steps on namespace queues
- **StepProcessor**: Process claimed steps through FFI
- **ResultSender**: Send results back to orchestration queue
- **HealthReporter**: Report worker health to orchestration

#### 2.3 Step Processing Workflow
1. **Listen**: Poll namespace queue for enqueued steps
2. **Claim**: Atomically claim step (enqueued → in_progress)
3. **Hydrate**: Load task, sequence, and step data from database
4. **Process**: Call FFI layer with hydrated data
5. **Capture**: Receive result from binding language
6. **Persist**: Update database with results
7. **Notify**: Send completion message to orchestration

### Phase 3: FFI Integration Layer
**Goal**: Create clean FFI interface for binding languages

#### 3.1 FFI Data Structures
```rust
// Structures to pass to binding languages
pub struct StepExecutionRequest {
    pub task: TaskData,
    pub sequence: SequenceData,
    pub step: StepData,
    pub execution_id: String,
}

pub struct StepExecutionResult {
    pub success: bool,
    pub data: serde_json::Value,
    pub error: Option<ErrorData>,
    pub metadata: ExecutionMetadata,
}
```

#### 3.2 Ruby Integration via dry-events
- **Rust → Ruby flow**:
  1. Rust calls `execute_step_ffi` with StepExecutionRequest
  2. Magnus wraps data for Ruby consumption
  3. Publish to dry-events: `TaskerCore::Events.publish('step.execute', payload)`
  4. Ruby handler processes and publishes result
  5. Rust receives result via callback

- **Ruby-side setup**:
  ```ruby
  # New file: lib/tasker_core/worker/event_bridge.rb
  module TaskerCore::Worker
    class EventBridge
      def self.setup!
        TaskerCore::Events.subscribe('step.execute') do |event|
          handler = resolve_handler(event.payload.step)
          result = handler.call(event.payload.task, event.payload.sequence, event.payload.step)
          TaskerCore::Events.publish('step.complete', result)
        end
      end
    end
  end
  ```

### Phase 4: Database and Queue Integration
**Goal**: Reuse existing models and clients for consistency

#### 4.1 Shared Components
- Use existing database models from `src/models/`
- Reuse `PgmqClient` and `UnifiedPgmqClient`
- Share circuit breaker configuration
- Common metrics and telemetry

#### 4.2 Worker-specific Database Operations
- Create `src/worker/database/` module:
  - `step_claimer.rs` - Atomic step claiming logic
  - `result_persister.rs` - Result persistence logic
  - `sequence_builder.rs` - Build sequence from dependencies

### Phase 5: Resource Management and Monitoring
**Goal**: Implement worker-specific resource management

#### 5.1 Worker Resource Validator
- Adapt `ResourceValidator` for worker context:
  - Monitor database connections per worker pool
  - Track memory usage for step processing
  - Validate queue connection availability

#### 5.2 Worker Health Monitoring
- Create `WorkerHealthMonitor`:
  - Track steps processed per second
  - Monitor error rates and retry patterns
  - Queue depth and processing latency
  - Integration with operational state

#### 5.3 Metrics Collection
- Worker-specific metrics:
  - `worker.steps.processed` - Steps successfully processed
  - `worker.steps.failed` - Steps that failed
  - `worker.queue.depth` - Current queue depth
  - `worker.processing.duration` - Step processing time
  - `worker.pool.size` - Current executor pool size

### Phase 6: HTTP API Service
**Goal**: Create Axum web service for task submission

#### 6.1 Web Service Structure
```rust
// src/worker/api/
├── mod.rs
├── server.rs          # Axum server setup
├── routes/
│   ├── mod.rs
│   ├── tasks.rs       # POST /v1/tasks endpoint
│   ├── health.rs      # GET /health endpoint
│   └── metrics.rs     # GET /metrics endpoint
└── middleware/
    ├── auth.rs        # Authentication middleware
    └── logging.rs     # Request logging
```

#### 6.2 API Endpoints
- `POST /v1/tasks` - Create new task request
- `GET /v1/tasks/{uuid}` - Get task status
- `GET /health` - Worker health status
- `GET /metrics` - Prometheus metrics

### Phase 7: Ruby Codebase Simplification
**Goal**: Remove infrastructure code from Ruby, focus on business logic

#### 7.1 Remove from Ruby
- Queue worker thread management
- Database connection pooling
- Queue connection management
- Message parsing and claiming logic
- State transition management

#### 7.2 Keep in Ruby
- Step handler business logic
- Domain-specific processing
- External API integrations
- Business rule validation

#### 7.3 New Ruby Structure
```ruby
# Simplified Ruby worker
module TaskerCore::Worker
  class StepHandler
    def call(task, sequence, step)
      # Pure business logic only
    end
  end
end
```

### Phase 8: Docker and Deployment
**Goal**: Create deployable worker containers

#### 8.1 Docker Images
- Create `Dockerfile.worker`:
  - Based on Rust release build
  - Include Ruby runtime for handlers
  - Minimal dependencies

#### 8.2 Configuration
- Environment-based configuration:
  - `TASKER_ORCHESTRATION_URL` - Orchestration endpoint
  - `TASKER_WORKER_NAMESPACES` - Namespaces to handle
  - `TASKER_WORKER_POOL_SIZE` - Executor pool configuration

### Phase 9: Testing Strategy
**Goal**: Comprehensive testing of worker system

#### 9.1 Unit Tests
- Test each worker component in isolation
- Mock FFI boundaries
- Validate resource management

#### 9.2 Integration Tests
- End-to-end workflow testing
- Multi-worker coordination
- Failure and recovery scenarios

#### 9.3 Performance Tests
- Benchmark step processing throughput (target: 1000+ steps/second from day one)
- Memory usage under load
- Database connection pooling efficiency

#### 9.4 Chaos Testing
- Kill workers mid-processing to verify step recoverability
- Network partition testing between worker and orchestration
- Database connection exhaustion scenarios



### Key Design Decisions

1. **Reuse Existing Patterns**: Leverage proven patterns from orchestration system
2. **FFI via Events**: Use pub-sub pattern for clean language boundary
3. **Shared Database**: Maintain atomicity through shared PostgreSQL
4. **Configuration Parity**: Similar configuration structure for consistency
5. **Resource Isolation**: Separate pools for orchestration vs worker
6. **Operational State**: Unified state management across systems

### Success Criteria

- ✓ Ruby codebase reduced by 70%+ (complete removal of all infrastructure code)
- ✓ Worker system handles 1000+ steps/second from initial release
- ✓ Auto-scaling responds within 10 seconds to load changes
- ✓ Zero race conditions in step claiming with atomic guarantees
- ✓ Clean FFI boundary with zero infrastructure leakage to binding languages
- ✓ Comprehensive metrics and monitoring available from day one
- ✓ Graceful shutdown without data loss in under 5 seconds
- ✓ Production-ready Docker deployment on first release
- ✓ Support for multiple binding languages with identical infrastructure guarantees

This implementation plan provides a direct path to building the Worker Foundations system as a greenfield project, with no legacy constraints or migration concerns. We can implement the ideal architecture from the start.

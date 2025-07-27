# Evolving from ZeroMQ to Tokio-Based TCP Architecture

## Executive Summary

This document outlines the architectural evolution from ZeroMQ-based pub-sub communication to a native Tokio TCP server/client implementation. This change maintains the same bidirectional async pub-sub pattern while eliminating external dependencies and resolving persistent communication reliability issues.

## Current State Analysis

### ZeroMQ Architecture Challenges

1. **Message Delivery Uncertainty**: Despite correct endpoint configuration and topic subscription, Ruby consistently receives `rc=-1` (EAGAIN) errors
2. **Port Binding Conflicts**: Both Rust and Ruby attempt to bind to specific ports, creating deployment complexity
3. **Socket Timing Issues**: ZeroMQ socket establishment and connection sequence creates race conditions
4. **Configuration Complexity**: Separate bind/connect endpoint management across languages
5. **External Dependency**: ZeroMQ adds deployment complexity and potential version conflicts
6. **Limited Observability**: Difficult to debug socket state and message flow issues

### Current ZeroMQ Flow
```
Rust ZmqPubSubExecutor:
  - Binds to batch_endpoint (tcp://127.0.0.1:8555)
  - Connects to result_endpoint (tcp://127.0.0.1:8556)
  - Publishes: "steps {json_batch_request}"

Ruby ZeromqOrchestrator:
  - Connects to step_sub_endpoint (tcp://127.0.0.1:8555)
  - Binds to result_pub_endpoint (tcp://127.0.0.1:8556)
  - Subscribes to "steps" topic
  - Publishes: "partial_result {json}" / "batch_completion {json}"
```

## Proposed Tokio-Based Architecture

### Core Design Principles

1. **Native Tokio Integration**: Leverage existing async runtime for optimal performance
2. **Persistent TCP Connections**: Replace fire-and-forget sockets with reliable connections
3. **Actor-Based Message Handling**: Use Tokio tasks and channels for concurrent processing
4. **Unified Command Pattern**: All operations flow through standardized Command structure
5. **Explicit Worker Registration**: Active worker pool management with capacity tracking
6. **Network Transparency**: FFI operations can migrate to network commands seamlessly
7. **Complete ZeroMQ Removal**: No backward compatibility - total replacement and cleanup

### Architecture Components

#### Rust Side: Command-Based TCP Architecture

```rust
pub struct TokioTcpExecutor {
    server: TcpListener,
    command_router: CommandRouter,
    worker_pool: WorkerPool,
    connection_manager: ConnectionManager,
}

pub struct CommandRouter {
    // Route commands to appropriate handlers
    handlers: HashMap<CommandType, Box<dyn CommandHandler>>,
    
    // Command lifecycle management
    pending_commands: HashMap<String, PendingCommand>,
    command_history: CommandHistory,
}

pub struct WorkerPool {
    workers: HashMap<WorkerId, WorkerState>,
    load_balancer: LoadBalancer,
}

pub struct WorkerState {
    connection: TcpStream,
    current_load: usize,
    max_capacity: usize,
    last_heartbeat: Instant,
    supported_namespaces: Vec<String>,
    capabilities: WorkerCapabilities,
}
```

**Key Command Handlers:**
- **TaskInitializationHandler**: Handles `initialize_task` commands
- **BatchExecutionHandler**: Manages step batch distribution to workers
- **ResultAggregationHandler**: Collects and processes step results
- **WorkerManagementHandler**: Handles registration, heartbeats, capacity management
- **HealthCheckHandler**: System monitoring and diagnostics

#### Ruby Side: TcpOrchestrator

```ruby
class TcpOrchestrator
  # Maintains persistent TCP connection to Rust server
  # Implements reconnection logic with exponential backoff
  # Handles worker registration and capability advertisement
  # Command-based communication with structured responses
  
  def initialize(capabilities:)
    @worker_id = generate_worker_id
    @capabilities = capabilities
    @max_concurrent_steps = capabilities[:max_concurrent_steps] || 10
    @supported_namespaces = capabilities[:namespaces] || []
  end
  
  def start
    connect_to_server
    register_worker
    start_command_listener
  end
  
  private
  
  def register_worker
    command = {
      command_type: "register_worker",
      command_id: SecureRandom.uuid,
      payload: {
        worker_id: @worker_id,
        max_concurrent_steps: @max_concurrent_steps,
        supported_namespaces: @supported_namespaces,
        capabilities: @capabilities
      }
    }
    send_command(command)
  end
end
```

### Unified Command Protocol

All communication uses a standardized Command structure with rich metadata:

```rust
#[derive(Serialize, Deserialize)]
pub struct Command {
    // Command identification
    pub command_type: CommandType,
    pub command_id: String,
    pub correlation_id: Option<String>,
    
    // Routing and execution context
    pub metadata: CommandMetadata,
    
    // The actual payload
    pub payload: CommandPayload,
}

#[derive(Serialize, Deserialize)]
pub struct CommandMetadata {
    pub timestamp: DateTime<Utc>,
    pub source: CommandSource,
    pub target: Option<CommandTarget>,
    pub timeout_ms: Option<u64>,
    pub retry_policy: Option<RetryPolicy>,
    pub namespace: Option<String>,
    pub priority: Option<CommandPriority>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CommandPayload {
    // Task management operations
    InitializeTask {
        task_context: TaskContext,
        workflow_steps: Vec<WorkflowStep>,
    },
    
    // Step execution operations
    ExecuteBatch {
        batch_id: String,
        steps: Vec<StepExecutionRequest>,
    },
    
    // Result reporting
    ReportPartialResult {
        batch_id: String,
        step_id: i64,
        result: StepResult,
        execution_time_ms: u64,
    },
    
    ReportBatchCompletion {
        batch_id: String,
        step_summaries: Vec<StepSummary>,
        total_execution_time_ms: u64,
    },
    
    // Worker lifecycle management
    RegisterWorker {
        worker_capabilities: WorkerCapabilities,
    },
    
    WorkerHeartbeat {
        current_load: usize,
        system_stats: Option<SystemStats>,
    },
    
    // System operations
    HealthCheck {
        diagnostic_level: HealthCheckLevel,
    },
    
    // Generic FFI operation (future migration path)
    FfiOperation {
        method: String,
        args: serde_json::Value,
    },
}

#[derive(Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub worker_id: String,
    pub max_concurrent_steps: usize,
    pub supported_namespaces: Vec<String>,
    pub step_timeout_ms: u64,
    pub supports_retries: bool,
    pub language_runtime: String, // "ruby", "python", "node"
    pub version: String,
}
```

### Example Command Flows

**Worker Registration Flow:**
```json
// Ruby → Rust
{
  "command_type": "register_worker",
  "command_id": "cmd_12345",
  "metadata": {
    "timestamp": "2025-01-26T10:30:00Z",
    "source": { "type": "ruby_worker", "id": "worker_pid_67890" },
    "timeout_ms": 5000
  },
  "payload": {
    "type": "RegisterWorker",
    "data": {
      "worker_capabilities": {
        "worker_id": "ruby_worker_pid_67890",
        "max_concurrent_steps": 10,
        "supported_namespaces": ["order_fulfillment", "inventory"],
        "step_timeout_ms": 30000,
        "supports_retries": true,
        "language_runtime": "ruby",
        "version": "3.1.0"
      }
    }
  }
}

// Rust → Ruby (Acknowledgment)
{
  "command_type": "worker_registered",
  "command_id": "resp_12345",
  "correlation_id": "cmd_12345",
  "metadata": {
    "timestamp": "2025-01-26T10:30:00.150Z",
    "source": { "type": "rust_server", "id": "server_main" }
  },
  "payload": {
    "type": "Success",
    "data": {
      "worker_id": "ruby_worker_pid_67890",
      "assigned_pool": "default",
      "queue_position": 1
    }
  }
}
```

**Batch Execution Flow:**
```json
// Rust → Ruby
{
  "command_type": "execute_batch",
  "command_id": "batch_cmd_456",
  "metadata": {
    "timestamp": "2025-01-26T10:35:00Z",
    "source": { "type": "rust_orchestrator", "id": "coordinator_1" },
    "target": { "type": "ruby_worker", "id": "worker_pid_67890" },
    "namespace": "order_fulfillment",
    "priority": "normal"
  },
  "payload": {
    "type": "ExecuteBatch",
    "data": {
      "batch_id": "batch_789",
      "steps": [
        {
          "step_id": 42,
          "step_name": "validate_order",
          "step_context": {...},
          "dependencies": [...]
        }
      ]
    }
  }
}
```

## Command Pattern: Future Evolution Path

### Current FFI → Future Command Migration

The Command pattern provides a natural evolution path for migrating FFI operations to network-based commands:

**Current Direct FFI:**
```ruby
# Direct method calls with FFI boundary
result = handler.initialize_task(context)
handle_result = handler.handle(context)
step_results = handler.get_steps_for_task(task_id)
```

**Future Command-Based (Network Transparent):**
```ruby
# Send commands over TCP - same external API
result = handler.initialize_task(context)      # → InitializeTask command
handle_result = handler.handle(context)        # → ExecuteBatch command  
step_results = handler.get_steps_for_task(task_id) # → QuerySteps command
```

### Benefits of Command-Based Architecture

1. **Network Transparency**: FFI vs network becomes implementation detail
2. **Distributed Workers**: Ruby workers can run on different machines
3. **Language Agnostic**: Python, Node.js workers using same command protocol
4. **Observability**: All operations flow through central command router with logging
5. **Replay/Debugging**: Commands can be logged, stored, and replayed for testing
6. **Circuit Breakers**: Network-level failure handling and fallback strategies
7. **Load Balancing**: Intelligent routing based on worker capabilities and load
8. **Horizontal Scaling**: Multiple Rust servers can coordinate through command forwarding

### Worker Registration Specifics

**Ruby Worker Lifecycle:**
```
1. Ruby process starts → connects to Rust TCP server
2. Sends RegisterWorker command with capabilities:
   - max_concurrent_steps: 10
   - supported_namespaces: ["order_fulfillment", "inventory"]
   - step_timeout_ms: 30000
   - language_runtime: "ruby", version: "3.1.0"
3. Rust adds worker to pool with capacity tracking
4. Rust routes batches based on:
   - Worker current load vs max capacity
   - Namespace compatibility
   - Connection health and latency
5. Worker sends periodic heartbeats with current load
6. On disconnect, Rust removes worker and redistributes work
```

**Intelligent Work Distribution:**
```rust
impl WorkerPool {
    fn select_worker_for_batch(&self, batch: &BatchRequest) -> Option<WorkerId> {
        self.workers
            .iter()
            .filter(|(_, worker)| {
                // Namespace compatibility
                worker.supports_namespace(&batch.namespace) &&
                // Capacity available
                worker.current_load < worker.max_capacity &&
                // Connection healthy
                worker.is_healthy()
            })
            .min_by_key(|(_, worker)| worker.current_load)
            .map(|(id, _)| *id)
    }
}
```

This replaces ZeroMQ's blind broadcasting with intelligent, capacity-aware work distribution.

## Benefits of Evolution

### Operational Benefits
1. **Zero External Dependencies**: Eliminates ZeroMQ deployment complexity
2. **Predictable Connection State**: TCP provides clear connection status
3. **Built-in Reliability**: TCP handles message delivery, ordering, and flow control
4. **Simplified Configuration**: Single server port, multiple client connections
5. **Enhanced Debugging**: Connection logs, message tracing, clear error reporting

### Performance Benefits
1. **Native Tokio Integration**: Optimal resource usage within existing async runtime
2. **Reduced Memory Footprint**: No ZeroMQ internal buffering layers
3. **Custom Protocol Optimization**: Message format tuned for specific use case
4. **Connection Pooling**: Multiple Ruby workers per Rust server instance
5. **Load Balancing**: Intelligent batch distribution across workers

### Reliability Benefits
1. **Guaranteed Message Delivery**: TCP reliability vs ZeroMQ fire-and-forget
2. **Automatic Reconnection**: Ruby clients reconnect with exponential backoff
3. **Graceful Degradation**: Circuit breaker patterns for worker failures
4. **Connection Health Monitoring**: Heartbeat mechanism for failure detection
5. **Graceful Shutdown**: Connection draining ensures in-flight batches complete

## Implementation Roadmap

### Phase 1: Command Infrastructure & TCP Foundation (Week 1-2)
**Goal**: Build complete Command pattern infrastructure alongside existing ZeroMQ (for reference)

- [ ] Implement unified `Command` struct with `CommandPayload` enum
- [ ] Create `CommandRouter` with pluggable `CommandHandler` trait
- [ ] Implement `TokioTcpExecutor` as complete replacement for ZmqPubSubExecutor
- [ ] Build `ConnectionManager` actor for client lifecycle management
- [ ] Add command serialization/deserialization with proper error handling
- [ ] Unit tests for command routing and TCP infrastructure

**Success Criteria**:
- TCP server accepts connections and maintains connection state
- Command serialization/deserialization working for all payload types
- CommandRouter successfully routes commands to appropriate handlers
- Basic connection lifecycle management with graceful disconnect handling
- **ZeroMQ remains available for pattern reference only**

### Phase 2: Ruby Command Client & Worker Registration (Week 2-3)
**Goal**: Build complete TCP command client to replace ZeroMQ

- [ ] Implement `TcpOrchestrator` as complete replacement for `ZeromqOrchestrator`
- [ ] Implement worker registration with capability advertisement
- [ ] Add TCP client with reconnection logic and exponential backoff
- [ ] Implement command sending/receiving with correlation ID tracking
- [ ] **Replace all ZeroMQ usage** in BatchStepExecutionOrchestrator
- [ ] Integration tests proving Command system works end-to-end

**Success Criteria**:
- Ruby worker registers successfully with capabilities and namespace support
- Command exchange working bidirectionally with proper correlation
- Reconnection logic handles server restarts and network failures
- Worker registration enables intelligent work distribution
- **BatchStepExecutionOrchestrator uses TCP commands exclusively**

### Phase 3: Complete WorkflowCoordinator Integration (Week 3-4)
**Goal**: Replace ZeroMQ execution path in WorkflowCoordinator with Command system

- [ ] Implement `WorkerManagementHandler` for registration, heartbeats, capacity tracking
- [ ] Implement `BatchExecutionHandler` with intelligent worker selection
- [ ] Implement `ResultAggregationHandler` for partial results and batch completion
- [ ] Add `WorkerPool` with load balancing and namespace-aware routing
- [ ] **Replace ZmqPubSubExecutor with TokioTcpExecutor** in WorkflowCoordinator
- [ ] **Update OrchestrationSystem** to use CommandRouter instead of ZeroMQ
- [ ] Add comprehensive command logging, metrics, and tracing

**Success Criteria**:
- Worker registration and capability-based routing working
- Batch commands distributed intelligently based on worker load and namespace
- Partial results and batch completions properly aggregated via commands
- Worker capacity and namespace constraints respected for load balancing
- **WorkflowCoordinator uses Command system exclusively**
- **All integration tests pass with Command execution**

### Phase 4: Production-Ready Reliability & Performance (Week 4-5)
**Goal**: Production-grade reliability and performance optimization

- [ ] Implement connection pooling for multiple Ruby processes per machine
- [ ] Add health check commands and heartbeat mechanism with failure detection
- [ ] Implement circuit breaker patterns for worker failure tolerance
- [ ] Add command retry policies and timeout handling
- [ ] Implement `FfiOperation` command type for future migration path
- [ ] Performance optimization and benchmarking (target: match/exceed ZeroMQ)
- [ ] Add command history and replay capabilities for debugging
- [ ] **Comprehensive performance testing vs current ZeroMQ baseline**

**Success Criteria**:
- Multiple Ruby processes connect and receive work based on collective capacity
- Failed workers detected within heartbeat interval and removed from pool
- System handles partial worker failures gracefully with automatic redistribution
- Command retry and timeout policies working for resilient operation
- **Performance equals or exceeds ZeroMQ baseline**
- **Complete feature parity with current ZeroMQ functionality**
- FFI operations can be optionally routed through command infrastructure

### Phase 5: Complete ZeroMQ Removal & Documentation (Week 5-6)
**Goal**: **TOTAL ZEROMQ ELIMINATION** - Complete cleanup and documentation

- [ ] **Remove ALL ZeroMQ files completely** (zeromq_pub_sub_executor.rs, zeromq_orchestrator.rb)
- [ ] **Remove ALL ZeroMQ dependencies** from Cargo.toml and Gemfile  
- [ ] **Remove ALL ZeroMQ configuration** sections from YAML files
- [ ] **Remove ALL ZeroMQ imports and references** from codebase
- [ ] Update configuration system to use TCP endpoints exclusively
- [ ] Update deployment guides for command-based architecture only
- [ ] Document command pattern architecture and worker registration
- [ ] Add command debugging and monitoring guides
- [ ] **Verify zero ZeroMQ references remain** with comprehensive grep audit

**Success Criteria**:
- **🔥 ZERO ZeroMQ references in entire codebase** (verified by grep audit)
- **🔥 ZERO ZeroMQ dependencies** in any configuration files
- **🔥 ALL ZeroMQ files deleted** with no remaining artifacts
- All integration tests pass with command-based TCP implementation only
- Performance benchmarks meet or exceed former ZeroMQ baseline
- Documentation complete for command pattern and worker registration
- Deployment guides reference TCP server configuration exclusively
- **Clean, ZeroMQ-free architecture ready for production**

## Risk Mitigation

### Technical Risks

1. **TCP Head-of-Line Blocking**
   - **Risk**: TCP connection blocking affects all messages
   - **Mitigation**: Use separate connections per worker or async message framing

2. **Message Ordering**
   - **Risk**: TCP doesn't guarantee application-level message ordering
   - **Mitigation**: Add sequence numbers and reordering buffer if needed

3. **Backpressure Handling**
   - **Risk**: Slow Ruby workers overwhelm system
   - **Mitigation**: Bounded channels and worker capacity tracking

4. **Network Partitions**
   - **Risk**: Connection failures disrupt batch processing
   - **Mitigation**: Robust reconnection with circuit breaker patterns

5. **Performance Regression**
   - **Risk**: Custom TCP implementation slower than ZeroMQ
   - **Mitigation**: Benchmark early, optimize with MessagePack if needed

### Operational Risks

1. **Migration Complexity**
   - **Risk**: Breaking existing workflows during transition
   - **Mitigation**: Incremental migration with backward compatibility

2. **Configuration Changes**
   - **Risk**: Deployment updates required across environments
   - **Mitigation**: Environment-aware configuration with gradual rollout

## Success Metrics

### Reliability Metrics
- **Zero message delivery failures** (vs current EAGAIN errors)
- **< 100ms connection establishment time**
- **99.9% uptime** for worker connections
- **Graceful handling** of worker failures without batch loss

### Performance Metrics
- **Message throughput** equals or exceeds ZeroMQ baseline
- **< 1ms additional latency** for TCP vs ZeroMQ
- **< 10% CPU overhead** vs current implementation
- **50% reduction** in memory usage (no ZeroMQ buffers)

### Operational Metrics
- **Zero deployment failures** due to port conflicts
- **< 30s recovery time** from worker failures
- **Complete observability** of connection state and message flow
- **Simplified configuration** with single server endpoint

## Conclusion

The evolution from ZeroMQ to a command-based Tokio TCP architecture represents a transformative improvement in reliability, maintainability, and future extensibility. By eliminating the external ZeroMQ dependency and implementing a unified Command pattern, we solve persistent communication issues while creating a foundation for distributed, multi-language operations.

### Key Architectural Innovations

1. **Unified Command Pattern**: All operations (current batch execution, future FFI migrations) flow through standardized Command structure
2. **Intelligent Worker Management**: Explicit registration with capability-based routing replaces blind broadcasting
3. **Network Transparency**: Commands provide seamless migration path from FFI to distributed operations
4. **Enhanced Observability**: Complete command lifecycle tracking, correlation, and replay capabilities

### Strategic Benefits

- **Immediate**: Resolves ZeroMQ EAGAIN errors and connection reliability issues
- **Short-term**: Enables intelligent load balancing and worker pool management
- **Long-term**: Provides foundation for distributed Ruby workers across machines
- **Future**: Clear migration path for all FFI operations to become network-transparent

The incremental migration approach ensures backward compatibility while providing immediate benefits from improved connection reliability and intelligent work distribution. This evolution aligns perfectly with the existing Tokio-based async architecture and positions the system for horizontal scaling, multi-language worker support, and sophisticated orchestration capabilities.

### Worker Registration Innovation

The explicit worker registration system transforms the architecture from:
- **ZeroMQ**: Blind topic broadcasting with unknown worker state
- **TCP Commands**: Intelligent routing based on worker capabilities, load, and namespace compatibility

This enables production-grade features like capacity planning, namespace isolation, and graceful failure handling.

**Recommendation**: Proceed with implementation starting with Phase 1, with the goal of having a production-ready command-based TCP implementation within 6 weeks. The Command pattern foundation will provide immediate reliability improvements while establishing the architecture for future distributed operations.

---

## Explicit Implementation Plan

### File-Level Dependencies and Creation Strategy

Based on analysis of the current ZeroMQ architecture documented in `docs/analysis-after-zeromq.md`, this section provides explicit file-level implementation details with dependency mapping.

### Phase 1: Command Infrastructure & TCP Foundation

#### Files to Create

**Rust Core Command Infrastructure:**
```
src/execution/command.rs                    # ← No dependencies, foundational
├── Command struct with CommandPayload enum
├── CommandMetadata with routing information
├── CommandResult and error handling
└── Serialization/deserialization traits

src/execution/command_router.rs             # ← Depends on: command.rs
├── CommandRouter with handler registry
├── CommandHandler trait definition
├── Command lifecycle management
└── Handler dispatch and error handling

src/execution/command_handlers/             # ← Depends on: command_router.rs
├── mod.rs                                  # Handler module exports
├── worker_management_handler.rs            # Worker registration/heartbeat
├── batch_execution_handler.rs              # Step batch distribution
├── result_aggregation_handler.rs           # Partial result collection
├── task_initialization_handler.rs          # Task creation commands
└── health_check_handler.rs                 # System diagnostics

src/execution/tokio_tcp_executor.rs         # ← Depends on: command_router.rs, worker_pool.rs
├── TokioTcpExecutor replacing ZmqPubSubExecutor
├── TCP server lifecycle management
├── Connection handling with async/await
└── Integration with CommandRouter

src/execution/worker_pool.rs                # ← Depends on: command.rs
├── WorkerPool with capability tracking
├── WorkerState and WorkerCapabilities
├── Load balancing algorithms
├── Connection health monitoring
└── Namespace-aware work distribution
```

**Database Models for Command Architecture:**
```
src/models/core/worker_registration.rs      # ← No dependencies
├── WorkerRegistration database model
├── Capability tracking and validation
├── Connection state management
└── Namespace support tracking

src/models/core/command_execution.rs        # ← No dependencies
├── CommandExecution audit trail
├── Command lifecycle tracking
├── Performance metrics collection
└── Error/retry tracking

migrations/20250726000001_create_command_tables.sql  # ← No dependencies
├── worker_registrations table
├── command_executions table
├── worker_capabilities join table
└── Indexes for performance
```

#### Files to Update

**Integration Points:**
```
src/ffi/shared/orchestration_system.rs     # ← Replace ZeroMQ with CommandRouter
├── Replace zeromq_executor with command_router: Arc<CommandRouter>
├── Remove all ZeroMQ initialization code
├── Use TokioTcpExecutor exclusively
└── Simplify to single execution path

src/orchestration/workflow_coordinator.rs  # ← Replace ZeroMQ execution path
├── Replace execute_via_zeromq() with execute_via_commands()
├── Remove ZmqPubSubExecutor usage entirely
├── Use TokioTcpExecutor exclusively
└── Simplify to single execution strategy

config/tasker-config.yaml                  # ← Replace ZeroMQ with TCP config
├── Replace zeromq section with tcp_server section
├── Environment-specific TCP endpoints (not separate file)
├── Worker registration settings in main config
└── Remove all ZeroMQ configuration
```

#### Dependency Map - Phase 1
```
command.rs
  └── command_router.rs
      ├── command_handlers/*.rs
      └── tokio_tcp_executor.rs
          └── worker_pool.rs

orchestration_system.rs ← command_router.rs (replaces zeromq)
workflow_coordinator.rs ← orchestration_system.rs
config files ← independent (unified tasker-config.yaml only)
database models ← independent
migrations ← independent
```

### Phase 2: Ruby Command Client & Worker Registration

#### Files to Create

**Ruby Command Infrastructure:**
```
bindings/ruby/lib/tasker_core/orchestration/tcp_orchestrator.rb
├── Replace ZeromqOrchestrator functionality
├── TCP connection management
├── Command sending/receiving
├── Automatic reconnection logic
└── Worker capability advertisement

bindings/ruby/lib/tasker_core/orchestration/command_client.rb
├── TCP connection abstraction
├── Command correlation tracking
├── Connection pooling support
├── Error handling and circuit breaker
└── Async command processing

bindings/ruby/lib/tasker_core/orchestration/worker_capabilities.rb
├── Capability definition and validation
├── Namespace support declaration
├── Performance characteristics
├── Ruby runtime information
└── Registration payload building

bindings/ruby/lib/tasker_core/types/command_types.rb
├── Ruby Command structure matching Rust
├── CommandPayload variants as Ruby classes
├── JSON serialization compatibility
├── Type validation and conversion
└── Error handling structures
```

#### Files to Update

**Ruby Integration Points:**
```
bindings/ruby/lib/tasker_core/orchestration/batch_step_execution_orchestrator.rb
├── Replace zeromq_orchestrator with tcp_orchestrator
├── Remove all ZeroMQ dependencies and imports
├── Use TcpOrchestrator exclusively
├── Preserve external API (initialize_task, handle methods)
└── Remove ZeroMQ configuration options

bindings/ruby/lib/tasker_core/config.rb
├── Replace ZeroMQConfig with TcpConfig class
├── TCP server endpoint configuration from main tasker-config.yaml
├── Worker capability configuration
├── Remove ZeroMQ configuration support entirely
└── Environment-aware TCP settings from unified config

bindings/ruby/lib/tasker_core/internal/orchestration_manager.rb
├── Replace ZeroMQ handle with TCP command handle
├── Remove ZeroMQ orchestrator lifecycle management
├── Use CommandRouter handle exclusively
├── Simplify to single orchestrator type
└── Remove dual orchestrator complexity
```

#### Dependency Map - Phase 2
```
command_types.rb
  └── tcp_orchestrator.rb
      ├── command_client.rb
      └── worker_capabilities.rb

batch_step_execution_orchestrator.rb ← tcp_orchestrator.rb (replaces zeromq)
config.rb ← command_types.rb (removes ZeroMQConfig)
orchestration_manager.rb ← config.rb (simplified single orchestrator)
```

### Phase 3: Command Handlers & Worker Pool Management

#### Files to Update

**Handler Implementation:**
```
src/execution/command_handlers/worker_management_handler.rs
├── Process RegisterWorker commands
├── Maintain worker pool state
├── Handle heartbeat commands
├── Worker capability validation
└── Connection lifecycle management

src/execution/command_handlers/batch_execution_handler.rs
├── Process ExecuteBatch commands
├── Intelligent worker selection
├── Namespace compatibility checking
├── Load balancing implementation
└── Batch distribution coordination

src/execution/command_handlers/result_aggregation_handler.rs
├── Process ReportPartialResult commands
├── Process ReportBatchCompletion commands
├── Dual result pattern implementation
├── Result reconciliation logic
└── State machine integration

src/execution/worker_pool.rs
├── Worker capability matching
├── Load balancing algorithms
├── Connection health monitoring
├── Namespace-aware routing
└── Graceful worker removal
```

#### Integration Updates
```
src/execution/tokio_tcp_executor.rs
├── Full TCP server implementation
├── Connection acceptance and management
├── Command routing to handlers
├── Worker lifecycle coordination
└── Performance monitoring

src/orchestration/workflow_coordinator.rs
├── Complete command execution path
├── Feature flag implementation
├── Performance comparison logging
├── Backward compatibility preservation
└── Error handling unification
```

### Phase 4: Advanced Reliability & FFI Migration Path

#### Files to Create

**FFI Evolution Support:**
```
src/execution/command_handlers/ffi_operation_handler.rs
├── Process FfiOperation commands
├── Dynamic method dispatch
├── Argument serialization/deserialization
├── Result marshaling
└── Error handling and timeout

bindings/ruby/lib/tasker_core/internal/command_proxy.rb
├── Proxy FFI calls through command system
├── Transparent migration support
├── Performance comparison capabilities
├── Backward compatibility bridge
└── Feature flag support for migration
```

#### Files to Update

**Reliability Enhancements:**
```
src/execution/command_router.rs
├── Command retry policies
├── Timeout handling
├── Circuit breaker integration
├── Command history and replay
└── Performance metrics collection

src/execution/worker_pool.rs
├── Connection pooling for multiple processes
├── Health check command processing
├── Automatic worker removal on failure
├── Load balancing optimization
└── Graceful degradation handling

bindings/ruby/lib/tasker_core/orchestration/tcp_orchestrator.rb
├── Exponential backoff reconnection
├── Connection pooling support
├── Health check implementation
├── Performance monitoring
└── Graceful shutdown handling
```

### Phase 5: Migration, Cleanup & Documentation

#### Files to Remove (After Migration Complete)

**ZeroMQ-Specific Components:**
```
src/execution/zeromq_pub_sub_executor.rs              # ← Replaced by TokioTcpExecutor
bindings/ruby/lib/tasker_core/orchestration/zeromq_orchestrator.rb  # ← Replaced by TcpOrchestrator

# ZeroMQ Dependencies
Cargo.toml                                            # ← Remove zmq dependency
bindings/ruby/Gemfile                                 # ← Remove ffi-rzmq dependency

# Configuration Cleanup
config/zeromq.yaml sections                          # ← Remove ZeroMQ config
bindings/ruby/config/zeromq.yaml                     # ← Replace with TCP config
```

#### Files to Update for Cleanup

**Configuration Simplification:**
```
config/tasker-config.yaml                  # ← Unified config file only
├── Remove zeromq sections completely
├── Consolidate TCP configuration in main file
├── Environment-specific TCP settings (test/development/production)
├── Worker registration settings
└── Remove separate ZeroMQ config files

bindings/ruby/lib/tasker_core/config.rb
├── Remove ZeroMQConfig class entirely
├── Consolidate into single TcpConfig class
├── Load TCP config from main tasker-config.yaml only
├── Remove dual configuration support complexity
└── Simplify environment detection logic
```

### Critical File Dependencies

#### Preservation Requirements (Never Remove)

**FFI Boundary Architecture:**
- `src/ffi/shared/orchestration_system.rs` - Core orchestration system (simplified to Command-only)
- `src/ffi/shared/handles.rs` - Handle-based architecture foundation (unchanged)
- `bindings/ruby/lib/tasker_core/internal/orchestration_manager.rb` - Ruby handle lifecycle (simplified)

**Core Orchestration (Communication-Independent):**
- `src/orchestration/workflow_coordinator.rs` - Core workflow logic (communication layer abstracted)
- `src/orchestration/state_manager.rs` - State management (communication layer independent)
- `src/orchestration/task_initializer.rs` - Task creation (communication layer independent)

**Database Models (Fully Reusable):**
- `src/models/core/step_execution_batch*.rs` - Batch concept identical for Command system
- All migration files - Database schema communication-layer independent
- Worker registration and command execution models - new additions

### Migration Strategy - Complete Replacement Approach

#### No Backward Compatibility - Clean Implementation Strategy

Since this is a new branch where ZeroMQ was recently added, we take a **complete replacement** approach:

1. **Phase 1**: Build Command infrastructure (ZeroMQ remains for pattern reference)
2. **Phase 2**: Replace ZeroMQ usage in Ruby layer completely  
3. **Phase 3**: Replace ZeroMQ usage in Rust orchestration completely
4. **Phase 4**: Production testing and performance validation
5. **Phase 5**: **TOTAL ZEROMQ ELIMINATION** - delete all files, dependencies, config

#### Rollback Strategy
- **Git-based rollback**: Each phase is a separate commit/branch
- **Feature completeness**: Each phase results in fully working system
- **Performance validation**: Each phase includes benchmarking
- **Integration testing**: Each phase must pass full test suite

#### Implementation Pattern
```rust
// Phase 1-2: Build new system
impl OrchestrationSystem {
    fn new() -> Self {
        // Build Command infrastructure
        let command_router = CommandRouter::new();
        // ZeroMQ still exists for reference
    }
}

// Phase 3: Replace completely  
impl OrchestrationSystem {
    fn execute_workflow(&self) {
        // Only Command system - ZeroMQ usage removed
        self.command_router.execute()
    }
}

// Phase 5: Total cleanup
// All ZeroMQ files deleted, no references remain
```

This approach results in **cleaner, simpler architecture** without dual-system complexity.

### Success Metrics and Validation

#### Functional Validation
- All integration tests pass with Command execution
- Performance equals or exceeds ZeroMQ baseline
- Worker registration and capability routing working correctly
- Dual result pattern maintaining compatibility

#### Performance Validation
- Command overhead < 1ms vs ZeroMQ
- Worker pool efficiency >= ZeroMQ worker utilization
- Connection establishment < 100ms
- Memory usage reduction from eliminated ZeroMQ buffers

#### Operational Validation
- Zero deployment failures due to port conflicts
- Graceful handling of worker failures
- Complete observability of command flow and worker state
- Simplified configuration and deployment
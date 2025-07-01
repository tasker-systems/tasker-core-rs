# Tasker Core Rust - Development Plan

## Vision & Goals

Transform the Rails Tasker engine's performance-critical components into a high-performance Rust core while maintaining compatibility and extending capabilities.

**Performance Targets:**
- 10-100x faster dependency resolution vs PostgreSQL functions
- Memory-safe concurrent step execution
- Sub-millisecond state transitions
- High-throughput event processing

## Development Strategy

### Phase 1: Foundation Layer 🏗️
**Duration**: ~1-2 weeks  
**Goal**: Type-safe database interactions with clean abstractions

#### Components to Build:
```rust
// Core database models
src/models/
├── task_namespace.rs      // TaskNamespace struct + queries
├── named_task.rs          // NamedTask struct + queries  
├── named_step.rs          // NamedStep struct + queries
├── task.rs                // Task struct + lifecycle queries
├── workflow_step.rs       // WorkflowStep struct + state queries
├── workflow_step_edge.rs  // Dependency relationships
└── transitions.rs         // Audit trail models

// Repository pattern for complex queries
src/database/
├── repositories/
│   ├── task_repository.rs      // Task CRUD + complex queries
│   ├── step_repository.rs      // Step CRUD + dependency queries
│   └── analytics_repository.rs // Performance metrics
└── query_builders/
    ├── dependency_resolver.rs  // DAG traversal queries
    └── viable_steps_query.rs   // Step readiness calculation
```

#### Success Criteria:
- [ ] All database tables have corresponding Rust structs
- [ ] CRUD operations for all entities with SQLx compile-time verification
- [ ] Complex dependency queries ported from PostgreSQL functions
- [ ] Comprehensive test coverage for all database operations
- [ ] Performance benchmarks showing query speed improvements

#### Key Decisions:
- **Repository Pattern**: Clean separation between models and complex queries
- **SQLx Queries**: Compile-time verified SQL for performance and safety
- **Error Handling**: Comprehensive error types for database operations

---

### Phase 2: State Management 🔄
**Duration**: ~1-2 weeks  
**Goal**: Port proven state machine logic with Rust memory safety

#### Components to Build:
```rust
src/state_machine/
├── task_state_machine.rs
│   ├── TaskState enum (created, running, complete, error, etc.)
│   ├── TaskStateMachine with atomic transitions
│   ├── transition validation and business rules
│   └── state persistence with audit trails
├── step_state_machine.rs  
│   ├── StepState enum (created, ready, running, complete, etc.)
│   ├── StepStateMachine with retry logic
│   ├── backoff calculations for failed steps
│   └── concurrent-safe state updates
└── state_transitions.rs
    ├── TransitionEvent structs
    ├── transition logging and metrics
    └── state machine event publishing
```

#### Success Criteria:
- [ ] Complete task and step state machines ported from Rails
- [ ] Thread-safe state transitions with atomic operations
- [ ] Comprehensive state validation and business rule enforcement
- [ ] State transition audit trails with full history
- [ ] Performance tests showing sub-millisecond transitions

#### Key Decisions:
- **Atomic Operations**: Use parking_lot for high-performance locking
- **State Validation**: Compile-time state guarantees where possible
- **Audit Trails**: Complete transition history for debugging and compliance

---

### Phase 3: Orchestration Engine ⚡
**Duration**: ~2-3 weeks  
**Goal**: 10-100x performance improvement in dependency resolution

#### Components to Build:
```rust
src/orchestration/
├── coordinator.rs
│   ├── TaskCoordinator for system initialization
│   ├── health monitoring and metrics collection
│   └── graceful shutdown and resource cleanup
├── viable_step_discovery.rs
│   ├── high-performance DAG traversal algorithms
│   ├── parallel dependency resolution
│   ├── step readiness calculation (replace PostgreSQL functions)
│   └── incremental updates for efficiency
├── step_executor.rs
│   ├── concurrent step execution with work-stealing
│   ├── resource management and backpressure
│   ├── error isolation and recovery
│   └── performance monitoring and telemetry
├── task_finalizer.rs
│   ├── task completion logic and cleanup
│   ├── final state validation
│   └── completion event publishing
└── backoff_calculator.rs
    ├── exponential backoff with jitter
    ├── circuit breaker patterns
    └── retry policy configuration
```

#### Success Criteria:
- [ ] Dependency resolution 10-100x faster than PostgreSQL functions
- [ ] Memory-safe concurrent step execution without data races
- [ ] Comprehensive error handling and recovery mechanisms
- [ ] Full observability with metrics and tracing
- [ ] Performance benchmarks demonstrating improvements

#### Key Decisions:
- **Concurrency Model**: Tokio async runtime with work-stealing scheduler
- **Memory Safety**: Rust ownership system eliminates race conditions
- **Performance**: Custom algorithms optimized for workflow orchestration

---

### Phase 4: Event System 📡
**Duration**: ~1-2 weeks  
**Goal**: High-throughput event processing for workflow lifecycle

#### Components to Build:
```rust
src/events/
├── lifecycle_events.rs
│   ├── 56+ event type definitions from Rails engine
│   ├── event serialization and deserialization
│   └── event validation and schema enforcement
├── publisher.rs
│   ├── high-throughput async event publisher
│   ├── batch publishing for efficiency
│   ├── back-pressure handling and flow control
│   └── event persistence and durability guarantees
└── subscriber.rs
    ├── subscriber registry with type-safe routing
    ├── async event handling with error isolation
    ├── subscriber health monitoring
    └── dead letter queue for failed events
```

#### Success Criteria:
- [ ] All 56+ Rails lifecycle events defined and working
- [ ] High-throughput event publishing (>10k events/sec)
- [ ] Type-safe event routing and handling
- [ ] Comprehensive error handling and dead letter queues
- [ ] Event persistence and replay capabilities

#### Key Decisions:
- **Event Schema**: Strong typing with serde for serialization
- **Publisher Model**: Async-first with batching for performance
- **Reliability**: At-least-once delivery with idempotency

---

### Phase 5: Integration Layer 🔌
**Duration**: ~1-2 weeks  
**Goal**: Seamless integration with Rails engine and other languages

#### Components to Build:
```rust
src/ffi/
├── ruby.rs (magnus bindings)
│   ├── Ruby class wrappers for core functionality
│   ├── Rails integration helpers
│   ├── callback mechanisms for Ruby handlers
│   └── error translation between Rust and Ruby
├── python.rs (PyO3 bindings)  
│   ├── Python class exports for data science workflows
│   ├── async integration with Python asyncio
│   ├── numpy/pandas compatibility for analytics
│   └── Python exception handling
└── c_api.rs
    ├── C-compatible API for maximum interoperability
    ├── opaque handle management
    ├── memory management and cleanup
    └── error code definitions
```

#### Success Criteria:
- [ ] Ruby FFI enabling Rails engine to use Rust core
- [ ] Python bindings for data science and analytics workflows
- [ ] C API for integration with other languages
- [ ] Performance benchmarks showing FFI overhead is minimal
- [ ] Comprehensive integration tests with real workflows

#### Key Decisions:
- **FFI Safety**: Careful memory management across language boundaries
- **Performance**: Minimize FFI overhead with efficient data marshaling
- **Compatibility**: Maintain API compatibility with Rails engine

---

## Cross-Cutting Concerns

### Observability & Monitoring
- **Structured Logging**: tracing crate with JSON output
- **Metrics**: Custom metrics for workflow performance
- **Health Checks**: Comprehensive system health monitoring
- **Distributed Tracing**: OpenTelemetry integration

### Testing Strategy
- **Unit Tests**: Comprehensive coverage for all components
- **Integration Tests**: End-to-end workflow testing
- **Performance Tests**: Benchmarks for all critical paths
- **Property Tests**: Fuzz testing for edge cases

### Documentation & Memory
- **API Documentation**: Comprehensive rustdoc for all public APIs
- **Architecture Decision Records**: Document key design choices
- **Performance Analysis**: Regular benchmarking and optimization
- **Knowledge Transfer**: Update CLAUDE.md with learnings

## Success Metrics

### Performance Targets:
- Dependency resolution: 10-100x faster than PostgreSQL
- Step execution throughput: >1000 steps/second
- Memory usage: <50MB for typical workloads
- Event processing: >10k events/second

### Quality Targets:
- Test coverage: >90% for all components
- Zero memory leaks in long-running processes
- Sub-second startup time for orchestration engine
- Complete compatibility with Rails Tasker engine

## Risk Mitigation

### Technical Risks:
- **FFI Complexity**: Start with simple bindings, iterate
- **Performance Goals**: Continuous benchmarking and optimization
- **Concurrency Issues**: Extensive testing with stress scenarios

### Project Risks:
- **Scope Creep**: Focus on core performance bottlenecks first
- **Integration Challenges**: Early and frequent testing with Rails
- **Knowledge Transfer**: Document decisions and rationale thoroughly

## Next Steps

1. **Immediate**: Begin Phase 1 with database models implementation
2. **Week 1-2**: Complete foundation layer and begin state machines  
3. **Week 3-4**: Implement orchestration engine core components
4. **Week 5-6**: Build event system and begin FFI work
5. **Week 7+**: Integration testing, optimization, and documentation

This plan balances ambition with pragmatism, focusing on the core performance improvements while building a solid foundation for future enhancements.